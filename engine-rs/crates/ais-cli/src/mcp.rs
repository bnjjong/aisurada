//! `ais-cli mcp`: Claude 같은 MCP 클라이언트가 vault의 노트를 도구로 읽고 쓰게 하는 MCP 서버.
//!
//! 표준 입출력으로 한 줄에 JSON-RPC 메시지 하나씩 주고받고(표준 입력이 닫히면 끝낸다), 도구 호출은 명령줄과 같은 길(`crate::run`)로
//! 엔진에 보낸다. 같은 프로세스 안에서 엔진을 직접 부르므로 사이드카도, 호출마다 프로세스를 띄우는 비용도 없다.
//!
//! vault는 `AISURADA_VAULT`가 있으면 그것으로, 없으면 앱이 열어 둔 vault로 정하고 도구를 부를 때마다 다시 정한다. 그래서 등록에는
//! vault 경로를 박지 않아도 앱에서 vault를 바꾸면 따라간다. 기본은 읽기 도구만 내놓고, `--write`를 주면 쓰기 도구도 내놓는다.
//! 영구 삭제(`purge_doc`)는 어느 쪽으로도 내놓지 않는다.

use std::io::{BufRead, Write};

use ais_core::json::{self, JsonValue};

use crate::env::{Env, SessionSource};
use crate::mcp_tools::TOOLS;

pub const SERVER_NAME: &str = "aisurada";
pub const SERVER_VERSION: &str = "0.1.0";

/// 이 서버가 아는 MCP 규격 개정. 맨 앞이 가장 새것이고, 클라이언트가 모르는 것을 청하면 이것으로 답한다.
const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

const PARSE_ERROR: i32 = -32700;
const INVALID_REQUEST: i32 = -32600;
const METHOD_NOT_FOUND: i32 = -32601;
const INVALID_PARAMS: i32 = -32602;
const INTERNAL_ERROR: i32 = -32603;

const INSTRUCTIONS: &str = "aisurada vault의 노트를 읽고 쓰는 도구들이다. 기존 노트를 고치는 도구(update_doc, set_favorite, evaluate_doc, link_docs, update_sub_note)는 f1이 꼭 필요하다. 고치기 전에 get_doc으로 읽어 precondition 값을 얻어 f1으로 넘긴다. 방금 create_doc이나 update_doc을 했다면 그 응답의 precondition을 쓴다. 그 사이 다른 곳에서 고쳐졌으면 덮어쓰지 않고 멈춘다. id는 list_docs나 search_docs가 준 값을 그대로 쓴다.";

/// MCP 클라이언트에 내놓는 도구 하나의 정의
#[derive(Debug)]
pub struct McpTool {
    pub name: &'static str,
    pub description: &'static str,
    /// 한 줄로 줄인 JSON 스키마 글
    pub input_schema: &'static str,
    /// 노트나 vault를 바꾸는 도구인가
    pub write: bool,
    /// 있던 내용을 지우거나 덮어쓸 수 있는 도구인가
    pub destructive: bool,
}

/// 모델에게 절대 내놓지 않는 도구. 되돌릴 수 없는 삭제다.
const NEVER_EXPOSED: [&str; 1] = ["purge_doc"];

fn find(name: &str) -> Option<&'static McpTool> {
    TOOLS.iter().find(|tool| tool.name == name)
}

/// 이 모드에서 목록에 내놓는 도구인가. 읽기 모드는 읽기 도구만, 쓰기 모드는 영구 삭제만 뺀 전부다.
fn is_exposed(tool: &McpTool, allow_write: bool) -> bool {
    !NEVER_EXPOSED.contains(&tool.name) && (allow_write || !tool.write)
}

/// 도구를 한 번 부른 결과. 명령줄 한 번의 종료 코드와 두 출력이다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

/// 도구를 부르는 쪽. 인자는 `[도구 이름, 압축한 JSON 인자]`이고, 세션 출처는 이 호출에만 쓴다.
pub type Caller<'a> = dyn FnMut(&[String], Option<SessionSource>) -> Call + 'a;

