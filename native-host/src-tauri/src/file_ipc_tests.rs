//! Exercise the actual command adapter without opening external applications.
use crate::{files, ipc::SELF_WRITE, lifecycle::HostState, self_write};
use serde_json::{json, Value};
use std::{
    fs,
    sync::{Arc, Mutex},
};
use tauri::{
    ipc::{CallbackFn, InvokeBody},
    test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime},
    webview::InvokeRequest,
    Listener, Manager, WebviewWindow,
};
fn call(
    window: &WebviewWindow<MockRuntime>,
    name: &str,
    body: InvokeBody,
    headers: tauri::http::HeaderMap,
) -> Result<Value, Value> {
    get_ipc_response(
        window,
        InvokeRequest {
            cmd: name.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body,
            headers,
            invoke_key: tauri::test::INVOKE_KEY.into(),
        },
    )
    .map(|v| v.deserialize().unwrap())
}
fn headers(dir: &str, name: &str) -> tauri::http::HeaderMap {
    let mut headers = tauri::http::HeaderMap::new();
    headers.insert(
        "x-wl-note-dir",
        percent_encoding::utf8_percent_encode(dir, percent_encoding::NON_ALPHANUMERIC)
            .to_string()
            .parse()
            .unwrap(),
    );
    headers.insert(
        "x-wl-name",
        percent_encoding::utf8_percent_encode(name, percent_encoding::NON_ALPHANUMERIC)
            .to_string()
            .parse()
            .unwrap(),
    );
    headers
}
#[test]
fn attachments_decode_before_vault_check_and_support_both_windows_and_body_forms() {
    let app = mock_builder()
        .manage(HostState::default())
        .invoke_handler(tauri::generate_handler![
            files::write_attachment_bytes,
            files::open_in_default_app
        ])
        .build(mock_context(noop_assets()))
        .unwrap();
    let main = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let note = tauri::WebviewWindowBuilder::new(&app, "note-test", Default::default())
        .build()
        .unwrap();
    let error = call(
        &main,
        "write_attachment_bytes",
        InvokeBody::Raw(vec![]),
        Default::default(),
    )
    .unwrap_err();
    assert!(error.as_str().unwrap().contains("x-wl-note-dir"));
    let h = headers("/none", "a.png");
    let error = call(
        &main,
        "write_attachment_bytes",
        InvokeBody::Json(json!([256])),
        h.clone(),
    )
    .unwrap_err();
    assert_eq!(error, json!("본문이 바이트가 아니에요"));
    assert_eq!(
        call(&main, "write_attachment_bytes", InvokeBody::Raw(vec![]), h).unwrap_err(),
        json!("vault가 아직 열리지 않았어요")
    );
    assert_eq!(
        call(
            &main,
            "open_in_default_app",
            InvokeBody::Json(json!({"path":"/none"})),
            Default::default()
        )
        .unwrap_err(),
        json!("vault가 아직 열리지 않았어요")
    );
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().to_string_lossy().into_owned();
    let dir = temp.path().join("notes/일/노트/sub/child");
    fs::create_dir_all(&dir).unwrap();
    *app.state::<HostState>().vault.lock().unwrap() = Some(root.clone());
    for (window, body, name) in [
        (&main, InvokeBody::Raw(vec![0, 255, 7]), "한글.PNG"),
        (&note, InvokeBody::Json(json!([0, 255, 7])), "다른.png"),
    ] {
        let result = call(
            window,
            "write_attachment_bytes",
            body,
            headers(dir.to_str().unwrap(), name),
        )
        .unwrap();
        let relative = result.as_str().unwrap();
        assert!(relative.ends_with(name));
        assert_eq!(
            fs::read(temp.path().join(relative)).unwrap(),
            vec![0, 255, 7]
        );
    }
    let forbidden = dir.join("run.command");
    fs::write(&forbidden, b"never execute").unwrap();
    let error = call(
        &note,
        "open_in_default_app",
        InvokeBody::Json(json!({"path":forbidden.to_str().unwrap()})),
        Default::default(),
    )
    .unwrap_err();
    assert!(error.as_str().unwrap().contains("실행 파일"));
    let outside = tempfile::tempdir().unwrap();
    let error = call(
        &main,
        "open_in_default_app",
        InvokeBody::Json(json!({"path":outside.path().to_str().unwrap()})),
        Default::default(),
    )
    .unwrap_err();
    assert!(error.as_str().unwrap().contains("현재 vault 밖"));
}
#[test]
fn self_write_broadcasts_actual_sender_to_every_window_and_silently_drops_stale_roots() {
    let app = mock_builder()
        .manage(HostState::default())
        .invoke_handler(tauri::generate_handler![self_write::announce_self_write])
        .build(mock_context(noop_assets()))
        .unwrap();
    let main = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let note = tauri::WebviewWindowBuilder::new(&app, "note-id", Default::default())
        .build()
        .unwrap();
    let main_events = Arc::new(Mutex::new(Vec::<Value>::new()));
    let note_events = Arc::new(Mutex::new(Vec::<Value>::new()));
    for (window, events) in [(&main, main_events.clone()), (&note, note_events.clone())] {
        window.listen(SELF_WRITE, move |event| {
            events
                .lock()
                .unwrap()
                .push(serde_json::from_str(event.payload()).unwrap());
        });
    }
    let notify = |window, payload| {
        call(
            window,
            "announce_self_write",
            InvokeBody::Json(payload),
            Default::default(),
        )
    };
    assert_eq!(
        notify(&note, json!({"root":"/vault","relPath":"uncreated/place"})).unwrap(),
        Value::Null
    );
    assert!(main_events.lock().unwrap().is_empty());
    assert!(notify(&main, json!({"root":"/wrong","relPath":"../bad"})).is_err());
    *app.state::<HostState>().vault.lock().unwrap() = Some("/vault/".into());
    notify(&main, json!({"root":"/vault","relPath":"uncreated/place"})).unwrap();
    assert!(main_events.lock().unwrap().is_empty());
    assert_eq!(
        notify(
            &note,
            json!({"root":"/vault/","relPath":".withlog/facts/a.md","from":"main"})
        )
        .unwrap(),
        Value::Null
    );
    let expected = json!({"root":"/vault/","relPath":".withlog/facts/a.md","from":"note-id"});
    assert_eq!(*main_events.lock().unwrap(), vec![expected.clone()]);
    assert_eq!(*note_events.lock().unwrap(), vec![expected]);
    notify(
        &main,
        json!({"root":"/vault/","relPath":"notes/한글/없는 파일"}),
    )
    .unwrap();
    assert_eq!(main_events.lock().unwrap().len(), 2);
    assert_eq!(note_events.lock().unwrap()[1]["from"], "main");
    assert!(notify(&main, json!({"root":"/vault/","relPath":"한".repeat(683)})).is_err());
    assert_eq!(main_events.lock().unwrap().len(), 2);
}
