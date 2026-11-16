//! 엔진의 진입점. 앱의 `engine_call`, `ais-cli`, MCP가 모두 이 한 곳으로 들어옵니다.

use crate::ports::Ports;
use crate::write::pipeline::{self, HasWriteContext, WriteContext, WriteScope};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::commands::{
    agenda_write, ask_chat_commands, comment_write, commit_vault, daily_notes, doc_create,
    doc_write, export_markdown, facts_brief, facts_write, get_agenda_facts, get_doc, get_sub_note,
    get_template, history, init_workspace, list_agendas, list_attachments, list_comments,
    list_docs, list_projects, list_sub_notes, list_tags, list_templates, list_trash,
    note_meta_write, project_write, reindex, resource_commands, search_docs, session_commands,
    shard_notes, sub_note_write, task_commands, template_write, trash_write, write_attachment,
};
use crate::error::EngineError;
use crate::vault::index::NoteIndex;

/// `export_markdown`은 인자 없이 호출 맥락만 받는다.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ExportMarkdownRequest {
    #[serde(flatten)]
    context: crate::write::pipeline::WriteContext,
}

/// 쓰기 요청 종류마다 `context` 칸에서 [`WriteContext`]를 꺼내는 구현을 붙인다.
macro_rules! write_context {
    ($request:ty) => {
        impl HasWriteContext for $request {
            fn write_context(&self) -> &WriteContext {
                &self.context
            }
        }
    };
}

write_context!(agenda_write::CreateAgendaRequest);
write_context!(agenda_write::UpdateAgendaRequest);
write_context!(agenda_write::DeleteAgendaRequest);
write_context!(project_write::CreateProjectRequest);
write_context!(project_write::UpdateProjectRequest);
write_context!(project_write::DeleteProjectRequest);
write_context!(template_write::SaveTemplateRequest);
write_context!(template_write::SaveRitualTemplateRequest);
write_context!(template_write::DeleteDocTemplateRequest);
write_context!(facts_write::UpdateAgendaFactsRequest);
write_context!(init_workspace::InitWorkspaceRequest);
write_context!(note_meta_write::SetFavoriteRequest);
write_context!(note_meta_write::EvaluateDocRequest);
write_context!(note_meta_write::LinkDocsRequest);
write_context!(task_commands::CreateTaskRequest);
write_context!(task_commands::UpdateTaskRequest);
write_context!(task_commands::TransitionTaskRequest);
write_context!(task_commands::LinkTasksRequest);
write_context!(task_commands::AddTaskCommentRequest);
write_context!(task_commands::TaskRefRequest);
write_context!(resource_commands::SaveResourceRequest);
write_context!(resource_commands::ResourceIdRequest);
write_context!(resource_commands::SetResourceRefRequest);
write_context!(resource_commands::ChangeDecisionRequest);
write_context!(ask_chat_commands::AppendAskTurnRequest);
write_context!(ask_chat_commands::AppendChatTurnRequest);
write_context!(write_attachment::WriteAttachmentRequest);
write_context!(shard_notes::ShardNotesRequest);
write_context!(daily_notes::PlanDayRequest);
write_context!(daily_notes::CheckNotesRequest);
write_context!(facts_brief::FactsBriefRequest);
write_context!(trash_write::DeleteDocRequest);
write_context!(trash_write::TrashEntryRequest);
write_context!(trash_write::EmptyTrashRequest);
write_context!(comment_write::CommentWriteRequest);
write_context!(sub_note_write::CreateSubNoteRequest);
write_context!(sub_note_write::UpdateSubNoteRequest);
write_context!(sub_note_write::DeleteSubNoteRequest);
write_context!(doc_create::CreateDocRequest);
write_context!(doc_write::UpdateDocRequest);
write_context!(resource_commands::ResolveResourceEditRequest);
write_context!(resource_commands::SetResourcePolicyRequest);
write_context!(ExportMarkdownRequest);

/// vault 하나를 맡는 엔진. 요청마다 vault를 새로 훑으므로 상태가 없다.
pub struct Engine {
    vault_root: PathBuf,
    /// 이 엔진이 명령을 부르는 동안 현재 스레드에 거는 약속 묶음. 없으면 기본 어댑터(환경 변수와 실제 시계)를 쓴다.
    ports: Option<Arc<Ports>>,
}

/// 워크스페이스 표식 검사를 건너뛰는 쓰기 명령. 표식을 처음 만드는 `init_workspace`는 표식이 아직 없는 폴더에서 부른다.
const MARKER_EXEMPT: &[&str] = &["init_workspace"];

