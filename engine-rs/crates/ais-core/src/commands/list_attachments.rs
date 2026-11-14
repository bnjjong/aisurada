//! `list_attachments`: 노트 또는 후속 노트의 `attach/` 폴더에 든 첨부를 이름 차례로 돌려준다.

use std::fs;
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;

use crate::error::EngineError;
use crate::model::{AttachmentRow, Page};
use crate::text::{SUB_NOTE_ID, compare_utf16, encode_uri_component, is_blank, utf16_len};
use crate::vault::index::NoteIndex;

use super::subnote::check_note_id;

static HAS_EXTENSION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\.[A-Za-z0-9]+$").unwrap());
const IMAGE_EXTENSIONS: [&str; 8] = ["svg", "png", "jpg", "jpeg", "gif", "webp", "avif", "bmp"];

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ListAttachmentsRequest {
    pub id: String,
    pub sub_id: Option<String>,
}

pub fn execute(
    index: &NoteIndex,
    request: &ListAttachmentsRequest,
) -> Result<Page<AttachmentRow>, EngineError> {
    check_note_id(&request.id, "list-attachments <노트 id> [하위 노트 id]")?;
    let sub_id = request.sub_id.as_deref().filter(|s| !is_blank(s));
    if let Some(sub_id) = sub_id
        && !SUB_NOTE_ID.is_match(sub_id)
    {
        return Err(EngineError::invalid_argument(format!(
            "하위 노트 id 모양이 아니다: {sub_id}"
        )));
    }
    let root = index
        .note(&request.id)
        .ok_or_else(|| EngineError::not_found(format!("Note not found: {}", request.id)))?;
    let owner = match sub_id {
        None => root,
        Some(sub_id) => root
            .sub_notes
            .iter()
            .find(|s| s.id == sub_id)
            .ok_or_else(|| {
                EngineError::not_found(format!("SubNote not found: {}/{sub_id}", request.id))
            })?,
    };

    let mut names: Vec<String> = Vec::new();
    if !owner.is_single_file {
        let attach_dir = index.vault_root.join(&owner.rel_path).join("attach");
        if let Ok(entries) = fs::read_dir(&attach_dir) {
            names = entries
                .flatten()
                .filter_map(|e| {
                    e.file_name()
                        .into_string()
                        .ok()
                        .map(|name| (name, e.path()))
                })
                .filter(|(name, path)| {
                    is_valid_attachment_name(name)
                        && fs::metadata(path).map(|m| m.is_file()).unwrap_or(false)
                })
                .map(|(name, _)| name)
                .collect();
            names.sort_by(|a, b| compare_utf16(a, b));
        }
    }
    let total = names.len();
    Ok(Page::new(
        names
            .into_iter()
            .map(|name| AttachmentRow {
                reference: make_ref(&name),
                name,
            })
            .collect(),
        total,
    ))
}

fn is_valid_attachment_name(name: &str) -> bool {
    !name.is_empty()
        && utf16_len(name) <= 120
        && !name.ends_with(".tmp")
        && !name.starts_with('.')
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && name.chars().all(|c| c >= ' ')
        && HAS_EXTENSION.is_match(name)
}

/// 본문에 붙여 넣을 마크다운 참조: 그림은 `![이름](attach/…)`, 그 밖은 `[이름](attach/…)`
fn make_ref(name: &str) -> String {
    let extension = name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_lowercase())
        .unwrap_or_default();
    let label = name.replace(']', "");
    let url = format!(
        "attach/{}",
        encode_uri_component(name)
            .replace('(', "%28")
            .replace(')', "%29")
    );
    if IMAGE_EXTENSIONS.contains(&extension.as_str()) {
        format!("![{label}]({url})")
    } else {
        format!("[{label}]({url})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makes_markdown_references() {
        assert_eq!(
            make_ref("지도.PNG"),
            "![지도.PNG](attach/%EC%A7%80%EB%8F%84.PNG)"
        );
        assert_eq!(make_ref("a (1).pdf"), "[a (1).pdf](attach/a%20%281%29.pdf)");
        assert_eq!(make_ref("x]y.txt"), "[xy.txt](attach/x%5Dy.txt)");
    }

    #[test]
    fn attachment_name_rules() {
        assert!(is_valid_attachment_name("a.png"));
        assert!(
            !is_valid_attachment_name(".hidden.png")
                && !is_valid_attachment_name("a.tmp")
                && !is_valid_attachment_name("noext")
        );
        assert!(!is_valid_attachment_name("a/b.png") && !is_valid_attachment_name("a..b.png"));
    }
}
