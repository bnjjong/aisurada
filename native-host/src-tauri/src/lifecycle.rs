use std::{
    collections::HashMap,
    path::PathBuf,
    process::Child,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
};
use tauri::{Emitter, Manager};

#[derive(Default)]
pub struct HostState {
    pub vault: Mutex<Option<PathBuf>>,
    pub terminals: Mutex<HashMap<String, Child>>,
    pub quitting: AtomicBool,
    pub note_windows: Mutex<Vec<String>>,
    pub positions: Mutex<HashMap<String, serde_json::Value>>,
}

impl HostState {
    pub fn page_loaded(&self, label: &str) {
        if label == "main" {
            self.reap_terminals();
        }
    }

    pub fn reap_terminals(&self) {
        let children = std::mem::take(&mut *self.terminals.lock().unwrap());
        for (_, mut child) in children {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

pub struct SecretTable {
    pub text: &'static str,
    pub source: &'static str,
}
impl Default for SecretTable {
    fn default() -> Self {
        Self {
            text: include_str!(env!("WITHLOG_SECRET_TABLE")),
            source: env!("WITHLOG_SECRET_TABLE_SOURCE"),
        }
    }
}

pub fn bring_main_forward(app: &tauri::AppHandle) {
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
                state
                    .note_windows
                    .lock()
                    .unwrap()
                    .retain(|label| label != window.label());
            }
            let mut windows = app
                .webview_windows()
                .keys()
                .filter(|s| s.as_str() != window.label())
                .filter_map(|s| s.strip_prefix("note-").map(str::to_owned))
                .collect::<Vec<_>>();
            windows.sort();
            let _ = app.emit("wl://note-windows", windows);
        }
        tauri::WindowEvent::Moved(_)
        | tauri::WindowEvent::Resized(_)
        | tauri::WindowEvent::Focused(_) => {
            if let (Ok(size), Ok(position), Ok(scale)) = (
                window.inner_size(),
                window.inner_position(),
                window.scale_factor(),
            ) {
                let value = serde_json::json!({"width":size.width as f64 / scale,"height":size.height as f64 / scale,"x":position.x as f64 / scale,"y":position.y as f64 / scale});
                state
                    .positions
                    .lock()
                    .unwrap()
                    .insert(window.label().into(), value.clone());
                if window.label() == "main" {
                    let _ = save_main(app, &value);
                }
            }
        }
        _ => {}
    }
}

fn main_file(app: &tauri::AppHandle) -> Result<PathBuf, tauri::Error> {
    Ok(app.path().app_data_dir()?.join(if cfg!(debug_assertions) {
        "main-window.dev.json"
    } else {
        "main-window.json"
    }))
}
fn save_main(
    app: &tauri::AppHandle,
    value: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = main_file(app)?;
    std::fs::create_dir_all(path.parent().unwrap())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

pub fn create_main(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .ok_or("tauri.conf.json에 main 창 설정이 없다")?;
    let mut builder = tauri::WebviewWindowBuilder::from_config(app, config)?.center();
    if let Ok(bytes) = std::fs::read(main_file(app)?) {
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            if let (Some(x), Some(y), Some(w), Some(h)) = (
                v["x"].as_f64(),
                v["y"].as_f64(),
                v["width"].as_f64(),
                v["height"].as_f64(),
            ) {
                if [x, y, w, h].iter().all(|v| v.is_finite()) {
                    for monitor in app.available_monitors()? {
                        let scale = monitor.scale_factor();
                        let p = monitor.position();
                        let s = monitor.size();
                        let (mx, my, mw, mh) = (
                            p.x as f64 / scale,
                            p.y as f64 / scale,
                            s.width as f64 / scale,
                            s.height as f64 / scale,
                        );
                        if x >= mx && y >= my && x < mx + mw && y < my + mh {
                            let w = w.max(520.).min(mw);
                            let h = h.max(420.).min(mh);
                            builder = builder
                                .inner_size(w, h)
                                .position(x.min(mx + mw - w), y.min(my + mh - h));
                            break;
                        }
                    }
                }
            }
        }
    }
    builder.build()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initial_state_has_no_vault_and_no_terminals() {
        let state = HostState::default();
        assert!(state.vault.lock().unwrap().is_none());
        assert!(state.terminals.lock().unwrap().is_empty());
        assert!(!state.quitting.load(Ordering::SeqCst));
    }
    #[test]
    fn cleanup_reaps_real_child_and_is_repeatable() {
        let state = HostState::default();
        let child = std::process::Command::new("/bin/sleep")
            .arg("30")
            .spawn()
            .unwrap();
        state.terminals.lock().unwrap().insert("test".into(), child);
        state.page_loaded("note-test");
        assert!(state
            .terminals
            .lock()
            .unwrap()
            .get_mut("test")
            .unwrap()
            .try_wait()
            .unwrap()
            .is_none());
        state.page_loaded("main");
        state.page_loaded("main");
        assert!(state.terminals.lock().unwrap().is_empty());
    }
    #[test]
    fn public_table_is_sample_and_empty() {
        let table = SecretTable::default();
        if table.source == "sample" {
            assert!(table
                .text
                .lines()
                .all(|line| line.trim().is_empty() || line.starts_with('#')));
        }
    }
}
