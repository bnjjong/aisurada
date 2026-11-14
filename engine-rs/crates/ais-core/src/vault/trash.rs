//! 휴지통 읽기. `.aisurada/trash/trash.json`(JSON이자 YAML인 목록)을 읽어 항목마다 검사합니다.

use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::note_meta::parse_created;
use crate::frontmatter::{self, Value};
use crate::text::{NOTE_ID, trim};

static TRASH_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^tr[0-9a-z]+-[0-9a-f]{8}$").unwrap());
static AGENDA_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^ag[0-9a-z]+-[0-9a-f]{8}$").unwrap());

const MANIFEST: &str = ".aisurada/trash/trash.json";
const MAX_MANIFEST_BYTES: u64 = 10_000_000;
const DOC_TYPES: [&str; 6] = ["plan", "build", "report", "research", "meeting", "free"];

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
            doc_type: if DOC_TYPES.contains(&doc_type) {
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
        };
        entries.push((record, trashed, i));
    }
    // 버린 시각이 늦은 것부터, 같으면 목록에 적힌 차례
    entries.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)));
    scan.entries = entries.into_iter().map(|(record, _, _)| record).collect();
    scan
}
