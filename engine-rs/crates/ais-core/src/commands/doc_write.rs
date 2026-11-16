//! 노트 본문 쓰기 명령: `update_doc`(제목·본문·태그·템플릿·날짜·시각 고치기).
//! 인자 이름, 검사 순서, 오류 문구와 응답 필드는 Java 엔진과 같고, 파일 글은 바이트까지 같다.

use crate::kernel::java::to_json_value;
use crate::ports::fs;
use std::path::Path;

use serde::Deserialize;
use serde_json::{Value as Json, json};

use super::Caller;
use super::subnote::check_writable;
use crate::error::EngineError;
use crate::frontmatter::Value;
use crate::json::{self, JsonValue};
use crate::kernel::token::compute_token;
use crate::text::{NOTE_ID, cut_utf16, is_blank, trim, utf16_len};
use crate::vault::note_file::split_text;
use crate::vault::note_meta::{TEMPLATE_ID, get};
use crate::vault::scanner::{Format, NoteRecord};
use crate::write::atomic::write_atomic;
use crate::write::doc_place::{
    check_path, folder_name, parent_folder, parse_date_arg, parse_time_arg, place_folder,
    read_created,
};
use crate::write::meta_writer::{Draft, MAX_FILE_UNITS, judge};
use crate::write::pipeline::{WriteContext, WriteScope};
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
    pub context: WriteContext,
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

