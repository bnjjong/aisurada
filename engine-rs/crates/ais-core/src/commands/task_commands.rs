//! 태스크(티켓)를 다루는 명령 9개: `create_task`, `get_task`, `list_tasks`, `update_task`, `transition_task`, `link_tasks`,
//! `add_task_comment`, `delete_task`, `restore_task`. 외부 설명서에 없는 확장이다.
//! 인자 이름, 검사 순서, 오류 문구와 응답 필드는 Java 엔진의 `TaskCommands`와 같고, 파일 글은 바이트까지 같다.
//!
//! 태스크는 키(`TSK-14`)나 id(`tk…`)로 부른다. 완료 기준은 만들 때 반드시 적어야 하고, 상태 전이의 규칙과 조건은
//! `transition_task`가 한 곳에서 검사한다(`task::transitions`).

use crate::kernel::task_link::TaskLinkKind;
use std::cmp::Reverse;
use std::path::Path;

use serde::Deserialize;
use serde_json::{Value as Json, json};

use super::note_meta_write::BoolArg;
use crate::error::EngineError;
use crate::kernel::actor::{CallContext, ManagerContext};
use crate::task::aggregate::{self, Fields, Scan};
use crate::task::format::{self, java_trim};
use crate::task::store;
use crate::task::transitions::{self, HUMAN};
use crate::task::{Task, TaskStatus};
use crate::text::{is_blank, trim};
use crate::vault::check_manager_project;
use crate::write::marker::check_workspace_marker;
use crate::write::stamp::stamp_of;

#[allow(dead_code)]
const CREATE_USAGE: &str = "create-task title=<요약> repo=<owner/name> criteria=<완료 기준 JSON 배열. 하나 이상. 예: [\"…\",{\"text\":\"…\"}]> [type=feature|bug|chore] [priority=high|medium|low] [project=<프로젝트 slug>] [base=<기준 브랜치>] [body=<설명>] [labels=<JSON 배열>] [refs=<연결한 자원 JSON 배열. 예: [{\"kind\":\"note\",\"target\":\"nd…\"},{\"kind\":\"github\",\"target\":\"owner/name#53\"}]>] [actor=human|runner]";
const GET_USAGE: &str =
    "get-task <키(TSK-14) 또는 id> [activity=<최근 활동 개수. 기본 100. 0이면 전부>]";
const UPDATE_USAGE: &str = "update-task <키 또는 id> f1=<직전에 읽은 토큰> [title=<요약>] [type=] [priority=] [repo=<owner/name>] [project=<프로젝트 slug. 빈 값이면 지운다>] [base=<기준 브랜치. 빈 값이면 지운다>] [body=<설명>] [labels=<JSON 배열. 전체 교체>] [criteria=<JSON 배열. 전체 교체>] [refs=<JSON 배열. 전체 교체>] [actor=human|runner]";
const TRANSITION_USAGE: &str = "transition-task <키 또는 id> to=<상태: draft|ready-for-develop|in-progress|waiting-answer|in-review|done|failed|cancelled> [actor=human|runner. 기본 human] [from=<기대하는 지금 상태. 다르면 거절한다>]";
const LINK_USAGE: &str = "link-tasks <건 쪽 키> <받는 쪽 키> type=blocks|relates|duplicates [remove=true] [actor=human|runner]";
const COMMENT_USAGE: &str = "add-task-comment <키 또는 id> body=<댓글> [actor=human|runner]";
const DELETE_USAGE: &str = "delete-task <키 또는 id>";
const RESTORE_USAGE: &str =
    "restore-task <휴지통의 태스크 키 또는 id. list-tasks trash=true 로 조회한다>";
