//! `list_agendas`: 아젠다를 slug 차례로 돌려줍니다.

use crate::model::{AgendaRow, Page};
use crate::text::compare_utf16;
use crate::vault::index::NoteIndex;

pub fn execute(index: &NoteIndex) -> Page<AgendaRow> {
    let mut agendas: Vec<_> = index.agendas.table.infos().collect();
    agendas.sort_by(|a, b| compare_utf16(&a.slug, &b.slug));
    let total = agendas.len();
    Page::new(agendas.into_iter().map(AgendaRow::from).collect(), total)
}
