//! 28장: `shard_notes`(색인에 들지 못한 노트까지 디스크에서 모아 계획하고 원자적으로 옮긴다)와 `verify_shard`(이동 전 매핑의 수와
//! 현재 디스크를 대조한다). 규칙과 응답은 Java 엔진의 `ShardNotes`·`VerifyShard`와 같다.

use crate::ports::fs;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;
use serde_json::{Map, Value as Json, json};

use super::option_text::text_of;
use crate::error::EngineError;
use crate::frontmatter::Value;
use crate::text::{compare_utf16, is_blank, trim};
use crate::vault::file_size::{self, FileSize};
use crate::vault::note_file::read_note_file;
use crate::vault::note_meta::parse_created;
use crate::vault::scanner::{
    find_folder_content_file, is_conflict_name, is_reserved_name, is_sharded,
};
use crate::write::apparatus::now_millis;
use crate::write::atomic::write_atomic;
use crate::write::confine::{absolute_normalized, check_segment};
use crate::ports::fs::check_confinement;
use crate::write::doc_place::move_path;
use crate::write::local_time::local_clock;

static NOTE_FILE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)^.*\.(md|html|json)$").unwrap());

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ShardNotesRequest {
    pub apply: Option<Json>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct VerifyShardRequest {
    pub file: Option<Json>,
}

struct Candidate {
    id: String,
    old_rel: String,
    new_rel: String,
    size: FileSize,
    reason: String,
    agenda: String,
}

fn candidate(rel: &str, reason: &str, agenda: &str) -> Candidate {
    Candidate {
        id: String::new(),
        old_rel: rel.to_string(),
        new_rel: String::new(),
        size: FileSize::default(),
        reason: reason.to_string(),
        agenda: agenda.to_string(),
    }
}

/// `true`/`false`만 받는 옵션. 비면 거짓이다.
pub(crate) fn bool_option(raw: &Option<Json>, key: &str) -> Result<bool, EngineError> {
    let value = text_of(raw)
        .map(|r| trim(&r).to_string())
        .unwrap_or_default();
    match value.as_str() {
        "" | "false" => Ok(false),
        "true" => Ok(true),
        _ => Err(EngineError::invalid_argument(format!(
            "{key} 은 true 나 false 여야 한다: {value}"
        ))),
    }
}

fn children(dir: &Path) -> Option<Vec<PathBuf>> {
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .collect();
    paths.sort_by(|a, b| compare_utf16(&file_name(a), &file_name(b)));
    Some(paths)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn rel_of(vault: &Path, path: &Path) -> String {
    path.strip_prefix(vault)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn is_dir(path: &Path) -> bool {
    fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false)
}

fn is_file(path: &Path) -> bool {
    fs::metadata(path).map(|m| m.is_file()).unwrap_or(false)
}

fn collect(
    vault: &Path,
    dir: &Path,
    agenda: &str,
    depth: usize,
    sharded: bool,
    out: &mut Vec<Candidate>,
) {
    let Some(paths) = children(dir) else {
        return;
    };
    for path in paths {
        let name = file_name(&path);
        let relative = rel_of(vault, &path);
        if is_reserved_name(&name) {
            continue;
        }
        let reason = if fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
            Some("심링크라 따라가지 않았다")
        } else if is_conflict_name(&name) {
            Some("동기화 충돌 사본이다")
        } else {
            None
        };
        if let Some(reason) = reason {
            out.push(candidate(&relative, reason, agenda));
            continue;
        }
        let body = if is_dir(&path) {
            match find_folder_content_file(&path) {
                Some(body) => body,
                None => {
                    if depth >= 8 {
                        out.push(candidate(&relative, "스캔 깊이 상한을 넘었다", agenda));
                    } else {
                        collect(vault, &path, agenda, depth + 1, sharded, out);
                    }
                    continue;
                }
            }
        } else if is_file(&path)
            && NOTE_FILE.is_match(&name)
            && name != "comments.json"
            && name != "ask.json"
        {
            path.clone()
        } else {
            continue;
        };
        let (mut id, mut destination, mut reason) = (String::new(), String::new(), String::new());
        match read_note_file(&body) {
            Err(_) => reason = "본문 파일을 못 읽었다".to_string(),
            Ok(parsed) => match parsed.frontmatter {
                None => reason = "프론트매터가 없다".to_string(),
                Some(meta) => {
                    let get = |key: &str| {
                        meta.iter()
                            .find(|(k, _)| matches!(k, Value::Str(s) if s == key))
                            .map(|(_, v)| v)
                    };
                    if let Some(Value::Str(s)) = get("id") {
                        id = s.clone();
                    }
                    let created = match get("created") {
                        Some(Value::Str(s)) => parse_created(s),
                        _ => None,
                    };
                    match created {
                        None => reason = "created 가 달력에 없는 날짜다".to_string(),
                        Some(c) => {
                            let month = if sharded {
                                format!("/{}", &c.date[..7])
                            } else {
                                String::new()
                            };
                            destination = format!("notes/{agenda}{month}/{name}");
                        }
                    }
                }
            },
        }
        out.push(Candidate {
            id,
            old_rel: relative,
            new_rel: destination,
            size: file_size::measure(&path),
            reason,
            agenda: agenda.to_string(),
        });
    }
}

fn mapping_path(vault: &Path, apply: bool) -> PathBuf {
    let directory = absolute_normalized(&vault.join(".aisurada/migration"));
    if !apply {
        return directory.join("shard-plan.tsv");
    }
    let c = local_clock(now_millis());
    let stem = format!(
        "shard-map-{}{:02}{:02}-{:02}{:02}{:02}",
        &c.date.era_text()[..c.date.era_text().find('-').unwrap_or(4)],
        c.date.month,
        c.date.day,
        c.hour,
        c.minute,
        c.second
    );
    let mut file = directory.join(format!("{stem}.tsv"));
    let mut n = 2;
    while file.exists() {
        file = directory.join(format!("{stem}~{n}.tsv"));
        n += 1;
    }
    file
}

pub fn shard_notes(vault: &Path, request: &ShardNotesRequest) -> Result<Json, EngineError> {
    let apply = bool_option(&request.apply, "apply")?;
    migrate(vault, apply)
}

fn migrate(vault: &Path, apply: bool) -> Result<Json, EngineError> {
    let sharded = is_sharded(vault);
    let mut candidates: Vec<Candidate> = Vec::new();
    let notes = vault.join("notes");
    if is_dir(&notes) {
        let agendas = children(&notes).ok_or_else(|| {
            EngineError::internal(format!("노트 폴더를 읽지 못했다: {}", notes.display()))
        })?;
        for agenda in agendas {
            if is_dir(&agenda) {
                collect(
                    vault,
                    &agenda,
                    &file_name(&agenda),
                    0,
                    sharded,
                    &mut candidates,
                );
            }
        }
    }
    let mut totals: HashMap<String, usize> = HashMap::new();
    for c in &candidates {
        *totals.entry(c.agenda.clone()).or_default() += 1;
    }
    candidates.sort_by(|a, b| {
        totals[&a.agenda]
            .cmp(&totals[&b.agenda])
            .then_with(|| compare_utf16(&a.agenda, &b.agenda))
            .then_with(|| compare_utf16(&a.old_rel, &b.old_rel))
    });
    let mapping = mapping_path(vault, apply);
    let mut tsv = String::from("noteId\toldRel\tnewRel\tfileCount\tbyteSum\treason\n");
    for c in &candidates {
        tsv.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\n",
            c.id,
            c.old_rel,
            c.new_rel,
            c.size.files,
            c.size.bytes,
            c.reason.replace(['\t', '\n'], " ")
        ));
    }
    write_atomic(&mapping, tsv.as_bytes())?;

    let (mut planned, mut moved, mut already, mut pruned) = (0usize, 0usize, 0usize, 0usize);
    let mut unmovable: Vec<Json> = Vec::new();
    let mut agendas: HashMap<String, [usize; 2]> = HashMap::new();
    let mut old_parents: Vec<PathBuf> = Vec::new();
    let mut stopped: Option<Json> = None;
    for c in &candidates {
        if c.new_rel.is_empty() {
            unmovable.push(json!({"path": c.old_rel, "reason": c.reason}));
            continue;
        }
        if c.new_rel == c.old_rel {
            already += 1;
            continue;
        }
        planned += 1;
        agendas.entry(c.agenda.clone()).or_default()[0] += 1;
    }
    if apply {
        for c in &candidates {
            if c.new_rel.is_empty() || c.new_rel == c.old_rel {
                continue;
            }
            let source = vault.join(&c.old_rel);
            let target = vault.join(&c.new_rel);
            check_segment(vault, &c.new_rel)?;
            check_confinement(vault, &target)?;
            move_path(&source, &target)?;
            let after = file_size::measure(&target);
            if after != c.size {
                stopped = Some(json!({
                    "notePath": c.old_rel,
                    "movedTo": c.new_rel,
                    "fileCountBefore": c.size.files,
                    "fileCountAfter": after.files,
                    "byteSumBefore": c.size.bytes,
                    "byteSumAfter": after.bytes,
                }));
                break;
            }
            moved += 1;
            agendas.entry(c.agenda.clone()).or_default()[1] += 1;
            if let Some(parent) = source.parent()
                && !old_parents.iter().any(|p| p == parent)
            {
                old_parents.push(parent.to_path_buf());
            }
        }
    }
    for parent in &old_parents {
        let relative = parent.strip_prefix(vault).unwrap_or(parent);
        if relative.components().count() < 3 || !relative.starts_with("notes") {
            continue;
        }
        if fs::remove_dir(parent).is_ok() {
            pruned += 1;
        }
    }
    let mut names: Vec<&String> = agendas.keys().collect();
    names.sort_by(|a, b| {
        agendas[*a][0]
            .cmp(&agendas[*b][0])
            .then_with(|| compare_utf16(a, b))
    });
    let by_agenda: Vec<Json> = names
        .iter()
        .map(|a| json!({"agenda": a, "planned": agendas[*a][0], "moved": agendas[*a][1]}))
        .collect();
    let mut result = Map::new();
    result.insert("sharded".into(), json!(sharded));
    result.insert("applied".into(), json!(apply));
    result.insert("mappingFile".into(), json!(mapping.to_string_lossy()));
    result.insert("total".into(), json!(candidates.len()));
    result.insert("planned".into(), json!(planned));
    result.insert("moved".into(), json!(moved));
    result.insert("alreadyInPlace".into(), json!(already));
    result.insert("prunedEmptyDirs".into(), json!(pruned));
    result.insert("unmovable".into(), Json::Array(unmovable));
    result.insert("byAgenda".into(), Json::Array(by_agenda));
    if let Some(stopped) = stopped {
        result.insert("stoppedAt".into(), stopped);
    }
    Ok(Json::Object(result))
}

