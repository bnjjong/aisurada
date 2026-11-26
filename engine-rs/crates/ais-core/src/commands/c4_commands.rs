//! C4 아키텍처 모델 명령 7개: `get_c4_model`, `apply_c4_changes`, `list_c4_diagrams`, `create_c4_diagram`,
//! `rename_c4_diagram`, `delete_c4_diagram`, `list_c4_history`.
//! 외부 설명서에 없는 확장이다. 화면과 AI 에이전트(MCP)가 같은 명령으로 모델을 읽고 고친다.
//!
//! 프로젝트는 아키텍처(다이어그램)를 여러 개 가질 수 있다. 읽기와 쓰기 명령은 선택 인자 `diagram`으로 어느 것인지 고르고,
//! 안 주면 기존 파일이 가리키는 `main`이다. 만들기, 이름 바꾸기, 지우기는 위 목록 명령들로 한다.
//!
//! 변경이 적용될 때마다 이력(`c4::history_store`)에 점이 쌓이고, `list_c4_history`로 점 목록을, `get_c4_model`의 `at`으로
//! 그 시점의 모델을 읽는다(타임 트래블, 읽기 전용).
//!
//! 고치려면 먼저 `get_c4_model`로 읽고, 응답의 `precondition`을 `apply_c4_changes`의 `f1`에 그대로 싣는다. 그 사이 파일이
//! 바뀌었으면 아무것도 쓰지 않고 거절한다. 변경 목록(`operations`)은 한 번에 적용하거나 전혀 적용하지 않는다(`c4::apply`).

use std::path::Path;

use serde::Deserialize;
use serde_json::{Value as Json, json};

use crate::c4::apply::{apply, parse_operations};
use crate::c4::{history_store, store};
use crate::error::EngineError;
use crate::kernel::actor::CallContext;
use crate::kernel::token::PREFIX;

