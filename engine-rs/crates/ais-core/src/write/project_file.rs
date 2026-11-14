//! 프로젝트 파일(`.aisurada/projects/<slug>.md`) 만들기와, 사용자가 준 저장소 목록(JSON)의 엄격한 검사.

use serde_json::Value as Json;

use crate::error::EngineError;
use crate::frontmatter::dump_string;
use crate::text::{trim, utf16_len};
use crate::vault::agenda::is_valid_slug_segment;
use crate::vault::project::{GITHUB, ProjectInfo, ProjectRepo};

const MAX_ENTRIES: usize = 20;
const MAX_PATH_UNITS: usize = 1024;
const KEYS: [&str; 4] = ["name", "path", "github", "githubId"];

pub fn rel_path(slug: &str) -> String {
    format!(".aisurada/projects/{slug}.md")
}

pub fn archived_rel_path(slug: &str, id: &str) -> String {
    format!(".aisurada/trash/projects/{slug}-{id}.md")
}

/// 프로젝트 파일 전체 내용: 머리(`id`, `slug`, `title`, `hue`와 저장소가 있으면 `repos: <한 줄 JSON>`)와 요약 본문
pub fn render(info: &ProjectInfo) -> Vec<u8> {
    let mut out = String::from("---\n");
    let mut line =
        |key: &str, value: &str| out.push_str(&format!("{key}: {}\n", dump_string(value, 1)));
    line("id", &info.id);
    line("slug", &info.slug);
    line("title", &info.title);
    line("hue", &info.hue);
    let repos = repos_json(&info.repos);
    if !repos.is_empty() {
        out.push_str(&format!("repos: {repos}\n"));
    }
    out.push_str("---\n");
    out.push_str(&info.summary);
    out.into_bytes()
}

/// 저장소 목록을 머리에 쓰는 한 줄 JSON으로 만든다. 없으면 빈 글이다.
pub fn repos_json(repos: &[ProjectRepo]) -> String {
    if repos.is_empty() {
        return String::new();
    }
    let items: Vec<String> = repos
        .iter()
        .map(|repo| {
            let mut item = format!("{{\"name\":{}", json_string(&repo.name));
            if let Some(path) = &repo.path {
                item.push_str(&format!(",\"path\":{}", json_string(path)));
            }
            if let Some(github) = &repo.github {
                item.push_str(&format!(",\"github\":{}", json_string(github)));
            }
            if let Some(id) = repo.github_id {
                item.push_str(&format!(",\"githubId\":{id}"));
            }
            item.push('}');
            item
        })
        .collect();
    format!("[{}]", items.join(","))
}

/// JSON 문자열 글: `"`, `\`, 제어 문자만 이스케이프하고 나머지는 그대로 쓴다(Java 엔진과 같다).
fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) <= 0x1f => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn bad(message: String) -> EngineError {
    EngineError::invalid_argument(message)
}

/// 사용자가 준 `repos` 글(JSON 배열)을 엄격하게 읽는다. 비었거나 공백이면 저장소 없음이다.
///
/// JSON 문법 오류의 문구는 Java 엔진의 JSON 읽개와 다르다. 모르는 키가 여러 개면 Java는 적은 차례의 첫 키를,
/// 여기서는 이름 차례의 첫 키를 알린다.
pub fn parse_strict(text: Option<&str>) -> Result<Vec<ProjectRepo>, EngineError> {
    let Some(text) = text.filter(|t| !crate::text::is_blank(t)) else {
        return Ok(Vec::new());
    };
    let parsed: Json = serde_json::from_str(text)
        .map_err(|e| bad(format!("repos 는 JSON 배열이어야 한다: {e}")))?;
    let Json::Array(items) = parsed else {
        return Err(bad("repos 는 JSON 배열이어야 한다".to_string()));
    };
    if items.len() > MAX_ENTRIES {
        return Err(bad(format!(
            "repos 는 {MAX_ENTRIES}개까지다: {}",
            items.len()
        )));
    }
    let mut repos: Vec<ProjectRepo> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let repo = entry(item, i + 1)?;
        if repos.iter().any(|r| r.name == repo.name) {
            return Err(bad(format!("repos 의 이름이 겹친다: {}", repo.name)));
        }
        repos.push(repo);
    }
    Ok(repos)
}

