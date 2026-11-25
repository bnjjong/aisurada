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
    converse_with(vault, args, &[], messages)
}

/// `converse`에 서버 프로세스의 환경 변수를 더해 준다(세션 표식, 승인 기록 폴더 따위).
fn converse_with(
    vault: &Path,
    args: &[&str],
    extra_env: &[(&str, &str)],
    messages: &[Value],
) -> (i32, Vec<Value>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ais-cli"))
        .arg("mcp")
        .args(args)
        .env_clear()
        .env("AISURADA_VAULT", vault)
        .env("AISURADA_LOCK_DIR", vault.join("../locks-for-mcp-test"))
        .env("AISURADA_VAULT_AUTOCOMMIT_ENABLED", "false")
        .env("HOME", vault)
        .envs(extra_env.iter().copied())
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

/// 단발 `ais-cli`를 빈 환경에서 부른다. `extra_env`로만 세션 표식 따위를 준다. (종료 코드, 표준 출력, 표준 오류)
fn run_cli(vault: &Path, extra_env: &[(&str, &str)], args: &[&str]) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_ais-cli"))
        .args(args)
        .env_clear()
        .env("AISURADA_VAULT", vault)
        .env("AISURADA_LOCK_DIR", vault.join("../locks-for-mcp-test"))
        .env("AISURADA_VAULT_AUTOCOMMIT_ENABLED", "false")
        .env("HOME", vault)
        .envs(extra_env.iter().copied())
        .output()
        .expect("ais-cli를 못 띄웠다");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// `notes/` 아래에서 머리에 `id: <id>` 줄이 있는 마크다운 파일의 글을 찾는다.
fn note_text(vault: &Path, id: &str) -> String {
    fn walk(dir: &Path, wanted: &str, found: &mut Option<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, wanted, found);
            } else if path.extension().is_some_and(|ext| ext == "md")
                && let Ok(text) = std::fs::read_to_string(&path)
                && text.contains(wanted)
            {
                *found = Some(text);
            }
        }
    }
    let mut found = None;
    walk(&vault.join("notes"), &format!("\nid: {id}\n"), &mut found);
    found.unwrap_or_else(|| panic!("노트 {id} 파일을 못 찾았다"))
}

/// 도구 호출의 답이 오류가 아니어야 한다. 본문 글을 JSON으로 읽어 돌려준다.
fn tool_json(reply: &Value) -> Value {
    assert_eq!(reply["result"]["isError"], false, "{}", text(reply));
    serde_json::from_str(text(reply)).unwrap()
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
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("사용법: ais-cli mcp [--write | --manager NAME]")
    );
    let (code, _) = converse(&vault, &["--manager"], &[]);
    assert_eq!(code, 2);
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
    // 서버는 커밋한 뒤 같은 백그라운드 스레드에서 이어서 대기 기록 파일을 지운다. 서버가 살아 있는 동안 그것까지 기다린다.
    // 커밋이 보이자마자 입력을 닫으면 서버가 끝나면서 파일을 지우기 전에 스레드가 멈출 수 있다(느린 CI 러너에서 보였다).
    let record = vault.join(".git/aisurada-pending.json");
    let removed = (0..50).any(|_| {
        if !record.exists() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
        false
    });
    drop(child.stdin.take());
    let _ = child.wait_with_output();
    assert!(committed, "MCP 서버가 남은 기록을 커밋해야 한다");
    let message = git(&["log", "-1", "--format=%B"]);
    // `create_doc` 도구 표면의 호출이라 쓴 쪽은 도구(tool)로 남는다
    assert!(
        message.contains("Aisurada-Writer: tool") && message.contains("Aisurada-Reason: idle"),
        "{message}"
    );
    assert!(removed, "커밋한 뒤 대기 기록 파일을 지워야 한다");
    let _ = std::fs::remove_dir_all(&vault);
}

/// 승인 길 끝에서 끝까지: AI가 MCP 서버로 올린 변경은 대기에 머물고, 세션 표식이 달린 `ais-cli`는 승인하지 못하며,
/// 표식 없는 `ais-cli`가 승인해야 올린 세션의 기록으로 적용된다.
#[test]
fn an_ai_change_waits_until_a_human_runs_the_approval() {
    let vault = fixture("approve");
    let approvals = vault.join("../approvals-for-mcp-test-approve");
    let approvals_text = approvals.to_string_lossy().into_owned();
    let approvals_env = ("AISURADA_APPROVALS_DIR", approvals_text.as_str());

    // AI(세션 표식이 달린 MCP 서버)가 글로벌 스킬을 만든다. 글로벌 변경은 정책상 승인을 기다린다
    let (code, replies) = converse_with(
        &vault,
        &["--write"],
        &[
            approvals_env,
            ("AISURADA_SESSION_ID", "ai-sess-1"),
            ("AISURADA_SESSION_AGENT", "codex"),
        ],
        &[call(
            1,
            "save_resource",
            json!({"kind":"skill","scope":"global","name":"제안 스킬","description":"d","body":"본문"}),
        )],
    );
    assert_eq!(code, 0);
    let queued = tool_json(&replies[0]);
    assert_eq!(queued["queued"], true);
    assert_eq!(queued["status"], "pending");
    assert_eq!(queued["reasons"][0]["category"], "global-change");
    let change = queued["changeId"].as_str().unwrap().to_string();

    // 승인 전에는 목록에 아무것도 없고, 대기 목록에는 올린 세션이 남아 있다
    let (code, listed, _) = run_cli(
        &vault,
        &[approvals_env],
        &["list-resources", "scope=global"],
    );
    assert_eq!(code, 0);
    let listed: Value = serde_json::from_str(&listed).unwrap();
    assert_eq!(listed["resources"].as_array().unwrap().len(), 0);
    assert_eq!(listed["approvals"]["pending"], 1);
    let (_, pending, _) = run_cli(&vault, &[approvals_env], &["get-resource-change", &change]);
    let pending: Value = serde_json::from_str(&pending).unwrap();
    assert_eq!(pending["status"], "pending");
    assert_eq!(pending["proposer"]["session"], "ai-sess-1");
    assert_eq!(pending["proposer"]["agent"], "codex");

    // 세션 표식이 달린 `ais-cli`는 어느 에이전트의 것이든 승인하지 못한다(AIS-403, 종료 코드 3)
    for markers in [
        vec![
            ("AISURADA_SESSION_ID", "ai-sess-2"),
            ("AISURADA_SESSION_AGENT", "codex"),
        ],
        vec![("CLAUDE_CODE_SESSION_ID", "claude-sess-1")],
        vec![("CODEX_THREAD_ID", "thread-1")],
        vec![
            ("CLAUDE_CODE_SESSION_ID", "claude-sess-1"),
            ("CODEX_THREAD_ID", "thread-1"),
        ],
    ] {
        let mut env = vec![approvals_env];
        env.extend(markers.iter().copied());
        let (code, stdout, stderr) = run_cli(&vault, &env, &["approve-resource-change", &change]);
        assert_eq!(code, 3, "{markers:?}: {stderr}");
        assert!(stderr.starts_with("AIS-403"), "{markers:?}: {stderr}");
        assert!(stdout.is_empty());
    }
    let (_, still, _) = run_cli(&vault, &[approvals_env], &["get-resource-change", &change]);
    let still: Value = serde_json::from_str(&still).unwrap();
    assert_eq!(
        still["status"], "pending",
        "거절된 승인은 아무것도 바꾸지 않는다"
    );

    // 표식 없는 `ais-cli`(사람)가 승인하면 올린 세션의 기록으로 적용된다
    let (code, approved, stderr) = run_cli(
        &vault,
        &[approvals_env],
        &["approve-resource-change", &change],
    );
    assert_eq!(code, 0, "{stderr}");
    let approved: Value = serde_json::from_str(&approved).unwrap();
    assert_eq!(approved["approved"], true);
    assert_eq!(approved["result"]["created"], true);
    assert_eq!(approved["result"]["editedSession"], "ai-sess-1");
    assert_eq!(approved["result"]["editedAgent"], "codex");
    let path = approved["result"]["path"].as_str().unwrap();
    let file = std::fs::read_to_string(vault.join(path)).unwrap();
    assert!(file.contains("editedSession: ai-sess-1\n"), "{file}");
    assert!(file.contains("editedAgent: codex\n"), "{file}");

    let (_, listed, _) = run_cli(
        &vault,
        &[approvals_env],
        &["list-resources", "scope=global"],
    );
    let listed: Value = serde_json::from_str(&listed).unwrap();
    assert_eq!(listed["resources"][0]["name"], "제안 스킬");
    assert_eq!(listed["approvals"]["pending"], 0);

    // 이미 처리한 변경은 다시 승인할 수 없다
    let (code, _, stderr) = run_cli(
        &vault,
        &[approvals_env],
        &["approve-resource-change", &change],
    );
    assert_eq!(code, 3);
    assert!(stderr.starts_with("AIS-409"), "{stderr}");
    let _ = std::fs::remove_dir_all(&vault);
    let _ = std::fs::remove_dir_all(&approvals);
}

