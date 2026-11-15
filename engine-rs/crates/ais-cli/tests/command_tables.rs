//! 명령 목록 교차 시험: 같은 명령 정보를 따로 적은 곳들이 서로 어긋나지 않는지 본다.
//!
//! 명령 정보는 지금 여러 곳에 흩어져 있다(DDD·헥사고날 리팩토링 계획 7단계가 표 하나로 합친다).
//! - 엔진: `ais_core::engine::{COMMANDS, WRITE_COMMANDS}`(밑줄 이름, 읽기·쓰기 구분)
//! - 명령줄: `ais_cli::registry`의 `ENGINE_COMMANDS_*`(하이픈 이름)와 명세 표 `spec_table.rs`(쓰기 깃발, 도구 이름)
//! - MCP 서버: `tools/list`가 내놓는 도구 이름(`mcp_tools.rs`)과 `registry::CLI_TOOL_NAMES`
//! - 화면: `native-host/src/data/commands.ts`의 `POSITIONAL`(자리 인자를 받는 명령)
//!
//! 지금 알려진 어긋남은 `KNOWN`에 적어 둔다. **새 어긋남이 생기거나, 알려진 어긋남이 사라지면 실패한다**(사라진 것은 목록에서 지운다).
//! 고치는 일은 동작을 바꾸므로 리팩토링 범위 밖이다(이슈 #75).

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use ais_cli::mcp::McpServer;
use ais_cli::registry;
use ais_core::engine::{COMMANDS, WRITE_COMMANDS};
use serde_json::Value;

/// 지금 있는 어긋남(없애면 목록에서도 지운다)
const KNOWN: &[&str] = &[
    // 의도한 차이: `purge_doc`는 되돌릴 수 없는 삭제라 MCP 서버가 모델에게 절대 내놓지 않는다(`mcp.rs`의 `NEVER_EXPOSED`)
    "CLI_TOOL_NAMES에는 있고 MCP 쓰기 모드 도구에는 없다: purge_doc",
    // 어긋남: 명령줄 명세 표(`spec_table.rs`)는 `commit-vault`를 `write: false`로 적었는데 엔진은 vault를 고치는 쓰기 명령으로 본다
    // (`engine.rs`의 `WRITE_COMMANDS`). 표는 Java 엔진의 `CommandRegistry`에서 그대로 옮긴 값이다. 어느 쪽이 맞는지는 이슈 #75에서 정한다.
    "읽기/쓰기가 다르다: commit_vault (명령줄 write=false, 엔진 쓰기 목록=true)",
];

fn snake(name: &str) -> String {
    name.replace('-', "_")
}

fn mcp_tool_names(write: bool) -> BTreeSet<String> {
    let mut server = McpServer::new(
        write,
        Box::new(|_, _| panic!("도구 목록만 본다. 호출하지 않는다")),
    );
    let reply = server
        .handle(r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#)
        .expect("tools/list 응답");
    let value: Value = serde_json::from_str(&reply).expect("JSON-RPC 응답");
    value["result"]["tools"]
        .as_array()
        .expect("도구 목록")
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_string())
        .collect()
}

/// `commands.ts`의 `POSITIONAL` 객체에 적힌 명령 이름(밑줄)
fn screen_positional_names() -> BTreeSet<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../native-host/src/data/commands.ts");
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let start = text
        .find("const POSITIONAL")
        .expect("commands.ts에 POSITIONAL이 없다");
    let block = &text[start..];
    let block = &block[block.find('{').unwrap()..];
    let block = &block[..block.find("\n};").expect("POSITIONAL 끝")];
    let mut names = BTreeSet::new();
    // `name: [` 꼴의 키만 모은다(값 배열 안은 건너뛴다)
    let mut rest = block;
    while let Some(i) = rest.find(": [") {
        let key: String = rest[..i]
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        if !key.is_empty() {
            names.insert(key);
        }
        let close = rest[i..].find(']').map(|j| i + j + 1).unwrap_or(rest.len());
        rest = &rest[close..];
    }
    names
}