fn entry(item: &Json, index: usize) -> Result<ProjectRepo, EngineError> {
    let at = format!("repos[{index}]");
    let Json::Object(object) = item else {
        return Err(bad(format!("{at} 는 객체여야 한다")));
    };
    for key in object.keys() {
        if !KEYS.contains(&key.as_str()) {
            return Err(bad(format!(
                "{at} 의 모르는 키다: {key}. 쓸 수 있는 것은 [name, path, github, githubId] 뿐이다."
            )));
        }
    }
    let text = |key: &str| -> Result<Option<String>, EngineError> {
        match object.get(key) {
            None | Some(Json::Null) => Ok(None),
            Some(Json::String(s)) => Ok(Some(trim(s).to_string())),
            Some(_) => Err(bad(format!("{at} 의 {key} 는 글이어야 한다"))),
        }
    };
    let name = text("name")?
        .filter(|n| !n.is_empty())
        .ok_or_else(|| bad(format!("{at} 에 name 이 필요하다")))?;
    if utf16_len(&name) > 50 || !is_valid_slug_segment(&name) {
        return Err(bad(format!(
            "{at} 의 name 이 안전한 이름이 아니다(50자까지, 빗금 없음): {name}"
        )));
    }
    let path = text("path")?.filter(|p| !p.is_empty());
    if let Some(path) = &path
        && (!path.starts_with('/')
            || utf16_len(path) > MAX_PATH_UNITS
            || path.contains('\n')
            || path.contains('\r'))
    {
        return Err(bad(format!(
            "{at} 의 path 는 이 컴퓨터의 절대 경로여야 한다: {path}"
        )));
    }
    let github = text("github")?.filter(|g| !g.is_empty());
    if let Some(github) = &github
        && !GITHUB.is_match(github)
    {
        return Err(bad(format!(
            "{at} 의 github 는 owner/name 모양이어야 한다: {github}"
        )));
    }
    let github_id = match object.get("githubId") {
        None | Some(Json::Null) => None,
        Some(Json::Number(n)) => {
            let ok = n.as_u64().filter(|v| *v > 0 && v.to_string().len() <= 18);
            Some(ok.ok_or_else(|| bad(format!("{at} 의 githubId 는 양의 정수여야 한다")))?)
        }
        Some(_) => return Err(bad(format!("{at} 의 githubId 는 양의 정수여야 한다"))),
    };
    if github_id.is_some() && github.is_none() {
        return Err(bad(format!("{at} 의 githubId 는 github 와 함께 줘야 한다")));
    }
    if path.is_none() && github.is_none() {
        return Err(bad(format!(
            "{at} 에 path 나 github 가운데 하나는 있어야 한다"
        )));
    }
    Ok(ProjectRepo {
        name,
        path,
        github,
        github_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(name: &str, path: Option<&str>, github: Option<&str>, id: Option<u64>) -> ProjectRepo {
        ProjectRepo {
            name: name.into(),
            path: path.map(Into::into),
            github: github.map(Into::into),
            github_id: id,
        }
    }

    #[test]
    fn writes_compact_json_like_the_java_engine() {
        assert_eq!(repos_json(&[]), "");
        let repos = [
            repo("web", Some("/a \"b\"\\c\n"), Some("o/n"), Some(7)),
            repo("api", None, Some("x/y"), None),
        ];
        assert_eq!(
            repos_json(&repos),
            "[{\"name\":\"web\",\"path\":\"/a \\\"b\\\"\\\\c\\n\",\"github\":\"o/n\",\"githubId\":7},{\"name\":\"api\",\"github\":\"x/y\"}]"
        );
        assert_eq!(json_string("한\u{1}\u{7f}"), "\"한\\u0001\u{7f}\"");
    }

    #[test]
    fn parses_strictly_with_java_messages() {
        let ok = parse_strict(Some(r#"[{"name":" web ","path":"/tmp/x","github":"o/n","githubId":5},{"name":"api","github":"a/b"}]"#)).unwrap();
        assert_eq!(ok[0], repo("web", Some("/tmp/x"), Some("o/n"), Some(5)));
        assert!(
            parse_strict(None).unwrap().is_empty() && parse_strict(Some("  ")).unwrap().is_empty()
        );
        let msg = |text: &str| parse_strict(Some(text)).unwrap_err().message;
        assert_eq!(msg("{}"), "repos 는 JSON 배열이어야 한다");
        assert_eq!(msg("[1]"), "repos[1] 는 객체여야 한다");
        assert_eq!(
            msg(r#"[{"name":"a","path":"/x","bad":1}]"#),
            "repos[1] 의 모르는 키다: bad. 쓸 수 있는 것은 [name, path, github, githubId] 뿐이다."
        );
        assert_eq!(msg(r#"[{"path":"/x"}]"#), "repos[1] 에 name 이 필요하다");
        assert_eq!(
            msg(r#"[{"name":1,"path":"/x"}]"#),
            "repos[1] 의 name 는 글이어야 한다"
        );
        assert_eq!(
            msg(r#"[{"name":"a/b","path":"/x"}]"#),
            "repos[1] 의 name 이 안전한 이름이 아니다(50자까지, 빗금 없음): a/b"
        );
        assert_eq!(
            msg(r#"[{"name":"a","path":"rel"}]"#),
            "repos[1] 의 path 는 이 컴퓨터의 절대 경로여야 한다: rel"
        );
        assert_eq!(
            msg(r#"[{"name":"a","github":"nope"}]"#),
            "repos[1] 의 github 는 owner/name 모양이어야 한다: nope"
        );
        assert_eq!(
            msg(r#"[{"name":"a","github":"o/n","githubId":0}]"#),
            "repos[1] 의 githubId 는 양의 정수여야 한다"
        );
        assert_eq!(
            msg(r#"[{"name":"a","github":"o/n","githubId":"5"}]"#),
            "repos[1] 의 githubId 는 양의 정수여야 한다"
        );
        assert_eq!(
            msg(r#"[{"name":"a","path":"/x","githubId":5}]"#),
            "repos[1] 의 githubId 는 github 와 함께 줘야 한다"
        );
        assert_eq!(
            msg(r#"[{"name":"a"}]"#),
            "repos[1] 에 path 나 github 가운데 하나는 있어야 한다"
        );
        assert_eq!(
            msg(r#"[{"name":"a","path":"/x"},{"name":"a","path":"/y"}]"#),
            "repos 의 이름이 겹친다: a"
        );
        let many = format!("[{}]", vec![r#"{"name":"a","path":"/x"}"#; 21].join(","));
        assert_eq!(msg(&many), "repos 는 20개까지다: 21");
    }
}