pub struct McpServer<'a> {
    allow_write: bool,
    caller: Box<Caller<'a>>,
    /// initialize 요청의 `clientInfo.name`. 어느 에이전트가 붙었는지 가릴 때 쓴다.
    client_name: Option<String>,
}

impl<'a> McpServer<'a> {
    pub fn new(allow_write: bool, caller: Box<Caller<'a>>) -> Self {
        McpServer {
            allow_write,
            caller,
            client_name: None,
        }
    }

    /// 한 줄에 메시지 하나씩 읽어 답을 쓰고, 입력이 닫히면 돌아온다.
    pub fn serve(&mut self, input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
        for line in input.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Some(response) = self.handle(&line) {
                output.write_all(response.as_bytes())?;
                output.write_all(b"\n")?;
                output.flush()?;
            }
        }
        Ok(())
    }

    /// 메시지 한 줄을 처리해 답 한 줄을 돌려준다. 알림이라 답이 없으면 `None`이다.
    pub fn handle(&mut self, line: &str) -> Option<String> {
        let parsed = match json::parse(line) {
            Ok(value) => value,
            Err(_) => return Some(error("null", PARSE_ERROR, "JSON을 못 읽었다")),
        };
        let JsonValue::Object(_) = &parsed else {
            return Some(error("null", INVALID_REQUEST, "요청은 JSON 객체여야 한다"));
        };
        let id_value = parsed.get("id");
        let has_id = id_value.is_some_and(|value| !value.is_null());
        let id = match id_value {
            Some(value) if has_id => json::compact(value),
            _ => "null".to_string(),
        };
        let Some(JsonValue::Str(method)) = parsed.get("method") else {
            // method가 없는 것은 우리가 보낸 요청의 답이거나 깨진 메시지다. 답할 id가 있을 때만 거절한다.
            return has_id.then(|| error(&id, INVALID_REQUEST, "method가 없다"));
        };
        if !has_id {
            return None; // 알림(notifications/initialized 등)에는 답하지 않는다
        }
        let params = parsed.get("params");
        // 처리 중 예상 밖 실패가 나도 서버는 살아 있고 그 요청에만 내부 오류로 답한다
        let dispatched =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match method.as_str() {
                "initialize" => result(&id, &self.initialize_result(params)),
                "ping" => result(&id, "{}"),
                "tools/list" => result(&id, &self.tools_list_result()),
                "tools/call" => self.call_tool(&id, params),
                other => error(
                    &id,
                    METHOD_NOT_FOUND,
                    &format!("지원하지 않는 메서드다: {other}"),
                ),
            }));
        Some(dispatched.unwrap_or_else(|panic| {
            let detail = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            error(
                &id,
                INTERNAL_ERROR,
                &format!("서버 안에서 예외가 났다: {detail}"),
            )
        }))
    }

    fn initialize_result(&mut self, params: Option<&JsonValue>) -> String {
        if let Some(JsonValue::Str(name)) = params
            .and_then(|p| p.get("clientInfo"))
            .and_then(|info| info.get("name"))
        {
            self.client_name = Some(name.clone());
        }
        let requested = match params.and_then(|p| p.get("protocolVersion")) {
            Some(JsonValue::Str(version)) => Some(version.as_str()),
            _ => None,
        };
        let version = requested
            .filter(|v| PROTOCOL_VERSIONS.contains(v))
            .unwrap_or(PROTOCOL_VERSIONS[0]);
        format!(
            "{{\"protocolVersion\":{},\"capabilities\":{{\"tools\":{{\"listChanged\":false}}}},\"serverInfo\":{{\"name\":{},\"version\":{}}},\"instructions\":{}}}",
            quote(version),
            quote(SERVER_NAME),
            quote(SERVER_VERSION),
            quote(INSTRUCTIONS)
        )
    }

    fn tools_list_result(&self) -> String {
        let tools: Vec<String> = TOOLS
            .iter()
            .filter(|tool| is_exposed(tool, self.allow_write))
            .map(|tool| {
                let mut annotations = format!("\"readOnlyHint\":{}", !tool.write);
                if tool.write {
                    annotations.push_str(&format!(",\"destructiveHint\":{}", tool.destructive));
                }
                format!(
                    "{{\"name\":{},\"description\":{},\"inputSchema\":{},\"annotations\":{{{annotations}}}}}",
                    quote(tool.name),
                    quote(tool.description),
                    tool.input_schema
                )
            })
            .collect();
        format!("{{\"tools\":[{}]}}", tools.join(","))
    }

    fn call_tool(&mut self, id: &str, params: Option<&JsonValue>) -> String {
        let Some(JsonValue::Str(name)) = params.and_then(|p| p.get("name")) else {
            return error(id, INVALID_PARAMS, "params.name이 필요하다");
        };
        let params = params.expect("name을 읽었으므로 params가 있다");
        let Some(tool) = find(name) else {
            return error(id, INVALID_PARAMS, &format!("없는 도구다: {name}"));
        };
        let empty = JsonValue::Object(Vec::new());
        let arguments = match params.get("arguments") {
            None => &empty,
            Some(value) if value.is_null() => &empty,
            Some(value) => value,
        };
        if !matches!(arguments, JsonValue::Object(_)) {
            return error(id, INVALID_PARAMS, "arguments는 객체여야 한다");
        }
        if NEVER_EXPOSED.contains(&name.as_str()) {
            return result(
                id,
                &tool_result(
                    "이 도구는 MCP로 열지 않는다. 영구 삭제는 앱에서 사람이 한다.",
                    true,
                ),
            );
        }
        if !is_exposed(tool, self.allow_write) {
            return result(
                id,
                &tool_result(
                    &format!(
                        "이 서버는 읽기 전용으로 떠 있어 {name} 을 쓸 수 없다. 쓰려면 서버를 `ais-cli mcp --write`로 다시 띄워야 한다."
                    ),
                    true,
                ),
            );
        }

        // Codex는 도구 호출마다 _meta에 세션 id를 싣는다. 환경변수로는 MCP 서버에 세션이 전해지지 않는다
        let session = session_from_request(
            self.client_name.as_deref(),
            meta_session_id(params.get("_meta")),
        );
        let call = (self.caller)(&[name.clone(), json::compact(arguments)], session);
        if call.status == 0 {
            let text = if call.stdout.is_empty() {
                "(출력 없음)"
            } else {
                call.stdout.as_str()
            };
            return result(id, &tool_result(text, false));
        }
        let mut text = if is_blank(&call.stderr) {
            call.stdout.clone()
        } else {
            java_strip(&call.stderr).to_string()
        };
        if text.is_empty() {
            text.push_str("(오류 글 없음)");
        }
        text.push_str(&format!("\n(종료 코드 {})", call.status));
        result(id, &tool_result(&text, true))
    }
}

