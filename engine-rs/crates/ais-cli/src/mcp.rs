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
use crate::manager::{self, Manager};
use crate::mcp_tools::{MANAGER_INTAKE_TOOLS, TOOLS};
use ais_core::{CallContext, engine::IntakeOperation};

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

/// 결정 블록 안내의 머리. Java에 없던 확장이라 기본 안내문 뒤에 붙이고, Java 기준 녹화본과 견줄 때는 이 머리부터 잘라 낸다.
pub const DECISION_GUIDE_MARK: &str = "\n\n[결정 블록]";

/// 결정 블록 안내. 사람이 골라야 할 것을 노트에 쓰고, 사람이 노트에서 답하면 이어 가는 법이다(2026-11-20 사용자와 정한 형식).
/// `exe`는 이 서버를 띄운 `ais-cli`의 경로이고, `client`는 `initialize`의 `clientInfo.name`이다. 접속한 CLI에 맞는
/// 기다리는 법만 싣고, 모르는 클라이언트에는 셋을 모두 싣는다.
fn decision_guide(client: Option<&str>, exe: &str) -> String {
    let cli = shell_word(exe);
    let claude = "- Claude Code: Bash 도구의 run_in_background로 건다. 명령이 끝나면 세션이 다시 불리므로 그동안 턴을 끝내도 된다.";
    let gemini = "- Gemini CLI: 셸 도구에 is_background=true로 건다. 사용자가 settings.json에 tools.shell.backgroundCompletionBehavior=\"inject\"와 experimental.modelSteering=true를 켜 두었으면 끝날 때 대화로 돌아온다. 켜져 있지 않으면 사람에게 「답을 다 하면 말해 달라」고 한다.";
    let codex = "- Codex: 쉬는 세션을 깨우지 못하므로 턴을 끝내지 말고 이 명령을 직접 실행해 끝날 때까지 기다린다. 백그라운드 터미널이 되면 write_stdin으로 다시 기다린다.";
    let name = client.unwrap_or("").to_ascii_lowercase();
    let how = if name.contains("claude") {
        claude.to_string()
    } else if name.contains("gemini") {
        gemini.to_string()
    } else if name.contains("codex") {
        codex.to_string()
    } else {
        [claude, gemini, codex].join("\n")
    };
    format!(
        "{DECISION_GUIDE_MARK} 사람이 골라야 할 것(정할 것)을 노트에 쓸 때는 결정 블록으로 쓰고, 사람이 노트에서 답하게 한다.\n\
결정마다 블록 하나:\n\
> [!decision] 결정 1 · 질문\n\
> - [ ] 가(추천): 설명\n\
> - [ ] 나: 설명\n\
>\n\
> 답:\n\
제목은 `1.`처럼 숫자와 점으로 시작하지 않는다(편집기가 번호 목록으로 바꾼다). `답:` 줄과 체크는 비워 둔다.\n\
모든 결정 뒤에 한 번:\n\
> [!decided] 답을 다 적었으면 체크하세요\n\
> - [ ] 답 다 했음\n\
노트를 쓴 뒤 셸에서 `{cli} wait-decisions <노트 id>[/<하위 노트 id>]`로 답을 기다린다. 사람이 「답 다 했음」을 체크해 저장하면 답을 JSON으로 내고 0으로 끝난다. 2시간이 지나면 종료 코드 6이니, 아직 답이 없다고 사람에게 알리고 멈춘다.\n\
결과의 decisions[].choice(고른 선택지)와 answer(답 줄)를 읽고 이어 간다. 답이 없는 결정(answered=false)은 추천안으로 채우지 말고, 그 결정에 기대지 않는 일만 한 뒤 다시 묻는다. 다음 묶음을 물을 때는 「답 다 했음」 체크를 풀어 둔다.\n\
기다리는 법:\n{how}"
    )
}

/// 셸에 그대로 붙일 수 있게 경로를 감싼다. 공백이나 특수 문자가 없으면 그대로 둔다.
fn shell_word(path: &str) -> String {
    if !path.is_empty()
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-+".contains(c))
    {
        path.to_string()
    } else {
        format!("'{}'", path.replace('\'', r"'\''"))
    }
}

/// `initialize`가 돌려주는 안내문: 기본 안내문 뒤에 결정 블록 안내를 붙인다.
fn instructions(client: Option<&str>) -> String {
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "ais-cli".to_string());
    format!("{INSTRUCTIONS}{}", decision_guide(client, &exe))
}

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

struct ManagerGate {
    initial: Manager,
    env: Env,
}

impl ManagerGate {
    fn current(&self) -> Result<Manager, String> {
        let current = manager::find_enabled(&self.env, &self.initial.name)?;
        if !self.initial.same_registration(&current) {
            return Err("매니저 등록이 바뀌었다. 서버를 다시 연결해 주세요".into());
        }
        Ok(current)
    }

