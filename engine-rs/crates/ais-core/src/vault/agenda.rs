//! 아젠다(노트를 묶는 폴더 단위) 읽기. `.aisurada/agendas/*.md`의 머리를 한 줄짜리 `키: 값` 읽개로 읽습니다.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::Skipped;
use crate::text::{compare_utf16, is_blank, is_whitespace, trim, utf16_len};

static AGENDA_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^ag[0-9a-z]+-[0-9a-f]{8}$").unwrap());

const MAX_FILE_BYTES: u64 = 1_000_000;
const MAX_FRONTMATTER_UNITS: usize = 16_000;
pub(super) const HUES: [&str; 12] = [
    "blue", "violet", "teal", "orange", "pink", "green", "red", "amber", "lime", "cyan", "fuchsia",
    "brown",
];

/// 아젠다 하나
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgendaInfo {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub hue: String,
    pub repo: Option<String>,
    pub project: Option<String>,
    /// 프로젝트의 저장소 가운데 이 아젠다가 쓰는 이름
    pub project_repo: Option<String>,
    pub summary: String,
}

/// slug와 id로 아젠다를 찾는 표
#[derive(Debug, Default)]
pub struct AgendaTable {
    /// slug가 처음 나온 차례
    slugs: Vec<String>,
    slug_to_info: HashMap<String, AgendaInfo>,
    /// id가 처음 나온 차례. 같은 id의 파일이 둘이면 뒤 파일의 값이 남는다.
    id_order: Vec<String>,
    id_to_info: HashMap<String, AgendaInfo>,
}

impl AgendaTable {
    fn insert(&mut self, info: AgendaInfo) {
        if self
            .slug_to_info
            .insert(info.slug.clone(), info.clone())
            .is_none()
        {
            self.slugs.push(info.slug.clone());
        }
        if self
            .id_to_info
            .insert(info.id.clone(), info.clone())
            .is_none()
        {
            self.id_order.push(info.id.clone());
        }
    }

    pub fn has_slug(&self, slug: &str) -> bool {
        self.slug_to_info.contains_key(slug)
    }

    pub fn by_id(&self, id: &str) -> Option<&AgendaInfo> {
        self.id_to_info.get(id)
    }

    pub fn by_slug(&self, slug: &str) -> Option<&AgendaInfo> {
        self.slug_to_info.get(slug)
    }

    pub fn title_of(&self, slug: &str) -> &str {
        self.slug_to_info
            .get(slug)
            .map(|a| a.title.as_str())
            .unwrap_or("")
    }

    /// 아젠다 slug 개수(`reindex`의 `agendas`)
    pub fn slug_count(&self) -> usize {
        self.slugs.len()
    }

    /// id마다 하나씩, 처음 나온 차례로
    pub fn infos(&self) -> impl Iterator<Item = &AgendaInfo> {
        self.id_order
            .iter()
            .filter_map(|id| self.id_to_info.get(id))
    }

    /// slug와 아젠다 제목(대소문자 무시)으로 찾는 `(slug, title)` 목록
    pub fn slug_title_pairs(&self) -> impl Iterator<Item = (&str, &str)> {
        self.slugs.iter().filter_map(|s| {
            self.slug_to_info
                .get(s)
                .map(|a| (a.slug.as_str(), a.title.as_str()))
        })
    }
}

/// 아젠다 훑기 결과
#[derive(Debug, Default)]
pub struct AgendaScan {
    pub table: AgendaTable,
    pub skipped: Vec<Skipped>,
}

fn skip(skipped: &mut Vec<Skipped>, path: &str, reason: &str) {
    skipped.push(Skipped {
        kind: "agenda",
        path: path.to_string(),
        reason: reason.to_string(),
    });
}

