//! `claude-mcp`·`codex-mcp`·`gemini-mcp`를 가짜 에이전트 CLI(셸 스크립트)로 시험한다. 진짜 설정 파일은 건드리지 않는다.
//! 가짜 CLI는 받은 인자를 로그에 한 줄씩 남기고, 등록 상태를 파일 하나로 흉내 낸다.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use ais_cli::env::Env;

const EXE: &str = "/fake/bundle/bin/ais-cli";

struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Sandbox {
        let root = std::env::temp_dir().join(format!("ais-register-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("home")).unwrap();
        Sandbox { root }
    }

    fn script(&self, file_name: &str, body: &str) -> PathBuf {
        let dir = self.root.join("bin");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(file_name);
        std::fs::write(
            &path,
            format!("#!/bin/bash\nprintf '%s\\n' \"$*\" >> \"$FAKE_LOG\"\n{body}\n"),
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn env(&self, bin_var: &str, bin: &Path) -> Env {
        let mut vars = BTreeMap::new();
        vars.insert(
            "HOME".to_string(),
            self.root.join("home").display().to_string(),
        );
        vars.insert("PATH".to_string(), "/usr/bin:/bin".to_string());
        vars.insert(
            "FAKE_LOG".to_string(),
            self.root.join("log").display().to_string(),
        );
        vars.insert(
            "FAKE_STATE".to_string(),
            self.root.join("state").display().to_string(),
        );
        vars.insert(bin_var.to_string(), bin.display().to_string());
        Env {
            vars,
            home: self.root.join("home").display().to_string(),
            cwd: self.root.display().to_string(),
            exe: EXE.to_string(),
            session: None,
        }
    }

    fn log(&self) -> Vec<String> {
        std::fs::read_to_string(self.root.join("log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn clear_log(&self) {
        let _ = std::fs::remove_file(self.root.join("log"));
    }

    fn set_state(&self, text: &str) {
        std::fs::write(self.root.join("state"), text).unwrap();
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn run(env: &Env, args: &[&str]) -> ais_cli::Outcome {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    ais_cli::run(&args, env)
}

fn json(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("JSON이 아니다({e}): {text}"))
}

const CLAUDE: &str = r#"
case "$1 $2" in
  "mcp get") if [ -f "$FAKE_STATE" ]; then echo "aisurada:"; echo "  Command: $(cat "$FAKE_STATE")"; exit 0; else echo "No MCP server found"; exit 1; fi ;;
  "mcp remove") rm -f "$FAKE_STATE"; exit 0 ;;
  "mcp add-json") printf '%s' "$4" > "$FAKE_STATE"; exit 0 ;;
esac
exit 9"#;

#[test]
fn claude_registers_replaces_and_removes() {
    let sandbox = Sandbox::new("claude");
    let claude = sandbox.script("claude", CLAUDE);
    let env = sandbox.env("AISURADA_CLAUDE_BIN", &claude);

    let status = run(&env, &["claude-mcp", "status"]);
    assert_eq!(status.code, 0);
    assert_eq!(
        json(&status.stdout),
        serde_json::json!({"claudeFound": true, "registered": false, "write": false, "detail": "", "stale": false})
    );

    sandbox.clear_log();
    let added = run(&env, &["claude-mcp", "add"]);
    assert_eq!(added.code, 0, "{}", added.stderr);
    assert_eq!(
        json(&added.stdout),
        serde_json::json!({"ok": true, "registered": true, "write": false, "followsAppVault": true, "command": EXE})
    );
    // 같은 이름을 먼저 지우고(없어도 괜찮다) 사용자 범위로 넣는다. 등록 값에는 vault가 없다
    let log = sandbox.log();
    assert_eq!(log[0], "mcp remove aisurada --scope user");
    let payload =
        format!("{{\"type\":\"stdio\",\"command\":\"{EXE}\",\"args\":[\"mcp\"],\"env\":{{}}}}");
    assert_eq!(
        log[1],
        format!("mcp add-json aisurada {payload} --scope user")
    );

    let status = json(&run(&env, &["claude-mcp", "status"]).stdout);
    assert_eq!(
        (
            status["registered"].clone(),
            status["write"].clone(),
            status["stale"].clone()
        ),
        (true.into(), false.into(), false.into())
    );
    assert!(status["detail"].as_str().unwrap().contains(EXE));

    run(&env, &["claude-mcp", "add", "--write"]);
    let status = json(&run(&env, &["claude-mcp", "status"]).stdout);
    assert_eq!(status["write"], true);

    let removed = run(&env, &["claude-mcp", "remove"]);
    assert_eq!(
        (removed.code, json(&removed.stdout)),
        (0, serde_json::json!({"ok": true, "registered": false}))
    );
    assert_eq!(
        json(&run(&env, &["claude-mcp", "status"]).stdout)["registered"],
        false
    );
}

#[test]
fn claude_flags_a_java_era_registration_as_stale() {
    let sandbox = Sandbox::new("stale");
    let claude = sandbox.script("claude", CLAUDE);
    let env = sandbox.env("AISURADA_CLAUDE_BIN", &claude);
    sandbox.set_state(r#"{"type":"stdio","command":"/App/Contents/Resources/runtime/bin/java","args":["-jar","/App/Contents/Resources/ais-cli.jar","mcp"]}"#);
    let status = json(&run(&env, &["claude-mcp", "status"]).stdout);
    assert_eq!(
        (status["registered"].clone(), status["stale"].clone()),
        (true.into(), true.into())
    );
    // 다시 add하면 새 등록으로 바뀐다
    run(&env, &["claude-mcp", "add"]);
    let status = json(&run(&env, &["claude-mcp", "status"]).stdout);
    assert_eq!(status["stale"], false);
}

#[test]
fn claude_config_prints_the_server_entry_without_registering() {
    let sandbox = Sandbox::new("config");
    let claude = sandbox.script("claude", CLAUDE);
    let env = sandbox.env("AISURADA_CLAUDE_BIN", &claude);
    let out = run(&env, &["claude-mcp", "config", "--write"]);
    assert_eq!(out.code, 0);
    assert_eq!(
        json(&out.stdout),
        serde_json::json!({"mcpServers": {"aisurada": {"type": "stdio", "command": EXE, "args": ["mcp", "--write"], "env": {}}}})
    );
    assert!(sandbox.log().is_empty(), "config는 claude를 부르지 않는다");
}

#[test]
fn usage_errors_exit_2() {
    let sandbox = Sandbox::new("usage");
    let claude = sandbox.script("claude", CLAUDE);
    let env = sandbox.env("AISURADA_CLAUDE_BIN", &claude);
    for (args, message) in [
        (
            vec!["claude-mcp"],
            "사용법: ais-cli claude-mcp <status|add|remove|config> [--write]",
        ),
        (
            vec!["claude-mcp", "bogus"],
            "사용법: ais-cli claude-mcp <status|add|remove|config> [--write]",
        ),
        (
            vec!["claude-mcp", "add", "status"],
            "사용법: ais-cli claude-mcp <status|add|remove|config> [--write]",
        ),
        (
            vec!["claude-mcp", "status", "--write"],
            "--write는 add와 config에서만 쓴다",
        ),
        (
            vec!["codex-mcp", "config"],
            "사용법: ais-cli codex-mcp <status|add|remove> [--write]",
        ),
        (
            vec!["gemini-mcp", "remove", "--write"],
            "--write는 add에서만 쓴다",
        ),
    ] {
        let out = run(&env, &args);
        assert_eq!((out.code, out.stderr.trim()), (2, message), "{args:?}");
    }
}

#[test]
fn claude_failure_messages() {
    let sandbox = Sandbox::new("fail");
    let claude = sandbox.script(
        "claude",
        "case \"$1 $2\" in \"mcp add-json\") echo '거절했다'; exit 4 ;; esac\nexit 1",
    );
    let env = sandbox.env("AISURADA_CLAUDE_BIN", &claude);
    let out = run(&env, &["claude-mcp", "add"]);
    assert_eq!(
        (out.code, out.stderr.trim()),
        (1, "연결을 못 했다: 거절했다")
    );
}

#[test]
fn codex_uses_get_add_and_remove() {
    let sandbox = Sandbox::new("codex");
    let codex = sandbox.script(
        "codex",
        r#"
case "$1 $2" in
  "mcp get") if [ -f "$FAKE_STATE" ]; then cat "$FAKE_STATE"; exit 0; else echo "not found"; exit 1; fi ;;
  "mcp remove") rm -f "$FAKE_STATE"; exit 0 ;;
  "mcp add") echo "$*" > "$FAKE_STATE"; exit 0 ;;
esac
exit 9"#,
    );
    let env = sandbox.env("AISURADA_CODEX_BIN", &codex);

    let status = json(&run(&env, &["codex-mcp", "status"]).stdout);
    assert_eq!(
        (status["codexFound"].clone(), status["registered"].clone()),
        (true.into(), false.into())
    );
    assert_eq!(status["cli"], "codex");

    run(&env, &["codex-mcp", "add", "--write"]);
    assert_eq!(
        sandbox.log().last().unwrap(),
        &format!("mcp add aisurada -- {EXE} mcp --write")
    );
    let status = json(&run(&env, &["codex-mcp", "status"]).stdout);
    assert_eq!(
        (status["registered"].clone(), status["write"].clone()),
        (true.into(), true.into())
    );

    // 이미 있으면 지우고 다시 넣는다
    sandbox.clear_log();
    run(&env, &["codex-mcp", "add"]);
    let log = sandbox.log();
    assert!(log.iter().any(|l| l == "mcp remove aisurada"), "{log:?}");
    assert_eq!(
        log.last().unwrap(),
        &format!("mcp add aisurada -- {EXE} mcp")
    );

    let removed = run(&env, &["codex-mcp", "remove"]);
    assert_eq!(
        json(&removed.stdout),
        serde_json::json!({"ok": true, "registered": false})
    );
}

#[test]
fn antigravity_and_gemini_cli_find_themselves_in_the_list() {
    // agy: 등록 줄이 "aisurada  stdio  enabled  <명령>" 꼴이고 제거에 범위 옵션이 없다
    let sandbox = Sandbox::new("agy");
    let agy = sandbox.script(
        "agy",
        r#"
case "$1 $2" in
  "mcp list") echo "other  stdio  enabled  /x"; [ -f "$FAKE_STATE" ] && echo "aisurada  stdio  enabled  $(cat "$FAKE_STATE")"; exit 0 ;;
  "mcp remove") rm -f "$FAKE_STATE"; exit 0 ;;
  "mcp add") echo "$*" > "$FAKE_STATE"; exit 0 ;;