const GET_USAGE: &str = "get-c4-model <프로젝트 slug>";
const APPLY_USAGE: &str = "apply-c4-changes <프로젝트 slug> f1=<직전에 읽은 토큰> operations=<변경 JSON 배열>";
const LIST_USAGE: &str = "list-c4-diagrams <프로젝트 slug>";
const CREATE_USAGE: &str = "create-c4-diagram <프로젝트 slug> name=<이름>";
const RENAME_USAGE: &str = "rename-c4-diagram <프로젝트 slug> diagram=<아키텍처 id> name=<새 이름>";
const HISTORY_USAGE: &str = "list-c4-history <프로젝트 slug> [diagram=<아키텍처 id>]";
const DELETE_USAGE: &str = "delete-c4-diagram <프로젝트 slug> diagram=<아키텍처 id>";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GetC4ModelRequest {
    pub project: Option<String>,
    /// 아키텍처 id. 안 주면 `main`
    pub diagram: Option<String>,
    /// 이력의 점 번호. 주면 그 시점의 모델을 읽기 전용으로 낸다(호출 객체에서는 숫자, 명령줄에서는 글)
    pub at: Option<Json>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ListC4HistoryRequest {
    pub project: Option<String>,
    /// 아키텍처 id. 안 주면 `main`
    pub diagram: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ApplyC4ChangesRequest {
    pub project: Option<String>,
    /// 아키텍처 id. 안 주면 `main`
    pub diagram: Option<String>,
    pub f1: Option<String>,
    /// 호출 객체에서는 배열, 명령줄에서는 JSON 글
    pub operations: Option<Json>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ListC4DiagramsRequest {
    pub project: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CreateC4DiagramRequest {
    pub project: Option<String>,
    pub name: Option<String>,
}

/// 이름 바꾸기와 지우기가 같이 쓴다. 지울 때는 `name`을 안 받는다.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct C4DiagramRequest {
    pub project: Option<String>,
    pub diagram: Option<String>,
    pub name: Option<String>,
}

fn project_of<'a>(raw: &'a Option<String>, usage: &str) -> Result<&'a str, EngineError> {
    raw.as_deref()
        .filter(|slug| !slug.trim().is_empty())
        .ok_or_else(|| EngineError::invalid_argument(format!("프로젝트 slug 가 필요하다: {usage}")))
}

pub fn get_c4_model(vault: &Path, request: &GetC4ModelRequest) -> Result<Json, EngineError> {
    let project = project_of(&request.project, GET_USAGE)?;
    let diagram = store::diagram_of(request.diagram.as_deref())?;
    let stored = store::load(vault, project, diagram)?;
    if let Some(at) = &request.at {
        return past_model(vault, project, &stored, at);
    }
    Ok(store::to_json(project, &stored.info, &stored.model, &stored.views, &stored.token))
}

/// 이력의 한 시점을 읽기 전용으로 낸다. 쓰기에 쓸 `precondition`은 싣지 않는다(과거에는 쓸 수 없다).
fn past_model(vault: &Path, project: &str, stored: &store::Stored, at: &Json) -> Result<Json, EngineError> {
    let seq = match at {
        Json::Number(number) => number.as_u64(),
        Json::String(text) => text.trim().parse::<u64>().ok(),
        _ => None,
    }
    .and_then(|n| u32::try_from(n).ok())
    .filter(|n| *n >= 1)
    .ok_or_else(|| EngineError::invalid_argument("at 은 1 이상의 점 번호여야 한다"))?;
    let snapshot = history_store::get(vault, project, &stored.info.id, seq)?;
    let mut out = store::to_json(project, &stored.info, &snapshot.model, &snapshot.views, "");
    out.as_object_mut().map(|map| map.remove("precondition"));
    out["readOnly"] = json!(true);
    out["snapshot"] = snapshot.row_json();
    Ok(out)
}

pub fn apply_c4_changes(
    vault: &Path,
    request: &ApplyC4ChangesRequest,
    context: &CallContext,
) -> Result<Json, EngineError> {
    let project = project_of(&request.project, APPLY_USAGE)?;
    let operations = request
        .operations
        .as_ref()
        .ok_or_else(|| EngineError::invalid_argument(format!("operations 가 필요하다: {APPLY_USAGE}")))?;
    let operations = parse_operations(operations)?;
    let diagram = store::diagram_of(request.diagram.as_deref())?;
    let stored = store::load(vault, project, diagram)?;
    check_token(request.f1.as_deref(), &stored.token, project)?;

    let outcome = apply(&stored.model, &stored.views, &operations)?;
    let token = store::save(vault, project, diagram, &stored, &outcome.model, &outcome.views)?;
    let changed = token != stored.token;
    if changed {
        // 이력은 덤이다. 못 쓰더라도 이미 저장한 변경을 되돌리거나 실패로 알리지 않는다.
        if let Err(error) = history_store::record(
            vault,
            project,
            diagram,
            &stored,
            &outcome.model,
            &outcome.views,
            context.caller.as_str(),
            crate::write::apparatus::now_millis(),
        ) {
            eprintln!("경고: C4 이력을 남기지 못했다: {error}");
        }
    }
    let mut out = store::to_json(project, &stored.info, &outcome.model, &outcome.views, &token);
    out["changed"] = json!(changed);
    out["applied"] = json!(operations.len());
    out["createdRelationships"] = json!(outcome.created_relationships);
    out["removedElements"] = json!(outcome.removed_elements);
    out["removedRelationships"] = json!(outcome.removed_relationships);
    Ok(out)
}

/// 이력의 점 목록(오래된 것부터). `latestIsCurrent`가 거짓이면 마지막 점 뒤에 이력에 없는 변경(손으로 고침 등)이 있다.
pub fn list_c4_history(vault: &Path, request: &ListC4HistoryRequest) -> Result<Json, EngineError> {
    let project = project_of(&request.project, HISTORY_USAGE)?;
    let diagram = store::diagram_of(request.diagram.as_deref())?;
    let stored = store::load(vault, project, diagram)?;
    let snapshots = history_store::list(vault, project, diagram)?;
    Ok(json!({
        "project": project,
        "diagram": stored.info.to_json(),
        "snapshots": snapshots.iter().map(|snapshot| snapshot.row_json()).collect::<Vec<_>>(),
        "latestIsCurrent": history_store::latest_is_current(&snapshots, &stored),
    }))
}

pub fn list_c4_diagrams(vault: &Path, request: &ListC4DiagramsRequest) -> Result<Json, EngineError> {
    let project = project_of(&request.project, LIST_USAGE)?;
    let diagrams = store::list(vault, project)?;
    Ok(json!({
        "project": project,
        "diagrams": diagrams.iter().map(store::DiagramInfo::to_json).collect::<Vec<_>>(),
    }))
}

pub fn create_c4_diagram(vault: &Path, request: &CreateC4DiagramRequest) -> Result<Json, EngineError> {
    let project = project_of(&request.project, CREATE_USAGE)?;
    let name = request
        .name
        .as_deref()
        .ok_or_else(|| EngineError::invalid_argument(format!("name 이 필요하다: {CREATE_USAGE}")))?;
    let created = store::create(vault, project, name)?;
    Ok(json!({ "project": project, "diagram": created.to_json() }))
}

pub fn rename_c4_diagram(vault: &Path, request: &C4DiagramRequest) -> Result<Json, EngineError> {
    let project = project_of(&request.project, RENAME_USAGE)?;
    let diagram = diagram_required(request, RENAME_USAGE)?;
    let name = request
        .name
        .as_deref()
        .ok_or_else(|| EngineError::invalid_argument(format!("name 이 필요하다: {RENAME_USAGE}")))?;
    let renamed = store::rename(vault, project, diagram, name)?;
    Ok(json!({ "project": project, "diagram": renamed.to_json() }))
}

pub fn delete_c4_diagram(vault: &Path, request: &C4DiagramRequest) -> Result<Json, EngineError> {
    let project = project_of(&request.project, DELETE_USAGE)?;
    let diagram = diagram_required(request, DELETE_USAGE)?;
    store::delete(vault, project, diagram)?;
    Ok(json!({ "project": project, "deleted": diagram }))
}

/// 지우기와 이름 바꾸기는 어느 것인지 꼭 적어야 한다(안 적었다고 `main`으로 보지 않는다).
fn diagram_required<'a>(request: &'a C4DiagramRequest, usage: &str) -> Result<&'a str, EngineError> {
    match request.diagram.as_deref().map(str::trim).filter(|id| !id.is_empty()) {
        Some(id) => store::diagram_of(Some(id)),
        None => Err(EngineError::invalid_argument(format!(
            "diagram 이 필요하다: {usage}"
        ))),
    }
}

