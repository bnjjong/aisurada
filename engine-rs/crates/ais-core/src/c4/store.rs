//! C4 모델 파일을 vault에서 읽고 쓰는 곳.
//!
//! ```text
//! .aisurada/c4/<프로젝트 slug>/model.yaml                      main 아키텍처의 요소와 관계
//! .aisurada/c4/<프로젝트 slug>/views.yaml                      main 아키텍처의 뷰별 요소 위치
//! .aisurada/c4/<프로젝트 slug>/diagram.yaml                    main의 이름(없으면 「기본」)
//! .aisurada/c4/<프로젝트 slug>/diagrams/<id>/{model,views,diagram}.yaml   새로 만든 아키텍처
//! ```
//!
//! 프로젝트는 아키텍처를 여러 개 가질 수 있다. 기존 파일이 `main`이라 옮길 것이 없고, 옛 앱과 다른 세션이 읽어도 깨지지 않는다.
//! 이름 파일(`diagram.yaml`)은 확인 값에 넣지 않아서, 이름을 바꿔도 모델을 읽고 있던 쪽이 낡았다고 거절되지 않는다.
//!
//! 읽은 뒤 바뀌었는지 가리는 확인 값(`f1:`)은 두 파일의 바이트를 이어서 계산한다(없는 파일은 빈 바이트). 그래서 아직 파일이
//! 없는 프로젝트도 확인 값이 있고, 어느 한 파일만 바뀌어도 값이 달라진다. 두 파일은 모델, 뷰 차례로 쓴다. 쓰기 범위가
//! 잠금을 쥐고 있어 다른 쓰기와 섞이지 않는다.

use std::path::{Path, PathBuf};

use super::diagram::{self, MAIN, MAIN_NAME, MAX_DIAGRAMS};
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
const NAME_FILE: &str = "diagram.yaml";
const DIAGRAMS_DIR: &str = "diagrams";

/// 아키텍처 하나의 이름표
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagramInfo {
    pub id: String,
    pub name: String,
    /// 요소와 관계 개수. 파일이 깨졌으면 `None`
    pub counts: Option<(usize, usize)>,
}

impl DiagramInfo {
    pub fn to_json(&self) -> serde_json::Value {
        let mut out = serde_json::json!({ "id": self.id, "name": self.name });
        if let Some((elements, relationships)) = self.counts {
            out["elements"] = serde_json::json!(elements);
            out["relationships"] = serde_json::json!(relationships);
        }
        out
    }
}

/// 파일에서 읽은 한 아키텍처의 모델
pub struct Stored {
    pub info: DiagramInfo,
    pub model: Model,
    pub views: Views,
    /// 두 파일 바이트의 확인 값
    pub token: String,
    model_bytes: Vec<u8>,
    views_bytes: Vec<u8>,
}

fn project_dir(vault: &Path, project: &str) -> PathBuf {
    vault.join(ROOT).join(project)
}

/// 아키텍처의 폴더. `main`은 프로젝트 폴더 자체이고, 나머지는 그 아래 `diagrams/<id>/`다.
pub(super) fn dir(vault: &Path, project: &str, diagram: &str) -> PathBuf {
    if diagram == MAIN {
        project_dir(vault, project)
    } else {
        project_dir(vault, project).join(DIAGRAMS_DIR).join(diagram)
    }
}