/// 서버 프로세스가 환경 변수로 받은 세션이 노트 머리의 `sessionId`와 `editedSessionId`에 남는다. 에이전트 이름은 claude가 아닐 때만 적힌다.
#[test]
fn the_server_stamps_the_session_it_was_started_under() {
    let vault = fixture("stamp");
    let create = |id: u32, title: &str| {
        call(
            id,
            "create_doc",
            json!({"agenda":"demo","type":"plan","title":title,"body":"# 본문\n"}),
        )
    };

    // 래퍼가 넣는 짝: 만들 때 `sessionId`와 `sessionAgent`, 고칠 때 `editedSessionId`와 `editedSessionAgent`
    let (code, replies) = converse_with(
        &vault,
        &["--write"],
        &[
            ("AISURADA_SESSION_ID", "sess-pinned"),
            ("AISURADA_SESSION_AGENT", "gemini"),
        ],
        &[create(1, "짝으로 만든 노트")],
    );
    assert_eq!(code, 0);
    let made = tool_json(&replies[0]);
    let id = made["id"].as_str().unwrap().to_string();
    let front = note_text(&vault, &id);
    assert!(front.contains("sessionId: sess-pinned\n"), "{front}");
    assert!(front.contains("sessionAgent: gemini\n"), "{front}");

    let (_, replies) = converse_with(
        &vault,
        &["--write"],
        &[
            ("AISURADA_SESSION_ID", "sess-editor"),
            ("AISURADA_SESSION_AGENT", "gemini"),
        ],
        &[call(
            1,
            "update_doc",
            json!({"id": id, "title": "고친 제목", "f1": made["precondition"]}),
        )],
    );
    tool_json(&replies[0]);
    let front = note_text(&vault, &id);
    assert!(front.contains("editedSessionId: sess-editor\n"), "{front}");
    assert!(front.contains("editedSessionAgent: gemini\n"), "{front}");
    assert!(
        front.contains("sessionId: sess-pinned\n"),
        "만든 세션은 그대로다"
    );

    // Claude Code의 변수만 보이면 그 세션이 적히고, claude는 에이전트 줄을 적지 않는다
    let (_, replies) = converse_with(
        &vault,
        &["--write"],
        &[("CLAUDE_CODE_SESSION_ID", "claude-sess")],
        &[create(1, "클로드가 만든 노트")],
    );
    let front = note_text(&vault, tool_json(&replies[0])["id"].as_str().unwrap());
    assert!(front.contains("sessionId: claude-sess\n"), "{front}");
    assert!(!front.contains("sessionAgent:"), "{front}");

    // 서로 다른 에이전트의 변수가 함께 보이면 어느 쪽인지 몰라 아무 세션도 적지 않는다
    let (_, replies) = converse_with(
        &vault,
        &["--write"],
        &[
            ("CLAUDE_CODE_SESSION_ID", "claude-sess"),
            ("CODEX_SESSION_ID", "codex-sess"),
        ],
        &[create(1, "모호한 노트")],
    );
    let front = note_text(&vault, tool_json(&replies[0])["id"].as_str().unwrap());
    assert!(!front.contains("sessionId:"), "{front}");
    let _ = std::fs::remove_dir_all(&vault);
}

