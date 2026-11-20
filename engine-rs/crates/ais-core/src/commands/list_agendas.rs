//! `list_agendas`: 아젠다를 slug 차례로 돌려줍니다.

use crate::model::{AgendaRow, Page};
use crate::text::compare_utf16;
use crate::vault::index::NoteIndex;

pub fn execute(index: &NoteIndex) -> Page<AgendaRow> {
    let mut agendas: Vec<_> = index.agendas.table.infos().collect();
    agendas.sort_by(|a, b| compare_utf16(&a.slug, &b.slug));
    let total = agendas.len();
    let items = agendas
        .into_iter()
        .map(|agenda| {
            let mut row = AgendaRow::from(agenda);
            row.sub_agendas = index.sub_agendas(&agenda.slug).to_vec();
            row
        })
        .collect();
    Page::new(items, total)
}
