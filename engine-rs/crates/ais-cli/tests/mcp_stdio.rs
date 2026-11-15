//! 실제 `ais-cli mcp` 프로세스를 표준 입출력으로 부려 본다. 합성 vault(`tools/compat/make-fixture.sh`)를 쓰고 Java 없이 돈다.
//! Java 서버와 프로토콜을 맞대는 시험은 `examples/mcp_compat.rs`이고, 여기서는 진짜 엔진까지 이어지는지 본다.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ais-mcp-stdio-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/compat/make-fixture.sh");
    let output = Command::new("bash")
        .arg(script)
        .arg(&dir)
        .output()
        .expect("bash를 못 돌렸다");
    assert!(output.status.success(), "합성 vault를 못 만들었다");
    // 쓰기 명령은 워크스페이스 표식이 있어야 일한다
    std::fs::write(dir.join(".aisurada/workspace.json"), "{}\n").unwrap();
    dir
}

/// 서버를 띄워 메시지를 한 줄씩 보내고, 입력을 닫은 뒤 (종료 코드, 답 목록)을 돌려준다.
fn converse(vault: &Path, args: &[&str], messages: &[Value]) -> (i32, Vec<Value>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ais-cli"))
        .arg("mcp")
        .args(args)
        .env_clear()
        .env("AISURADA_VAULT", vault)
        .env("AISURADA_LOCK_DIR", vault.join("../locks-for-mcp-test"))
        .env("AISURADA_VAULT_AUTOCOMMIT_ENABLED", "false")
        .env("HOME", vault)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("ais-cli를 못 띄웠다");
    {
        let stdin = child.stdin.as_mut().unwrap();
        for message in messages {
            writeln!(stdin, "{message}").unwrap();
        }
    }
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();
    let replies = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| serde_json::from_str(line).expect("답은 JSON 한 줄이어야 한다"))
        .collect();
    (output.status.code().unwrap_or(-1), replies)
}

fn call(id: u32, name: &str, arguments: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":arguments}})
}

fn text(reply: &Value) -> &str {
    reply["result"]["content"][0]["text"].as_str().unwrap_or("")
}

#[test]
fn read_only_server_lists_reads_and_runs_a_real_tool() {
    let vault = fixture("read");
    let (code, replies) = converse(
        &vault,
        &[],
        &[
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            call(3, "list_docs", json!({})),
            call(4, "get_doc", json!({"id": "zzzz-0000"})),
            call(
                5,
                "create_doc",
                json!({"agenda":"demo","type":"plan","title":"x","body":"y"}),
            ),
            call(6, "purge_doc", json!({"trashId":"t"})),
        ],
    );
    assert_eq!(code, 0, "표준 입력이 닫히면 0으로 끝난다");
    assert_eq!(replies.len(), 6, "알림에는 답이 없다");
    assert_eq!(replies[0]["result"]["serverInfo"]["name"], "aisurada");

    let tools = replies[1]["result"]["tools"].as_array().unwrap();
    assert!(
        !tools.is_empty()
            && tools
                .iter()
                .all(|t| t["annotations"]["readOnlyHint"] == true)
    );
    assert!(tools.iter().any(|t| t["name"] == "list_docs"));
    assert!(
        !tools
            .iter()
            .any(|t| t["name"] == "create_doc" || t["name"] == "purge_doc")
    );

    // 진짜 엔진까지 간다: 합성 vault의 노트 목록이 옛 모양(배열)으로 온다
    assert_eq!(replies[2]["result"]["isError"], false);
    let listed: Value = serde_json::from_str(text(&replies[2])).unwrap();
    assert!(listed.as_array().is_some_and(|rows| rows.len() > 10));

    // 엔진 오류는 isError와 종료 코드 줄로 온다
    assert_eq!(replies[3]["result"]["isError"], true);
    assert!(text(&replies[3]).starts_with("AIS-"));
    assert!(text(&replies[3]).ends_with("(종료 코드 3)"));

    // 읽기 모드에서 쓰기 도구는 막히고, 영구 삭제는 늘 막힌다
    assert!(text(&replies[4]).contains("읽기 전용"));
    assert!(text(&replies[5]).contains("MCP로 열지 않는다"));
    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn write_server_creates_and_reads_back_a_note() {
    let vault = fixture("write");
    let (code, replies) = converse(
        &vault,
        &["--write"],
        &[
            json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}),
            call(
                2,
                "create_doc",
                json!({"agenda":"demo","type":"plan","title":"MCP로 만든 노트","body":"# 본문\n"}),
            ),
        ],
    );
    assert_eq!(code, 0);
    let tools = replies[0]["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().any(|t| t["name"] == "create_doc"));
    assert!(!tools.iter().any(|t| t["name"] == "purge_doc"));
    assert_eq!(
        replies[1]["result"]["isError"],
        false,
        "{}",
        text(&replies[1])
    );
    let created: Value = serde_json::from_str(text(&replies[1])).unwrap();
    let id = created["id"].as_str().unwrap().to_string();

    // 같은 vault를 다시 읽는다(새 서버 프로세스)
    let (_, replies) = converse(&vault, &[], &[call(1, "get_doc", json!({"id": id}))]);
    let doc: Value = serde_json::from_str(text(&replies[0])).unwrap();
    assert_eq!(doc["title"], "MCP로 만든 노트");
    // 도구 표면은 아젠다 사실층을 함께 싣는다
    assert!(doc.get("agendaFacts").is_some());
    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn bad_arguments_and_help_exit_cleanly() {
    let vault = fixture("args");
    let (code, _) = converse(&vault, &["--nope"], &[]);
    assert_eq!(code, 2);
    let output = Command::new(env!("CARGO_BIN_EXE_ais-cli"))
        .args(["mcp", "--help"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("사용법: ais-cli mcp [--write]"));
    let _ = std::fs::remove_dir_all(&vault);
}
