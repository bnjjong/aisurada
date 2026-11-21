//! 후속 노트 쓰기: `create_sub_note`(만들기), `update_sub_note`(제목·본문·태그 고치기), `delete_sub_note`(지우기).
//! 인자 이름, 검사 순서, 오류 문구와 응답 필드는 Java 엔진과 같고, 파일 글은 바이트까지 같다.
//!
//! 부모는 `메인 노트 id`이거나 `메인 노트 id/후속 노트 id/…` 사슬이다. 읽기 쪽과 달리 후속 노트 id 하나만으로는 부모를 가리킬 수 없다.

use crate::kernel::tier::Tier;
use std::path::Path;

use serde::Deserialize;
use serde_json::{Value as Json, json};

use super::Caller;
use super::doc_write::parse_tags;
use super::subnote::check_writable;
use crate::error::EngineError;
use crate::kernel::actor::CallContext;
use crate::kernel::token::compute_token;
use crate::note::front::{Draft, judge};
use crate::note::sub::{NewSub, check_given_title, updated_title};
use crate::text::{NOTE_ID, SUB_NOTE_ID, is_blank, trim};
use crate::vault::index::NoteIndex;
use crate::vault::note_file::split_text;
use crate::vault::scanner::{Format, NoteRecord};
use crate::write::apparatus::{next_sub_note_id, now_millis};
use crate::write::confine::has_unusable_path_characters;
use crate::ports::fs::check_confinement;
use crate::write::doc_place::folder_name_from;
use crate::write::local_time::local_clock;
use crate::write::meta_writer::editor_of;
use crate::write::pipeline::WriteScope;
use crate::write::sub_note::{
    SINGLE_FILE_MESSAGE, place_and_write, read_derived, remove_tree, require_markdown_folder,
    rewrite_root, rewrite_sub, root_agenda, strip_child_cards,
};
use crate::write::{precondition, warnings};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CreateSubNoteRequest {
    /// 부모 참조(`메인 id` 또는 `메인 id/후속 id/…`)
    pub parent: String,
    pub title: Option<String>,
    pub body: Option<String>,
    /// 안 주면 `free`
    #[serde(rename = "type")]
    pub doc_type: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UpdateSubNoteRequest {
    pub parent: String,
    pub id: String,
    pub title: Option<String>,
    pub body: Option<String>,
    /// 전체 교체. 호출 객체에서는 문자열 배열, 명령줄에서는 JSON 배열 글
    pub tags: Option<Json>,
    pub f1: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DeleteSubNoteRequest {
    pub parent: String,
    pub id: String,
}

/// 풀어 낸 부모
struct Parent<'a> {
    root_id: String,
    /// 메인 노트. 없을 수 있다(사슬이 메인 노트 id 하나이고 그 노트가 없을 때)
    root: Option<&'a NoteRecord>,
    /// 후속 노트들이 직접 달려 있는 노트(메인 노트이거나 다른 후속 노트)
    container: Option<&'a NoteRecord>,
    /// 사슬을 다듬어 `/`로 다시 이은 글
    text: String,
}

impl Parent<'_> {
    fn container_id(&self) -> &str {
        self.container
            .map_or(self.root_id.as_str(), |c| c.id.as_str())
    }
}

fn resolve_parent<'a>(
    index: &'a NoteIndex,
    raw: &str,
    caller: Caller,
) -> Result<Parent<'a>, EngineError> {
    let segments: Vec<&str> = raw.split('/').map(trim).filter(|s| !s.is_empty()).collect();
    if segments.is_empty() {
        return Err(EngineError::invalid_argument(format!(
            "부모 노트 참조가 비었다: {raw}"
        )));
    }
    let root_id = segments[0];
    if !NOTE_ID.is_match(root_id) {
        return Err(EngineError::invalid_argument(format!(
            "노트 id 모양이 아니다: {root_id}"
        )));
    }
    let root = index.note(root_id);
    if let Some(root) = root {
        check_writable(root, caller)?;
    }
    let mut container = root;
    let mut previous = root_id;
    for segment in &segments[1..] {
        let sub = direct_child(root, root_id, previous, segment)?;
        previous = segment;
        container = Some(sub);
    }
    Ok(Parent {
        root_id: root_id.to_string(),
        root,
        container,
        text: segments.join("/"),
    })
}

fn resolve_target<'a>(parent: &Parent<'a>, raw_id: &str) -> Result<&'a NoteRecord, EngineError> {
    direct_child(parent.root, &parent.root_id, parent.container_id(), raw_id)
}

