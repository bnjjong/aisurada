//! `list_comments`: 노트 또는 후속 노트의 코멘트를 읽는다.

use serde::Deserialize;

use super::subnote::{check_note_id, sub_comments_file};
use crate::error::EngineError;
use crate::text::{SUB_NOTE_ID, is_blank};
use crate::vault::comments::{CommentsEnvelope, read_comments};
use crate::vault::index::NoteIndex;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ListCommentsRequest {
    /// 메인 노트 id
    pub id: String,
    /// 후속 노트 id. 주면 그 후속 노트의 코멘트를 읽는다.
    pub sub_id: Option<String>,
}

pub fn execute(
    index: &NoteIndex,
    request: &ListCommentsRequest,
) -> Result<CommentsEnvelope, EngineError> {
    check_note_id(&request.id, "list-comments <노트 id> [하위 노트 id]")?;
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

    let file = match sub_id {
        None => index.vault_root.join(&root.rel_path).join("comments.json"),
        Some(sub_id) => {
            let sub = root
                .sub_notes
                .iter()
                .find(|s| s.id == sub_id)
                .ok_or_else(|| EngineError::not_found(format!("SubNote not found: {sub_id}")))?;
            sub_comments_file(index, root, sub)
        }
    };
    Ok(read_comments(&index.vault_root, &file))
}
