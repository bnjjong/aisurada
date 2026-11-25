//! 노트 본문 쓰기 명령: `update_doc`(제목·본문·태그·템플릿·날짜·시각 고치기와 다른 아젠다로 옮기기).
//! 인자 이름, 검사 순서, 오류 문구와 응답 필드는 Java 엔진과 같고, 파일 글은 바이트까지 같다.
//! 아젠다 옮기기(`agenda`)는 Java 엔진 뒤에 더했다(TSK-2). 이미 있는 아젠다만 받고 아젠다를 새로 만들지 않는다.

mod place;
mod retag;
mod warning;

use crate::ports::fs;
use std::path::Path;

use serde::Deserialize;
use serde_json::{Value as Json, json};

use super::Caller;
use super::doc_create::{AgendaMatch, find_agenda};
use super::note_meta_write::{BoolArg, bool_arg};
use super::subnote::check_writable;
use crate::error::EngineError;
use crate::frontmatter::dump_string;
use crate::kernel::actor::CallContext;
use crate::kernel::token::compute_token;
pub(crate) use crate::note::args::parse_tags;
use crate::note::edit::{EditInput, ParsedEdit};
use crate::note::front::check_size;
use crate::text::{NOTE_ID, is_blank};
use crate::vault::note_file::{split_fence, split_text};
use crate::vault::scanner::{Format, NoteRecord};
use crate::vault::sub_agenda;
use crate::write::atomic::write_atomic;
use crate::write::doc_place::{check_path, folder_name, move_path, parent_folder, place_folder};
use crate::write::meta_writer::editor_of;
use crate::write::pipeline::WriteScope;
use crate::write::{precondition, warnings};

use place::place_and_write;
use retag::retag_sub_notes;
use warning::collect_content_warnings;

/// `update_doc` 요청. `tags`는 호출 객체에서는 문자열 배열, 명령줄에서는 JSON 배열 글로 온다.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UpdateDocRequest {
    pub id: String,
    pub title: Option<String>,
    pub body: Option<String>,
    /// 전체 교체
    pub tags: Option<Json>,
    pub sub_agenda_ids: Option<Json>,
    /// 아젠다 이동을 되돌릴 때 이전에 쓰던 보관 분류도 복원한다.
    pub restore_sub_agendas: Option<BoolArg>,
    pub template: Option<String>,
    /// `YYYY-MM-DD`. AI 호출에서는 받지 않는다.
    pub date: Option<String>,
    /// `HH:MM`. AI 호출에서는 받지 않는다.
    pub time: Option<String>,
    /// 옮길 아젠다의 slug나 이름. 이미 있는 아젠다만 받는다. AI 호출도 받는다.
    pub agenda: Option<String>,
    /// 직전에 읽은 확인 값
    pub f1: Option<String>,
}

/// 노트 id가 있고 모양이 맞는지 본다.
pub(crate) fn require_note_id(id: &str, usage: &str) -> Result<(), EngineError> {
    if is_blank(id) {
        return Err(EngineError::invalid_argument(format!(
            "인자가 하나 필요하다: {usage}"
        )));
    }
    if !NOTE_ID.is_match(id) {
        return Err(EngineError::invalid_argument(format!(
            "노트 id 모양이 아니다: {id}"
        )));
    }
    Ok(())
}