/// Codex는 도구 호출마다 `_meta`에 세션 id를 싣는다. 클라이언트 이름이 codex일 때만 믿고, `sessionId`가 없으면 `threadId`를 쓴다.
#[test]
fn a_codex_client_session_in_meta_is_stamped_and_other_clients_are_not_trusted() {
    let vault = fixture("meta");
    let create = |id: u32, title: &str, meta: Value| {
        let mut message = call(
            id,
            "create_doc",
            json!({"agenda":"demo","type":"plan","title":title,"body":"# 본문\n"}),
        );
        if !meta.is_null() {
            message["params"]["_meta"] = meta;
        }
        message
    };
    let initialize = |client: &str| json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"2025-06-18","clientInfo":{"name":client}}});
    let stamped = |reply: &Value| note_text(&vault, tool_json(reply)["id"].as_str().unwrap());

    let (code, replies) = converse(
        &vault,
        &["--write"],
        &[
            initialize("codex-mcp-client"),
            create(1, "세션 id로 만든 노트", json!({"sessionId": "sess-meta"})),
            create(
                2,
                "스레드 id로 만든 노트",
                json!({"threadId": "thread-meta"}),
            ),
            create(3, "표식 없이 만든 노트", Value::Null),
        ],
    );
    assert_eq!(code, 0);
    let front = stamped(&replies[1]);
    assert!(front.contains("sessionId: sess-meta\n"), "{front}");
    assert!(front.contains("sessionAgent: codex\n"), "{front}");
    let front = stamped(&replies[2]);
    assert!(front.contains("sessionId: thread-meta\n"), "{front}");
    assert!(front.contains("sessionAgent: codex\n"), "{front}");
    assert!(
        !stamped(&replies[3]).contains("sessionId:"),
        "`_meta`가 없으면 세션을 적지 않는다"
    );

    // codex가 아닌 클라이언트가 보낸 `_meta`는 믿지 않는다
    let (_, replies) = converse(
        &vault,
        &["--write"],
        &[
            initialize("some-other-client"),
            create(
                1,
                "다른 클라이언트의 노트",
                json!({"sessionId": "sess-fake"}),
            ),
        ],
    );
    let front = stamped(&replies[1]);
    assert!(!front.contains("sessionId:"), "{front}");
    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn manager_stays_in_its_registered_vault_and_project() {
    let vault = fixture("manager-project");
    let other_vault = fixture("manager-other-vault");
    let project_file = vault.join(".aisurada/projects/demo-project.md");
    let project = r#"---
id: pjfx000a-000000f1
slug: demo-project
title: 첫 프로젝트
repos: [{"name":"shared","github":"demo/shared"}]
---
"#;
    std::fs::write(&project_file, project).unwrap();
    std::fs::write(
        vault.join(".aisurada/projects/other-project.md"),
        project
            .replace("pjfx000a-000000f1", "pjfx000b-000000f2")
            .replace("demo-project", "other-project"),
    )
    .unwrap();
    std::fs::write(vault.join(".aisurada/agendas/other.md"),
        "---\nid: agfx000a-0000001f\nslug: other\ntitle: 다른 프로젝트\nproject: other-project\n---\n").unwrap();

    let cli = |args: &[&str]| {
        let (code, stdout, stderr) = run_cli(&vault, &[], args);
        assert_eq!(code, 0, "{stderr}");
        serde_json::from_str::<Value>(&stdout).unwrap()
    };
    let outside = cli(&[
        "create-task",
        "repo=demo/shared",
        "project=other-project",
        "title=다른 태스크",
        r#"criteria=["확인"]"#,
    ]);
    let outside_doc = cli(&[
        "create-doc",
        "agenda=other",
        "type=plan",
        "title=scopecheck 외부",
        "body=다른 본문",
    ]);
    let outside_key = outside["key"].as_str().unwrap();
    let outside_id = outside_doc["id"].as_str().unwrap();

    assert_ne!(
        run_cli(
            &vault,
            &[],
            &[
                "manager",
                "add",
                "bad",
                "project=demo-project",
                "repos=demo/no"
            ]
        )
        .0,
        0
    );
    assert_ne!(
        run_cli(
            &vault,
            &[],
            &[
                "manager",
                "add",
                "bad",
                "project=demo-project",
                "agendas=other"
            ]
        )
        .0,
        0
    );
    cli(&[
        "manager",
        "add",
        "hermes-demo",
        "project=demo-project",
        "repos=demo/shared",
        "agendas=demo",
        "tools=create_task,add_task_comment,create_doc,list_tasks,get_task,list_docs,search_docs,get_doc,list_tags,list_sub_notes",
        "slackChannels=#demo",
        "webhook=http://127.0.0.1:8644/webhooks/demo-project",
        "notifyEnabled=true",
    ]);
    let copy = cli(&[
        "manager",
        "copy",
        "hermes-demo",
        "hermes-other",
        "project=other-project",
    ]);
    assert_eq!(copy["repos"], json!([]));
    assert_eq!(copy["agendas"], json!([]));
    assert_eq!(copy["slackChannels"], json!([]));
    assert_eq!(copy["enabled"], false);
    assert_eq!(copy["notifyEnabled"], true);
    assert_eq!(
        copy["webhook"],
        "http://127.0.0.1:8644/webhooks/other-project"
    );

    std::fs::write(
        vault.join(".aisurada/agendas/other-duplicate.md"),
        "---\nid: agfx000b-0000002f\nslug: other\ntitle: 겹치는 아젠다\nproject: other-project\n---\n",
    ).unwrap();
    let switch = [("AISURADA_VAULT", other_vault.to_str().unwrap())];
    let run = |messages: &[Value]| {
        let (code, replies) =
            converse_with(&vault, &["--manager", "hermes-demo"], &switch, messages);
        assert_eq!(code, 0);
        replies
    };
    let replies = run(&[
        call(
            1,
            "create_task",
            json!({"repo":"demo/shared","title":"scopecheck 내부","criteria":["확인"],"body":"기록에서 빼야 할 본문"}),
        ),
        call(
            2,
            "create_task",
            json!({"project":"other-project","repo":"demo/shared","title":"다른 범위","criteria":["확인"]}),
        ),
        call(3, "list_tasks", json!({"bodies":true})),
        call(4, "get_task", json!({"id":outside_key})),
        call(
            5,
            "add_task_comment",
            json!({"id":outside_key,"body":"다른 범위 댓글"}),
        ),
        call(
            6,
            "create_doc",
            json!({"agenda":"demo","type":"plan","title":"scopecheck 내부 노트","body":"기록에서 빼야 할 노트 본문"}),
        ),
        call(
            7,
            "create_doc",
            json!({"agenda":"other","type":"plan","title":"외부 노트","body":"다른 범위"}),
        ),
        call(8, "list_docs", json!({})),
        call(9, "search_docs", json!({"query":"scopecheck"})),
        call(10, "get_doc", json!({"id":outside_id})),
        call(11, "list_sub_notes", json!({"parent":outside_id})),
        call(12, "list_tasks", json!({"trash":true})),
    ]);
    let task = tool_json(&replies[0]);
    assert_eq!(task["project"], "demo-project");
    assert_eq!(task["status"], "draft");
    assert_eq!(tool_json(&replies[2])["tasks"].as_array().unwrap().len(), 1);
    let doc = tool_json(&replies[5]);
    assert_eq!(doc["agenda"], "demo");
    let listed = tool_json(&replies[7]);
    assert!(!listed.as_array().unwrap().is_empty());
    assert!(
        listed
            .as_array()
            .unwrap()
            .iter()
            .all(|note| note["agenda"] == "demo")
    );
    let hits = tool_json(&replies[8]);
    assert_eq!(hits["total"], 1);
    assert_eq!(hits["docs"][0]["id"], doc["id"]);
    for i in [1, 3, 4, 6, 9, 10, 11] {
        assert_eq!(replies[i]["result"]["isError"], true, "{}", replies[i]);
    }
    let history = cli(&["manager", "dashboard"]);
    assert_eq!(history["items"][0]["work"], json!({"tasks":1,"comments":0}));
    let events = &history["items"][0]["history"];
    assert!(events["firstCall"].is_number());
    assert_eq!(events["events"].as_array().unwrap().len(), 12);
    assert!(!events.to_string().contains("기록에서 빼야 할"));
    let summary = cli(&["manager", "dashboard", "--compact", "--summary"]);
    let summary_history = &summary["items"][0]["history"];
    assert_eq!(summary_history["events"], json!([]));
    assert_eq!(summary_history["today"]["calls"], 12);
    assert_eq!(summary_history["latestCall"], events["events"][0]);
    assert_eq!(
        cli(&["manager", "history", "hermes-demo"])["events"],
        events["events"]
    );

    assert_eq!(
        events["events"].as_array().unwrap().last().unwrap()["target"],
        task["key"]
    );

    std::fs::write(
        &project_file,
        project.replace("demo/shared", "demo/replaced"),
    )
    .unwrap();
    let replies = run(&[call(
        1,
        "create_task",
        json!({"repo":"demo/shared","title":"이전 저장소","criteria":["확인"]}),
    )]);
    assert_eq!(replies[0]["result"]["isError"], true);
    let registry = vault.join("Library/Application Support/aisurada/managers.json");
    let mut rows = cli(&["manager", "list"]).as_array().unwrap().clone();
    rows.push(json!({
        "name": "legacy", "label": "Hermes", "enabled": true,
        "tools": ["list_tasks"], "repos": ["demo/shared"], "agendas": [],
        "webhook": "http://127.0.0.1:8644/webhooks/demo-project",
    }));
    let broken = json!({"name":"broken","tools":42});
    rows.push(broken.clone());
    std::fs::write(&registry, serde_json::to_vec(&rows).unwrap()).unwrap();
    assert!(cli(&["manager", "dashboard", "--compact"])["registryIssue"].is_string());
    assert!(
        !cli(&["manager", "list"])
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["name"] == "broken")
    );
    let (code, _) = converse(&vault, &["--manager", "legacy"], &[]);
    assert_eq!(
        code, 2,
        "프로젝트를 지정하기 전에는 기존 등록으로 연결할 수 없다"
    );
    let migrated = cli(&["manager", "update", "legacy", "project=other-project"]);
    assert_eq!(migrated["project"], "other-project");
    assert_eq!(migrated["legacyKey"], true, "기존 키체인 계정을 이어 쓴다");
    let persisted: Value = serde_json::from_slice(&std::fs::read(&registry).unwrap()).unwrap();
    assert!(persisted.as_array().unwrap().contains(&broken));
    let (code, _) = converse(
        &vault,
        &["--manager", "legacy"],
        &[call(1, "list_tasks", json!({}))],
    );
    assert_eq!(code, 0);

    std::fs::remove_file(&project_file).unwrap();
    let rebound = cli(&[
        "manager",
        "update",
        "hermes-demo",
        "project=other-project",
        "repos=demo/shared",
        "agendas=[]",
    ]);
    assert_eq!(rebound["project"], "other-project");
    assert_eq!(rebound["enabled"], false);
    assert_eq!(rebound["notifyEnabled"], false);

    std::fs::remove_dir_all(vault).unwrap();
    std::fs::remove_dir_all(other_vault).unwrap();
}