fn direct_child<'a>(
    root: Option<&'a NoteRecord>,
    root_id: &str,
    parent_id: &str,
    sub_id: &str,
) -> Result<&'a NoteRecord, EngineError> {
    if !SUB_NOTE_ID.is_match(sub_id) {
        return Err(EngineError::invalid_argument(format!(
            "하위 노트 id 모양이 아니다: {sub_id}"
        )));
    }
    let sub = root
        .and_then(|r| r.sub_notes.iter().find(|s| s.id == sub_id))
        .ok_or_else(|| EngineError::not_found(format!("SubNote not found: {root_id}/{sub_id}")))?;
    if sub.direct_parent_id.as_deref() != Some(parent_id) {
        return Err(EngineError::not_found(format!(
            "SubNote not found: {sub_id}"
        )));
    }
    Ok(sub)
}

fn require_one(parent: &str, usage: &str) -> Result<(), EngineError> {
    if is_blank(parent) {
        return Err(EngineError::invalid_argument(format!(
            "인자가 하나 필요하다: {usage}"
        )));
    }
    Ok(())
}

fn require_two(parent: &str, id: &str, usage: &str) -> Result<(), EngineError> {
    require_one(parent, usage)?;
    if is_blank(id) {
        return Err(EngineError::invalid_argument(format!(
            "인자가 둘 필요하다: {usage}"
        )));
    }
    Ok(())
}

/// 후속 노트 한 건을 메인 노트나 후속 노트 아래에 만든다. 폴더 노트에만 둘 수 있다.
pub fn create_sub_note(
    vault: &Path,
    request: &CreateSubNoteRequest,
    scope: &WriteScope,
    context: &CallContext,
) -> Result<Json, EngineError> {
    const USAGE: &str = "create-sub-note <부모 노트 id> title=<제목> body=<본문> [type=<유형>]";
    require_one(&request.parent, USAGE)?;
    let parent = resolve_parent(&scope.index, &request.parent, context.caller)?;
    let new = NewSub::parse(
        request.title.as_deref(),
        request.body.as_deref(),
        request.doc_type.as_deref(),
        USAGE,
    )?;
    let (title, body) = (new.title.as_str(), new.body.as_str());
    let root = parent
        .root
        .ok_or_else(|| EngineError::not_found(format!("Note not found: {}", parent.root_id)))?;

    let sub_id = next_sub_note_id();
    let now = local_clock(now_millis());
    let created = new.created(&now);
    let agenda = root_agenda(root);
    let container = parent.container.unwrap_or(root);
    if container.is_single_file || container.is_extension_folder {
        return Err(EngineError::internal(format!(
            "{SINGLE_FILE_MESSAGE}{}",
            container.rel_path
        )));
    }
    let folder_rel = format!(
        "{}/sub/{}",
        container.rel_path,
        folder_name_from(&now.date, now.hour, now.minute, title, &sub_id)
    );
    let text = new.render(&sub_id, &agenda, &created, &folder_rel)?;
    if has_unusable_path_characters(&folder_rel) {
        return Err(EngineError::invalid_argument(format!(
            "경로로 쓸 수 없는 값이다: {folder_rel}"
        )));
    }
    check_confinement(vault, &vault.join(&folder_rel))?;
    crate::write::atomic::write_atomic(&vault.join(&folder_rel).join("note.md"), text.as_bytes())?;

    let mut found: Vec<String> = Vec::new();
    found.extend(warnings::detect_indented_blocks_warning(Some(body)));
    found.extend(warnings::detect_title_mismatch_warning(body, title));
    if body.contains('\u{FFFD}') {
        found.push(warnings::broken_char_warning(
            body,
            warnings::count_broken_chars(body),
        ));
    }
    let mut response = json!({
        "id": sub_id,
        "parent": parent.text,
        "ref": format!("{}/{sub_id}", parent.text),
        "title": title,
        "path": folder_rel,
        "precondition": compute_token(text.as_bytes()),
    });
    if !found.is_empty() {
        response["warning"] = json!(found.join("\n"));
    }
    Ok(response)
}

