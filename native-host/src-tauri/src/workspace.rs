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
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("폴더 경로가 비어있어요".into());
    }
    let p = Path::new(trimmed);
    if p == Path::new("/") || trimmed == "/" {
        return Err("루트 폴더는 워크스페이스로 열 수 없어요".into());
    }
    if let Ok(canon) = p.canonicalize() {
        if canon == Path::new("/") {
            return Err("루트 폴더는 워크스페이스로 열 수 없어요".into());
        }
        let s = canon.to_string_lossy();
        if s == "/System"
            || s.starts_with("/System/")
            || s == "/bin"
            || s.starts_with("/bin/")
            || s == "/sbin"
            || s.starts_with("/sbin/")
            || s == "/usr"
            || s.starts_with("/usr/")
            || s == "/etc"
            || s.starts_with("/etc/")
            || s == "/private/etc"
            || s.starts_with("/private/etc/")
        {
            return Err("시스템 폴더는 워크스페이스로 열 수 없어요".into());
        }
    }
    let mut current = current.lock().map_err(|_| "vault 잠금을 얻지 못했다")?;
    if has_notes() && current.as_ref().is_some_and(|old| !same_vault(old, &path)) {
        return Err("노트 창이 열려 있어서 vault 를 바꾸지 않았어요. 노트 창의 저장하지 못한 글을 정리하고 창을 닫은 뒤 다시 열어 주세요.".into());
    }
    allow(&path)?;
    *current = Some(path);
    Ok(())
}

/// 앱이 지금 연 vault를 파일로 적어 둔다(`~/Library/Application Support/aisurada/current-vault.json`).
/// 에이전트의 MCP 서버와 `ais-cli`는 `AISURADA_VAULT`가 없으면 이 파일을 호출마다 읽어서 앱이 연 vault를 따라간다.
/// 엔진의 `VaultPointer`가 읽는 모양과 같다. 임시 파일에 쓰고 이름을 바꿔서 읽는 쪽이 반쯤 쓴 파일을 못 보게 한다.
pub(crate) fn write_vault_pointer(home: &Path, vault: &str) -> std::io::Result<()> {
    let dir = home.join("Library/Application Support/aisurada");
    std::fs::create_dir_all(&dir)?;
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let json = format!("{}\n", serde_json::json!({ "vault": vault, "updatedAt": millis }));
    let temp = dir.join("current-vault.json.tmp");
    std::fs::write(&temp, json)?;
    std::fs::rename(&temp, dir.join("current-vault.json"))
}

#[tauri::command]
pub fn allow_workspace<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, HostState>,
    path: String,
) -> Result<(), String> {
    let opened = path.clone();
    let result = switch_workspace(
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
                .allow_directory(format!("{path}/.aisurada"), true)
                .map_err(|e| e.to_string())?;
            app.asset_protocol_scope()
                .allow_directory(path, true)
                .map_err(|e| e.to_string())?;
            Ok(())
        },
    );
    // vault를 열었으면 파일에 적는다. 못 적어도 vault 열기는 막지 않는다. 시험은 사용자의 진짜 포인터를 덮어쓰지 않게 건너뛴다
    if result.is_ok() && !cfg!(test) {
        if let Some(home) = std::env::var_os("HOME") {
            if let Err(e) = write_vault_pointer(Path::new(&home), &opened) {
                eprintln!("앱이 연 vault를 파일에 적지 못했다: {e}");
            }
        }
    }
    result
}

#[tauri::command]
pub fn vault_root(state: State<'_, HostState>) -> Option<String> {
    state.vault.lock().unwrap().clone()
}

#[tauri::command]
pub fn is_workspace(root: String) -> bool {
    Path::new(&root).join(".aisurada/workspace.json").is_file()
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
    // 저장 위치가 아직 없으면 만든다. 없는 폴더를 적었다는 것은 거기에 두겠다는 뜻이다. 이름 검사가 모두 끝난
    // 뒤에 만들어서 잘못된 이름이 빈 폴더를 남기지 않는다. 있는 파일을 가리키면 아래 단계가 실패한다.
    if !parent_path.exists() {
        std::fs::create_dir_all(parent_path)
            .map_err(|e| format!("저장 위치를 만들 수 없어요: {parent} ({e})"))?;
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

    fn pointer_file(home: &Path) -> std::path::PathBuf {
        home.join("Library/Application Support/aisurada/current-vault.json")
    }

    #[test]
    fn the_opened_vault_is_written_where_the_engine_reads_it() {
        let home = tempfile::tempdir().unwrap();
        write_vault_pointer(home.path(), "/Users/me/df-ai/df-ai").unwrap();
        let first: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(pointer_file(home.path())).unwrap()).unwrap();
        assert_eq!(first["vault"], "/Users/me/df-ai/df-ai");
        assert!(first["updatedAt"].as_u64().unwrap() > 0);

        // 다른 vault를 열면 같은 파일이 새 값으로 바뀌고, 임시 파일은 남지 않는다
        write_vault_pointer(home.path(), "/Users/me/other vault").unwrap();
        let second: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(pointer_file(home.path())).unwrap()).unwrap();
        assert_eq!(second["vault"], "/Users/me/other vault", "경로의 공백도 그대로 간다");
        let leftovers: Vec<_> = std::fs::read_dir(pointer_file(home.path()).parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(leftovers, vec!["current-vault.json".to_string()]);
    }

    #[test]
    fn a_home_that_cannot_be_written_reports_an_error_instead_of_panicking() {
        let home = tempfile::tempdir().unwrap();
        let blocker = home.path().join("Library");
        std::fs::write(&blocker, "파일이 폴더 자리를 막고 있다").unwrap();
        assert!(write_vault_pointer(home.path(), "/v").is_err());
    }

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
    fn create_dir_makes_a_missing_parent_after_validating_the_name() {
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("new/parent");
        let parent_text = parent.to_string_lossy().into_owned();
        // 잘못된 이름은 저장 위치를 만들기 전에 거절하므로 빈 폴더를 남기지 않는다.
        assert!(create_workspace_dir(parent_text.clone(), "..".into()).is_err());
        assert!(create_workspace_dir(parent_text.clone(), ".hidden".into()).is_err());
        assert!(!parent.exists());
        let path = create_workspace_dir(parent_text.clone(), "df-ai".into()).unwrap();
        assert!(path.ends_with("/new/parent/df-ai"));
        assert!(Path::new(&path).is_dir());
        // 같은 요청을 다시 해도 그대로 성공한다.
        assert_eq!(
            create_workspace_dir(parent_text, "df-ai".into()).unwrap(),
            path
        );
        // 저장 위치가 파일이면 만들지 않고 실패한다.
        let file = temp.path().join("plain-file");
        std::fs::write(&file, b"data").unwrap();
        assert!(create_workspace_dir(file.to_string_lossy().into_owned(), "x".into()).is_err());
    }
    #[test]
    fn workspace_marker_must_be_a_file_and_is_not_parsed() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_string_lossy().into_owned();
        std::fs::create_dir(temp.path().join(".aisurada")).unwrap();
        let marker = temp.path().join(".aisurada/workspace.json");
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
            .is_allowed(temp.path().join(".aisurada/workspace.json")));
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
            headers.insert("x-ais-name", "%ED%95%9C.png".parse().unwrap());
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
