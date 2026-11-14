//! 명령 실행기. 요청은 이름과 인자 객체로 받고, 응답은 JSON으로 돌려줍니다.

pub mod get_agenda_facts;
pub mod get_doc;
pub mod get_sub_note;
pub mod list_agendas;
pub mod list_attachments;
pub mod list_comments;
pub mod list_docs;
pub mod list_projects;
pub mod list_sub_notes;
pub mod list_templates;
pub mod list_trash;
pub mod reindex;
mod subnote;

use serde::Deserialize;

/// 호출자. 앱과 AI는 보이는 것이 다르다(AI에게는 기록 층 노트가 안 보인다).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Caller {
    #[default]
    App,
    Ai,
}