/// 이 엔진이 처리하는 명령 이름
pub const COMMANDS: &[&str] = &[
    "list_docs",
    "list_agendas",
    "list_projects",
    "list_templates",
    "list_trash",
    "reindex",
    "reindex_paths",
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
    "create_sub_note",
    "update_sub_note",
    "delete_sub_note",
    "create_comment",
    "update_comment",
    "delete_comment",
    "delete_doc",
    "restore_doc",
    "purge_doc",
    "empty_trash",
    "get_task",
    "list_tasks",
    "create_task",
    "update_task",
    "transition_task",
    "link_tasks",
    "add_task_comment",
    "delete_task",
    "restore_task",
    "list_resources",
    "get_resource",
    "usable_resources",
    "list_resource_versions",
    "list_resource_changes",
    "get_resource_change",
    "get_resource_policy",
    "save_resource",
    "delete_resource",
    "restore_resource",
    "set_resource_ref",
    "approve_resource_change",
    "reject_resource_change",
    "resolve_resource_edit",
    "set_resource_policy",
    "get_ask",
    "list_chats",
    "append_ask_turn",
    "append_chat_turn",
    "read_session",
    "session_titles",
    "recent_sessions",
    "write_attachment",
    "export_markdown",
    "shard_notes",
    "verify_shard",
    "plan_day",
    "check_notes",
    "facts_brief",
];

/// 쓰기 명령. 앱도 이 명령을 이 엔진으로 보낸다(Java 엔진과 함께 쓰기 스위치를 없앴다).
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
    "create_sub_note",
    "update_sub_note",
    "delete_sub_note",
    "create_comment",
    "update_comment",
    "delete_comment",
    "delete_doc",
    "restore_doc",
    "purge_doc",
    "empty_trash",
    "create_task",
    "update_task",
    "transition_task",
    "link_tasks",
    "add_task_comment",
    "delete_task",
    "restore_task",
    "save_resource",
    "delete_resource",
    "restore_resource",
    "set_resource_ref",
    "approve_resource_change",
    "reject_resource_change",
    "resolve_resource_edit",
    "set_resource_policy",
    "append_ask_turn",
    "append_chat_turn",
    "write_attachment",
    "export_markdown",
    "shard_notes",
    "plan_day",
    "check_notes",
    "facts_brief",
    "commit_vault",
];

impl Engine {
    pub fn new(vault_root: impl AsRef<Path>) -> Self {
        Engine {
            vault_root: vault_root.as_ref().to_path_buf(),
            ports: None,
        }
    }