fn differences() -> Vec<String> {
    let mut diffs: Vec<String> = Vec::new();
    let engine: BTreeSet<String> = COMMANDS.iter().map(|c| c.to_string()).collect();
    let engine_write: BTreeSet<String> = WRITE_COMMANDS.iter().map(|c| c.to_string()).collect();

    // 엔진 안: 쓰기 목록은 전체 목록의 부분이다
    for name in engine_write.difference(&engine) {
        diffs.push(format!("WRITE_COMMANDS에만 있다(COMMANDS에 없다): {name}"));
    }

    // 명령줄 표 ↔ 엔진: 명령줄이 엔진으로 보내는 이름은 엔진이 알아야 하고, 엔진 명령은 명령줄이 알아야 한다
    let cli: BTreeSet<String> = registry::ENGINE_COMMANDS_61
        .iter()
        .chain(registry::ENGINE_COMMANDS_TASKS)
        .chain(registry::ENGINE_COMMANDS_APPROVALS)
        .map(|c| snake(c))
        .collect();
    for name in cli.difference(&engine) {
        diffs.push(format!("명령줄 표에만 있다(엔진에 없다): {name}"));
    }
    for name in engine.difference(&cli) {
        diffs.push(format!("엔진에만 있다(명령줄 표에 없다): {name}"));
    }

    // 읽기/쓰기 구분: 명령줄 명세의 쓰기 깃발은 엔진의 쓰기 목록과 같아야 한다
    for name in cli.intersection(&engine) {
        let kebab = name.replace('_', "-");
        if let Some(spec) = registry::command(&kebab) {
            let cli_write = spec.write;
            let engine_is_write = engine_write.contains(name);
            if cli_write != engine_is_write {
                diffs.push(format!(
                    "읽기/쓰기가 다르다: {name} (명령줄 write={cli_write}, 엔진 쓰기 목록={engine_is_write})"
                ));
            }
        } else {
            diffs.push(format!("명령줄 명세 표에 없다: {name}"));
        }
    }

    // 도구(MCP): 도구 이름은 명세 표와 엔진이 알아야 한다. 읽기 모드 도구는 쓰기 모드 도구의 부분이다
    let tools_read = mcp_tool_names(false);
    let tools_write = mcp_tool_names(true);
    let cli_tools: BTreeSet<String> = registry::CLI_TOOL_NAMES.iter().map(|t| t.to_string()).collect();
    for name in &tools_write {
        if !engine.contains(name) {
            diffs.push(format!("MCP 도구가 엔진에 없다: {name}"));
        }
        if registry::tool(name).is_none() {
            diffs.push(format!("MCP 도구가 명령줄 명세 표에 없다: {name}"));
        }
    }
    for name in tools_read.difference(&tools_write) {
        diffs.push(format!("읽기 모드에만 있는 MCP 도구: {name}"));
    }
    for name in cli_tools.difference(&tools_write) {
        diffs.push(format!("CLI_TOOL_NAMES에는 있고 MCP 쓰기 모드 도구에는 없다: {name}"));
    }

    // 화면: 자리 인자를 받는 명령은 엔진이 알아야 한다
    for name in screen_positional_names().difference(&engine) {
        diffs.push(format!("화면 POSITIONAL에만 있다(엔진에 없다): {name}"));
    }
    diffs.sort();
    diffs
}

#[test]
fn command_tables_agree_except_for_known_differences() {
    let now = differences();
    if std::env::var_os("PRINT_COMMAND_DIFFS").is_some() {
        for line in &now {
            println!("    {line:?},");
        }
    }
    let known: Vec<String> = KNOWN.iter().map(|s| s.to_string()).collect();
    let new: Vec<&String> = now.iter().filter(|d| !known.contains(d)).collect();
    let gone: Vec<&String> = known.iter().filter(|d| !now.contains(d)).collect();
    assert!(
        new.is_empty(),
        "명령 목록에 새 어긋남이 생겼다. 표를 맞추거나 의도한 것이면 KNOWN에 적는다.\n  {}",
        new.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n  ")
    );
    assert!(
        gone.is_empty(),
        "알려진 어긋남이 사라졌다. KNOWN에서 지운다.\n  {}",
        gone.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n  ")
    );
}
