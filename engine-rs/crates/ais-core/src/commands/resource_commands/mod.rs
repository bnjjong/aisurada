//! 스킬과 에이전트 정의(자원)를 다루는 명령 15개: `list_resources`, `get_resource`, `save_resource`, `delete_resource`, `restore_resource`,
//! `set_resource_ref`, `usable_resources`, `list_resource_versions`, `list_resource_changes`, `get_resource_change`,
//! `approve_resource_change`, `reject_resource_change`, `resolve_resource_edit`, `get_resource_policy`, `set_resource_policy`.
//! 외부 설명서에 없는 확장이다. 인자 이름, 검사 순서, 오류 문구와 응답 필드는 Java 엔진의 `ResourceCommands`와 같고, 파일 글은 바이트까지 같다.
//!
//! 자원은 글로벌과 프로젝트 두 범위이고, 프로젝트는 글로벌 자원을 ID로 참조해야 쓸 수 있다. 범위는 `scope=global`이나
//! `project=<프로젝트 slug>`로 준다. 참조는 이름이 아니라 `sk…`/`ap…` ID로 건다.
//!
//! 승인 워크플로(#53): AI가 보낸 변경(`caller: "ai"`이거나 AI 세션 표식이 달린 요청)은 정책에 따라 바로 적용하거나 「대기 변경」으로 올린다.
//! 대기 변경은 사람이 승인해야 적용되고, 그 전에는 목록·조회·`usable_resources`에 이전 판이 그대로 보인다. 권한 올림, 참조 켜기,
//! 도구 칸 변경은 정책과 상관없이 늘 승인을 거친다. 사람이 앱이나 명령줄에서 보낸 변경은 바로 적용한다.
//! 승인, 거절, 정책 변경은 사람만 한다(AI가 보내면 `AIS-403`).
//!
//! 옵션은 명령줄처럼 글로 다룬다(`skills`와 `tools`의 JSON도 글로 받아 읽는다). 대기 변경에 적는 `args`는 옵션 이름 순서로 적힌다.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::Deserialize;
use serde_json::{Map, Value as Json, json};

use super::option_text::text_of;
use crate::commands::Caller;
use crate::error::EngineError;
use crate::json::{self, JsonValue};
use crate::kernel::actor::CallContext;
use crate::resource::aggregate::{self, Category, POLICY_KEYS, Policy, TrustState};
use crate::resource::approvals;
use crate::resource::format::{json_to_serde, value_to_serde};
use crate::resource::refs;
use crate::resource::rules::{
    self, RefInfo, RefsChange, Scope, change_refs, check_same_kind, check_same_scope, fields_of, kind_of,
    reject_agent_only_for_skill, summarize_change,
};
use crate::resource::store::{self, Stamp};
use crate::resource::{Resource, ResourceKind, changes, resolver};
use crate::task::format::java_trim;
use crate::text::{compare_utf16, is_blank, trim};
use crate::write::apparatus::now_millis;
use crate::write::json_out::{literal, string};
use crate::write::local_time::iso_offset_date_time;
use crate::write::marker::check_workspace_marker;
use crate::write::stamp::stamp_of;

mod change;
mod crud;
mod policy;
mod ref_cmd;

pub use change::{
    ChangeDecisionRequest, ListResourceChangesRequest, ResourceChangeRequest,
    approve_resource_change, get_resource_change, list_resource_changes, reject_resource_change,
};
pub use crud::{
    GetResourceRequest, ListResourcesRequest, ResourceIdRequest, SaveResourceRequest,
    delete_resource, get_resource, list_resources, restore_resource, save_resource,
};
pub use policy::{
    ResolveResourceEditRequest, SetResourcePolicyRequest, get_resource_policy,
    resolve_resource_edit, set_resource_policy,
};
pub use ref_cmd::{
    ResourceVersionsRequest, SetResourceRefRequest, UsableResourcesRequest,
    list_resource_versions, set_resource_ref, usable_resources,
};

use change::{approval_counts, queue_if_needed};
use crud::{delete, restore, save};
use ref_cmd::ref_impl;

// ---------------------------------------------------------------- 실행 맥락

/// 옵션 이름과 글. 이름 차례로 쥔다.
type Opts = BTreeMap<String, String>;

fn collect(pairs: &[(&str, &Option<Json>)]) -> Opts {
    pairs
        .iter()
        .filter_map(|(key, value)| text_of(value).map(|v| ((*key).to_string(), v)))
        .collect()
}