/// `tags` 인자를 읽어 다듬는다. 공백뿐인 항목은 버리고, 다듬은 뒤 줄바꿈이 있으면 거절하고, 같은 글은 처음 하나만 남긴다.
pub(crate) fn parse_tags(raw: &Json) -> Result<Vec<String>, EngineError> {
    let items = match raw {
        Json::String(text) => {
            let not_array =
                || EngineError::invalid_argument(format!("tags 은 JSON 배열이어야 한다: {text}"));
            match json::parse(text).map_err(|_| not_array())? {
                JsonValue::Array(items) => items,
                _ => return Err(not_array()),
            }
        }
        Json::Array(items) => items.iter().map(to_json_value).collect(),
        other => {
            return Err(EngineError::invalid_argument(format!(
                "tags 은 JSON 배열이어야 한다: {other}"
            )));
        }
    };
    let mut result: Vec<String> = Vec::new();
    for item in items {
        let JsonValue::Str(text) = item else {
            return Err(EngineError::invalid_argument(format!(
                "tags 항목이 문자열이 아니다: {}",
                json::compact(&item)
            )));
        };
        if is_blank(&text) {
            continue;
        }
        let trimmed = trim(&text);
        if trimmed.contains(['\n', '\r']) {
            return Err(EngineError::invalid_argument(format!(
                "태그에 줄바꿈이 있다: '{trimmed}'"
            )));
        }
        if !result.iter().any(|seen| seen == trimmed) {
            result.push(trimmed.to_string());
        }
    }
    Ok(result)
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
    // AI 도구 호출은 날짜와 시각을 받지 않는다
    let tool = context.caller == Caller::Ai;
    let given_date = request
        .date
        .as_deref()
        .filter(|_| !tool)
        .map(parse_date_arg)
        .transpose()?;
    let given_time = request
        .time
        .as_deref()
        .filter(|_| !tool)
        .map(parse_time_arg)
        .transpose()?;
    let given_title = request.title.as_deref();
    if given_title.is_some_and(is_blank) {
        return Err(EngineError::invalid_argument(format!(
            "노트 제목이 비었다: {note_id}"
        )));
    }
    let given_body = request.body.as_deref();
    let token = request.f1.as_deref().map(trim).filter(|t| !t.is_empty());

    let note: &NoteRecord = scope
        .index
        .note(note_id)
        .ok_or_else(|| EngineError::not_found(format!("Note not found: {note_id}")))?;
    check_writable(note, context.caller)?;
    let meta = &note.meta;

    // 준 칸만 갈아 끼운 다음 값. 못 고치는 값은 색인의 값이 따라온다
    let title = given_title.unwrap_or(meta.title.as_str());
    let tags: &[String] = new_tags.as_deref().unwrap_or(&meta.tags);
    let template = request
        .template
        .as_deref()
        .map(trim)
        .filter(|t| TEMPLATE_ID.is_match(t))
        .or(meta.template.as_deref());

    // 날짜와 시각은 준 쪽만 갈아 끼우고, 오프셋은 노트의 지금 값을 지킨다
    let current = read_created(&meta.created).ok_or_else(|| {
        EngineError::internal(format!("created 를 읽지 못해 고치지 않았다: {note_id}"))
    })?;
    let (created, date, hour, minute) = if given_date.is_none() && given_time.is_none() {
        (
            meta.created.clone(),
            current.date,
            current.hour,
            current.minute,
        )
    } else {
        let date = given_date.unwrap_or(current.date);
        let (hour, minute) = given_time.unwrap_or((current.hour, current.minute));
        (
            format!(
                "{}T{hour:02}:{minute:02}:00{}",
                date.era_text(),
                current.offset
            ),
            date,
            hour,
            minute,
        )
    };

    // json 노트에 본문을 주면 파일을 읽기 전에 선다
    if note.format == Format::Json && given_body.is_some() {
        return Err(EngineError::internal(format!(
            "Lexical JSON 노트의 본문은 고칠 수 없다(마크다운을 editorState 로 바꾸는 일이라 사이드카에 없다): {note_id}"
        )));
    }

    let bytes = precondition::check(vault, note, token, "고치지")?;
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

    // 새 자리의 아젠다 slug. 파일의 agenda가 빈 글이 아닌 문자열이면 그 글자 그대로다
    let old_rel = note.rel_path.as_str();
    let agenda = match get(&raw, "agenda") {
        Some(Value::Str(s)) if !s.is_empty() => s.clone(),
        _ => {
            let parts: Vec<&str> = old_rel.split('/').collect();
            if parts.len() >= 2 {
                parts[1].to_string()
            } else {
                meta.agenda_slug.clone()
            }
        }
    };
    let body = given_body.unwrap_or(&old_body);
    let session_id = match get(&raw, "sessionId") {
        Some(Value::Str(s)) if !trim(s).is_empty() => Some(cut_utf16(trim(s), 128)),
        _ => None,
    };
    let session_agent = session_id.as_ref().and(meta.session_agent.as_deref());

    let text = judge(
        note.format,
        &Draft {
            meta,
            agenda: &agenda,
            title,
            created: &created,
            session_id: session_id.as_deref(),
            session_agent,
            tier: meta.tier,
            tags,
            favorite: meta.favorite,
            favorite_category: meta.favorite_category.as_deref(),
            template,
            evaluation: meta.evaluation.as_ref(),
            links: &meta.links,
        },
        body,
        &old_body,
        &raw,
        context,
    )?;

    // 자리 갈래: 부모 폴더와 폴더 이름
    let parent = parent_folder(vault, &agenda, &date);
    let folder = folder_name(&date, hour, minute, title, note_id);
    let content_name = note
        .content_file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let check_size = |rel: &str| -> Result<(), EngineError> {
        if utf16_len(&text) > MAX_FILE_UNITS {
            return Err(EngineError::internal(format!(
                "노트가 저장 상한을 넘어 저장하지 않았다: {rel}"
            )));
        }
        Ok(())
    };

    let mut warnings_found: Vec<String> = Vec::new();
    let final_rel: String;
    if !note.is_single_file {
        // 폴더 노트: 경로 검사, 빈자리 고르기, 옮기기 뒤에 상한을 보고 쓴다
        let canonical = format!("{parent}/{folder}");
        check_path(vault, &canonical)?;
        final_rel = place_folder(vault, old_rel, &canonical)?;
        check_size(&final_rel)?;
        write_atomic(&vault.join(&final_rel).join(&content_name), text.as_bytes())?;
    } else if note.format == Format::Html {
        // 옛 .html 단일 파일: 이름만 바꾼다. 상한, 경로, 쓰기 차례이고 같은 이름 파일은 덮는다
        final_rel = format!("{parent}/{folder}.html");
        check_size(&final_rel)?;
        check_path(vault, &final_rel)?;
        write_atomic(&vault.join(&final_rel), text.as_bytes())?;
        if final_rel != old_rel {
            remove_old(vault, old_rel, &mut warnings_found);
        }
    } else {
        // 옛 .md, .json 단일 파일: 빈자리를 안 보고 새 폴더에 쓴 뒤 옛 파일을 지운다
        final_rel = format!("{parent}/{folder}");
        check_size(&final_rel)?;
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
