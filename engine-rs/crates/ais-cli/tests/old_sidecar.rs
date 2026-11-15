//! 옛 사이드카를 걷어 내는 `sweep`과, 상태를 보여 주는 `doctor`, 안내만 하는 `install`을 가짜 HOME과 가짜 launchctl로 시험한다.
//! 진짜 launchd와 진짜 홈은 건드리지 않는다.

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use ais_cli::env::Env;

struct Home {
    root: PathBuf,
}

impl Home {
    fn new(name: &str) -> Home {
        let root = std::env::temp_dir().join(format!("ais-sweep-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("home")).unwrap();
        Home { root }
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    fn write(&self, rel: &str, text: &str) -> PathBuf {
        let path = self.home().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path
    }

    fn env(&self) -> Env {
        let launchctl = self.root.join("launchctl");
        std::fs::write(
            &launchctl,
            "#!/bin/bash\nprintf '%s\\n' \"$*\" >> \"$FAKE_LOG\"\nexit 0\n",
        )
        .unwrap();
        std::fs::set_permissions(&launchctl, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut vars = BTreeMap::new();
        vars.insert("HOME".to_string(), self.home().display().to_string());
        vars.insert("PATH".to_string(), "/usr/bin:/bin".to_string());
        vars.insert(
            "FAKE_LOG".to_string(),
            self.root.join("log").display().to_string(),
        );
        vars.insert(
            "AISURADA_LAUNCHCTL_CMD".to_string(),
            launchctl.display().to_string(),
        );
        Env {
            vars,
            home: self.home().display().to_string(),
            cwd: self.root.display().to_string(),
            exe: "/fake/ais-cli".to_string(),
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

    fn exists(&self, rel: &str) -> bool {
        self.home().join(rel).exists()
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn run(env: &Env, args: &[&str]) -> ais_cli::Outcome {
    let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    ais_cli::run(&args, env)
}

/// 이미 끝난 프로세스의 pid(그 pid로 도는 것이 없다)
fn dead_pid() -> u32 {
    let mut child = std::process::Command::new("true").spawn().unwrap();
    let pid = child.id();
    child.wait().unwrap();
    pid
}

fn plist(label: &str, vault: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?>\n<plist version=\"1.0\">\n<dict>\n  <key>Label</key>\n  <string>{label}</string>\n  <key>EnvironmentVariables</key>\n  <dict>\n    <key>AISURADA_VAULT</key>\n    <string>{vault}</string>\n  </dict>\n</dict>\n</plist>\n"
    )
}

fn seed(h: &Home) {
    let label = "co.kcd.aisurada.sidecar.abc123";
    h.write(
        &format!("Library/LaunchAgents/{label}.plist"),
        &plist(label, "/Users/example/a &amp; b"),
    );
    h.write(
        "Library/Application Support/aisurada/run/abc123.json",
        &format!(
            "{{\"pid\": {}, \"vault\": \"/Users/example/a & b\"}}",
            dead_pid()
        ),
    );
    h.write("Library/Application Support/aisurada/run/abc123.lock", "");
    h.write(
        "Library/Application Support/aisurada/bin/ais-cli.jar",
        "jar",
    );
    h.write(
        "Library/Application Support/aisurada/bin/ais-sidecar.jar",
        "jar",
    );
    h.write(
        "Library/Application Support/aisurada/bin/notes.txt",
        "남의 파일",
    );
    h.write(
        "Library/Application Support/aisurada/logs/sidecar-abc123.log",
        "log",
    );
    h.write("Library/Application Support/aisurada/locks/keep.lock", "");
    h.write(
        "Library/Caches/aisurada/last-sweep",
        "2026-01-01T00:00:00Z\n",
    );
    // 우리 것이 아닌 launchd 항목
    h.write(
        "Library/LaunchAgents/com.other.thing.plist",
        &plist("com.other.thing", "/x"),
    );
}

#[test]
fn dry_run_lists_without_changing_anything() {
    let h = Home::new("dry");
    seed(&h);
    let out = run(&h.env(), &["sweep", "--dry-run"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(out.stdout.contains("옛 사이드카 1개"), "{}", out.stdout);
    assert!(out.stdout.contains("co.kcd.aisurada.sidecar.abc123"));
    assert!(
        out.stdout.contains("vault=/Users/example/a & b"),
        "XML 탈출을 푼다: {}",
        out.stdout
    );
    assert!(out.stdout.contains("아직 안 내렸다"));
    assert!(h.log().is_empty(), "launchctl을 부르지 않는다");
    assert!(h.exists("Library/LaunchAgents/co.kcd.aisurada.sidecar.abc123.plist"));
}

#[test]
fn sweep_boots_out_our_labels_only_and_removes_their_traces() {
    let h = Home::new("sweep");
    seed(&h);
    let out = run(&h.env(), &["sweep"]);
    assert_eq!((out.code, out.stderr.as_str()), (0, ""), "{}", out.stdout);

    let log = h.log();
    assert_eq!(log.len(), 1, "{log:?}");
    assert!(
        log[0].starts_with("bootout gui/") && log[0].ends_with("/co.kcd.aisurada.sidecar.abc123"),
        "{log:?}"
    );

    for gone in [
        "Library/LaunchAgents/co.kcd.aisurada.sidecar.abc123.plist",
        "Library/Application Support/aisurada/run/abc123.json",
        "Library/Application Support/aisurada/bin/ais-cli.jar",
        "Library/Application Support/aisurada/bin/ais-sidecar.jar",
        "Library/Application Support/aisurada/logs/sidecar-abc123.log",
        "Library/Caches/aisurada/last-sweep",
    ] {
        assert!(!h.exists(gone), "{gone}는 지워져야 한다");
    }
    for kept in [
        "Library/LaunchAgents/com.other.thing.plist",
        "Library/Application Support/aisurada/run/abc123.lock",
        "Library/Application Support/aisurada/locks/keep.lock",
        "Library/Application Support/aisurada/bin/notes.txt",
    ] {
        assert!(h.exists(kept), "{kept}는 남아야 한다");
    }

    // 걷어 낼 것이 없으면 그렇게 말한다
    let again = run(&h.env(), &["sweep"]);
    assert_eq!(again.code, 0);
    assert!(again.stdout.contains("걷어 낼 옛 사이드카 흔적이 없다"));
}

#[test]
fn a_sidecar_that_keeps_running_is_reported_and_its_files_stay() {
    let h = Home::new("alive");
    // 이 시험 프로세스 자신의 pid를 "아직 도는 사이드카"로 쓴다
    h.write(
        "Library/Application Support/aisurada/run/live.json",
        &format!("{{\"pid\": {}, \"vault\": \"/v\"}}", std::process::id()),
    );
    let out = run(&h.env(), &["sweep"]);
    assert_eq!(out.code, 1);
    assert!(out.stderr.contains("가 아직 돈다"), "{}", out.stderr);
    assert!(h.exists("Library/Application Support/aisurada/run/live.json"));
    assert!(
        h.log()
            .iter()
            .any(|l| l.ends_with("/co.kcd.aisurada.sidecar.live")),
        "{:?}",
        h.log()
    );
}

#[test]
fn doctor_reports_vault_registrations_and_old_traces() {
    let h = Home::new("doctor");
    seed(&h);
    let vault = h.root.join("vault");
    std::fs::create_dir_all(&vault).unwrap();
    // 낡은 Java 등록이 남은 가짜 claude
    let claude = h.root.join("claude");
    std::fs::write(
        &claude,
        "#!/bin/bash\ncase \"$1 $2\" in \"mcp get\") echo 'Command: /App/runtime/bin/java -jar /App/ais-cli.jar mcp --write'; exit 0 ;; esac\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut env = h.env();
    env.vars
        .insert("AISURADA_VAULT".into(), vault.display().to_string());
    env.vars
        .insert("AISURADA_CLAUDE_BIN".into(), claude.display().to_string());
    env.vars.insert(
        "AISURADA_CODEX_BIN".into(),
        h.root.join("none-codex").display().to_string(),
    );

    let out = run(&env, &["doctor"]);
    assert_eq!(out.code, 0, "{}", out.stderr);
    let text = out.stdout;
    assert!(
        text.contains(&format!(
            "vault      {}  (환경변수 AISURADA_VAULT)",
            vault.display()
        )),
        "{text}"
    );
    assert!(text.contains("git 저장소가 아니다"), "{text}");
    assert!(text.contains("★낡은 등록(읽기+쓰기)"), "{text}");
    assert!(text.contains("claude-mcp add --write"), "{text}");
    assert!(
        text.contains("옛 사이드카  1개, jar 2개 남아 있다"),
        "{text}"
    );
    assert!(
        h.exists("Library/LaunchAgents/co.kcd.aisurada.sidecar.abc123.plist"),
        "doctor는 읽기만 한다"
    );

    // --prune --dry-run은 걷어 낼 목록까지만
    let out = run(&env, &["doctor", "--prune", "--dry-run"]);
    assert!(out.stdout.contains("아직 안 내렸다"));
    assert!(h.exists("Library/LaunchAgents/co.kcd.aisurada.sidecar.abc123.plist"));
}

#[test]
fn doctor_without_a_vault_still_reports_the_rest() {
    let h = Home::new("novault");
    let out = run(&h.env(), &["doctor"]);
    assert_eq!(out.code, 0);
    assert!(
        out.stdout.contains("vault      (못 정했다."),
        "{}",
        out.stdout
    );
    assert!(out.stdout.contains("옛 사이드카  없다"));
}

#[test]
fn install_only_explains() {
    let h = Home::new("install");
    seed(&h);
    let out = run(&h.env(), &["install"]);
    assert_eq!(out.code, 0);
    assert!(out.stdout.contains("설치할 파일이 없다"));
    assert!(out.stdout.contains("ais-cli sweep"));
    assert!(
        Path::new(
            &h.home()
                .join("Library/Application Support/aisurada/bin/ais-cli.jar")
        )
        .exists()
    );
}

#[test]
fn doctor_fix_sweeps_and_reregisters_a_stale_mcp_with_the_same_mode() {
    let h = Home::new("fix");
    seed(&h);
    // 상태에 낡은 Java 등록(쓰기 모드)이 들어 있고, add-json이 오면 그 값을 상태에 적는 가짜 claude
    let claude = h.root.join("claude");
    let state = h.root.join("claude-state");
    std::fs::write(&state, "java -jar /App/ais-cli.jar mcp --write").unwrap();
    std::fs::write(
        &claude,
        "#!/bin/bash\nprintf '%s\\n' \"$*\" >> \"$FAKE_LOG\"\nS=\"$FAKE_STATE\"\ncase \"$1 $2\" in\n  \"mcp get\") [ -f \"$S\" ] && { echo \"Command: $(cat \"$S\")\"; exit 0; }; exit 1 ;;\n  \"mcp remove\") rm -f \"$S\"; exit 0 ;;\n  \"mcp add-json\") printf '%s' \"$4\" > \"$S\"; exit 0 ;;\nesac\nexit 9\n",
    )
    .unwrap();
    std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut env = h.env();
    env.vars
        .insert("AISURADA_CLAUDE_BIN".into(), claude.display().to_string());
    env.vars.insert(
        "AISURADA_CODEX_BIN".into(),
        h.root.join("none-codex").display().to_string(),
    );
    env.vars
        .insert("FAKE_STATE".into(), state.display().to_string());
    env.vars
        .insert("AISURADA_VAULT".into(), h.root.display().to_string());

    let out = run(&env, &["doctor", "--fix"]);
    assert_eq!(out.code, 0, "{}\n{}", out.stdout, out.stderr);
    assert!(
        out.stdout
            .contains("Claude Code의 낡은 MCP 등록을 새 실행 파일로 다시 등록했다(읽기+쓰기)"),
        "{}",
        out.stdout
    );
    let registered = std::fs::read_to_string(&state).unwrap();
    assert!(
        registered.contains("/fake/ais-cli") && registered.contains("\"--write\""),
        "{registered}"
    );
    assert!(!registered.contains(".jar"));
    assert!(!h.exists("Library/LaunchAgents/co.kcd.aisurada.sidecar.abc123.plist"));

    // 다시 돌리면 할 일이 없다
    let again = run(&env, &["doctor", "--fix"]);
    assert_eq!(again.code, 0);
    assert!(again.stdout.contains("걷어 낼 옛 사이드카 흔적이 없다"));
    assert!(!again.stdout.contains("다시 등록했다"));
}

#[test]
fn migration_leaves_a_note_when_the_executable_is_unknown() {
    let h = Home::new("noexe");
    let claude = h.root.join("claude");
    std::fs::write(
        &claude,
        "#!/bin/bash\necho 'Command: java -jar /App/ais-cli.jar mcp'\nexit 0\n",
    )
    .unwrap();
    std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut env = h.env();
    env.exe = String::new();
    env.vars
        .insert("AISURADA_CLAUDE_BIN".into(), claude.display().to_string());
    let migration = ais_cli::migrate::run(&env);
    assert_eq!(migration.left.len(), 1, "{:?}", migration.left);
    assert!(migration.left[0].contains("ais-cli 실행 파일의 자리를 모른다"));
}

#[test]
fn run_once_marks_a_finished_migration_and_skips_the_next_time() {
    let h = Home::new("once");
    seed(&h);
    let env = h.env();
    let first = ais_cli::migrate::run_once(&env).expect("처음에는 이주를 한다");
    assert!(first.left.is_empty(), "{:?}", first.left);
    assert!(h.exists("Library/Application Support/aisurada/rust-engine-migrated"));
    assert!(!h.exists("Library/LaunchAgents/co.kcd.aisurada.sidecar.abc123.plist"));
    assert!(
        ais_cli::migrate::run_once(&env).is_none(),
        "표시가 있으면 다시 하지 않는다"
    );
}

#[test]
fn run_once_does_not_mark_when_something_is_left() {
    let h = Home::new("once-left");
    // 도는 사이드카(이 프로세스의 pid)는 지우지 못하므로 남는다
    h.write(
        "Library/Application Support/aisurada/run/live.json",
        &format!("{{\"pid\": {}, \"vault\": \"/v\"}}", std::process::id()),
    );
    let env = h.env();
    assert!(!ais_cli::migrate::run_once(&env).unwrap().left.is_empty());
    assert!(!h.exists("Library/Application Support/aisurada/rust-engine-migrated"));
}
