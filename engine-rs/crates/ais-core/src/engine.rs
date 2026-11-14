//! 엔진의 진입점. 앱의 `engine_call`, `ais-cli`, MCP가 모두 이 한 곳으로 들어옵니다.

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::commands::{
    agenda_write, commit_vault, doc_create, doc_write, facts_write, get_agenda_facts, get_doc,
    get_sub_note, get_template, history, init_workspace, list_agendas, list_attachments,
    list_comments, list_docs, list_projects, list_sub_notes, list_tags, list_templates, list_trash,
    note_meta_write, project_write, reindex, search_docs, template_write,
};
use crate::error::EngineError;
use crate::vault::index::NoteIndex;

/// vault 하나를 맡는 엔진. 요청마다 vault를 새로 훑으므로 상태가 없다.
pub struct Engine {
    vault_root: PathBuf,
}

/// 이 엔진이 처리하는 명령 이름
pub const COMMANDS: &[&str] = &[
    "list_docs",
    "list_agendas",
    "list_projects",
    "list_templates",
    "list_trash",
    "reindex",
    "get_doc",
    "list_sub_notes",
    "get_sub_note",
    "list_comments",
    "list_attachments",
    "get_agenda_facts",
    "search_docs",
    "list_tags",
    "get_template",
    "history",
    "commit_vault",
    "create_agenda",
    "update_agenda",
    "delete_agenda",
    "create_project",
    "update_project",
    "delete_project",
    "save_template",
    "save_ritual_template",
    "delete_doc_template",
    "update_agenda_facts",
    "init_workspace",
    "set_favorite",
    "evaluate_doc",
    "link_docs",
    "update_doc",
    "create_doc",
];

/// 쓰기 명령. 앱이 이 명령을 Rust로 보내는 것은 쓰기 스위치(`AISURADA_ENGINE_WRITE=rust`)가 켜졌을 때뿐이다.
pub const WRITE_COMMANDS: &[&str] = &[
    "create_agenda",
    "update_agenda",
    "delete_agenda",
    "create_project",
    "update_project",
    "delete_project",
    "save_template",
    "save_ritual_template",
    "delete_doc_template",
    "update_agenda_facts",
    "init_workspace",
    "set_favorite",
    "evaluate_doc",
    "link_docs",
    "update_doc",
    "create_doc",
    "commit_vault",
];

impl Engine {
    pub fn new(vault_root: impl AsRef<Path>) -> Self {
        Engine {
            vault_root: vault_root.as_ref().to_path_buf(),
        }
    }

    /// 쓰기 명령인가
    pub fn is_write(command: &str) -> bool {
        WRITE_COMMANDS.contains(&command)
    }

    /// 이 엔진이 처리하는 명령인가(아니면 호출 쪽이 다른 길로 보낸다)
    pub fn supports(command: &str) -> bool {
        COMMANDS.contains(&command)
    }

