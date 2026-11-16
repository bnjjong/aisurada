//! `export_markdown`: 읽기 층 메인 노트의 마크다운 본문을 고정 내부 폴더(`.aisurada/md-export`)에 내보낸다(27장).
//! 규칙과 응답은 Java 엔진의 `ExportMarkdown`과 같다. 내보내기 폴더는 매번 지우고 새로 쓴다.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value as Json, json};

use crate::error::EngineError;
use crate::text::is_blank;
use crate::vault::scanner::Format;
use crate::write::atomic::write_atomic;
use crate::write::confine::{absolute_normalized, check_confinement, check_segment, normalize};
use crate::write::pipeline::WriteScope;
use crate::write::sub_note::{read_derived, remove_tree};

static SINGLE_FILE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)^.*\.(md|html|json)$").unwrap());
static EXTENSION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\.(md|html|json)$").unwrap());

pub fn export_markdown(vault: &Path, scope: &WriteScope) -> Result<Json, EngineError> {
    let mut exports: Vec<(String, String)> = Vec::new();
    let (mut total, mut html, mut json_notes, mut unsafe_count, mut exported) = (0, 0, 0, 0, 0);
    for note in scope.index.root_notes() {
        if !note.meta.tier.is_reading() {
            continue;
        }
        total += 1;
        match note.format {
            Format::Html => {
                html += 1;
                continue;
            }
            Format::Json => {
                json_notes += 1;
                continue;
            }
            Format::Markdown => {}
        }
        let path = note.rel_path.clone();
        if is_blank(&path) {
            return Err(EngineError::conflict(format!(
                "저장 자리를 모르는 노트가 있어 내보내지 않았다. 색인을 다시 만들어야 한다: {}",
                note.id
            )));
        }
        // 확장자로 단일 파일을 판정하는 기존 읽기 경계도 유지한다(27.9의 9번)
        let body_file = if SINGLE_FILE.is_match(&path) {
            vault.join(&path)
        } else {
            note.content_file.clone()
        };
        exports.push((path, read_derived(&body_file).0));
    }
    let dest = absolute_normalized(&vault.join(".aisurada/md-export"));
    remove_tree(&dest)?;
    crate::ports::fs::create_dir_all(&dest)
        .map_err(|_| EngineError::internal(format!("폴더를 만들지 못했다: {}", dest.display())))?;
    for (path, body) in &exports {
        let stripped = path.strip_prefix("notes/").unwrap_or(path);
        let relative = format!("{}.md", EXTENSION.replace(stripped, ""));
        if is_blank(&relative)
            || relative.starts_with('/')
            || relative.contains("..")
            || relative.contains('\\')
            || relative.chars().any(|c| (c as u32) < 32)
        {
            unsafe_count += 1;
            continue;
        }
        check_segment(&dest, &relative)?;
        let target = normalize(&dest.join(&relative));
        check_confinement(&dest, &target)?;
        write_atomic(&target, body.as_bytes())?;
        exported += 1;
    }
    let mut result = Map::new();
    result.insert("dest".into(), json!(dest.to_string_lossy()));
    result.insert("exported".into(), json!(exported));
    result.insert("totalNotes".into(), json!(total));
    result.insert("skippedHtml".into(), json!(html));
    if unsafe_count != 0 {
        result.insert("skippedUnsafe".into(), json!(unsafe_count));
    }
    if json_notes != 0 {
        result.insert("skippedJson".into(), json!(json_notes));
    }
    Ok(Json::Object(result))
}
