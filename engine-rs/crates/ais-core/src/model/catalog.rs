//! 프로젝트, 템플릿, 휴지통, 훑기 요약의 응답 줄입니다.

use serde::Serialize;

use crate::vault::project::{ProjectInfo, ProjectRepo};
use crate::vault::template::TemplateRecord;
use crate::vault::trash::TrashRecord;
use crate::vault::{Conflict, Skipped};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoRow {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github_id: Option<u64>,
}

impl From<&ProjectRepo> for RepoRow {
    fn from(r: &ProjectRepo) -> Self {
        RepoRow {
            name: r.name.clone(),
            path: r.path.clone(),
            github: r.github.clone(),
            github_id: r.github_id,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ProjectRow {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub hue: String,
    pub summary: String,
    /// 저장소가 있을 때만 싣는다
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub repos: Vec<RepoRow>,
}

impl From<&ProjectInfo> for ProjectRow {
    fn from(p: &ProjectInfo) -> Self {
        ProjectRow {
            id: p.id.clone(),
            slug: p.slug.clone(),
            title: p.title.clone(),
            hue: p.hue.clone(),
            summary: p.summary.clone(),
            repos: p.repos.iter().map(RepoRow::from).collect(),
        }
    }
}

/// 템플릿 한 줄. 문서 템플릿은 `type`·`builtin`, 회고 템플릿은 `rev`·`icon`이 붙는다.
#[derive(Debug, Serialize)]
pub struct TemplateRow {
    pub id: String,
    pub category: &'static str,
    pub name: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub doc_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub format: &'static str,
    pub desc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub builtin: Option<bool>,
}

impl TemplateRow {
    pub fn from_record(t: &TemplateRecord, bodies: bool) -> Self {
        TemplateRow {
            id: t.id.clone(),
            category: t.category.as_str(),
            name: t.name.clone(),
            doc_type: t.doc_type.clone(),
            rev: t.rev.clone(),
            icon: t.icon.clone(),
            format: t.format,
            desc: t.desc.clone(),
            body: bodies.then(|| t.body.clone()),
            builtin: t.builtin,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashRow {
    pub trash_id: String,
    pub doc_id: String,
    pub title: String,
    pub agenda: String,
    pub agenda_id: String,
    #[serde(rename = "type")]
    pub doc_type: String,
    pub format: &'static str,
    pub original_path: String,
    pub file: String,
    pub created: String,
    pub trashed_at: String,
}

impl From<&TrashRecord> for TrashRow {
    fn from(r: &TrashRecord) -> Self {
        TrashRow {
            trash_id: r.trash_id.clone(),
            doc_id: r.doc_id.clone(),
            title: r.title.clone(),
            agenda: r.agenda_slug.clone(),
            agenda_id: r.agenda_id.clone(),
            doc_type: r.doc_type.clone(),
            format: r.format,
            original_path: r.original_path.clone(),
            file: r.file.clone(),
            created: r.created.clone(),
            trashed_at: r.trashed_at.clone(),
        }
    }
}

/// `reindex`의 응답: vault를 한 번 훑은 요약과, 읽지 못한 파일·충돌 사본 목록
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanSummary {
    pub mode: &'static str,
    pub elapsed_ms: u128,
    /// 아젠다 slug 개수
    pub agendas: usize,
    pub scanned: usize,
    pub indexed: usize,
    pub unchanged: usize,
    pub removed: usize,
    pub skipped: Vec<Skipped>,
    pub conflicts: Vec<Conflict>,
    pub unsharded: Vec<String>,
}
