//! 후속 노트 쓰기의 공통 부품: 메인 노트의 아젠다 읽기, 후속 노트 파일 글 짓기, 정식 폴더로 옮기며 쓰기,
//! 부모 본문의 하위 페이지 카드 걷어내기, 폴더 지우기. 규칙은 Java 엔진의 `SubNoteWriter`, `DerivedBodyWriter`,
//! `ChildPageCards`, `TreeRemover`와 같다.

use crate::kernel::tier::Tier;
use crate::ports::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::atomic::write_atomic;
use super::confine::{check_confinement, has_unusable_path_characters};
use super::doc_place::{folder_name_from, place_folder, read_created};
use super::meta_writer::MAX_FILE_UNITS;
use super::note_front::NoteFront;
use crate::error::EngineError;
use crate::frontmatter::Value;
use crate::text::{cut_utf16, trim, utf16_len};
use crate::vault::note_file::split_text;
use crate::vault::note_meta::get;
use crate::vault::scanner::{Format, NoteRecord};

pub const SINGLE_FILE_MESSAGE: &str =
    "폴더로 안 올라간 단일 파일 노트에는 하위 노트를 둘 수 없다(앱에서 한 번 열면 폴더가 된다): ";

/// 쓴 결과: 최종 폴더(vault 기준 상대 경로)와 쓴 바이트
pub struct Placed {
    pub folder_rel: String,
    pub bytes: Vec<u8>,
}

