//! 프로젝트 생성, 수정, 삭제. 유형과 저장소 목록을 프로젝트 파일에 기록한다.

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::agenda_write::{archive, hue_arg};
use super::task_commands::project_task_count;
use crate::error::EngineError;
use crate::model::ProjectRow;
use crate::text::{is_blank, trim};
use crate::vault::project::{
    ProjectInfo, ProjectType, archived_project_slugs, is_valid_project_id, scan_projects,
};
use crate::write::agenda_file;
use crate::write::apparatus::{format_utc_millis, next_id, now_millis};
use crate::write::atomic::write_atomic;
use crate::write::pipeline::WriteScope;
use crate::write::project_file;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CreateProjectRequest {
    pub title: Option<String>,
    #[serde(rename = "type")]
    pub project_type: Option<String>,
    pub hue: Option<String>,
    pub summary: Option<String>,
    /// 저장소 목록(JSON 배열 글)
    pub repos: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UpdateProjectRequest {
    pub id: String,
    pub title: Option<String>,
    #[serde(rename = "type")]
    pub project_type: Option<String>,
    pub hue: Option<String>,
    pub summary: Option<String>,
    /// 주면 저장소 목록을 통째로 바꾼다. 빈 글이면 지운다.
    pub repos: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DeleteProjectRequest {
    pub id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProjectResponse {
    pub id: String,
    pub slug: String,
    pub archived_path: String,
}

fn require_id(id: &str, usage: &str) -> Result<(), EngineError> {
    if is_blank(id) {
        return Err(EngineError::invalid_argument(format!(
            "인자가 하나 필요하다: {usage}"
        )));
    }
    if !is_valid_project_id(id) {
        return Err(EngineError::invalid_argument(format!(
            "프로젝트 id 모양이 아니다: {id}"
        )));
    }
    Ok(())
}

fn find(projects: &[ProjectInfo], id: &str) -> Result<ProjectInfo, EngineError> {
    projects
        .iter()
        .find(|p| p.id == id)
        .cloned()
        .ok_or_else(|| EngineError::not_found(format!("Project not found: {id}")))
}

fn parse_project_type(value: &str) -> Result<ProjectType, EngineError> {
    ProjectType::parse(trim(value)).ok_or_else(|| {
        EngineError::invalid_argument(format!(
            "type 은 development 또는 general 이어야 한다: {value}"
        ))
    })
}

pub fn create(vault: &Path, request: &CreateProjectRequest) -> Result<ProjectRow, EngineError> {
    let title = trim(request.title.as_deref().unwrap_or(""));
    if title.is_empty() {
        return Err(EngineError::invalid_argument("title 이 필요하다"));
    }
    let hue = hue_arg(request.hue.as_deref())?;
    let project_type = request
        .project_type
        .as_deref()
        .map(parse_project_type)
        .transpose()?
        .unwrap_or_default();
    let summary = request.summary.clone().unwrap_or_default();
    let repos = project_file::parse_strict(request.repos.as_deref())?;
    let projects = scan_projects(vault);
    let id = next_id("pj");
    let archived = archived_project_slugs(vault).map_err(|error| {
        EngineError::internal(format!("지워진 프로젝트 목록을 읽지 못했다: {error}"))
    })?;
    let taken: HashSet<String> = projects.iter().map(|p| p.slug.clone())
        .chain(archived).collect();
    let slug = agenda_file::allocate_entity_slug("프로젝트가", title, &taken)?;
    let hue = hue.unwrap_or_else(|| {
        agenda_file::least_used_hue(projects.iter().map(|p| p.hue.as_str())).to_string()
    });
    let at = format_utc_millis(now_millis());
    let info = ProjectInfo {
        id,
        created_at: Some(at.clone()),
        edited_at: Some(at.clone()),
        slug,
        title: title.to_string(),
        project_type,
        hue,
        summary,
        repos,
    };
    write_atomic(
        &vault.join(project_file::rel_path(&info.slug)),
        &project_file::render(&info, &at, &at),
    )?;
    Ok(ProjectRow::from(&info))
}

pub fn update(vault: &Path, request: &UpdateProjectRequest) -> Result<ProjectRow, EngineError> {
    const USAGE: &str = "update-project <프로젝트 id> [title=<이름>] [type=development|general] [hue=<색>] [summary=<한 문단 설명>] [repos=<저장소 목록 JSON을 통째로 바꾼다. 빈 값이면 지운다>]";
    require_id(&request.id, USAGE)?;
    let hue = hue_arg(request.hue.as_deref())?;
    let current = find(&scan_projects(vault), &request.id)?;
    let project_type = request
        .project_type
        .as_deref()
        .map(parse_project_type)
        .transpose()?
        .unwrap_or(current.project_type);
    let title = match trim(request.title.as_deref().unwrap_or("")) {
        "" => current.title.clone(),
        given => given.to_string(),
    };
    let summary = request
        .summary
        .clone()
        .unwrap_or_else(|| current.summary.clone());
    let repos = match &request.repos {
        Some(raw) => project_file::parse_strict(Some(raw))?,
        None => current.repos.clone(),
    };
    let mut info = ProjectInfo {
        id: request.id.clone(),
        created_at: current.created_at.clone(),
        edited_at: current.edited_at.clone(),
        slug: current.slug.clone(),
        title,
        project_type,
        hue: hue.unwrap_or_else(|| current.hue.clone()),
        summary,
        repos,
    };
    let changed = info != current;
    if changed || info.created_at.is_none() || info.edited_at.is_none() {
        let at = format_utc_millis(now_millis());
        let created_at = info.created_at.take().unwrap_or_else(|| at.clone());
        let edited_at = if changed {
            at
        } else {
            info.edited_at.take().unwrap_or(at)
        };
        write_atomic(
            &vault.join(project_file::rel_path(&info.slug)),
            &project_file::render(&info, &created_at, &edited_at),
        )?;
        info.created_at = Some(created_at);
        info.edited_at = Some(edited_at);
    }
    Ok(ProjectRow::from(&info))
}

pub fn delete(
    vault: &Path,
    request: &DeleteProjectRequest,
    scope: &WriteScope,
) -> Result<DeleteProjectResponse, EngineError> {
    const USAGE: &str = "delete-project <프로젝트 id>";
    require_id(&request.id, USAGE)?;
    let project = find(&scan_projects(vault), &request.id)?;
    let agendas = scope
        .index
        .agendas
        .table
        .infos()
        .filter(|a| a.project.as_deref() == Some(project.slug.as_str()))
        .count();
    if agendas > 0 {
        return Err(EngineError::invalid_argument(format!(
            "아젠다가 {agendas}건 있는 프로젝트는 지울 수 없다. 먼저 그 아젠다를 옮긴다: {}",
            project.slug
        )));
    }
    let tasks = project_task_count(vault, &project.slug);
    if tasks > 0 {
        return Err(EngineError::invalid_argument(format!(
            "태스크가 {tasks}건 있는 프로젝트는 지울 수 없다. 먼저 그 태스크를 옮긴다: {}",
            project.slug
        )));
    }
    let archived = project_file::archived_rel_path(&project.slug, &request.id);
    archive(
        &vault.join(project_file::rel_path(&project.slug)),
        &vault.join(&archived),
        "프로젝트",
    )?;
    Ok(DeleteProjectResponse {
        id: request.id.clone(),
        slug: project.slug,
        archived_path: archived,
    })
}