// ---------------------------------------------------------------- verify_shard

struct Row {
    id: String,
    old_rel: String,
    new_rel: String,
    files: i64,
    bytes: i64,
    reason: String,
}

fn read_rows(file: &Path) -> Result<Vec<Row>, EngineError> {
    if !file.exists() {
        return Ok(Vec::new());
    }
    let bytes = fs::read(file)
        .map_err(|_| EngineError::internal(format!("파일을 읽지 못했다: {}", file.display())))?;
    let text = String::from_utf8_lossy(&bytes);
    let lines = split_keep_empty(&text);
    let mut rows = Vec::new();
    for line in lines.iter().skip(1) {
        if is_blank(line) {
            continue;
        }
        let columns: Vec<&str> = line.split('\t').collect();
        if columns.len() < 6 {
            continue;
        }
        let (Ok(files), Ok(bytes)) = (columns[3].parse::<i64>(), columns[4].parse::<i64>()) else {
            continue;
        };
        rows.push(Row {
            id: columns[0].to_string(),
            old_rel: columns[1].to_string(),
            new_rel: columns[2].to_string(),
            files,
            bytes,
            reason: columns[5].to_string(),
        });
    }
    Ok(rows)
}

/// `split("\r\n|\r|\n", -1)`: 빈 조각도 모두 둔다.
fn split_keep_empty(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\n' => out.push(std::mem::take(&mut current)),
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push(std::mem::take(&mut current));
            }
            _ => current.push(c),
        }
    }
    out.push(current);
    out
}

