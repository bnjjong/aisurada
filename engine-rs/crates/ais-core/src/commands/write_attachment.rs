//! `write_attachment`: 노트나 후속 노트 폴더의 `attach/`에 글 첨부 1개를 쓴다. 첨부는 「주인과 이름」이 열쇠라 같은 이름이면 덮는다.
//! 검사 차례와 오류 문구는 설명서 18.8과 Java 엔진의 `WriteAttachment`를 따른다.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;
use serde_json::{Value as Json, json};

use super::list_attachments::make_ref;
use super::option_text::text_of;
use super::subnote::check_writable;
use crate::error::EngineError;
use crate::kernel::actor::CallContext;
use crate::text::{NOTE_ID, SUB_NOTE_ID, is_blank, trim, utf16_len};
use crate::write::atomic::write_atomic;
use crate::write::confine::has_unusable_path_characters;
use crate::write::pipeline::WriteScope;

const USAGE: &str = "write-attachment <노트 id> [하위 노트 id] name=<파일명.확장자> content=<내용>";
const TEXT_EXTENSIONS: [&str; 12] = [
    "svg", "html", "htm", "md", "markdown", "txt", "json", "csv", "tsv", "xml", "yaml", "yml",
];
static EXTENSION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\.[A-Za-z0-9]+$").unwrap());

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WriteAttachmentRequest {
    pub id: String,
    pub sub_id: Option<String>,
    pub name: Option<Json>,
    pub content: Option<Json>,
    #[serde(flatten)]
    pub context: CallContext,
}

/// 이름 규칙을 이 차례로 보고 처음 걸린 1개만 낸다(18.8의 9번).
fn check_name(name: &str) -> Result<(), EngineError> {
    let bad = |message: String| Err(EngineError::invalid_argument(message));
    if name.is_empty() {
        return bad("첨부 파일명이 비었다".to_string());
    }
    if utf16_len(name) > 120 {
        return bad(format!("첨부 파일명이 너무 길다: {}자", utf16_len(name)));
    }
    if name.contains('/') || name.contains('\\') {
        return bad(format!("첨부 파일명에 경로 구분자가 있다: {name}"));
    }
    if name.contains("..") {
        return bad(format!("첨부 파일명에 상위 디렉터리 참조가 있다: {name}"));
    }
    if name.starts_with('.') {
        return bad(format!("첨부 파일명이 점으로 시작한다: {name}"));
    }
    if name.chars().any(|c| (c as u32) < 0x20) {
        return bad("첨부 파일명에 제어문자가 있다".to_string());
    }
    // Java의 `$`라서 글 끝에 U+0085, U+2028, U+2029 1개가 붙어도 그 앞에서 맞는다(18.5)
    let body = name
        .strip_suffix(['\u{85}', '\u{2028}', '\u{2029}'])
        .unwrap_or(name);
    if !EXTENSION.is_match(body) {
        return bad(format!("첨부 파일명에 확장자가 없다: {name}"));
    }
    Ok(())
}

pub fn write_attachment(
    vault: &Path,
    request: &WriteAttachmentRequest,
    scope: &WriteScope,
) -> Result<Json, EngineError> {
    if is_blank(&request.id) {
        return Err(EngineError::invalid_argument(format!(
            "인자가 하나 필요하다: {USAGE}"
        )));
    }
    let root_id = request.id.as_str();
    let sub_id = request.sub_id.as_deref().filter(|s| !is_blank(s));
    if !NOTE_ID.is_match(root_id) {
        return Err(EngineError::invalid_argument(format!(
            "노트 id 모양이 아니다: {root_id}"
        )));
    }
    if let Some(sub) = sub_id
        && !SUB_NOTE_ID.is_match(sub)
    {
        return Err(EngineError::invalid_argument(format!(
            "하위 노트 id 모양이 아니다: {sub}"
        )));
    }
    let raw_name = text_of(&request.name)
        .ok_or_else(|| EngineError::invalid_argument(format!("name 이 필요하다: {USAGE}")))?;
    let content = text_of(&request.content)
        .ok_or_else(|| EngineError::invalid_argument(format!("content 이 필요하다: {USAGE}")))?;
    let name = trim(&raw_name).to_string();
    check_name(&name)?;
    let ext = name[name.rfind('.').map_or(0, |i| i + 1)..].to_lowercase();
    if !TEXT_EXTENSIONS.contains(&ext.as_str()) {
        return Err(EngineError::invalid_argument(format!(
            "텍스트로 쓸 수 없는 확장자다(이미지 같은 바이너리는 attach/ 에 직접 둔다): {name}"
        )));
    }
    let root = scope
        .index
        .note(root_id)
        .ok_or_else(|| EngineError::not_found(format!("Note not found: {root_id}")))?;
    check_writable(root, request.context.caller)?;
    let owner =
        match sub_id {
            None => root,
            Some(sub) => root.sub_notes.iter().find(|s| s.id == sub).ok_or_else(|| {
                EngineError::not_found(format!("SubNote not found: {root_id}/{sub}"))
            })?,
        };
    let owner_rel = owner.rel_path.as_str();
    let last = owner_rel.rsplit('/').next().unwrap_or(owner_rel);
    if last.ends_with(".md") || last.ends_with(".html") || last.ends_with(".json") {
        return Err(EngineError::internal(format!(
            "폴더로 안 올라간 단일 파일 노트에는 첨부를 둘 수 없다(앱에서 한 번 열면 폴더가 된다): {owner_rel}"
        )));
    }
    if has_unusable_path_characters(&name) {
        return Err(EngineError::invalid_argument(format!(
            "경로로 쓸 수 없는 값이다: {name}"
        )));
    }
    let rel_path = format!("{owner_rel}/attach/{name}");
    write_atomic(&vault.join(&rel_path), content.as_bytes())?;
    let parent = match sub_id {
        Some(sub) => format!("{root_id}/{sub}"),
        None => root_id.to_string(),
    };
    Ok(json!({"parent": parent, "name": name, "path": rel_path, "ref": make_ref(&name)}))
}