/// 요청에서 온 아키텍처 id를 정한다. 안 주면 `main`이다.
pub fn diagram_of(raw: Option<&str>) -> Result<&str, EngineError> {
    match raw.map(str::trim).filter(|id| !id.is_empty()) {
        None => Ok(MAIN),
        Some(id) => diagram::check_id(id).map(|()| id),
    }
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

pub(super) fn read_bytes(file: &Path) -> Result<Vec<u8>, EngineError> {
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

pub(super) fn utf8(bytes: &[u8], file: &str) -> Result<String, EngineError> {
    String::from_utf8(bytes.to_vec())
        .map_err(|_| EngineError::invalid_argument(format!("{file}이 UTF-8 글이 아니다")))
}

fn read_name(folder: &Path) -> Result<Option<String>, EngineError> {
    let bytes = read_bytes(&folder.join(NAME_FILE))?;
    diagram::parse_name(&utf8(&bytes, NAME_FILE)?)
}

fn require_diagram(folder: &Path, diagram: &str) -> Result<(), EngineError> {
    if diagram == MAIN || fs::metadata(folder).is_ok_and(|meta| meta.is_dir()) {
        return Ok(());
    }
    Err(EngineError::not_found(format!(
        "아키텍처를 못 찾았다: {diagram}"
    )))
}

fn default_name(diagram: &str) -> String {
    if diagram == MAIN { MAIN_NAME } else { diagram }.to_string()
}

/// 프로젝트의 아키텍처 하나를 읽는다. 파일이 없으면 빈 모델이다(`main`만. 다른 아키텍처는 폴더가 있어야 한다).
/// 깨진 파일은 이유를 적은 오류로 멈춘다(덮어쓰지 않는다).
pub fn load(vault: &Path, project: &str, diagram: &str) -> Result<Stored, EngineError> {
    require_project(vault, project)?;
    let folder = dir(vault, project, diagram);
    require_diagram(&folder, diagram)?;
    let model_bytes = read_bytes(&folder.join(MODEL_FILE))?;
    let views_bytes = read_bytes(&folder.join(VIEWS_FILE))?;
    let model = parse_model(&utf8(&model_bytes, MODEL_FILE)?)?;
    let views = parse_views(&utf8(&views_bytes, VIEWS_FILE)?, &model)?;
    let name = read_name(&folder)?.unwrap_or_else(|| default_name(diagram));
    Ok(Stored {
        info: DiagramInfo {
            id: diagram.to_string(),
            name,
            counts: Some((model.elements.len(), model.relationships.len())),
        },
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
    diagram: &str,
    before: &Stored,
    model: &Model,
    views: &Views,
) -> Result<String, EngineError> {
    let folder = dir(vault, project, diagram);
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

/// 응답에 싣는 JSON(모델, 뷰, 확인 값, 어느 아키텍처인지)
pub fn to_json(
    project: &str,
    info: &DiagramInfo,
    model: &Model,
    views: &Views,
    token: &str,
) -> serde_json::Value {
    let mut out = format::model_json(model);
    out["project"] = serde_json::json!(project);
    out["diagram"] = info.to_json();
    out["views"] = format::views_json(views);
    out["precondition"] = serde_json::json!(token);
    out
}

fn info_of(vault: &Path, project: &str, id: &str) -> Result<DiagramInfo, EngineError> {
    let folder = dir(vault, project, id);
    let name = read_name(&folder)?.unwrap_or_else(|| default_name(id));
    let counts = (|| {
        let model_bytes = read_bytes(&folder.join(MODEL_FILE)).ok()?;
        let model = parse_model(&String::from_utf8(model_bytes).ok()?).ok()?;
        Some((model.elements.len(), model.relationships.len()))
    })();
    Ok(DiagramInfo {
        id: id.to_string(),
        name,
        counts,
    })
}

fn extra_ids(vault: &Path, project: &str) -> Vec<String> {
    let Ok(entries) = fs::read_dir(project_dir(vault, project).join(DIAGRAMS_DIR)) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|id| id != MAIN && diagram::check_id(id).is_ok())
        .collect()
}

/// 프로젝트의 아키텍처 목록. `main`이 맨 앞이고 파일이 없어도 늘 있다.
pub fn list(vault: &Path, project: &str) -> Result<Vec<DiagramInfo>, EngineError> {
    require_project(vault, project)?;
    let mut ids = extra_ids(vault, project);
    ids.push(MAIN.to_string());
    ids.sort_by_key(|id| diagram::order_key(id));
    ids.iter().map(|id| info_of(vault, project, id)).collect()
}

fn check_name_free(all: &[DiagramInfo], name: &str, own: Option<&str>) -> Result<(), EngineError> {
    match all
        .iter()
        .find(|info| info.name == name && Some(info.id.as_str()) != own)
    {
        Some(taken) => Err(EngineError::conflict(format!(
            "같은 이름의 아키텍처가 이미 있다: {} ({})",
            taken.name, taken.id
        ))),
        None => Ok(()),
    }
}

/// 새 아키텍처를 빈 모델로 만든다. id는 `arch-N`으로 자동으로 붙는다.
pub fn create(vault: &Path, project: &str, name: &str) -> Result<DiagramInfo, EngineError> {
    let name = diagram::clean_name(name)?;
    let all = list(vault, project)?;
    if all.len() >= MAX_DIAGRAMS {
        return Err(EngineError::invalid_argument(format!(
            "아키텍처는 프로젝트마다 {MAX_DIAGRAMS}개까지 만들 수 있다"
        )));
    }
    check_name_free(&all, &name, None)?;
    let id = diagram::next_id(all.iter().map(|info| info.id.as_str()));
    let folder = dir(vault, project, &id);
    write_atomic(&folder.join(NAME_FILE), diagram::render_name(&name)?.as_bytes())?;
    Ok(DiagramInfo {
        id,
        name,
        counts: Some((0, 0)),
    })
}

/// 아키텍처의 이름만 바꾼다. 모델과 확인 값은 그대로다.
pub fn rename(
    vault: &Path,
    project: &str,
    id: &str,
    name: &str,
) -> Result<DiagramInfo, EngineError> {
    let name = diagram::clean_name(name)?;
    let all = list(vault, project)?;
    let Some(current) = all.iter().find(|info| info.id == id) else {
        return Err(EngineError::not_found(format!(
            "아키텍처를 못 찾았다: {id}"
        )));
    };
    check_name_free(&all, &name, Some(id))?;
    let folder = dir(vault, project, id);
    write_atomic(&folder.join(NAME_FILE), diagram::render_name(&name)?.as_bytes())?;
    Ok(DiagramInfo {
        name,
        ..current.clone()
    })
}

/// 아키텍처를 지운다(이력도 함께). `main`은 모델·위치·이름 파일과 이력만 지워서 다시 빈 `main`이 된다. 되살리는 길은 vault의 git 이력뿐이다.
pub fn delete(vault: &Path, project: &str, id: &str) -> Result<(), EngineError> {
    require_project(vault, project)?;
    let folder = dir(vault, project, id);
    require_diagram(&folder, id)?;
    let failed = |_| EngineError::internal(format!("아키텍처를 지우지 못했다: {id}"));
    if id == MAIN {
        for file in [MODEL_FILE, VIEWS_FILE, NAME_FILE] {
            match fs::remove_file(folder.join(file)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(failed(e)),
            }
        }
        return match fs::remove_dir_all(folder.join(super::history_store::DIR)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(failed(e)),
            _ => Ok(()),
        };
    }
    fs::remove_dir_all(&folder).map_err(failed)
}
