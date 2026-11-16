//! 노트 본문 쓰기 명령: `update_doc`(제목·본문·태그·템플릿·날짜·시각 고치기).
//! 인자 이름, 검사 순서, 오류 문구와 응답 필드는 Java 엔진과 같고, 파일 글은 바이트까지 같다.

use crate::ports::fs;
use std::path::Path;

use serde::Deserialize;
use serde_json::{Value as Json, json};

use super::Caller;
use super::subnote::check_writable;
use crate::error::EngineError;
use crate::kernel::actor::CallContext;
use crate::kernel::token::compute_token;
pub(crate) use crate::note::args::parse_tags;
use crate::note::edit::{EditInput, ParsedEdit};
use crate::note::front::check_size;
use crate::text::{NOTE_ID, is_blank};
use crate::vault::note_file::split_text;
use crate::vault::scanner::{Format, NoteRecord};
use crate::write::atomic::write_atomic;
use crate::write::doc_place::{check_path, folder_name, parent_folder, place_folder};
use crate::write::meta_writer::editor_of;
use crate::write::pipeline::WriteScope;
use crate::write::{precondition, warnings};

/// `update_doc` 요청. `tags`는 호출 객체에서는 문자열 배열, 명령줄에서는 JSON 배열 글로 온다.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UpdateDocRequest {
    pub id: String,
    pub title: Option<String>,
    pub body: Option<String>,
    /// 전체 교체
    pub tags: Option<Json>,
    pub template: Option<String>,
    /// `YYYY-MM-DD`. AI 호출에서는 받지 않는다.
    pub date: Option<String>,
    /// `HH:MM`. AI 호출에서는 받지 않는다.
    pub time: Option<String>,
    /// 직전에 읽은 확인 값
    pub f1: Option<String>,
    #[serde(flatten)]
    pub context: CallContext,
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

/// 노트 한 건의 제목, 본문, 태그, 템플릿, 날짜, 시각을 고치고, 그 값으로 다시 계산한 정식 자리로 노트를 옮긴다.
pub fn update_doc(
    vault: &Path,
    request: &UpdateDocRequest,
    scope: &WriteScope,
) -> Result<Json, EngineError> {
    const USAGE: &str = "update-doc <노트 id> [title=<제목>] [body=<본문>] [tags=<JSON 배열. 전체 교체>] [template=<템플릿 id>] [date=<YYYY-MM-DD>] [time=<HH:MM>] [f1=<직전에 읽은 토큰>]";
    let context = &request.context;

    require_note_id(&request.id, USAGE)?;
    let note_id = request.id.as_str();
    let new_tags = request.tags.as_ref().map(parse_tags).transpose()?;
    // 인자 검사: AI 도구 호출은 날짜와 시각을 받지 않고, 제목은 비면 안 된다(규칙은 `note::edit`)
    let edit = ParsedEdit::parse(EditInput {
        note_id,
        title: request.title.as_deref(),
        body: request.body.as_deref(),
        tags: new_tags,
        template: request.template.as_deref(),
        date: request.date.as_deref(),
        time: request.time.as_deref(),
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
    let slot = edit.slot(meta, note.format, note_id)?;

    let bytes = precondition::check(vault, note, edit.token(), "고치지")?;
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
    let content_name = note
        .content_file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let check_text_size = |rel: &str| check_size(&text, rel);

    let mut warnings_found: Vec<String> = Vec::new();
    let final_rel: String;
    if !note.is_single_file {
        // 폴더 노트: 경로 검사, 빈자리 고르기, 옮기기 뒤에 상한을 보고 쓴다
        let canonical = format!("{parent}/{folder}");
        check_path(vault, &canonical)?;
        final_rel = place_folder(vault, old_rel, &canonical)?;
        check_text_size(&final_rel)?;
        write_atomic(&vault.join(&final_rel).join(&content_name), text.as_bytes())?;
    } else if note.format == Format::Html {
        // 옛 .html 단일 파일: 이름만 바꾼다. 상한, 경로, 쓰기 차례이고 같은 이름 파일은 덮는다
        final_rel = format!("{parent}/{folder}.html");
        check_text_size(&final_rel)?;
        check_path(vault, &final_rel)?;
        write_atomic(&vault.join(&final_rel), text.as_bytes())?;
        if final_rel != old_rel {
            remove_old(vault, old_rel, &mut warnings_found);
        }
    } else {
        // 옛 .md, .json 단일 파일: 빈자리를 안 보고 새 폴더에 쓴 뒤 옛 파일을 지운다
        final_rel = format!("{parent}/{folder}");
        check_text_size(&final_rel)?;
        check_path(vault, &final_rel)?;
        let name = if note.format == Format::Json {
            "note.json"
        } else {
            "note.md"
        };
        write_atomic(&vault.join(&final_rel).join(name), text.as_bytes())?;
        remove_old(vault, old_rel, &mut warnings_found);
    }

    // 경고: 못 치운 옛 자리, 들여쓴 블록, 제목과 첫 헤딩, U+FFFD 차례
    if let Some(given) = given_body.filter(|_| note.format != Format::Html) {
        warnings_found.extend(warnings::detect_indented_blocks_warning(Some(given)));
    }
    if note.format == Format::Markdown {
        warnings_found.extend(warnings::detect_title_mismatch_warning(body, title));
    }
    if let Some(given) = given_body {
        let grown = warnings::count_broken_chars(given) as i64
            - warnings::count_broken_chars(&old_body) as i64;
        if grown > 0 {
            warnings_found.push(warnings::broken_char_warning(given, grown as usize));
        }
    }

    let mut response = json!({
        "id": note_id,
        "path": final_rel,
        "precondition": compute_token(text.as_bytes()),
        "renamed": final_rel != old_rel,
    });
    if !warnings_found.is_empty() {
        response["warning"] = json!(warnings_found.join("\n"));
    }
    Ok(response)
}

/// 옛 단일 파일을 지운다. 못 지우면 응답을 실패로 바꾸지 않고 경고로 알린다.
fn remove_old(vault: &Path, old_rel: &str, warnings: &mut Vec<String>) {
    match fs::remove_file(vault.join(old_rel)) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => warnings.push(format!(
            "옛 노트 삭제 실패 — 같은 id가 둘 남았을 수 있어요(정리 필요): {old_rel}"
        )),
    }
}