fn check_token(provided: Option<&str>, current: &str, project: &str) -> Result<(), EngineError> {
    let Some(token) = provided.filter(|t| !t.is_empty()) else {
        return Err(EngineError::precondition_failed(format!(
            "선결 조건 토큰이 안 왔어요. 아무것도 쓰지 않았습니다: {project}\nget_c4_model 로 모델을 읽고, 그 응답의 precondition 을 f1 인자에 그대로 실어 다시 부릅니다.\n방금 apply_c4_changes 로 고쳤다면 그 응답이 이미 다음 토큰을 줍니다."
        )));
    };
    if !token.starts_with(PREFIX) {
        return Err(EngineError::precondition_failed(format!(
            "선결 조건 토큰의 모양이 아니에요. 아무것도 쓰지 않았습니다: {project}\nget_c4_model 응답의 precondition 을 글자 그대로 f1 인자에 되돌립니다. 계산해서 만드는 값이 아닙니다."
        )));
    }
    if token != current {
        return Err(EngineError::precondition_failed(format!(
            "다른 쪽이 이 프로젝트의 C4 모델을 먼저 고쳤어요. 아무것도 쓰지 않았습니다: {project}\nget_c4_model 로 다시 읽고, 그 새 모델 위에 자기 변경을 다시 얹은 뒤, 그 응답의 precondition 을 f1 로 실어 보냅니다."
        )));
    }
    Ok(())
}
