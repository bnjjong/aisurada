//! 4부 세션 목록(PROJECTS 레일) 지원을 위한 호스트 커맨드 모듈.
//! list_claude_agents, alive_pids, held_agent_sessions, release_agent_session을 구현합니다.
//! 설명서 6장 및 부록 C(D14 고침)의 계약을 준수합니다.

use serde::{Deserialize, Serialize};
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::ask::resolve_claude;

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

#[derive(Debug, Deserialize)]
pub struct HeldSessionsArgs {
    pub vault: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReleaseSessionArgs {
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
}

/// claude agents --json을 실행하여 기기 내 Claude 세션 목록 원문을 조회합니다 (6.2).
#[tauri::command]
pub async fn list_claude_agents() -> Result<AgentsResponse, String> {
    let (tx, rx) = mpsc::channel();

    thread::spawn(move || {
        let res = run_list_agents();
        let _ = tx.send(res);
    });

    rx.recv()
        .map_err(|e| format!("세션 목록을 받다 멈췄다: {}", e))
}

fn run_list_agents() -> AgentsResponse {
    let start = Instant::now();

    let claude_bin = match resolve_claude() {
        Some(bin) => bin,
        None => {
            return AgentsResponse {
                ok: false,
                stdout: String::new(),
                error: Some("claude 실행 파일을 못 찾았다. WITHLOG_CLAUDE_BIN 으로 자리를 알려 줄 수 있다.".into()),
                elapsed_ms: start.elapsed().as_millis() as u64,
            };
        }
    };

    let mut cmd = Command::new(claude_bin);
    cmd.arg("agents").arg("--json");
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return AgentsResponse {
                ok: false,
                stdout: String::new(),
                error: Some(format!("claude 를 못 띄웠다: {}", e)),
                elapsed_ms: start.elapsed().as_millis() as u64,
            };
        }
    };

    let pid = child.id();
    let timeout = Duration::from_secs(LIST_AGENTS_TIMEOUT_SECS);

    let (exit_tx, exit_rx) = mpsc::channel();
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    let io_handle = thread::spawn(move || {
        let mut out = Vec::new();
        let mut err = Vec::new();
        if let Some(mut r) = stdout_pipe {
            let _ = r.read_to_end(&mut out);
        }
        if let Some(mut r) = stderr_pipe {
            let _ = r.read_to_end(&mut err);
        }
        (out, err)
    });

    thread::spawn(move || {
        let status = child.wait();
        let _ = exit_tx.send(status);
    });

    match exit_rx.recv_timeout(timeout) {
        Ok(Ok(exit_status)) => {
            let (stdout_bytes, stderr_bytes) = io_handle.join().unwrap_or_default();
            let elapsed_ms = start.elapsed().as_millis() as u64;

            if exit_status.success() {
                AgentsResponse {
                    ok: true,
                    stdout: String::from_utf8_lossy(&stdout_bytes).to_string(),
                    error: None,
                    elapsed_ms,
                }
            } else {
                let code_str = match exit_status.code() {
                    Some(c) => c.to_string(),
                    None => "없음".to_string(),
                };
                let err_str = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
                let tail = if err_str.chars().count() > 400 {
                    let chars: Vec<char> = err_str.chars().collect();
                    chars[chars.len() - 400..].iter().collect()
                } else {
                    err_str
                };

                AgentsResponse {
                    ok: false,
                    stdout: String::new(),
                    error: Some(format!("claude 가 종료 코드 {} 로 끝났다: {}", code_str, tail)),
                    elapsed_ms,
                }
            }
        }
        _ => {
            // 상한 10초 초과: SIGTERM -> 3초 후 SIGKILL (6.2, 5.1)
            terminate_pid(pid);
            AgentsResponse {
                ok: false,
                stdout: String::new(),
                error: Some("10초 안에 세션 목록이 안 왔다.".into()),
                elapsed_ms: start.elapsed().as_millis() as u64,
            }
        }
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

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let res = run_alive_pids(pid_list);
        let _ = tx.send(res);
    });

    rx.recv()
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
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| format!("ps를 못 띄웠다: {}", e))?;
    let child_pid = child.id();
    let timeout = Duration::from_secs(ALIVE_PIDS_TIMEOUT_SECS);

    let (exit_tx, exit_rx) = mpsc::channel();
    let stdout_pipe = child.stdout.take();

    let io_handle = thread::spawn(move || {
        let mut out = Vec::new();
        if let Some(mut r) = stdout_pipe {
            let _ = r.read_to_end(&mut out);
        }
        out
    });

    thread::spawn(move || {
        let st = child.wait();
        let _ = exit_tx.send(st);
    });

    match exit_rx.recv_timeout(timeout) {
        Ok(Ok(_status)) => {
            let stdout_bytes = io_handle.join().unwrap_or_default();
            let text = String::from_utf8_lossy(&stdout_bytes);
            let mut alive = Vec::new();
            for line in text.lines() {
                let trimmed = line.trim();
                if let Ok(pid) = trimmed.parse::<u32>() {
                    alive.push(pid);
                }
            }
            Ok(alive)
        }
        _ => {
            // D14: 5초 초과 시 정리 및 Err
            terminate_pid(child_pid);
            Err("ps가 5초 안에 안 끝났다.".into())
        }
    }
}

/// 기본 길에서 데몬 세션은 없으므로 빈 배열을 반환합니다 (6.6, 부록 E).
#[tauri::command]
pub async fn held_agent_sessions(_args: Option<HeldSessionsArgs>) -> Result<Vec<String>, String> {
    Ok(Vec::new())
}

/// 기본 길에서 데몬 세션 해제는 "not_live"를 반환합니다 (부록 E).
#[tauri::command]
pub async fn release_agent_session(_args: Option<ReleaseSessionArgs>) -> Result<String, String> {
    Ok("not_live".into())
}

fn terminate_pid(pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
        thread::sleep(Duration::from_millis(50));
        let mut status = 0;
        let res = libc::waitpid(pid as i32, &mut status, libc::WNOHANG);
        if res == 0 {
            // 아직 살아있으면 1초 대기 후 SIGKILL
            thread::sleep(Duration::from_millis(1000));
            let res2 = libc::waitpid(pid as i32, &mut status, libc::WNOHANG);
            if res2 == 0 {
                libc::kill(pid as i32, libc::SIGKILL);
                libc::waitpid(pid as i32, &mut status, 0);
            }
        }
    }
    #[cfg(not(unix))]
    {
        // Windows 등
    }
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
