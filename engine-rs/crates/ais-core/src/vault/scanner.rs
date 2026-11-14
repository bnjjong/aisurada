//! vault의 `notes/` 폴더 훑기. 노트와 후속 노트를 찾아 메타를 만들고, 건너뛴 것과 충돌 사본을 모읍니다.
//!
//! 후보를 먼저 모두 모은 뒤(이름 차례) 하나씩 연다. 읽기 명령은 예상 밖의 문제를 만나도 훑기를 멈추지 않고
//! 그 노트만 이유를 적어 건너뛴다(Java 엔진은 이럴 때 뒤쪽 노트를 통째로 버렸다).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use super::agenda::AgendaTable;
use super::note_file::{ParsedNoteFile, read_note_file};
use super::note_meta::{MetaData, MetaError, parse_meta};
use super::{Conflict, Skipped};
use crate::text::compare_utf16;

static CONFLICT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)conflicted copy|\(conflicted\b|충돌(된)?\s*사본|conflicted version").unwrap()
});
static YEAR_MONTH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[0-9]{4}-(0[1-9]|1[0-2])$").unwrap());

const RESERVED_NAMES: [&str; 4] = ["attach", "sub", "sub-comments", "chats"];
const MAX_SCAN_DEPTH: usize = 8;

/// 본문 형식
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Markdown,
    Json,
    Html,
}

impl Format {
    pub fn as_str(self) -> &'static str {
        match self {
            Format::Markdown => "markdown",
            Format::Json => "json",
            Format::Html => "html",
        }
    }

    fn from_file_name(name: &str) -> Self {
        if name.ends_with(".json") {
            Format::Json
        } else if name.ends_with(".html") {
            Format::Html
        } else {
            Format::Markdown
        }
    }
}

/// 훑어서 찾은 노트(메인 또는 후속) 하나
#[derive(Debug, Clone)]
pub struct NoteRecord {
    pub id: String,
    pub is_sub_note: bool,
    pub root_note_id: String,
    /// 후속 노트의 바로 위 노트 id
    pub direct_parent_id: Option<String>,
    /// vault 기준 상대 경로(`/` 구분)
    pub rel_path: String,
    pub format: Format,
    pub content_file: PathBuf,
    pub meta: MetaData,
    pub body: String,
    /// 고치기 전 확인 값. JSON 노트와 확장자 폴더 노트에는 없다.
    pub token: Option<String>,
    /// 파일 하나로 된 노트(폴더가 아님)
    pub is_single_file: bool,
    /// 폴더 이름이 `.md`·`.html`·`.json`으로 끝나는 노트
    pub is_extension_folder: bool,
    /// 메인 노트일 때, 이 노트 아래 모든 후속 노트(깊이와 상관없이 찾은 차례)
    pub sub_notes: Vec<NoteRecord>,
}

impl NoteRecord {
    /// JSON 노트와 확장자 폴더 노트는 본문과 확인 값, 도장을 내지 않는다.
    pub fn hides_body_and_token(&self) -> bool {
        self.format == Format::Json || self.is_extension_folder
    }
}

/// 훑기 결과
#[derive(Debug, Default)]
pub struct NoteScan {
    /// 메인 노트, 찾은 차례
    pub notes: Vec<NoteRecord>,
    pub skipped: Vec<Skipped>,
    pub conflicts: Vec<Conflict>,
    /// 월별 폴더(`YYYY-MM`) 배치인 vault에서 그 자리에 있지 않은 노트 경로
    pub unsharded: Vec<String>,
}

struct Candidate {
    path: PathBuf,
    body_file: PathBuf,
    agenda: String,
    single: bool,
}

struct Collector {
    vault: PathBuf,
    skipped: Vec<Skipped>,
    conflicts: Vec<Conflict>,
    candidates: Vec<Candidate>,
    seen: HashSet<PathBuf>,
}

fn is_dir(path: &Path) -> bool {
    fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false)
}

fn is_file(path: &Path) -> bool {
    fs::metadata(path).map(|m| m.is_file()).unwrap_or(false)
}

fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// 폴더 안의 항목을 이름의 UTF-16 차례로 돌려준다.
fn sorted_entries(dir: &Path) -> Option<Vec<(String, PathBuf)>> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok().map(|n| (n, e.path())))
        .collect();
    entries.sort_by(|a, b| compare_utf16(&a.0, &b.0));
    Some(entries)
}