    fn check(&self, tool: &str) -> Result<Manager, String> {
        let current = self.current()?;
        if !self.allowed(&current, tool) {
            return Err(format!(
                "매니저 {}에게 열지 않은 도구다: {tool}",
                current.name
            ));
        }
        Ok(current)
    }

    fn allowed(&self, current: &Manager, tool: &str) -> bool {
        manager::INTAKE_TOOLS.contains(&tool)
            || (self.initial.tools.iter().any(|name| name == tool)
                && current.tools.iter().any(|name| name == tool))
    }

    fn record(&self, tool: &str, result: &str, target: Option<&str>) {
        if let Err(error) = manager::record(&self.env, &self.initial, tool, result, target) {
            eprintln!("매니저 호출 기록을 쓰지 못했다: {error}");
        }
    }
}

pub struct McpServer<'a> {
    allow_write: bool,
    manager: Option<ManagerGate>,
    caller: Box<Caller<'a>>,
    /// initialize 요청의 `clientInfo.name`. 어느 에이전트가 붙었는지 가릴 때 쓴다.
    client_name: Option<String>,
}

impl<'a> McpServer<'a> {
    pub fn new(allow_write: bool, caller: Box<Caller<'a>>) -> Self {
        McpServer {
            allow_write,
            manager: None,
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
            quote(&self.manager.as_ref().map_or_else(
                || instructions(self.client_name.as_deref()),
                |gate| format!(
                    "매니저 {}의 프로젝트 {} 제보 접수 도구다. 텔레그램이나 데스크톱에서 직접 제보를 받으면 어댑터가 준 고정 메시지 키로 submit_intake를 먼저 부른다. 저장된 접수 번호와 버전을 보관한다. 처리 대기인 경우 유사 태스크와 노트를 찾은 뒤 complete_intake에 새 태스크, 재제보 댓글, 되물음이나 무시 결과를 보낸다. 완료된 접수라면 저장된 결과를 안내한다. 메시지 키가 없으면 접수 완료를 안내하지 않는다. Slack을 확인할 때는 get_intake로 설정과 커서를 읽고, 커서 뒤의 글에서 지정 이모지가 달린 글만 record_intake로 기록한다. checkedAt, 실제 검색 시각 searchedAt, Slack 반응을 아는 경우 넘긴다. Slack 처리 대기 중 asked는 새 스레드 답변이 있을 때 처리한다. 등록 채널의 제보를 모두 기록한 뒤 move_cursor를 부른다. 완료 표시가 켜져 있으면 성공 항목에 ✅를 달고 reactionAdded를 기록한다. 되물음은 스레드 답글 초안으로 사용자에게 확인받는다.",
                    gate.initial.name, gate.initial.project
                )
            ))
        )
    }

