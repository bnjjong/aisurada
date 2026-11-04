use serde::{Deserialize, Serialize};
use std::{
    env,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const CAUSE_CHARS: usize = 400;
const TERM_GRACE: Duration = Duration::from_secs(3);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeedMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskClaudeArgs {
    pub vault: String,
    pub session_id: String,
    pub model: String,
    pub effort: String,
    pub prompt: String,
    pub resume: bool,
    pub timeout_seconds: u64,
    pub keep_alive: Option<bool>,
    pub seed: Option<Vec<SeedMessage>>,
    pub system: Option<String>,
    pub route: Option<String>,
    pub cwd: Option<String>,
    pub resume_only: Option<bool>,
    pub persona: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AskClaudeResponse {
    pub ok: bool,
    pub answer: String,
    pub error: Option<String>,
    pub elapsed_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub missing_cwd: Option<String>,
}

pub fn is_regular_file(path: &Path) -> bool {
    match fs::metadata(path) {
        Ok(m) => m.is_file(),
        Err(_) => false,
    }
}

pub fn resolve_claude() -> Option<PathBuf> {
    // 1. WITHLOG_CLAUDE_BIN
    if let Ok(bin) = env::var("WITHLOG_CLAUDE_BIN") {
        if !bin.is_empty() {
            let p = PathBuf::from(&bin);
            if is_regular_file(&p) {
                return Some(p);
            }
        }
    }

    // 2. $HOME/.local/bin/claude
    if let Ok(home) = env::var("HOME") {
        if !home.is_empty() {
            let p = PathBuf::from(home).join(".local/bin/claude");
            if is_regular_file(&p) {
                return Some(p);
            }
        }
    }

    // 3. /opt/homebrew/bin/claude
    let brew_opt = PathBuf::from("/opt/homebrew/bin/claude");
    if is_regular_file(&brew_opt) {
        return Some(brew_opt);
    }

    // 4. /usr/local/bin/claude
    let usr_local = PathBuf::from("/usr/local/bin/claude");
    if is_regular_file(&usr_local) {
        return Some(usr_local);
    }

    // 5. PATH
    if let Ok(path_var) = env::var("PATH") {
        for dir in env::split_paths(&path_var) {
            let p = dir.join("claude");
            if is_regular_file(&p) {
                return Some(p);
            }
        }
    }

    None
}

pub fn augmented_path() -> String {
    let mut prefixes: Vec<String> = Vec::new();
    if let Ok(home) = env::var("HOME") {
        if !home.is_empty() {
            prefixes.push(format!("{}/.local/bin", home));
        }
    }
    prefixes.push("/opt/homebrew/bin".to_string());
    prefixes.push("/usr/local/bin".to_string());

    let prefix_str = prefixes.join(":");
    match env::var("PATH") {
        Ok(inherited) if !inherited.is_empty() => format!("{}:{}", prefix_str, inherited),
        _ => prefix_str,
    }
}

pub fn ask_route() -> &'static str {
    let http = env::var("WITHLOG_ASK_VIA_HTTP")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let sdk = env::var("WITHLOG_ASK_VIA_AGENT_SDK")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);

    if http {
        "http"
    } else if sdk {
        "agent-sdk"
    } else {
        "cli"
    }
}

#[tauri::command]
pub fn ask_runtime() -> &'static str {
    ask_route()
}

fn determine_work_dir(vault: &str, raw_cwd: Option<&str>) -> (PathBuf, Option<String>) {
    match raw_cwd {
        None => (PathBuf::from(vault), None),
        Some(s) if s.is_empty() => (PathBuf::from(vault), None),
        Some(s) => {
            let p = Path::new(s);
            if p.is_absolute() && p.is_dir() {
                (p.to_path_buf(), None)
            } else {
                (PathBuf::from(vault), Some(s.to_string()))
            }
        }
    }
}

