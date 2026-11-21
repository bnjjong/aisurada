//! 프로젝트 쓰기: `create_project`, `update_project`, `delete_project`. 인자 이름과 응답 필드는 Java 엔진과 같다.

use std::collections::HashSet;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::agenda_write::{archive, hue_arg};
use crate::error::EngineError;
use crate::model::ProjectRow;
use crate::text::{is_blank, trim};
use crate::vault::project::{ProjectInfo, is_valid_project_id, scan_projects};
use crate::write::agenda_file::{self, MAX_SLUG_SUFFIX};
use crate::write::apparatus::{format_utc_millis, next_id, now_millis};
use crate::write::atomic::write_atomic;
use crate::write::confine::has_unusable_path_characters;
use crate::write::pipeline::WriteScope;
use crate::write::{project_file, slug};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CreateProjectRequest {
    pub title: Option<String>,
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

pub fn create(vault: &Path, request: &CreateProjectRequest) -> Result<ProjectRow, EngineError> {
    let title = trim(request.title.as_deref().unwrap_or(""));
    if title.is_empty() {
        return Err(EngineError::invalid_argument("title 이 필요하다"));
    }
    let hue = hue_arg(request.hue.as_deref())?;
    let summary = request.summary.clone().unwrap_or_default();
    let repos = project_file::parse_strict(request.repos.as_deref())?;
    let projects = scan_projects(vault);
    let id = next_id("pj");
    let taken: HashSet<String> = projects.iter().map(|p| p.slug.clone()).collect();
    let Some(slug) = agenda_file::pick_free_slug(&slug::build(title, 50), &taken) else {
        return Err(EngineError::invalid_argument(format!(
            "같은 이름의 프로젝트가 {MAX_SLUG_SUFFIX} 개까지 차 있어 새로 만들 수 없다: {title}"
        )));
    };
    let hue = hue.unwrap_or_else(|| {
        agenda_file::least_used_hue(projects.iter().map(|p| p.hue.as_str())).to_string()
    });
    let file_name = format!("{slug}.md");
    if has_unusable_path_characters(&file_name) {
        return Err(EngineError::invalid_argument(format!(
            "경로로 쓸 수 없는 값이다: {file_name}"
        )));
    }
    let at = format_utc_millis(now_millis());
    let info = ProjectInfo {
        id,
        created_at: Some(at.clone()),
        edited_at: Some(at),
        slug,
        title: title.to_string(),
        hue,
        summary,
        repos,
    };
    write_atomic(
        &vault.join(project_file::rel_path(&info.slug)),
        &project_file::render(&info),
    )?;
    Ok(ProjectRow::from(&info))
}

pub fn update(vault: &Path, request: &UpdateProjectRequest) -> Result<ProjectRow, EngineError> {
    const USAGE: &str = "update-project <프로젝트 id> [title=<이름>] [hue=<색>] [summary=<한 문단 설명>] [repos=<저장소 목록 JSON을 통째로 바꾼다. 빈 값이면 지운다>]";
    require_id(&request.id, USAGE)?;
    let hue = hue_arg(request.hue.as_deref())?;
    let current = find(&scan_projects(vault), &request.id)?;
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
        hue: hue.unwrap_or_else(|| current.hue.clone()),
        summary,
        repos,
    };
    if info != current {
        info.edited_at = Some(format_utc_millis(now_millis()));
        write_atomic(
            &vault.join(project_file::rel_path(&info.slug)),
            &project_file::render(&info),
        )?;
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
