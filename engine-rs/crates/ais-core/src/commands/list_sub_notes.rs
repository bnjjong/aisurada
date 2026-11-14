//! `list_sub_notes`: 노트(또는 후속 노트) 바로 아래 후속 노트를 만든 시각 차례로 돌려준다.

use serde::Deserialize;

use super::Caller;
use super::subnote::{check_readable, resolve_container};
use crate::error::EngineError;
use crate::model::{Page, SubNoteRow};
use crate::vault::index::NoteIndex;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ListSubNotesRequest {
    /// 부모 참조: 노트 id, 부모 사슬(`루트/자식/…`), 또는 후속 노트 id
    pub parent: String,
    pub bodies: bool,
    pub caller: Caller,
}

pub fn execute(
    index: &NoteIndex,
    request: &ListSubNotesRequest,
) -> Result<Page<SubNoteRow>, EngineError> {
    if crate::text::is_blank(&request.parent) {
        return Err(EngineError::invalid_argument(
            "인자가 하나 필요하다: list-sub-notes <부모 노트 id> [bodies=true. 본문까지 싣는다. 기본은 메타만]",
        ));
    }
    let container = resolve_container(index, &request.parent)?;

    // 부모가 메인 노트 하나일 때만 그 노트가 있어야 한다(사슬이면 풀면서 이미 확인했다)
    let root = container
        .root
        .ok_or_else(|| EngineError::not_found(format!("Note not found: {}", container.root_id)))?;
    check_readable(root, request.caller)?;

    let mut children: Vec<_> = root
        .sub_notes
        .iter()
        .filter(|s| s.direct_parent_id.as_deref() == Some(container.container_id.as_str()))
        .collect();
    children.sort_by(|a, b| {
        a.meta
            .created
            .as_bytes()
            .cmp(b.meta.created.as_bytes())
            .then_with(|| a.id.as_bytes().cmp(b.id.as_bytes()))
    });

    let total = children.len();
    Ok(Page::new(
        children
            .into_iter()
            .map(|sub| SubNoteRow::from_record(sub, request.bodies, None))
            .collect(),
        total,
    ))
}