fn format_cause(stderr: &str, stdout: &str) -> String {
    let raw = if !stderr.trim().is_empty() {
        stderr.trim()
    } else {
        stdout.trim()
    };
    if raw.is_empty() {
        return String::new();
    }

    let tail = if raw.chars().count() <= CAUSE_CHARS {
        raw.to_string()
    } else {
        let count = raw.chars().count();
        let tail_slice: String = raw.chars().skip(count - CAUSE_CHARS).collect();
        format!("…{}", tail_slice)
    };

    let first_line = raw
        .lines()
        .map(|l| l.trim())
        .find(|l| !l.is_empty())
        .unwrap_or("");

    let first_line_capped = if first_line.chars().count() <= CAUSE_CHARS {
        first_line.to_string()
    } else {
        let capped: String = first_line.chars().take(CAUSE_CHARS).collect();
        format!("{}…", capped)
    };

    if tail.contains(&first_line_capped) || first_line_capped.is_empty() {
        tail
    } else {
        format!("{}\n{}", first_line_capped, tail)
    }
}

struct ExecResult {
    exit_code: Option<i32>,
    timed_out: bool,
    stdout: String,
    stderr: String,
}

fn run_claude_process(
    claude_bin: &Path,
    work_dir: &Path,
    vault: &str,
    args: &[String],
    input_text: &str,
    timeout_secs: u64,
) -> Result<ExecResult, String> {
    let mut cmd = Command::new(claude_bin);
    cmd.current_dir(work_dir);
    cmd.args(args);

    // 환경 변수 설정
    cmd.env("PATH", augmented_path());
    cmd.env("WITHLOG_VAULT", vault);

    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("claude 를 못 띄웠다: {}", e))?;

    if let Some(mut stdin) = child.stdin.take() {
        let text_bytes = input_text.as_bytes().to_vec();
        thread::spawn(move || {
            let _ = stdin.write_all(&text_bytes);
            let _ = stdin.flush();
            drop(stdin);
        });
    }

    let (tx, rx) = mpsc::channel();
    let child_id = child.id();

    let timeout_duration = Duration::from_secs(timeout_secs);
    let wait_handle = thread::spawn(move || {
        let output = child.wait_with_output();
        let _ = tx.send(output);
    });

    match rx.recv_timeout(timeout_duration) {
        Ok(output_res) => {
            let output = output_res.map_err(|e| format!("claude 를 기다리던 일꾼이 끊겼다: {}", e))?;
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            Ok(ExecResult {
                exit_code: output.status.code(),
                timed_out: false,
                stdout,
                stderr,
            })
        }
        Err(_) => {
            // 타임아웃 종료: SIGTERM -> 3초 유예 -> SIGKILL
            unsafe {
                libc::kill(child_id as libc::pid_t, libc::SIGTERM);
            }
            if rx.recv_timeout(TERM_GRACE).is_err() {
                unsafe {
                    libc::kill(child_id as libc::pid_t, libc::SIGKILL);
                }
            }
            let _ = wait_handle.join();
            Ok(ExecResult {
                exit_code: None,
                timed_out: true,
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }
}

pub fn execute_ask(args: AskClaudeArgs) -> AskClaudeResponse {
    let start_time = Instant::now();

    if let Some(route) = &args.route {
        if route != "http" {
            return AskClaudeResponse {
                ok: false,
                answer: String::new(),
                error: Some(format!(
                    "모르는 묻기 길이다: {}. 받는 값은 http 하나다.",
                    route
                )),
                elapsed_ms: 0,
                missing_cwd: None,
            };
        }
    }

    let (work_dir, missing_cwd) = determine_work_dir(&args.vault, args.cwd.as_deref());

    let claude_bin = match resolve_claude() {
        Some(b) => b,
        None => {
            return AskClaudeResponse {
                ok: false,
                answer: String::new(),
                error: Some(
                    "claude 실행 파일을 못 찾았다. WITHLOG_CLAUDE_BIN 으로 자리를 알려 줄 수 있다."
                        .to_string(),
                ),
                elapsed_ms: 0,
                missing_cwd,
            }
        }
    };

    let build_args = |is_resume: bool| -> Vec<String> {
        let mut a = vec![
            "-p".to_string(),
            if is_resume {
                "--resume".to_string()
            } else {
                "--session-id".to_string()
            },
            args.session_id.clone(),
            "--model".to_string(),
            args.model.clone(),
            "--effort".to_string(),
            args.effort.clone(),
            "--permission-mode".to_string(),
            "auto".to_string(),
            "--permission-prompts".to_string(),
            "none".to_string(),
        ];
        if let Some(persona) = &args.persona {
            if !persona.trim().is_empty() {
                a.push("--append-system-prompt".to_string());
                a.push(persona.clone());
            }
        }
        a
    };

    let assemble_input = |is_resume: bool| -> String {
        if !is_resume {
            if let Some(seed) = &args.seed {
                if !seed.is_empty() {
                    let mut b = String::new();
                    b.push_str("## 앞 대화\n\n이 대화는 다른 길에서 시작해 여기로 옮겨 왔다. 아래가 앞서 오간 문답이다. 이 문답을 이어서 답하라.\n\n");
                    for msg in seed {
                        let role_head = if msg.role == "user" {
                            "### 사용자"
                        } else {
                            "### Claude"
                        };
                        b.push_str(role_head);
                        b.push('\n');
                        b.push_str(&msg.content);
                        b.push_str("\n\n");
                    }
                    b.push_str("## 이번 물음\n\n");
                    b.push_str(&args.prompt);
                    return b;
                }
            }
        }
        args.prompt.clone()
    };

    let mut current_resume = args.resume;
    let mut attempt = 0;
    let mut last_result: Option<ExecResult> = None;

    while attempt < 2 {
        attempt += 1;
        let cli_args = build_args(current_resume);
        let input_text = assemble_input(current_resume);

        match run_claude_process(
            &claude_bin,
            &work_dir,
            &args.vault,
            &cli_args,
            &input_text,
            args.timeout_seconds,
        ) {
            Ok(res) => {
                if res.timed_out {
                    last_result = Some(res);
                    break;
                }
                if res.exit_code == Some(0) {
                    last_result = Some(res);
                    break;
                }

                // 종료 코드 0이 아닐 때 뒤집기 판정 (5.4)
                let stderr = &res.stderr;
                let should_flip_to_resume = stderr.contains("is already in use");
                let should_flip_to_session_id = stderr.contains("No conversation found");

                last_result = Some(res);

                if attempt == 1 {
                    if should_flip_to_resume && !current_resume {
                        current_resume = true;
                        continue;
                    } else if should_flip_to_session_id
                        && current_resume
                        && !args.resume_only.unwrap_or(false)
                    {
                        current_resume = false;
                        continue;
                    }
                }
                break;
            }
            Err(e) => {
                let elapsed = start_time.elapsed().as_millis() as u64;
                return AskClaudeResponse {
                    ok: false,
                    answer: String::new(),
                    error: Some(e),
                    elapsed_ms: elapsed,
                    missing_cwd,
                };
            }
        }
    }

    let elapsed = start_time.elapsed().as_millis() as u64;
    let res = match last_result {
        Some(r) => r,
        None => {
            return AskClaudeResponse {
                ok: false,
                answer: String::new(),
                error: Some("실행 결과를 얻지 못했다.".to_string()),
                elapsed_ms: elapsed,
                missing_cwd,
            }
        }
    };

    if res.timed_out {
        let msg = format!(
            "{}초 안에 답이 안 왔다. 터미널에서 `claude --resume {}` 로 이어 볼 수 있다.",
            args.timeout_seconds, args.session_id
        );
        return AskClaudeResponse {
            ok: false,
            answer: String::new(),
            error: Some(msg),
            elapsed_ms: elapsed,
            missing_cwd,
        };
    }

    if res.exit_code == Some(0) {
        return AskClaudeResponse {
            ok: true,
            answer: res.stdout.trim().to_string(),
            error: None,
            elapsed_ms: elapsed,
            missing_cwd,
        };
    }

    let code_str = match res.exit_code {
        Some(c) => c.to_string(),
        None => "없음".to_string(),
    };

    let cause = format_cause(&res.stderr, &res.stdout);
    let mut err_msg = format!("claude 가 종료 코드 {} 로 끝났다: {}", code_str, cause);

    // 5.4: resumeOnly 인 물음이 실패했고 No conversation found 가 들면 앞붙임 글 추가
    if args.resume_only.unwrap_or(false) && (res.stderr.contains("No conversation found") || res.stdout.contains("No conversation found")) {
        let prefix = "이 기계에서 그 세션의 대화 기록을 못 찾아서 안 보냈다. 새 대화로 바꿔 보내면 앞 맥락 없는 대화가 같은 id 로 생긴다. ";
        err_msg = format!("{}{}", prefix, err_msg);
    }

    AskClaudeResponse {
        ok: false,
        answer: String::new(),
        error: Some(err_msg),
        elapsed_ms: elapsed,
        missing_cwd,
    }
}

#[tauri::command]
pub async fn ask_claude(
    vault: String,
    session_id: String,
    model: String,
    effort: String,
    prompt: String,
    resume: bool,
    timeout_seconds: u64,
    keep_alive: Option<bool>,
    seed: Option<Vec<SeedMessage>>,
    system: Option<String>,
    route: Option<String>,
    cwd: Option<String>,
    resume_only: Option<bool>,
    persona: Option<String>,
) -> Result<AskClaudeResponse, String> {
    let args = AskClaudeArgs {
        vault,
        session_id,
        model,
        effort,
        prompt,
        resume,
        timeout_seconds,
        keep_alive,
        seed,
        system,
        route,
        cwd,
        resume_only,
        persona,
    };

    // async 일꾼 밖의 blocking thread 에서 실행 (5.1 4번)
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let resp = execute_ask(args);
        let _ = tx.send(resp);
    });

    rx.recv()
        .map_err(|e| format!("claude 를 기다리던 일꾼이 끊겼다: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::Mutex;
    use tempfile::tempdir;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_augmented_path_structure() {
        let p = augmented_path();
        assert!(p.contains("/opt/homebrew/bin"));
        assert!(p.contains("/usr/local/bin"));
        if let Ok(home) = env::var("HOME") {
            if !home.is_empty() {
                assert!(p.starts_with(&format!("{}/.local/bin", home)));
            }
        }
    }

    #[test]
    fn test_ask_route_default_is_cli() {
        assert_eq!(ask_route(), "cli");
    }

    #[test]
    fn test_determine_work_dir_fallbacks() {
        let (dir, missing) = determine_work_dir("/tmp/vault", None);
        assert_eq!(dir, PathBuf::from("/tmp/vault"));
        assert!(missing.is_none());

        let (dir2, missing2) = determine_work_dir("/tmp/vault", Some(""));
        assert_eq!(dir2, PathBuf::from("/tmp/vault"));
        assert!(missing2.is_none());

        let (dir3, missing3) = determine_work_dir("/tmp/vault", Some("relative/dir"));
        assert_eq!(dir3, PathBuf::from("/tmp/vault"));
        assert_eq!(missing3, Some("relative/dir".to_string()));
    }

    #[test]
    fn test_format_cause_under_400_chars() {
        let err = "Simple error line\nAnother line";
        let cause = format_cause(err, "");
        assert_eq!(cause, "Simple error line\nAnother line");
    }

    #[test]
    fn test_format_cause_first_line_and_tail_400() {
        let long_line = "A".repeat(500);
        let cause = format_cause(&long_line, "");
        assert!(cause.contains("…"));
        assert!(cause.lines().count() >= 2);
    }

    #[test]
    fn test_resolve_claude_with_env_var() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = tempdir().unwrap();
        let fake_bin = dir.path().join("fake_claude");
        File::create(&fake_bin).unwrap();

        env::set_var("WITHLOG_CLAUDE_BIN", fake_bin.to_str().unwrap());
        let resolved = resolve_claude();
        assert_eq!(resolved, Some(fake_bin));
        env::remove_var("WITHLOG_CLAUDE_BIN");
    }

    fn create_executable_script(dir: &Path, name: &str, content: &str) -> PathBuf {
        let script_path = dir.join(name);
        fs::write(&script_path, content).unwrap();
        let mut perms = fs::metadata(&script_path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script_path, perms).unwrap();
        script_path
    }

    #[test]
    fn test_fake_claude_success_stdin_and_args() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = tempdir().unwrap();
        // 스크립트: 전달받은 인자들과 표준입력을 출력
        let script = r#"#!/bin/sh
echo "ARGS: $@"
echo "VAULT: $WITHLOG_VAULT"
echo "STDIN: $(cat)"
"#;
        let fake_bin = create_executable_script(dir.path(), "fake_claude_success.sh", script);
        env::set_var("WITHLOG_CLAUDE_BIN", fake_bin.to_str().unwrap());

        let vault_dir = dir.path().join("vault");
        fs::create_dir(&vault_dir).unwrap();
        let vault_str = vault_dir.to_str().unwrap().to_string();

        let args = AskClaudeArgs {
            vault: vault_str.clone(),
            session_id: "test-sess-uuid".to_string(),
            model: "claude-sonnet-5-5".to_string(),
            effort: "low".to_string(),
            prompt: "캐시는 언제 비우나?".to_string(),
            resume: false,
            timeout_seconds: 5,
            keep_alive: None,
            seed: None,
            system: None,
            route: None,
            cwd: None,
            resume_only: None,
            persona: Some("시스템 페르소나".to_string()),
        };

        let resp = execute_ask(args);
        env::remove_var("WITHLOG_CLAUDE_BIN");

        assert!(resp.ok, "should be ok: {:?}", resp.error);
        assert!(resp.error.is_none());
        assert!(resp.answer.contains("ARGS: -p --session-id test-sess-uuid --model claude-sonnet-5-5 --effort low --permission-mode auto --permission-prompts none --append-system-prompt 시스템 페르소나"));
        assert!(resp.answer.contains(&format!("VAULT: {}", vault_str)));
        assert!(resp.answer.contains("STDIN: 캐시는 언제 비우나?"));
        assert!(resp.elapsed_ms > 0);
    }

    #[test]
    fn test_fake_claude_flag_flip_on_already_in_use() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = tempdir().unwrap();
        let count_file = dir.path().join("attempt.txt");
        // 첫 번째 시도에는 'is already in use' 에러로 실패, 두 번째 시도(--resume)에는 성공
        let script = format!(
            r#"#!/bin/sh
CF="{}"
if [ ! -f "$CF" ]; then
    echo "1" > "$CF"
    echo "Error: Session is already in use" >&2
    exit 1
else
    echo "Flip success: $@"
    exit 0
fi
"#,
            count_file.to_str().unwrap()
        );

        let fake_bin = create_executable_script(dir.path(), "fake_claude_flip.sh", &script);
        env::set_var("WITHLOG_CLAUDE_BIN", fake_bin.to_str().unwrap());

        let args = AskClaudeArgs {
            vault: "/tmp".to_string(),
            session_id: "sess-already-in-use".to_string(),
            model: "claude-sonnet-5-5".to_string(),
            effort: "low".to_string(),
            prompt: "질문".to_string(),
            resume: false, // 첫 짐작은 session-id
            timeout_seconds: 5,
            keep_alive: None,
            seed: None,
            system: None,
            route: None,
            cwd: None,
            resume_only: None,
            persona: None,
        };

        let resp = execute_ask(args);
        env::remove_var("WITHLOG_CLAUDE_BIN");

        assert!(resp.ok, "flag flip should succeed on 2nd attempt: {:?}", resp.error);
        assert!(resp.answer.contains("--resume sess-already-in-use"));
    }

    #[test]
    fn test_fake_claude_no_record_prefix_on_resume_only() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = tempdir().unwrap();
        let script = r#"#!/bin/sh