/// 후속 노트의 제목, 본문, 태그를 고치고 정식 폴더로 옮긴다. 본문만 주면 제목은 본문의 첫 헤딩을 따른다.
pub fn update_sub_note(
    vault: &Path,
    request: &UpdateSubNoteRequest,
    scope: &WriteScope,
    context: &CallContext,
) -> Result<Json, EngineError> {
    const USAGE: &str = "update-sub-note <부모 노트 id> <하위 노트 id> [title=<제목>] [body=<본문>] [tags=<JSON 배열. 전체 교체>] [f1=<직전에 읽은 토큰>]";
    let context = &context;
    require_two(&request.parent, &request.id, USAGE)?;
    let parent = resolve_parent(&scope.index, &request.parent, context.caller)?;
    let sub = resolve_target(&parent, &request.id)?;
    let meta = &sub.meta;
    let new_tags = request.tags.as_ref().map(parse_tags).transpose()?;
    let given_title = request.title.as_deref();
    check_given_title(given_title, &sub.id)?;
    let given_body = request.body.as_deref();
    let token = request.f1.as_deref().map(trim).filter(|t| !t.is_empty());

    let title = updated_title(given_title, given_body, sub.format, &meta.title);
    let tags: &[String] = new_tags.as_deref().unwrap_or(&meta.tags);
    let root = parent.root.ok_or_else(|| {
        EngineError::not_found(format!("SubNote not found: {}/{}", parent.root_id, sub.id))
    })?;
    let agenda = root_agenda(root);
    let snippet = if given_body.is_some() {
        "하위 노트 본문을 저장하지"
    } else {
        "하위 노트를 저장하지"
    };
    let content_rel = sub
        .content_file
        .strip_prefix(vault)
        .unwrap_or(&sub.content_file)
        .to_string_lossy()
        .replace('\\', "/");
    let bytes = precondition::check(vault, sub, token, snippet)?;
    let content = String::from_utf8(bytes).map_err(|_| {
        EngineError::internal(format!(
            "노트 본문 파일을 못 읽어 {snippet} 않았다: {content_rel}"
        ))
    })?;
    let split = split_text(&content);
    let Some(raw) = split.frontmatter else {
        return Err(EngineError::internal(format!(
            "프론트매터가 없어 {snippet} 않았다: {content_rel}"
        )));
    };
    let old_body = split.body;
    let body = given_body.unwrap_or(&old_body);
    let text = judge(
        sub.format,
        &Draft {
            meta,
            agenda: &agenda,
            title: &title,
            created: &meta.created,
            session_id: None,
            session_agent: None,
            tier: Tier::Reading,
            tags,
            favorite: false,
            sub_agenda_ids: &[],
            favorite_category: None,
            template: None,
            evaluation: None,
            links: &meta.links,
        },
        body,
        &old_body,
        &raw,
        &editor_of(context),
        crate::write::apparatus::now_millis(),
    )?;
    require_markdown_folder(sub)?;
    let placed = place_and_write(vault, sub, &title, &text)?;

    let mut found: Vec<String> = Vec::new();
    if let Some(given) = given_body {
        found.extend(warnings::detect_indented_blocks_warning(Some(given)));
        let grown = warnings::count_broken_chars(given) as i64
            - warnings::count_broken_chars(&old_body) as i64;
        if grown > 0 {
            found.push(warnings::broken_char_warning(given, grown as usize));
        }
    }
    let mut response = json!({
        "id": sub.id,
        "parent": parent.text,
        "title": title,
        "path": placed.folder_rel,
        "precondition": compute_token(&placed.bytes),
    });
    if !found.is_empty() {
        response["warning"] = json!(found.join("\n"));
    }
    Ok(response)
}

/// 후속 노트를 지운다(그 아래 후속 노트와 첨부까지). 부모 본문에 이 노트를 가리키는 하위 페이지 카드 줄이 있으면 먼저 걷어낸다.
pub fn delete_sub_note(
    vault: &Path,
    request: &DeleteSubNoteRequest,
    scope: &WriteScope,
    context: &CallContext,
) -> Result<Json, EngineError> {
    const USAGE: &str = "delete-sub-note <부모 노트 id> <하위 노트 id>";
    require_two(&request.parent, &request.id, USAGE)?;
    let parent = resolve_parent(&scope.index, &request.parent, context.caller)?;
    let sub = resolve_target(&parent, &request.id)?;
    let container = parent
        .container
        .ok_or_else(|| EngineError::not_found(format!("SubNote not found: {}", sub.id)))?;
    if container.format != Format::Markdown {
        return Err(EngineError::conflict(format!(
            "본문이 markdown 이 아니라 하위 페이지 카드를 걷어낼 수 없다(포맷={}): {}",
            container.format.as_str().to_uppercase(),
            container.id
        )));
    }
    let (body, bytes) = read_derived(&container.content_file);
    let stripped = strip_child_cards(&body, &sub.id);
    let mut sub_rel = sub.rel_path.clone();
    let mut container_tokens: Option<(String, String)> = None;
    if stripped != body {
        let written = if container.is_sub_note {
            let root = parent
                .root
                .ok_or_else(|| EngineError::not_found(format!("SubNote not found: {}", sub.id)))?;
            let placed = rewrite_sub(vault, root, container, &stripped)?;
            sub_rel = format!(
                "{}{}",
                placed.folder_rel,
                &sub_rel[container.rel_path.len()..]
            );
            placed.bytes
        } else {
            rewrite_root(vault, container, &stripped)?
        };
        container_tokens = Some((
            compute_token(bytes.as_deref().unwrap_or_default()),
            compute_token(&written),
        ));
    }
    remove_tree(&vault.join(&sub_rel))?;
    let mut response = json!({
        "id": sub.id,
        "parent": parent.text,
        "deleted": true,
        "path": sub_rel,
        "cardRemoved": container_tokens.is_some(),
    });
    if let Some((before, after)) = container_tokens {
        response["containerBefore"] = json!(before);
        response["containerAfter"] = json!(after);
    }
    Ok(response)
}
