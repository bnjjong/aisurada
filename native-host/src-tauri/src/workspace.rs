use crate::lifecycle::HostState;
use std::{
    path::{Component, Path},
    sync::Mutex,
};
use tauri::{Manager, Runtime, State, WebviewWindow};
use tauri_plugin_fs::FsExt;

pub(crate) fn same_vault(a: &str, b: &str) -> bool {
    let (a, b) = (Path::new(a), Path::new(b));
    if a == b {
        return true;
    }
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn switch_workspace(
    caller: &str,
    current: &Mutex<Option<String>>,
    path: String,
    has_notes: impl FnOnce() -> bool,
    allow: impl FnOnce(&str) -> Result<(), String>,
) -> Result<(), String> {
    if caller != "main" {
        return Err("노트 창은 vault 를 바꾸지 않는다".into());
    }
    let mut current = current.lock().map_err(|_| "vault 잠금을 얻지 못했다")?;
    if has_notes() && current.as_ref().is_some_and(|old| !same_vault(old, &path)) {
        return Err("노트 창이 열려 있어서 vault 를 바꾸지 않았어요. 노트 창의 저장하지 못한 글을 정리하고 창을 닫은 뒤 다시 열어 주세요.".into());
    }
    allow(&path)?;
    *current = Some(path);
    Ok(())
}

#[tauri::command]
pub fn allow_workspace<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, HostState>,
    path: String,
) -> Result<(), String> {
    switch_workspace(
        window.label(),
        &state.vault,
        path,
        || {
            window
                .app_handle()
                .webview_windows()
                .keys()
                .any(|label| label.starts_with("note-"))
        },
        |path| {
            let app = window.app_handle();
            app.fs_scope()
                .allow_directory(path, true)
                .map_err(|e| e.to_string())?;
            app.fs_scope()
                .allow_directory(format!("{path}/.withlog"), true)
                .map_err(|e| e.to_string())?;
            app.asset_protocol_scope()
                .allow_directory(path, true)
                .map_err(|e| e.to_string())?;
            Ok(())
        },
    )
}

#[tauri::command]
pub fn vault_root(state: State<'_, HostState>) -> Option<String> {
    state.vault.lock().unwrap().clone()
}

#[tauri::command]
pub fn is_workspace(root: String) -> bool {
    Path::new(&root).join(".withlog/workspace.json").is_file()
}