echo "Error: No conversation found for session" >&2
exit 1
"#;
        let fake_bin = create_executable_script(dir.path(), "fake_claude_no_conv.sh", script);
        env::set_var("WITHLOG_CLAUDE_BIN", fake_bin.to_str().unwrap());

        let args = AskClaudeArgs {
            vault: "/tmp".to_string(),
            session_id: "sess-not-found".to_string(),
            model: "claude-sonnet-5-5".to_string(),
            effort: "low".to_string(),
            prompt: "질문".to_string(),
            resume: true,
            timeout_seconds: 5,
            keep_alive: None,
            seed: None,
            system: None,
            route: None,
            cwd: None,
            resume_only: Some(true),
            persona: None,
        };

        let resp = execute_ask(args);
        env::remove_var("WITHLOG_CLAUDE_BIN");

        assert!(!resp.ok, "should fail");
        let err = resp.error.unwrap();
        assert!(err.contains("이 기계에서 그 세션의 대화 기록을 못 찾아서 안 보냈다. 새 대화로 바꿔 보내면 앞 맥락 없는 대화가 같은 id 로 생긴다."));
        assert!(err.contains("No conversation found"));
    }

    #[test]
    fn test_fake_claude_timeout_reaps_child() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = tempdir().unwrap();
        let script = r#"#!/bin/sh