esac
exit 9"#,
    );
    let env = sandbox.env("AISURADA_GEMINI_BIN", &agy);
    assert_eq!(
        json(&run(&env, &["gemini-mcp", "status"]).stdout)["registered"],
        false
    );
    run(&env, &["gemini-mcp", "add", "--write"]);
    assert_eq!(
        sandbox.log().last().unwrap(),
        &format!("mcp add aisurada -- {EXE} mcp --write")
    );
    let status = json(&run(&env, &["gemini-mcp", "status"]).stdout);
    assert_eq!(
        (
            status["geminiFound"].clone(),
            status["cli"].clone(),
            status["registered"].clone(),
            status["write"].clone()
        ),
        (true.into(), "agy".into(), true.into(), true.into())
    );
    run(&env, &["gemini-mcp", "remove"]);
    assert_eq!(sandbox.log().last().unwrap(), "mcp remove aisurada");

    // gemini CLI: 등록 줄이 "✓ aisurada: <명령> (stdio) - Connected" 꼴이고 `-s user`가 붙는다
    let sandbox = Sandbox::new("gemini");
    let gemini = sandbox.script(
        "gemini",
        r#"
case "$1 $2" in
  "mcp list") [ -f "$FAKE_STATE" ] && echo "✓ aisurada: $(cat "$FAKE_STATE") (stdio) - Connected"; exit 0 ;;
  "mcp remove") rm -f "$FAKE_STATE"; exit 0 ;;
  "mcp add") echo "$*" > "$FAKE_STATE"; exit 0 ;;
