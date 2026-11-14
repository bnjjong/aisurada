//! `list_docs`: 조건에 맞는 메인 노트를 만든 시각 차례로 돌려줍니다.

use serde::Deserialize;

use super::Caller;
use crate::error::EngineError;
use crate::model::{NoteRow, Page};
use crate::vault::attach_text;
use crate::vault::filter::{self, DocFilter};
use crate::vault::index::NoteIndex;

/// 앱이 건수를 안 정했을 때 돌려주는 건수
const DEFAULT_LIMIT: usize = 100;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ListDocsRequest {
    #[serde(flatten)]
    pub filter: DocFilter,
    /// 돌려줄 최대 건수. 없으면 앱은 100건, `0`이면 전부. AI는 늘 전부다.
    pub limit: Option<usize>,
    /// 본문(과 HTML 첨부 글)까지 싣는가. 기본은 메타만.
    pub bodies: bool,
    pub caller: Caller,
}

pub fn execute(index: &NoteIndex, request: &ListDocsRequest) -> Result<Page<NoteRow>, EngineError> {
    let criteria = filter::build(&index.agendas.table, &request.filter)?;

    let candidates: Vec<_> = index
        .root_notes()
        // AI에게는 기록 층 노트를 보여 주지 않는다
        .filter(|note| !(request.caller == Caller::Ai && note.meta.tier == "record"))
        .filter(|note| filter::matches(note, &criteria))
        .collect();
    let total = candidates.len();

    let take = match (request.caller, request.limit) {
        (Caller::Ai, _) | (_, Some(0)) => total,
        (Caller::App, Some(n)) => n.min(total),
        (Caller::App, None) => DEFAULT_LIMIT.min(total),
    };

    let items = candidates
        .into_iter()
        .take(take)
        .map(|note| {
            let attach = request
                .bodies
                .then(|| attach_text::extract(&index.vault_root, note))
                .flatten();
            NoteRow::from_record(note, request.caller, request.bodies, attach)
        })
        .collect();
    Ok(Page::new(items, total))
}
