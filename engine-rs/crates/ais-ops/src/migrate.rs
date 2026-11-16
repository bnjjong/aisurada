//! Java 시절에서 넘어올 때 한 번 하는 정리: 옛 사이드카를 걷어 내고, 낡은 MCP 등록을 새 실행 파일로 다시 등록한다.
//!
//! 앱의 첫 실행(호스트)과 `ais-cli doctor --fix`가 이것을 쓴다. 우리 레이블의 사이드카만 내리고, 등록은 이미 `aisurada`라는 이름으로
//! 등록돼 있고 Java 시절의 jar를 가리키는 것(`stale`)만 같은 모드(`--write` 여부)로 다시 넣는다. 등록돼 있지 않은 에이전트에는 아무것도 하지 않는다.

use std::path::Path;

use ais_core::json::{self, JsonValue};

use crate::Outcome;
use crate::env::Env;
use crate::old_sidecar;
use crate::register::{self, Kind};

/// 정리 결과. `left`가 비어 있어야 다 한 것이다.
#[derive(Debug, Default)]
pub struct Migration {
    pub lines: Vec<String>,
    /// 못 한 것. 사람이 손으로 하거나 다음 실행에서 다시 시도한다.
    pub left: Vec<String>,
}

fn flag(status: &JsonValue, key: &str) -> bool {
    matches!(status.get(key), Some(JsonValue::Literal(raw)) if raw == "true")
}

/// 에이전트 하나의 낡은 등록을 다시 등록한다. `status`를 읽고, 낡은 등록일 때만 `add`한다.
fn reregister(
    label: &str,
    status: Outcome,
    add: impl Fn(&[String]) -> Outcome,
    exe_known: bool,
    result: &mut Migration,
) {
    let Ok(parsed) = json::parse(&status.stdout) else {
        return;
    };
    if !(flag(&parsed, "registered") && flag(&parsed, "stale")) {
        return;
    }
    if !exe_known {
        result.left.push(format!(
            "{label}의 낡은 MCP 등록을 못 고쳤다: ais-cli 실행 파일의 자리를 모른다"
        ));
        return;
    }
    let mut args = vec!["add".to_string()];
    if flag(&parsed, "write") {
        args.push("--write".to_string());
    }
    let added = add(&args);
    if added.code == 0 {
        result.lines.push(format!(
            "{label}의 낡은 MCP 등록을 새 실행 파일로 다시 등록했다({})",
            if flag(&parsed, "write") {
                "읽기+쓰기"
            } else {
                "읽기 전용"
            }
        ));
    } else {
        result.left.push(format!(
            "{label}의 낡은 MCP 등록을 못 고쳤다: {}",
            added.stderr.trim()
        ));
    }
}

/// 옛 사이드카를 걷어 내고 낡은 MCP 등록을 다시 등록한다.
pub fn run(env: &Env) -> Migration {
    let mut result = Migration::default();
    let swept = old_sidecar::sweep(env, Path::new(&env.home), false);
    result.lines.extend(swept.lines);
    result.left.extend(swept.left);

    let exe_known = !env.exe.is_empty();
    let status = vec!["status".to_string()];
    reregister(
        "Claude Code",
        register::claude_mcp(&status, env),
        |args| register::claude_mcp(args, env),
        exe_known,
        &mut result,
    );
    reregister(
        "Codex",
        register::agent_mcp(Kind::Codex, &status, env),
        |args| register::agent_mcp(Kind::Codex, args, env),
        exe_known,
        &mut result,
    );
    reregister(
        "Gemini",
        register::agent_mcp(Kind::Gemini, &status, env),
        |args| register::agent_mcp(Kind::Gemini, args, env),
        exe_known,
        &mut result,
    );
    result
}

/// 이주를 이미 마쳤다는 표시 파일
pub fn marker(home: &str) -> std::path::PathBuf {
    Path::new(home).join("Library/Application Support/aisurada/rust-engine-migrated")
}

/// 이 기기에서 이주를 한 번만 한다. 표시 파일이 있으면 아무것도 안 하고 `None`이다. 다 마쳤으면(`left`가 비면) 표시 파일을 남겨 다시는
/// 하지 않는다. 못 한 것이 있으면 표시하지 않아서 다음 실행에서 다시 시도한다(에이전트 CLI를 안 찾는 것은 못 한 일이 아니다).
pub fn run_once(env: &Env) -> Option<Migration> {
    let marker = marker(&env.home);
    if marker.exists() {
        return None;
    }
    let result = run(env);
    if result.left.is_empty() {
        if let Some(dir) = marker.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&marker, "1\n");
    }
    Some(result)
}
