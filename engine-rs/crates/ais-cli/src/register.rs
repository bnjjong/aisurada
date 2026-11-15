//! `ais-cli claude-mcp|codex-mcp|gemini-mcp`: 에이전트 CLI에 이 앱의 MCP 서버(`aisurada`)를 등록하는 일을 대신 한다.
//! 앱의 설정 화면 버튼과 터미널이 같은 길을 쓴다.
//!
//! 에이전트의 설정 파일을 직접 고치지 않고 각 CLI의 `mcp` 하위 명령만 부른다. 등록하는 명령은 **이 명령을 돌린 바로 그 `ais-cli`
//! 실행 파일**(`mcp` 인자)이라서, 앱에서 부르면 앱 번들 안의 실행 파일이 등록된다. 등록에는 vault 경로를 박지 않는다. 서버가 호출마다
//! 앱이 열어 둔 vault를 읽으므로 앱에서 vault를 바꾸면 에이전트가 따라간다. 다시 `add`하면 바꿔 끼운다(예전에 vault를 박았거나
//! Java `java -jar ais-cli.jar mcp`로 등록한 것도 이때 사라진다).

use std::path::PathBuf;

use ais_core::json::{self, JsonValue};

use crate::Outcome;
use crate::agent_cli::{self, AgentResult};
use crate::env::{Env, resolve_vault};

pub const SERVER_NAME: &str = "aisurada";

fn quote(text: &str) -> String {
    json::compact(&JsonValue::Str(text.to_string()))
}

/// 낡은 등록인가: Java 시절의 `java -jar …/ais-cli.jar mcp`는 앱을 새로 받으면 그 자리가 사라진다.
fn is_stale(detail: &str) -> bool {
    detail.contains(".jar")
}

/// 이 `ais-cli`를 MCP 서버로 등록할 값. 서버 인자(`mcp`, `mcp --write`)와 등록 JSON을 만든다.
struct Registration {
    exe: String,
}

impl Registration {
    fn current(env: &Env) -> Result<Registration, String> {
        if env.exe.is_empty() {
            return Err("ais-cli 실행 파일의 자리를 알 수 없다.".to_string());
        }
        Ok(Registration {
            exe: env.exe.clone(),
        })
    }

    fn server_args(&self, write: bool) -> Vec<String> {
        if write {
            vec!["mcp".to_string(), "--write".to_string()]
        } else {
            vec!["mcp".to_string()]
        }
    }

    /// Claude가 받는 `add-json` 값(Claude Desktop 설정 파일의 `mcpServers` 항목과 같은 모양)
    fn payload(&self, write: bool) -> String {
        let args: Vec<String> = self.server_args(write).iter().map(|a| quote(a)).collect();
        format!(
            "{{\"type\":\"stdio\",\"command\":{},\"args\":[{}],\"env\":{{}}}}",
            quote(&self.exe),
            args.join(",")
        )
    }
}

/// 한 번의 실행을 모은다: 표준 출력에 줄 단위로 쓰고, 표준 오류에 줄 단위로 쓰고, 종료 코드를 정한다.
#[derive(Default)]
struct Run {
    out: String,
    err: String,
}

impl Run {
    fn out(&mut self, line: &str) {
        self.out.push_str(line);
        self.out.push('\n');
    }

    fn err(&mut self, line: &str) {
        self.err.push_str(line);
        self.err.push('\n');
    }

    fn finish(self, code: i32) -> Outcome {
        Outcome {
            stdout: self.out,
            stderr: self.err,
            code,
        }
    }
}