#[tauri::command]
pub fn create_workspace_dir(parent: String, name: String) -> Result<String, String> {
    let parent_path = Path::new(&parent);
    if !parent_path.is_absolute() {
        return Err(format!("저장 위치가 절대경로가 아니에요: {parent}"));
    }
    let mut components = Path::new(&name).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(format!(
            "워크스페이스 이름은 폴더 이름 하나여야 해요(빗금과 `..` 는 못 써요): {name}"
        ));
    }
    if name.starts_with('.') {
        return Err(format!(
            "워크스페이스 이름은 점으로 시작할 수 없어요: {name}"
        ));
    }
    let canonical = parent_path
        .canonicalize()
        .map_err(|e| format!("저장 위치를 찾을 수 없어요: {parent} ({e})"))?;
    let target = canonical.join(&name);
    if !target.starts_with(&canonical) {
        return Err(format!(
            "만들려는 폴더가 저장 위치 밖이에요: {}",
            target.display()
        ));
    }
    // Use the validated components for I/O; preserve the received suffix in the reply.
    // This also creates a missing `a/.` directory on current Rust versions.
    std::fs::create_dir_all(target.components().collect::<std::path::PathBuf>())
        .map_err(|e| format!("워크스페이스 폴더를 못 만들었어요: {e}"))?;
    Ok(target.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn switching_is_guarded_transactional_and_keeps_raw_spelling() {
        let current = Mutex::new(None);
        assert!(switch_workspace(
            "note-x",
            &current,
            "/x".into(),
            || false,
            |_| panic!("scope opened")
        )
        .is_err());
        switch_workspace("main", &current, "/a".into(), || true, |_| Ok(())).unwrap();
        assert!(switch_workspace(
            "main",
            &current,
            "/b".into(),
            || true,
            |_| panic!("scope opened")
        )
        .is_err());
        assert_eq!(current.lock().unwrap().as_deref(), Some("/a"));
        switch_workspace("main", &current, "/a//".into(), || true, |_| Ok(())).unwrap();
        assert_eq!(current.lock().unwrap().as_deref(), Some("/a//"));
        assert!(switch_workspace(
            "main",
            &current,
            "/b".into(),
            || false,
            |_| Err("scope failed".into())
        )
        .is_err());
        assert_eq!(current.lock().unwrap().as_deref(), Some("/a//"));
    }
    #[test]
    fn same_vault_follows_links_but_missing_different_paths_do_not_match() {
        let temp = tempfile::tempdir().unwrap();
        let alias = temp.path().join("alias");
        #[cfg(unix)]
        std::os::unix::fs::symlink(temp.path(), &alias).unwrap();
        #[cfg(unix)]
        assert!(same_vault(
            temp.path().to_str().unwrap(),
            alias.to_str().unwrap()
        ));
        assert!(!same_vault("/does-not-exist-a", "/does-not-exist-b"));
    }
    #[test]
    fn create_dir_preserves_component_quirks_and_validation_order() {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().to_string_lossy().into_owned();
        for name in ["a/", "b//", "c/.", " spaced ", "한글"] {
            let path = create_workspace_dir(parent.clone(), name.into()).unwrap();
            assert!(path.ends_with(name));
            assert!(Path::new(&path).is_dir());
            assert_eq!(
                create_workspace_dir(parent.clone(), name.into()).unwrap(),
                path
            );
            assert!(!is_workspace(path));
        }
        for name in ["", ".", "..", "./a", "a/b", "/x"] {
            assert!(create_workspace_dir(parent.clone(), name.into())
                .unwrap_err()
                .contains("폴더 이름 하나"));
        }
        assert!(create_workspace_dir("relative".into(), "..".into())
            .unwrap_err()
            .contains("절대경로"));
        assert!(create_workspace_dir(parent.clone(), ".hidden".into())
            .unwrap_err()
            .contains("점으로 시작"));
        std::fs::write(temp.path().join("file"), b"data").unwrap();
        assert!(create_workspace_dir(parent, "file".into()).is_err());
    }
    #[test]
    fn workspace_marker_must_be_a_file_and_is_not_parsed() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_string_lossy().into_owned();
        std::fs::create_dir(temp.path().join(".withlog")).unwrap();
        let marker = temp.path().join(".withlog/workspace.json");
        std::fs::create_dir(&marker).unwrap();
        assert!(!is_workspace(root.clone()));
        std::fs::remove_dir(&marker).unwrap();
        std::fs::write(&marker, b"invalid JSON").unwrap();
        assert!(is_workspace(root));
    }
}

