//! C4 모델 파일을 vault에서 읽고 쓰는 곳.
//!
//! ```text
//! .aisurada/c4/<프로젝트 slug>/model.yaml   요소와 관계
//! .aisurada/c4/<프로젝트 slug>/views.yaml   뷰별 요소 위치
//! ```
//!
//! 읽은 뒤 바뀌었는지 가리는 확인 값(`f1:`)은 두 파일의 바이트를 이어서 계산한다(없는 파일은 빈 바이트). 그래서 아직 파일이
//! 없는 프로젝트도 확인 값이 있고, 어느 한 파일만 바뀌어도 값이 달라진다. 두 파일은 모델, 뷰 차례로 쓴다. 쓰기 범위가
//! 잠금을 쥐고 있어 다른 쓰기와 섞이지 않는다.

use std::path::{Path, PathBuf};

use super::format::{self, parse_model, parse_views, render_model, render_views};
use super::{MAX_FILE_BYTES, Model, Views};
use crate::error::EngineError;
use crate::kernel::token::compute_token;
use crate::ports::fs;
use crate::vault::project::scan_projects;
use crate::write::atomic::write_atomic;

pub const ROOT: &str = ".aisurada/c4";
const MODEL_FILE: &str = "model.yaml";
const VIEWS_FILE: &str = "views.yaml";

/// 파일에서 읽은 한 프로젝트의 모델
pub struct Stored {
    pub model: Model,
    pub views: Views,
    /// 두 파일 바이트의 확인 값
    pub token: String,
    model_bytes: Vec<u8>,
    views_bytes: Vec<u8>,
}

fn dir(vault: &Path, project: &str) -> PathBuf {
    vault.join(ROOT).join(project)
}

/// 프로젝트가 있는지 확인한다. 요청의 slug가 실제 프로젝트의 slug와 같을 때만 폴더 이름으로 쓴다(경로 장난을 막는다).
pub fn require_project(vault: &Path, slug: &str) -> Result<(), EngineError> {
    if slug.trim().is_empty() {
        return Err(EngineError::invalid_argument("프로젝트 slug 가 필요하다"));
    }
    if scan_projects(vault).iter().any(|p| p.slug == slug) {
        return Ok(());
    }
    Err(EngineError::not_found(format!(
        "프로젝트를 못 찾았다: {slug}"
    )))
}

fn read_bytes(file: &Path) -> Result<Vec<u8>, EngineError> {
    match fs::metadata(file) {
        Err(_) => Ok(Vec::new()),
        Ok(meta) if !meta.is_file() => Err(EngineError::internal(format!(
            "C4 모델 자리에 파일이 아닌 것이 있다: {}",
            file.display()
        ))),
        Ok(meta) if meta.len() > MAX_FILE_BYTES => Err(EngineError::invalid_argument(format!(
            "C4 모델 파일이 너무 크다({MAX_FILE_BYTES}바이트 초과): {}",
            file.display()
        ))),
        Ok(_) => fs::read(file).map_err(|_| {
            EngineError::internal(format!("C4 모델 파일을 못 읽었다: {}", file.display()))
        }),
    }
}

fn token_of(model: &[u8], views: &[u8]) -> String {
    let mut joined = Vec::with_capacity(model.len() + views.len() + 1);
    joined.extend_from_slice(model);
    joined.push(0);
    joined.extend_from_slice(views);
    compute_token(&joined)
}

fn utf8(bytes: &[u8], file: &str) -> Result<String, EngineError> {
    String::from_utf8(bytes.to_vec())
        .map_err(|_| EngineError::invalid_argument(format!("{file}이 UTF-8 글이 아니다")))
}

/// 프로젝트의 모델을 읽는다. 파일이 없으면 빈 모델이다. 깨진 파일은 이유를 적은 오류로 멈춘다(덮어쓰지 않는다).
pub fn load(vault: &Path, project: &str) -> Result<Stored, EngineError> {
    require_project(vault, project)?;
    let folder = dir(vault, project);
    let model_bytes = read_bytes(&folder.join(MODEL_FILE))?;
    let views_bytes = read_bytes(&folder.join(VIEWS_FILE))?;
    let model = parse_model(&utf8(&model_bytes, MODEL_FILE)?)?;
    let views = parse_views(&utf8(&views_bytes, VIEWS_FILE)?, &model)?;
    Ok(Stored {
        token: token_of(&model_bytes, &views_bytes),
        model,
        views,
        model_bytes,
        views_bytes,
    })
}

/// 바뀐 파일만 쓰고 새 확인 값을 돌려준다. 내용이 같으면(손으로 쓴 글 모양이 달라도) 쓰지 않고 지금 값을 돌려준다.
pub fn save(
    vault: &Path,
    project: &str,
    before: &Stored,
    model: &Model,
    views: &Views,
) -> Result<String, EngineError> {
    let folder = dir(vault, project);
    let model_bytes = if *model != before.model {
        let bytes = render_model(model)?.into_bytes();
        write_atomic(&folder.join(MODEL_FILE), &bytes)?;
        bytes
    } else {
        before.model_bytes.clone()
    };
    let views_bytes = if *views != before.views {
        let bytes = render_views(views)?.into_bytes();
        write_atomic(&folder.join(VIEWS_FILE), &bytes)?;
        bytes
    } else {
        before.views_bytes.clone()
    };
    Ok(token_of(&model_bytes, &views_bytes))
}

/// 응답에 싣는 JSON(모델, 뷰, 확인 값)
pub fn to_json(project: &str, model: &Model, views: &Views, token: &str) -> serde_json::Value {
    let mut out = format::model_json(model);
    out["project"] = serde_json::json!(project);
    out["views"] = format::views_json(views);
    out["precondition"] = serde_json::json!(token);
    out
}
