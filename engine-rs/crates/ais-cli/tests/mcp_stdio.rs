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

/// 앱이 닫힌 동안 단발 `ais-cli`가 남긴 대기 기록을, 나중에 뜬 MCP 서버가 이어받아 커밋한다(결정 2: 상주 사이드카 없이 자동 커밋).
#[test]
fn mcp_server_commits_the_record_a_single_shot_cli_left_behind() {
    let vault = fixture("recover");
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(&vault)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "tester")
            .env("GIT_COMMITTER_NAME", "tester")
            .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
            .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["add", "-A"]);
    git(&["commit", "-q", "-m", "init"]);
    let commits = || {
        git(&["rev-list", "--count", "HEAD"])
            .trim()
            .parse::<u32>()
            .unwrap()
    };
    let before = commits();

    // 단발 호출: 노트를 쓰고 곧바로 끝난다. 유휴 타이머는 5초 뒤에야 도는데 그 전에 프로세스가 끝난다.
    let env = |command: &mut Command| {
        command
            .env_clear()
            .env("AISURADA_VAULT", &vault)
            .env("AISURADA_LOCK_DIR", vault.join("../locks-for-mcp-test"))
            .env("AISURADA_VAULT_AUTOCOMMIT_IDLE_SECONDS", "0")
            .env("HOME", &vault);
    };
    let mut cli = Command::new(env!("CARGO_BIN_EXE_ais-cli"));
    cli.args([
        "create_doc",
        r##"{"agenda":"demo","type":"plan","title":"단발이 남긴 노트","body":"# 본문\n"}"##,
    ]);
    env(&mut cli);
    let out = cli.output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        commits(),
        before,
        "단발 호출은 커밋하지 않고 대기 기록만 남긴다"
    );
    assert!(vault.join(".git/aisurada-pending.json").is_file());

    // 나중에 뜬 MCP 서버가 첫 도구 호출에서 남은 기록을 이어받아 커밋한다
    let mut server = Command::new(env!("CARGO_BIN_EXE_ais-cli"));
    server
        .arg("mcp")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    env(&mut server);
    let mut child = server.spawn().unwrap();
    {
        let stdin = child.stdin.as_mut().unwrap();
        writeln!(stdin, "{}", call(1, "list_tags", json!({}))).unwrap();
        stdin.flush().unwrap();
    }
    let mut committed = false;
    for _ in 0..50 {
        if commits() > before {
            committed = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    drop(child.stdin.take());
    let _ = child.wait_with_output();
    assert!(committed, "MCP 서버가 남은 기록을 커밋해야 한다");
    let message = git(&["log", "-1", "--format=%B"]);
    // `create_doc` 도구 표면의 호출이라 쓴 쪽은 도구(tool)로 남는다
    assert!(
        message.contains("Aisurada-Writer: tool") && message.contains("Aisurada-Reason: idle"),
        "{message}"
    );
    assert!(!vault.join(".git/aisurada-pending.json").exists());
    let _ = std::fs::remove_dir_all(&vault);
}
