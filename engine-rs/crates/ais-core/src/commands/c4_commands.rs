//! C4 아키텍처 모델 명령 2개: `get_c4_model`, `apply_c4_changes`.
//! 외부 설명서에 없는 확장이다. 화면, 챗, AI 에이전트(MCP)가 같은 두 명령으로 모델을 읽고 고친다.
//!
//! 고치려면 먼저 `get_c4_model`로 읽고, 응답의 `precondition`을 `apply_c4_changes`의 `f1`에 그대로 싣는다. 그 사이 파일이
//! 바뀌었으면 아무것도 쓰지 않고 거절한다. 변경 목록(`operations`)은 한 번에 적용하거나 전혀 적용하지 않는다(`c4::apply`).

use std::path::Path;

use serde::Deserialize;
use serde_json::{Value as Json, json};

use crate::c4::apply::{apply, parse_operations};
use crate::c4::store;
use crate::error::EngineError;
use crate::kernel::token::PREFIX;

const GET_USAGE: &str = "get-c4-model <프로젝트 slug>";
const APPLY_USAGE: &str = "apply-c4-changes <프로젝트 slug> f1=<직전에 읽은 토큰> operations=<변경 JSON 배열>";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GetC4ModelRequest {
    pub project: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ApplyC4ChangesRequest {
    pub project: Option<String>,
    pub f1: Option<String>,
    /// 호출 객체에서는 배열, 명령줄에서는 JSON 글
    pub operations: Option<Json>,
}

fn project_of<'a>(raw: &'a Option<String>, usage: &str) -> Result<&'a str, EngineError> {
    raw.as_deref()
        .filter(|slug| !slug.trim().is_empty())
        .ok_or_else(|| EngineError::invalid_argument(format!("프로젝트 slug 가 필요하다: {usage}")))
}

pub fn get_c4_model(vault: &Path, request: &GetC4ModelRequest) -> Result<Json, EngineError> {
    let project = project_of(&request.project, GET_USAGE)?;
    let stored = store::load(vault, project)?;
    Ok(store::to_json(project, &stored.model, &stored.views, &stored.token))
}

pub fn apply_c4_changes(vault: &Path, request: &ApplyC4ChangesRequest) -> Result<Json, EngineError> {
    let project = project_of(&request.project, APPLY_USAGE)?;
    let operations = request
        .operations
        .as_ref()
        .ok_or_else(|| EngineError::invalid_argument(format!("operations 가 필요하다: {APPLY_USAGE}")))?;
    let operations = parse_operations(operations)?;
    let stored = store::load(vault, project)?;
    check_token(request.f1.as_deref(), &stored.token, project)?;

    let outcome = apply(&stored.model, &stored.views, &operations)?;
    let token = store::save(vault, project, &stored, &outcome.model, &outcome.views)?;
    let changed = token != stored.token;
    let mut out = store::to_json(project, &outcome.model, &outcome.views, &token);
    out["changed"] = json!(changed);
    out["applied"] = json!(operations.len());
    out["createdRelationships"] = json!(outcome.created_relationships);
    out["removedElements"] = json!(outcome.removed_elements);
    out["removedRelationships"] = json!(outcome.removed_relationships);
    Ok(out)
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
