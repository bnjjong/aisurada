//! 프로젝트(아젠다를 묶는 단위) 읽기. `.aisurada/projects/*.md`의 머리를 아젠다와 같은 한 줄짜리 `키: 값` 읽개로 읽습니다.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value as Json;

use super::agenda::{HUES, is_valid_slug_segment, parse_line_reader, split_fence};
use crate::text::{compare_utf16, trim, utf16_len};

static PROJECT_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^pj[0-9a-z]+-[0-9a-f]{8}$").unwrap());
static GITHUB: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Za-z0-9](?:[A-Za-z0-9-]{0,38})/[A-Za-z0-9._-]{1,100}$").unwrap()
});

const MAX_BYTES: u64 = 100_000;
const MAX_REPOS: usize = 20;
const MAX_PATH_UNITS: usize = 1024;

/// 프로젝트가 가진 저장소 하나
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRepo {
    pub name: String,
    pub path: Option<String>,
    pub github: Option<String>,
    pub github_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectInfo {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub hue: String,
    pub summary: String,
    pub repos: Vec<ProjectRepo>,
}

/// `.aisurada/projects/`를 훑어 프로젝트를 id가 처음 나온 차례로 돌려줍니다. 읽지 못하는 파일은 조용히 뺍니다.
pub fn scan_projects(vault_root: &Path) -> Vec<ProjectInfo> {
    let dir = vault_root.join(".aisurada/projects");
    let Ok(read_dir) = fs::read_dir(&dir) else {
        return Vec::new();
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

    let mut order: Vec<String> = Vec::new();
    let mut by_id: HashMap<String, ProjectInfo> = HashMap::new();
    let mut slug_owner: HashMap<String, String> = HashMap::new();
    for (name, path) in files {
        let Some(info) = read_project(&name, &path) else {
            continue;
        };
        // 이미 다른 id가 쓰는 slug면 이 파일은 뺀다
        if slug_owner
            .get(&info.slug)
            .is_some_and(|owner| *owner != info.id)
        {
            continue;
        }
        if let Some(previous) = by_id.insert(info.id.clone(), info.clone()) {
            slug_owner.remove(&previous.slug);
        } else {
            order.push(info.id.clone());
        }
        slug_owner.insert(info.slug.clone(), info.id.clone());
    }
    order
        .into_iter()
        .filter_map(|id| by_id.remove(&id))
        .collect()
}

fn read_project(file_name: &str, path: &Path) -> Option<ProjectInfo> {
    if fs::metadata(path).ok()?.len() > MAX_BYTES {
        return None;
    }
    let content = String::from_utf8(fs::read(path).ok()?).ok()?;
    let content = content.strip_prefix('\u{FEFF}').unwrap_or(&content);
    let (front, body) = split_fence(content)?;
    let kv = parse_line_reader(front);

    let slug = match kv.get("slug").map(|s| trim(s)) {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => file_name[..file_name.len() - 3].to_string(),
    };
    if !is_valid_slug_segment(&slug) {
        return None;
    }
    let id = kv.get("id").map(|s| trim(s)).unwrap_or("");
    if !PROJECT_ID.is_match(id) {
        return None;
    }
    let title = match kv.get("title").map(|s| trim(s)) {
        Some(t) if !t.is_empty() => t.to_string(),
        _ => slug.clone(),
    };
    let hue = match kv.get("hue").map(|s| trim(s)) {
        Some(h) if HUES.contains(&h) => h.to_string(),
        _ => "blue".to_string(),
    };
    // 요약은 닫는 울타리 뒤 줄바꿈 하나만 떼고 그대로 둔다
    let summary = body
        .strip_prefix("\r\n")
        .or_else(|| body.strip_prefix('\n'))
        .unwrap_or(body)
        .to_string();
    Some(ProjectInfo {
        id: id.to_string(),
        slug,
        title,
        hue,
        summary,
        repos: parse_repos_lenient(kv.get("repos").map(String::as_str)),
    })
}

/// `repos` 값(JSON 배열 글)을 너그럽게 읽는다: 규칙에 안 맞는 항목은 빼고, 이름이 겹치면 앞의 것만 두고, 20개까지만 둔다.
fn parse_repos_lenient(value: Option<&str>) -> Vec<ProjectRepo> {
    let Some(text) = value.map(trim).filter(|t| !t.is_empty()) else {
        return Vec::new();
    };
    let Ok(Json::Array(items)) = serde_json::from_str::<Json>(text) else {
        return Vec::new();
    };
    let mut repos: Vec<ProjectRepo> = Vec::new();
    let mut names = HashSet::new();
    for item in &items {
        if repos.len() >= MAX_REPOS {
            break;
        }
        if let Some(repo) = parse_repo(item)
            && names.insert(repo.name.clone())
        {
            repos.push(repo);
        }
    }
    repos
}

fn json_text(object: &serde_json::Map<String, Json>, key: &str) -> Result<Option<String>, ()> {
    match object.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(Json::String(s)) => Ok(Some(trim(s).to_string())),
        Some(_) => Err(()),
    }
}

fn parse_repo(item: &Json) -> Option<ProjectRepo> {
    let object = item.as_object()?;
    if object
        .keys()
        .any(|k| !["name", "path", "github", "githubId"].contains(&k.as_str()))
    {
        return None;
    }
    let name = json_text(object, "name").ok()??;
    if name.is_empty() || utf16_len(&name) > 50 || !is_valid_slug_segment(&name) {
        return None;
    }
    let path = json_text(object, "path").ok()?.filter(|p| !p.is_empty());
    if let Some(p) = &path
        && (!p.starts_with('/')
            || utf16_len(p) > MAX_PATH_UNITS
            || p.contains('\n')
            || p.contains('\r'))
    {
        return None;
    }
    let github = json_text(object, "github").ok()?.filter(|g| !g.is_empty());
    if github.as_deref().is_some_and(|g| !GITHUB.is_match(g)) {
        return None;
    }
    let github_id = match object.get("githubId") {
        None | Some(Json::Null) => None,
        // 1~18자리 양의 정수만(소수나 지수 표기는 안 된다)
        Some(Json::Number(n)) => Some(n.as_u64().filter(|v| *v > 0 && v.to_string().len() <= 18)?),
        Some(_) => return None,
    };
    if github_id.is_some() && github.is_none() {
        return None;
    }
    if path.is_none() && github.is_none() {
        return None;
    }
    Some(ProjectRepo {
        name,
        path,
        github,
        github_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repos_keep_valid_entries_only() {
        let repos = parse_repos_lenient(Some(
            r#"[{"name":"a","path":"/x/y"},{"name":"a","github":"o/r"},{"name":"b","github":"o/r","githubId":7},{"name":"c"},{"name":"d","path":"rel"},{"name":"e","github":"o/r","extra":1},{"name":"f","githubId":3}]"#,
        ));
        let names: Vec<_> = repos.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["a", "b"]);
        assert_eq!(repos[1].github_id, Some(7));
    }

    #[test]
    fn unreadable_repos_become_empty() {
        assert!(parse_repos_lenient(Some("not json")).is_empty());
        assert!(parse_repos_lenient(Some("{}")).is_empty());
        assert!(parse_repos_lenient(None).is_empty());
    }
}
