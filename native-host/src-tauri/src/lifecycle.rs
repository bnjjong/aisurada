use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex,
    },
};
use tauri::{Emitter, Manager, Runtime};
#[derive(Default)]
pub struct HostState {
    pub vault: Mutex<Option<String>>,
    pub terminals: crate::terminal::Registry,
    pub quitting: AtomicBool,
    pub open_lock: Mutex<()>,
    pub note_file_lock: Mutex<()>,
    pub layout_lock: Mutex<()>,
    pub positions: Mutex<HashMap<String, u64>>,
    pub layout_generation: AtomicU64,
}
impl HostState {
    pub fn page_loaded(&self, label: &str) {
        if label == "main" {
            self.reap_terminals();
        }
    }
    pub fn reap_terminals(&self) {
        self.terminals.reap();
    }
}
pub fn bring_main_forward<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(main) = app.get_webview_window("main") {
        let _ = main.unminimize();
        let _ = main.show();
        let _ = main.set_focus();
    }
}
pub fn window_event(window: &tauri::Window, event: &tauri::WindowEvent) {
    let app = window.app_handle();
    let state = app.state::<HostState>();
    match event {
        tauri::WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
            if !state.quitting.load(Ordering::SeqCst) {
                api.prevent_close();
                let _ = window.emit("ais_close_requested", ());
                return;
            }
            state.reap_terminals();
            for (label, note) in app.webview_windows() {
                if label.starts_with("note-") {
                    let _ = note.close();
                }
            }
        }
        tauri::WindowEvent::Destroyed if window.label().starts_with("note-") => {
            if !state.quitting.load(Ordering::SeqCst) {
                crate::notes::forget(app, window.label().strip_prefix("note-").unwrap());
            }
            crate::notes::broadcast(app, Some(window.label()));
        }
        tauri::WindowEvent::Moved(_)
        | tauri::WindowEvent::Resized(_)
        | tauri::WindowEvent::Focused(_) => crate::layout::schedule(window),
        _ => {}
    }
}
pub fn run_event<R: Runtime>(app: &tauri::AppHandle<R>, event: tauri::RunEvent) {
    match event {
        tauri::RunEvent::ExitRequested { api, .. } => {
            let state = app.state::<HostState>();
            if !state.quitting.load(Ordering::SeqCst) {
                api.prevent_exit();
                if let Some(main) = app.get_webview_window("main") {
                    let _ = main.emit("ais_close_requested", ());
                } else {
                    exit_app(app.clone());
                }
            }
        }
        _ => {}
    }
}
#[tauri::command]
pub fn exit_app<R: Runtime>(app: tauri::AppHandle<R>) {
    let state = app.state::<HostState>();
    state.quitting.store(true, Ordering::SeqCst);
    state.reap_terminals();
    for (label, note) in app.webview_windows() {
        if label.starts_with("note-") {
            let _ = note.close();
        }
    }
    app.exit(0);
}
pub fn is_allowed_navigation(url: &tauri::Url) -> bool {
    match url.scheme() {
        "tauri" => true,
        "https" => url.host_str() == Some("tauri.localhost"),
        "http" => matches!(url.host_str(), Some("localhost") | Some("127.0.0.1")),
        "asset" => matches!(url.host_str(), Some("localhost")),
        "aisframe" => true,
        "data" | "about" => true,
        _ => false,
    }
}

pub fn create_main(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .ok_or("tauri.conf.json에 main 창 설정이 없다")?;
    let ms = crate::layout::monitors_from(app.available_monitors()?);
    let remembered = crate::layout::remembered(app, &ms, "main");
    let fallback = crate::layout::file(app, "main-window")
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice::<crate::layout::Geometry>(&b).ok());
    let mut builder = tauri::WebviewWindowBuilder::from_config(app, config)?.center();
    if let Some(g) = remembered.as_ref().map(|p| p.0.clone()).or(fallback) {
        let g = crate::layout::place(Some(g), &ms, 0);
        if let (Some(x), Some(y)) = (g.x, g.y) {
            builder = builder.inner_size(g.width, g.height).position(x, y);
        }
    }
    let builder = builder.on_navigation(|url| is_allowed_navigation(url));
    builder.build()?;
    if let Some((_, l)) = remembered {
        crate::desktop::restore(app, "main", l, ms);
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initial_state_has_no_vault_and_cleanup_is_repeatable() {
        let s = HostState::default();
        assert!(s.vault.lock().unwrap().is_none());
        assert!(!s.quitting.load(Ordering::SeqCst));
        s.page_loaded("note-x");
        s.page_loaded("main");
        s.page_loaded("main");
    }

    #[test]
    fn navigation_allows_local_and_aisframe_and_blocks_spoofing() {
        use tauri::Url;
        assert!(is_allowed_navigation(&Url::parse("aisframe://localhost/attachment/index.html").unwrap()));
        assert!(is_allowed_navigation(&Url::parse("http://127.0.0.1:1420/").unwrap()));
        assert!(is_allowed_navigation(&Url::parse("http://localhost:1420/").unwrap()));
        assert!(is_allowed_navigation(&Url::parse("https://tauri.localhost/").unwrap()));
        assert!(is_allowed_navigation(&Url::parse("tauri://localhost/").unwrap()));
        assert!(is_allowed_navigation(&Url::parse("asset://localhost/path").unwrap()));
        assert!(is_allowed_navigation(&Url::parse("about:blank").unwrap()));
        assert!(is_allowed_navigation(&Url::parse("data:text/html,test").unwrap()));

        // 스푸핑 및 외부 URL 차단
        assert!(!is_allowed_navigation(&Url::parse("http://localhost@evil.com/").unwrap()));
        assert!(!is_allowed_navigation(&Url::parse("http://localhost.evil.com/").unwrap()));
        assert!(!is_allowed_navigation(&Url::parse("https://evil.com/").unwrap()));
        assert!(!is_allowed_navigation(&Url::parse("http://192.168.1.1/").unwrap()));
    }
}