const DEFAULT_ACTIVITY: usize = 100;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CreateTaskRequest {
    pub title: Option<String>,
    pub repo: Option<String>,
    /// 호출 객체에서는 배열, 명령줄에서는 JSON 글
    pub criteria: Option<Json>,
    #[serde(rename = "type")]
    pub task_type: Option<String>,
    pub priority: Option<String>,
    pub project: Option<String>,
    pub base: Option<String>,
    pub body: Option<String>,
    pub labels: Option<Json>,
    pub refs: Option<Json>,
    pub actor: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GetTaskRequest {
    /// 태스크 키(`TSK-14`) 또는 id(`tk…`)
    pub id: String,
    /// 최근 활동 개수(안 주면 100, 0이면 전부)
    pub activity: Option<Json>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ListTasksRequest {
    pub status: Option<String>,
    #[serde(rename = "type")]
    pub task_type: Option<String>,
    pub repo: Option<String>,
    pub label: Option<String>,
    pub q: Option<String>,
    pub blocked: Option<BoolArg>,
    pub bodies: Option<BoolArg>,
    pub trash: Option<BoolArg>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UpdateTaskRequest {
    pub id: String,
    pub f1: Option<String>,
    pub title: Option<String>,
    #[serde(rename = "type")]
    pub task_type: Option<String>,
    pub priority: Option<String>,
    pub repo: Option<String>,
    pub project: Option<String>,
    pub base: Option<String>,
    pub body: Option<String>,
    pub labels: Option<Json>,
    pub criteria: Option<Json>,
    pub refs: Option<Json>,
    pub actor: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TransitionTaskRequest {
    pub id: String,
    pub to: Option<String>,
    /// 기대하는 지금 상태. 다르면 거절한다
    pub from: Option<String>,
    pub actor: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LinkTasksRequest {
    /// 건 쪽 키
    pub from: Option<String>,
    /// 받는 쪽 키
    pub to: Option<String>,
    #[serde(rename = "type")]
    pub link_type: Option<String>,
    pub remove: Option<BoolArg>,
    pub actor: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AddTaskCommentRequest {
    pub id: String,
    pub body: Option<String>,
    pub actor: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TaskRefRequest {
    pub id: String,
}

// ---------------------------------------------------------------- 공통

fn is_true(value: &Option<BoolArg>) -> bool {
    match value {
        None => false,
        Some(BoolArg::Bool(b)) => *b,
        Some(BoolArg::Text(text)) => trim(text).to_lowercase() == "true",
    }
}

fn given(value: &Option<String>) -> Option<&str> {
    value.as_deref().filter(|v| !is_blank(v))
}

fn only_ref<'a>(id: &'a str, usage: &str) -> Result<&'a str, EngineError> {
    if is_blank(id) {
        return Err(EngineError::invalid_argument(format!(
            "인자가 하나 필요하다: {usage}"
        )));
    }
    Ok(java_trim(id))
}

fn actor_of(actor: &Option<String>) -> Result<String, EngineError> {
    let Some(raw) = actor.as_deref().filter(|a| !is_blank(a)) else {
        return Ok(HUMAN.to_string());
    };
    let value = trim(raw).to_lowercase();
    if !transitions::is_actor(&value) {
        return Err(EngineError::invalid_argument(format!(
            "actor 는 human 이나 runner 여야 한다: {raw}"
        )));
    }
    Ok(value)
}

#[allow(clippy::too_many_arguments)]
fn fields_of(
    title: &Option<String>,
    task_type: &Option<String>,
    priority: &Option<String>,
    repo: &Option<String>,
    project: &Option<String>,
    base: &Option<String>,
    body: &Option<String>,
    labels: &Option<Json>,
    criteria: &Option<Json>,
    refs: &Option<Json>,
) -> Result<Fields, EngineError> {
    Ok(Fields {
        title: title.as_deref().map(format::title).transpose()?,
        task_type: task_type.as_deref().map(format::task_type).transpose()?,
        priority: priority.as_deref().map(format::priority).transpose()?,
        repo: repo
            .as_deref()
            .map(|r| format::repo(r).map(Option::unwrap_or_default))
            .transpose()?,
        project: project.as_deref().map(|p| trim(p).to_string()),
        base_branch: base
            .as_deref()
            .map(|b| format::base_branch(b).map(Option::unwrap_or_default))
            .transpose()?,
        body: body.as_deref().map(format::body).transpose()?,
        labels: labels.as_ref().map(format::labels).transpose()?,
        criteria: criteria.as_ref().map(format::criteria).transpose()?,
        refs: refs.as_ref().map(format::refs).transpose()?,
    })
}

// ---------------------------------------------------------------- 응답 모양

/// 목록 행. 설명은 `with_body`일 때만 싣는다.
fn row(scan: &Scan, task: &Task, with_body: bool) -> Json {
    let blocks: Vec<&str> = task
        .links
        .iter()
        .filter(|l| l.kind == TaskLinkKind::Blocks && scan.by_key(&l.to).is_some())
        .map(|l| l.to.as_str())
        .collect();
    let mut m = json!({
        "key": task.key,
        "id": task.id,
        "title": task.title,
        "type": task.task_type,
        "priority": task.priority,
        "status": task.status.label(),
        "statusTitle": task.status.title(),
        "labels": task.labels,
        "criteriaDone": task.criteria.iter().filter(|c| c.done).count(),
        "criteriaTotal": task.criteria.len(),
        "blockedBy": aggregate::unfinished_blockers(scan, &task.key),
        "blocks": blocks,
        "path": task.rel_path,
        "createdAt": task.created_at,
        "editedAt": task.edited_at,
    });
    for (key, value) in [
        ("repo", &task.repo),
        ("project", &task.project),
        ("baseBranch", &task.base_branch),
        ("manager", &task.manager),
        ("editedSession", &task.edited_session),
        ("editedAgent", &task.edited_agent),
    ] {
        if let Some(value) = value {
            m[key] = json!(value);
        }
    }
    if with_body {
        m["body"] = json!(task.body);
    }
    m
}

fn related_rows(scan: &Scan, task: &Task) -> Vec<Json> {
    aggregate::related(scan, task)
        .into_iter()
        .map(
            |r| json!({"type": r.kind, "key": r.key, "title": r.title, "status": r.status.label()}),
        )
        .collect()
}

/// 상세. 설명, 완료 기준, 자원, 연결, Ready 조건, 사람이 바꿀 수 있는 상태, 활동, 고칠 때 넘길 토큰을 모두 싣는다.
fn detail(
    vault: &Path,
    scan: &Scan,
    task: &Task,
    activity_limit: usize,
) -> Result<Json, EngineError> {
    let mut m = row(scan, task, true);
    m["criteria"] = json!(
        task.criteria
            .iter()
            .map(|c| {
                let mut r = json!({"text": c.text, "done": c.done});
                if let Some(at) = &c.checked_at {
                    r["checkedAt"] = json!(at);
                }
                if let Some(by) = &c.checked_by {
                    r["checkedBy"] = json!(by);
                }
                r
            })
            .collect::<Vec<_>>()
    );
    m["refs"] = json!(
        task.refs
            .iter()
            .map(|r| {
                let mut v = json!({"kind": r.kind, "target": r.target});
                if let Some(title) = &r.title {
                    v["title"] = json!(title);
                }
                v
            })
            .collect::<Vec<_>>()
    );
    m["links"] = json!(related_rows(scan, task));
    let readiness = transitions::readiness(task);
    m["ready"] = json!(readiness.iter().all(|c| c.ok));
    m["readiness"] = json!(
        readiness
            .iter()
            .map(|c| json!({"item": c.item, "ok": c.ok, "label": c.label}))
            .collect::<Vec<_>>()
    );
    let blockers = aggregate::unfinished_blockers(scan, &task.key);
    m["transitions"] = json!(
        transitions::options(task, HUMAN, &blockers)
            .into_iter()
            .map(|o| {
                let mut r = json!({"to": o.to.label(), "title": o.to.title(), "ok": o.ok});
                if let Some(reason) = o.reason {
                    r["reason"] = json!(reason);
                }
                r
            })
            .collect::<Vec<_>>()
    );
    m["activity"] = json!(store::activity(vault, &task.key, activity_limit)?);
    if let Some(token) = &task.token {
        m["precondition"] = json!(token);
    }
    Ok(m)
}

fn refetch<'a>(scan: &'a Scan, task: &'a Task) -> &'a Task {
    scan.by_key(&task.key).unwrap_or(task)
}

// ---------------------------------------------------------------- 쓰기

pub fn create_task(vault: &Path, request: &CreateTaskRequest, context: &CallContext) -> Result<Json, EngineError> {
    let mut fields = fields_of(
        &request.title,
        &request.task_type,
        &request.priority,
        &request.repo,
        &request.project,
        &request.base,
        &request.body,
        &request.labels,
        &request.criteria,
        &request.refs,
    )?;
    if let Some(manager) = &context.manager {
        if request.project.as_ref().is_some_and(|project| project != &manager.project) {
            return Err(EngineError::invalid_argument("매니저의 프로젝트로만 태스크를 만들 수 있다"));
        }
        check_manager_repo(vault, manager, fields.repo.as_deref())?;
        fields.project = Some(manager.project.clone());
    }
    let actor = actor_of(&request.actor)?;
    let task = store::create(vault, fields, &stamp_of(&context), &actor)?;
    let scan = scan_for_reader(vault, context)?;
    let mut m = detail(vault, &scan, refetch(&scan, &task), 0)?;
    m["created"] = json!(true);
    Ok(m)
}

pub fn update_task(vault: &Path, request: &UpdateTaskRequest, context: &CallContext) -> Result<Json, EngineError> {
    let reference = only_ref(&request.id, UPDATE_USAGE)?;
    let scan = store::scan(vault);
    let cur = store::find(&scan, reference)?;
    let fields = fields_of(
        &request.title,
        &request.task_type,
        &request.priority,
        &request.repo,
        &request.project,
        &request.base,
        &request.body,
        &request.labels,
        &request.criteria,
        &request.refs,
    )?;
    let actor = actor_of(&request.actor)?;
    let next = store::update(
        vault,
        cur,
        fields,
        request.f1.as_deref(),
        &stamp_of(&context),
        &actor,
    )?;
    let after = store::scan(vault);
    let task = next.as_ref().unwrap_or(cur);
    let mut m = detail(vault, &after, refetch(&after, task), 0)?;
    m["changed"] = json!(next.is_some());
    Ok(m)
}

pub fn transition_task(vault: &Path, request: &TransitionTaskRequest, context: &CallContext) -> Result<Json, EngineError> {
    let reference = only_ref(&request.id, TRANSITION_USAGE)?;
    let scan = store::scan(vault);
    let cur = store::find(&scan, reference)?;
    let Some(to_raw) = request.to.as_deref().filter(|t| !is_blank(t)) else {
        return Err(EngineError::invalid_argument(format!(
            "to 가 필요하다: {TRANSITION_USAGE}"
        )));
    };
    let Some(to) = TaskStatus::parse(to_raw) else {
        return Err(EngineError::invalid_argument(format!(
            "to 는 상태 이름이어야 한다(draft, ready-for-develop, in-progress, waiting-answer, in-review, done, failed, cancelled): {to_raw}"
        )));
    };
    let from = request.from.as_deref().map(|f| trim(f).to_string());
    let actor = actor_of(&request.actor)?;
    let next = store::transition(
        vault,
        cur,
        to,
        &actor,
        from.as_deref(),
        &stamp_of(&context),
    )?;
    let after = store::scan(vault);
    let mut m = detail(vault, &after, refetch(&after, &next), 0)?;
    m["from"] = json!(cur.status.label());
    Ok(m)
}

pub fn link_tasks(vault: &Path, request: &LinkTasksRequest, context: &CallContext) -> Result<Json, EngineError> {
    let (Some(from_ref), Some(to_ref)) = (request.from.as_deref(), request.to.as_deref()) else {
        return Err(EngineError::invalid_argument(format!(
            "link-tasks 는 건 쪽 키와 받는 쪽 키를 받는다: {LINK_USAGE}"
        )));
    };
    let Some(kind_raw) = request.link_type.as_deref().filter(|t| !is_blank(t)) else {
        return Err(EngineError::invalid_argument(format!(
            "type 이 필요하다(blocks, relates, duplicates): {LINK_USAGE}"
        )));
    };
    let scan = store::scan(vault);
    let from = store::find(&scan, from_ref)?;
    let to = store::find(&scan, to_ref)?;
    let remove = is_true(&request.remove);
    let kind = trim(kind_raw).to_lowercase();
    let actor = actor_of(&request.actor)?;
    let changed = store::link(
        vault,
        from,
        to,
        &kind,
        remove,
        &stamp_of(&context),
        &actor,
    )?;
    let after = store::scan(vault);
    let links = related_rows(&after, refetch(&after, from));
    Ok(json!({
        "from": from.key,
        "to": to.key,
        "type": kind,
        "removed": remove,
        "changed": changed.is_some(),
        "links": links,
    }))
}

pub fn add_task_comment(
    vault: &Path,
    request: &AddTaskCommentRequest,
    context: &CallContext,
) -> Result<Json, EngineError> {
    let reference = only_ref(&request.id, COMMENT_USAGE)?;
    let scan = scan_for_reader(vault, context)?;
    let task = store::find(&scan, reference)?;
    if let Some(manager) = &context.manager {
        check_manager_repo(vault, manager, task.repo.as_deref())?;
    }
    let body = format::comment(request.body.as_deref().unwrap_or(""))?;
    let actor = actor_of(&request.actor)?;
    let ev = store::comment(vault, task, &body, &stamp_of(&context), &actor)?;
    Ok(json!({"key": task.key, "comment": ev}))
}

pub fn delete_task(vault: &Path, request: &TaskRefRequest) -> Result<Json, EngineError> {
    let reference = only_ref(&request.id, DELETE_USAGE)?;
    let scan = store::scan(vault);
    let task = store::find(&scan, reference)?;
    let blocking: Vec<&str> = scan
        .items
        .iter()
        .filter(|o| {
            o.links
                .iter()
                .any(|l| l.kind == TaskLinkKind::Blocks && l.to == task.key)
        })
        .map(|o| o.key.as_str())
        .collect();
    let blocked: Vec<&str> = task
        .links
        .iter()
        .filter(|l| l.kind == TaskLinkKind::Blocks)
        .map(|l| l.to.as_str())
        .collect();
    store::delete(vault, task)?;
    // 지운 태스크가 막고 있던 태스크는 이제 막히지 않는다. 되살리면 다시 막는다
    Ok(json!({
        "key": task.key,
        "id": task.id,
        "title": task.title,
        "trashed": true,
        "wasBlocking": blocked,
        "wasBlockedBy": blocking,
    }))
}

pub fn restore_task(vault: &Path, request: &TaskRefRequest) -> Result<Json, EngineError> {
    let reference = only_ref(&request.id, RESTORE_USAGE)?;
    let task = store::restore(vault, reference)?;
    let scan = store::scan(vault);
    let mut m = detail(vault, &scan, refetch(&scan, &task), 0)?;
    m["restored"] = json!(true);
    Ok(m)
}

// ---------------------------------------------------------------- 읽기

pub(super) fn project_task_count(vault: &Path, slug: &str) -> usize {
    store::scan(vault)
        .items
        .into_iter()
        .filter(|task| task.project.as_deref() == Some(slug))
        .count()
}

fn scan_for_reader(vault: &Path, context: &CallContext) -> Result<Scan, EngineError> {
    let mut scan = store::scan(vault);
    if let Some(manager) = &context.manager {
        check_manager_project(vault, manager)?;
        scan.items.retain(|task| task.project.as_deref() == Some(&manager.project));
        scan.skipped.clear();
    }
    Ok(scan)
}

pub(super) fn check_manager_repo(vault: &Path, manager: &ManagerContext, repo: Option<&str>) -> Result<(), EngineError> {
    let project = check_manager_project(vault, manager)?;
    if repo.is_some_and(|repo| {
        manager.repos.iter().any(|allowed| allowed == repo)
            && project.repos.iter().any(|allowed| allowed.github.as_deref() == Some(repo))
    }) {
        return Ok(());
    }
    Err(EngineError::invalid_argument("매니저에게 허용하지 않은 프로젝트 저장소다"))
}

pub fn list_tasks(vault: &Path, request: &ListTasksRequest, context: &CallContext) -> Result<Json, EngineError> {
    check_workspace_marker(vault)?;
    if is_true(&request.trash) {
        if context.manager.is_some() {
            return Err(EngineError::invalid_argument("매니저에게 태스크 휴지통을 열지 않는다"));
        }
        let rows: Vec<Json> = store::trash(vault)
            .into_iter()
            .map(|e| json!({"key": e.key, "id": e.id, "title": e.title, "path": e.rel_path, "deletedAt": e.deleted_at}))
            .collect();
        return Ok(json!({"trash": rows}));
    }
    let status = match given(&request.status) {
        Some(raw) => Some(TaskStatus::parse(raw).ok_or_else(|| {
            EngineError::invalid_argument(format!("status 는 상태 이름이어야 한다: {raw}"))
        })?),
        None => None,
    };
    let task_type = given(&request.task_type)
        .map(format::task_type)
        .transpose()?;
    let repo = match given(&request.repo) {
        Some(raw) => format::repo(raw)?,
        None => None,
    };
    let label = request.label.as_deref().map(|l| trim(l).to_string());
    let q = request.q.as_deref().map(|q| trim(q).to_lowercase());
    let only_blocked = is_true(&request.blocked);
    let bodies = is_true(&request.bodies);

    let scan = scan_for_reader(vault, context)?;
    let mut items: Vec<&Task> = scan
        .items
        .iter()
        .filter(|t| status.is_none_or(|s| t.status == s))
        .filter(|t| task_type.as_ref().is_none_or(|ty| &t.task_type == ty))
        .filter(|t| repo.as_ref().is_none_or(|r| t.repo.as_ref() == Some(r)))
        .filter(|t| match &label {
            Some(l) if !l.is_empty() => t.labels.contains(l),
            _ => true,
        })
        .filter(|t| match &q {
            Some(q) if !q.is_empty() => {
                t.title.to_lowercase().contains(q.as_str())
                    || t.key.to_lowercase().contains(q.as_str())
            }
            _ => true,
        })
        .filter(|t| !only_blocked || !aggregate::unfinished_blockers(&scan, &t.key).is_empty())
        .collect();
    items.sort_by_key(|t| Reverse(t.number()));
    let rows: Vec<Json> = items.iter().map(|t| row(&scan, t, bodies)).collect();
    let mut out = json!({"tasks": rows});
    if !scan.skipped.is_empty() {
        out["skipped"] = json!(scan.skipped);
    }
    Ok(out)
}

fn activity_limit(raw: &Option<Json>) -> Result<usize, EngineError> {
    let bad = |text: String| {
        EngineError::invalid_argument(format!("activity 는 0 이상의 정수여야 한다: {text}"))
    };
    match raw {
        None | Some(Json::Null) => Ok(DEFAULT_ACTIVITY),
        Some(Json::String(text)) => {
            if is_blank(text) {
                return Ok(DEFAULT_ACTIVITY);
            }
            let n: i64 = trim(text).parse().map_err(|_| bad(text.clone()))?;
            if n < 0 || n > i64::from(i32::MAX) {
                return Err(bad(text.clone()));
            }
            Ok(n as usize)
        }
        Some(Json::Number(n)) => match n.as_i64() {
            Some(v) if (0..=i64::from(i32::MAX)).contains(&v) => Ok(v as usize),
            _ => Err(bad(n.to_string())),
        },
        Some(other) => Err(bad(other.to_string())),
    }
}

pub fn get_task(vault: &Path, request: &GetTaskRequest, context: &CallContext) -> Result<Json, EngineError> {
    check_workspace_marker(vault)?;
    let reference = only_ref(&request.id, GET_USAGE)?;
    let scan = scan_for_reader(vault, context)?;
    let task = store::find(&scan, reference)?;
    let limit = activity_limit(&request.activity)?;
    detail(vault, &scan, refetch(&scan, task), limit)
}
