//! 노트 본문 쓰기 명령: `update_doc`(제목·본문·태그·템플릿·날짜·시각 고치기와 다른 아젠다로 옮기기).
//! 인자 이름, 검사 순서, 오류 문구와 응답 필드는 Java 엔진과 같고, 파일 글은 바이트까지 같다.
//! 아젠다 옮기기(`agenda`)는 Java 엔진 뒤에 더했다(TSK-2). 이미 있는 아젠다만 받고 아젠다를 새로 만들지 않는다.

use crate::ports::fs;
use std::path::Path;

use serde::Deserialize;
use serde_json::{Value as Json, json};

use super::Caller;
use super::doc_create::{AgendaMatch, find_agenda};
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
    if let Some(slug) = &moved_to {
        response["agenda"] = json!(slug);
    }
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

/// 옮긴 노트 폴더 안 후속 노트들의 머리 `agenda:` 줄을 새 `slug`로 바꾼다.
/// 색인은 후속 노트의 아젠다를 폴더 자리로 읽으므로 이 줄이 낡아도 목록은 그대로지만, 파일을 직접 읽는 사람과 도구를 위해 맞춘다.
/// 그 줄 하나만 바꾸고 나머지 바이트(편집 도장 포함)는 그대로 둔다. 폴더째 옮기기는 이미 끝났으므로
/// 한 파일을 못 고쳐도 되돌리지 않고 `warnings`에 적는다.
fn retag_sub_notes(
    vault: &Path,
    old_folder: &Path,
    new_rel: &str,
    subs: &[NoteRecord],
    slug: &str,
    warnings: &mut Vec<String>,
) {
    let new_folder = vault.join(new_rel);
    let line = format!("agenda: {}", dump_string(slug, 1));
    for sub in subs {
        let Ok(inside) = sub.content_file.strip_prefix(old_folder) else {
            continue;
        };
        let path = new_folder.join(inside);
        let result = fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| String::from_utf8(bytes).map_err(|_| "UTF-8 이 아니다".to_string()))
            .and_then(|text| match replace_agenda_line(&text, &line) {
                Some(changed) if changed != text => {
                    write_atomic(&path, changed.as_bytes()).map_err(|e| e.message)
                }
                _ => Ok(()),
            });
        if let Err(reason) = result {
            warnings.push(format!(
                "후속 노트 머리의 agenda 를 못 고쳤다: {} ({reason})",
                rel_of(vault, &path)
            ));
        }
    }
}

/// 머리 울타리 안에서 들여쓰지 않은 `agenda:` 줄을 `line`으로 바꾼 글. 머리나 그 줄이 없으면 `None`
fn replace_agenda_line(text: &str, line: &str) -> Option<String> {
    let (yaml, _) = split_fence(text)?;
    let mut offset = yaml.as_ptr() as usize - text.as_ptr() as usize;
    for raw in yaml.split_inclusive('\n') {
        let content = raw.trim_end_matches(['\n', '\r']);
        if content.starts_with("agenda:") {
            let end = offset + content.len();
            return Some(format!("{}{line}{}", &text[..offset], &text[end..]));
        }
        offset += raw.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::replace_agenda_line;

    #[test]
    fn replaces_only_the_agenda_line_of_the_head() {
        let text = "---\nid: sn1\nagenda: demo\ntitle: t\n---\n\nagenda: 본문은 그대로\n";
        assert_eq!(
            replace_agenda_line(text, "agenda: home").as_deref(),
            Some("---\nid: sn1\nagenda: home\ntitle: t\n---\n\nagenda: 본문은 그대로\n")
        );
        let html = "<!--aisurada\nid: sn1\nagenda: demo\n-->\n<p>x</p>";
        assert_eq!(
            replace_agenda_line(html, "agenda: home").as_deref(),
            Some("<!--aisurada\nid: sn1\nagenda: home\n-->\n<p>x</p>")
        );
        // 들여쓴 줄(다른 키의 값)은 아젠다 줄이 아니다. 머리가 없으면 손대지 않는다
        assert_eq!(replace_agenda_line("---\nmeta:\n  agenda: x\n---\n", "agenda: y"), None);
        assert_eq!(replace_agenda_line("agenda: x\n", "agenda: y"), None);
    }
}