/// `.aisurada/agendas/`를 훑습니다. 읽기 명령용입니다: 같은 slug에 다른 id가 있으면 표 전체를 비웁니다.
pub fn scan_agendas(vault_root: &Path) -> AgendaScan {
    let mut scan = AgendaScan::default();
    let dir = vault_root.join(".aisurada/agendas");
    let Ok(read_dir) = fs::read_dir(&dir) else {
        return scan;
    };

    let mut files: Vec<_> = read_dir
        .flatten()
        .filter(|e| fs::metadata(e.path()).map(|m| m.is_file()).unwrap_or(false))
        .filter_map(|e| {
            e.file_name()
                .into_string()
                .ok()
                .map(|name| (name, e.path()))
        })
        .filter(|(name, _)| name.ends_with(".md"))
        .collect();
    files.sort_by(|a, b| compare_utf16(&a.0, &b.0));

    for (name, path) in files {
        let rel = format!(".aisurada/agendas/{name}");
        let Ok(meta) = fs::metadata(&path) else {
            skip(&mut scan.skipped, &rel, "파일을 못 읽었다");
            continue;
        };
        if meta.len() > MAX_FILE_BYTES {
            skip(&mut scan.skipped, &rel, "파일이 크기 상한을 넘었다");
            continue;
        }
        let Some(content) = fs::read(&path).ok().and_then(|b| String::from_utf8(b).ok()) else {
            skip(&mut scan.skipped, &rel, "파일을 못 읽었다");
            continue;
        };
        let content = content.strip_prefix('\u{FEFF}').unwrap_or(&content);

        let Some((front, body)) = split_fence(content) else {
            skip(&mut scan.skipped, &rel, "프론트매터가 없다");
            continue;
        };
        let kv = parse_line_reader(front);
        let file_base = &name[..name.len() - 3];

        let slug = match kv.get("slug") {
            Some(v) if !is_blank(v) => trim(v).to_string(),
            _ => file_base.to_string(),
        };
        if !is_valid_slug_segment(&slug) {
            skip(
                &mut scan.skipped,
                &rel,
                "slug 가 안전한 경로 세그먼트가 아니다",
            );
            continue;
        }

        let id = kv
            .get("id")
            .map(|v| trim(v).to_string())
            .unwrap_or_default();
        if !is_valid_agenda_id(&id) {
            skip(
                &mut scan.skipped,
                &rel,
                "아젠다 id 가 없거나 모양이 어긋난다",
            );
            continue;
        }

        let title = match kv.get("title") {
            Some(v) if !is_blank(v) => trim(v).to_string(),
            _ => {
                let extracted = extract_title_from_body(body);
                if extracted.is_empty() {
                    slug.clone()
                } else {
                    extracted
                }
            }
        };
        if is_blank(&title) {
            skip(&mut scan.skipped, &rel, "아젠다 제목이 비었다");
            continue;
        }

        let hue = match kv.get("hue").map(|h| trim(h)) {
            Some(h) if HUES.contains(&h) => h.to_string(),
            _ => "blue".to_string(),
        };
        let repo = kv
            .get("repo")
            .map(|r| trim(r).to_string())
            .filter(|r| !r.is_empty());
        let project = kv
            .get("project")
            .map(|p| trim(p).to_string())
            .filter(|p| is_valid_slug_segment(p));
        let project_repo = kv
            .get("projectRepo")
            .map(|r| trim(r).to_string())
            .filter(|r| project.is_some() && is_valid_slug_segment(r));

        if let Some(existing) = scan.table.slug_to_info.get(&slug)
            && existing.id != id
        {
            // 같은 slug에 다른 id: 읽기 명령은 아젠다 훑기를 멈추고 표를 비운다
            scan.table = AgendaTable::default();
            return scan;
        }
        scan.table.insert(AgendaInfo {
            id,
            slug,
            title,
            hue,
            repo,
            project,
            project_repo,
            summary: trim(body).to_string(),
        });
    }
    scan
}

/// 아젠다·프로젝트 파일의 울타리 가르기. `---`로 시작해야 하고, 닫는 `\n---`가 있어야 하고, 머리가 16,000자 이하여야 한다.
pub(super) fn split_fence(content: &str) -> Option<(&str, &str)> {
    let after_open = content.strip_prefix("---")?;
    let first_lf = 3 + after_open.find('\n')?;
    let close = first_lf + content[first_lf..].find("\n---")?;
    let front = &content[first_lf + 1..close + 1];
    if utf16_len(front) > MAX_FRONTMATTER_UNITS {
        return None;
    }
    Some((front, &content[close + 4..]))
}

/// 아젠다·프로젝트 slug가 경로 한 조각으로 안전한 모양인지 봅니다.
pub fn is_valid_slug_segment(s: &str) -> bool {
    !s.is_empty()
        && utf16_len(s) <= 100
        && !s.starts_with('.')
        && !s.contains('/')
        && !s.contains('\\')
        && !s.contains("..")
        && s.chars().all(|c| c >= ' ')
}

pub fn is_valid_agenda_id(id: &str) -> bool {
    id == "ag-recovered" || AGENDA_ID.is_match(id)
}

/// 본문 첫 줄에서 `#` 머리를 떼고 제목으로 씁니다.
fn extract_title_from_body(body: &str) -> String {
    let trimmed = trim(body);
    let first_line = trimmed.split(['\r', '\n']).next().unwrap_or("");
    let without_hashes = first_line.trim_start_matches('#');
    let line = if without_hashes.len() != first_line.len() {
        without_hashes.trim_start_matches([' ', '\t', '\u{B}', '\u{C}'])
    } else {
        first_line
    };
    trim(line).to_string()
}

/// 한 줄짜리 `키: 값` 읽개. 들여쓴 줄, `#` 줄, 콜론 뒤에 공백이 없는 줄은 건너뛴다.
pub(super) fn parse_line_reader(text: &str) -> HashMap<String, String> {
    let mut result = HashMap::new();
    for line in text.split(['\n', '\r']) {
        let Some(first) = line.chars().next() else {
            continue;
        };
        if is_whitespace(first) || first == '#' || is_blank(line) {
            continue;
        }
        let Some(colon) = line.find(':') else {
            continue;
        };
        if colon == 0 {
            continue;
        }
        if let Some(after) = line[colon + 1..].chars().next()
            && after != ' '
        {
            continue;
        }
        let key = trim(&line[..colon]);
        if key.is_empty() {
            continue;
        }
        let mut value = trim(&line[colon + 1..]).to_string();
        if value.chars().count() >= 2 {
            if value.starts_with('\'') && value.ends_with('\'') {
                value = value[1..value.len() - 1].replace("''", "'");
            } else if value.starts_with('"') && value.ends_with('"') {
                value = unescape_double_quoted(&value[1..value.len() - 1]);
            }
        }
        result.insert(key.to_string(), trim(&value).to_string());
    }
    result
}

fn unescape_double_quoted(inner: &str) -> String {
    let mut out = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek().is_some() {
            let next = chars.next().unwrap();
            out.push(if next == 'n' { '\n' } else { next });
        } else {
            out.push(c);
        }
    }
    out
}