#[cfg(test)]
mod ipc_tests {
    use super::*;
    use serde_json::{json, Value};
    use tauri::{
        ipc::{CallbackFn, InvokeBody},
        test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime},
        webview::InvokeRequest,
    };

    #[tauri::command]
    fn camel_case(probe_key: String) -> String {
        probe_key
    }
    #[tauri::command]
    async fn attachment_transport(request: tauri::ipc::Request<'_>) -> Result<Value, String> {
        Ok(json!({"name":crate::ipc::header(&request,"name")?,"bytes":crate::ipc::body(&request)?}))
    }
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
        .map(|result| result.deserialize::<Value>().unwrap())
    }
    fn invoke(
        window: &WebviewWindow<MockRuntime>,
        name: &str,
        value: Value,
    ) -> Result<Value, Value> {
        call(window, name, InvokeBody::Json(value), Default::default())
    }
    #[test]
    fn real_tauri_ipc_guards_windows_and_extends_both_scopes() {
        let app = mock_builder()
            .plugin(tauri_plugin_fs::init())
            .manage(HostState::default())
            .invoke_handler(tauri::generate_handler![
                allow_workspace,
                vault_root,
                is_workspace,
                create_workspace_dir,
                camel_case,
                attachment_transport
            ])
            .build(mock_context(noop_assets()))
            .unwrap();
        let main = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let note = tauri::WebviewWindowBuilder::new(&app, "note-test", Default::default())
            .build()
            .unwrap();
        assert_eq!(invoke(&main, "vault_root", json!({})).unwrap(), Value::Null);
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_string_lossy().into_owned();
        assert!(invoke(&note, "allow_workspace", json!({"path":root})).is_err());
        assert_eq!(
            invoke(&main, "allow_workspace", json!({"path":root})).unwrap(),
            Value::Null
        );
        assert_eq!(invoke(&note, "vault_root", json!({})).unwrap(), json!(root));
        assert!(app
            .fs_scope()
            .is_allowed(temp.path().join("notes/a/note.md")));
        assert!(app
            .fs_scope()
            .is_allowed(temp.path().join(".withlog/workspace.json")));
        assert!(app
            .asset_protocol_scope()
            .is_allowed(temp.path().join("notes/a/image.png")));
        assert!(invoke(&main, "allow_workspace", json!({"path":"/other-vault"})).is_err());
        let spelled = format!("{root}//");
        invoke(&main, "allow_workspace", json!({"path":spelled})).unwrap();
        assert_eq!(
            invoke(&main, "vault_root", json!({})).unwrap(),
            json!(spelled)
        );
        // Missing/type-invalid inputs are rejected by Tauri before command execution.
        assert!(invoke(&main, "allow_workspace", json!({})).is_err());
        assert!(invoke(&main, "allow_workspace", json!({"path":42})).is_err());
        assert!(invoke(&main, "unknown", json!({}))
            .unwrap_err()
            .as_str()
            .unwrap()
            .contains("not found"));
        assert_eq!(
            invoke(&main, "camel_case", json!({"probeKey":"ok"})).unwrap(),
            json!("ok")
        );
        assert!(invoke(&main, "camel_case", json!({"probe_key":"wrong"})).is_err());
        assert_eq!(
            invoke(&main, "is_workspace", json!({"root":root})).unwrap(),
            json!(false)
        );
        let created = invoke(
            &note,
            "create_workspace_dir",
            json!({"parent":root,"name":"made-by-host"}),
        )
        .unwrap();
        assert!(Path::new(created.as_str().unwrap()).is_dir());
        for payload in [
            InvokeBody::Raw(vec![0, 255, 7]),
            InvokeBody::Json(json!([0, 255, 7])),
        ] {
            let mut headers = tauri::http::HeaderMap::new();
            headers.insert("x-wl-name", "%ED%95%9C.png".parse().unwrap());
            assert_eq!(
                call(&main, "attachment_transport", payload, headers).unwrap(),
                json!({"name":"한.png","bytes":[0,255,7]})
            );
        }
    }
    #[test]
    fn changing_workspace_does_not_revoke_old_scope_or_require_existence() {
        let app = mock_builder()
            .plugin(tauri_plugin_fs::init())
            .manage(HostState::default())
            .invoke_handler(tauri::generate_handler![allow_workspace, vault_root])
            .build(mock_context(noop_assets()))
            .unwrap();
        let main = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        for path in [a.path().to_path_buf(), b.path().join("absent")] {
            invoke(
                &main,
                "allow_workspace",
                json!({"path":path.to_str().unwrap()}),
            )
            .unwrap();
        }
        assert!(app.fs_scope().is_allowed(a.path().join("notes/a.md")));
        assert!(app
            .asset_protocol_scope()
            .is_allowed(a.path().join("notes/a.png")));
        assert_eq!(
            invoke(&main, "vault_root", json!({})).unwrap(),
            json!(b.path().join("absent").to_str().unwrap())
        );
    }
}