    /// 시계, 난수, 환경 변수, 잠금, git, sqlite3를 바꿔 끼운 약속 묶음으로 일하는 엔진(시험과 다른 저장소 구현을 위한 자리).
    /// 명령을 부르는 동안만 이 묶음이 현재 스레드에 걸린다. 엔진이 새로 만든 스레드는 기본 어댑터를 쓴다.
    pub fn with_ports(vault_root: impl AsRef<Path>, ports: Ports) -> Self {
        Engine {
            vault_root: vault_root.as_ref().to_path_buf(),
            ports: Some(Arc::new(ports)),
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
        let _installed = self.ports.clone().map(crate::ports::install);
        self.dispatch(command, args)
    }

    /// 쓰기 명령의 디스패치. 요청을 읽고, **범위를 이 한 곳에서 열고**(잠금 → 표식·색인 맞추기 → 자동 커밋 기록), 명령이 일하게 한 뒤 응답을 JSON으로 바꾼다.
    /// 명령은 범위를 열지 않는다. 열린 범위(`&WriteScope`)를 받는다. 범위는 응답을 만든 뒤에, 성공이든 실패든 놓인다.
    /// `ledger_baseline`은 자원 명령용이다: 범위를 연 직후 승인 기록장이 없으면 기준선을 만든다(쓰기보다 먼저여야 방금 쓴 파일이 기준선에 섞이지 않는다).
    fn write<Req, T>(
        &self,
        command: &str,
        args: Value,
        ledger_baseline: bool,
        run: impl FnOnce(&Req, &WriteScope) -> Result<T, EngineError>,
    ) -> Result<Value, EngineError>
    where
        Req: serde::de::DeserializeOwned + HasWriteContext,
        T: serde::Serialize,
    {
        let request: Req = parse_args(command, args)?;
        let marker_required = !MARKER_EXEMPT.contains(&command);
        let scope = pipeline::open(&self.vault_root, marker_required, request.write_context())?;
        if ledger_baseline {
            crate::resource::approvals::ledger(&self.vault_root)?;
        }
        to_json(run(&request, &scope)?)
    }

    fn dispatch(&self, command: &str, args: Value) -> Result<Value, EngineError> {
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
            "reindex_paths" => {
                let request: reindex::ReindexPathsRequest = parse_args(command, args)?;
                to_json(reindex::execute_paths(&self.vault_root, &request)?)
            }
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
            "create_agenda" => self.write::<agenda_write::CreateAgendaRequest, _>(
                command,
                args,
                false,
                |request, scope| agenda_write::create(&self.vault_root, request, scope),
            ),
            "update_agenda" => self.write::<agenda_write::UpdateAgendaRequest, _>(
                command,
                args,
                false,
                |request, scope| agenda_write::update(&self.vault_root, request, scope),
            ),
            "delete_agenda" => self.write::<agenda_write::DeleteAgendaRequest, _>(
                command,
                args,
                false,
                |request, scope| agenda_write::delete(&self.vault_root, request, scope),
            ),
            "create_project" => self.write::<project_write::CreateProjectRequest, _>(
                command,
                args,
                false,
                |request, _scope| project_write::create(&self.vault_root, request),
            ),
            "update_project" => self.write::<project_write::UpdateProjectRequest, _>(
                command,
                args,
                false,
                |request, _scope| project_write::update(&self.vault_root, request),
            ),
            "delete_project" => self.write::<project_write::DeleteProjectRequest, _>(
                command,
                args,
                false,
                |request, scope| project_write::delete(&self.vault_root, request, scope),
            ),
            "save_template" => self.write::<template_write::SaveTemplateRequest, _>(
                command,
                args,
                false,
                |request, _scope| template_write::save_document(&self.vault_root, request),
            ),
            "save_ritual_template" => self.write::<template_write::SaveRitualTemplateRequest, _>(
                command,
                args,
                false,
                |request, _scope| template_write::save_ritual(&self.vault_root, request),
            ),
            "delete_doc_template" => self.write::<template_write::DeleteDocTemplateRequest, _>(
                command,
                args,
                false,
                |request, _scope| template_write::delete_document(&self.vault_root, request),
            ),
            "update_agenda_facts" => self.write::<facts_write::UpdateAgendaFactsRequest, _>(
                command,
                args,
                false,
                |request, scope| facts_write::update(&self.vault_root, request, scope),
            ),
            "init_workspace" => self.write::<init_workspace::InitWorkspaceRequest, _>(
                command,
                args,
                false,
                |request, _scope| init_workspace::execute(&self.vault_root, request),
            ),
            "set_favorite" => self.write::<note_meta_write::SetFavoriteRequest, _>(
                command,
                args,
                false,
                |request, scope| note_meta_write::set_favorite(&self.vault_root, request, scope),
            ),
            "evaluate_doc" => self.write::<note_meta_write::EvaluateDocRequest, _>(
                command,
                args,
                false,
                |request, scope| note_meta_write::evaluate_doc(&self.vault_root, request, scope),
            ),
            "link_docs" => self.write::<note_meta_write::LinkDocsRequest, _>(
                command,
                args,
                false,
                |request, scope| note_meta_write::link_docs(&self.vault_root, request, scope),
            ),
            "get_task" => {
                let request: task_commands::GetTaskRequest = parse_args(command, args)?;
                to_json(task_commands::get_task(&self.vault_root, &request)?)
            }
            "list_tasks" => {
                let request: task_commands::ListTasksRequest = parse_args(command, args)?;
                to_json(task_commands::list_tasks(&self.vault_root, &request)?)
            }
            "create_task" => self.write::<task_commands::CreateTaskRequest, _>(
                command,
                args,
                false,
                |request, _scope| task_commands::create_task(&self.vault_root, request),
            ),
            "update_task" => self.write::<task_commands::UpdateTaskRequest, _>(
                command,
                args,
                false,
                |request, _scope| task_commands::update_task(&self.vault_root, request),
            ),
            "transition_task" => self.write::<task_commands::TransitionTaskRequest, _>(
                command,
                args,
                false,
                |request, _scope| task_commands::transition_task(&self.vault_root, request),
            ),
            "link_tasks" => self.write::<task_commands::LinkTasksRequest, _>(
                command,
                args,
                false,
                |request, _scope| task_commands::link_tasks(&self.vault_root, request),
            ),
            "add_task_comment" => self.write::<task_commands::AddTaskCommentRequest, _>(
                command,
                args,
                false,
                |request, _scope| task_commands::add_task_comment(&self.vault_root, request),
            ),
            "delete_task" => self.write::<task_commands::TaskRefRequest, _>(
                command,
                args,
                false,
                |request, _scope| task_commands::delete_task(&self.vault_root, request),
            ),
            "restore_task" => self.write::<task_commands::TaskRefRequest, _>(
                command,
                args,
                false,
                |request, _scope| task_commands::restore_task(&self.vault_root, request),
            ),
            "list_resources" => {
                let request: resource_commands::ListResourcesRequest = parse_args(command, args)?;
                to_json(resource_commands::list_resources(
                    &self.vault_root,
                    &request,
                )?)
            }
            "get_resource" => {
                let request: resource_commands::GetResourceRequest = parse_args(command, args)?;
                to_json(resource_commands::get_resource(&self.vault_root, &request)?)
            }
            "usable_resources" => {
                let request: resource_commands::UsableResourcesRequest = parse_args(command, args)?;
                to_json(resource_commands::usable_resources(
                    &self.vault_root,
                    &request,
                )?)
            }
            "list_resource_versions" => {
                let request: resource_commands::ResourceVersionsRequest =
                    parse_args(command, args)?;
                to_json(resource_commands::list_resource_versions(
                    &self.vault_root,
                    &request,
                )?)
            }
            "list_resource_changes" => {
                let request: resource_commands::ListResourceChangesRequest =
                    parse_args(command, args)?;
                to_json(resource_commands::list_resource_changes(
                    &self.vault_root,
                    &request,
                )?)
            }
            "get_resource_change" => {
                let request: resource_commands::ResourceChangeRequest = parse_args(command, args)?;
                to_json(resource_commands::get_resource_change(
                    &self.vault_root,
                    &request,
                )?)
            }
            "save_resource" => self.write::<resource_commands::SaveResourceRequest, _>(
                command,
                args,
                true,
                |request, _scope| resource_commands::save_resource(&self.vault_root, request),
            ),
            "delete_resource" => self.write::<resource_commands::ResourceIdRequest, _>(
                command,
                args,
                true,
                |request, _scope| resource_commands::delete_resource(&self.vault_root, request),
            ),
            "restore_resource" => self.write::<resource_commands::ResourceIdRequest, _>(
                command,
                args,
                true,
                |request, _scope| resource_commands::restore_resource(&self.vault_root, request),
            ),
            "set_resource_ref" => self.write::<resource_commands::SetResourceRefRequest, _>(
                command,
                args,
                true,
                |request, _scope| resource_commands::set_resource_ref(&self.vault_root, request),
            ),
            "approve_resource_change" => self.write::<resource_commands::ChangeDecisionRequest, _>(
                command,
                args,
                true,
                |request, _scope| {
                    resource_commands::approve_resource_change(&self.vault_root, request)
                },
            ),
            "reject_resource_change" => self.write::<resource_commands::ChangeDecisionRequest, _>(
                command,
                args,
                true,
                |request, _scope| {
                    resource_commands::reject_resource_change(&self.vault_root, request)
                },
            ),
            "resolve_resource_edit" => self
                .write::<resource_commands::ResolveResourceEditRequest, _>(
                    command,
                    args,
                    true,
                    |request, _scope| {
                        resource_commands::resolve_resource_edit(&self.vault_root, request)
                    },
                ),
            "set_resource_policy" => self.write::<resource_commands::SetResourcePolicyRequest, _>(
                command,
                args,
                true,
                |request, _scope| resource_commands::set_resource_policy(&self.vault_root, request),
            ),
            "get_ask" => {
                let request: ask_chat_commands::NoteRefRequest = parse_args(command, args)?;
                to_json(ask_chat_commands::get_ask(&self.vault_root, &request)?)
            }
            "list_chats" => {
                let request: ask_chat_commands::NoteRefRequest = parse_args(command, args)?;
                to_json(ask_chat_commands::list_chats(&self.vault_root, &request)?)
            }
            "append_ask_turn" => self.write::<ask_chat_commands::AppendAskTurnRequest, _>(
                command,
                args,
                false,
                |request, scope| {
                    ask_chat_commands::append_ask_turn(&self.vault_root, request, scope)
                },
            ),
            "append_chat_turn" => self.write::<ask_chat_commands::AppendChatTurnRequest, _>(
                command,
                args,
                false,
                |request, scope| {
                    ask_chat_commands::append_chat_turn(&self.vault_root, request, scope)
                },
            ),
            "read_session" => {
                let request: session_commands::ReadSessionRequest = parse_args(command, args)?;
                to_json(session_commands::read_session(&request)?)
            }
            "session_titles" => {
                let request: session_commands::SessionTitlesRequest = parse_args(command, args)?;
                to_json(session_commands::session_titles(&request)?)
            }
            "recent_sessions" => {
                let request: session_commands::RecentSessionsRequest = parse_args(command, args)?;
                to_json(session_commands::recent_sessions(&request)?)
            }
            "write_attachment" => self.write::<write_attachment::WriteAttachmentRequest, _>(
                command,
                args,
                false,
                |request, scope| {
                    write_attachment::write_attachment(&self.vault_root, request, scope)
                },
            ),
            "shard_notes" => self.write::<shard_notes::ShardNotesRequest, _>(
                command,
                args,
                false,
                |request, _scope| shard_notes::shard_notes(&self.vault_root, request),
            ),
            "verify_shard" => {
                let request: shard_notes::VerifyShardRequest = parse_args(command, args)?;
                to_json(shard_notes::verify_shard(&self.vault_root, &request)?)
            }
            "plan_day" => self.write::<daily_notes::PlanDayRequest, _>(
                command,
                args,
                false,
                |request, scope| daily_notes::plan_day(&self.vault_root, request, scope),
            ),
            "check_notes" => self.write::<daily_notes::CheckNotesRequest, _>(
                command,
                args,
                false,
                |request, scope| daily_notes::check_notes(&self.vault_root, request, scope),
            ),
            "facts_brief" => self.write::<facts_brief::FactsBriefRequest, _>(
                command,
                args,
                false,
                |request, scope| facts_brief::facts_brief(&self.vault_root, request, scope),
            ),
            "export_markdown" => {
                self.write::<ExportMarkdownRequest, _>(command, args, false, |_request, scope| {
                    export_markdown::export_markdown(&self.vault_root, scope)
                })
            }
            "get_resource_policy" => {
                to_json(resource_commands::get_resource_policy(&self.vault_root)?)
            }
            "delete_doc" => self.write::<trash_write::DeleteDocRequest, _>(
                command,
                args,
                false,
                |request, scope| trash_write::delete_doc(&self.vault_root, request, scope),
            ),
            "restore_doc" => self.write::<trash_write::TrashEntryRequest, _>(
                command,
                args,
                false,
                |request, scope| trash_write::restore_doc(&self.vault_root, request, scope),
            ),
            "purge_doc" => self.write::<trash_write::TrashEntryRequest, _>(
                command,
                args,
                false,
                |request, _scope| trash_write::purge_doc(&self.vault_root, request),
            ),
            "empty_trash" => self.write::<trash_write::EmptyTrashRequest, _>(
                command,
                args,
                false,
                |_request, _scope| trash_write::empty_trash(&self.vault_root),
            ),
            "create_comment" => self.write::<comment_write::CommentWriteRequest, _>(
                command,
                args,
                false,
                |request, scope| comment_write::create_comment(&self.vault_root, request, scope),
            ),
            "update_comment" => self.write::<comment_write::CommentWriteRequest, _>(
                command,
                args,
                false,
                |request, scope| comment_write::update_comment(&self.vault_root, request, scope),
            ),
            "delete_comment" => self.write::<comment_write::CommentWriteRequest, _>(
                command,
                args,
                false,
                |request, scope| comment_write::delete_comment(&self.vault_root, request, scope),
            ),
            "create_sub_note" => self.write::<sub_note_write::CreateSubNoteRequest, _>(
                command,
                args,
                false,
                |request, scope| sub_note_write::create_sub_note(&self.vault_root, request, scope),
            ),
            "update_sub_note" => self.write::<sub_note_write::UpdateSubNoteRequest, _>(
                command,
                args,
                false,
                |request, scope| sub_note_write::update_sub_note(&self.vault_root, request, scope),
            ),
            "delete_sub_note" => self.write::<sub_note_write::DeleteSubNoteRequest, _>(
                command,
                args,
                false,
                |request, scope| sub_note_write::delete_sub_note(&self.vault_root, request, scope),
            ),
            "create_doc" => self.write::<doc_create::CreateDocRequest, _>(
                command,
                args,
                false,
                |request, scope| doc_create::create_doc(&self.vault_root, request, scope),
            ),
            "update_doc" => self.write::<doc_write::UpdateDocRequest, _>(
                command,
                args,
                false,
                |request, scope| doc_write::update_doc(&self.vault_root, request, scope),
            ),
            "commit_vault" => commit_vault::execute(&self.vault_root).and_then(to_json),
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
        "reindex_paths",
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
        "get_task",
        "list_tasks",
        "list_resources",
        "get_resource",
        "usable_resources",
        "list_resource_versions",
        "list_resource_changes",
        "get_resource_change",
        "get_resource_policy",
        "get_ask",
        "list_chats",
        "read_session",
        "session_titles",
        "recent_sessions",
        "verify_shard",
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
