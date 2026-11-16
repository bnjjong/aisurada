//! 표 계약 시험: 5d부터 원본이 된 두 표, MCP 도구 표(`mcp_tools.rs`)와 명령 명세 표(`spec_table.rs`)가 그 표를 읽는 코드와 어긋나지 않는지 본다.
//!
//! Java 시절에는 `McpServerTest`가 도구 스키마의 키와 엔진이 받는 키를 대조했는데 그 시험이 Java와 함께 지워졌다(이슈 #67).
//! 이름과 읽기·쓰기 구분을 표끼리 견주는 일은 `command_tables.rs`가 맡고, 여기서는 표의 **내용**을 본다.
//!
//! - 도구 스키마의 `properties`와 도구 해석기(`tool.rs`)가 받는 키가 같은 키, 같은 차례인가
//! - 스키마의 필수 키를 해석기가 스키마 차례대로 검사하는가
//! - 읽기 전용 도구가 엔진의 쓰기 명령이 아닌 것과 정확히 같은가
//! - 명세 표의 옵션이 정렬돼 있고, 모두 엔진 인자에 닿으며, 사용법 글에 적혀 있는가
//!
//! 표 자체는 `mcp_tools`, `spec_table`이 비공개 모듈이라 서버의 `tools/list` 응답과 `registry`로 읽는다.
//! 지금 알려진 어긋남은 `KNOWN_USAGE_GAPS`에 적어 둔다. **새 어긋남이 생기거나, 알려진 어긋남이 사라지면 실패한다.**

use std::collections::BTreeSet;

use ais_cli::map::to_call;
use ais_cli::mcp::McpServer;
use ais_cli::parse::ParsedCommand;
use ais_cli::registry;
use ais_cli::tool::parse_tool;
use ais_core::engine::catalog;
use ais_core::json::{self, JsonValue};

/// 도구 하나를 `tools/list`에서 읽은 것. 키 차례를 지키려고 `ais_core::json`으로 읽는다.
struct ToolInfo {
    name: String,
    /// `inputSchema.properties`의 키(적힌 차례)
    properties: Vec<String>,
    /// `inputSchema.required`
    required: Vec<String>,
    read_only: bool,
}

fn object<'a>(value: &'a JsonValue, what: &str) -> &'a Vec<(String, JsonValue)> {
    match value {
        JsonValue::Object(fields) => fields,
        _ => panic!("{what}가 객체가 아니다"),
    }
}

