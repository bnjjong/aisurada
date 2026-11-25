//! 명령 실행기. 요청은 이름과 인자 객체로 받고, 응답은 JSON으로 돌려줍니다.

pub mod agenda_write;
pub mod ask_chat_commands;
pub mod c4_commands;
pub mod comment_write;
pub mod commit_vault;
pub mod daily_notes;
pub mod decisions;
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
pub(crate) mod intake_commands;
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
pub mod sub_agenda_write;
pub mod sub_note_write;
mod subnote;
pub mod task_commands;
pub mod template_write;
pub mod trash_write;
pub mod write_attachment;

/// 호출자는 공용 커널이 정의한다(`kernel::actor`). 바깥 크레이트가 쓰는 옛 경로를 위해 다시 내보낸다.
pub use crate::kernel::actor::Caller;