fn rel_of(vault: &Path, path: &Path) -> String {
    path.strip_prefix(vault)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// 노트 한 건의 제목, 본문, 태그, 템플릿, 날짜, 시각, 아젠다를 고치고, 그 값으로 다시 계산한 정식 자리로 노트를 옮긴다.
pub fn update_doc(
    vault: &Path,
    request: &UpdateDocRequest,
    scope: &WriteScope,
    context: &CallContext,
) -> Result<Json, EngineError> {
    const USAGE: &str = "update-doc <노트 id> [title=<제목>] [body=<본문>] [tags=<JSON 배열. 전체 교체>] [template=<템플릿 id>] [date=<YYYY-MM-DD>] [time=<HH:MM>] [agenda=<옮길 아젠다의 slug 또는 이름. 이미 있는 아젠다만>] [f1=<직전에 읽은 토큰>]";
    let context = &context;

    require_note_id(&request.id, USAGE)?;
    let note_id = request.id.as_str();
    let new_tags = request.tags.as_ref().map(parse_tags).transpose()?;
    // 인자 검사: AI 도구 호출은 날짜와 시각을 받지 않고, 제목과 옮길 아젠다는 비면 안 된다(규칙은 `note::edit`)
    let edit = ParsedEdit::parse(EditInput {
        note_id,
        title: request.title.as_deref(),
        body: request.body.as_deref(),
        tags: new_tags,
        template: request.template.as_deref(),
        date: request.date.as_deref(),
        time: request.time.as_deref(),
        agenda: request.agenda.as_deref(),
        f1: request.f1.as_deref(),
        tool: context.caller == Caller::Ai,
    })?;
    let given_body = edit.given_body();

    let note: &NoteRecord = scope
        .index
        .note(note_id)
        .ok_or_else(|| EngineError::not_found(format!("Note not found: {note_id}")))?;
    check_writable(note, context.caller)?;
    let meta = &note.meta;

    // 새 제목·날짜·시각. 못 읽는 created와 JSON 노트의 본문은 파일을 읽기 전에 선다
    let mut slot = edit.slot(meta, note.format, note_id)?;

    let bytes = precondition::check(vault, note, edit.token(), "고치지")?;
    // 옮길 아젠다: 이미 있는 아젠다만 받는다. 이 명령은 아젠다를 만들지 않는다(사용자 결정 2026-11-20, TSK-2).
    // AI가 오타 이름으로 아젠다를 새로 만드는 일을 막으려는 것이다. 새 아젠다로 옮기려면 앱이 `create-agenda`를 먼저 부른다.
    let moved_to = match edit.agenda_key() {
        None => None,
        Some(key) => match find_agenda(scope, key)? {
            AgendaMatch::Found(slug) => {
                slot.move_to(&slug);
                Some(slug)
            }
            AgendaMatch::Missing(_) => {
                return Err(EngineError::not_found(format!(
                    "아젠다가 없어 옮기지 않았다: {key}. update_doc 은 아젠다를 새로 만들지 않는다. 이미 있는 아젠다의 slug 나 이름을 준다(list_docs·get_doc 응답의 agenda 값)"
                )));
            }
        },
    };
    let agenda_changed = moved_to
        .as_ref()
        .is_some_and(|slug| slug != &meta.agenda_slug);
    let restoring_move =
        bool_arg("restoreSubAgendas", &request.restore_sub_agendas)?.unwrap_or(false);
    if restoring_move && (!agenda_changed || context.caller != Caller::App) {
        return Err(EngineError::invalid_argument(
            "아젠다 이동을 되돌릴 때만 분류를 복원할 수 있다",
        ));
    }
    if let Some(raw) = &request.sub_agenda_ids {
        let ids = sub_agenda::parse_ids(raw)?;
        let slug = moved_to.as_deref().unwrap_or(&meta.agenda_slug);
        let previous = if agenda_changed {
            &[]
        } else {
            meta.sub_agenda_ids.as_slice()
        };
        sub_agenda::validate(
            &ids,
            scope.index.sub_agendas(slug),
            previous,
            restoring_move,
        )?;
        slot.sub_agenda_ids = ids;
    } else if agenda_changed {
        slot.sub_agenda_ids.clear();
    }
    let content_rel = rel_of(vault, &note.content_file);
    let content = String::from_utf8(bytes).map_err(|_| {
        EngineError::internal(format!(
            "노트 본문 파일을 못 읽어 고치지 않았다: {content_rel}"
        ))
    })?;
    let split = split_text(&content);
    let Some(raw) = split.frontmatter else {
        return Err(EngineError::internal(format!(
            "프론트매터가 없어 고치지 않았다: {content_rel}"
        )));
    };
    let old_body = split.body;

    // 새 파일 글(편집 도장 판정 포함). 지금 시각과 편집자는 여기서 정해 넘긴다
    let old_rel = note.rel_path.as_str();
    let rendered = slot.render(
        note.format,
        meta,
        old_rel,
        &raw,
        &old_body,
        given_body,
        &editor_of(context),
        crate::write::apparatus::now_millis(),
    )?;
    let (text, agenda, body) = (&rendered.text, &rendered.agenda, rendered.body.as_str());
    let (title, date, hour, minute) = (slot.title.as_str(), &slot.date, slot.hour, slot.minute);

    // 자리 갈래: 부모 폴더와 폴더 이름
    let parent = parent_folder(vault, agenda, date);
    let folder = folder_name(date, hour, minute, title, note_id);

    let mut warnings_found: Vec<String> = Vec::new();

    // 정식 자리에 파일 쓰기 및 이전 파일 정리
    let final_rel = place_and_write(
        vault,
        note,
        &parent,
        &folder,
        text,
        agenda_changed,
        &mut warnings_found,
    )?;

    // 옮긴 폴더 노트의 후속 노트 머리를 새 아젠다로 맞춘다(옛 단일 파일 노트에는 후속 노트가 없다)
    if let (Some(slug), false) = (&moved_to, note.is_single_file) {
        retag_sub_notes(
            vault,
            &vault.join(old_rel),
            &final_rel,
            &note.sub_notes,
            slug,
            &mut warnings_found,
        );
    }

    // 경고: 못 치운 옛 자리, 후속 노트 머리, 들여쓴 블록, 제목과 첫 헤딩, U+FFFD 차례
    collect_content_warnings(
        note.format,
        given_body,
        &old_body,
        body,
        title,
        &mut warnings_found,
    );

    let mut response = json!({
        "id": note_id,
        "path": final_rel,
        "precondition": compute_token(text.as_bytes()),
        "renamed": final_rel != old_rel,
    });
    if let Some(slug) = &moved_to {
        response["agenda"] = json!(slug);
    }
    if request.sub_agenda_ids.is_some() || agenda_changed && !meta.sub_agenda_ids.is_empty() {
        response["subAgendaIds"] = json!(slot.sub_agenda_ids);
    }
    if agenda_changed && !meta.sub_agenda_ids.is_empty() {
        response["removedSubAgendaIds"] = json!(meta.sub_agenda_ids);
    }
    if !warnings_found.is_empty() {
        response["warning"] = json!(warnings_found.join("\n"));
    }
    Ok(response)
}
