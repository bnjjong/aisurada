//! 에이전트 CLI(claude, codex, agy, gemini)의 `mcp` 하위 명령을 돌리는 도구.
//!
//! 에이전트의 설정 파일을 직접 고치지 않고 각 CLI가 정한 등록 명령만 부르려는 것이라, 찾기와 실행을 한곳에 둔다.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::env::Env;

/// 등록 명령 하나가 이 시간 안에 안 끝나면 실패로 본다.
pub const TIMEOUT: Duration = Duration::from_secs(40);

/// CLI를 한 번 돌린 결과. `text`는 표준 출력과 표준 오류를 한데 모은 것이다.
#[derive(Debug, Clone)]
pub struct AgentResult {
    pub status: i32,
    pub text: String,
}

fn is_regular_file(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|meta| meta.is_file())
}

/// 실행 파일을 찾는다. 차례는 환경변수로 준 자리(`env_names` 차례), 홈의 `.local/bin`, Homebrew, `/usr/local/bin`, `PATH`이고
/// 각 자리에서 `bin_names` 차례로 본다. 없으면 `None`이다.
pub fn resolve(env: &Env, env_names: &[&str], bin_names: &[&str]) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    for name in env_names {
        if let Some(bin) = env.var(name).filter(|bin| !bin.is_empty()) {
            candidates.push(PathBuf::from(bin));
        }
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(home) = env.var("HOME").filter(|home| !home.is_empty()) {
        dirs.push(Path::new(home).join(".local/bin"));
    }
    dirs.push(PathBuf::from("/opt/homebrew/bin"));
    dirs.push(PathBuf::from("/usr/local/bin"));
    if let Some(path) = env.var("PATH") {
        dirs.extend(path.split(':').filter(|d| !d.is_empty()).map(PathBuf::from));
    }
    for dir in &dirs {
        for name in bin_names {
            candidates.push(dir.join(name));
        }
    }
    candidates
        .into_iter()
        .find(|candidate| is_regular_file(candidate))
}

/// CLI를 홈 폴더에서 돌려 종료 코드와 출력을 모은다. 프로젝트 설정(`.mcp.json` 따위)이 끼어들지 않게 하려는 것이다.
/// `label`은 시간이 넘었을 때 알림 글에 쓰는 이름이다. 못 돌렸거나 시간이 넘으면 오류 글을 돌려준다.
pub fn run(label: &str, bin: &Path, env: &Env, args: &[String]) -> Result<AgentResult, String> {
    let mut command = Command::new(bin);
    command.args(args).env_clear().envs(&env.vars);
    if let Some(home) = env.var("HOME").filter(|home| Path::new(home).is_dir()) {
        command.current_dir(home);
    }
    // 표준 출력과 표준 오류를 한 관에 모은다(Java `redirectErrorStream(true)`)
    let (mut reader, writer) = std::io::pipe().map_err(|e| e.to_string())?;
    let writer_for_stderr = writer.try_clone().map_err(|e| e.to_string())?;
    command
        .stdin(Stdio::null())
        .stdout(writer)
        .stderr(writer_for_stderr);
    let mut child = command.spawn().map_err(|e| e.to_string())?;
    // `command`가 쥔 쓰는 쪽을 놓아야 자식이 끝났을 때 읽기가 끝난다
    drop(command);

    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = reader.read_to_end(&mut bytes);
        let _ = sender.send(String::from_utf8_lossy(&bytes).into_owned());
    });
    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => break status,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                let shown: Vec<&str> = args.iter().take(2).map(String::as_str).collect();
                return Err(format!(
                    "{label} {} 가 {}초 안에 안 끝났다",
                    shown.join(" "),
                    TIMEOUT.as_secs()
                ));
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    // 자식이 끝난 뒤에도 손주가 관을 쥐고 있으면 읽기가 안 끝난다. 5초만 기다리고 빈 글로 본다.
    let text = receiver
        .recv_timeout(Duration::from_secs(5))
        .unwrap_or_default();
    Ok(AgentResult {
        status: status.code().unwrap_or(-1),
        text,
    })
}