/// 앱이 열어 둔 vault도 환경변수도 없으면 도구가 실패한다. 오류는 아니라서 알리기만 한다.
fn notice_if_no_vault(run: &mut Run, env: &Env) {
    if resolve_vault(env).is_none() {
        run.err("알림: 아직 vault를 정할 수 없다. 앱을 열어 vault를 열면 도구가 그 vault를 쓴다.");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// claude-mcp
// ---------------------------------------------------------------------------------------------------------------------

const CLAUDE_USAGE: &str = "사용법: ais-cli claude-mcp <status|add|remove|config> [--write]";

/// `ais-cli claude-mcp <status|add|remove|config> [--write]`. `config`는 등록하지 않고 Claude Desktop 설정 파일에 붙여 넣을
/// `mcpServers` 조각만 출력한다. 종료 코드는 0 성공, 1 claude를 못 돌렸거나 실패, 2 쓰는 법이 틀림이다.
pub fn claude_mcp(args: &[String], env: &Env) -> Outcome {
    let mut run = Run::default();
    let mut action: Option<&str> = None;
    let mut write = false;
    for arg in args {
        match arg.as_str() {
            "--write" => write = true,
            "status" | "add" | "remove" | "config" if action.is_none() => action = Some(arg),
            _ => {
                run.err(CLAUDE_USAGE);
                return run.finish(2);
            }
        }
    }
    let Some(action) = action else {
        run.err(CLAUDE_USAGE);
        return run.finish(2);
    };
    if write && action != "add" && action != "config" {
        run.err("--write는 add와 config에서만 쓴다");
        return run.finish(2);
    }
    if action == "config" {
        return match Registration::current(env) {
            Ok(registration) => {
                run.out(&format!(
                    "{{\"mcpServers\":{{{}:{}}}}}",
                    quote(SERVER_NAME),
                    registration.payload(write)
                ));
                run.finish(0)
            }
            Err(message) => {
                run.err(&message);
                run.finish(1)
            }
        };
    }

    let Some(claude) = agent_cli::resolve(env, &["AISURADA_CLAUDE_BIN"], &["claude"]) else {
        if action == "status" {
            run.out("{\"claudeFound\":false,\"registered\":false,\"write\":false,\"detail\":\"\",\"stale\":false}");
            return run.finish(0);
        }
        run.err("claude 실행 파일을 못 찾았다. Claude Code를 먼저 깔거나 AISURADA_CLAUDE_BIN으로 자리를 알려 준다.");
        return run.finish(1);
    };

    let claude_run = |args: &[&str]| -> Result<AgentResult, String> {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        agent_cli::run("claude", &claude, env, &args)
    };
    let result: Result<i32, String> = (|| {
        match action {
            "status" => {
                let get = claude_run(&["mcp", "get", SERVER_NAME])?;
                let registered = get.status == 0;
                let detail = if registered { get.text.trim() } else { "" };
                run.out(&format!(
                    "{{\"claudeFound\":true,\"registered\":{registered},\"write\":{},\"detail\":{},\"stale\":{}}}",
                    registered && get.text.contains("--write"),
                    quote(detail),
                    registered && is_stale(&get.text)
                ));
                Ok(0)
            }
            "remove" => {
                if claude_run(&["mcp", "get", SERVER_NAME])?.status == 0 {
                    let removed = claude_run(&["mcp", "remove", SERVER_NAME, "--scope", "user"])?;
                    if removed.status != 0 {
                        run.err(&format!("연결을 못 끊었다: {}", removed.text.trim()));
                        return Ok(1);
                    }
                }
                run.out("{\"ok\":true,\"registered\":false}");
                Ok(0)
            }
            _ => {
                let registration = match Registration::current(env) {
                    Ok(registration) => registration,
                    Err(message) => {
                        run.err(&message);
                        return Ok(1);
                    }
                };
                // 같은 이름이 이미 있으면 바꿔 끼운다. 없어서 실패하는 것은 괜찮다.
                claude_run(&["mcp", "remove", SERVER_NAME, "--scope", "user"])?;
                let payload = registration.payload(write);
                let added =
                    claude_run(&["mcp", "add-json", SERVER_NAME, &payload, "--scope", "user"])?;
                if added.status != 0 {
                    run.err(&format!("연결을 못 했다: {}", added.text.trim()));
                    return Ok(1);
                }
                run.out(&format!(
                    "{{\"ok\":true,\"registered\":true,\"write\":{write},\"followsAppVault\":true,\"command\":{}}}",
                    quote(&registration.exe)
                ));
                notice_if_no_vault(&mut run, env);
                Ok(0)
            }
        }
    })();
    match result {
        Ok(code) => run.finish(code),
        Err(message) => {
            run.err(&format!("claude를 못 돌렸다: {message}"));
            run.finish(1)
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// codex-mcp, gemini-mcp
// ---------------------------------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Codex,
    Gemini,
}

impl Kind {
    fn command_name(self) -> &'static str {
        match self {
            Kind::Codex => "codex-mcp",
            Kind::Gemini => "gemini-mcp",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Kind::Codex => "Codex",
            Kind::Gemini => "Gemini",
        }
    }

    fn found_key(self) -> &'static str {
        match self {
            Kind::Codex => "codexFound",
            Kind::Gemini => "geminiFound",
        }
    }

    fn resolve(self, env: &Env) -> Option<PathBuf> {
        match self {
            Kind::Codex => agent_cli::resolve(env, &["AISURADA_CODEX_BIN"], &["codex"]),
            Kind::Gemini => agent_cli::resolve(
                env,
                &["AISURADA_ANTIGRAVITY_BIN", "AISURADA_GEMINI_BIN"],
                &["antigravity", "agy", "gemini"],
            ),
        }
    }
}

struct Registered {
    registered: bool,
    write: bool,
    detail: String,
}

/// 등록돼 있는지, 쓰기 모드인지, 그리고 CLI가 보여 주는 글. Antigravity와 Gemini CLI는 `get`이 없어 `list` 줄에서 찾는다.
fn registered(
    kind: Kind,
    gemini_cli: bool,
    cli: &std::path::Path,
    env: &Env,
) -> Result<Registered, String> {
    let run = |args: &[&str]| {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        agent_cli::run(kind.label(), cli, env, &args)
    };
    if kind == Kind::Codex {
        let get = run(&["mcp", "get", SERVER_NAME])?;
        return Ok(Registered {
            registered: get.status == 0,
            write: get.status == 0 && get.text.contains("--write"),
            detail: get.text.trim().to_string(),
        });
    }
    let list = run(&["mcp", "list"])?;
    let none = || Registered {
        registered: false,
        write: false,
        detail: String::new(),
    };
    if list.status != 0 {
        return Ok(none());
    }
    for line in list.text.split('\n') {
        let trimmed = line.trim();
        // agy: "aisurada  stdio  enabled  <명령>", gemini: "✓ aisurada: <명령> (stdio) - Connected" 꼴이다
        let mine = if gemini_cli {
            matches_gemini_line(trimmed)
        } else {
            trimmed
                .strip_prefix(SERVER_NAME)
                .and_then(|rest| rest.chars().next())
                .is_some_and(is_regex_space)
        };
        if mine {
            return Ok(Registered {
                registered: true,
                write: trimmed.contains("--write"),
                detail: trimmed.to_string(),
            });
        }
    }
    Ok(none())
}

/// 정규식 `\s`(영문 공백 글자)
fn is_regex_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\u{b}' | '\u{c}' | '\r')
}

/// 정규식 `^\S{0,3}\s*aisurada:.*`: 앞에 표시 글자(✓ 따위)가 세 글자까지 붙을 수 있다.
fn matches_gemini_line(line: &str) -> bool {
    let target = format!("{SERVER_NAME}:");
    (0..=3).any(|marks| {
        let mut chars = line.chars();
        for _ in 0..marks {
            match chars.next() {
                Some(c) if !is_regex_space(c) => {}
                _ => return false,
            }
        }
        chars
            .as_str()
            .trim_start_matches(is_regex_space)
            .starts_with(&target)
    })
}

fn remove_args(kind: Kind, gemini_cli: bool) -> Vec<String> {
    let args: &[&str] = match (kind, gemini_cli) {
        (Kind::Codex, _) => &["mcp", "remove", SERVER_NAME],
        (Kind::Gemini, true) => &["mcp", "remove", "-s", "user", SERVER_NAME],
        (Kind::Gemini, false) => &["mcp", "remove", SERVER_NAME],
    };
    args.iter().map(|a| a.to_string()).collect()
}

fn add_args(kind: Kind, gemini_cli: bool, registration: &Registration, write: bool) -> Vec<String> {
    let mut args: Vec<String> = vec!["mcp".into(), "add".into()];
    if kind == Kind::Gemini && gemini_cli {
        // gemini mcp add -s user <이름> <명령> -- <인자...>
        args.extend([
            "-s".into(),
            "user".into(),
            SERVER_NAME.into(),
            registration.exe.clone(),
            "--".into(),
        ]);
    } else {
        // codex mcp add <이름> -- <명령> <인자...>, agy mcp add <이름> -- <명령> <인자...>
        args.extend([SERVER_NAME.into(), "--".into(), registration.exe.clone()]);
    }
    args.extend(registration.server_args(write));
    args
}

/// `ais-cli codex-mcp|gemini-mcp <status|add|remove> [--write]`. 종료 코드는 0 성공, 1 CLI를 못 돌렸거나 실패, 2 쓰는 법이 틀림이다.
pub fn agent_mcp(kind: Kind, args: &[String], env: &Env) -> Outcome {
    let usage = format!(
        "사용법: ais-cli {} <status|add|remove> [--write]",
        kind.command_name()
    );
    let mut run = Run::default();
    let mut action: Option<&str> = None;
    let mut write = false;
    for arg in args {
        match arg.as_str() {
            "--write" => write = true,
            "status" | "add" | "remove" if action.is_none() => action = Some(arg),
            _ => {
                run.err(&usage);
                return run.finish(2);
            }
        }
    }
    let Some(action) = action else {
        run.err(&usage);
        return run.finish(2);
    };
    if write && action != "add" {
        run.err("--write는 add에서만 쓴다");
        return run.finish(2);
    }

    let label = kind.label();
    let found_key = kind.found_key();
    let Some(cli) = kind.resolve(env) else {
        if action == "status" {
            run.out(&format!(
                "{{\"{found_key}\":false,\"registered\":false,\"write\":false,\"detail\":\"\",\"stale\":false}}"
            ));
            return run.finish(0);
        }
        run.err(&format!(
            "{label} 실행 파일을 못 찾았다. {}",
            if kind == Kind::Codex {
                "Codex를 먼저 깔거나 AISURADA_CODEX_BIN으로 자리를 알려 준다."
            } else {
                "Antigravity(agy)나 Gemini CLI를 먼저 깔거나 AISURADA_GEMINI_BIN으로 자리를 알려 준다."
            }
        ));
        return run.finish(1);
    };
    let cli_name = cli
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let gemini_cli = kind == Kind::Gemini && cli_name == "gemini";

    let result: Result<i32, String> = (|| match action {
        "status" => {
            let found = registered(kind, gemini_cli, &cli, env)?;
            run.out(&format!(
                "{{\"{found_key}\":true,\"cli\":{},\"registered\":{},\"write\":{},\"detail\":{},\"stale\":{}}}",
                quote(&cli_name),
                found.registered,
                found.write,
                quote(if found.registered { &found.detail } else { "" }),
                found.registered && is_stale(&found.detail)
            ));
            Ok(0)
        }
        "remove" => {
            if registered(kind, gemini_cli, &cli, env)?.registered {
                let removed = agent_cli::run(label, &cli, env, &remove_args(kind, gemini_cli))?;
                if removed.status != 0 {
                    run.err(&format!("연결을 못 끊었다: {}", removed.text.trim()));
                    return Ok(1);
                }
            }
            run.out("{\"ok\":true,\"registered\":false}");
            Ok(0)
        }
        _ => {
            let registration = match Registration::current(env) {
                Ok(registration) => registration,
                Err(message) => {
                    run.err(&message);
                    return Ok(1);
                }
            };
            // 같은 이름이 이미 있으면 바꿔 끼운다. 예전에 vault를 박아 등록한 것도 이때 사라진다.
            if registered(kind, gemini_cli, &cli, env)?.registered {
                agent_cli::run(label, &cli, env, &remove_args(kind, gemini_cli))?;
            }
            let added = agent_cli::run(
                label,
                &cli,
                env,
                &add_args(kind, gemini_cli, &registration, write),
            )?;
            if added.status != 0 {
                run.err(&format!("연결을 못 했다: {}", added.text.trim()));
                return Ok(1);
            }
            run.out(&format!(
                "{{\"ok\":true,\"registered\":true,\"write\":{write},\"followsAppVault\":true,\"cli\":{},\"command\":{}}}",
                quote(&cli_name),
                quote(&registration.exe)
            ));
            notice_if_no_vault(&mut run, env);
            Ok(0)
        }
    })();
    match result {
        Ok(code) => run.finish(code),
        Err(message) => {
            run.err(&format!("{label}를 못 돌렸다: {message}"));
            run.finish(1)
        }
    }
}
