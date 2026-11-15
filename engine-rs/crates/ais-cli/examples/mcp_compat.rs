//! Java MCP 서버와 Rust MCP 서버가 같은 JSON-RPC 대화에 같은 답을 내는지 한 줄씩 견준다.
//!
//! 사용: `cargo run -p ais-cli --example mcp_compat -- <기준 폴더(ais-cli.jar, ais-engine.jar)>`
//! 기준 폴더는 `engine-rs/tools/compat/build-oracle.sh`가 태그 `java-engine-final`에서 지어 `engine-rs/target/oracle/`에 둔다.
//!
//! 도구 호출은 두 서버 모두 엔진 대신 같은 가짜 호출기(`tools/compat/DriveMcp.java`)가 받는다. 그래서 여기서 보는 것은 프로토콜 층이다:
//! 초기화·도구 목록(읽기 모드와 쓰기 모드의 38개)·호출 결과 모양·오류 코드·Codex 세션 출처 규칙. 도구가 엔진에서 하는 일은
//! `cli_compat`이 이미 견줬다.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::{env, fs};

use ais_cli::mcp::{Call, McpServer};
use ais_cli::registry::CLI_TOOL_NAMES;

/// 가짜 호출기: Java `DriveMcp.fake`와 같다.
fn fake(args: &[String], session: Option<ais_cli::env::SessionSource>) -> Call {
    let arguments = &args[1];
    let label = session
        .map(|s| format!("{}/{}", s.agent, s.id))
        .unwrap_or_else(|| "-".to_string());
    let call = |status, stdout: &str, stderr: &str| Call {
        status,
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
    };
    if arguments.contains("fail") {
        return call(3, "", "AIS-400 boom\n");
    }
    if arguments.contains("silent") {
        return call(0, "", "");
    }
    if arguments.contains("nostderr") {
        return call(3, "out text", "");
    }
    if arguments.contains("blank") {
        return call(5, "", "  \n");
    }
    if arguments.contains("both") {
        return call(3, "stdout part", "AIS-412 stale \n  ");
    }
    call(0, &format!("ARGS:{} SESSION:{label}\n", args.join("|")), "")
}

