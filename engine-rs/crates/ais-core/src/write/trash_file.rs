//! 휴지통 파일 쓰기: 목록 파일(`.aisurada/trash/trash.json`)과 보관 폴더(`.aisurada/trash/files/`).
//! 규칙은 Java 엔진의 `TrashCommands`와 같다.

use std::path::{Path, PathBuf};

use super::atomic::write_atomic;
use super::confine::{absolute_normalized, check_segment, normalize};
use crate::ports::fs::check_confinement;
use super::json_out::{literal, string, to_file_bytes};
use super::sub_note::remove_tree;
use crate::error::EngineError;
use crate::json::JsonValue;
use crate::vault::trash::{TrashRecord, scan_trash};

pub const MANIFEST: &str = ".aisurada/trash/trash.json";
const MAX_MANIFEST_BYTES: usize = 1_000_000;

/// 보관 파일의 자리. 이름 한 조각만 받고 보관 폴더 밖으로 나가지 못한다.
pub fn archive_path(vault: &Path, file: &str) -> Result<PathBuf, EngineError> {
    let base = absolute_normalized(&vault.join(".aisurada/trash/files"));
    check_segment(&base, file)?;
    let target = normalize(&base.join(file));
    check_confinement(&base, &target)?;
    // 보관 폴더(`files`) 자체가 vault 밖을 가리키는 링크여도 거기로 옮기거나 지우지 않는다
    check_confinement(vault, &target)?;
    Ok(target)
}

/// 휴지통 항목들을 목록 파일에 적힌 차례로. 못 읽은 항목은 빠진다(다음 쓰기에서 목록 파일에서도 사라진다).
pub fn entries_in_file_order(vault: &Path) -> (Vec<TrashRecord>, usize) {
    let scan = scan_trash(vault);
    let skipped = scan.skipped.len();
    let mut entries = scan.entries;
    entries.sort_by_key(|entry| entry.file_order);
    (entries, skipped)
}

fn serialize(entries: &[TrashRecord]) -> Vec<u8> {
    let rows = entries
        .iter()
        .map(|entry| {
            JsonValue::Object(vec![
                ("trashId".to_string(), string(&entry.trash_id)),
                ("file".to_string(), string(&entry.file)),
                ("originalPath".to_string(), string(&entry.original_path)),
                ("docId".to_string(), string(&entry.doc_id)),
                (
                    "agendaId".to_string(),
                    if entry.agenda_id.is_empty() {
                        literal("null")
                    } else {
                        string(&entry.agenda_id)
                    },
                ),
                ("agendaSlug".to_string(), string(&entry.agenda_slug)),
                ("title".to_string(), string(&entry.title)),
                ("type".to_string(), string(&entry.doc_type)),
                ("format".to_string(), string(entry.format)),
                ("created".to_string(), string(&entry.created)),
                ("trashedAt".to_string(), string(&entry.trashed_at)),
            ])
        })
        .collect();
    to_file_bytes(&JsonValue::Array(rows))
}

/// 목록 파일을 쓴다. 1,000,000바이트를 넘으면 목록의 맨 뒤 항목(가장 오래된 것)부터 보관물까지 영구 삭제하며 줄이고,
/// 지운 항목들을 돌려준다. 항목이 하나만 남으면 더 줄이지 않는다.
pub fn save(vault: &Path, entries: &[TrashRecord]) -> Result<Vec<TrashRecord>, EngineError> {
    let mut list = entries.to_vec();
    let mut purged = Vec::new();
    let mut bytes = serialize(&list);
    while bytes.len() > MAX_MANIFEST_BYTES && list.len() > 1 {
        let oldest = list.pop().expect("항목이 둘 이상이다");
        if let Ok(path) = archive_path(vault, &oldest.file) {
            let _ = remove_tree(&path);
        }
        purged.push(oldest);
        bytes = serialize(&list);
    }
    write_atomic(&vault.join(MANIFEST), &bytes)?;
    Ok(purged)
}