    /// 이름과 인자 객체로 명령을 실행하고 응답 JSON을 돌려줍니다.
    pub fn call(&self, command: &str, args: Value) -> Result<Value, EngineError> {
        match command {
            "list_docs" => {
                let request: list_docs::ListDocsRequest = parse_args(command, args)?;
                to_json(list_docs::execute(
                    &NoteIndex::fresh(&self.vault_root),
                    &request,
                )?)
            }
            "list_agendas" => to_json(list_agendas::execute(&NoteIndex::fresh(&self.vault_root))),
            "list_projects" => to_json(list_projects::execute(&self.vault_root)),
            "list_templates" => {
                let request: list_templates::ListTemplatesRequest = parse_args(command, args)?;
                to_json(list_templates::execute(&self.vault_root, &request)?)
            }
            "list_trash" => to_json(list_trash::execute(&self.vault_root)),
            "reindex" => to_json(reindex::execute(&self.vault_root)?),
            "get_doc" => {
                let request: get_doc::GetDocRequest = parse_args(command, args)?;
                to_json(get_doc::execute(
                    &NoteIndex::fresh(&self.vault_root),
                    &request,
                )?)
            }
            "list_sub_notes" => {
                let request: list_sub_notes::ListSubNotesRequest = parse_args(command, args)?;
                to_json(list_sub_notes::execute(
                    &NoteIndex::fresh(&self.vault_root),
                    &request,
                )?)
            }
            "get_sub_note" => {
                let request: get_sub_note::GetSubNoteRequest = parse_args(command, args)?;
                to_json(get_sub_note::execute(
                    &NoteIndex::fresh(&self.vault_root),
                    &request,
                )?)
            }
            "list_comments" => {
                let request: list_comments::ListCommentsRequest = parse_args(command, args)?;
                to_json(list_comments::execute(
                    &NoteIndex::fresh(&self.vault_root),
                    &request,
                )?)
            }
            "list_attachments" => {
                let request: list_attachments::ListAttachmentsRequest = parse_args(command, args)?;
                to_json(list_attachments::execute(
                    &NoteIndex::fresh(&self.vault_root),
                    &request,
                )?)
            }
            "get_agenda_facts" => {
                let request: get_agenda_facts::GetAgendaFactsRequest = parse_args(command, args)?;
                to_json(get_agenda_facts::execute(
                    &NoteIndex::fresh(&self.vault_root),
                    &request,
                )?)
            }
            "search_docs" => {
                let request: search_docs::SearchDocsRequest = parse_args(command, args)?;
                to_json(search_docs::execute(
                    &NoteIndex::fresh(&self.vault_root),
                    &request,
                )?)
            }
            "list_tags" => {
                let request: list_tags::ListTagsRequest = parse_args(command, args)?;
                to_json(list_tags::execute(
                    &NoteIndex::fresh(&self.vault_root),
                    &request,
                )?)
            }
            "get_template" => {
                let request: get_template::GetTemplateRequest = parse_args(command, args)?;
                to_json(get_template::execute(
                    &NoteIndex::fresh(&self.vault_root),
                    &request,
                )?)
            }
            "history" => {
                let request: history::HistoryRequest = parse_args(command, args)?;
                to_json(history::execute(&self.vault_root, &request)?)
            }
            "create_agenda" => {
                let request: agenda_write::CreateAgendaRequest = parse_args(command, args)?;
                to_json(agenda_write::create(&self.vault_root, &request)?)
            }
            "update_agenda" => {
                let request: agenda_write::UpdateAgendaRequest = parse_args(command, args)?;
                to_json(agenda_write::update(&self.vault_root, &request)?)
            }
            "delete_agenda" => {
                let request: agenda_write::DeleteAgendaRequest = parse_args(command, args)?;
                to_json(agenda_write::delete(&self.vault_root, &request)?)
            }
            "create_project" => {
                let request: project_write::CreateProjectRequest = parse_args(command, args)?;
                to_json(project_write::create(&self.vault_root, &request)?)
            }
            "update_project" => {
                let request: project_write::UpdateProjectRequest = parse_args(command, args)?;
                to_json(project_write::update(&self.vault_root, &request)?)
            }
            "delete_project" => {
                let request: project_write::DeleteProjectRequest = parse_args(command, args)?;
                to_json(project_write::delete(&self.vault_root, &request)?)
            }
            "save_template" => {
                let request: template_write::SaveTemplateRequest = parse_args(command, args)?;
                to_json(template_write::save_document(&self.vault_root, &request)?)
            }
            "save_ritual_template" => {
                let request: template_write::SaveRitualTemplateRequest = parse_args(command, args)?;
                to_json(template_write::save_ritual(&self.vault_root, &request)?)
            }
            "delete_doc_template" => {
                let request: template_write::DeleteDocTemplateRequest = parse_args(command, args)?;
                to_json(template_write::delete_document(&self.vault_root, &request)?)
            }
            "update_agenda_facts" => {
                let request: facts_write::UpdateAgendaFactsRequest = parse_args(command, args)?;
                to_json(facts_write::update(&self.vault_root, &request)?)
            }
            "init_workspace" => {
                let request: init_workspace::InitWorkspaceRequest = parse_args(command, args)?;
                to_json(init_workspace::execute(&self.vault_root, &request)?)
            }
            "set_favorite" => {
                let request: note_meta_write::SetFavoriteRequest = parse_args(command, args)?;
                to_json(note_meta_write::set_favorite(&self.vault_root, &request)?)
            }
            "evaluate_doc" => {
                let request: note_meta_write::EvaluateDocRequest = parse_args(command, args)?;
                to_json(note_meta_write::evaluate_doc(&self.vault_root, &request)?)
            }
            "link_docs" => {
                let request: note_meta_write::LinkDocsRequest = parse_args(command, args)?;
                to_json(note_meta_write::link_docs(&self.vault_root, &request)?)
            }
            "create_doc" => {
                let request: doc_create::CreateDocRequest = parse_args(command, args)?;
                to_json(doc_create::create_doc(&self.vault_root, &request)?)
            }
            "update_doc" => {
                let request: doc_write::UpdateDocRequest = parse_args(command, args)?;
                to_json(doc_write::update_doc(&self.vault_root, &request)?)
            }
            "commit_vault" => to_json(commit_vault::execute(&self.vault_root)),
            other => Err(EngineError::unsupported(format!(
                "이 엔진에는 {other} 명령이 없다"
            ))),
        }
    }
}

fn parse_args<T: DeserializeOwned>(command: &str, args: Value) -> Result<T, EngineError> {
    // 인자가 없으면 빈 객체로 본다
    let args = if args.is_null() {
        Value::Object(Default::default())
    } else {
        args
    };
    serde_json::from_value(args)
        .map_err(|e| EngineError::invalid_argument(format!("{command} 인자가 잘못됐다: {e}")))
}

fn to_json<T: serde::Serialize>(value: T) -> Result<Value, EngineError> {
    serde_json::to_value(value)
        .map_err(|e| EngineError::internal(format!("응답을 JSON으로 못 만들었다: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 읽기 명령의 전부. 쓰기 명령을 `COMMANDS`에 더하고 `WRITE_COMMANDS`에 빠뜨리면 이 시험이 실패한다.
    /// 그러면 쓰기가 쓰기 스위치 없이 Rust로 가기 때문이다.
    const READS: &[&str] = &[
        "list_docs",
        "list_agendas",
        "list_projects",
        "list_templates",
        "list_trash",
        "reindex",
        "get_doc",
        "list_sub_notes",
        "get_sub_note",
        "list_comments",
        "list_attachments",
        "get_agenda_facts",
        "search_docs",
        "list_tags",
        "get_template",
        "history",
    ];

    #[test]
    fn every_command_is_either_a_known_read_or_gated_as_a_write() {
        for command in COMMANDS {
            assert!(
                READS.contains(command) != WRITE_COMMANDS.contains(command),
                "{command}: 읽기 목록과 쓰기 목록 가운데 정확히 한 곳에 있어야 한다"
            );
        }
        for command in WRITE_COMMANDS {
            assert!(
                COMMANDS.contains(command),
                "{command}: 쓰기 명령은 COMMANDS에도 있어야 한다"
            );
        }
        assert!(READS.iter().all(|r| COMMANDS.contains(r)));
    }
}
