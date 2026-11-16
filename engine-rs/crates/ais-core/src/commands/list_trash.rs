//! `list_trash`: 휴지통 항목을 버린 시각이 늦은 것부터 돌려줍니다.
//! AI 호출자에게는 기록 층 노트의 항목을 숨긴다(제목과 id도 새지 않게). 앱에는 모두 보인다.

use std::path::Path;

use serde::Deserialize;

use super::Caller;
use crate::model::{Page, TrashRow};
use crate::vault::trash::{archived_tier, scan_trash};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ListTrashRequest {
    pub caller: Caller,
}

pub fn execute(vault_root: &Path, request: &ListTrashRequest) -> Page<TrashRow> {
    let scan = scan_trash(vault_root);
    // 앱에는 층이 상관없으므로 보관 파일을 열지 않는다
    let rows: Vec<TrashRow> = scan
        .entries
        .iter()
        .filter(|entry| {
            request.caller == Caller::App
                || archived_tier(vault_root, entry).visible_to(request.caller)
        })
        .map(TrashRow::from)
        .collect();
    let total = rows.len();
    Page::new(rows, total)
}