/// 폴더 노트의 본문 파일: `note.md`, 없으면 `note.json`, 없으면 `note.html`
pub fn find_folder_content_file(folder: &Path) -> Option<PathBuf> {
    ["note.md", "note.json", "note.html"]
        .iter()
        .map(|n| folder.join(n))
        .find(|p| is_file(p))
}

fn is_single_file_note_name(name: &str) -> bool {
    name != "comments.json"
        && name != "ask.json"
        && (name.ends_with(".md") || name.ends_with(".html") || name.ends_with(".json"))
}

impl Collector {
    fn rel(&self, path: &Path) -> String {
        path.strip_prefix(&self.vault)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }

    fn skip(&mut self, path: &str, reason: &str) {
        self.skipped.push(Skipped {
            kind: "note",
            path: path.to_string(),
            reason: reason.to_string(),
        });
    }

    fn collect_all(&mut self) {
        let notes = self.vault.join("notes");
        let Some(entries) = sorted_entries(&notes) else {
            return;
        };
        for (name, path) in entries {
            if is_dir(&path) {
                self.collect_folder(&path, &name, 0);
            }
        }
    }

    fn collect_folder(&mut self, dir: &Path, agenda: &str, depth: usize) {
        if let Some(entries) = sorted_entries(dir) {
            for (name, path) in entries {
                self.collect_entry(&name, &path, agenda, depth);
            }
        }
    }

    fn collect_entry(&mut self, name: &str, entry: &Path, agenda: &str, depth: usize) {
        let rel = self.rel(entry);
        if RESERVED_NAMES.contains(&name) {
            return;
        }
        if is_symlink(entry) {
            self.skip(&rel, "심링크라 따라가지 않았다");
            return;
        }
        if CONFLICT.is_match(name) {
            self.conflicts.push(Conflict {
                kind: "note",
                path: rel,
            });
            return;
        }
        if is_dir(entry) {
            if let Some(body) = find_folder_content_file(entry) {
                if self.seen.insert(entry.to_path_buf()) {
                    self.candidates.push(Candidate {
                        path: entry.to_path_buf(),
                        body_file: body,
                        agenda: agenda.to_string(),
                        single: false,
                    });
                }
            } else if depth < MAX_SCAN_DEPTH {
                self.collect_folder(entry, agenda, depth + 1);
            } else {
                self.skip(&rel, "스캔 깊이 상한을 넘었다");
            }
        } else if is_file(entry)
            && is_single_file_note_name(name)
            && self.seen.insert(entry.to_path_buf())
        {
            self.candidates.push(Candidate {
                path: entry.to_path_buf(),
                body_file: entry.to_path_buf(),
                agenda: agenda.to_string(),
                single: true,
            });
        }
    }
}

/// vault의 `notes/`를 훑어 노트를 모읍니다.
pub fn scan_notes(vault_root: &Path, agendas: &AgendaTable) -> NoteScan {
    let mut collector = Collector {
        vault: vault_root.to_path_buf(),
        skipped: Vec::new(),
        conflicts: Vec::new(),
        candidates: Vec::new(),
        seen: HashSet::new(),
    };
    collector.collect_all();

    let mut notes: Vec<NoteRecord> = Vec::new();
    let mut ids: HashSet<String> = HashSet::new();
    let mut opening_skipped: Vec<Skipped> = Vec::new();
    let candidates = std::mem::take(&mut collector.candidates);
    for candidate in &candidates {
        if let Some(record) = open_root(
            &collector.vault,
            candidate,
            agendas,
            &mut ids,
            &mut opening_skipped,
        ) {
            notes.push(record);
        }
    }

    let mut skipped = std::mem::take(&mut collector.skipped);
    skipped.extend(opening_skipped);

    let mut unsharded = Vec::new();
    if !candidates.is_empty() && is_sharded(&collector.vault) {
        for candidate in &candidates {
            let rel = collector.rel(&candidate.path);
            let parts: Vec<&str> = rel.split('/').collect();
            if parts.len() != 4 || !YEAR_MONTH.is_match(parts[2]) {
                unsharded.push(rel);
            }
        }
        unsharded.sort_by(|a, b| compare_utf16(a, b));
    }
    NoteScan {
        notes,
        skipped,
        conflicts: std::mem::take(&mut collector.conflicts),
        unsharded,
    }
}

