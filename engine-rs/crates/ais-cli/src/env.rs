//! 호출 환경: 환경변수, 홈, 작업 폴더, 어느 vault를 쓸지, 어느 에이전트 세션에서 불렀는지.
//!
//! 시험이 진짜 환경을 건드리지 않도록 프로세스 환경을 직접 읽지 않고 [`Env`]를 통해서만 읽는다.

use std::collections::BTreeMap;
use std::path::Path;

use ais_core::json::{self, JsonValue};
use ais_core::text::{is_blank, trim};

/// vault를 못 정했을 때 오류 줄에 붙이는 안내(앞에 `AIS-400 `을 붙여 낸다)
pub const MISSING_VAULT_MESSAGE: &str = "vault를 정하지 못했다. AISURADA_VAULT 환경변수에 vault 폴더의 절대 경로를 넣거나, 앱을 열어 vault를 연다(앱이 연 vault를 ~/Library/Application Support/aisurada/current-vault.json 에 적어 둔다).";

/// 앱이 지금 연 vault를 적어 두는 파일 이름
const POINTER_FILE: &str = "current-vault.json";

/// 한 번의 호출이 보는 환경
#[derive(Debug, Clone, Default)]
pub struct Env {
    pub vars: BTreeMap<String, String>,
    /// 사용자 홈 폴더. 앱이 연 vault 쪽지(`current-vault.json`)가 이 아래에 있다.
    pub home: String,
    /// 상대 경로를 풀 기준 폴더
    pub cwd: String,
    /// 지금 도는 `ais-cli` 실행 파일의 절대 경로. 에이전트에 MCP 서버로 등록할 명령이 이것이다.
    pub exe: String,
    /// 이 호출이 직접 정한 세션 출처. 환경변수보다 먼저 쓴다(MCP 요청에 실려 온 세션 id가 이것이다).
    pub session: Option<SessionSource>,
}

impl Env {
    /// 지금 프로세스의 환경
    pub fn from_process() -> Env {
        Env {
            vars: std::env::vars().collect(),
            home: std::env::var("HOME").unwrap_or_default(),
            cwd: std::env::current_dir()
                .map(|dir| dir.to_string_lossy().into_owned())
                .unwrap_or_else(|_| ".".to_string()),
            exe: std::env::current_exe()
                .ok()
                .and_then(|path| path.canonicalize().ok().or(Some(path)))
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default(),
            session: None,
        }
    }

    pub fn var(&self, name: &str) -> Option<&str> {
        self.vars.get(name).map(String::as_str)
    }
}

/// 이 호출이 쓸 vault. `AISURADA_VAULT`가 비지 않았으면 그것이, 아니면 앱이 적어 둔 쪽지가 이긴다. 둘 다 없으면 `None`.
pub fn resolve_vault(env: &Env) -> Option<String> {
    match env.var("AISURADA_VAULT") {
        Some(value) if !is_blank(value) => Some(value.to_string()),
        _ => read_pointer(&env.home),
    }
}

/// 앱이 적어 둔 쪽지의 vault 경로. 파일이 없거나 JSON이 아니거나 절대 경로가 아니면 `None`.
fn read_pointer(home: &str) -> Option<String> {
    if is_blank(home) {
        return None;
    }
    let file = Path::new(home)
        .join("Library")
        .join("Application Support")
        .join("aisurada")
        .join(POINTER_FILE);
    if !file.is_file() {
        return None;
    }
    let text = std::fs::read_to_string(&file).ok()?;
    match json::parse(&text).ok()?.get("vault")? {
        JsonValue::Str(path) => {
            let path = trim(path);
            path.starts_with('/').then(|| path.to_string())
        }
        _ => None,
    }
}

/// 경로를 절대 경로로 바로잡는다: 앞뒤 공백을 떼고, 상대 경로면 작업 폴더를 앞에 붙이고, `.`·`<이름>/..`을 지운다.
/// 심볼릭 링크는 풀지 않고 유니코드 정규화도 하지 않는다(Java `VaultPath.normalize`).
pub fn normalize_path(raw: &str, cwd: &str) -> Result<String, String> {
    let trimmed = trim(raw);
    if trimmed.is_empty() {
        return Err("경로가 비어 있습니다.".to_string());
    }
    if trimmed.contains('\0') {
        return Err("경로에 NUL 문자가 포함되어 있다.".to_string());
    }
    let joined = if trimmed.starts_with('/') {
        trimmed.to_string()
    } else {
        format!("{cwd}/{trimmed}")
    };
    let mut parts: Vec<&str> = Vec::new();
    for part in joined.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            name => parts.push(name),
        }
    }
    Ok(format!("/{}", parts.join("/")))
}

