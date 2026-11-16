//! 휴지통 읽기. `.aisurada/trash/trash.json`(JSON이자 YAML인 목록)을 읽어 항목마다 검사합니다.

use crate::kernel::doc_type::DocType;
use crate::kernel::ids::AGENDA_ID;
use crate::kernel::ids::TRASH_ID;
use crate::ports::fs;
use std::path::Path;

use super::note_file::read_note_file;
use super::note_meta::{parse_created, text};
use crate::frontmatter::{self, Value};
use crate::kernel::{
    confine::{absolute_normalized, check_confinement, check_segment, normalize},
    tier::Tier,
};
use crate::text::{NOTE_ID, trim};

const MANIFEST: &str = ".aisurada/trash/trash.json";
const MAX_MANIFEST_BYTES: u64 = 10_000_000;

#[derive(Debug, Clone)]
pub struct TrashRecord {
    pub trash_id: String,
    pub doc_id: String,
    pub title: String,
    pub agenda_slug: String,
    pub agenda_id: String,
    pub doc_type: String,
    pub format: &'static str,
    pub original_path: String,
    pub file: String,
    pub created: String,
    /// `YYYY-MM-DDTHH:MM:SS.mmmZ`
    pub trashed_at: String,
    /// 목록 파일에 적힌 차례(0부터). 쓸 때는 이 차례를 지킨다.
    pub file_order: usize,
}

#[derive(Debug, Clone)]
pub struct SkippedTrash {
    pub location: String,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct TrashScan {
    /// 버린 시각이 늦은 것부터(같으면 목록에 적힌 차례)
    pub entries: Vec<TrashRecord>,
    pub skipped: Vec<SkippedTrash>,
}

fn skipped(location: &str, reason: &str) -> SkippedTrash {
    SkippedTrash {
        location: location.to_string(),
        reason: reason.to_string(),
    }
}

fn field<'a>(map: &'a [(Value, Value)], key: &str) -> &'a str {
    match map
        .iter()
        .find(|(k, _)| matches!(k, Value::Str(s) if s == key))
    {
        Some((_, Value::Str(s))) => trim(s),
        _ => "",
    }
}

/// 휴지통에 보관된 노트의 층. 목록 파일에는 층이 없어서(바이트를 Java와 같게 둔 파일이다) 보관된 노트 머리의 `tier`를 읽는다.
/// 되살렸을 때 색인이 매길 층과 같은 규칙이다. 보관 파일이 없거나 못 읽거나 머리·`tier` 칸이 없으면 읽기 층으로 본다
/// (그런 항목은 되살리기도 못 하거나, 되살려도 읽기 층으로 보인다).
pub fn archived_tier(vault_root: &Path, entry: &TrashRecord) -> Tier {
    let base = absolute_normalized(&vault_root.join(".aisurada/trash/files"));
    if check_segment(&base, &entry.file).is_err() {
        return Tier::Reading;
    }
    let target = normalize(&base.join(&entry.file));
    if check_confinement(&base, &target).is_err() {
        return Tier::Reading;
    }
    // 단일 파일 노트는 보관물이 파일 하나이고, 폴더 노트는 폴더 안의 `note.<확장자>`다
    let content = if fs::metadata(&target).map(|m| m.is_file()).unwrap_or(false) {
        target
    } else {
        let extension = if entry.format == "markdown" {
            "md"
        } else {
            entry.format
        };
        target.join(format!("note.{extension}"))
    };
    read_note_file(&content)
        .ok()
        .and_then(|parsed| parsed.frontmatter)
        .and_then(|map| text(&map, "tier").map(|raw| Tier::from_front(trim(raw))))
        .unwrap_or(Tier::Reading)
}

pub fn scan_trash(vault_root: &Path) -> TrashScan {
    let mut scan = TrashScan::default();
    let path = vault_root.join(MANIFEST);
    if !fs::metadata(&path).map(|m| m.is_file()).unwrap_or(false) {
        return scan;
    }
    let Ok(meta) = fs::metadata(&path) else {
        return scan;
    };
    if meta.len() > MAX_MANIFEST_BYTES {
        scan.skipped
            .push(skipped(MANIFEST, "매니페스트가 10MB를 초과하여 못 읽었다"));
        return scan;
    }
    let Some(content) = fs::read(&path).ok().and_then(|b| String::from_utf8(b).ok()) else {
        scan.skipped
            .push(skipped(MANIFEST, "매니페스트를 못 읽었다"));
        return scan;
    };
    let content = content.strip_prefix('\u{FEFF}').unwrap_or(&content);
    let Some(Value::List(items)) = frontmatter::parse_value(content) else {
        scan.skipped
            .push(skipped(MANIFEST, "매니페스트가 배열이 아니다"));
        return scan;
    };

    let mut entries: Vec<(TrashRecord, String, usize)> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let location = format!("{MANIFEST}[{i}]");
        let Value::Map(map) = item else {
            scan.skipped
                .push(skipped(&location, "항목이 객체가 아니다"));
            continue;
        };
        let (trash_id, file, original_path) = (
            field(map, "trashId"),
            field(map, "file"),
            field(map, "originalPath"),
        );
        let (doc_id, agenda_id, agenda_slug) = (
            field(map, "docId"),
            field(map, "agendaId"),
            field(map, "agendaSlug"),
        );

        let created = parse_created(field(map, "created")).map(|c| c.formatted);
        let trashed = parse_created(field(map, "trashedAt")).map(|c| c.utc_instant);
        let valid = TRASH_ID.is_match(trash_id)
            && !file.is_empty()
            && !original_path.is_empty()
            && NOTE_ID.is_match(doc_id)
            && (AGENDA_ID.is_match(agenda_id) || agenda_id == "ag-recovered")
            && !agenda_slug.is_empty();
        let (Some(created), Some(trashed), true) = (created, trashed, valid) else {
            let hint = if !trash_id.is_empty() {
                trash_id
            } else if !file.is_empty() {
                file
            } else {
                "식별 불가"
            };
            scan.skipped.push(skipped(
                &location,
                &format!("휴지통 항목으로 세울 수 없다: {hint}"),
            ));
            continue;
        };

        let title = match field(map, "title") {
            "" => doc_id.to_string(),
            t => t.to_string(),
        };
        let doc_type = field(map, "type");
        let format = field(map, "format");
        let record = TrashRecord {
            trash_id: trash_id.to_string(),
            doc_id: doc_id.to_string(),
            title,
            agenda_slug: agenda_slug.to_string(),
            agenda_id: agenda_id.to_string(),
            doc_type: if DocType::parse(doc_type).is_some() {
                doc_type.to_string()
            } else {
                "build".to_string()
            },
            format: match format {
                "html" => "html",
                "json" => "json",
                _ => "markdown",
            },
            original_path: original_path.to_string(),
            file: file.to_string(),
            created,
            trashed_at: trashed.clone(),
            file_order: i,
        };
        entries.push((record, trashed, i));
    }
    // 버린 시각이 늦은 것부터, 같으면 목록에 적힌 차례
    entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)));
    scan.entries = entries.into_iter().map(|(record, _, _)| record).collect();
    scan
}