/// 월별 폴더 배치인지 디스크로 판정한다: `notes/<아젠다>/YYYY-MM/` 아래에 노트가 바로 있는 곳이 하나라도 있으면 그렇다.
fn is_sharded(vault: &Path) -> bool {
    let Ok(agendas) = fs::read_dir(vault.join("notes")) else {
        return false;
    };
    for agenda in agendas.flatten() {
        if !is_dir(&agenda.path()) {
            continue;
        }
        let Ok(months) = fs::read_dir(agenda.path()) else {
            continue;
        };
        for month in months.flatten() {
            let name = month.file_name().to_string_lossy().to_string();
            if is_dir(&month.path())
                && YEAR_MONTH.is_match(&name)
                && contains_direct_note(&month.path())
            {
                return true;
            }
        }
    }
    false
}

fn contains_direct_note(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|e| {
        let name = e.file_name().to_string_lossy().to_string();
        let path = e.path();
        (is_dir(&path) && find_folder_content_file(&path).is_some())
            || (is_file(&path) && is_single_file_note_name(&name))
    })
}

fn skip_reason(error: &MetaError, sub: bool) -> String {
    match error {
        MetaError::BadCreated => "created 가 달력에 없는 날짜다".to_string(),
        MetaError::UnknownAgenda(slug) if !sub => {
            format!("아젠다 slug 를 식별자로 못 바꾼다: {slug}")
        }
        MetaError::BadEvaluation => "평가 점수를 읽지 못했다".to_string(),
        _ if sub => "하위 노트 id 가 없거나 모양이 어긋난다".to_string(),
        _ => "노트 id 가 없거나 모양이 어긋난다".to_string(),
    }
}

fn open_root(
    vault: &Path,
    candidate: &Candidate,
    agendas: &AgendaTable,
    ids: &mut HashSet<String>,
    skipped: &mut Vec<Skipped>,
) -> Option<NoteRecord> {
    let rel = candidate
        .path
        .strip_prefix(vault)
        .unwrap_or(&candidate.path)
        .to_string_lossy()
        .replace('\\', "/");
    let mut skip = |reason: &str| {
        skipped.push(Skipped {
            kind: "note",
            path: rel.clone(),
            reason: reason.to_string(),
        })
    };
    let body_name = candidate
        .body_file
        .file_name()?
        .to_string_lossy()
        .to_string();
    let format = Format::from_file_name(&body_name);

    let parsed = match read_note_file(&candidate.body_file) {
        Ok(p) => p,
        Err(_) => {
            skip("본문 파일을 못 읽었다");
            return None;
        }
    };
    let Some(front) = parsed.frontmatter.as_ref() else {
        skip("프론트매터가 없다");
        return None;
    };
    let meta = match parse_meta(front, &candidate.agenda, Some(agendas), false, None) {
        Ok(m) => m,
        Err(e) => {
            skip(&skip_reason(&e, false));
            return None;
        }
    };
    if !ids.insert(meta.id.clone()) {
        skip(&format!("이미 읽은 노트가 같은 id 를 쓴다: {}", meta.id));
        return None;
    }

    let candidate_name = candidate.path.file_name()?.to_string_lossy().to_string();
    let is_extension_folder = !candidate.single
        && (candidate_name.ends_with(".md")
            || candidate_name.ends_with(".html")
            || candidate_name.ends_with(".json"));

    let ParsedNoteFile { body, token, .. } = parsed;
    let (meta, body, token) = if format == Format::Json || is_extension_folder {
        // JSON 노트와 확장자 폴더 노트: 본문은 비고, 확인 값·도장·세션 id는 없다
        let hidden = MetaData {
            session_id: None,
            session_agent: None,
            edited_by: None,
            edited_at: None,
            edited_session_id: None,
            edited_session_agent: None,
            ..meta
        };
        (hidden, String::new(), None)
    } else {
        (meta, body, Some(token))
    };

    let mut record = NoteRecord {
        id: meta.id.clone(),
        is_sub_note: false,
        root_note_id: meta.id.clone(),
        direct_parent_id: None,
        rel_path: rel.clone(),
        format,
        content_file: candidate.body_file.clone(),
        meta,
        body,
        token,
        is_single_file: candidate.single,
        is_extension_folder,
        sub_notes: Vec::new(),
    };

    if !candidate.single && is_dir(&candidate.path.join("sub")) {
        let mut subs = Vec::new();
        let mut sub_ids = HashSet::new();
        scan_sub_notes(
            vault,
            &candidate.path,
            &record.id.clone(),
            &record.id.clone(),
            &candidate.agenda,
            1,
            &mut subs,
            &mut sub_ids,
            skipped,
        );
        record.sub_notes = subs;
    }
    Some(record)
}