/// 호출 요청의 `_meta`에서 세션 id를 꺼낸다. `sessionId`를 먼저 보고 없으면 `threadId`를 본다.
fn meta_session_id(meta: Option<&JsonValue>) -> Option<String> {
    let meta = meta?;
    ["sessionId", "threadId"]
        .iter()
        .find_map(|key| match meta.get(key) {
            Some(JsonValue::Str(value)) if !is_blank(value) => Some(value.clone()),
            _ => None,
        })
}

/// MCP 요청에 실려 온 세션 정보로 출처를 정한다. 클라이언트 이름이 codex일 때만 믿는다.
fn session_from_request(client: Option<&str>, meta_id: Option<String>) -> Option<SessionSource> {
    let (client, id) = (client?, meta_id?);
    let id = ais_core::text::trim(&id);
    if id.is_empty() {
        return None;
    }
    client
        .to_lowercase()
        .contains("codex")
        .then(|| SessionSource {
            agent: "codex".to_string(),
            id: id.to_string(),
        })
}

/// Java `Character.isWhitespace`: 줄바꿈 없는 공백(U+00A0, U+2007, U+202F)은 빼고 나머지 공백 글자
fn is_java_whitespace(c: char) -> bool {
    match c {
        '\u{a0}' | '\u{2007}' | '\u{202f}' => false,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | '\u{1c}'..='\u{1f}' => true,
        other => other.is_whitespace(),
    }
}

