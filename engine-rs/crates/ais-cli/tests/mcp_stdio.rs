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
