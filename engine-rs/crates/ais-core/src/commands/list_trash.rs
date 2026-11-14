//! `list_trash`: 휴지통 항목을 버린 시각이 늦은 것부터 돌려줍니다.

use std::path::Path;

use crate::model::{Page, TrashRow};
use crate::vault::trash::scan_trash;

pub fn execute(vault_root: &Path) -> Page<TrashRow> {
    let scan = scan_trash(vault_root);
    let total = scan.entries.len();
    Page::new(scan.entries.iter().map(TrashRow::from).collect(), total)
}