    fn tools_list_result(&self) -> String {
        let active = self.manager.as_ref().and_then(|gate| gate.current().ok());
        let intake = if self.manager.is_some() {
            MANAGER_INTAKE_TOOLS
        } else {
            &[]
        };
        let tools: Vec<String> = TOOLS
            .iter()
            .chain(intake)
            .filter(|tool| match &self.manager {
                Some(gate) => active.as_ref().is_some_and(|current| gate.allowed(current, tool.name)),
                None => is_exposed(tool, self.allow_write),
            })
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
        let intake = match (self.manager.is_some(), name.as_str()) {
            (true, "get_intake") => Some(IntakeOperation::Get),
            (true, "submit_intake") => Some(IntakeOperation::Submit),
            (true, "complete_intake") => Some(IntakeOperation::Complete),
            (true, "record_intake") => Some(IntakeOperation::Record),
            (true, "move_cursor") => Some(IntakeOperation::Move),
            _ => None,
        };
        let tool = if intake.is_some() {
            MANAGER_INTAKE_TOOLS.iter().find(|tool| tool.name == name)
        } else {
            find(name)
        };
        let Some(tool) = tool else {
            if let Some(gate) = &self.manager {
                gate.record("unknown", "rejected", None);
            }
            return error(id, INVALID_PARAMS, &format!("없는 도구다: {name}"));
        };
        let empty = JsonValue::Object(Vec::new());
        let arguments = match params.get("arguments") {
            None => &empty,
            Some(value) if value.is_null() => &empty,
            Some(value) => value,
        };
        if !matches!(arguments, JsonValue::Object(_)) {
            if let Some(gate) = &self.manager {
                gate.record(tool.name, "rejected", None);
            }
            return error(id, INVALID_PARAMS, "arguments는 객체여야 한다");
        }
        if NEVER_EXPOSED.contains(&name.as_str()) {
            if let Some(gate) = &self.manager {
                gate.record(tool.name, "rejected", None);
            }
            return result(
                id,
                &tool_result(
                    "이 도구는 MCP로 열지 않는다. 영구 삭제는 앱에서 사람이 한다.",
                    true,
                ),
            );
        }
        let managed = if let Some(gate) = &self.manager {
            match gate.check(name) {
                Ok(manager) => Some((gate, manager)),
                Err(message) => {
                    gate.record(tool.name, "rejected", None);
                    return result(id, &tool_result(&message, true));
                }
            }
        } else {
            None
        };
        if managed.is_none() && !is_exposed(tool, self.allow_write) {
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
        if let (Some(operation), Some((gate, manager))) = (intake, managed.as_ref()) {
            let raw = json::compact(arguments);
            if raw.len() > 1_048_576 {
                gate.record(tool.name, "rejected", None);
                return result(id, &tool_result("접수 인자는 1 MiB까지 보낼 수 있다", true));
            }
            let args: serde_json::Value = match serde_json::from_str(&raw) {
                Ok(value) => value,
                Err(_) => return result(id, &tool_result("접수 인자를 읽지 못했다", true)),
            };
            let mut manager = manager.clone();
            manager
                .tools
                .retain(|tool| gate.initial.tools.contains(tool));
            let context = CallContext::ai(
                session.as_ref().map(|source| source.id.clone()),
                session.as_ref().map(|source| source.agent.clone()),
            );
            return match manager.intake(operation, context, args) {
                Ok(value) => {
                    gate.record(tool.name, "ok", value["item"]["entry"]["id"].as_str());
                    result(id, &tool_result(&value.to_string(), false))
                }
                Err(error) => {
                    gate.record(tool.name, "rejected", None);
                    result(id, &tool_result(&error.to_string(), true))
                }
            };
        }
        let args = [name.clone(), json::compact(arguments)];
        let call = if let Some((gate, manager)) = managed {
            let call = call_engine(&gate.env, &args, session, Some(&manager));
            let payload = (call.status == 0)
                .then(|| serde_json::from_str::<serde_json::Value>(&call.stdout).ok())
                .flatten();
            let target = payload
                .as_ref()
                .and_then(|value| value["key"].as_str().or_else(|| value["id"].as_str()));
            let outcome = match call.status {
                0 => "ok",
                2..=5 => "rejected",
                _ => "failed",
            };
            gate.record(tool.name, outcome, target);
            call
        } else {
            (self.caller)(&args, session)
        };
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

/// `ais-cli mcp [--write | --manager NAME]`의 진입점. 표준 입력이 닫힐 때까지 돌고 종료 코드를 돌려준다.
pub fn run_stdio(args: &[String], env: &Env) -> i32 {
    let mut write = false;
    let mut manager_name = None;
    let mut args = args.iter().map(String::as_str);
    while let Some(arg) = args.next() {
        match arg {
            "--write" => write = true,
            "--manager" if manager_name.is_none() => {
                let Some(name) = args.next().filter(|name| !name.starts_with('-')) else {
                    eprintln!("--manager 뒤에 매니저 이름이 필요하다");
                    return 2;
                };
                manager_name = Some(name);
            }
            option if option.starts_with("--manager=") && manager_name.is_none() => {
                manager_name = option.strip_prefix("--manager=");
            }
            "--help" | "-h" => {
                println!("사용법: ais-cli mcp [--write | --manager NAME]");
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
                eprintln!("알 수 없는 인자: {other} (ais-cli mcp [--write | --manager NAME])");
                return 2;
            }
        }
    }
    let manager = match manager_name
        .map(|name| manager::find_enabled(env, name))
        .transpose()
    {
        Ok(value) => value,
        Err(error) => {
            eprintln!("{error}");
            return 2;
        }
    };
    // 프로토콜 출력은 표준 출력으로만 낸다. 안내 글은 표준 오류로 낸다.
    let vault = if let Some(manager) = &manager {
        Some(manager.vault.clone())
    } else {
        crate::env::resolve_vault(env)
    };
    eprintln!(
        "aisurada MCP 서버 {SERVER_VERSION}: {}{}",
        if let Some(manager) = &manager {
            format!("매니저 {}", manager.name)
        } else if write {
            "읽기+쓰기".into()
        } else {
            "읽기 전용".into()
        },
        if vault.as_deref().is_none_or(is_blank) {
            " (vault를 아직 못 정했다. 앱을 열어 vault를 열면 도구 호출이 그 vault를 쓴다)"
        } else {
            ""
        }
    );

    let base = env.clone();
    if let Some(manager) = &manager {
        ais_core::write::autocommit::watch(std::path::Path::new(&manager.vault));
    } else {
        watch_vault(&base);
    }
    let mut server = McpServer::new(
        write,
        Box::new(move |args, session| call_engine(&base, args, session, None)),
    );
    server.manager = manager.map(|initial| ManagerGate {
        initial,
        env: env.clone(),
    });
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
fn call_engine(
    base: &Env,
    args: &[String],
    session: Option<SessionSource>,
    manager: Option<&Manager>,
) -> Call {
    let mut env = base.clone();
    if let Some(manager) = manager {
        env.vars
            .insert("AISURADA_VAULT".into(), manager.vault.clone());
    }
    watch_vault(&env);
    env.session = session;
    let args = args.to_vec();
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::run_with_manager(&args, &env, manager)
    })) {
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
