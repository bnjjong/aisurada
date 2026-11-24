//! `get_sub_note`: 후속 노트 하나를 본문과 코멘트까지 읽는다.

use serde::{Deserialize, Serialize};

use super::subnote::{check_readable, resolve_container, sub_comments_file};
use crate::kernel::actor::CallContext;
use crate::error::EngineError;
use crate::model::SubNoteRow;
use crate::text::{SUB_NOTE_ID, is_blank};
use crate::vault::comments::{CommentsEnvelope, read_comments};
use crate::vault::index::NoteIndex;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GetSubNoteRequest {
    /// 부모 사슬. 주지 않으면 후속 노트 id만으로 찾는다.
    pub parent: Option<String>,
    pub id: String,
    pub include_id_path: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FoundSubNote {
    #[serde(flatten)]
    note: SubNoteRow,
    /// 최상위 노트부터 찾은 후속 노트까지의 ID 목록.
    #[serde(skip_serializing_if = "Option::is_none")]
    id_path: Option<Vec<String>>,
}

pub fn execute(index: &NoteIndex, request: &GetSubNoteRequest, context: &CallContext) -> Result<FoundSubNote, EngineError> {
    if is_blank(&request.id) {
        return Err(EngineError::invalid_argument(
            "인자가 둘 필요하다: get-sub-note <부모 노트 id> <하위 노트 id>",
        ));
    }
    let id = request.id.as_str();

    let parent = request.parent.as_deref().filter(|p| !is_blank(p));
    let (root, sub) = match parent {
        Some(parent) => {
            let container = resolve_container(index, parent)?;
            if !SUB_NOTE_ID.is_match(id) {
                return Err(EngineError::invalid_argument(format!(
                    "하위 노트 id 모양이 아니다: {id}"
                )));
            }
            let sub = container
                .root
                .and_then(|r| r.sub_notes.iter().find(|s| s.id == id))
                .ok_or_else(|| {
                    EngineError::not_found(format!("SubNote not found: {}/{id}", container.root_id))
                })?;
            if sub.direct_parent_id.as_deref() != Some(container.container_id.as_str()) {
                return Err(EngineError::not_found(format!("SubNote not found: {id}")));
            }
            (
                container
                    .root
                    .expect("후속 노트를 찾았으니 메인 노트가 있다"),
                sub,
            )
        }
        None => {
            if !SUB_NOTE_ID.is_match(id) {
                return Err(EngineError::invalid_argument(format!(
                    "하위 노트 id 모양이 아니다: {id}"
                )));
            }
            index
                .find_sub(id)
                .ok_or_else(|| EngineError::not_found(format!("SubNote not found: {id}")))?
        }
    };
    check_readable(root, context.caller)?;

    let file = sub_comments_file(index, root, sub);
    let comments = if index.can_read_related(root, &file) {
        read_comments(&index.vault_root, &file)
    } else {
        CommentsEnvelope::Absent
    };
    Ok(FoundSubNote {
        note: SubNoteRow::from_record(sub, true, Some(comments)),
        id_path: if request.include_id_path {
            Some(root.sub_note_id_path(sub).ok_or_else(|| {
                EngineError::internal(format!("후속 노트의 상위 경로를 찾을 수 없다: {id}"))
            })?)
        } else {
            None
        },
    })
}
