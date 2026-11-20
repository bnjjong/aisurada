//! 명령 표. 엔진이 받는 명령은 모두 이 표의 한 줄이다.
//!
//! 한 줄에 **이름, 읽기·쓰기, 쓰기 범위를 여는 방식, 요청 종류, 처리 함수**가 한꺼번에 적힌다. 명령 목록, 쓰기 목록, 디스패처는
//! 이 표 하나에서 나온다. 명령을 더할 때 고칠 곳은 이 표 한 줄과 처리 함수다(명령줄·MCP에 내놓는 명령이면 그쪽 표에도 한 줄이 든다).
//!
//! 읽기 명령은 요청을 읽고 처리 함수를 부르기만 한다. 쓰기 명령은 요청을 읽은 뒤 **이 표가 정한 방식으로 범위를 열고**
//! (잠금 → 표식·색인 맞추기 → 자동 커밋 기록), 처리 함수에 열린 범위를 넘긴다. 범위는 응답을 만든 뒤에 놓인다.

use serde_json::Value;

use super::{Engine, finish, parse_args};
use crate::commands::{
    agenda_write, ask_chat_commands, comment_write, commit_vault, daily_notes, decisions,
    doc_create, doc_write, export_markdown, facts_brief, facts_write, get_agenda_facts, get_doc,
    get_sub_note, get_template, history, init_workspace, list_agendas, list_attachments,
    list_comments, list_docs, list_projects, list_sub_notes, list_tags, list_templates, list_trash,
    note_meta_write, project_write, reindex, resource_commands, search_docs, session_commands,
    shard_notes, sub_agenda_write, sub_note_write, task_commands, template_write, trash_write,
    write_attachment,
};
use crate::error::EngineError;
use crate::kernel::actor::CallContext;
use crate::vault::index::NoteIndex;

/// 명령이 vault를 고치는가
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
}

/// 쓰기 범위를 여는 방식. 범위를 여는 일은 잠금, 표식·색인 맞추기, 자동 커밋 기록의 세 조각이다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// 범위를 열지 않는다(읽기 명령과, 자기 길로 커밋하는 `commit_vault`)
    None,
    /// 세 조각을 모두 연다
    Standard,
    /// 세 조각에 더해, 범위를 연 직후 승인 기록장이 없으면 기준선을 만든다. 쓰기보다 먼저여야 방금 쓴 파일이 기준선에 섞이지 않는다.
    WithLedger,
    /// 워크스페이스 표식 검사를 건너뛴다. 표식을 처음 만드는 `init_workspace`는 표식이 아직 없는 폴더에서 부른다.
    NoMarker,
}

/// 표의 한 줄. 처리 함수는 요청을 읽는 일까지 맡아 응답 JSON을 돌려준다.
pub(super) struct Def {
    pub(super) name: &'static str,
    pub(super) access: Access,
    pub(super) scope: Scope,
    pub(super) run: fn(&Engine, &CallContext, Value) -> Result<Value, EngineError>,
}

/// 읽기 명령 한 줄. 요청 종류를 주면 인자를 그 모양으로 읽고, 안 주면 인자를 보지 않는다. 마지막 이름을 하나 더 주면 호출 맥락(`&CallContext`)이 거기 담긴다.
macro_rules! read {
    ($name:literal, $req:ty, |$root:ident, $request:ident, $ctx:ident| $body:expr) => {
        Def {
            name: $name,
            access: Access::Read,
            scope: Scope::None,
            run: |engine: &Engine, call_context: &CallContext, args: Value| {
                let parsed: $req = parse_args($name, args)?;
                let $request = &parsed;
                let $ctx = call_context;
                let $root = engine.vault_root.as_path();
                finish($body)
            },
        }
    };
    ($name:literal, $req:ty, |$root:ident, $request:ident| $body:expr) => {
        Def {
            name: $name,
            access: Access::Read,
            scope: Scope::None,
            run: |engine: &Engine, _call_context: &CallContext, args: Value| {
                let parsed: $req = parse_args($name, args)?;
                let $request = &parsed;
                let $root = engine.vault_root.as_path();
                finish($body)
            },
        }
    };
    ($name:literal, |$root:ident| $body:expr) => {
        Def {
            name: $name,
            access: Access::Read,
            scope: Scope::None,
            run: |engine: &Engine, _call_context: &CallContext, _args: Value| {
                let $root = engine.vault_root.as_path();
                finish($body)
            },
        }
    };
}