/// 한 번의 실행. `ai`는 AI의 통로로 온 요청인지, `apply`는 승인된 대기 변경을 적용하는 중인지다.
/// 적용 중에는 대기에 다시 올리지 않고 올린 사람의 기록(`stamp`)으로 쓰며, 승인 기록에는 `by`가 남는다.
struct Run<'a> {
    vault: &'a Path,
    ai: bool,
    stamp: Stamp,
    apply: bool,
    by: &'static str,
    forced_id: Option<String>,
    channel: String,
}

/// AI의 통로 판별: AI 세션 표식이 달렸거나 AI 도구로 불렀으면 AI다. 앱 화면은 세션 표식 없이 부른다.
fn is_ai(context: &CallContext) -> bool {
    context.caller == Caller::Ai
        || context
            .session
            .as_deref()
            .is_some_and(|s| !crate::task::format::java_blank(s))
}

fn run<'a>(vault: &'a Path, context: &CallContext) -> Run<'a> {
    let ai = is_ai(context);
    Run {
        vault,
        ai,
        stamp: stamp_of(context),
        apply: false,
        by: if ai { "policy" } else { "human" },
        forced_id: None,
        channel: if context.caller == Caller::Ai {
            "mcp"
        } else {
            "cli"
        }
        .to_string(),
    }
}

/// 읽기 명령의 앞길: 워크스페이스 표식을 보고, 기록장이 없으면 기준선을 만든다(쓰기보다 먼저여야 방금 쓴 파일이 기준선에 섞이지 않는다).
fn begin_read(vault: &Path) -> Result<(), EngineError> {
    check_workspace_marker(vault)?;
    approvals::ledger(vault)?;
    Ok(())
}

fn scope_of(opts: &Opts, required: bool) -> Result<Scope, EngineError> {
    rules::scope_of(
        opts.get("scope").map(String::as_str),
        opts.get("project").map(String::as_str),
        required,
    )
}

fn is_true(value: Option<&String>) -> bool {
    value.is_some_and(|v| trim(v).to_lowercase() == "true")
}

fn only_id<'a>(id: &'a str, usage: &str) -> Result<&'a str, EngineError> {
    if is_blank(id) {
        return Err(EngineError::invalid_argument(format!(
            "인자가 하나 필요하다: {usage}"
        )));
    }
    Ok(java_trim(id))
}

/// 응답 행. 글 칸은 있을 때만 싣는다.
fn row(r: &Resource, with_body: bool, with_token: bool) -> Map<String, Json> {
    let mut m = Map::new();
    m.insert("id".into(), json!(r.id));
    m.insert("kind".into(), json!(r.kind.label()));
    m.insert("scope".into(), json!(r.scope()));
    if let Some(project) = &r.project {
        m.insert("project".into(), json!(project));
    }
    m.insert("name".into(), json!(r.name));
    m.insert("description".into(), json!(r.description));
    if r.kind == ResourceKind::Agent {
        m.insert(
            "permission".into(),
            json!(r.permission.map(|p| p.as_str())),
        );
        m.insert("skills".into(), json!(r.skills));
        if let Some(repo) = &r.repo {
            m.insert("repo".into(), json!(repo));
        }
    }
    if !r.tools.is_empty() {
        m.insert("tools".into(), tools_json(r));
    }
    m.insert("createdAt".into(), json!(r.created_at));
    m.insert("editedAt".into(), json!(r.edited_at));
    for (key, value) in [
        ("editedSession", &r.edited_session),
        ("editedAgent", &r.edited_agent),
    ] {
        if let Some(value) = value {
            m.insert(key.into(), json!(value));
        }
    }
    m.insert("path".into(), json!(r.rel_path));
    if with_body {
        m.insert("body".into(), json!(r.body));
    }
    if with_token && let Some(token) = &r.token {
        m.insert("precondition".into(), json!(token));
    }
    m
}

fn tools_json(r: &Resource) -> Json {
    Json::Object(
        r.tools
            .iter()
            .map(|(k, v)| (k.clone(), value_to_serde(v)))
            .collect(),
    )
}

fn view_json(r: &Resource) -> Json {
    json_to_serde(&changes::view(r))
}

fn kind_ordinal(kind: ResourceKind) -> usize {
    match kind {
        ResourceKind::Skill => 0,
        ResourceKind::Agent => 1,
    }
}

fn require_human(ai: bool, what: &str) -> Result<(), EngineError> {
    if ai {
        return Err(EngineError::forbidden(format!(
            "{what}은(는) 사람만 할 수 있다. 앱의 스킬·에이전트 화면에서 한다"
        )));
    }
    Ok(())
}

/// 글자 차례를 UTF-16로 가르는 열쇠(Java `TreeSet<String>`의 차례)
#[derive(PartialEq, Eq)]
struct Utf16Key(String);

impl Ord for Utf16Key {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_utf16(&self.0, &other.0)
    }
}

impl PartialOrd for Utf16Key {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