/// Java `String.strip()`
fn java_strip(text: &str) -> &str {
    text.trim_matches(is_java_whitespace)
}

/// Java `String.isBlank()`
fn is_blank(text: &str) -> bool {
    text.chars().all(is_java_whitespace)
}

fn quote(text: &str) -> String {
    json::compact(&JsonValue::Str(text.to_string()))
}

fn tool_result(text: &str, is_error: bool) -> String {
    format!(
        "{{\"content\":[{{\"type\":\"text\",\"text\":{}}}],\"isError\":{is_error}}}",
        quote(text)
    )
}

fn result(id: &str, result_json: &str) -> String {
    format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":{result_json}}}")
}

fn error(id: &str, code: i32, message: &str) -> String {
    format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"error\":{{\"code\":{code},\"message\":{}}}}}",
        quote(message)
    )
}

/// `ais-cli mcp [--write]`의 진입점. 표준 입력이 닫힐 때까지 돌고 종료 코드를 돌려준다.
pub fn run_stdio(args: &[String], env: &Env) -> i32 {
    let mut write = false;
    for arg in args {
        match arg.as_str() {
            "--write" => write = true,
            "--help" | "-h" => {
                println!("사용법: ais-cli mcp [--write]");
                println!(
                    "  MCP 서버로 선다(표준 입출력). vault는 AISURADA_VAULT로 정하고, 없으면 앱이 열어 둔 vault를 호출마다 따른다."
                );
                println!(
                    "  기본은 읽기 도구만 내놓는다. --write를 주면 노트를 만들고 고치는 도구도 내놓는다."
                );
                println!("  영구 삭제(purge_doc)는 어느 쪽으로도 내놓지 않는다.");
                return 0;
            }
            other => {
                eprintln!("알 수 없는 인자: {other} (ais-cli mcp [--write])");
                return 2;
            }
        }
    }
    // 프로토콜 출력은 표준 출력으로만 낸다. 안내 글은 표준 오류로 낸다.
    let vault = crate::env::resolve_vault(env);
    eprintln!(
        "aisurada MCP 서버 {SERVER_VERSION}: {}{}",
        if write {
            "읽기+쓰기"
        } else {
            "읽기 전용"
        },
        if vault.as_deref().is_none_or(is_blank) {
            " (vault를 아직 못 정했다. 앱을 열어 vault를 열면 도구 호출이 그 vault를 쓴다)"
        } else {
            ""
        }
    );

    let base = env.clone();
    watch_vault(&base);
    let mut server = McpServer::new(
        write,
        Box::new(move |args, session| call_engine(&base, args, session)),
    );
    let stdin = std::io::stdin();
    match server.serve(stdin.lock(), std::io::stdout().lock()) {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("MCP 입출력이 끊겼다: {error}");
            1
        }
    }
}

/// 이 서버가 다루는 vault의 자동 커밋을 지켜본다. 단발 `ais-cli`가 앱이 닫힌 동안 남긴 대기 기록을 이어받아 커밋하려는 것이다.
/// 호출마다 vault를 다시 정하므로 호출마다 부른다(같은 vault면 처음 한 번만 일한다).
fn watch_vault(env: &Env) {
    if let Some(vault) = crate::env::resolved_vault_path(env)
        && std::path::Path::new(&vault).is_dir()
    {
        ais_core::write::autocommit::watch(std::path::Path::new(&vault));
    }
}

/// 도구 한 번을 명령줄과 같은 길로 부른다. 안쪽에서 패닉이 나도 서버는 살아 있게 종료 코드 1로 바꾼다.
fn call_engine(base: &Env, args: &[String], session: Option<SessionSource>) -> Call {
    watch_vault(base);
    let mut env = base.clone();
    env.session = session;
    let args = args.to_vec();
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::run(&args, &env))) {
        Ok(outcome) => Call {
            status: outcome.code,
            stdout: outcome.stdout,
            stderr: outcome.stderr,
        },
        Err(panic) => {
            let detail = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            Call {
                status: 1,
                stdout: String::new(),
                stderr: format!("ais-cli 안에서 예외가 났다: {detail}\n"),
            }
        }
    }
}