/// 쓰기 명령 한 줄. 요청을 읽고, 표가 정한 방식(`$scope`)으로 범위를 열고, 처리 함수에 요청과 열린 범위를 넘긴다.
/// 범위를 여는 데는 호출 맥락을 쓴다. 처리 함수도 맥락이 필요하면 마지막 이름을 하나 더 준다.
macro_rules! write {
    ($name:literal, $scope:ident, $req:ty, |$root:ident, $request:ident, $open:ident, $ctx:ident| $body:expr) => {
        Def {
            name: $name,
            access: Access::Write,
            scope: Scope::$scope,
            run: |engine: &Engine, call_context: &CallContext, args: Value| {
                let parsed: $req = parse_args($name, args)?;
                let scope = engine.open_scope(Scope::$scope, call_context)?;
                let $request = &parsed;
                let $open = &scope;
                let $ctx = call_context;
                let $root = engine.vault_root.as_path();
                finish($body)
            },
        }
    };
    ($name:literal, $scope:ident, $req:ty, |$root:ident, $request:ident, $open:ident| $body:expr) => {
        Def {
            name: $name,
            access: Access::Write,
            scope: Scope::$scope,
            run: |engine: &Engine, call_context: &CallContext, args: Value| {
                let parsed: $req = parse_args($name, args)?;
                let scope = engine.open_scope(Scope::$scope, call_context)?;
                let $request = &parsed;
                let $open = &scope;
                let $root = engine.vault_root.as_path();
                finish($body)
            },
        }
    };
}

/// 쓰기 명령이지만 범위를 열지 않는 한 줄(`commit_vault`는 자기 길로 vault를 커밋한다)
macro_rules! write_unscoped {
    ($name:literal, |$root:ident| $body:expr) => {
        Def {
            name: $name,
            access: Access::Write,
            scope: Scope::None,
            run: |engine: &Engine, _call_context: &CallContext, _args: Value| {
                let $root = engine.vault_root.as_path();
                finish($body)
            },
        }
    };
}

/// `export_markdown`은 인자가 없다.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct ExportMarkdownRequest {}

