//! `get_agenda_facts`: 아젠다의 사실층(지금도 참인 것)을 고치기 좋게 읽는다.

use serde::Deserialize;

use crate::error::EngineError;
use crate::model::FactsRow;
use crate::text::is_blank;
use crate::vault::agenda::is_valid_agenda_id;
use crate::vault::facts::{self, EDIT_LIMIT, LIMIT};
use crate::vault::index::NoteIndex;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GetAgendaFactsRequest {
    /// 아젠다 id
    pub id: String,
}

pub fn execute(
    index: &NoteIndex,
    request: &GetAgendaFactsRequest,
) -> Result<FactsRow, EngineError> {
    if is_blank(&request.id) {
        return Err(EngineError::invalid_argument(
            "인자가 하나 필요하다: get-agenda-facts <아젠다 id>",
        ));
    }
    if !is_valid_agenda_id(&request.id) {
        return Err(EngineError::invalid_argument(format!(
            "아젠다 id 모양이 아니다: {}",
            request.id
        )));
    }
    let agenda = index
        .agendas
        .table
        .by_id(&request.id)
        .ok_or_else(|| EngineError::not_found(format!("Agenda not found: {}", request.id)))?;
    let read = facts::read_for_edit(&index.vault_root, &agenda.slug);
    Ok(FactsRow {
        id: agenda.id.clone(),
        slug: agenda.slug.clone(),
        path: facts::relative_path(&agenda.slug),
        limit: LIMIT,
        edit_limit: EDIT_LIMIT,
        facts: read.facts,
        too_large_to_edit: read.too_large,
    })
}