#[test]
fn manager_direct_intake_survives_retries_without_slack_channels() {
    let vault = fixture("direct-intake");
    std::fs::write(
        vault.join(".aisurada/projects/intake.md"),
        "---\nid: pjfx100a-000000f1\nslug: intake\ntitle: intake\nrepos: [{\"name\":\"shared\",\"github\":\"demo/shared\"}]\n---\n",
    ).unwrap();

    let cli = |args: &[&str]| {
        let (status, stdout, stderr) = run_cli(&vault, &[], args);
        assert_eq!(status, 0, "{stderr}");
        serde_json::from_str::<Value>(&stdout).unwrap()
    };
    let name = "hermes-direct";
    for manager in [name, "other-direct"] {
        cli(&[
            "manager",
            "add",
            manager,
            "project=intake",
            "repos=demo/shared",
        ]);
    }
    let tool = |manager, action, args| {
        let (status, rows) = converse(&vault, &["--manager", manager], &[call(1, action, args)]);
        assert_eq!(status, 0);
        rows.into_iter().next().unwrap()
    };
    let mcp = |action, args| tool_json(&tool(name, action, args));
    let report = |kind, key| {
        json!({
            "source": {"kind":kind, "key":key},
            "reporter":"텔레그램 사용자",
            "excerpt":"로그인 화면이 멈춰요"
        })
    };

    assert_eq!(
        tool(name, "submit_intake", report("slack", "bot:chat:1"))["result"]["isError"],
        true
    );
    let mut payload = report("telegram", "bot:chat:1");
    payload["source"]["postedAt"] = json!(1_700_000_000_000_i64);
    payload["source"]["url"] = json!("https://t.me/demo/1");
    payload["source"]["destination"] =
        json!({"conversation":"-10012345","message":"42","thread":"11"});
    for url in ["javascript:alert(1)", "https://person@t.me/demo/1"] {
        let mut unsafe_link = payload.clone();
        unsafe_link["source"]["url"] = json!(url);
        assert_eq!(
            tool(name, "submit_intake", unsafe_link)["result"]["isError"],
            true
        );
    }
    let received = mcp("submit_intake", payload.clone());
    assert_eq!(received["recorded"], false);
    assert_eq!(received["item"]["current"]["result"], "pending");
    let receipt = &received["item"]["entry"]["id"];
    let lookup = json!({"entryId":receipt});
    assert_eq!(mcp("get_intake", lookup.clone())["item"], received["item"]);
    let queues = mcp("get_intake", json!({}));
    assert_eq!(queues["pending"], json!([]));
    assert_eq!(queues["directPending"][0], received["item"]);
    assert_eq!(
        tool_json(&tool("other-direct", "get_intake", lookup))["recorded"],
        false
    );
    assert_eq!(
        mcp("submit_intake", payload.clone())["item"],
        received["item"]
    );

    let complete = json!({
        "entryId":receipt,
        "priorRevision":received["item"]["current"]["revision"],
        "outcome":{"result":"created", "task":{"title":"로그인 화면이 멈춤", "repo":"demo/shared", "criteria":["로그인 확인"]}}
    });
    assert_eq!(
        tool("other-direct", "complete_intake", complete.clone())["result"]["isError"],
        true
    );
    cli(&["manager", "update", "other-direct", "tools=list_tasks"]);
    let other_received = tool_json(&tool(
        "other-direct",
        "submit_intake",
        report("desktop", "client:chat:1"),
    ));
    let mut read_only = complete.clone();
    read_only["entryId"] = other_received["item"]["entry"]["id"].clone();
    read_only["priorRevision"] = other_received["item"]["current"]["revision"].clone();
    let rejected = tool("other-direct", "complete_intake", read_only);
    assert_eq!(rejected["result"]["isError"], true);
    assert!(text(&rejected).contains("허용하지 않은 태스크 작업"));
    let mut stale = complete.clone();
    stale["priorRevision"] = json!("stale");
    assert_eq!(
        tool(name, "complete_intake", stale)["result"]["isError"],
        true
    );
    let mut denied = complete.clone();
    denied["outcome"]["task"]["repo"] = json!("outside/repo");
    assert_eq!(
        tool(name, "complete_intake", denied)["result"]["isError"],
        true
    );

    let ledger = vault.join(".aisurada/intake/ledger.json");
    let checkpoint = |receipt: &Value, work| {
        let mut snapshot: Value = serde_json::from_slice(&std::fs::read(&ledger).unwrap()).unwrap();
        let entries = snapshot["entries"].as_object_mut().unwrap();
        let entry = entries
            .values_mut()
            .find(|entry| entry["id"] == *receipt)
            .unwrap();
        entry["work"] = work;
        serde_json::to_vec(&snapshot).unwrap()
    };
    let pending = checkpoint(receipt, json!({"result":"created", "taskId":null}));
    let resolved = mcp("complete_intake", complete.clone());
    let task_key = resolved["item"]["entry"]["taskKey"].as_str().unwrap();

    // 태스크 저장 직후 원장 결과 쓰기가 실패한 상태에서도 같은 태스크를 연결한다.
    std::fs::write(&ledger, pending).unwrap();
    let mut changed = complete.clone();
    changed["outcome"] = json!({"result":"ignored"});
    assert_eq!(
        tool(name, "complete_intake", changed)["result"]["isError"],
        true
    );
    assert_eq!(
        mcp("complete_intake", complete.clone())["item"]["entry"]["taskKey"],
        task_key
    );
    assert_eq!(mcp("complete_intake", complete)["recorded"], true);
    assert_eq!(
        mcp("submit_intake", payload)["item"]["entry"]["taskKey"],
        task_key
    );

    let repeat = mcp("submit_intake", report("telegram", "bot:chat:2"));
    let comment = json!({
        "entryId":repeat["item"]["entry"]["id"],
        "priorRevision":repeat["item"]["current"]["revision"],
        "outcome":{"result":"commented", "taskKey":task_key}
    });
    let pending = checkpoint(
        &repeat["item"]["entry"]["id"],
        json!({"result":"commented", "taskId":resolved["item"]["entry"]["taskId"]}),
    );
    assert_eq!(
        mcp("complete_intake", comment.clone())["item"]["current"]["result"],
        "commented"
    );
    std::fs::write(&ledger, pending).unwrap();
    assert_eq!(
        mcp("complete_intake", comment)["item"]["entry"]["taskKey"],
        task_key
    );
    let task = cli(&["get-task", task_key]);
    let sources = task["intakeSources"].as_array().unwrap();
    assert_eq!(sources.len(), 2);
    assert_eq!(sources[0]["id"], *receipt);
    assert_eq!(sources[0]["manager"], name);
    assert_eq!(sources[0]["taskOrigin"], "created");
    assert_eq!(sources[0]["source"]["postedAt"], 1_700_000_000_000_i64);
    assert_eq!(sources[0]["url"], "https://t.me/demo/1");
    assert_eq!(sources[0]["destination"]["conversation"], "-10012345");
    assert_eq!(sources[0]["destination"]["thread"], "11");
    assert_eq!(sources[0]["reporter"], "텔레그램 사용자");
    assert_eq!(sources[0]["excerpt"], "로그인 화면이 멈춰요");
    assert_eq!(sources[1]["taskOrigin"], "commented");
    assert_eq!(sources[1]["url"], "");
    assert!(sources[1]["receivedAt"].as_i64().is_some());
    let agent_env = [
        ("GEMINI_SESSION_ID", "test-gemini"),
        ("CODEX_THREAD_ID", "test-codex"),
    ];
    let (code, detail, stderr) = run_cli(&vault, &agent_env, &["get-task", task_key]);
    assert_eq!(code, 0, "{stderr}");
    let detail: Value = serde_json::from_str(&detail).unwrap();
    assert_eq!(detail["key"], task_key);
    assert!(detail.get("intakeSources").is_none());
    for command in [
        &["manager", "intake-sources", task_key][..],
        &["manager", "intake-review", name, "{}"][..],
        &["manager", "update", name, "tools=create_task"][..],
    ] {
        let (code, stdout, stderr) = run_cli(&vault, &agent_env, command);
        assert_eq!(code, 3);
        assert!(stdout.is_empty());
        assert!(stderr.contains("로컬 작업자"), "{stderr}");
    }
    assert_eq!(
        task["activity"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["kind"] == "comment")
            .count(),
        1
    );

    let desktop = cli(&["manager", "intake", name]);
    assert_eq!(desktop["channels"], json!([]));
    assert_eq!(desktop["counts"], json!([2, 1, 1, 0, 0]));
    assert_eq!(desktop["rows"].as_array().unwrap().len(), 2);
    assert_eq!(desktop["rows"][0]["entry"]["source"]["kind"], "telegram");
    let dashboard = cli(&["manager", "dashboard"]);
    let item = dashboard["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["manager"]["name"] == name)
        .unwrap();
    assert_eq!(item["intake"]["counts"], desktop["counts"]);
    assert_eq!(item["work"], json!({"tasks":1,"comments":1}));
    // 태스크가 원래 상태로 돌아와도 두 변경은 각각 보관된다.
    cli(&["transition-task", task_key, "to=ready-for-develop"]);
    cli(&["transition-task", task_key, "to=draft"]);
    let status_tasks = cli(&["manager", "status-tasks"]);
    let changes = status_tasks["tasks"][0]["statusChanges"]
        .as_array()
        .unwrap();
    assert_eq!(changes.len(), 2);
    assert_ne!(changes[0]["id"], changes[1]["id"]);
    assert_eq!(changes[0]["status"], "ready-for-develop");
    assert_eq!(changes[1]["status"], "draft");
    assert_eq!(cli(&["manager", "status-tasks"]), status_tasks);
    let configured = cli(&[
        "manager",
        "update",
        name,
        "telegramWebhook=http://127.0.0.1:8644/webhooks/demo-telegram",
        "telegramChats=[\"-10012345\"]",
        "notifyEnabled=true",
    ]);
    assert_eq!(configured["telegramChats"], json!(["-10012345"]));
    let summary = cli(&["manager", "dashboard", "--summary"]);
    let item = summary["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["manager"]["name"] == name)
        .unwrap();
    assert!(item["work"].is_null());
    assert_eq!(item["intake"]["counts"], desktop["counts"]);
    let tasks = vault.join(".aisurada/tasks");
    assert_eq!(
        std::fs::read_dir(tasks)
            .unwrap()
            .filter(|entry| entry
                .as_ref()
                .unwrap()
                .path()
                .extension()
                .is_some_and(|ext| ext == "md"))
            .count(),
        1
    );
    // 상세 활동의 500건을 넘겨도 오늘 저장한 매니저 댓글은 집계에 남는다.
    let activity = vault
        .join(".aisurada/tasks")
        .join(task_key)
        .join("activity.jsonl");
    let mut lines = std::fs::read_to_string(&activity).unwrap();
    lines.push_str(
        "{\"at\":\"2020-01-01T00:00:00Z\",\"kind\":\"edited\",\"by\":\"human\"}\n"
            .repeat(500)
            .as_str(),
    );
    std::fs::write(activity, lines).unwrap();
    mcp(
        "add_task_comment",
        json!({"id":task_key,"body":"매니저가 직접 남긴 댓글"}),
    );
    cli(&["add-task-comment", task_key, "body=사람이 남긴 댓글"]);
    assert_eq!(
        cli(&["get-task", task_key, "activity=0"])["activity"]
            .as_array()
            .unwrap()
            .len(),
        500
    );
    cli(&["delete-task", task_key]);
    let dashboard = cli(&["manager", "dashboard"]);
    let item = dashboard["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["manager"]["name"] == name)
        .unwrap();
    assert_eq!(item["work"], json!({"tasks":1,"comments":2}));
    assert_eq!(item["intake"]["counts"], desktop["counts"]);

    let waiting = mcp("submit_intake", report("telegram", "bot:chat:3"));
    let entry = &waiting["item"]["entry"]["id"];
    let asked = mcp(
        "complete_intake",
        json!({
            "entryId":entry,
            "priorRevision":waiting["item"]["current"]["revision"],
            "outcome":{"result":"asked","question":"어느 화면에서 멈췄나요?"}
        }),
    );
    // 화면과 호스트가 실행되지 않은 동안 오래 남은 질문을 다시 읽는다.
    let mut saved: Value = serde_json::from_slice(&std::fs::read(&ledger).unwrap()).unwrap();
    let record = saved["entries"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .find(|row| row["id"] == *entry)
        .unwrap();
    let event = record["history"]
        .as_array_mut()
        .unwrap()
        .last_mut()
        .unwrap();
    let older = event["at"].as_i64().unwrap() - 2 * 24 * 60 * 60 * 1000;
    event["at"] = json!(older);
    std::fs::write(&ledger, serde_json::to_vec(&saved).unwrap()).unwrap();
    let questions = cli(&["manager", "intake-questions"]);
    assert_eq!(questions.as_array().unwrap().len(), 1);
    assert_eq!(questions[0]["manager"], name);
    assert_eq!(questions[0]["entry"], *entry);
    assert_eq!(questions[0]["at"], older);
    assert_eq!(questions[0]["question"], "어느 화면에서 멈췄나요?");
    assert!(questions[0]["url"].is_null());
    assert_eq!(cli(&["manager", "intake-questions"]), questions);
    cli(&["manager", "disable", name]);
    assert_eq!(cli(&["manager", "intake-questions"]), json!([]));
    cli(&["manager", "enable", name]);
    assert_eq!(cli(&["manager", "intake-questions"]), questions);
    cli(&[
        "manager",
        "intake-review",
        name,
        &json!({
            "id": entry, "revision": asked["item"]["current"]["revision"], "result":"ignored"
        })
        .to_string(),
    ]);
    assert_eq!(cli(&["manager", "intake-questions"]), json!([]));
    std::fs::remove_dir_all(vault).unwrap();
}

#[test]
fn manager_escalates_third_question_and_expired_intake() {
    use std::cell::Cell;

    let vault = fixture("manager-intake-attention");
    std::fs::write(
        vault.join(".aisurada/projects/intake.md"),
        "---\nid: pjfx100a-000000f1\nslug: intake\ntitle: Intake\nrepos: [{\"name\":\"shared\",\"github\":\"demo/shared\"}]\n---\n",
    ).unwrap();
    let start: i64 = 1_700_000_010_000;
    let now = Cell::new(start);
    let name = "hermes-intake";
    let channel = "CDEMO123";
    let cli = |args: &[&str]| {
        let time = now.get().to_string();
        let (status, output, error) = run_cli(&vault, &[("AISURADA_TEST_NOW", &time)], args);
        assert_eq!(status, 0, "{error}");
        serde_json::from_str::<Value>(&output).unwrap()
    };
    let mcp = |tool: &str, args: Value| {
        let time = now.get().to_string();
        let (status, replies) = converse_with(
            &vault,
            &["--manager", name],
            &[("AISURADA_TEST_NOW", &time)],
            &[call(1, tool, args)],
        );
        assert_eq!(status, 0);
        tool_json(&replies[0])
    };
    cli(&[
        "manager",
        "add",
        name,
        "project=intake",
        "repos=demo/shared",
    ]);
    let settings = json!({"channel":channel,"visibility":"public","label":"가상 채널",
        "emoji":"ticket","markProcessed":true,"paused":false,"intervalMinutes":15});
    cli(&["manager", "intake-channel", name, &settings.to_string()]);

    let mut request = json!({
        "channel":channel, "messageTs":"1700000000.000001",
        "url":"https://demo.slack.com/archives/CDEMO123/p1700000000000001",
        "reporter":"가상 제보자", "excerpt":"로그인 오류",
        "result":"asked", "question":"재현 화면을 알려 주세요", "via":"desktop",
    });
    let first = mcp("record_intake", request.clone());
    request["priorRevision"] = first["item"]["current"]["revision"].clone();
    request["replyTs"] = json!("1700000001.000001");
    let second = mcp("record_intake", request.clone());
    assert_eq!(second["item"]["current"]["result"], "asked");
    request["priorRevision"] = second["item"]["current"]["revision"].clone();
    request["replyTs"] = json!("1700000002.000001");
    let third = mcp("record_intake", request.clone());
    assert_eq!(third["questionAccepted"], false);
    assert_eq!(third["item"]["attention"]["reason"], "question_limit");
    let notices = cli(&["manager", "intake-questions"]);
    assert_eq!(notices.as_array().unwrap().len(), 1);
    assert_eq!(notices[0]["reviewReason"], "question_limit");
    assert!(notices[0]["question"].is_null());
    assert_eq!(
        third["item"]["entry"]["history"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["result"] == "asked")
            .count(),
        2
    );
    let review = |payload: Value| cli(&["manager", "intake-review", name, &payload.to_string()]);
    let ignored = review(json!({"id":third["item"]["entry"]["id"],
        "revision":third["item"]["current"]["revision"],"result":"ignored"}));
    assert_eq!(ignored["current"]["result"], "ignored");
    assert!(ignored.get("attention").is_none());

    let submitted = mcp(
        "submit_intake",
        json!({"source":{"kind":"telegram","key":"bot/chat/report"},
        "reporter":"가상 제보자","excerpt":"다른 오류"}),
    );
    let asked = mcp(
        "complete_intake",
        json!({"entryId":submitted["item"]["entry"]["id"],
        "priorRevision":submitted["item"]["current"]["revision"],
        "outcome":{"result":"asked","question":"어떤 화면인가요?"}}),
    );
    let deadline = start + 3 * 24 * 60 * 60 * 1000;
    now.set(deadline - 1);
    let before = cli(&["manager", "intake", name]);
    assert_eq!(before["waiting"], 1);
    assert_eq!(before["attention"], json!([]));
    now.set(deadline);
    let expired = cli(&["manager", "intake", name]);
    assert_eq!(expired["waiting"], 0);
    assert_eq!(expired["attention"][0]["reason"], "no_answer");
    assert_eq!(expired["attention"][0]["at"], deadline);
    let notices = cli(&["manager", "intake-questions"]);
    assert_eq!(notices.as_array().unwrap().len(), 1);
    assert_eq!(notices[0]["reviewReason"], "no_answer");
    assert_eq!(notices[0]["at"], deadline);
    assert_ne!(before["rowsRevision"], expired["rowsRevision"]);
    let payload = json!({"id":asked["item"]["entry"]["id"],"revision":asked["item"]["current"]["revision"],
        "result":"created","task":{"title":"오류 수정","repo":"demo/shared","criteria":["오류가 사라진다"]}});
    let accepted = review(payload.clone());
    assert_eq!(accepted["current"]["result"], "created");
    assert!(accepted.get("attention").is_none());
    let key = accepted["entry"]["taskKey"].as_str().unwrap();
    assert_eq!(review(payload)["entry"]["taskKey"], key);
    assert_eq!(cli(&["get-task", key])["status"], "draft");
    assert_eq!(
        cli(&["manager", "intake-sources", key])[0]["id"],
        asked["item"]["entry"]["id"]
    );
    std::fs::remove_dir_all(vault).unwrap();
}

#[test]
fn manager_intake_keeps_one_record_and_resumes_questions_after_rewind() {
    use std::time::{SystemTime, UNIX_EPOCH};

    let vault = fixture("intake-ledger");
    let projects = vault.join(".aisurada/projects");
    for (slug, id) in [
        ("intake", "pjfx100a-000000f1"),
        ("outside", "pjfx100b-000000f2"),
    ] {
        std::fs::write(
            projects.join(format!("{slug}.md")),
            format!("---\nid: {id}\nslug: {slug}\ntitle: {slug}\nrepos: [{{\"name\":\"shared\",\"github\":\"demo/shared\"}}]\n---\n"),
        ).unwrap();
    }

    let cli = |args: &[&str]| {
        let (status, stdout, stderr) = run_cli(&vault, &[], args);
        assert_eq!(status, 0, "{stderr}");
        serde_json::from_str::<Value>(&stdout).unwrap()
    };
    let name = "hermes-intake";
    let channel = "CDEMO123";
    cli(&[
        "manager",
        "add",
        name,
        "project=intake",
        "repos=demo/shared",
    ]);
    cli(&[
        "manager",
        "add",
        "hermes-other",
        "project=outside",
        "repos=demo/shared",
        "tools=list_tasks",
    ]);
    let desktop =
        |action: &str, payload: Value| cli(&["manager", action, name, &payload.to_string()]);
    let configure = json!({"channel": channel, "label": "#제보", "emoji": "ticket", "intervalMinutes": 10,
        "markProcessed": true, "paused": false});
    let cursor = desktop("intake-channel", configure.clone());

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let at = |seconds: u64| format!("{:010}.000000", now - seconds);
    let first = at(30);
    let repeat = at(20);
    let reply = at(10);
    let start = at(60);
    let rewind = |revision: &Value| {
        desktop(
            "intake-rewind",
            json!({
                "channel": channel, "revision": revision, "messageTs": start
            }),
        )
    };
    let cursor = rewind(&cursor["revision"]);

    let tool = |manager: &str, name: &str, args: Value| {
        let (status, rows) = converse(&vault, &["--manager", manager], &[call(1, name, args)]);
        assert_eq!(status, 0);
        rows.into_iter().next().unwrap()
    };
    let own = |tool_name, args| tool(name, tool_name, args);
    let mcp = |tool_name, args| tool_json(&own(tool_name, args));
    let rejected = |response: Value| assert_eq!(response["result"]["isError"], true, "{response}");
    let move_to = |cursor: &Value, ts: &str, reports: &[&str]| {
        json!({
            "channel": channel, "revision": cursor["revision"], "messageTs": ts,
            "via": "scheduled", "reportTimestamps": reports
        })
    };
    let report = |ts: &str| {
        json!({
            "channel": channel, "messageTs": ts, "url": format!("https://demo.slack.com/archives/{channel}/p{}", ts.replace('.', "")),
            "reporter": "제보자", "excerpt": "가".repeat(210), "via": "scheduled"
        })
    };
    rejected(own("move_cursor", move_to(&cursor, &first, &[&first])));
    rejected(own("get_intake", json!({"channel":"CNOTMINE"})));

    let mut original = report(&first);
    original["result"] = json!("created");
    original["task"] = json!({"title":"원글 태스크", "repo":"demo/shared", "criteria":["확인"]});
    let created = mcp("record_intake", original.clone());
    let task_key = created["item"]["entry"]["taskKey"].as_str().unwrap();
    assert_eq!(created["recorded"], false);
    assert_eq!(
        created["item"]["entry"]["excerpt"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        200
    );
    assert_eq!(
        mcp("record_intake", original.clone())["item"],
        created["item"]
    );
    assert_eq!(
        mcp("list_tasks", json!({}))["tasks"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let task = mcp("get_task", json!({"id":task_key}));
    assert_eq!(
        (task["status"].as_str(), task["refs"][0]["target"].as_str()),
        (Some("draft"), original["url"].as_str())
    );

    let mut asked = report(&repeat);
    asked["result"] = json!("asked");
    asked["question"] = json!("어떤 화면인가요?");
    let question = mcp("record_intake", asked.clone());
    let queued = cli(&["manager", "intake-questions"]);
    let notice = &queued[0];
    let revision = question["item"]["current"]["revision"].as_str().unwrap();
    assert_eq!(notice["id"], format!("intake-question-{revision}"));
    assert_eq!(notice["entry"], question["item"]["entry"]["id"]);
    assert_eq!(notice["url"], asked["url"]);
    let cursor = mcp("move_cursor", move_to(&cursor, &repeat, &[&first, &repeat]));
    assert_eq!(
        mcp("get_intake", json!({}))["pending"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    rejected(own("move_cursor", move_to(&cursor, &first, &[])));

    let foreign = cli(&[
        "create-task",
        "repo=demo/shared",
        "project=outside",
        "title=다른 프로젝트",
        r#"criteria=["확인"]"#,
    ]);
    asked["result"] = json!("commented");
    asked["replyTs"] = json!(reply);
    asked["priorRevision"] = question["item"]["current"]["revision"].clone();
    asked["taskKey"] = foreign["key"].clone();
    let denied = own("record_intake", asked.clone());
    assert!(text(&denied).contains("태스크를 못 찾았다"));
    rejected(denied);
    asked["taskKey"] = json!(task_key);
    let commented = mcp("record_intake", asked.clone());
    assert_eq!(cli(&["manager", "intake-questions"]), json!([]));
    assert_eq!(
        commented["item"]["entry"]["history"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(mcp("record_intake", asked.clone())["recorded"], true);
    assert_eq!(
        mcp("get_task", json!({"id":task_key}))["activity"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["kind"] == "comment")
            .count(),
        1
    );
    assert_eq!(mcp("get_intake", json!({}))["waiting"], 0);

    let cursors = rewind(&cursor["revision"]);
    assert_eq!(cursors["cursor"], start);
    rejected(own("move_cursor", move_to(&cursor, &repeat, &[])));
    assert_eq!(
        mcp("get_intake", json!({"channel":channel,"messageTs":first}))["recorded"],
        true
    );
    assert_eq!(
        cli(&["manager", "intake-sources", task_key])
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let mut ack = original.clone();
    ack["priorRevision"] = created["item"]["current"]["revision"].clone();
    ack["reactionAdded"] = json!(true);
    assert_eq!(
        mcp("record_intake", ack)["item"]["entry"]["reactionAdded"],
        true
    );
    let reviewed = desktop(
        "intake-review",
        json!({"id": commented["item"]["entry"]["id"],
        "revision": commented["item"]["current"]["revision"], "result": "pending"}),
    );
    assert_eq!(reviewed["current"]["result"], "pending");

    cli(&[
        "manager",
        "intake-channel",
        "hermes-other",
        &configure.to_string(),
    ]);
    assert_eq!(
        tool_json(&tool(
            "hermes-other",
            "get_intake",
            json!({"channel":channel,"messageTs":first})
        )),
        json!({"recorded":false,"item":null})
    );
    rejected(tool("hermes-other", "record_intake", original.clone()));

    // 0.2.3의 원장 형식으로 바꿔 읽어도 작성한 매니저의 기록은 남는다.
    let ledger_path = vault.join(".aisurada/intake/ledger.json");
    let mut ledger: Value = serde_json::from_slice(&std::fs::read(&ledger_path).unwrap()).unwrap();
    let records = ledger["entries"].as_object_mut().unwrap();
    let previous = std::mem::take(records);
    for (_, entry) in previous {
        records.insert(entry["id"].as_str().unwrap().into(), entry);
    }
    std::fs::write(&ledger_path, serde_json::to_vec(&ledger).unwrap()).unwrap();
    assert_eq!(
        mcp("get_intake", json!({"channel":channel,"messageTs":first}))["item"]["entry"]["taskKey"],
        task_key
    );

    let shared = at(5);
    let mut ignored = report(&shared);
    ignored["result"] = json!("ignored");
    assert_eq!(mcp("record_intake", ignored)["recorded"], false);
    cli(&[
        "manager",
        "update",
        "hermes-other",
        "tools=list_tasks,create_task,add_task_comment",
    ]);
    let outsider = |name, args| tool("hermes-other", name, args);
    let other_channel = cli(&["manager", "intake", "hermes-other"]);
    let other_cursor = cli(&["manager", "intake-rewind", "hermes-other", &json!({
        "channel": channel, "revision": other_channel["channels"][0]["revision"], "messageTs": start
    }).to_string()]);
    assert_eq!(
        tool_json(&outsider(
            "get_intake",
            json!({"channel":channel,"messageTs":shared})
        ))["recorded"],
        false
    );
    rejected(outsider(
        "move_cursor",
        move_to(&other_cursor, &shared, &[&shared]),
    ));
    let mut report_from_other = report(&shared);
    report_from_other["result"] = json!("created");
    report_from_other["task"] =
        json!({"title":"다른 프로젝트 제보", "repo":"demo/shared", "criteria":["확인"]});
    let revision_before = other_channel["rowsRevision"].clone();
    let created_outside = tool_json(&outsider("record_intake", report_from_other));
    assert_eq!(created_outside["recorded"], false);
    let created_outside_key = created_outside["item"]["entry"]["taskKey"]
        .as_str()
        .unwrap();
    assert_eq!(
        cli(&["get-task", created_outside_key])["project"],
        "outside"
    );
    assert_eq!(
        mcp("get_intake", json!({"channel":channel,"messageTs":shared}))["item"]["current"]["result"],
        "ignored"
    );

    let duplicate = at(6);
    let mut comment_from_other = report(&duplicate);
    comment_from_other["result"] = json!("commented");
    comment_from_other["taskKey"] = foreign["key"].clone();
    assert_eq!(
        tool_json(&outsider("record_intake", comment_from_other))["recorded"],
        false
    );
    let other_after_record = cli(&["manager", "intake", "hermes-other"]);
    assert_ne!(other_after_record["rowsRevision"], revision_before);
    let foreign_key = foreign["key"].as_str().unwrap();
    assert_eq!(
        cli(&["get-task", foreign_key])["intakeSources"][0]["taskOrigin"],
        "commented"
    );
    assert!(
        mcp("get_task", json!({"id":task_key}))
            .get("intakeSources")
            .is_none()
    );
    assert_eq!(
        cli(&["get-task", task_key])["intakeSources"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        tool_json(&outsider(
            "move_cursor",
            move_to(&other_cursor, &shared, &[&duplicate, &shared])
        ))["cursor"],
        shared
    );
    let other_after_move = cli(&["manager", "intake", "hermes-other"]);
    assert_eq!(
        other_after_move["rowsRevision"],
        other_after_record["rowsRevision"]
    );

    let mut paused = configure;
    paused["paused"] = json!(true);
    let paused = desktop("intake-channel", paused);
    rejected(own("move_cursor", move_to(&paused, &repeat, &[])));
    let telegram = mcp(
        "submit_intake",
        json!({"source":{"kind":"telegram","key":"bot:chat:13"},"reporter":"텔레그램 제보자","excerpt":"같은 문제를 발견했어요"}),
    );
    mcp(
        "complete_intake",
        json!({
            "entryId": telegram["item"]["entry"]["id"],
            "priorRevision": telegram["item"]["current"]["revision"],
            "outcome":{"result":"commented","taskKey":task_key}
        }),
    );
    let sources = cli(&["manager", "intake-sources", task_key]);
    assert_eq!(sources.as_array().unwrap().len(), 3);
    assert_eq!(sources[0]["taskOrigin"], "created");
    assert_eq!(sources[0]["url"], original["url"]);
    assert_eq!(sources[2]["source"]["kind"], "telegram");
    assert_eq!(sources[2]["taskOrigin"], "commented");
    let dashboard = cli(&["manager", "dashboard"]);
    let item = dashboard["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["manager"]["name"] == name)
        .unwrap();
    assert_eq!(item["work"], json!({"tasks":1,"comments":2}));
    let raw = std::fs::read_to_string(vault.join(".aisurada/intake/ledger.json")).unwrap();
    assert!(!raw.contains(&"가".repeat(201)));
    std::fs::remove_dir_all(vault).unwrap();
}
