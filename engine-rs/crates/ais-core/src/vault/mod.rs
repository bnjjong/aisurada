//! vault(노트 폴더) 읽기: 아젠다, 노트, 후속 노트를 훑어 색인을 만들고 조건으로 거릅니다.

internal_mods!(
    agenda,
    attach_text,
    comment_copy,
    comments,
    file_size,
    filter,
    history,
    note_file,
    note_meta,
    plain_text,
    project,
    scanner,
    sub_agenda,
);
pub mod facts;
pub mod git;
pub mod index;

pub(crate) fn check_manager_project(
    root: &Path,
    manager: &ManagerContext,
) -> Result<project::ProjectInfo, EngineError> {
    project::scan_projects(root)
        .into_iter()
        .find(|item| item.slug == manager.project && item.id == manager.project_id)
        .ok_or_else(|| EngineError::not_found("매니저의 프로젝트를 찾을 수 없다"))
}
pub mod template;
pub mod trash;

use crate::error::EngineError;
use crate::kernel::actor::ManagerContext;
use serde::Serialize;
use std::path::Path;

/// 훑다가 건너뛴 파일 하나. `kind`는 `note`, `agenda`, `docTemplate`, `ritualTemplate`, `trash` 가운데 하나다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Skipped {
    pub kind: &'static str,
    pub path: String,
    pub reason: String,
}

/// 동기화 도구가 만든 충돌 사본으로 보고 노트로 읽지 않은 항목
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Conflict {
    pub kind: &'static str,
    pub path: String,
}
