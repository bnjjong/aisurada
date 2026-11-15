//! Isolated subprocess test: native IPC -> the in-process Rust ais-cli -> a temporary vault (no Java, no sidecar).
use crate::{environment, lifecycle::HostState, sidecar, workspace};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::Path,
    process::{Command, Stdio},
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
#[ignore = "invoked only by the isolated engine test"]
fn ipc_probe() {
    let root = std::env::var("AISURADA_VAULT").unwrap();
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
    assert!(out["stderr"].as_str().unwrap().starts_with("AIS-404"));
    // 문답 명령도 이 엔진이 한다(옛 4부 없는 판의 AIS-409가 아니다). 인자가 없으면 인자 오류다
    let out = invoke(&main, "sidecar_call", json!({"argv":["get-ask"]})).unwrap();
    assert_eq!(out["status"], 3);
    assert!(out["stderr"].as_str().unwrap().starts_with("AIS-400"), "{out}");
    let out = invoke(
        &main,
        "sidecar_call",
        json!({"argv":["set-favorite","ndfixture-00000001"]}),
    )
    .unwrap();
    assert_eq!(out["status"], 5);
    assert!(out["stderr"].as_str().unwrap().starts_with("AIS-412"));
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
#[test]
#[cfg(unix)]
fn real_engine_round_trip_uses_isolated_home() {
    let temp = tempfile::Builder::new()
        .prefix("ais-host-")
        .tempdir_in("/tmp")
        .unwrap();
    let home = temp.path().join("home");
    let vault = temp.path().join("vault");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(vault.join(".aisurada/agendas")).unwrap();
    fs::create_dir_all(vault.join("notes/demo/root")).unwrap();
    fs::write(vault.join(".aisurada/workspace.json"), "{}").unwrap();
    fs::write(
        vault.join(".aisurada/agendas/demo.md"),
        "---\nid: agfixture-00000001\nslug: demo\ntitle: 데모\n---\n",
    )
    .unwrap();
    fs::write(vault.join("notes/demo/root/note.md"),"---\nid: ndfixture-00000001\nagenda: demo\ntitle: 시험\ntype: free\ncreated: '2026-06-16T10:00:00+09:00'\n---\n실제 엔진\n").unwrap();
    let mut env: BTreeMap<OsString, OsString> = BTreeMap::new();
    for (key, value) in [("HOME", home.as_os_str()), ("AISURADA_VAULT", vault.as_os_str())] {
        env.insert(key.into(), value.into());
    }
    env.insert("PATH".into(), "/usr/bin:/bin".into());
    env.insert("AISURADA_VAULT_AUTOCOMMIT_ENABLED".into(), "false".into());
    // 앱을 AI 세션의 터미널에서 띄운 것처럼 세션 표식을 환경에 남겨 두면, 앱이 엔진에 넘기기 전에 지워야 한다
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
