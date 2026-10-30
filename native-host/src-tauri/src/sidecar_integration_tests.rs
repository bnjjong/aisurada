//! Isolated subprocess test: native IPC -> real java wl-cli -> a test-owned serve engine.
use crate::{environment, lifecycle::HostState, sidecar, workspace};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use tauri::{
    ipc::{CallbackFn, InvokeBody},
    test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime},
    webview::InvokeRequest,
    WebviewWindow,
};

fn invoke(window: &WebviewWindow<MockRuntime>, name: &str, value: Value) -> Result<Value, Value> {
    get_ipc_response(
        window,
        InvokeRequest {
            cmd: name.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: InvokeBody::Json(value),
            headers: Default::default(),
            invoke_key: tauri::test::INVOKE_KEY.into(),
        },
    )
    .map(|v| v.deserialize().unwrap())
}
#[test]
#[ignore = "invoked only by the isolated real-engine test"]
fn ipc_probe() {
    let root = std::env::var("WITHLOG_VAULT").unwrap();
    let app = mock_builder()
        .plugin(tauri_plugin_fs::init())
        .manage(HostState::default())
        .invoke_handler(tauri::generate_handler![
            workspace::allow_workspace,
            sidecar::sidecar_call
        ])
        .build(mock_context(noop_assets()))
        .unwrap();
    let main = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    assert_eq!(
        invoke(&main, "sidecar_call", json!({"argv":["list-docs"]})).unwrap_err(),
        json!("vault가 아직 열리지 않았어요")
    );
    invoke(&main, "allow_workspace", json!({"path":root})).unwrap();
    assert!(invoke(&main, "sidecar_call", json!({"argv":[42]})).is_err());
    let out = invoke(&main, "sidecar_call", json!({"argv":["list-docs"]})).unwrap();
    assert_eq!(out["status"], 0);
    let notes: Value = serde_json::from_str(out["stdout"].as_str().unwrap()).unwrap();
    assert_eq!(notes.as_array().unwrap().len(), 1);
    let out = invoke(
        &main,
        "sidecar_call",
        json!({"argv":["get-doc","ndfixture-00000001"]}),
    )
    .unwrap();
    assert_eq!(out["status"], 0);
    assert!(out["stdout"].as_str().unwrap().contains("실제 엔진"));
    let out = invoke(
        &main,
        "sidecar_call",
        json!({"argv":["get-doc","ndfixture-00000099"]}),
    )
    .unwrap();
    assert_eq!(out["status"], 3);
    assert_eq!(out["stdout"], "");
    assert!(out["stderr"].as_str().unwrap().starts_with("WL-404"));
    let out = invoke(&main, "sidecar_call", json!({"argv":["get-ask"]})).unwrap();
    assert_eq!(out["status"], 3);
    assert!(out["stderr"].as_str().unwrap().starts_with("WL-409"));
    let out = invoke(
        &main,
        "sidecar_call",
        json!({"argv":["set-favorite","ndfixture-00000001"]}),
    )
    .unwrap();
    assert_eq!(out["status"], 5);
    assert!(out["stderr"].as_str().unwrap().starts_with("WL-412"));
    let out=invoke(&main,"sidecar_call",json!({"argv":["create_doc","{\"agenda\":\"demo\",\"type\":\"free\",\"title\":\"호스트 기록\",\"body\":\"새 글\"}"]})).unwrap();
    assert_eq!(out["status"], 0);
    let created: Value = serde_json::from_str(out["stdout"].as_str().unwrap()).unwrap();
    let record = fs::read_to_string(
        Path::new(&root)
            .join(created["path"].as_str().unwrap())
            .join("note.md"),
    )
    .unwrap();
    assert!(!record.contains("parent-session-should-not-leak"));
    assert!(std::env::var_os("CLAUDE_CODE_SESSION_ID").is_none());
}
struct Server {
    child: Child,
    socket: PathBuf,
}
impl Drop for Server {
    fn drop(&mut self) {
        let _ = Command::new("/bin/kill")
            .args(["-TERM", &self.child.id().to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                let _ = fs::remove_file(&self.socket);
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.socket);
    }
}
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
#[test]
#[cfg(unix)]
fn real_engine_round_trip_uses_isolated_home_and_socket() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::Builder::new()
        .prefix("wl-host-")
        .tempdir_in("/tmp")
        .unwrap();
    let home = temp.path().join("home");
    let vault = temp.path().join("vault");
    let socket = temp.path().join("engine.sock");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(vault.join(".withlog/agendas")).unwrap();
    fs::create_dir_all(vault.join("notes/demo/root")).unwrap();
    fs::write(vault.join(".withlog/workspace.json"), "{}").unwrap();
    fs::write(
        vault.join(".withlog/agendas/demo.md"),
        "---\nid: agfixture-00000001\nslug: demo\ntitle: 데모\n---\n",
    )
    .unwrap();
    fs::write(vault.join("notes/demo/root/note.md"),"---\nid: ndfixture-00000001\nagenda: demo\ntitle: 시험\ntype: free\ncreated: '2026-06-16T10:00:00+09:00'\n---\n실제 엔진\n").unwrap();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let cli = repo.join("dist/wl-cli.jar");
    let engine = repo.join("dist/wl-engine.jar");
    assert!(
        cli.is_file() && engine.is_file(),
        "run bash test-native.sh to build Java jars first"
    );
    let java = std::env::var_os("JAVA_HOME")
        .map(|v| PathBuf::from(v).join("bin/java"))
        .filter(|p| p.is_file())
        .or_else(|| {
            std::env::var_os("PATH").and_then(|path| {
                std::env::split_paths(&path)
                    .map(|p| p.join("java"))
                    .find(|p| p.is_file())
            })
        })
        .unwrap_or_else(|| PathBuf::from("/usr/bin/java"));
    let wrapper = temp.path().join("java");
    fs::write(
        &wrapper,
        format!(
            "#!/bin/sh\nexec {} {} \"$@\"\n",
            quote(java.to_str().unwrap()),
            quote(&format!("-Duser.home={}", home.display()))
        ),
    )
    .unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o700)).unwrap();
    let mut env: BTreeMap<OsString, OsString> = BTreeMap::new();
    for (key, value) in [
        ("HOME", home.as_os_str()),
        ("WITHLOG_VAULT", vault.as_os_str()),
        ("WITHLOG_SIDECAR_SOCKET", socket.as_os_str()),
        ("WITHLOG_CLI_JAR", cli.as_os_str()),
        ("WITHLOG_SIDECAR_JAVA", wrapper.as_os_str()),
    ] {
        env.insert(key.into(), value.into());
    }
    env.insert("PATH".into(), "/usr/bin:/bin".into());
    env.insert("WITHLOG_VAULT_AUTOCOMMIT_ENABLED".into(), "false".into());
    let error_log = temp.path().join("engine.err");
    let child = Command::new(&wrapper)
        .arg("-jar")
        .arg(&engine)
        .arg("serve")
        .env_clear()
        .envs(&env)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(fs::File::create(&error_log).unwrap())
        .spawn()
        .unwrap();
    let mut server = Server {
        child,
        socket: socket.clone(),
    };
    let deadline = Instant::now() + Duration::from_secs(15);
    while !socket.exists() && Instant::now() < deadline {
        assert!(
            server.child.try_wait().unwrap().is_none(),
            "engine failed: {}",
            fs::read_to_string(&error_log).unwrap()
        );
        thread::sleep(Duration::from_millis(25));
    }
    assert!(socket.exists(), "engine not ready");
    env.insert("CLAUDECODE".into(), "1".into());
    env.insert(
        "CLAUDE_CODE_SESSION_ID".into(),
        "parent-session-should-not-leak".into(),
    );
    let mut probe = Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "sidecar_integration_tests::ipc_probe",
            "--nocapture",
        ])
        .env_clear()
        .envs(environment::cleaned(env))
        .stdin(Stdio::null())
        .stdout(fs::File::create(temp.path().join("probe.out")).unwrap())
        .stderr(fs::File::create(temp.path().join("probe.err")).unwrap())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = probe.try_wait().unwrap() {
            assert!(
                status.success(),
                "IPC probe failed: {} {}",
                fs::read_to_string(temp.path().join("probe.out")).unwrap(),
                fs::read_to_string(temp.path().join("probe.err")).unwrap()
            );
            break;
        }
        if Instant::now() >= deadline {
            let _ = probe.kill();
            let _ = probe.wait();
            panic!("IPC probe timed out");
        }
        thread::sleep(Duration::from_millis(25));
    }
}