sleep 10
echo "Late answer"
"#;
        let fake_bin = create_executable_script(dir.path(), "fake_claude_sleep.sh", script);
        env::set_var("WITHLOG_CLAUDE_BIN", fake_bin.to_str().unwrap());

        let args = AskClaudeArgs {
            vault: "/tmp".to_string(),
            session_id: "sess-timeout".to_string(),
            model: "claude-sonnet-5-5".to_string(),
            effort: "low".to_string(),
            prompt: "질문".to_string(),
            resume: false,
            timeout_seconds: 1, // 1초 타임아웃
            keep_alive: None,
            seed: None,
            system: None,
            route: None,
            cwd: None,
            resume_only: None,
            persona: None,
        };

        let resp = execute_ask(args);
        env::remove_var("WITHLOG_CLAUDE_BIN");

        assert!(!resp.ok, "should time out");
        let err = resp.error.unwrap();
        assert!(err.contains("1초 안에 답이 안 왔다. 터미널에서 `claude --resume sess-timeout` 로 이어 볼 수 있다."));
    }

    #[test]
    fn test_fake_claude_d12_first_line_and_huge_policy_warning() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = tempdir().unwrap();
        // D12 고침: 실패 원인 첫 줄 + 450자 경고 줄
        let warning_text = "Policy warning: ".to_string() + &"W".repeat(450);
        let script = format!(
            r#"#!/bin/sh
echo "Fatal: Invalid configuration detected" >&2
echo "{}" >&2
exit 1
"#,
            warning_text
        );
        let fake_bin = create_executable_script(dir.path(), "fake_claude_d12.sh", &script);
        env::set_var("WITHLOG_CLAUDE_BIN", fake_bin.to_str().unwrap());

        let args = AskClaudeArgs {
            vault: "/tmp".to_string(),
            session_id: "sess-d12".to_string(),
            model: "claude-sonnet-5-5".to_string(),
            effort: "low".to_string(),
            prompt: "질문".to_string(),
            resume: false,
            timeout_seconds: 5,
            keep_alive: None,
            seed: None,
            system: None,
            route: None,
            cwd: None,
            resume_only: None,
            persona: None,
        };

        let resp = execute_ask(args);
        env::remove_var("WITHLOG_CLAUDE_BIN");

        assert!(!resp.ok);
        let err = resp.error.unwrap();
        assert!(err.contains("Fatal: Invalid configuration detected"), "First line cause must be preserved (D12)");
        assert!(err.contains("…"), "Tail must be truncated with ellipsis");
    }
}