/// 이 엔진이 처리하는 명령 전부. 차례는 앱에 알리는 목록의 차례다.
#[rustfmt::skip]
pub(super) static TABLE: &[Def] = &[
    // ── 읽기: 목록과 조회 ──
    read!("list_docs", list_docs::ListDocsRequest, |root, request, ctx| list_docs::execute(&NoteIndex::fresh(root), request, ctx)),
    read!("list_agendas", |root| Ok(list_agendas::execute(&NoteIndex::fresh(root)))),
    read!("list_projects", |root| Ok(list_projects::execute(root))),
    read!("list_templates", list_templates::ListTemplatesRequest, |root, request| list_templates::execute(root, request)),
    read!("list_trash", list_trash::ListTrashRequest, |root, request, ctx| Ok(list_trash::execute(root, request, ctx))),
    read!("reindex", |root| reindex::execute(root)),
    read!("reindex_paths", reindex::ReindexPathsRequest, |root, request| reindex::execute_paths(root, request)),
    read!("get_doc", get_doc::GetDocRequest, |root, request, ctx| get_doc::execute(&NoteIndex::fresh(root), request, ctx)),
    read!("list_sub_notes", list_sub_notes::ListSubNotesRequest, |root, request, ctx| list_sub_notes::execute(&NoteIndex::fresh(root), request, ctx)),
    read!("get_sub_note", get_sub_note::GetSubNoteRequest, |root, request, ctx| get_sub_note::execute(&NoteIndex::fresh(root), request, ctx)),
    read!("list_comments", list_comments::ListCommentsRequest, |root, request, ctx| list_comments::execute(&NoteIndex::fresh(root), request, ctx)),
    read!("list_attachments", list_attachments::ListAttachmentsRequest, |root, request, ctx| list_attachments::execute(&NoteIndex::fresh(root), request, ctx)),
    read!("list_decisions", decisions::ListDecisionsRequest, |root, request, ctx| decisions::list(&NoteIndex::fresh(root), request, ctx)),
    read!("pending_decisions", decisions::PendingDecisionsRequest, |root, _request, ctx| Ok::<_, EngineError>(decisions::pending(&NoteIndex::fresh(root), ctx))),
    read!("get_agenda_facts", get_agenda_facts::GetAgendaFactsRequest, |root, request| get_agenda_facts::execute(&NoteIndex::fresh(root), request)),
    read!("search_docs", search_docs::SearchDocsRequest, |root, request, ctx| search_docs::execute(&NoteIndex::fresh(root), request, ctx)),
    read!("list_tags", list_tags::ListTagsRequest, |root, request| list_tags::execute(&NoteIndex::fresh(root), request)),
    read!("get_template", get_template::GetTemplateRequest, |root, request| get_template::execute(&NoteIndex::fresh(root), request)),
    read!("history", history::HistoryRequest, |root, request| history::execute(root, request)),
    write_unscoped!("commit_vault", |root| commit_vault::execute(root)),
    // ── 쓰기: 아젠다·프로젝트·템플릿·사실 ──
    write!("create_agenda", Standard, agenda_write::CreateAgendaRequest, |root, request, scope| agenda_write::create(root, request, scope)),
    write!("update_agenda", Standard, agenda_write::UpdateAgendaRequest, |root, request, scope| agenda_write::update(root, request, scope)),
    write!("delete_agenda", Standard, agenda_write::DeleteAgendaRequest, |root, request, scope| agenda_write::delete(root, request, scope)),
    write!("create_sub_agenda", Standard, sub_agenda_write::CreateRequest, |root, request, scope| sub_agenda_write::create(root, request, scope)),
    write!("update_sub_agenda", Standard, sub_agenda_write::UpdateRequest, |root, request, scope| sub_agenda_write::update(root, request, scope)),
    write!("create_project", Standard, project_write::CreateProjectRequest, |root, request, _scope| project_write::create(root, request)),
    write!("update_project", Standard, project_write::UpdateProjectRequest, |root, request, _scope| project_write::update(root, request)),
    write!("delete_project", Standard, project_write::DeleteProjectRequest, |root, request, scope| project_write::delete(root, request, scope)),
    write!("save_template", Standard, template_write::SaveTemplateRequest, |root, request, _scope| template_write::save_document(root, request)),
    write!("save_ritual_template", Standard, template_write::SaveRitualTemplateRequest, |root, request, _scope| template_write::save_ritual(root, request)),
    write!("delete_doc_template", Standard, template_write::DeleteDocTemplateRequest, |root, request, _scope| template_write::delete_document(root, request)),
    write!("update_agenda_facts", Standard, facts_write::UpdateAgendaFactsRequest, |root, request, scope| facts_write::update(root, request, scope)),
    write!("init_workspace", NoMarker, init_workspace::InitWorkspaceRequest, |root, request, _scope| init_workspace::execute(root, request)),
    // ── 쓰기: 노트 ──
    write!("set_favorite", Standard, note_meta_write::SetFavoriteRequest, |root, request, scope, ctx| note_meta_write::set_favorite(root, request, scope, ctx)),
    write!("evaluate_doc", Standard, note_meta_write::EvaluateDocRequest, |root, request, scope, ctx| note_meta_write::evaluate_doc(root, request, scope, ctx)),
    write!("link_docs", Standard, note_meta_write::LinkDocsRequest, |root, request, scope, ctx| note_meta_write::link_docs(root, request, scope, ctx)),
    write!("update_doc", Standard, doc_write::UpdateDocRequest, |root, request, scope, ctx| doc_write::update_doc(root, request, scope, ctx)),
    write!("create_doc", Standard, doc_create::CreateDocRequest, |root, request, scope, ctx| doc_create::create_doc(root, request, scope, ctx)),
    write!("create_sub_note", Standard, sub_note_write::CreateSubNoteRequest, |root, request, scope, ctx| sub_note_write::create_sub_note(root, request, scope, ctx)),
    write!("update_sub_note", Standard, sub_note_write::UpdateSubNoteRequest, |root, request, scope, ctx| sub_note_write::update_sub_note(root, request, scope, ctx)),
    write!("delete_sub_note", Standard, sub_note_write::DeleteSubNoteRequest, |root, request, scope, ctx| sub_note_write::delete_sub_note(root, request, scope, ctx)),
    write!("create_comment", Standard, comment_write::CommentWriteRequest, |root, request, scope, ctx| comment_write::create_comment(root, request, scope, ctx)),
    write!("update_comment", Standard, comment_write::CommentWriteRequest, |root, request, scope, ctx| comment_write::update_comment(root, request, scope, ctx)),
    write!("delete_comment", Standard, comment_write::CommentWriteRequest, |root, request, scope, ctx| comment_write::delete_comment(root, request, scope, ctx)),
    write!("delete_doc", Standard, trash_write::DeleteDocRequest, |root, request, scope, ctx| trash_write::delete_doc(root, request, scope, ctx)),
    write!("restore_doc", Standard, trash_write::TrashEntryRequest, |root, request, scope, ctx| trash_write::restore_doc(root, request, scope, ctx)),
    write!("purge_doc", Standard, trash_write::TrashEntryRequest, |root, request, _scope| trash_write::purge_doc(root, request)),
    write!("empty_trash", Standard, trash_write::EmptyTrashRequest, |root, _request, _scope| trash_write::empty_trash(root)),
    // ── 태스크 ──
    read!("get_task", task_commands::GetTaskRequest, |root, request| task_commands::get_task(root, request)),
    read!("list_tasks", task_commands::ListTasksRequest, |root, request| task_commands::list_tasks(root, request)),
    write!("create_task", Standard, task_commands::CreateTaskRequest, |root, request, _scope, ctx| task_commands::create_task(root, request, ctx)),
    write!("update_task", Standard, task_commands::UpdateTaskRequest, |root, request, _scope, ctx| task_commands::update_task(root, request, ctx)),
    write!("transition_task", Standard, task_commands::TransitionTaskRequest, |root, request, _scope, ctx| task_commands::transition_task(root, request, ctx)),
    write!("link_tasks", Standard, task_commands::LinkTasksRequest, |root, request, _scope, ctx| task_commands::link_tasks(root, request, ctx)),
    write!("add_task_comment", Standard, task_commands::AddTaskCommentRequest, |root, request, _scope, ctx| task_commands::add_task_comment(root, request, ctx)),
    write!("delete_task", Standard, task_commands::TaskRefRequest, |root, request, _scope| task_commands::delete_task(root, request)),
    write!("restore_task", Standard, task_commands::TaskRefRequest, |root, request, _scope| task_commands::restore_task(root, request)),
    // ── 자원: 읽기 ──
    read!("list_resources", resource_commands::ListResourcesRequest, |root, request| resource_commands::list_resources(root, request)),
    read!("get_resource", resource_commands::GetResourceRequest, |root, request| resource_commands::get_resource(root, request)),
    read!("usable_resources", resource_commands::UsableResourcesRequest, |root, request| resource_commands::usable_resources(root, request)),
    read!("list_resource_versions", resource_commands::ResourceVersionsRequest, |root, request| resource_commands::list_resource_versions(root, request)),
    read!("list_resource_changes", resource_commands::ListResourceChangesRequest, |root, request| resource_commands::list_resource_changes(root, request)),
    read!("get_resource_change", resource_commands::ResourceChangeRequest, |root, request| resource_commands::get_resource_change(root, request)),
    read!("get_resource_policy", |root| resource_commands::get_resource_policy(root)),
    // ── 자원: 쓰기(승인 기록장 기준선을 먼저 만든다) ──
    write!("save_resource", WithLedger, resource_commands::SaveResourceRequest, |root, request, _scope, ctx| resource_commands::save_resource(root, request, ctx)),
    write!("delete_resource", WithLedger, resource_commands::ResourceIdRequest, |root, request, _scope, ctx| resource_commands::delete_resource(root, request, ctx)),
    write!("restore_resource", WithLedger, resource_commands::ResourceIdRequest, |root, request, _scope, ctx| resource_commands::restore_resource(root, request, ctx)),
    write!("set_resource_ref", WithLedger, resource_commands::SetResourceRefRequest, |root, request, _scope, ctx| resource_commands::set_resource_ref(root, request, ctx)),
    write!("approve_resource_change", WithLedger, resource_commands::ChangeDecisionRequest, |root, request, _scope, ctx| resource_commands::approve_resource_change(root, request, ctx)),
    write!("reject_resource_change", WithLedger, resource_commands::ChangeDecisionRequest, |root, request, _scope, ctx| resource_commands::reject_resource_change(root, request, ctx)),
    write!("resolve_resource_edit", WithLedger, resource_commands::ResolveResourceEditRequest, |root, request, _scope, ctx| resource_commands::resolve_resource_edit(root, request, ctx)),
    write!("set_resource_policy", WithLedger, resource_commands::SetResourcePolicyRequest, |root, request, _scope, ctx| resource_commands::set_resource_policy(root, request, ctx)),
    // ── 묻기·대화·세션 ──
    read!("get_ask", ask_chat_commands::NoteRefRequest, |root, request| ask_chat_commands::get_ask(root, request)),
    read!("list_chats", ask_chat_commands::NoteRefRequest, |root, request| ask_chat_commands::list_chats(root, request)),
    write!("append_ask_turn", Standard, ask_chat_commands::AppendAskTurnRequest, |root, request, scope| ask_chat_commands::append_ask_turn(root, request, scope)),
    write!("append_chat_turn", Standard, ask_chat_commands::AppendChatTurnRequest, |root, request, scope| ask_chat_commands::append_chat_turn(root, request, scope)),
    read!("read_session", session_commands::ReadSessionRequest, |_root, request| session_commands::read_session(request)),
    read!("session_titles", session_commands::SessionTitlesRequest, |_root, request| session_commands::session_titles(request)),
    read!("recent_sessions", session_commands::RecentSessionsRequest, |_root, request| session_commands::recent_sessions(request)),
    // ── 첨부·내보내기·정리·하루 계획 ──
    write!("write_attachment", Standard, write_attachment::WriteAttachmentRequest, |root, request, scope, ctx| write_attachment::write_attachment(root, request, scope, ctx)),
    write!("export_markdown", Standard, ExportMarkdownRequest, |root, _request, scope| export_markdown::export_markdown(root, scope)),
    write!("shard_notes", Standard, shard_notes::ShardNotesRequest, |root, request, _scope| shard_notes::shard_notes(root, request)),
    read!("verify_shard", shard_notes::VerifyShardRequest, |root, request| shard_notes::verify_shard(root, request)),
    write!("plan_day", Standard, daily_notes::PlanDayRequest, |root, request, scope, ctx| daily_notes::plan_day(root, request, scope, ctx)),
    write!("check_notes", Standard, daily_notes::CheckNotesRequest, |root, request, scope| daily_notes::check_notes(root, request, scope)),
    write!("facts_brief", Standard, facts_brief::FactsBriefRequest, |root, request, scope| facts_brief::facts_brief(root, request, scope)),
];
