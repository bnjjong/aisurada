//! aisurada 명령줄 클라이언트의 몸통.
//!
//! Java `ais-cli`는 사이드카 프로세스에 요청을 보내고 엔진이 해석·실행·출력까지 했다. 여기서는 같은 프로세스 안에서
//! 인자를 풀고(`parse`), 엔진 호출로 바꾸고(`map`), Rust 엔진(`ais-core`)을 부르고, 응답을 Java와 같은 모양으로
//! 되돌려(`legacy`) 찍는다(`output`). 종료 코드도 같다: 0 성공, 2 이름이나 vault를 못 정함, 3 AIS 오류, 5 확인 값 어긋남, 1 안쪽 실패.

pub mod env;
pub mod error;
pub mod legacy;
pub mod map;
pub mod output;
pub mod parse;
pub mod registry;
mod spec_table;

use ais_core::engine::Engine;

use crate::env::{Env, MISSING_VAULT_MESSAGE};
use crate::error::CliError;

/// 한 번의 호출 결과: 표준 출력·표준 오류에 쓸 글과 종료 코드
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

impl Outcome {
    fn out(stdout: String) -> Outcome {
        Outcome {
            stdout,
            stderr: String::new(),
            code: 0,
        }
    }

    fn err(stderr: String, code: i32) -> Outcome {
        Outcome {
            stdout: String::new(),
            stderr,
            code,
        }
    }

    fn from_error(error: &CliError) -> Outcome {
        Outcome::err(format!("{}\n", error.line()), error.exit_code())
    }
}

/// `ais-cli`를 부른다. `args`는 프로그램 이름을 뺀 명령줄이다.
pub fn run(args: &[String], env: &Env) -> Outcome {
    let Some(first) = args.first().map(String::as_str) else {
        return Outcome::out(registry::usage_text());
    };
    if first == "--help" || first == "-h" {
        return Outcome::out(registry::usage_text());
    }
    if first == "--list" || first == "-l" {
        return Outcome::out(registry::list_text());
    }

    if first == "schedule" {
        // 모르는 이름으로 본다
        return Outcome::err(registry::unknown_name_text(first), 2);
    }
    if registry::DIRECT_COMMANDS.contains(&first) {
        return Outcome::from_error(&CliError::new(
            409,
            format!("{first} 는 Rust ais-cli에 아직 없다. 5b 단계에서 옮긴다."),
        ));
    }

    let is_tool = registry::is_tool_name(first);
    if !is_tool && !registry::is_engine_command(first) {
        return Outcome::err(registry::unknown_name_text(first), 2);
    }

    run_engine(first, is_tool, &args[1..], env)
}

fn run_engine(name: &str, is_tool: bool, rest: &[String], env: &Env) -> Outcome {
    // 1. vault 정하기: `AISURADA_VAULT`가 먼저, 없으면 앱이 열어 둔 vault
    let Some(raw_vault) = env::resolve_vault(env) else {
        return Outcome::err(format!("AIS-400 {MISSING_VAULT_MESSAGE}\n"), 2);
    };
    let trimmed = ais_core::text::trim(&raw_vault).to_string();
    let vault = env::normalize_path(&trimmed, &env.cwd).unwrap_or(trimmed);
    if !std::path::Path::new(&vault).is_dir() {
        return Outcome::err(format!("AIS-400 vault 뿌리가 폴더가 아니다: {vault}\n"), 2);
    }

    // 2. 요청 크기 한도(칸 64개, 칸마다 1 MiB). 세션 표식이 붙으면 칸이 둘(에이전트가 claude가 아니면 넷) 늘어난다.
    let session = env::session_from_env(env);
    let marker_cells = match &session {
        None => 0,
        Some(source) if env::agent_for_file(&source.agent).is_some() => 4,
        Some(_) => 2,
    };
    if 1 + rest.len() + marker_cells > 64 {
        return Outcome::err(
            format!("인자 개수가 64개를 초과했습니다: {}\n", 1 + rest.len() + marker_cells),
            1,
        );
    }
    if rest.iter().any(|arg| arg.len() > 1_048_576) {
        return Outcome::err("인자 길이가 1 MiB를 초과했습니다\n".to_string(), 1);
    }

    // 3. 해석하고 엔진을 부른다
    let outcome = if is_tool {
        Err(CliError::new(
            409,
            format!("{name} 도구 표면은 Rust ais-cli에 아직 없다. 5a의 다음 단계에서 옮긴다."),
        ))
    } else {
        run_command(name, rest, &vault, session.as_ref())
    };
    match outcome {
        Ok(legacy) => Outcome {
            stdout: output::stringify(&legacy.value),
            stderr: legacy.diagnostics,
            code: 0,
        },
        Err(error) => Outcome::from_error(&error),
    }
}

fn run_command(
    name: &str,
    rest: &[String],
    vault: &str,
    session: Option<&env::SessionSource>,
) -> Result<legacy::Legacy, CliError> {
    let spec = registry::command(name)
        .ok_or_else(|| CliError::new(409, format!("이 판에는 {name}이 없다")))?;
    let parsed = parse::parse_command(spec, rest)?;
    let call = map::to_call(&parsed, session)?;
    let response = Engine::new(vault).call(&call.command, call.args.clone())?;
    Ok(legacy::to_legacy(&call, vault, response))
}