fn resolve(vault: &Path, rel: &str) -> Result<PathBuf, EngineError> {
    check_segment(vault, rel)?;
    let target = absolute_normalized(&vault.join(rel));
    check_confinement(vault, &target)?;
    Ok(target)
}

/// 옮기기 기준 파일(TSV)과 지금 디스크를 견준다. 읽기뿐이라 워크스페이스 표식이 없어도 돈다(Java 엔진과 같다. 표식을 요구하던 것은 어긋남이었다, 이슈 #57).
pub fn verify_shard(vault: &Path, request: &VerifyShardRequest) -> Result<Json, EngineError> {
    let given = text_of(&request.file)
        .map(|r| trim(&r).to_string())
        .unwrap_or_default();
    let directory = absolute_normalized(&vault.join(".aisurada/migration"));
    let file: PathBuf;
    if !given.is_empty() {
        let missing = || EngineError::not_found(format!("shard map not found: {given}"));
        if !is_dir(&directory) {
            return Err(missing());
        }
        let name = Path::new(&given).file_name().ok_or_else(missing)?;
        file = directory.join(name);
        if !is_file(&file) {
            return Err(missing());
        }
    } else {
        let mut paths: Vec<PathBuf> = Vec::new();
        if is_dir(&directory) {
            for entry in fs::read_dir(&directory)
                .map_err(|e| EngineError::internal(e.to_string()))?
                .flatten()
            {
                let p = entry.path();
                let name = file_name(&p);
                if is_file(&p) && name.starts_with("shard-map-") && name.ends_with(".tsv") {
                    paths.push(p);
                }
            }
        }
        paths.sort_by(|a, b| compare_utf16(&file_name(a), &file_name(b)));
        let mut details: Vec<String> = Vec::new();
        for p in &paths {
            let rows = read_rows(p)?;
            let moving = rows
                .iter()
                .filter(|r| !r.new_rel.is_empty() && r.new_rel != r.old_rel)
                .count();
            details.push(format!(
                "  {} (줄 {}, 자리 바뀜 {moving})",
                file_name(p),
                rows.len()
            ));
        }
        if paths.is_empty() {
            return Err(EngineError::not_found(
                "shard map not found: (이 vault 에 기준값 파일이 없다)",
            ));
        }
        if paths.len() > 1 {
            return Err(EngineError::conflict(format!(
                "기준값 파일이 {} 개다. 어느 회차와 맞댈지 file= 로 못박아야 한다.\n{}\n자리 바뀜이 0 인 회차는 한 건도 안 옮겼다. 그 파일의 크기 값은 이동 뒤에 잰 것이라 기준값이 아니다.",
                paths.len(),
                details.join("\n")
            )));
        }
        file = paths.remove(0);
    }
    let rows = read_rows(&file)?;
    let mut checked = 0usize;
    let item =
        |id: &str, path: &str, reason: &str| json!({"noteId": id, "path": path, "reason": reason});
    let (mut missing, mut left, mut mismatch, mut unmovable): (
        Vec<Json>,
        Vec<Json>,
        Vec<Json>,
        Vec<Json>,
    ) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for row in &rows {
        if row.new_rel.is_empty() {
            unmovable.push(item(&row.id, &row.old_rel, &row.reason));
            continue;
        }
        checked += 1;
        let target = resolve(vault, &row.new_rel)?;
        if !target.exists() {
            missing.push(item(&row.id, &row.new_rel, "옮겨 갔다고 적힌 자리가 없다"));
            continue;
        }
        let size = file_size::measure(&target);
        if row.old_rel != row.new_rel && resolve(vault, &row.old_rel)?.exists() {
            left.push(item(
                &row.id,
                &row.old_rel,
                "옮겼는데 옛 자리에도 남아 있다",
            ));
        }
        if size.files as i64 != row.files || size.bytes as i64 != row.bytes {
            mismatch.push(json!({
                "noteId": row.id,
                "path": row.new_rel,
                "fileCountBefore": row.files,
                "fileCountAfter": size.files,
                "byteSumBefore": row.bytes,
                "byteSumAfter": size.bytes,
            }));
        }
    }
    let ok = missing.is_empty() && left.is_empty() && mismatch.is_empty();
    Ok(json!({
        "ok": ok,
        "mappingFile": file.to_string_lossy(),
        "rows": rows.len(),
        "checked": checked,
        "missing": missing,
        "leftBehind": left,
        "sizeMismatch": mismatch,
        "unmovable": unmovable,
    }))
}