fn field<'a>(fields: &'a [(String, JsonValue)], key: &str) -> Option<&'a JsonValue> {
    fields.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn strings(value: Option<&JsonValue>) -> Vec<String> {
    match value {
        Some(JsonValue::Array(items)) => items
            .iter()
            .map(|item| match item {
                JsonValue::Str(text) => text.clone(),
                _ => panic!("문자열 목록이 아니다"),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn tools_list(allow_write: bool) -> Vec<ToolInfo> {
    let mut server = McpServer::new(
        allow_write,
        Box::new(|_, _| panic!("도구 목록만 본다. 호출하지 않는다")),
    );
    let reply = server
        .handle(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#)
        .expect("tools/list 응답");
    let top = json::parse(&reply).expect("JSON-RPC 응답");
    let result = field(object(&top, "응답"), "result").expect("result");
    let list = match field(object(result, "result"), "tools") {
        Some(JsonValue::Array(list)) => list,
        _ => panic!("도구 목록이 없다"),
    };
    list.iter()
        .map(|tool| {
            let fields = object(tool, "도구");
            let name = match field(fields, "name") {
                Some(JsonValue::Str(name)) => name.clone(),
                _ => panic!("도구 이름이 없다"),
            };
            let schema = object(field(fields, "inputSchema").expect("inputSchema"), "스키마");
            let properties = match field(schema, "properties") {
                Some(JsonValue::Object(props)) => props.iter().map(|(k, _)| k.clone()).collect(),
                _ => Vec::new(),
            };
            let annotations = object(
                field(fields, "annotations").expect("annotations"),
                "annotations",
            );
            let read_only = matches!(
                field(annotations, "readOnlyHint"),
                Some(JsonValue::Literal(raw)) if raw == "true"
            );
            ToolInfo {
                name,
                properties,
                required: strings(field(schema, "required")),
                read_only,
            }
        })
        .collect()
}

/// 도구를 알려진 키가 아닌 칸 하나로 불러, 거절 글이 알려 주는 「쓸 수 있는 키」를 꺼낸다.
fn accepted_keys(tool: &str) -> Vec<String> {
    let error = parse_tool(tool, &[r#"{"__no_such_key__":1}"#.to_string()], None)
        .expect_err(&format!("{tool}: 모르는 키를 받아 버렸다"));
    assert!(
        error.message.contains("모르는 키다: [__no_such_key__]."),
        "{tool}: {}",
        error.message
    );
    let list = error
        .message
        .split("쓸 수 있는 것은 [")
        .nth(1)
        .and_then(|rest| rest.split("] 뿐이다.").next())
        .unwrap_or_else(|| {
            panic!(
                "{tool}: 거절 글에 쓸 수 있는 키 목록이 없다: {}",
                error.message
            )
        });
    if list.is_empty() {
        Vec::new()
    } else {
        list.split(", ").map(str::to_string).collect()
    }
}

#[test]
fn tool_schema_keys_and_the_keys_the_parser_accepts_agree() {
    let tools = tools_list(true);
    assert!(tools.len() > 30, "도구 목록이 너무 짧다: {}", tools.len());
    let mut diffs = Vec::new();
    for tool in &tools {
        let accepted = accepted_keys(&tool.name);
        if accepted != tool.properties {
            diffs.push(format!(
                "{}: 스키마 {:?} / 해석기 {:?}",
                tool.name, tool.properties, accepted
            ));
        }
    }
    assert!(
        diffs.is_empty(),
        "도구 스키마와 해석기가 받는 키(차례 포함)가 다르다. 두 곳을 같이 고친다(`mcp_tools.rs`, `tool.rs`).\n  {}",
        diffs.join("\n  ")
    );
}

#[test]
fn required_keys_are_checked_in_schema_order() {
    for tool in tools_list(true) {
        for key in &tool.required {
            assert!(
                tool.properties.contains(key),
                "{}: 필수 키 {key}가 properties에 없다",
                tool.name
            );
        }
        let outcome = parse_tool(&tool.name, &["{}".to_string()], None);
        // 스키마 차례에서 처음 만나는 필수 키가 먼저 걸린다(Java `ArgsParser.parseTool`과 같은 순서)
        let first = tool.properties.iter().find(|k| tool.required.contains(k));
        match (first, outcome) {
            (Some(key), Err(error)) => assert_eq!(
                error.message,
                format!("인자 검증 실패({}): {key} 가 필요하다.", tool.name),
                "{}",
                tool.name
            ),
            (Some(key), Ok(_)) => panic!("{}: 필수 키 {key} 없이 불러도 받아 버렸다", tool.name),
            (None, Err(error)) => assert!(
                !error.message.contains("가 필요하다."),
                "{}: 스키마에 필수 키가 없는데 해석기가 요구한다: {}",
                tool.name,
                error.message
            ),
            (None, Ok(_)) => {}
        }
    }
}

#[test]
fn read_only_tools_are_exactly_the_commands_the_engine_does_not_write() {
    let writes: BTreeSet<&str> = catalog().filter(|c| c.is_write()).map(|c| c.name).collect();
    let read_mode: BTreeSet<String> = tools_list(false).into_iter().map(|t| t.name).collect();
    let mut diffs = Vec::new();
    for tool in tools_list(true) {
        let engine_writes = writes.contains(tool.name.as_str());
        if tool.read_only == engine_writes {
            diffs.push(format!(
                "{}: MCP 읽기 전용 표시 {}, 엔진 쓰기 명령 {engine_writes}",
                tool.name, tool.read_only
            ));
        }
        if tool.read_only != read_mode.contains(&tool.name) {
            diffs.push(format!(
                "{}: 읽기 전용 표시({})와 읽기 모드 서버의 목록 포함이 다르다",
                tool.name, tool.read_only
            ));
        }
    }
    assert!(
        diffs.is_empty(),
        "MCP 도구의 읽기·쓰기 표시가 엔진과 다르다.\n  {}",
        diffs.join("\n  ")
    );
}

fn engine_command_names() -> Vec<&'static str> {
    registry::ENGINE_COMMANDS_61
        .iter()
        .chain(registry::ENGINE_COMMANDS_TASKS)
        .chain(registry::ENGINE_COMMANDS_APPROVALS)
        .copied()
        .collect()
}

/// 옵션 효과를 재려고 명령마다 넘기는 자리 인자 표본. 명세의 상한까지 채우되 둘을 넘기지 않는다.
fn sample_positionals(name: &str, max: Option<usize>) -> Vec<String> {
    match name {
        // 검색어를 자리 인자로 주면 `query=` 옵션은 쓰이지 않는다(자리 인자가 먼저). 옵션은 따로 본다.
        "search-docs" => Vec::new(),
        // 둘째 자리는 판 번호라 1 이상의 정수여야 한다
        "history" => vec!["x0".to_string(), "1".to_string()],
        _ => (0..max.unwrap_or(1).min(2))
            .map(|i| format!("x{i}"))
            .collect(),
    }
}

fn call_with(
    name: &str,
    positional: &[String],
    options: &[(&str, &str)],
) -> Result<serde_json::Value, String> {
    let parsed = ParsedCommand {
        name: name.to_string(),
        positional: positional.to_vec(),
        options: options
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    };
    to_call(&parsed, None)
        .map(|call| call.args)
        .map_err(|e| e.message)
}

#[test]
fn command_options_are_sorted_and_each_one_reaches_the_engine_arguments() {
    let mut diffs = Vec::new();
    for name in engine_command_names() {
        let spec = registry::command(name).unwrap_or_else(|| panic!("{name}: 명세가 없다"));
        let mut sorted = spec.allowed.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        if sorted != spec.allowed {
            diffs.push(format!(
                "{name}: 옵션이 정렬돼 있지 않거나 겹친다: {:?}",
                spec.allowed
            ));
        }
        let positional = sample_positionals(name, spec.max);
        // search-docs는 어떤 옵션을 주든 검색어가 있어야 하므로 검색어를 기본으로 깔아 둔다
        let base_options: Vec<(&str, &str)> = if name == "search-docs" {
            vec![("query", "q")]
        } else {
            Vec::new()
        };
        let base = call_with(name, &positional, &base_options);
        assert!(
            base.is_ok(),
            "{name}: 옵션 없이 부른 기준 호출이 실패한다(표본을 고친다): {:?}",
            base.as_ref().err()
        );
        for option in spec.allowed {
            if name == "search-docs" && *option == "query" {
                continue;
            }
            let mut options = base_options.clone();
            options.push((option, "5"));
            if call_with(name, &positional, &options) == base {
                diffs.push(format!(
                    "{name}: 옵션 {option}을 줘도 엔진 인자가 그대로다(해석기가 버린다)"
                ));
            }
        }
    }
    // search-docs의 query 옵션은 자리 인자 없이 줄 때 검색어가 된다
    let by_option = call_with("search-docs", &[], &[("query", "찾기")]).expect("query 옵션");
    assert_eq!(by_option["query"], "찾기");
    assert!(
        diffs.is_empty(),
        "명령 명세의 옵션이 엔진 인자로 이어지지 않는다. `spec_table.rs`와 `map.rs`를 같이 고친다.\n  {}",
        diffs.join("\n  ")
    );
}

/// 사용법 글과 옵션 목록이 어긋난 곳. 사용법 글은 Java가 오류 때 찍던 글을 그대로 옮긴 것이라(`cli_compat`이 글자까지 견준다)
/// 고치면 기준과 어긋난다. 그래서 일단 알려진 어긋남으로 적어 둔다.
const KNOWN_USAGE_GAPS: &[&str] = &[
    "create-agenda: 옵션 projectRepo가 사용법 글에 없다",
    "create-project: 옵션 repos가 사용법 글에 없다",
    // 사용법 글이 다른 명령의 옵션(`list-… trash=true`)을 안내한다. 이 명령의 옵션이 아니다.
    "restore-resource: 사용법 글의 trash=는 옵션 목록에 없다",
    "restore-task: 사용법 글의 trash=는 옵션 목록에 없다",
    // 검색어는 자리 인자가 먼저이고 `query=`는 대신 쓰는 길이라 사용법에는 자리 인자만 적혀 있다
    "search-docs: 옵션 query가 사용법 글에 없다",
    "update-agenda: 옵션 projectRepo가 사용법 글에 없다",
    "update-project: 옵션 repos가 사용법 글에 없다",
];

/// 사용법 글에서 `이름=` 꼴로 적힌 옵션 이름들
fn usage_option_names(usage: &str) -> BTreeSet<String> {
    usage
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '=' || c == '_' || c == '-'))
        .filter_map(|token| token.split_once('='))
        .map(|(key, _)| key)
        .filter(|key| {
            !key.starts_with('-')
                && !key.is_empty()
                && key
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
        .map(str::to_string)
        .collect()
}

#[test]
fn usage_texts_name_the_options_the_command_accepts() {
    let mut now = Vec::new();
    for name in engine_command_names() {
        let spec = registry::command(name).unwrap_or_else(|| panic!("{name}: 명세가 없다"));
        let named = usage_option_names(spec.usage);
        for option in spec.allowed {
            if !named.contains(*option) {
                now.push(format!("{name}: 옵션 {option}가 사용법 글에 없다"));
            }
        }
        for key in &named {
            if !spec.allowed.contains(&key.as_str()) {
                now.push(format!("{name}: 사용법 글의 {key}=는 옵션 목록에 없다"));
            }
        }
    }
    now.sort();
    let known: Vec<String> = KNOWN_USAGE_GAPS.iter().map(|s| s.to_string()).collect();
    let new: Vec<&String> = now.iter().filter(|d| !known.contains(d)).collect();
    let gone: Vec<&String> = known.iter().filter(|d| !now.contains(d)).collect();
    assert!(
        new.is_empty(),
        "사용법 글과 옵션 목록에 새 어긋남이 생겼다. 맞추거나 의도한 것이면 KNOWN_USAGE_GAPS에 적는다.\n  {}",
        new.iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
    assert!(
        gone.is_empty(),
        "알려진 어긋남이 사라졌다. KNOWN_USAGE_GAPS에서 지운다.\n  {}",
        gone.iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}