#[allow(clippy::too_many_arguments)]
fn scan_sub_notes(
    vault: &Path,
    container: &Path,
    root_id: &str,
    direct_parent_id: &str,
    agenda: &str,
    nest_level: usize,
    subs: &mut Vec<NoteRecord>,
    sub_ids: &mut HashSet<String>,
    skipped: &mut Vec<Skipped>,
) {
    if nest_level > 8 {
        return; // 9겹째부터는 이유 없이 조용히 안 본다
    }
    let sub_dir = container.join("sub");
    if !is_dir(&sub_dir) {
        return;
    }
    let Some(entries) = sorted_entries(&sub_dir) else {
        return;
    };

    let rel = |p: &Path| {
        p.strip_prefix(vault)
            .unwrap_or(p)
            .to_string_lossy()
            .replace('\\', "/")
    };
    const DESCENDANT_REASON: &str = "직계 부모 하위 노트를 못 읽어 이 하위 노트도 건너뛴다";

    for (name, entry) in entries {
        if is_symlink(&entry) || RESERVED_NAMES.contains(&name.as_str()) || CONFLICT.is_match(&name)
        {
            continue;
        }
        let rel_path = rel(&entry);
        let (content_file, is_folder) = if is_dir(&entry) {
            match find_folder_content_file(&entry) {
                Some(f) => (f, true),
                None => continue, // 본문 파일 없는 폴더는 하위 노트가 아니고 아래로도 안 내려간다
            }
        } else if is_file(&entry) && nest_level == 1 && name.ends_with(".md") {
            (entry.clone(), false)
        } else {
            continue;
        };

        let skip = |reason: &str, skipped: &mut Vec<Skipped>| {
            skipped.push(Skipped {
                kind: "note",
                path: rel_path.clone(),
                reason: reason.to_string(),
            });
            skip_descendants(vault, &entry, skipped, DESCENDANT_REASON);
        };

        let parsed = match read_note_file(&content_file) {
            Ok(p) => p,
            Err(_) => {
                skip("본문 파일을 못 읽었다", skipped);
                continue;
            }
        };
        let Some(front) = parsed.frontmatter.as_ref() else {
            skip("프론트매터가 없다", skipped);
            continue;
        };
        let meta = match parse_meta(front, agenda, None, true, Some(root_id)) {
            Ok(m) => m,
            Err(e) => {
                skip(&skip_reason(&e, true), skipped);
                continue;
            }
        };
        if !sub_ids.insert(meta.id.clone()) {
            skip(
                &format!("이미 읽은 노트가 같은 id 를 쓴다: {}", meta.id),
                skipped,
            );
            continue;
        }

        let format = Format::from_file_name(
            &content_file
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
        );
        let sub_id = meta.id.clone();
        subs.push(NoteRecord {
            id: sub_id.clone(),
            is_sub_note: true,
            root_note_id: root_id.to_string(),
            direct_parent_id: Some(direct_parent_id.to_string()),
            rel_path,
            format,
            content_file,
            meta,
            body: parsed.body,
            token: Some(parsed.token),
            is_single_file: !is_folder,
            is_extension_folder: false,
            sub_notes: Vec::new(),
        });
        if is_folder {
            scan_sub_notes(
                vault,
                &entry,
                root_id,
                &sub_id,
                agenda,
                nest_level + 1,
                subs,
                sub_ids,
                skipped,
            );
        }
    }
}

fn skip_descendants(vault: &Path, container: &Path, skipped: &mut Vec<Skipped>, reason: &str) {
    let sub = container.join("sub");
    if !is_dir(&sub) {
        return;
    }
    let Some(entries) = sorted_entries(&sub) else {
        return;
    };
    for (_, path) in entries {
        if is_symlink(&path) {
            continue;
        }
        let rel = path
            .strip_prefix(vault)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        skipped.push(Skipped {
            kind: "note",
            path: rel,
            reason: reason.to_string(),
        });
        if is_dir(&path) {
            skip_descendants(vault, &path, skipped, reason);
        }
    }
}
