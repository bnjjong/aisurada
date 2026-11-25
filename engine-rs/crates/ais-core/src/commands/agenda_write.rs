//! 아젠다 쓰기: `create_agenda`, `update_agenda`, `delete_agenda`. 인자 이름과 응답 필드는 Java 엔진과 같다.

use crate::ports::fs;
use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::EngineError;
use crate::kernel::hue::is_hue;
use crate::model::AgendaRow;
use crate::text::{encode_uri_component, is_blank, trim};
use crate::vault::agenda::{AgendaInfo, is_valid_agenda_id, is_valid_slug_segment};
use crate::vault::project::scan_projects;
use crate::write::agenda_file;
use crate::write::apparatus::{format_utc_millis, next_id, now_millis};
use crate::write::atomic::write_atomic;
use crate::write::pipeline::WriteScope;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CreateAgendaRequest {
    pub title: Option<String>,
    pub hue: Option<String>,
    pub repo: Option<String>,
    pub project: Option<String>,
    pub project_repo: Option<String>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UpdateAgendaRequest {
    pub id: String,
    pub title: Option<String>,
    pub hue: Option<String>,
    pub repo: Option<String>,
    pub project: Option<String>,
    pub project_repo: Option<String>,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DeleteAgendaRequest {
    pub id: String,
}

/// 만들거나 고친 아젠다: 목록의 한 줄에 딥링크를 더한 것
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgendaWriteResponse {
    #[serde(flatten)]
    pub row: AgendaRow,
    pub deep_link: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteAgendaResponse {
    pub id: String,
    pub slug: String,
    pub archived_path: String,
    pub notes_dir: String,
}

fn response(info: &AgendaInfo, vault: &Path) -> AgendaWriteResponse {
    AgendaWriteResponse {
        row: AgendaRow::from(info),
        deep_link: format!(
            "aisurada://agenda/{}?ws={}",
            encode_uri_component(&info.slug),
            encode_uri_component(&vault.to_string_lossy())
        ),
    }
}

/// 색 인자: 비면 `None`, 목록에 없으면 400
pub(crate) fn hue_arg(raw: Option<&str>) -> Result<Option<String>, EngineError> {
    let hue = trim(raw.unwrap_or(""));
    if hue.is_empty() {
        return Ok(None);
    }
    if !is_hue(hue) {
        return Err(EngineError::invalid_argument(format!(
            "hue 값이 아니다: {hue}"
        )));
    }
    Ok(Some(hue.to_string()))
}

/// 프로젝트 slug 인자를 다듬는다. 비면 `None`이다.
fn project_arg(raw: &str, message: &str) -> Result<Option<String>, EngineError> {
    let project = trim(raw);
    if project.is_empty() {
        return Ok(None);
    }
    if !is_valid_slug_segment(project) {
        return Err(EngineError::invalid_argument(format!("{message}{project}")));
    }
    Ok(Some(project.to_string()))
}

/// `projectRepo` 인자: 비면 `None`. 값이 있으면 프로젝트가 있고 그 프로젝트에 그 이름의 저장소가 있어야 한다.
fn project_repo_arg(
    vault: &Path,
    project: Option<&str>,
    raw: Option<&str>,
) -> Result<Option<String>, EngineError> {
    let Some(raw) = raw else { return Ok(None) };
    let name = trim(raw);
    if name.is_empty() {
        return Ok(None);
    }
    let Some(project) = project else {
        return Err(EngineError::invalid_argument(format!(
            "projectRepo 는 project 와 함께 줘야 한다: {name}"
        )));
    };
    let projects = scan_projects(vault);
    let Some(found) = projects.iter().find(|p| p.slug == project) else {
        return Err(EngineError::invalid_argument(format!(
            "프로젝트를 찾지 못했다: {project}"
        )));
    };
    if !found.repos.iter().any(|r| r.name == name) {
        let names: Vec<&str> = found.repos.iter().map(|r| r.name.as_str()).collect();
        return Err(EngineError::invalid_argument(format!(
            "프로젝트 {project} 에 저장소 {name} 이 없다. 있는 이름: [{}]",
            names.join(", ")
        )));
    }
    Ok(Some(name.to_string()))
}

pub(crate) fn require_entity_id(
    id: &str,
    usage: &str,
    validator: impl Fn(&str) -> bool,
    kind_name: &str,
) -> Result<(), EngineError> {
    if is_blank(id) {
        return Err(EngineError::invalid_argument(format!(
            "인자가 하나 필요하다: {usage}"
        )));
    }
    if !validator(id) {
        return Err(EngineError::invalid_argument(format!(
            "{kind_name} id 모양이 아니다: {id}"
        )));
    }
    Ok(())
}

pub(crate) fn require_id(id: &str, usage: &str) -> Result<(), EngineError> {
    require_entity_id(id, usage, is_valid_agenda_id, "아젠다")
}

pub(crate) fn find<'a>(scope: &'a WriteScope, id: &str) -> Result<&'a AgendaInfo, EngineError> {
    scope
        .index
        .agendas
        .table
        .by_id(id)
        .ok_or_else(|| EngineError::not_found(format!("Agenda not found: {id}")))
}

pub fn create(
    vault: &Path,
    request: &CreateAgendaRequest,
    scope: &WriteScope,
) -> Result<AgendaWriteResponse, EngineError> {
    let title = trim(request.title.as_deref().unwrap_or(""));
    if title.is_empty() {
        return Err(EngineError::invalid_argument("title 이 필요하다"));
    }
    let hue = hue_arg(request.hue.as_deref())?;
    let project = project_arg(
        request.project.as_deref().unwrap_or(""),
        "project 값이 안전한 이름이 아니다: ",
    )?;
    let repo = Some(trim(request.repo.as_deref().unwrap_or("")))
        .filter(|r| !r.is_empty())
        .map(str::to_string);
    let project_repo =
        project_repo_arg(vault, project.as_deref(), request.project_repo.as_deref())?;
    let summary = request.summary.clone().unwrap_or_default();

    let rows: Vec<&AgendaInfo> = scope.index.agendas.table.infos().collect();
    let id = next_id("ag");
    let taken: HashSet<String> = rows.iter().map(|r| r.slug.clone()).collect();
    let slug = agenda_file::allocate_entity_slug("아젠다가", title, &taken)?;
    let hue = hue.unwrap_or_else(|| {
        agenda_file::least_used_hue(rows.iter().map(|r| r.hue.as_str())).to_string()
    });
    let at = format_utc_millis(now_millis());
    let info = AgendaInfo {
        id,
        created_at: Some(at.clone()),
        edited_at: Some(at.clone()),
        slug,
        title: title.to_string(),
        hue,
        repo,
        project,
        project_repo,
        summary,
    };
    write_atomic(
        &vault.join(agenda_file::rel_path(&info.slug)),
        &agenda_file::render(&info, &at, &at),
    )?;
    Ok(response(&info, &scope.index.vault_root))
}

pub fn update(
    vault: &Path,
    request: &UpdateAgendaRequest,
    scope: &WriteScope,
) -> Result<AgendaWriteResponse, EngineError> {
    const USAGE: &str = "update-agenda <아젠다 id> [title=<이름>] [hue=<색>] [repo=<저장소. 빈 값이면 지운다>] [summary=<한 문단 설명>] [project=<프로젝트 slug. 빈 값이면 미분류로>] [projectRepo=<프로젝트의 저장소 이름. 빈 값이면 지운다>]";
    require_id(&request.id, USAGE)?;
    let hue = hue_arg(request.hue.as_deref())?;
    let project_given = request.project.is_some();
    let mut project = match &request.project {
        Some(raw) => project_arg(raw, "프로젝트 slug 가 안전한 경로 세그먼트가 아니다: ")?,
        None => None,
    };
    let current = find(&scope, &request.id)?;
    let title = match trim(request.title.as_deref().unwrap_or("")) {
        "" => current.title.clone(),
        given => given.to_string(),
    };
    let hue = hue.unwrap_or_else(|| current.hue.clone());
    let repo = match &request.repo {
        Some(raw) => Some(trim(raw))
            .filter(|r| !r.is_empty())
            .map(str::to_string),
        None => current.repo.clone(),
    };
    if !project_given {
        project = current.project.clone();
    }
    let project_repo = if request.project_repo.is_some() {
        project_repo_arg(vault, project.as_deref(), request.project_repo.as_deref())?
    } else if project.is_some() && project == current.project {
        current.project_repo.clone()
    } else {
        None
    };
    let summary = request
        .summary
        .clone()
        .unwrap_or_else(|| current.summary.clone());
    let mut info = AgendaInfo {
        id: request.id.clone(),
        created_at: current.created_at.clone(),
        edited_at: current.edited_at.clone(),
        slug: current.slug.clone(),
        title,
        hue,
        repo,
        project,
        project_repo,
        summary,
    };
    let changed = info != *current;
    if changed || info.created_at.is_none() || info.edited_at.is_none() {
        let (created_at, edited_at) = agenda_file::resolve_edit_timestamps(
            changed,
            info.created_at.take(),
            info.edited_at.take(),
        );
        write_atomic(
            &vault.join(agenda_file::rel_path(&info.slug)),
            &agenda_file::render(&info, &created_at, &edited_at),
        )?;
        info.created_at = Some(created_at);
        info.edited_at = Some(edited_at);
    }
    Ok(response(&info, &scope.index.vault_root))
}

pub fn delete(
    vault: &Path,
    request: &DeleteAgendaRequest,
    scope: &WriteScope,
) -> Result<DeleteAgendaResponse, EngineError> {
    const USAGE: &str = "delete-agenda <아젠다 id>";
    require_id(&request.id, USAGE)?;
    let agenda = find(&scope, &request.id)?;
    let table = &scope.index.agendas.table;
    let notes = scope
        .index
        .root_notes()
        .filter(|n| {
            table
                .by_slug(&n.meta.agenda_slug)
                .is_some_and(|a| a.id == request.id)
        })
        .count();
    if notes > 0 {
        return Err(EngineError::invalid_argument(format!(
            "노트가 {notes}건 있는 아젠다는 지울 수 없다. 먼저 그 노트를 옮기거나 버린다: {}",
            agenda.slug
        )));
    }
    let slug = agenda.slug.clone();
    let archived = format!(".aisurada/trash/agendas/{slug}-{}.md", request.id);
    archive(
        &vault.join(agenda_file::rel_path(&slug)),
        &vault.join(&archived),
        "아젠다",
    )?;
    Ok(DeleteAgendaResponse {
        id: request.id.clone(),
        notes_dir: format!("notes/{slug}"),
        slug,
        archived_path: archived,
    })
}

/// 파일을 휴지통 폴더로 옮긴다. 옮길 파일이 없으면 경고만 하고 넘어가고, 옮길 자리에 이미 있으면 실패한다.
pub(crate) fn archive(source: &Path, target: &Path, kind: &str) -> Result<(), EngineError> {
    if !fs::exists(source) {
        eprintln!(
            "Warning: 옮길 {kind} 파일이 없어 옮기지 않았다: {}",
            source.display()
        );
        return Ok(());
    }
    if fs::exists(target) {
        return Err(EngineError::internal(format!(
            "옮길 자리에 이미 무엇이 있어 옮기지 않았다: {}",
            target.display()
        )));
    }
    let dir = target.parent().unwrap_or(target);
    fs::create_dir_all(dir)
        .map_err(|_| EngineError::internal(format!("폴더를 만들지 못했다: {}", dir.display())))?;
    fs::rename(source, target).map_err(|_| {
        EngineError::internal(format!(
            "옮기지 못했다: {} -> {}",
            source.display(),
            target.display()
        ))
    })
}
