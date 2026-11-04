//! 4부 세션 목록(PROJECTS 레일) 지원을 위한 호스트 커맨드 모듈.
//! list_claude_agents, alive_pids, held_agent_sessions, release_agent_session을 구현합니다.
//! 설명서 6장 및 부록 C(D14 고침)의 계약을 준수합니다.

use serde::{Deserialize, Serialize};
use std::process::Command;
use std::time::{Duration, Instant};

use crate::ask::{resolve_claude, tail_chars};
use crate::process::{self, Outcome, RunError};

const LIST_AGENTS_TIMEOUT_SECS: u64 = 10;
const ALIVE_PIDS_TIMEOUT_SECS: u64 = 5;

#[derive(Debug, Serialize, Deserialize)]
pub struct AgentsResponse {
    pub ok: bool,
    pub stdout: String,
    pub error: Option<String>,
    #[serde(rename = "elapsedMs")]
    pub elapsed_ms: u64,
}

#[derive(Debug, Deserialize)]
pub struct AlivePidsArgs {
    pub pids: Vec<u32>,
}

/// claude agents --json을 실행하여 기기 내 Claude 세션 목록 원문을 조회합니다 (6.2).
/// 기다리는 자리는 async 일꾼 밖이고, 일꾼이 끊기면 커맨드째 거절합니다.
#[tauri::command]
pub async fn list_claude_agents() -> Result<AgentsResponse, String> {
    tauri::async_runtime::spawn_blocking(run_list_agents)
        .await
        .map_err(|e| format!("세션 목록을 받다 멈췄다: {}", e))
}

fn run_list_agents() -> AgentsResponse {
    let start = Instant::now();
    let failure = |error: String| AgentsResponse {
        ok: false,
        stdout: String::new(),
        error: Some(error),
        elapsed_ms: start.elapsed().as_millis() as u64,
    };

    let claude_bin = match resolve_claude() {
        Some(bin) => bin,
        None => {
            return failure(
                "claude 실행 파일을 못 찾았다. AISURADA_CLAUDE_BIN 으로 자리를 알려 줄 수 있다.".into(),
            );
        }
    };

    // 작업 폴더도 환경도 안 건드린다. 호스트의 것을 물려받는다(1장 1.9의 7번)
    let mut cmd = Command::new(claude_bin);
    cmd.arg("agents").arg("--json");

    // 상한은 2부 6.3의 차례(SIGTERM, 3초, SIGKILL)로 끝내고, 파이프는 둘을 함께 비운다
    match process::run_typed(&mut cmd, None, Duration::from_secs(LIST_AGENTS_TIMEOUT_SECS)) {
        Ok(Outcome::Finished(output)) => {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            if output.status.success() {
                AgentsResponse {
                    ok: true,
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    error: None,
                    elapsed_ms,
                }
            } else {
                let code_str = match output.status.code() {
                    Some(c) => c.to_string(),
                    None => "없음".to_string(),
                };
                // 꼬리는 표준오류만 보고 앞뒤 공백을 걷어 마지막 400자를 남긴다(6.2)
                let stderr = String::from_utf8_lossy(&output.stderr);
                AgentsResponse {
                    ok: false,
                    stdout: String::new(),
                    error: Some(format!(
                        "claude 가 종료 코드 {} 로 끝났다: {}",
                        code_str,
                        tail_chars(stderr.trim())
                    )),
                    elapsed_ms,
                }
            }
        }
        Ok(Outcome::TimedOut) => failure("10초 안에 세션 목록이 안 왔다.".into()),
        Err(RunError::Spawn(e)) => failure(format!("claude 를 못 띄웠다: {}", e)),
        Err(RunError::Io(e)) => failure(format!("claude 를 기다리던 일꾼이 끊겼다: {}", e)),
    }
}

/// ps -p <pids> -o pid=를 실행하여 프로세스가 살아있는 pid들을 확인합니다 (6.4).
/// D14 결함 고침: 5초 상한 초과 시 SIGTERM/SIGKILL 정리 및 에러 반환.
#[tauri::command]
pub async fn alive_pids(args: Option<AlivePidsArgs>, pids: Option<Vec<u32>>) -> Result<Vec<u32>, String> {
    let pid_list = args.map(|a| a.pids).or(pids).unwrap_or_default();
    if pid_list.is_empty() {
        return Ok(Vec::new());
    }

    tauri::async_runtime::spawn_blocking(move || run_alive_pids(pid_list))
        .await
        .map_err(|e| format!("살았는지 묻다 멈췄다: {}", e))?
}

fn run_alive_pids(pids: Vec<u32>) -> Result<Vec<u32>, String> {
    let pid_args = pids
        .iter()
        .map(|p| p.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let mut cmd = Command::new("ps");
    cmd.arg("-p").arg(&pid_args).arg("-o").arg("pid=");

    match process::run_typed(&mut cmd, None, Duration::from_secs(ALIVE_PIDS_TIMEOUT_SECS)) {
        // 종료 코드를 안 본다. 물어야 할 것은 표준출력에 찍힌 목록이다
        Ok(Outcome::Finished(output)) => {
            let text = String::from_utf8_lossy(&output.stdout);
            let mut alive = Vec::new();
            for line in text.lines() {
                if let Ok(pid) = line.trim().parse::<u32>() {
                    alive.push(pid);
                }
            }
            Ok(alive)
        }
        Ok(Outcome::TimedOut) => Err("ps 가 5초 안에 안 끝났다.".into()),
        Err(error) => Err(format!("ps 를 못 띄웠다: {}", error.into_io())),
    }
}

/// 기본 길에서 데몬 세션은 없으므로 빈 배열을 반환합니다 (6.6, 부록 E). 인자는 `{vault}` 다.
#[tauri::command]
pub async fn held_agent_sessions(vault: Option<String>) -> Result<Vec<String>, String> {
    let _ = vault;
    Ok(Vec::new())
}

/// 기본 길에서 데몬 세션 해제는 "not_live"를 반환합니다 (부록 E). 인자는 `{vault, sessionId}` 다.
#[tauri::command]
pub async fn release_agent_session(vault: Option<String>, session_id: Option<String>) -> Result<String, String> {
    let _ = (vault, session_id);
    Ok("not_live".into())
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn test_alive_pids_empty() {
        let res = run_alive_pids(vec![]).unwrap();
        assert!(res.is_empty());
    }

    #[test]
    fn test_alive_pids_self_and_invalid() {
        let current_pid = std::process::id();
        let invalid_pid = 99998;
        let res = run_alive_pids(vec![current_pid, invalid_pid]).unwrap();
        assert!(res.contains(&current_pid));
        assert!(!res.contains(&invalid_pid));
    }
}
