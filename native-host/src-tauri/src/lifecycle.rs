use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Mutex,
    },
};
use tauri::{Manager, Runtime};
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
        tauri::WindowEvent::CloseRequested { .. } if window.label() == "main" => {
            state.reap_terminals();
            state.quitting.store(true, Ordering::SeqCst);
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
}
