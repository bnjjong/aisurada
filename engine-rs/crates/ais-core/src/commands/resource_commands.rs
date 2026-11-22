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

const GET_USAGE: &str = "get-resource <자원 id> [version=<list-resource-versions 의 판>]";
#[allow(dead_code)]
const SAVE_USAGE: &str = "save-resource kind=skill|agent scope=global|project=<프로젝트 slug> name=<이름> description=<설명> [body=<지침>] [permission=read|edit|publish. 에이전트] [skills=<스킬 id JSON 배열. 에이전트> ] [repo=<프로젝트 저장소 이름. 프로젝트 에이전트. 빈 값이면 지운다>] [tools=<도구별 칸 JSON 객체. 예: {\"claude\":{...}}. 빈 값이면 비운다>]  |  고칠 때: save-resource id=<자원 id> f1=<직전에 읽은 토큰> [고칠 칸...]. AI가 보낸 변경은 정책에 따라 승인 대기에 오를 수 있다(응답의 queued)";
const DELETE_USAGE: &str = "delete-resource <자원 id>";
const RESTORE_USAGE: &str =
    "restore-resource <휴지통의 자원 id. list-resources trash=true 로 조회한다>";
const REF_USAGE: &str = "set-resource-ref <프로젝트 slug> <글로벌 자원 id> [remove=true] [with-skills=true. 에이전트를 참조할 때 그 스킬도 함께]";
const USABLE_USAGE: &str = "usable-resources <프로젝트 slug> [bodies=true]";
const VERSIONS_USAGE: &str = "list-resource-versions <자원 id>";
const CHANGE_GET_USAGE: &str = "get-resource-change <변경 id. list-resource-changes 로 조회한다>";
const APPROVE_USAGE: &str = "approve-resource-change <변경 id>. 사람만 한다";
const REJECT_USAGE: &str = "reject-resource-change <변경 id> [reason=<거절 이유>]. 사람만 한다";
const RESOLVE_USAGE: &str = "resolve-resource-edit <자원 id 또는 refs:<프로젝트 slug>> action=approve|revert. 승인 안 된 직접 수정을 받아들이거나(approve) 승인한 판으로 되돌린다(revert. 새 파일이면 휴지통으로). 사람만 한다";
const POLICY_SET_USAGE: &str = "set-resource-policy [project=auto|approve] [global=auto|approve] [refs=auto|approve] [permission=auto|approve]. 사람만 한다";
const DEFAULT_DECIDED: i32 = 20;

/// 옵션 이름과 글. 이름 차례로 쥔다.
type Opts = BTreeMap<String, String>;