fn cases() -> Vec<String> {
    let mut cases: Vec<String> = Vec::new();
    let mut add = |line: &str| cases.push(line.to_string());

    // 초기화: 요청한 개정, 모르는 개정, 개정 없음, 클라이언트 이름
    add(
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"claude-code","version":"1"}}}"#,
    );
    add(
        r#"{"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}"#,
    );
    add(r#"{"jsonrpc":"2.0","id":3,"method":"initialize"}"#);
    add(
        r#"{"jsonrpc":"2.0","id":4,"method":"initialize","params":{"protocolVersion":"2025-03-26","clientInfo":{"name":"Codex-CLI"}}}"#,
    );
    add(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
    add(r#"{"jsonrpc":"2.0","id":5,"method":"ping"}"#);
    add(r#"{"jsonrpc":"2.0","id":6,"method":"tools/list"}"#);
    add(r#"{"jsonrpc":"2.0","id":"abc","method":"tools/list","params":{}}"#);

    // 모든 도구 호출(읽기 모드에서는 쓰기 도구가 막히고, 쓰기 모드에서는 영구 삭제만 막힌다)
    for (n, name) in CLI_TOOL_NAMES.iter().enumerate() {
        add(&format!(
            r#"{{"jsonrpc":"2.0","id":{},"method":"tools/call","params":{{"name":"{name}","arguments":{{"id":"x{n}","q":"한글 \"따옴표\" \n줄"}}}}}}"#,
            100 + n
        ));
    }
    add(r#"{"jsonrpc":"2.0","id":200,"method":"tools/call","params":{"name":"get_doc"}}"#);
    add(
        r#"{"jsonrpc":"2.0","id":201,"method":"tools/call","params":{"name":"get_doc","arguments":null}}"#,
    );
    add(
        r#"{"jsonrpc":"2.0","id":202,"method":"tools/call","params":{"name":"get_doc","arguments":[1]}}"#,
    );
    add(
        r#"{"jsonrpc":"2.0","id":203,"method":"tools/call","params":{"name":"get_doc","arguments":"x"}}"#,
    );
    add(
        r#"{"jsonrpc":"2.0","id":204,"method":"tools/call","params":{"name":"nope","arguments":{}}}"#,
    );
    add(r#"{"jsonrpc":"2.0","id":205,"method":"tools/call","params":{"arguments":{}}}"#);
    add(r#"{"jsonrpc":"2.0","id":206,"method":"tools/call","params":{"name":5}}"#);
    add(r#"{"jsonrpc":"2.0","id":207,"method":"tools/call"}"#);
    add(r#"{"jsonrpc":"2.0","id":208,"method":"tools/call","params":[]}"#);
    add(
        r#"{"jsonrpc":"2.0","id":209,"method":"tools/call","params":{"name":"purge_doc","arguments":{"trashId":"t"}}}"#,
    );
    add(
        r#"{"jsonrpc":"2.0","id":210,"method":"tools/call","params":{"name":"list_docs","arguments":{"agenda":"fail"}}}"#,
    );
    add(
        r#"{"jsonrpc":"2.0","id":211,"method":"tools/call","params":{"name":"list_docs","arguments":{"agenda":"silent"}}}"#,
    );
    add(
        r#"{"jsonrpc":"2.0","id":212,"method":"tools/call","params":{"name":"list_docs","arguments":{"agenda":"nostderr"}}}"#,
    );
    add(
        r#"{"jsonrpc":"2.0","id":213,"method":"tools/call","params":{"name":"list_docs","arguments":{"agenda":"blank"}}}"#,
    );
    add(
        r#"{"jsonrpc":"2.0","id":214,"method":"tools/call","params":{"name":"list_docs","arguments":{"agenda":"both"}}}"#,
    );
    add(
        r#"{"jsonrpc":"2.0","id":215,"method":"tools/call","params":{"name":"get_doc","arguments":{"id":"ünï","n":[1,2.5,true,null,{"a":"b"}]}}}"#,
    );

    // Codex 세션 출처: 클라이언트 이름이 codex일 때만, `sessionId`가 먼저고 없으면 `threadId`
    add(
        r#"{"jsonrpc":"2.0","id":300,"method":"initialize","params":{"clientInfo":{"name":"codex-mcp-client"}}}"#,
    );
    add(
        r#"{"jsonrpc":"2.0","id":301,"method":"tools/call","params":{"name":"get_doc","arguments":{"id":"a"},"_meta":{"sessionId":"sess-1"}}}"#,
    );
    add(
        r#"{"jsonrpc":"2.0","id":302,"method":"tools/call","params":{"name":"get_doc","arguments":{"id":"a"},"_meta":{"threadId":"thr-1"}}}"#,
    );
    add(
        r#"{"jsonrpc":"2.0","id":303,"method":"tools/call","params":{"name":"get_doc","arguments":{"id":"a"},"_meta":{"sessionId":"  ","threadId":"thr-2"}}}"#,
    );
    add(
        r#"{"jsonrpc":"2.0","id":304,"method":"tools/call","params":{"name":"get_doc","arguments":{"id":"a"},"_meta":{"sessionId":5}}}"#,
    );
    add(
        r#"{"jsonrpc":"2.0","id":305,"method":"tools/call","params":{"name":"get_doc","arguments":{"id":"a"},"_meta":"x"}}"#,
    );
    add(
        r#"{"jsonrpc":"2.0","id":306,"method":"tools/call","params":{"name":"get_doc","arguments":{"id":"a"},"_meta":{"sessionId":" padded "}}}"#,
    );
    add(
        r#"{"jsonrpc":"2.0","id":307,"method":"initialize","params":{"clientInfo":{"name":"claude-code"}}}"#,
    );
    add(
        r#"{"jsonrpc":"2.0","id":308,"method":"tools/call","params":{"name":"get_doc","arguments":{"id":"a"},"_meta":{"sessionId":"sess-9"}}}"#,
    );

    // id의 모양과 깨진 메시지
    add(r#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#);
    add(r#"{"jsonrpc":"2.0","id":1.5,"method":"ping"}"#);
    add(r#"{"jsonrpc":"2.0","id":{"a":[1]},"method":"ping"}"#);
    add(r#"{"jsonrpc":"2.0","id":"한글","method":"ping"}"#);
    add(r#"{"jsonrpc":"2.0","id":400,"method":"nope/nothing"}"#);
    add(r#"{"jsonrpc":"2.0","id":401,"method":5}"#);
    add(r#"{"jsonrpc":"2.0","id":402}"#);
    add(r#"{"jsonrpc":"2.0","result":{}}"#);
    add(r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":1}}"#);
    add(r#"[1,2]"#);
    add(r#"5"#);
    add(r#""text""#);
    add(r#"{"jsonrpc":"2.0","id":403,"method":"ping""#);
    add(r#"not json"#);
    add(r#"{"id":404,"method":"ping","id":405}"#);
    add(r#"  {"id" : 406 , "method" : "ping" }  "#);
    cases
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(dist) = args.first() else {
        eprintln!(
            "사용법: mcp_compat <ais-cli.jar와 ais-engine.jar가 든 기준 폴더(build-oracle.sh)>"
        );
        return ExitCode::from(2);
    };
    let dist = Path::new(dist);
    let root: PathBuf = env::temp_dir().join(format!("ais-mcp-compat-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("임시 폴더");

    let cases = cases();
    let case_file = root.join("cases.jsonl");
    fs::write(&case_file, cases.join("\n") + "\n").expect("사례 파일");

    // Java 구동기를 같은 패키지 이름으로 컴파일한다(서버 클래스가 패키지 안에서만 보인다)
    let classpath = format!(
        "{}:{}",
        dist.join("ais-cli.jar").display(),
        dist.join("ais-engine.jar").display()
    );
    let compiled = root.join("classes");
    fs::create_dir_all(&compiled).unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/compat/DriveMcp.java");
    let status = Command::new("javac")
        .args(["-encoding", "UTF-8", "-cp", &classpath, "-d"])
        .arg(&compiled)
        .arg(&source)
        .status()
        .expect("javac");
    assert!(status.success(), "DriveMcp를 못 지었다");

    let mut checked = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for write in [false, true] {
        let mut command = Command::new("java");
        command
            .arg("-cp")
            .arg(format!("{classpath}:{}", compiled.display()))
            .arg("aisurada.cli.DriveMcp")
            .arg(&case_file);
        if write {
            command.arg("--write");
        }
        let output = command.output().expect("java");
        assert!(
            output.status.success(),
            "Java 구동기 실패: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let java_lines: Vec<String> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_string)
            .collect();

        let mut server = McpServer::new(write, Box::new(fake));
        let mut index = 0usize;
        for line in &cases {
            if line.trim().is_empty() {
                continue;
            }
            let rust = server.handle(line).unwrap_or_else(|| "<none>".to_string());
            let rust = format!("{index} {rust}");
            let java = java_lines.get(index).cloned().unwrap_or_default();
            checked += 1;
            if java != rust {
                failures.push(format!(
                    "쓰기={write} 사례 {index}: {}\n    Java: {}\n    Rust: {}",
                    clip(line),
                    clip(&java),
                    clip(&rust)
                ));
            }
            index += 1;
        }
        if java_lines.len() != index {
            failures.push(format!(
                "쓰기={write}: 답 줄 수가 다르다 (Java {}, Rust {index})",
                java_lines.len()
            ));
        }
    }

    println!("mcp 맞대기: 확인 {checked}건, 어긋남 {}건", failures.len());
    for failure in failures.iter().take(30) {
        println!("  어긋남: {failure}");
    }
    let _ = fs::remove_dir_all(&root);
    if failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn clip(text: &str) -> String {
    let mut out: String = text.chars().take(220).collect();
    if text.chars().count() > 220 {
        out.push('…');
    }
    out
}
