use crate::process::{self, Outcome, RunError};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
    time::{Duration, Instant},
};

const CAUSE_CHARS: usize = 400;

/// 앱에 든 도구 폴더(`Contents/Resources/bin`). 앱이 뜰 때 한 번 정한다. 개발 실행에서는 정해지지 않는다.
static TOOLS_DIR: OnceLock<PathBuf> = OnceLock::new();

/// 앱에 든 `ais-cli` 스크립트가 있는 폴더를 알려 준다. 이 뒤로 claude를 띄울 때 PATH 맨 앞에 얹혀서,
/// 사용자가 손으로 래퍼를 만들거나 Java를 깔지 않아도 claude가 `ais-cli`를 부를 수 있다.
pub fn set_tools_dir(dir: PathBuf) {
    let _ = TOOLS_DIR.set(dir);
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
    pub seed: Option<Vec<serde_json::Value>>,
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
    // 1. AISURADA_CLAUDE_BIN
    if let Ok(bin) = env::var("AISURADA_CLAUDE_BIN") {
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

pub fn resolve_gemini() -> Option<PathBuf> {
    // 1. AISURADA_ANTIGRAVITY_BIN 또는 AISURADA_GEMINI_BIN
    if let Ok(bin) = env::var("AISURADA_ANTIGRAVITY_BIN").or_else(|_| env::var("AISURADA_GEMINI_BIN")) {
        if !bin.is_empty() {
            let p = PathBuf::from(&bin);
            if is_regular_file(&p) {
                return Some(p);
            }
        }
    }

    let bin_names = ["antigravity", "agy", "gemini"];

    // 2. $HOME/.local/bin
    if let Ok(home) = env::var("HOME") {
        if !home.is_empty() {
            for name in &bin_names {
                let p = PathBuf::from(&home).join(format!(".local/bin/{}", name));
                if is_regular_file(&p) {
                    return Some(p);
                }
            }
        }
    }

    // 3. /opt/homebrew/bin
    for name in &bin_names {
        let brew_opt = PathBuf::from(format!("/opt/homebrew/bin/{}", name));
        if is_regular_file(&brew_opt) {
            return Some(brew_opt);
        }
    }

    // 4. /usr/local/bin
    for name in &bin_names {
        let usr_local = PathBuf::from(format!("/usr/local/bin/{}", name));
        if is_regular_file(&usr_local) {
            return Some(usr_local);
        }
    }

    // 5. PATH
    if let Ok(path_var) = env::var("PATH") {
        for dir in env::split_paths(&path_var) {
            for name in &bin_names {
                let p = dir.join(name);
                if is_regular_file(&p) {
                    return Some(p);
                }
            }
        }
    }

    None
}

pub fn augmented_path() -> String {
    augmented_path_with(TOOLS_DIR.get().map(PathBuf::as_path))
}

/// [`augmented_path`]의 몸. 도구 폴더가 실제로 있는 폴더일 때만 맨 앞에 얹는다. 사용자가 예전 가이드대로 만든
/// `~/.local/bin/ais-cli`(시스템 Java를 부른다)보다 앱에 든 것이 먼저 잡히게 하려는 것이다.
fn augmented_path_with(tools: Option<&Path>) -> String {
    let mut prefixes: Vec<String> = Vec::new();
    if let Some(dir) = tools.filter(|d| d.is_dir()) {
        prefixes.push(dir.to_string_lossy().into_owned());
    }
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

/// 묻기 길. 다시 지은 호스트는 깃발 길(AISURADA_ASK_VIA_HTTP, AISURADA_ASK_VIA_AGENT_SDK)을 안 지었으므로
/// 깃발이 있어도 기본 길 cli 하나다(1장 1.6, 부록 E E.8의 「늘 cli」).
#[tauri::command]
pub fn ask_runtime() -> &'static str {
    "cli"
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

/// 실패 글의 꼬리. 걷은 글이 400자 이하이면 그대로이고, 넘으면 … 뒤에 마지막 400자를 붙인다(5.8).
pub fn tail_chars(text: &str) -> String {
    let count = text.chars().count();
    if count <= CAUSE_CHARS {
        text.to_string()
    } else {
        let tail: String = text.chars().skip(count - CAUSE_CHARS).collect();
        format!("…{}", tail)
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

    let tail = tail_chars(raw);

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

/// CLI 실행 파일을 한 번 띄워 끝나기를 기다린다. 상한에 닿으면 2부의 `process::run` 이 SIGTERM, 3초, SIGKILL로
/// 끝내고, 출력 파이프를 다른 프로세스가 쥐고 있어도 상한에서 답한다(5.6).
fn run_cli_process(
    binary: &Path,
    work_dir: &Path,
    vault: &str,
    args: &[String],
    input_text: &str,
    timeout_secs: u64,
) -> Result<ExecResult, RunError> {
    let mut cmd = Command::new(binary);
    cmd.current_dir(work_dir);
    cmd.args(args);

    // 환경 변수 설정
    cmd.env("PATH", augmented_path());
    cmd.env("AISURADA_VAULT", vault);
    cmd.env("GEMINI_CLI_TRUST_WORKSPACE", "true");

    match process::run_typed(
        &mut cmd,
        Some(input_text.as_bytes().to_vec()),
        Duration::from_secs(timeout_secs),
    )? {
        Outcome::Finished(output) => Ok(ExecResult {
            exit_code: output.status.code(),
            timed_out: false,
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        }),
        Outcome::TimedOut => Ok(ExecResult {
            exit_code: None,
            timed_out: true,
            stdout: String::new(),
            stderr: String::new(),
        }),
    }
}

/// 첫 부름의 표준오류로 다시 걸 세션 플래그를 가른다(5.4). true 는 --resume, false 는 --session-id 다.
/// 두 글자가 함께 들면 「is already in use」를 먼저 본다.
fn flip_target(stderr: &str) -> Option<bool> {
    if stderr.contains("is already in use") {
        Some(true)
    } else if stderr.contains("No conversation found") {
        Some(false)
    } else {
        None
    }
}

/// 새 세션을 여는 부름의 물음 글. 앞 문답이 실릴 것이 있으면 물음 앞에 싣고, 없으면 물음 글 그대로다(5.3, 8.8).
/// 앞뒤 공백을 걷어 빈 content 와 user, assistant 가 아닌 role 은 건너뛴다.
fn seed_prompt(seed: &[serde_json::Value], prompt: &str) -> String {
    let mut body = String::new();
    for message in seed {
        let head = match message.get("role").and_then(|value| value.as_str()) {
            Some("user") => "### 사용자",
            Some("assistant") => "### Claude",
            _ => continue,
        };
        let content = message.get("content").and_then(|value| value.as_str()).unwrap_or("").trim();
        if content.is_empty() {
            continue;
        }
        body.push_str(head);
        body.push_str("\n\n");
        body.push_str(content);
        body.push_str("\n\n");
    }
    if body.is_empty() {
        return prompt.to_string();
    }
    format!(
        "## 앞 대화\n\n이 대화는 다른 길에서 시작해 여기로 옮겨 왔다. 아래가 앞서 오간 문답이다. 이 문답을 이어서 답하라.\n\n{}## 이번 물음\n\n{}",
        body, prompt
    )
}

fn failure(error: String, elapsed_ms: u64, missing_cwd: Option<String>) -> AskClaudeResponse {
    AskClaudeResponse {
        ok: false,
        answer: String::new(),
        error: Some(error),
        elapsed_ms,
        missing_cwd,
    }
}

/// 받은 `route` 를 검사한다. 값이 없거나 http 하나만 받고(http 도 기본 길 claude -p 로 간다),
/// 다른 값은 커맨드째 거절한다(1장 1.6).
fn check_route(route: Option<&str>) -> Result<(), String> {
    match route {
        None | Some("http") => Ok(()),
        Some(other) => Err(format!(
            "모르는 묻기 길이다: {}. 받는 값은 http 하나다.",
            other
        )),
    }
}

pub fn map_gemini_model(model: &str) -> &str {
    match model {
        "gemini-3-8-high" | "gemini-3.8-high" | "gemini-high" => "gemini-3.8-flash-high",
        "gemini-3-8-medium" | "gemini-3.8-medium" | "gemini-medium" => "gemini-3.8-flash-medium",
        "gemini-3-8-flash" | "gemini-3.8-flash" | "gemini-flash" => "gemini-3.8-flash-high",
        other => other,
    }
}

fn execute_gemini_ask(
    args: AskClaudeArgs,
    work_dir: &Path,
    missing_cwd: Option<String>,
    start_time: Instant,
) -> AskClaudeResponse {
    let gemini_bin = match resolve_gemini() {
        Some(b) => b,
        None => {
            return failure(
                "antigravity 실행 파일을 못 찾았다. AISURADA_ANTIGRAVITY_BIN 으로 자리를 알려 줄 수 있다."
                    .to_string(),
                0,
                missing_cwd,
            );
        }
    };

    let bin_name = gemini_bin
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("antigravity");

    let input_text = if let Some(seed) = &args.seed {
        seed_prompt(seed, &args.prompt)
    } else {
        args.prompt.clone()
    };

    let mapped_model = map_gemini_model(&args.model);
    let mut cli_args = vec![
        "-p".to_string(),
        input_text.clone(),
        "--model".to_string(),
        mapped_model.to_string(),
    ];
    if bin_name.contains("gemini") {
        cli_args.push("--skip-trust".to_string());
    }

    match run_cli_process(
        &gemini_bin,
        work_dir,
        &args.vault,
        &cli_args,
        &input_text,
        args.timeout_seconds,
    ) {
        Ok(res) => {
            let elapsed = start_time.elapsed().as_millis() as u64;
            if res.timed_out {
                return AskClaudeResponse {
                    ok: false,
                    answer: String::new(),
                    error: Some(format!(
                        "{}초 안에 답이 안 왔다. {} 응답 시간을 초과했다.",
                        args.timeout_seconds, bin_name
                    )),
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
            let cause = format_cause(&res.stderr, &res.stdout);
            let code_str = res.exit_code.map_or("?".to_string(), |c| c.to_string());
            let err_msg = format!("{} 가 종료 코드 {} 로 끝났다: {}", bin_name, code_str, cause);
            failure(err_msg, elapsed, missing_cwd)
        }
        Err(RunError::Spawn(e)) => {
            let elapsed = start_time.elapsed().as_millis() as u64;
            failure(format!("{} 를 못 띄웠다: {}", bin_name, e), elapsed, missing_cwd)
        }
        Err(RunError::Io(e)) => failure(format!("{} 를 기다리던 일꾼이 끊겼다: {}", bin_name, e), 0, missing_cwd),
    }
}

pub fn execute_ask(args: AskClaudeArgs) -> AskClaudeResponse {
    let start_time = Instant::now();

    let (work_dir, missing_cwd) = determine_work_dir(&args.vault, args.cwd.as_deref());

    if args.model.starts_with("gemini") {
        return execute_gemini_ask(args, &work_dir, missing_cwd, start_time);
    }

    let claude_bin = match resolve_claude() {
        Some(b) => b,
        None => {
            return failure(
                "claude 실행 파일을 못 찾았다. AISURADA_CLAUDE_BIN 으로 자리를 알려 줄 수 있다."
                    .to_string(),
                0,
                missing_cwd,
            )
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
                return seed_prompt(seed, &args.prompt);
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

        match run_cli_process(
            &claude_bin,
            &work_dir,
            &args.vault,
            &cli_args,
            &input_text,
            args.timeout_seconds,
        ) {
            Ok(res) => {
                if res.timed_out || res.exit_code == Some(0) {
                    last_result = Some(res);
                    break;
                }

                // 종료 코드 0이 아닐 때 뒤집기 판정 (5.4). 표준오류만 본다
                let target = flip_target(&res.stderr);
                last_result = Some(res);

                if attempt == 1 {
                    if let Some(next_resume) = target {
                        // 다시 걸 플래그가 첫 부름과 같으면 안 건다. 이어 가기만 하는 물음은 --session-id 로 안 뒤집는다
                        if next_resume != current_resume
                            && (next_resume || !args.resume_only.unwrap_or(false))
                        {
                            current_resume = next_resume;
                            continue;
                        }
                    }
                }
                break;
            }
            Err(RunError::Spawn(e)) => {
                let elapsed = start_time.elapsed().as_millis() as u64;
                return failure(format!("claude 를 못 띄웠다: {}", e), elapsed, missing_cwd);
            }
            Err(RunError::Io(e)) => {
                // 기다리던 일꾼이 끊겼다. 5.8 표에서 이 실패의 elapsed_ms 는 0이다
                return failure(
                    format!("claude 를 기다리던 일꾼이 끊겼다: {}", e),
                    0,
                    missing_cwd,
                );
            }
        }
    }

    let elapsed = start_time.elapsed().as_millis() as u64;
    let res = match last_result {
        Some(r) => r,
        None => {
            return failure("실행 결과를 얻지 못했다.".to_string(), elapsed, missing_cwd);
        }
    };

    if res.timed_out {
        let msg = format!(
            "{}초 안에 답이 안 왔다. 터미널에서 `claude --resume {}` 로 이어 볼 수 있다.",
            args.timeout_seconds, args.session_id
        );
        return failure(msg, elapsed, missing_cwd);
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

    // 5.4: resumeOnly 인 물음이 실패했고 No conversation found 가 들면 앞붙임 글을 더한다. 판정은 자르기 전의
    // 표준오류 전체를 보고, 걷어서 비었을 때만 표준출력 전체를 본다. is already in use 가 함께 들면 안 붙인다
    if args.resume_only.unwrap_or(false) {
        let judged = if res.stderr.trim().is_empty() {
            &res.stdout
        } else {
            &res.stderr
        };
        if judged.contains("No conversation found") && !judged.contains("is already in use") {
            let prefix = "이 기계에서 그 세션의 대화 기록을 못 찾아서 안 보냈다. 새 대화로 바꿔 보내면 앞 맥락 없는 대화가 같은 id 로 생긴다. ";
            err_msg = format!("{}{}", prefix, err_msg);
        }
    }

    failure(err_msg, elapsed, missing_cwd)
}

#[tauri::command]
pub async fn ask_claude(
    state: tauri::State<'_, crate::lifecycle::HostState>,
    vault: String,
    session_id: String,
    model: String,
    effort: String,
    prompt: String,
    resume: bool,
    timeout_seconds: u64,
    keep_alive: Option<bool>,
    seed: Option<Vec<serde_json::Value>>,
    system: Option<String>,
    route: Option<String>,
    cwd: Option<String>,
    resume_only: Option<bool>,
    persona: Option<String>,
) -> Result<AskClaudeResponse, String> {
    // route 는 일을 시작하기 전에 거른다. 모르는 값이면 커맨드째 거절한다
    check_route(route.as_deref())?;

    let open_vault = state
        .vault
        .lock()
        .map_err(|_| "vault 잠금을 얻지 못했다")?
        .clone()
        .ok_or("vault가 아직 열리지 않았어요")?;
    if !crate::workspace::same_vault(&open_vault, &vault) {
        return Err("현재 열린 워크스페이스에서만 Claude 세션을 실행할 수 있어요".into());
    }

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

    // async 일꾼 밖의 blocking 일꾼에서 실행하고 await 로 기다린다. recv() 로 막으면 상한(1~2시간) 동안
    // async 일꾼 하나를 붙잡는다(5.1 4번). 일꾼이 끊겨도 실패는 ok: false 답 객체로 준다(5.8, elapsed_ms 0)
    let (_, missing_cwd) = determine_work_dir(&args.vault, args.cwd.as_deref());
    match tauri::async_runtime::spawn_blocking(move || execute_ask(args)).await {
        Ok(response) => Ok(response),
        Err(e) => Ok(failure(
            format!("claude 를 기다리던 일꾼이 끊겼다: {}", e),
            0,
            missing_cwd,
        )),
    }
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
    fn bundled_tools_dir_comes_first_only_when_it_exists() {
        let temp = tempfile::tempdir().unwrap();
        let tools = temp.path().join("bin");
        // 없는 폴더는 얹지 않는다(개발 실행이나 지워진 앱)
        let without = augmented_path_with(Some(&tools));
        assert!(!without.contains(tools.to_str().unwrap()));
        assert_eq!(without, augmented_path_with(None));
        // 있는 폴더는 `~/.local/bin`보다도 앞이다
        fs::create_dir_all(&tools).unwrap();
        let with = augmented_path_with(Some(&tools));
        assert!(with.starts_with(&format!("{}:", tools.display())));
        assert!(with.contains("/opt/homebrew/bin") && with.contains("/usr/local/bin"));
        assert_eq!(with.matches(tools.to_str().unwrap()).count(), 1);
    }

    #[test]
    fn ask_runtime_is_always_cli_even_with_flags() {
        let _lock = ENV_LOCK.lock().unwrap();
        env::set_var("AISURADA_ASK_VIA_HTTP", "1");
        env::set_var("AISURADA_ASK_VIA_AGENT_SDK", "1");
        let route = ask_runtime();
        env::remove_var("AISURADA_ASK_VIA_HTTP");
        env::remove_var("AISURADA_ASK_VIA_AGENT_SDK");
        assert_eq!(route, "cli");
    }

    #[test]
    fn route_accepts_http_and_rejects_the_rest_as_a_command() {
        assert!(check_route(None).is_ok());
        assert!(check_route(Some("http")).is_ok());
        assert_eq!(
            check_route(Some("sdk")).unwrap_err(),
            "모르는 묻기 길이다: sdk. 받는 값은 http 하나다."
        );
    }

    #[test]
    fn seed_trims_skips_unknown_roles_and_empty_content() {
        let seed = vec![
            serde_json::json!({"role": 4, "content": "skip"}),
            serde_json::json!({"role": "user", "content": false}),
            serde_json::json!(null),
            serde_json::json!(7),
            serde_json::json!({}),
            serde_json::json!({"role": "user", "content": "  레일 폭을 왜?  \n"}),
            serde_json::json!({"role": "system", "content": "건너뜀"}),
            serde_json::json!({"role": "assistant", "content": " \n "}),
            serde_json::json!({"role": "assistant", "content": "목업의 폭이에요."}),
        ];
        assert_eq!(
            seed_prompt(&seed, "그럼 960px?"),
            "## 앞 대화\n\n이 대화는 다른 길에서 시작해 여기로 옮겨 왔다. 아래가 앞서 오간 문답이다. 이 문답을 이어서 답하라.\n\n### 사용자\n\n레일 폭을 왜?\n\n### Claude\n\n목업의 폭이에요.\n\n## 이번 물음\n\n그럼 960px?"
        );
        let nothing = vec![serde_json::json!({"role": "user", "content": "  "})];
        assert_eq!(seed_prompt(&nothing, "물음 그대로"), "물음 그대로");
        assert_eq!(seed_prompt(&[], "물음 그대로"), "물음 그대로");
    }

    #[test]
    fn flip_checks_already_in_use_first() {
        assert_eq!(flip_target("Error: x is already in use"), Some(true));
        assert_eq!(flip_target("No conversation found"), Some(false));
        assert_eq!(flip_target("No conversation found; is already in use"), Some(true));
        assert_eq!(flip_target("boom"), None);
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

        env::set_var("AISURADA_CLAUDE_BIN", fake_bin.to_str().unwrap());
        let resolved = resolve_claude();
        assert_eq!(resolved, Some(fake_bin));
        env::remove_var("AISURADA_CLAUDE_BIN");
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
echo "VAULT: $AISURADA_VAULT"
echo "STDIN: $(cat)"
"#;
        let fake_bin = create_executable_script(dir.path(), "fake_claude_success.sh", script);
        env::set_var("AISURADA_CLAUDE_BIN", fake_bin.to_str().unwrap());

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
        env::remove_var("AISURADA_CLAUDE_BIN");

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
        env::set_var("AISURADA_CLAUDE_BIN", fake_bin.to_str().unwrap());

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
        env::remove_var("AISURADA_CLAUDE_BIN");

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
        env::set_var("AISURADA_CLAUDE_BIN", fake_bin.to_str().unwrap());

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
        env::remove_var("AISURADA_CLAUDE_BIN");

        assert!(!resp.ok, "should fail");
        let err = resp.error.unwrap();
        assert!(err.contains("이 기계에서 그 세션의 대화 기록을 못 찾아서 안 보냈다. 새 대화로 바꿔 보내면 앞 맥락 없는 대화가 같은 id 로 생긴다."));
        assert!(err.contains("No conversation found"));
    }

    #[test]
    fn test_fake_claude_timeout_reaps_child() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = tempdir().unwrap();
        let pid_file = dir.path().join("pid");
        let script = format!(
            "#!/bin/sh\necho $$ > \"{}\"\nexec sleep 10\n",
            pid_file.display()
        );
        let fake_bin = create_executable_script(dir.path(), "fake_claude_sleep.sh", &script);
        env::set_var("AISURADA_CLAUDE_BIN", fake_bin.to_str().unwrap());

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
        env::remove_var("AISURADA_CLAUDE_BIN");

        assert!(!resp.ok, "should time out");
        let err = resp.error.unwrap();
        assert!(err.contains("1초 안에 답이 안 왔다. 터미널에서 `claude --resume sess-timeout` 로 이어 볼 수 있다."));
        // 5.6: the process is really ended and reaped, not only left behind
        let pid: i32 = fs::read_to_string(&pid_file).unwrap().trim().parse().unwrap();
        let gone = unsafe { libc::kill(pid, 0) } == -1;
        assert!(gone, "상한에 끊긴 claude 프로세스가 아직 산다: {pid}");
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
        env::set_var("AISURADA_CLAUDE_BIN", fake_bin.to_str().unwrap());

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
        env::remove_var("AISURADA_CLAUDE_BIN");

        assert!(!resp.ok);
        let err = resp.error.unwrap();
        assert!(err.contains("Fatal: Invalid configuration detected"), "First line cause must be preserved (D12)");
        assert!(err.contains("…"), "Tail must be truncated with ellipsis");
    }

    fn ask_args(session: &str, resume: bool, resume_only: bool) -> AskClaudeArgs {
        AskClaudeArgs {
            vault: "/tmp".to_string(),
            session_id: session.to_string(),
            model: "claude-sonnet-5-5".to_string(),
            effort: "low".to_string(),
            prompt: "질문".to_string(),
            resume,
            timeout_seconds: 5,
            keep_alive: None,
            seed: None,
            system: None,
            route: None,
            cwd: None,
            resume_only: Some(resume_only),
            persona: None,
        }
    }

    #[test]
    fn both_letters_on_a_resume_call_do_not_flip_to_session_id() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = tempdir().unwrap();
        let count = dir.path().join("count");
        let script = format!(
            "#!/bin/sh\necho x >> \"{}\"\necho 'Session is already in use / No conversation found' >&2\nexit 1\n",
            count.display()
        );
        let fake = create_executable_script(dir.path(), "fake_both.sh", &script);
        env::set_var("AISURADA_CLAUDE_BIN", fake.to_str().unwrap());
        let resp = execute_ask(ask_args("sess-both", true, false));
        env::remove_var("AISURADA_CLAUDE_BIN");
        assert!(!resp.ok);
        assert_eq!(fs::read_to_string(count).unwrap().lines().count(), 1, "같은 플래그라 다시 안 건다");
    }

    #[test]
    fn prefix_is_judged_on_stdout_only_when_stderr_is_blank_and_skipped_with_already_in_use() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = tempdir().unwrap();
        // 표준오류가 비어 있지 않으면 표준출력은 안 본다
        let stderr_boom = create_executable_script(
            dir.path(),
            "fake_boom.sh",
            "#!/bin/sh\necho 'No conversation found'\necho 'Error: boom' >&2\nexit 1\n",
        );
        env::set_var("AISURADA_CLAUDE_BIN", stderr_boom.to_str().unwrap());
        let resp = execute_ask(ask_args("sess-prefix", true, true));
        assert!(!resp.error.unwrap().starts_with("이 기계에서"), "표준오류가 비지 않으면 표준출력은 안 본다");
        // 표준오류가 비면 표준출력을 본다
        let stdout_only = create_executable_script(
            dir.path(),
            "fake_stdout.sh",
            "#!/bin/sh\necho 'No conversation found'\nexit 1\n",
        );
        env::set_var("AISURADA_CLAUDE_BIN", stdout_only.to_str().unwrap());
        let resp = execute_ask(ask_args("sess-prefix", true, true));
        assert!(resp.error.unwrap().starts_with("이 기계에서"));
        // is already in use 가 함께 들면 안 붙인다
        let both = create_executable_script(
            dir.path(),
            "fake_both2.sh",
            "#!/bin/sh\necho 'No conversation found; is already in use' >&2\nexit 1\n",
        );
        env::set_var("AISURADA_CLAUDE_BIN", both.to_str().unwrap());
        let resp = execute_ask(ask_args("sess-prefix", true, true));
        env::remove_var("AISURADA_CLAUDE_BIN");
        assert!(!resp.error.unwrap().starts_with("이 기계에서"));
    }

    #[test]
    fn pipe_holder_after_claude_exit_answers_at_the_limit() {
        let _lock = ENV_LOCK.lock().unwrap();
        let dir = tempdir().unwrap();
        // claude 는 끝났는데 자식이 출력 파이프를 쥐고 있다. 상한에서 「상한에 끊긴 것」으로 답해야 한다
        let fake = create_executable_script(
            dir.path(),
            "fake_holder.sh",
            "#!/bin/sh\n(sleep 5 &) \nexit 0\n",
        );
        env::set_var("AISURADA_CLAUDE_BIN", fake.to_str().unwrap());
        let mut args = ask_args("sess-holder", false, false);
        args.timeout_seconds = 1;
        let started = Instant::now();
        let resp = execute_ask(args);
        env::remove_var("AISURADA_CLAUDE_BIN");
        assert!(!resp.ok);
        assert!(resp.error.unwrap().contains("1초 안에 답이 안 왔다"));
        assert!(started.elapsed() < Duration::from_secs(4));
    }

    #[test]
    fn test_resolve_gemini_with_env_var() {
        let _lock = ENV_LOCK.lock().unwrap();
        let temp = tempdir().unwrap();
        let fake = temp.path().join("antigravity");
        File::create(&fake).unwrap();

        env::set_var("AISURADA_ANTIGRAVITY_BIN", &fake);
        assert_eq!(resolve_gemini(), Some(fake));
        env::remove_var("AISURADA_ANTIGRAVITY_BIN");
    }

    #[test]
    fn test_map_gemini_model() {
        assert_eq!(map_gemini_model("gemini-3-8-high"), "gemini-3.8-flash-high");
        assert_eq!(map_gemini_model("gemini-3-8-medium"), "gemini-3.8-flash-medium");
        assert_eq!(map_gemini_model("gemini-3-8-flash"), "gemini-3.8-flash-high");
        assert_eq!(map_gemini_model("gemini-pro"), "gemini-pro");
    }

    #[test]
    fn test_gemini_missing_binary_message() {
        let _lock = ENV_LOCK.lock().unwrap();
        let orig_home = env::var("HOME").ok();
        let orig_path = env::var("PATH").ok();
        env::set_var("HOME", "/nonexistent_home_dir_for_test");
        env::set_var("PATH", "/nonexistent_path_dir_for_test");
        env::set_var("AISURADA_ANTIGRAVITY_BIN", "/nonexistent/antigravity-bin");
        env::remove_var("AISURADA_GEMINI_BIN");

        let res = execute_ask(AskClaudeArgs {
            vault: "/tmp".to_string(),
            session_id: "test-sess".to_string(),
            model: "gemini-3-8-high".to_string(),
            effort: "high".to_string(),
            prompt: "안녕".to_string(),
            resume: false,
            timeout_seconds: 10,
            keep_alive: None,
            seed: None,
            system: None,
            route: None,
            cwd: None,
            resume_only: None,
            persona: None,
        });

        if let Some(h) = orig_home {
            env::set_var("HOME", h);
        } else {
            env::remove_var("HOME");
        }
        if let Some(p) = orig_path {
            env::set_var("PATH", p);
        } else {
            env::remove_var("PATH");
        }
        env::remove_var("AISURADA_ANTIGRAVITY_BIN");

        assert!(!res.ok);
        assert!(res.error.unwrap().contains("antigravity 실행 파일을 못 찾았다"));
    }

    #[test]
    fn test_gemini_success_with_fake_binary() {
        let _lock = ENV_LOCK.lock().unwrap();
        let temp = tempdir().unwrap();
        let fake = temp.path().join("antigravity");
        let script = r#"#!/bin/sh
if ! echo "$@" | grep -q -- "-p"; then
  echo "Missing -p" >&2
  exit 1
fi
if ! echo "$@" | grep -q -- "--model gemini-3.8-flash-high"; then
  echo "Missing or incorrect --model" >&2
  exit 1
fi
echo 'Antigravity Gemini Flash 답변입니다'
exit 0
"#;
        fs::write(&fake, script).unwrap();
        let mut perms = fs::metadata(&fake).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&fake, perms).unwrap();

        env::set_var("AISURADA_ANTIGRAVITY_BIN", &fake);
        let res = execute_ask(AskClaudeArgs {
            vault: temp.path().to_str().unwrap().to_string(),
            session_id: "test-sess".to_string(),
            model: "gemini-3-8-high".to_string(),
            effort: "high".to_string(),
            prompt: "테스트 질문".to_string(),
            resume: false,
            timeout_seconds: 10,
            keep_alive: None,
            seed: None,
            system: None,
            route: None,
            cwd: None,
            resume_only: None,
            persona: None,
        });
        env::remove_var("AISURADA_ANTIGRAVITY_BIN");

        assert!(res.ok, "Expected ok but got error: {:?}", res.error);
        assert_eq!(res.answer, "Antigravity Gemini Flash 답변입니다");
    }
}
