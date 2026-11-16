//! 색인 맞추기가 코멘트 파일 사이에서 코멘트 id가 겹치는지 보는 검사.
//!
//! 노트 폴더를 통째로 복사해 두면 `comments.json`과 `sub-comments/<id>.json`이 두 벌이 되어 같은 코멘트 id가 두 곳에 있게 된다.
//! 색인(Java 엔진의 SQLite)은 코멘트 id를 유일하게 요구하므로, 읽기 명령은 처음 겹친 노트부터 뒤의 노트를 목록에서 빼고,
//! 쓰기 명령은 일을 하지 않고 실패한다. 이 검사는 코멘트 읽기(`comments.rs`)와 별개의 규칙을 쓴다.

use crate::kernel::ids::COMMENT_ID;
use std::collections::HashSet;
use crate::ports::fs;
use std::path::Path;

use super::note_meta::{get, parse_created};
use super::scanner::NoteRecord;
use crate::error::EngineError;
use crate::frontmatter::{self, Value};
use crate::text::is_blank;

/// 메인 노트를 훑은 차례대로 보며 처음 코멘트 id가 겹치는 노트의 번호와 오류를 돌려준다. 겹침이 없으면 `None`이다.
pub fn first_failure(vault_root: &Path, notes: &[NoteRecord]) -> Option<(usize, EngineError)> {
    let mut seen = HashSet::new();
    notes
        .iter()
        .enumerate()
        .find_map(|(i, note)| check(vault_root, note, &mut seen).err().map(|e| (i, e)))
}

fn check(
    vault_root: &Path,
    root: &NoteRecord,
    seen: &mut HashSet<String>,
) -> Result<(), EngineError> {
    if root.is_single_file {
        return Ok(());
    }
    read(&vault_root.join(&root.rel_path).join("comments.json"), seen)?;
    for sub in &root.sub_notes {
        // 후속 노트의 코멘트는 메인 노트 폴더의 `sub-comments/` 아래에 있다
        let container = vault_root.join(&sub.rel_path);
        if let Some(folder) = container.parent().and_then(Path::parent) {
            read(
                &folder.join("sub-comments").join(format!("{}.json", sub.id)),
                seen,
            )?;
        }
    }
    Ok(())
}

fn read(file: &Path, seen: &mut HashSet<String>) -> Result<(), EngineError> {
    if !fs::metadata(file).map(|m| m.is_file()).unwrap_or(false) {
        return Ok(());
    }
    let Ok(bytes) = fs::read(file) else {
        return Ok(());
    };
    // 깨진 UTF-8이면 코멘트 0건
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return Ok(());
    };
    // 코멘트 파일도 머리와 같은 규칙으로 읽는다(Java 엔진이 같은 읽개를 쓴다)
    let Some(map) = frontmatter::parse(text) else {
        return Ok(());
    };
    let Some(Value::List(items)) = get(&map, "comments") else {
        return Ok(());
    };
    let mut in_file = HashSet::new();
    for item in items {
        let Value::Map(entry) = item else { continue };
        let Some(Value::Str(id)) = get(entry, "id") else {
            continue;
        };
        if !COMMENT_ID.is_match(id) {
            continue;
        }
        let Some(Value::Str(created_at)) = get(entry, "createdAt") else {
            continue;
        };
        if is_blank(created_at) || parse_created(created_at).is_none() {
            continue;
        }
        let Some(Value::Str(body)) = get(entry, "body") else {
            continue;
        };
        if is_blank(body) || !in_file.insert(id.clone()) {
            continue;
        }
        if !seen.insert(id.clone()) {
            return Err(EngineError::internal(format!(
                "UNIQUE constraint failed: comments.id ({id}, {})",
                file.display()
            )));
        }
    }
    Ok(())
}
