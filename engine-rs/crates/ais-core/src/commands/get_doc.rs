//! `get_doc`: 노트 하나를 본문과 코멘트까지 읽는다.

use serde::Deserialize;

use super::subnote::{check_note_id, check_readable};
use crate::kernel::actor::CallContext;
use crate::error::EngineError;
use crate::model::{DocRow, NoteRow};
use crate::vault::comments::{CommentsEnvelope, read_comments};
use crate::vault::index::NoteIndex;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GetDocRequest {
    pub id: String,
}

pub fn execute(index: &NoteIndex, request: &GetDocRequest, context: &CallContext) -> Result<DocRow, EngineError> {
    check_note_id(&request.id, "get-doc <노트 id>")?;
    let note = index
        .note(&request.id)
        .ok_or_else(|| EngineError::not_found(format!("Note not found: {}", request.id)))?;
    check_readable(note, context.caller)?;

    // 파일 하나로 된 노트는 코멘트 파일을 둘 폴더가 없다
    let file = index.vault_root.join(&note.rel_path).join("comments.json");
    let comments = if !note.is_single_file && index.can_read_related(note, &file) {
        read_comments(&index.vault_root, &file)
    } else {
        CommentsEnvelope::Absent
    };
    Ok(DocRow {
        note: NoteRow::from_record(note, context.caller, true, None),
        comments,
    })
}
