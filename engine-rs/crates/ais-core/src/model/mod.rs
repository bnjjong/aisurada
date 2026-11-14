//! 앱이 받는 응답 모양입니다. 목록은 늘 `{items, total, returned, truncated}`입니다.

mod catalog;
mod notes;

pub use catalog::{ProjectRow, RepoRow, ScanSummary, TemplateRow, TrashRow};
pub use notes::{AgendaRow, EvaluationRow, LinkRow, NoteRow, Page};
