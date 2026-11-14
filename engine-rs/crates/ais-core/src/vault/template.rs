//! 템플릿 읽기. `.aisurada/templates/doc/`(문서 템플릿)와 `.aisurada/templates/ritual/`(회고 템플릿)의 파일을 읽습니다.

use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::note_file::split_fence;
use crate::frontmatter::{self, Value};
use crate::text::{compare_utf16, is_blank, trim, utf16_len};

static ID_SHAPE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9._-]{1,128}$").unwrap());
static ICON_SHAPE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9]{0,63}$").unwrap());

const DOC_TYPES: [&str; 6] = ["plan", "build", "report", "research", "meeting", "free"];
const MAX_FILE_BYTES: u64 = 1_000_000;
const MAX_FRONTMATTER_UNITS: usize = 16_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Doc,
    Ritual,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Doc => "doc",
            Category::Ritual => "ritual",
        }
    }
}

/// 템플릿 하나. 문서 템플릿은 `doc_type`·`builtin`, 회고 템플릿은 `rev`·`icon`을 쓴다.
#[derive(Debug, Clone)]
pub struct TemplateRecord {
    pub id: String,
    pub category: Category,
    pub name: String,
    pub doc_type: Option<String>,
    pub rev: Option<String>,
    pub icon: Option<String>,
    pub format: &'static str,
    pub desc: String,
    pub body: String,
    pub builtin: Option<bool>,
    pub rel_path: String,
}

#[derive(Debug, Clone)]
pub struct SkippedTemplate {
    pub category: Category,
    pub rel_path: String,
    pub reason: String,
}

#[derive(Debug, Default)]
pub struct TemplateScan {
    pub templates: Vec<TemplateRecord>,
    pub skipped: Vec<SkippedTemplate>,
}

pub fn scan_templates(vault_root: &Path) -> TemplateScan {
    let mut scan = TemplateScan::default();
    for category in [Category::Doc, Category::Ritual] {
        scan_category(vault_root, category, &mut scan);
    }
    scan
}

fn get<'a>(map: &'a [(Value, Value)], key: &str) -> Option<&'a Value> {
    map.iter()
        .find(|(k, _)| matches!(k, Value::Str(s) if s == key))
        .map(|(_, v)| v)
}

fn scan_category(vault_root: &Path, category: Category, scan: &mut TemplateScan) {
    let dir = vault_root
        .join(".aisurada/templates")
        .join(category.as_str());
    let Ok(read_dir) = fs::read_dir(&dir) else {
        return;
    };
    let mut files: Vec<_> = read_dir
        .flatten()
        .filter(|e| fs::metadata(e.path()).map(|m| m.is_file()).unwrap_or(false))
        .filter_map(|e| e.file_name().into_string().ok().map(|n| (n, e.path())))
        .filter(|(name, _)| match category {
            Category::Doc => {
                name.ends_with(".md") || name.ends_with(".html") || name.ends_with(".json")
            }
            Category::Ritual => name.ends_with(".md"),
        })
        .collect();
    files.sort_by(|a, b| compare_utf16(&a.0, &b.0));

    for (file_name, path) in files {
        let rel_path = format!(".aisurada/templates/{}/{file_name}", category.as_str());
        let mut skip = |reason: &str| {
            scan.skipped.push(SkippedTemplate {
                category,
                rel_path: rel_path.clone(),
                reason: reason.to_string(),
            })
        };

        let content = match fs::metadata(&path) {
            Ok(m) if m.len() <= MAX_FILE_BYTES => {
                fs::read(&path).ok().and_then(|b| String::from_utf8(b).ok())
            }
            _ => None,
        };
        let Some(content) = content else {
            skip("파일을 못 읽었거나 크기 상한을 넘었다");
            continue;
        };
        let content = content.strip_prefix('\u{FEFF}').unwrap_or(&content);

        const NO_FRONT: &str = "프론트매터가 없거나 문법이 어긋난다";
        let Some((yaml, raw_body)) = split_fence(content) else {
            skip(NO_FRONT);
            continue;
        };
        if utf16_len(yaml) > MAX_FRONTMATTER_UNITS {
            skip(NO_FRONT);
            continue;
        }
        let Some(fm) = frontmatter::parse(yaml) else {
            skip(NO_FRONT);
            continue;
        };
        let body = raw_body.strip_prefix("\r\n").unwrap_or(raw_body);
        let body = body.strip_prefix('\n').unwrap_or(body).to_string();

        let stem = file_name
            .strip_suffix(".html")
            .or_else(|| file_name.strip_suffix(".json"))
            .or_else(|| file_name.strip_suffix(".md"))
            .unwrap_or(&file_name);
        let id = match get(&fm, "id") {
            Some(Value::Str(s)) if !s.is_empty() => s.clone(),
            _ => stem.to_string(),
        };
        let safe = !id.is_empty()
            && utf16_len(&id) <= 100
            && !id.contains('/')
            && !id.contains('\\')
            && !id.contains("..")
            && !id.starts_with('.')
            && id.chars().all(|c| c >= ' ');
        if !safe {
            skip(&format!("템플릿 id 가 안전한 경로 세그먼트가 아니다: {id}"));
            continue;
        }
        if is_blank(&id) || !ID_SHAPE.is_match(&id) {
            skip(&format!("템플릿 id 모양이 어긋난다: {id}"));
            continue;
        }

        let name = match get(&fm, "name") {
            Some(Value::Str(s)) if !trim(s).is_empty() => trim(s).to_string(),
            _ => id.clone(),
        };
        let desc = match get(&fm, "desc") {
            Some(Value::Str(s)) => s.clone(),
            _ => String::new(),
        };

        let record = match category {
            Category::Doc => {
                let format = if file_name.ends_with(".html") {
                    "html"
                } else if file_name.ends_with(".json") {
                    "json"
                } else {
                    "markdown"
                };
                let doc_type = match get(&fm, "type") {
                    Some(Value::Str(t)) if DOC_TYPES.contains(&t.as_str()) => t.clone(),
                    _ => "build".to_string(),
                };
                let builtin = match get(&fm, "builtin") {
                    Some(Value::Bool(b)) => *b,
                    _ => true,
                };
                TemplateRecord {
                    id,
                    category,
                    name,
                    doc_type: Some(doc_type),
                    rev: None,
                    icon: None,
                    format,
                    desc,
                    body,
                    builtin: Some(builtin),
                    rel_path: rel_path.clone(),
                }
            }
            Category::Ritual => {
                let format = if matches!(get(&fm, "format"), Some(Value::Str(f)) if f == "markdown")
                {
                    "markdown"
                } else {
                    "html"
                };
                let rev = match get(&fm, "rev") {
                    Some(Value::Str(r))
                        if ["morning", "evening", "weekly"].contains(&r.as_str()) =>
                    {
                        r.clone()
                    }
                    _ => "morning".to_string(),
                };
                let icon = match get(&fm, "icon") {
                    Some(Value::Str(i)) if ICON_SHAPE.is_match(i) => i.clone(),
                    _ => match rev.as_str() {
                        "evening" => "sunset",
                        "weekly" => "calendar",
                        _ => "sunrise",
                    }
                    .to_string(),
                };
                TemplateRecord {
                    id,
                    category,
                    name,
                    doc_type: None,
                    rev: Some(rev),
                    icon: Some(icon),
                    format,
                    desc,
                    body,
                    builtin: None,
                    rel_path: rel_path.clone(),
                }
            }
        };
        scan.templates.push(record);
    }
}
