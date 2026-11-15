//! 명령 실행기. 요청은 이름과 인자 객체로 받고, 응답은 JSON으로 돌려줍니다.

pub mod agenda_write;
pub mod ask_chat_commands;
pub mod comment_write;
pub mod commit_vault;
pub mod daily_notes;
pub mod doc_create;
pub mod doc_write;
pub mod export_markdown;
pub mod facts_brief;
pub mod facts_write;
pub mod get_agenda_facts;
pub mod get_doc;
pub mod get_sub_note;
pub mod get_template;
pub mod history;
pub mod init_workspace;
pub mod list_agendas;
pub mod list_attachments;
pub mod list_comments;
pub mod list_docs;
pub mod list_projects;
pub mod list_sub_notes;
pub mod list_tags;
pub mod list_templates;
pub mod list_trash;
pub mod note_meta_write;
pub mod option_text;
pub mod project_write;
pub mod reindex;
pub mod resource_commands;
pub mod search_docs;
pub mod session_commands;
pub mod shard_notes;
pub mod sub_note_write;
mod subnote;
pub mod task_commands;
pub mod template_write;
pub mod trash_write;
pub mod write_attachment;

use serde::Deserialize;

/// 호출자. 앱과 AI는 보이는 것이 다르다(AI에게는 기록 층 노트가 안 보인다).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Caller {
    #[default]
    App,
    Ai,
}