esac
exit 9"#,
    );
    let env = sandbox.env("AISURADA_GEMINI_BIN", &gemini);
    run(&env, &["gemini-mcp", "add"]);
    assert_eq!(
        sandbox.log().last().unwrap(),
        &format!("mcp add -s user aisurada {EXE} -- mcp")
    );
    let status = json(&run(&env, &["gemini-mcp", "status"]).stdout);
    assert_eq!(
        (status["cli"].clone(), status["registered"].clone()),
        ("gemini".into(), true.into())
    );
    run(&env, &["gemini-mcp", "remove"]);
    assert_eq!(sandbox.log().last().unwrap(), "mcp remove -s user aisurada");
}

#[test]
fn a_hanging_cli_is_reported_not_waited_on_forever() {
    // 40초를 다 기다리는 시험은 너무 길어서, 바로 끝나는 실패(실행 불가 파일)로 "못 돌렸다" 글만 확인한다
    let sandbox = Sandbox::new("broken");
    let path = sandbox.root.join("bin");
    std::fs::create_dir_all(&path).unwrap();
    let broken = path.join("claude");
    std::fs::write(&broken, "not executable").unwrap();
    std::fs::set_permissions(&broken, std::fs::Permissions::from_mode(0o644)).unwrap();
    let env = sandbox.env("AISURADA_CLAUDE_BIN", &broken);
    let out = run(&env, &["claude-mcp", "status"]);
    assert_eq!(out.code, 1);
    assert!(
        out.stderr.starts_with("claude를 못 돌렸다: "),
        "{}",
        out.stderr
    );
}
