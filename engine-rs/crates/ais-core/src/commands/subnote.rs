//! 후속 노트를 찾는 공통 규칙: 부모 사슬(`루트/자식/손자`) 해석, 호출자별 접근 검사, 코멘트 파일 자리.

use std::path::PathBuf;

use super::Caller;
use crate::error::EngineError;
use crate::text::{NOTE_ID, SUB_NOTE_ID, is_blank, trim};
use crate::vault::index::NoteIndex;
use crate::vault::scanner::NoteRecord;

/// 부모 참조를 푼 결과
pub struct Container<'a> {
    /// 메인 노트. 사슬이 루트 하나이고 그 노트가 없으면 `None`
    pub root: Option<&'a NoteRecord>,
    pub root_id: String,
    /// 후속 노트들이 직접 달려 있는 노트의 id(메인 노트이거나 다른 후속 노트)
    pub container_id: String,
}

/// 부모 참조를 풉니다. 사슬(`루트/자식/…`)이거나, 후속 노트 id 하나(그 후속 노트를 부모로 본다)입니다.
pub fn resolve_container<'a>(
    index: &'a NoteIndex,
    raw_parent: &str,
) -> Result<Container<'a>, EngineError> {
    let segments: Vec<&str> = raw_parent
        .split('/')
        .map(trim)
        .filter(|s| !s.is_empty())
        .collect();
    if segments.is_empty() {
        return Err(EngineError::invalid_argument(format!(
            "부모 노트 참조가 비었다: {raw_parent}"
        )));
    }

    // 후속 노트 id 하나만 주면 그 노트가 어느 메인 노트 아래인지 찾아 준다
    if segments.len() == 1 && SUB_NOTE_ID.is_match(segments[0]) {
        let (root, sub) = index
            .find_sub(segments[0])
            .ok_or_else(|| EngineError::not_found(format!("SubNote not found: {}", segments[0])))?;
        return Ok(Container {
            root: Some(root),
            root_id: root.id.clone(),
            container_id: sub.id.clone(),
        });
    }

    let root_id = segments[0];
    if !NOTE_ID.is_match(root_id) {
        return Err(EngineError::invalid_argument(format!(
            "노트 id 모양이 아니다: {root_id}"
        )));
    }
    let root = index.note(root_id);
    let mut previous = root_id;
    for segment in &segments[1..] {
        if !SUB_NOTE_ID.is_match(segment) {
            return Err(EngineError::invalid_argument(format!(
                "하위 노트 id 모양이 아니다: {segment}"
            )));
        }
        let sub = root
            .and_then(|r| r.sub_notes.iter().find(|s| s.id == *segment))
            .ok_or_else(|| {
                EngineError::not_found(format!("SubNote not found: {root_id}/{segment}"))
            })?;
        if sub.direct_parent_id.as_deref() != Some(previous) {
            return Err(EngineError::not_found(format!(
                "SubNote not found: {segment}"
            )));
        }
        previous = segment;
    }
    Ok(Container {
        root,
        root_id: root_id.to_string(),
        container_id: previous.to_string(),
    })
}

/// AI에게는 기록 층 메인 노트를 열어 주지 않는다.
pub fn check_readable(root: &NoteRecord, caller: Caller) -> Result<(), EngineError> {
    if caller == Caller::Ai && root.meta.tier == "record" {
        return Err(EngineError::record_tier(&root.id));
    }
    Ok(())
}

/// 후속 노트의 코멘트 파일 자리: 바로 위 노트의 폴더 안 `sub-comments/<후속 노트 id>.json`
pub fn sub_comments_file(index: &NoteIndex, root: &NoteRecord, sub: &NoteRecord) -> PathBuf {
    let parent_id = sub.direct_parent_id.as_deref().unwrap_or(&root.id);
    let container = if parent_id == root.id {
        Some(root)
    } else {
        root.sub_notes.iter().find(|s| s.id == parent_id)
    };
    let dir = match container {
        Some(c) => index.vault_root.join(&c.rel_path),
        None => index.vault_root.clone(),
    };
    dir.join("sub-comments").join(format!("{}.json", sub.id))
}

/// 노트 id를 검사한다: 비었는지, 모양이 맞는지
pub fn check_note_id(id: &str, usage: &str) -> Result<(), EngineError> {
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