/// 요청을 보낸 에이전트 세션. 노트에 누가 고쳤는지 적을 때 쓴다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSource {
    pub agent: String,
    pub id: String,
}

/// 에이전트 이름으로 쓸 수 있는 모양인가: 소문자로 시작하고 소문자·숫자·하이픈만, 32자까지
fn is_agent_name(value: &str) -> bool {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        _ => return false,
    }
    value.len() <= 32 && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// 파일에 적을 에이전트 이름. claude와 모양이 틀린 값은 `None`이다(줄이 없으면 읽는 쪽이 claude로 본다).
pub fn agent_for_file(agent: &str) -> Option<String> {
    let lower = trim(agent).to_lowercase();
    (lower != "claude" && is_agent_name(&lower)).then_some(lower)
}

/// 환경변수로 세션 출처를 정한다.
///
/// 래퍼가 넣은 `AISURADA_SESSION_ID`와 `AISURADA_SESSION_AGENT`가 둘 다 쓸 수 있으면 그 쌍이 이긴다. 아니면 에이전트 고유 변수를 보고,
/// 둘 이상의 에이전트가 동시에 보이면 어느 쪽이 안쪽인지 모르므로 정하지 않는다(엉뚱한 세션보다 빈 칸이 낫다).
pub fn session_from_env(env: &Env) -> Option<SessionSource> {
    if env.session.is_some() {
        return env.session.clone();
    }
    let get = |name: &str| trim(env.var(name).unwrap_or("")).to_string();
    let pinned_id = get("AISURADA_SESSION_ID");
    let pinned_agent = get("AISURADA_SESSION_AGENT").to_lowercase();
    if !pinned_id.is_empty() && is_agent_name(&pinned_agent) {
        return Some(SessionSource {
            agent: pinned_agent,
            id: pinned_id,
        });
    }

    let mut found = Vec::new();
    let claude = get("CLAUDE_CODE_SESSION_ID");
    if !claude.is_empty() {
        found.push(SessionSource {
            agent: "claude".to_string(),
            id: claude,
        });
    }
    let mut codex = get("CODEX_SESSION_ID");
    if codex.is_empty() {
        codex = get("CODEX_THREAD_ID");
    }
    if !codex.is_empty() {
        found.push(SessionSource {
            agent: "codex".to_string(),
            id: codex,
        });
    }
    if found.len() == 1 { found.pop() } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> Env {
        Env {
            vars: pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            home: "/nonexistent-home".to_string(),
            cwd: "/work".to_string(),
            exe: String::new(),
            session: None,
        }
    }

    #[test]
    fn normalizes_like_java() {
        assert_eq!(normalize_path(" /a//b/./c/../d/ ", "/w").unwrap(), "/a/b/d");
        assert_eq!(normalize_path("x/y", "/w").unwrap(), "/w/x/y");
        assert_eq!(normalize_path("/../..", "/w").unwrap(), "/");
        assert!(normalize_path("  ", "/w").is_err());
        assert!(normalize_path("a\0b", "/w").is_err());
    }

    #[test]
    fn pinned_pair_wins_over_agent_variables() {
        let source = session_from_env(&env(&[
            ("AISURADA_SESSION_ID", "s-1"),
            ("AISURADA_SESSION_AGENT", "Gemini"),
            ("CLAUDE_CODE_SESSION_ID", "c-1"),
        ]))
        .unwrap();
        assert_eq!(source.agent, "gemini");
        assert_eq!(source.id, "s-1");
    }

    #[test]
    fn two_agents_at_once_are_ambiguous() {
        assert_eq!(
            session_from_env(&env(&[
                ("CLAUDE_CODE_SESSION_ID", "c"),
                ("CODEX_THREAD_ID", "x")
            ])),
            None
        );
        assert_eq!(
            session_from_env(&env(&[("CODEX_THREAD_ID", "x")])),
            Some(SessionSource {
                agent: "codex".into(),
                id: "x".into()
            })
        );
    }

    #[test]
    fn only_non_claude_agents_are_written_to_files() {
        assert_eq!(agent_for_file("claude"), None);
        assert_eq!(agent_for_file(" Codex "), Some("codex".to_string()));
        assert_eq!(agent_for_file("9bad"), None);
    }
}