fn collect(pairs: &[(&str, &Option<Json>)]) -> Opts {
    pairs
        .iter()
        .filter_map(|(key, value)| text_of(value).map(|v| ((*key).to_string(), v)))
        .collect()
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ListResourcesRequest {
    pub scope: Option<Json>,
    pub project: Option<Json>,
    pub kind: Option<Json>,
    pub bodies: Option<Json>,
    pub trash: Option<Json>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GetResourceRequest {
    pub id: String,
    pub version: Option<Json>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SaveResourceRequest {
    pub id: Option<Json>,
    pub kind: Option<Json>,
    pub scope: Option<Json>,
    pub project: Option<Json>,
    pub name: Option<Json>,
    pub description: Option<Json>,
    pub body: Option<Json>,
    pub permission: Option<Json>,
    pub skills: Option<Json>,
    pub repo: Option<Json>,
    pub tools: Option<Json>,
    pub f1: Option<Json>,
}

/// 자원 id 하나만 받는 쓰기 명령(`delete_resource`, `restore_resource`)
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ResourceIdRequest {
    pub id: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SetResourceRefRequest {
    pub project: Option<Json>,
    pub id: Option<Json>,
    pub remove: Option<Json>,
    #[serde(rename = "with-skills", alias = "withSkills")]
    pub with_skills: Option<Json>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UsableResourcesRequest {
    pub project: String,
    pub bodies: Option<Json>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ResourceVersionsRequest {
    pub id: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ListResourceChangesRequest {
    pub decided: Option<Json>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ResourceChangeRequest {
    pub id: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ChangeDecisionRequest {
    pub id: String,
    pub reason: Option<Json>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ResolveResourceEditRequest {
    pub id: String,
    pub action: Option<Json>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SetResourcePolicyRequest {
    pub project: Option<Json>,
    pub global: Option<Json>,
    pub refs: Option<Json>,
    pub permission: Option<Json>,
}

// ---------------------------------------------------------------- 실행 맥락

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

// ---------------------------------------------------------------- 목록·보기

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
        m.insert("permission".into(), json!(r.permission));
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

pub fn list_resources(vault: &Path, request: &ListResourcesRequest) -> Result<Json, EngineError> {
    begin_read(vault)?;
    let opts = collect(&[
        ("scope", &request.scope),
        ("project", &request.project),
        ("kind", &request.kind),
        ("bodies", &request.bodies),
        ("trash", &request.trash),
    ]);
    let scope = scope_of(&opts, false)?;
    let kind = match opts.get("kind") {
        Some(raw) => Some(kind_of(Some(raw))?),
        None => None,
    };
    if let Some(project) = &scope.project {
        store::require_project(vault, project)?;
    }

    if is_true(opts.get("trash")) {
        let rows: Vec<Json> = store::trash(vault)
            .into_iter()
            .filter(|t| kind.is_none_or(|k| k == t.kind))
            .filter(|t| !scope.given || scope.project == t.project)
            .map(|t| {
                let mut m = Map::new();
                m.insert("id".into(), json!(t.id));
                m.insert("kind".into(), json!(t.kind.label()));
                m.insert(
                    "scope".into(),
                    json!(if t.project.is_none() {
                        "global"
                    } else {
                        "project"
                    }),
                );
                if let Some(project) = &t.project {
                    m.insert("project".into(), json!(project));
                }
                m.insert("name".into(), json!(t.name));
                m.insert("path".into(), json!(t.rel_path));
                m.insert("deletedAt".into(), json!(t.deleted_at));
                Json::Object(m)
            })
            .collect();
        return Ok(json!({ "trash": rows }));
    }

    let bodies = is_true(opts.get("bodies"));
    let served = store::scan_served(vault)?;
    let mut items: Vec<&Resource> = served
        .items
        .iter()
        .filter(|r| kind.is_none_or(|k| k == r.kind))
        .filter(|r| !scope.given || scope.project == r.project)
        .collect();
    items.sort_by(|a, b| {
        compare_utf16(
            a.project.as_deref().unwrap_or(""),
            b.project.as_deref().unwrap_or(""),
        )
        .then_with(|| kind_ordinal(a.kind).cmp(&kind_ordinal(b.kind)))
        .then_with(|| compare_utf16(&a.name.to_lowercase(), &b.name.to_lowercase()))
    });
    let rows: Vec<Json> = items
        .into_iter()
        .map(|r| {
            let mut m = row(r, bodies, false);
            if served.state_of(&r.id) == Some(TrustState::Dirty) {
                m.insert("approval".into(), json!("dirty"));
            }
            Json::Object(m)
        })
        .collect();
    let mut out = Map::new();
    out.insert("resources".into(), Json::Array(rows));
    if !served.skipped.is_empty() {
        out.insert("skipped".into(), json!(served.skipped));
    }
    out.insert("approvals".into(), approval_counts(vault)?);
    Ok(Json::Object(out))
}

fn kind_ordinal(kind: ResourceKind) -> usize {
    match kind {
        ResourceKind::Skill => 0,
        ResourceKind::Agent => 1,
    }
}

pub fn get_resource(vault: &Path, request: &GetResourceRequest) -> Result<Json, EngineError> {
    begin_read(vault)?;
    let id = only_id(&request.id, GET_USAGE)?;
    let live = store::find(vault, id)?;
    // 읽는 것은 승인한 판이다. 승인 기록이 없는 새 파일은 보여 주지 않는다
    let served = store::scan_served(vault)?;
    let Some(cur) = served.by_id(id) else {
        return Err(EngineError::not_found(format!(
            "승인되지 않은 자원이라 조회되지 않는다(list-resource-changes 로 승인 상태를 본다): {id}"
        )));
    };
    let version = text_of(&request.version).filter(|v| !is_blank(v));
    let mut m = match &version {
        Some(version) => {
            let old = store::read_version(vault, cur, java_trim(version))?;
            let mut m = row(&Resource { token: None, ..old }, true, false);
            m.insert("version".into(), json!(java_trim(version)));
            m
        }
        None => row(cur, true, false),
    };
    if store::approval_state(vault, &live)? == TrustState::Dirty {
        m.insert("approval".into(), json!("dirty"));
    }
    // 고칠 때 넘길 토큰은 늘 지금 파일의 것이다(옛 판을 읽어도, 그 내용으로 되돌려 쓰려면 지금 파일의 토큰이 필요하다)
    m.insert("precondition".into(), json!(live.token));
    if cur.is_global() {
        m.insert(
            "referencedBy".into(),
            json!(refs::projects_referencing(vault, &cur.id)),
        );
    }
    Ok(Json::Object(m))
}

pub fn list_resource_versions(
    vault: &Path,
    request: &ResourceVersionsRequest,
) -> Result<Json, EngineError> {
    begin_read(vault)?;
    let id = only_id(&request.id, VERSIONS_USAGE)?;
    let cur = store::find(vault, id)?;
    let rows: Vec<Json> = store::versions(vault, &cur)
        .into_iter()
        .map(|v| {
            let mut m = Map::new();
            m.insert("version".into(), json!(v.version));
            m.insert("savedAt".into(), json!(v.saved_at));
            m.insert("bytes".into(), json!(v.bytes));
            m.insert("editedAt".into(), json!(v.edited_at));
            if let Some(session) = v.edited_session {
                m.insert("editedSession".into(), json!(session));
            }
            Json::Object(m)
        })
        .collect();
    Ok(json!({ "id": cur.id, "versions": rows }))
}

// ---------------------------------------------------------------- 저장·지우기

pub fn save_resource(vault: &Path, request: &SaveResourceRequest, context: &CallContext) -> Result<Json, EngineError> {
    let opts = collect(&[
        ("body", &request.body),
        ("description", &request.description),
        ("f1", &request.f1),
        ("id", &request.id),
        ("kind", &request.kind),
        ("name", &request.name),
        ("permission", &request.permission),
        ("project", &request.project),
        ("repo", &request.repo),
        ("scope", &request.scope),
        ("skills", &request.skills),
        ("tools", &request.tools),
    ]);
    save(&run(vault, &context), &opts)
}

fn save(run: &Run, opts: &Opts) -> Result<Json, EngineError> {
    let vault = run.vault;
    let id = opts.get("id").filter(|id| !is_blank(id));
    let Some(id) = id else {
        let kind = kind_of(opts.get("kind").map(String::as_str))?;
        let scope = scope_of(opts, true)?;
        reject_agent_only_for_skill(kind, |k| opts.contains_key(k))?;
        let fields = fields_of(|k| opts.get(k).map(String::as_str))?;
        let plan = store::plan_create(
            vault,
            kind,
            scope.project.as_deref(),
            &fields,
            &run.stamp,
            run.forced_id.as_deref(),
        )?;
        let cats = aggregate::classify(None, Some(&plan.after));
        if let Some(queued) = queue_if_needed(
            run,
            "save-resource",
            opts,
            &[],
            Some(&plan.after.id),
            None,
            Some(&plan.after),
            &cats,
            None,
        )? {
            return Ok(queued);
        }
        let r = store::commit_create(vault, &plan)?;
        approvals::record(
            vault,
            &approvals::res_key(&r.id),
            &store::live_bytes(vault, &r)?,
            run.by,
        )?;
        let mut m = row(&r, true, true);
        m.insert("created".into(), json!(true));
        return Ok(Json::Object(m));
    };
    let live = store::find(vault, java_trim(id))?;
    // 오류 차례는 종류 → 범위다. 범위 인자를 해석하기 전에 종류를 먼저 본다(둘 다 틀리면 종류 오류가 나온다).
    check_same_kind(&live, opts.get("kind").map(String::as_str))?;
    let scope = scope_of(opts, false)?;
    check_same_scope(&live, &scope)?;
    reject_agent_only_for_skill(live.kind, |k| opts.contains_key(k))?;
    // 고칠 바탕은 승인한 판이다. 직접 고쳐진 파일의 내용은 이어받지 않는다(받아들이려면 resolve-resource-edit)
    let base = store::base_for_edit(vault, &live)?;
    let plan = store::plan_update(
        vault,
        &base,
        &fields_of(|k| opts.get(k).map(String::as_str))?,
        opts.get("f1").map(String::as_str),
        &run.stamp,
    )?;
    if !plan.changed {
        let mut m = row(&base, true, true);
        m.insert("created".into(), json!(false));
        m.insert("changed".into(), json!(false));
        return Ok(Json::Object(m));
    }
    let cats = aggregate::classify(Some(&base), Some(&plan.after));
    if let Some(queued) = queue_if_needed(
        run,
        "save-resource",
        opts,
        &[],
        None,
        Some(&base),
        Some(&plan.after),
        &cats,
        None,
    )? {
        return Ok(queued);
    }
    let next = store::commit_update(vault, &plan)?;
    approvals::record(
        vault,
        &approvals::res_key(&next.id),
        &store::live_bytes(vault, &next)?,
        run.by,
    )?;
    let mut m = row(&next, true, true);
    m.insert("created".into(), json!(false));
    m.insert("changed".into(), json!(true));
    Ok(Json::Object(m))
}

pub fn delete_resource(vault: &Path, request: &ResourceIdRequest, context: &CallContext) -> Result<Json, EngineError> {
    let id = only_id(&request.id, DELETE_USAGE)?;
    delete(&run(vault, &context), id)
}

fn delete(run: &Run, id: &str) -> Result<Json, EngineError> {
    let vault = run.vault;
    let cur = store::find(vault, id)?;
    let cats = aggregate::classify(Some(&cur), None);
    if let Some(queued) = queue_if_needed(
        run,
        "delete-resource",
        &Opts::new(),
        &[id.to_string()],
        None,
        Some(&cur),
        None,
        &cats,
        None,
    )? {
        return Ok(queued);
    }
    let referenced_by = if cur.is_global() {
        refs::projects_referencing(vault, &cur.id)
    } else {
        Vec::new()
    };
    store::delete(vault, &cur)?;
    approvals::forget(vault, &approvals::res_key(&cur.id))?;
    let mut m = Map::new();
    m.insert("id".into(), json!(cur.id));
    m.insert("kind".into(), json!(cur.kind.label()));
    m.insert("scope".into(), json!(cur.scope()));
    if let Some(project) = &cur.project {
        m.insert("project".into(), json!(project));
    }
    m.insert("name".into(), json!(cur.name));
    m.insert("trashed".into(), json!(true));
    m.insert("referencedBy".into(), json!(referenced_by));
    Ok(Json::Object(m))
}

pub fn restore_resource(vault: &Path, request: &ResourceIdRequest, context: &CallContext) -> Result<Json, EngineError> {
    let id = only_id(&request.id, RESTORE_USAGE)?;
    restore(&run(vault, &context), id)
}

fn restore(run: &Run, id: &str) -> Result<Json, EngineError> {
    let vault = run.vault;
    let back = store::peek_trash(vault, id)?;
    let cats = aggregate::classify(None, Some(&back));
    if let Some(queued) = queue_if_needed(
        run,
        "restore-resource",
        &Opts::new(),
        &[id.to_string()],
        None,
        None,
        Some(&back),
        &cats,
        None,
    )? {
        return Ok(queued);
    }
    let r = store::restore(vault, id)?;
    approvals::record(
        vault,
        &approvals::res_key(&r.id),
        &store::live_bytes(vault, &r)?,
        run.by,
    )?;
    let mut m = row(&r, true, true);
    m.insert("restored".into(), json!(true));
    Ok(Json::Object(m))
}

// ---------------------------------------------------------------- 참조·쓸 수 있는 자원

pub fn set_resource_ref(
    vault: &Path,
    request: &SetResourceRefRequest,
    context: &CallContext,
) -> Result<Json, EngineError> {
    let (Some(project), Some(id)) = (text_of(&request.project), text_of(&request.id)) else {
        return Err(EngineError::invalid_argument(format!(
            "set-resource-ref 는 프로젝트 slug 와 글로벌 자원 id 를 받는다: {REF_USAGE}"
        )));
    };
    let opts = collect(&[
        ("remove", &request.remove),
        ("with-skills", &request.with_skills),
    ]);
    ref_impl(&run(vault, &context), &project, &id, &opts)
}

fn ref_impl(run: &Run, project: &str, id: &str, opts: &Opts) -> Result<Json, EngineError> {
    let vault = run.vault;
    let remove = is_true(opts.get("remove"));
    let with_skills = is_true(opts.get("with-skills"));
    store::require_project(vault, project)?;
    if !ResourceKind::is_id(id) {
        return Err(EngineError::invalid_argument(format!(
            "자원 id 모양이 아니다(sk… 또는 ap…): {id}"
        )));
    }

    // 바탕은 승인한 참조 목록과 승인한 자원이다(직접 고쳐진 refs.md의 내용은 이어받지 않는다)
    let served = store::scan_served(vault)?;
    let entries = refs::read_approved(vault, project)?;
    let RefsChange {
        next,
        added,
        removed,
    } = change_refs(entries, id, remove, with_skills, |rid| served.by_id(rid))?;

    let cats = aggregate::classify_refs(&added, &removed);
    if !cats.is_empty() {
        let info = RefInfo {
            project: project.to_string(),
            added: added.clone(),
            removed: removed.clone(),
        };
        if let Some(queued) = queue_if_needed(
            run,
            "set-resource-ref",
            opts,
            &[project.to_string(), id.to_string()],
            None,
            None,
            None,
            &cats,
            Some(&info),
        )? {
            return Ok(queued);
        }
    }
    refs::write(vault, project, &next)?;
    approvals::record(
        vault,
        &approvals::refs_key(project),
        &refs::live_bytes(vault, project),
        run.by,
    )?;

    let rows: Vec<Json> = next
        .iter()
        .map(|e| {
            let found = served.by_id(&e.id).filter(|r| r.is_global());
            json!({
                "id": e.id,
                "status": if found.is_some() { "ok" } else { "missing" },
                "kind": ResourceKind::of_id(&e.id).map_or("", ResourceKind::label),
                "name": found.map_or(e.hint.clone(), |r| r.name.clone()),
            })
        })
        .collect();
    Ok(json!({"project": project, "added": added, "removed": removed, "refs": rows}))
}

pub fn usable_resources(
    vault: &Path,
    request: &UsableResourcesRequest,
) -> Result<Json, EngineError> {
    begin_read(vault)?;
    let project = only_id(&request.project, USABLE_USAGE)?;
    let bodies = is_true(text_of(&request.bodies).as_ref());
    let u = resolver::resolve(vault, project)?;
    let item_rows = |items: &[resolver::Item]| -> Vec<Json> {
        items
            .iter()
            .map(|it| {
                let mut m = row(&it.resource, bodies, false);
                m.insert("source".into(), json!(it.source));
                Json::Object(m)
            })
            .collect()
    };
    let broken: Vec<Json> = u
        .broken
        .iter()
        .map(|b| json!({"id": b.id, "hint": b.hint}))
        .collect();
    let missing: Vec<Json> = u
        .missing_skills
        .iter()
        .map(|mi| {
            let mut m = Map::new();
            m.insert(
                "agent".into(),
                json!({"id": mi.agent.id, "name": mi.agent.name}),
            );
            m.insert("skillId".into(), json!(mi.skill_id));
            if let Some(skill) = &mi.skill {
                m.insert(
                    "skill".into(),
                    json!({"name": skill.name, "scope": skill.scope()}),
                );
            }
            m.insert("canReference".into(), json!(mi.can_reference()));
            Json::Object(m)
        })
        .collect();
    Ok(json!({
        "project": u.project,
        "skills": item_rows(&u.skills),
        "agents": item_rows(&u.agents),
        "broken": broken,
        "missingSkills": missing,
    }))
}

// ---------------------------------------------------------------- 승인 워크플로(#53)

fn strings(items: &[String]) -> JsonValue {
    JsonValue::Array(items.iter().map(string).collect())
}

/// 변경이 승인을 기다려야 하면 대기에 올리고 응답을 돌려준다. 바로 적용해도 되면 `None`이다.
/// 같은 변경이 이미 대기 중이면 새로 올리지 않고 그것을 알려 준다(AI가 같은 요청을 되풀이해도 쌓이지 않게).
#[allow(clippy::too_many_arguments)]
fn queue_if_needed(
    run: &Run,
    command: &str,
    args: &Opts,
    positional: &[String],
    planned_id: Option<&str>,
    before: Option<&Resource>,
    after: Option<&Resource>,
    cats: &[Category],
    ref_info: Option<&RefInfo>,
) -> Result<Option<Json>, EngineError> {
    if run.apply {
        return Ok(None);
    }
    let vault = run.vault;
    let decision = aggregate::decide(&approvals::policy(vault)?, cats, run.ai);
    if !decision.queue {
        return Ok(None);
    }

    for m in changes::pending(vault) {
        if changes::get_str(&m, "command") == Some(command)
            && same_args(m.get("args"), args)
            && same_positional(m.get("positional"), positional)
        {
            return Ok(Some(queued_response(&m, true)));
        }
    }
    let any = after.or(before);
    let mut proposer: Vec<(String, JsonValue)> = Vec::new();
    if let Some(session) = &run.stamp.session {
        proposer.push(("session".into(), string(session)));
    }
    if let Some(agent) = &run.stamp.agent {
        proposer.push(("agent".into(), string(agent)));
    }
    proposer.push(("channel".into(), string(&run.channel)));
    let fixed = decision.reasons.iter().any(|c| c.fixed());
    let mut target: Vec<(String, JsonValue)> = Vec::new();
    if let Some(any) = any {
        target.push(("kind".into(), string(any.kind.label())));
        target.push(("scope".into(), string(any.scope())));
        if let Some(project) = &any.project {
            target.push(("project".into(), string(project)));
        }
        target.push(("id".into(), string(&any.id)));
        target.push(("name".into(), string(&any.name)));
    } else if let Some(info) = ref_info {
        target.push(("scope".into(), string("project")));
        target.push(("project".into(), string(&info.project)));
    }
    let mut c: Vec<(String, JsonValue)> = vec![
        ("id".into(), string(changes::new_id())),
        ("status".into(), string(changes::PENDING)),
        ("command".into(), string(command)),
        ("createdAt".into(), string(&run.stamp.at)),
        ("proposer".into(), JsonValue::Object(proposer)),
        (
            "categories".into(),
            JsonValue::Array(cats.iter().map(|x| string(x.code())).collect()),
        ),
        (
            "reasons".into(),
            JsonValue::Array(decision.reasons.iter().map(|x| string(x.code())).collect()),
        ),
        ("fixed".into(), literal(fixed.to_string())),
        ("target".into(), JsonValue::Object(target)),
        (
            "args".into(),
            JsonValue::Object(args.iter().map(|(k, v)| (k.clone(), string(v))).collect()),
        ),
        (
            "positional".into(),
            JsonValue::Array(positional.iter().map(string).collect()),
        ),
    ];
    if let Some(planned) = planned_id {
        c.push(("forcedId".into(), string(planned)));
    }
    if let Some(before) = before {
        c.push(("before".into(), changes::view(before)));
    }
    if let Some(after) = after {
        c.push(("after".into(), changes::view(after)));
    }
    if let Some(info) = ref_info {
        c.push((
            "ref".into(),
            JsonValue::Object(vec![
                ("project".into(), string(&info.project)),
                ("added".into(), strings(&info.added)),
                ("removed".into(), strings(&info.removed)),
            ]),
        ));
    }
    c.push((
        "summary".into(),
        string(summarize_change(command, before, after, ref_info)),
    ));
    let change = JsonValue::Object(c);
    changes::write(vault, &change)?;
    Ok(Some(queued_response(&change, false)))
}

/// 저장된 `args`가 지금 옵션과 같은가(`Map.equals`: 값은 글일 때만 같다)
fn same_args(stored: Option<&JsonValue>, args: &Opts) -> bool {
    let Some(JsonValue::Object(entries)) = stored else {
        return false;
    };
    entries.len() == args.len()
        && entries
            .iter()
            .all(|(k, v)| matches!(v, JsonValue::Str(s) if args.get(k) == Some(s)))
}

fn same_positional(stored: Option<&JsonValue>, positional: &[String]) -> bool {
    let Some(JsonValue::Array(items)) = stored else {
        return false;
    };
    items.len() == positional.len()
        && items
            .iter()
            .zip(positional)
            .all(|(a, b)| matches!(a, JsonValue::Str(s) if s == b))
}

fn queued_response(c: &changes::Change, duplicate: bool) -> Json {
    let mut m = Map::new();
    m.insert("queued".into(), json!(true));
    m.insert(
        "changeId".into(),
        c.get("id").map_or(Json::Null, json_to_serde),
    );
    m.insert("status".into(), json!(changes::PENDING));
    if duplicate {
        m.insert("duplicate".into(), json!(true));
    }
    m.insert(
        "summary".into(),
        c.get("summary").map_or(Json::Null, json_to_serde),
    );
    m.insert("reasons".into(), Json::Array(reason_rows(c)));
    m.insert(
        "message".into(),
        json!("변경을 적용하지 않고 승인 대기에 올렸다. 사람이 앱의 스킬·에이전트 화면에서 승인해야 적용되고, 그때까지 목록과 조회에는 이전 판이 그대로 보인다. 처리 결과는 get-resource-change 로 본다."),
    );
    Json::Object(m)
}

fn category_row(cat: Category) -> Json {
    json!({"category": cat.code(), "label": cat.label(), "fixed": cat.fixed()})
}

/// 칸에 적힌 갈래 코드들을 응답 행으로(모르는 코드는 건너뛴다)
fn code_rows(c: &changes::Change, key: &str) -> Vec<Json> {
    match c.get(key) {
        Some(JsonValue::Array(codes)) => codes
            .iter()
            .filter_map(|code| Category::of_code(&java_text(code)))
            .map(category_row)
            .collect(),
        _ => Vec::new(),
    }
}

fn reason_rows(c: &changes::Change) -> Vec<Json> {
    code_rows(c, "reasons")
}

/// `String.valueOf`: 글은 그대로, 따옴표 없는 값은 읽은 모양(정수 `5`, 실수 `5.0`)으로 쓴다.
fn java_text(value: &JsonValue) -> String {
    match value {
        JsonValue::Str(s) => s.clone(),
        JsonValue::Literal(raw) => match raw.as_str() {
            "true" | "false" | "null" => raw.clone(),
            text => text.parse::<i64>().map_or_else(
                |_| {
                    text.parse::<f64>().map_or_else(
                        |_| text.to_string(),
                        crate::kernel::java::java_double_string,
                    )
                },
                |n| n.to_string(),
            ),
        },
        other => json::compact(other),
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

fn approval_counts(vault: &Path) -> Result<Json, EngineError> {
    Ok(json!({
        "pending": changes::pending(vault).len(),
        "unapproved": unapproved_rows(vault)?.len(),
    }))
}

pub fn list_resource_changes(
    vault: &Path,
    request: &ListResourceChangesRequest,
) -> Result<Json, EngineError> {
    begin_read(vault)?;
    let mut decided_limit = DEFAULT_DECIDED;
    if let Some(raw) = text_of(&request.decided).filter(|r| !is_blank(r)) {
        decided_limit = java_trim(&raw).parse::<i32>().unwrap_or(-1);
        if decided_limit < 0 {
            return Err(EngineError::invalid_argument(format!(
                "decided 는 0 이상의 정수여야 한다: {raw}"
            )));
        }
    }
    let mut pending: Vec<Json> = Vec::new();
    let mut decided: Vec<Json> = Vec::new();
    for c in changes::list(vault) {
        if changes::get_str(&c, "status") == Some(changes::PENDING) {
            pending.push(change_row(&c, false));
        } else if (decided.len() as i32) < decided_limit {
            decided.push(change_row(&c, false));
        }
    }
    let unapproved = unapproved_rows(vault)?;
    Ok(json!({
        "counts": {"pending": pending.len(), "unapproved": unapproved.len()},
        "pending": pending,
        "unapproved": unapproved,
        "decided": decided,
        "policy": approvals::policy(vault)?.to_json(),
    }))
}

pub fn get_resource_change(
    vault: &Path,
    request: &ResourceChangeRequest,
) -> Result<Json, EngineError> {
    begin_read(vault)?;
    let id = only_id(&request.id, CHANGE_GET_USAGE)?;
    Ok(change_row(&changes::read(vault, id)?, true))
}

/// `Objects.equals`로 본 같음: 맵은 차례를 안 따지고, 정수와 실수는 다른 값이다.
fn same_value(a: Option<&JsonValue>, b: Option<&JsonValue>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(JsonValue::Literal(x)), None) | (None, Some(JsonValue::Literal(x))) => x == "null",
        (Some(x), Some(y)) => same_json(x, y),
        _ => false,
    }
}

fn same_json(a: &JsonValue, b: &JsonValue) -> bool {
    match (a, b) {
        (JsonValue::Str(x), JsonValue::Str(y)) => x == y,
        (JsonValue::Array(x), JsonValue::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same_json(p, q))
        }
        (JsonValue::Object(x), JsonValue::Object(y)) => {
            x.len() == y.len()
                && x.iter().all(|(k, v)| {
                    y.iter()
                        .find(|(k2, _)| k2 == k)
                        .is_some_and(|(_, v2)| same_json(v, v2))
                })
        }
        (JsonValue::Literal(x), JsonValue::Literal(y)) => {
            match (x.parse::<i64>(), y.parse::<i64>()) {
                (Ok(p), Ok(q)) => p == q,
                (Ok(_), Err(_)) | (Err(_), Ok(_)) => false,
                (Err(_), Err(_)) => match (x.parse::<f64>(), y.parse::<f64>()) {
                    (Ok(p), Ok(q)) => p.to_bits() == q.to_bits() || (p.is_nan() && q.is_nan()),
                    _ => x == y,
                },
            }
        }
        _ => false,
    }
}

/// 변경 한 줄. 목록에서는 비교용 칸(before, after, args)을 빼고, 하나를 볼 때만 싣는다.
fn change_row(c: &changes::Change, detail: bool) -> Json {
    let mut m = Map::new();
    for key in [
        "id",
        "status",
        "command",
        "createdAt",
        "summary",
        "target",
        "proposer",
        "fixed",
    ] {
        if let Some(v) = c.get(key) {
            m.insert(key.into(), json_to_serde(v));
        }
    }
    m.insert("reasons".into(), Json::Array(reason_rows(c)));
    m.insert("categories".into(), Json::Array(code_rows(c, "categories")));
    for key in ["decidedAt", "decidedBy", "reason"] {
        if let Some(v) = c.get(key) {
            m.insert(key.into(), json_to_serde(v));
        }
    }
    if detail {
        for key in ["before", "after", "ref", "args", "positional", "forcedId"] {
            if let Some(v) = c.get(key) {
                m.insert(key.into(), json_to_serde(v));
            }
        }
        if let (Some(b @ JsonValue::Object(_)), Some(a @ JsonValue::Object(_))) =
            (c.get("before"), c.get("after"))
        {
            let changed: Vec<&str> = [
                "name",
                "description",
                "permission",
                "skills",
                "repo",
                "tools",
                "body",
            ]
            .into_iter()
            .filter(|key| !same_value(b.get(key), a.get(key)))
            .collect();
            m.insert("changedFields".into(), json!(changed));
        }
    }
    Json::Object(m)
}

/// 승인 안 된 직접 수정: 승인한 뒤 파일이 바뀌었거나(dirty), 승인 기록 없이 생긴(new) 자원 파일과 참조 목록.
fn unapproved_rows(vault: &Path) -> Result<Vec<Json>, EngineError> {
    let mut rows: Vec<Json> = Vec::new();
    let served = store::scan_served(vault)?;
    let raw = store::scan(vault);
    for approved in &served.items {
        if served.state_of(&approved.id) != Some(TrustState::Dirty) {
            continue;
        }
        let mut m = unapproved_row(approved, "dirty");
        m.insert("approved".into(), view_json(approved));
        if let Some(live) = raw.by_id(&approved.id) {
            m.insert("live".into(), view_json(live));
        }
        rows.push(Json::Object(m));
    }
    for r in &served.untrusted {
        let mut m = unapproved_row(r, "new");
        m.insert("live".into(), view_json(r));
        rows.push(Json::Object(m));
    }
    let mut projects: BTreeSet<Utf16Key> = refs::projects_with_refs_file(vault)
        .into_iter()
        .map(Utf16Key)
        .collect();
    for (key, _) in &approvals::ledger(vault)?.items {
        if let Some(project) = key.strip_prefix("refs:") {
            projects.insert(Utf16Key(project.to_string()));
        }
    }
    for Utf16Key(project) in projects {
        let state = refs::state(vault, &project)?;
        if state == TrustState::Approved {
            continue;
        }
        let ids = |entries: Vec<refs::Entry>| -> Vec<String> {
            entries.into_iter().map(|e| e.id).collect()
        };
        rows.push(json!({
            "key": approvals::refs_key(&project),
            "type": "refs",
            "state": state.label(),
            "project": project,
            "approved": ids(refs::read_approved(vault, &project)?),
            "live": ids(refs::read(vault, &project)),
        }));
    }
    Ok(rows)
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

fn unapproved_row(r: &Resource, state: &str) -> Map<String, Json> {
    let mut m = Map::new();
    m.insert("key".into(), json!(r.id));
    m.insert("type".into(), json!("resource"));
    m.insert("state".into(), json!(state));
    m.insert("id".into(), json!(r.id));
    m.insert("kind".into(), json!(r.kind.label()));
    m.insert("scope".into(), json!(r.scope()));
    if let Some(project) = &r.project {
        m.insert("project".into(), json!(project));
    }
    m.insert("name".into(), json!(r.name));
    m.insert("path".into(), json!(r.rel_path));
    m
}

pub fn approve_resource_change(
    vault: &Path,
    request: &ChangeDecisionRequest,
    context: &CallContext,
) -> Result<Json, EngineError> {
    require_human(is_ai(&context), "승인")?;
    let id = only_id(&request.id, APPROVE_USAGE)?;
    let mut change = pending_change(vault, id)?;
    let run = apply_run(vault, &change);
    let result = match apply_change(&run, &change) {
        Ok(result) => result,
        Err(e) => {
            if aggregate::goes_stale(&e) {
                decide(
                    vault,
                    &mut change,
                    changes::STALE,
                    Some(&format!("적용하지 못했다: {}", e.message)),
                )?;
            }
            return Err(e);
        }
    };
    decide(vault, &mut change, changes::APPROVED, None)?;
    Ok(json!({
        "approved": true,
        "changeId": change.get("id").map_or(Json::Null, json_to_serde),
        "summary": change.get("summary").map_or(Json::Null, json_to_serde),
        "result": result,
    }))
}

pub fn reject_resource_change(
    vault: &Path,
    request: &ChangeDecisionRequest,
    context: &CallContext,
) -> Result<Json, EngineError> {
    require_human(is_ai(&context), "거절")?;
    let id = only_id(&request.id, REJECT_USAGE)?;
    let mut change = pending_change(vault, id)?;
    let reason = text_of(&request.reason)
        .filter(|r| !is_blank(r))
        .map(|r| trim(&r).to_string());
    decide(vault, &mut change, changes::REJECTED, reason.as_deref())?;
    Ok(json!({
        "rejected": true,
        "changeId": change.get("id").map_or(Json::Null, json_to_serde),
        "summary": change.get("summary").map_or(Json::Null, json_to_serde),
    }))
}

fn pending_change(vault: &Path, id: &str) -> Result<changes::Change, EngineError> {
    let change = changes::read(vault, id)?;
    if changes::get_str(&change, "status") != Some(changes::PENDING) {
        let current = change
            .get("status")
            .map_or_else(|| "null".to_string(), java_text);
        return Err(aggregate::already_decided(id, &current));
    }
    Ok(change)
}

fn decide(
    vault: &Path,
    change: &mut changes::Change,
    status: &str,
    reason: Option<&str>,
) -> Result<(), EngineError> {
    changes::put(change, "status", string(status));
    changes::put(
        change,
        "decidedAt",
        string(iso_offset_date_time(now_millis())),
    );
    changes::put(change, "decidedBy", string("human"));
    if let Some(reason) = reason {
        changes::put(change, "reason", string(reason));
    }
    changes::write(vault, change)?;
    changes::prune(vault);
    Ok(())
}

/// 승인한 변경을 적용할 실행. 올린 사람의 기록으로 쓰고, 대기에 다시 올리지 않는다.
fn apply_run<'a>(vault: &'a Path, change: &changes::Change) -> Run<'a> {
    let proposer = change.get("proposer");
    let field = |key: &str| {
        proposer
            .and_then(|p| p.get(key))
            .filter(|v| !v.is_null())
            .map(java_text)
    };
    Run {
        vault,
        ai: false,
        stamp: Stamp {
            at: change
                .get("createdAt")
                .map_or_else(|| "null".to_string(), java_text),
            session: field("session"),
            agent: field("agent"),
        },
        apply: true,
        by: "approved",
        forced_id: change
            .get("forcedId")
            .filter(|v| !v.is_null())
            .map(java_text),
        channel: field("channel").unwrap_or_else(|| "mcp".to_string()),
    }
}

fn apply_change(run: &Run, change: &changes::Change) -> Result<Json, EngineError> {
    let mut args = Opts::new();
    if let Some(JsonValue::Object(entries)) = change.get("args") {
        for (key, value) in entries {
            args.insert(key.clone(), java_text(value));
        }
    }
    let mut positional: Vec<String> = Vec::new();
    if let Some(JsonValue::Array(items)) = change.get("positional") {
        positional.extend(items.iter().map(java_text));
    }
    let command = change
        .get("command")
        .map_or_else(|| "null".to_string(), java_text);
    let missing = || EngineError::internal(format!("대기 변경의 자리 인자가 모자라다: {command}"));
    match command.as_str() {
        "save-resource" => save(run, &args),
        "delete-resource" => delete(run, positional.first().ok_or_else(missing)?),
        "restore-resource" => restore(run, positional.first().ok_or_else(missing)?),
        "set-resource-ref" => {
            let (project, id) = (
                positional.first().ok_or_else(missing)?,
                positional.get(1).ok_or_else(missing)?,
            );
            ref_impl(run, project, id, &args)
        }
        _ => Err(EngineError::internal(format!(
            "알 수 없는 대기 변경 명령이다: {command}"
        ))),
    }
}

pub fn resolve_resource_edit(
    vault: &Path,
    request: &ResolveResourceEditRequest,
    context: &CallContext,
) -> Result<Json, EngineError> {
    require_human(is_ai(&context), "승인 안 된 직접 수정의 처리")?;
    let key = only_id(&request.id, RESOLVE_USAGE)?;
    let action = text_of(&request.action);
    let Some(action) = action
        .as_deref()
        .map(java_trim)
        .filter(|a| *a == "approve" || *a == "revert")
    else {
        return Err(EngineError::invalid_argument(format!(
            "action 은 approve 나 revert 여야 한다: {RESOLVE_USAGE}"
        )));
    };
    let approve = action == "approve";
    let out = json!({"key": key, "action": action});
    if let Some(project) = key.strip_prefix("refs:") {
        let state = refs::state(vault, project)?;
        if state == TrustState::Approved {
            return Err(EngineError::conflict(format!(
                "승인 안 된 변경이 없다: {key}"
            )));
        }
        if approve {
            approvals::record(vault, key, &refs::live_bytes(vault, project), "adopted")?;
        } else if state == TrustState::Dirty {
            let copy = approvals::copy(vault, key).unwrap_or_default();
            store::restore_refs_bytes(vault, project, &copy)?;
        } else {
            refs::write(vault, project, &[])?;
        }
        return Ok(out);
    }
    let live = store::find(vault, key)?;
    let state = store::approval_state(vault, &live)?;
    if state == TrustState::Approved {
        return Err(EngineError::conflict(format!(
            "승인 안 된 변경이 없다: {key}"
        )));
    }
    if approve {
        approvals::record(
            vault,
            &approvals::res_key(&live.id),
            &store::live_bytes(vault, &live)?,
            "adopted",
        )?;
    } else if state == TrustState::Dirty {
        store::revert_to_approved(vault, &live)?;
    } else {
        store::delete(vault, &live)?;
        approvals::forget(vault, &approvals::res_key(&live.id))?;
    }
    Ok(out)
}

fn policy_response(vault: &Path) -> Result<Json, EngineError> {
    Ok(json!({
        "policy": approvals::policy(vault)?.to_json(),
        "defaults": Policy::defaults().to_json(),
        "categories": aggregate::categories_json(),
    }))
}

pub fn get_resource_policy(vault: &Path) -> Result<Json, EngineError> {
    begin_read(vault)?;
    policy_response(vault)
}

pub fn set_resource_policy(
    vault: &Path,
    request: &SetResourcePolicyRequest,
    context: &CallContext,
) -> Result<Json, EngineError> {
    require_human(is_ai(&context), "승인 정책 변경")?;
    let opts = collect(&[
        ("project", &request.project),
        ("global", &request.global),
        ("refs", &request.refs),
        ("permission", &request.permission),
    ]);
    let mut policy = approvals::policy(vault)?;
    let mut any = false;
    for key in POLICY_KEYS {
        let Some(value) = opts.get(key) else {
            continue;
        };
        policy = policy.with(key, trim(value))?;
        any = true;
    }
    if !any {
        return Err(EngineError::invalid_argument(format!(
            "바꿀 칸이 필요하다: {POLICY_SET_USAGE}"
        )));
    }
    approvals::set_policy(vault, &policy)?;
    policy_response(vault)
}