/// `[제목](#sub:<id>)`만 있는 줄(뒤 공백 허용) = 하위 페이지 카드
static CARD_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[[^\]]+\]\(#sub:([^)]+)\)[ \t\n\x0B\f\r]*$").unwrap());

fn rel_of(vault: &Path, path: &Path) -> String {
    path.strip_prefix(vault)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// 메인 노트의 아젠다 slug. 파일 머리의 `agenda`가 빈 글이 아닌 문자열이면 그것, 아니면 경로의 둘째 조각, 아니면 색인의 값이다.
pub fn root_agenda(root: &NoteRecord) -> String {
    let from_file = (|| {
        let metadata = fs::metadata(&root.content_file).ok()?;
        if !metadata.is_file() || metadata.len() > 1_000_000 {
            return None;
        }
        let content = String::from_utf8(fs::read(&root.content_file).ok()?).ok()?;
        let raw = split_text(&content).frontmatter?;
        match get(&raw, "agenda") {
            Some(Value::Str(s)) if !s.is_empty() => Some(s.clone()),
            _ => None,
        }
    })();
    from_file.unwrap_or_else(|| {
        let parts: Vec<&str> = root.rel_path.split('/').collect();
        if parts.len() >= 2 {
            parts[1].to_string()
        } else {
            root.meta.agenda_slug.clone()
        }
    })
}

/// 후속 노트 파일 글: 머리는 색인의 값과 옛 편집 도장으로 짓고 세션, 층, 즐겨찾기, 템플릿, 평가는 적지 않는다.
pub fn file_text(
    sub: &NoteRecord,
    agenda: &str,
    title: &str,
    tags: &[String],
    body: &str,
) -> String {
    let meta = &sub.meta;
    let front = NoteFront {
        id: &meta.id,
        agenda,
        doc_type: &meta.doc_type,
        title,
        created: &meta.created,
        session_id: None,
        session_agent: None,
        tier: Tier::Reading,
        edited_by: meta.edited_by.as_deref(),
        edited_at: meta.edited_at.as_deref(),
        edited_session_id: meta.edited_session_id.as_deref(),
        edited_session_agent: meta.edited_session_agent.as_deref(),
        tags,
        favorite: false,
        favorite_category: None,
        template: None,
        evaluation: None,
        links: &meta.links,
    }
    .render();
    format!("---\n{front}---\n{body}")
}

/// 후속 노트를 정식 폴더 이름(만든 날짜·시각, 제목, id 꼬리)으로 옮기고 쓴다. 옮길 자리가 차 있으면 `-2`…를 붙인다.
pub fn place_and_write(
    vault: &Path,
    sub: &NoteRecord,
    title: &str,
    text: &str,
) -> Result<Placed, EngineError> {
    let current = sub.rel_path.as_str();
    let created = read_created(&sub.meta.created).ok_or_else(|| {
        EngineError::internal(format!("created 를 읽지 못해 고치지 않았다: {}", sub.id))
    })?;
    let directory = &current[..current.rfind('/').map_or(0, |i| i + 1)];
    let canonical = format!(
        "{directory}{}",
        folder_name_from(&created.date, created.hour, created.minute, title, &sub.id)
    );
    if has_unusable_path_characters(&canonical) {
        return Err(EngineError::invalid_argument(format!(
            "경로로 쓸 수 없는 값이다: {canonical}"
        )));
    }
    check_confinement(vault, &vault.join(&canonical))?;
    let final_rel = place_folder(vault, current, &canonical)?;
    if utf16_len(text) > MAX_FILE_UNITS {
        return Err(EngineError::internal(format!(
            "노트가 저장 상한을 넘어 저장하지 않았다: {final_rel}"
        )));
    }
    let name = sub
        .content_file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    write_atomic(&vault.join(&final_rel).join(name), text.as_bytes())?;
    Ok(Placed {
        folder_rel: final_rel,
        bytes: text.as_bytes().to_vec(),
    })
}

/// 마크다운 폴더 노트가 아니면 후속 노트를 저장하지 않는다.
pub fn require_markdown_folder(sub: &NoteRecord) -> Result<(), EngineError> {
    if sub.format != Format::Markdown {
        return Err(EngineError::internal(format!(
            "markdown 이 아닌 하위 노트는 저장하지 않는다(포맷={}): {}",
            sub.format.as_str().to_uppercase(),
            sub.id
        )));
    }
    if sub.is_single_file {
        return Err(EngineError::internal(format!(
            "{SINGLE_FILE_MESSAGE}{}",
            sub.rel_path
        )));
    }
    Ok(())
}

/// 부모 본문을 읽는다. 머리가 있으면 본문만, 없으면 파일 전체다. 못 읽으면 빈 본문과 `None`이다.
pub fn read_derived(file: &Path) -> (String, Option<Vec<u8>>) {
    let read = || {
        let metadata = fs::metadata(file).ok()?;
        if !metadata.is_file() || metadata.len() > 1_000_000 {
            return None;
        }
        let bytes = fs::read(file).ok()?;
        let content = String::from_utf8(bytes.clone()).ok()?;
        let split = split_text(&content);
        let body = if split.frontmatter.is_some() {
            split.body
        } else {
            content
        };
        Some((body, Some(bytes)))
    };
    read().unwrap_or_else(|| (String::new(), None))
}

/// 머리를 읽어 되살린다. 읽지 못하거나 머리가 없으면 `snippet`을 넣은 500이다.
fn reopen(vault: &Path, file: &Path, snippet: &str) -> Result<Vec<(Value, Value)>, EngineError> {
    let rel = rel_of(vault, file);
    let content = fs::metadata(file)
        .ok()
        .filter(|m| m.is_file() && m.len() <= 1_000_000)
        .and_then(|_| fs::read(file).ok())
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .ok_or_else(|| {
            EngineError::internal(format!("노트 본문 파일을 못 읽어 {snippet} 않았다: {rel}"))
        })?;
    split_text(&content)
        .frontmatter
        .ok_or_else(|| EngineError::internal(format!("프론트매터가 없어 {snippet} 않았다: {rel}")))
}

/// 메인 노트의 본문만 바꿔 쓴다(자리를 안 옮긴다). 머리는 색인의 값과 옛 편집 도장 그대로 다시 짓고 도장은 새로 찍지 않는다.
pub fn rewrite_root(vault: &Path, root: &NoteRecord, body: &str) -> Result<Vec<u8>, EngineError> {
    let raw = reopen(vault, &root.content_file, "본문을 저장하지")?;
    let meta = &root.meta;
    let agenda = match get(&raw, "agenda") {
        Some(Value::Str(s)) if !s.is_empty() => s.clone(),
        _ => {
            let parts: Vec<&str> = root.rel_path.split('/').collect();
            if parts.len() >= 2 {
                parts[1].to_string()
            } else {
                meta.agenda_slug.clone()
            }
        }
    };
    let session_id = match get(&raw, "sessionId") {
        Some(Value::Str(s)) if !trim(s).is_empty() => Some(cut_utf16(trim(s), 128)),
        _ => None,
    };
    let session_agent = session_id.as_ref().and(meta.session_agent.as_deref());
    let front = NoteFront {
        id: &meta.id,
        agenda: &agenda,
        doc_type: &meta.doc_type,
        title: &meta.title,
        created: &meta.created,
        session_id: session_id.as_deref(),
        session_agent,
        tier: meta.tier,
        edited_by: meta.edited_by.as_deref(),
        edited_at: meta.edited_at.as_deref(),
        edited_session_id: meta.edited_session_id.as_deref(),
        edited_session_agent: meta.edited_session_agent.as_deref(),
        tags: &meta.tags,
        favorite: meta.favorite,
        favorite_category: meta.favorite_category.as_deref(),
        template: meta.template.as_deref(),
        evaluation: meta.evaluation.as_ref(),
        links: &meta.links,
    }
    .render();
    let text = format!("---\n{front}---\n{body}");
    if utf16_len(&text) > MAX_FILE_UNITS {
        return Err(EngineError::internal(format!(
            "노트가 저장 상한을 넘어 저장하지 않았다: {}",
            rel_of(vault, &root.content_file)
        )));
    }
    write_atomic(&root.content_file, text.as_bytes())?;
    Ok(text.into_bytes())
}

/// 후속 노트(부모 역할)의 본문만 바꿔 쓰고 정식 폴더로 옮긴다.
pub fn rewrite_sub(
    vault: &Path,
    root: &NoteRecord,
    container: &NoteRecord,
    body: &str,
) -> Result<Placed, EngineError> {
    reopen(vault, &container.content_file, "하위 노트 본문을 저장하지")?;
    let text = file_text(
        container,
        &root_agenda(root),
        &container.meta.title,
        &container.meta.tags,
        body,
    );
    require_markdown_folder(container)?;
    place_and_write(vault, container, &container.meta.title, &text)
}

/// 본문에서 `sub_id`를 가리키는 하위 페이지 카드 줄을 걷어낸다. 걷어낼 줄이 없으면 본문 그대로다.
pub fn strip_child_cards(body: &str, sub_id: &str) -> String {
    let lines: Vec<&str> = body.split('\n').collect();
    let kept: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| {
            !CARD_LINE
                .captures(line)
                .is_some_and(|captures| trim(&captures[1]) == sub_id)
        })
        .collect();
    if kept.len() == lines.len() {
        body.to_string()
    } else {
        kept.join("\n")
    }
}

/// 파일이나 폴더(안의 것까지)를 지운다. 없으면 아무것도 안 한다. 심볼릭 링크는 따라가지 않고 링크만 지운다.
pub fn remove_tree(target: &Path) -> Result<(), EngineError> {
    // Java `Files.exists`처럼 링크를 따라가 보고, 가리키는 곳이 없으면 아무것도 안 한다
    if !target.exists() {
        return Ok(());
    }
    let Ok(metadata) = fs::symlink_metadata(target) else {
        return Ok(());
    };
    let removed = if metadata.is_dir() {
        fs::remove_dir_all(target)
    } else {
        fs::remove_file(target)
    };
    removed.map_err(|_| EngineError::internal(format!("지우지 못했다: {}", target.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_only_the_card_lines_of_that_sub_note() {
        let body = "# 부모\n[카드 하나](#sub:sn1-00000001) \n[다른](#sub:sn2-00000002)\n문장 [링크](#sub:sn1-00000001) 뒤\n";
        assert_eq!(
            strip_child_cards(body, "sn1-00000001"),
            "# 부모\n[다른](#sub:sn2-00000002)\n문장 [링크](#sub:sn1-00000001) 뒤\n"
        );
        assert_eq!(strip_child_cards(body, "sn9-00000009"), body);
        assert_eq!(
            strip_child_cards("[a](#sub: sn1-00000001 )", "sn1-00000001"),
            ""
        );
        assert_eq!(
            strip_child_cards("[a]( #sub:sn1-00000001)", "sn1-00000001"),
            "[a]( #sub:sn1-00000001)"
        );
    }
}
