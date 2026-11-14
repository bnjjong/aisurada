//! `list_projects`: 프로젝트를 id 차례로 돌려줍니다.

use std::path::Path;

use crate::model::{Page, ProjectRow};
use crate::text::compare_utf16;
use crate::vault::project::scan_projects;

pub fn execute(vault_root: &Path) -> Page<ProjectRow> {
    let mut projects = scan_projects(vault_root);
    projects.sort_by(|a, b| compare_utf16(&a.id, &b.id));
    let total = projects.len();
    Page::new(projects.iter().map(ProjectRow::from).collect(), total)
}
