//! Chapter 12: native note windows, caller-bound operations and restore records.
use crate::{
    ipc,
    layout::{self, Geometry},
    lifecycle::{self, HostState},
    workspace,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tauri::{Emitter, Manager, Runtime};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Saved {
    pub id: String,
    pub geometry: Geometry,
}
pub type SavedWindows = BTreeMap<String, Vec<Saved>>;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Doc {
    pub doc_id: String,
    pub title: Option<String>,
}
pub fn label(id: &str) -> Result<String, String> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(format!("노트 id 가 아니다: {id}"));
    }
    Ok(format!("note-{id}"))
}
pub fn key(saved: &SavedWindows, vault: &str) -> String {
    saved
        .keys()
        .find(|k| workspace::same_vault(k, vault))
        .cloned()
        .unwrap_or_else(|| vault.into())
}
fn change<R: Runtime>(app: &tauri::AppHandle<R>, f: impl FnOnce(&mut SavedWindows)) {
    let state = app.state::<HostState>();
    let _lock = state.note_file_lock.lock().unwrap();
    if let Some(path) = layout::file(app, "note-windows") {
        let mut saved = layout::load(&path);
        f(&mut saved);
        if let Err(e) = layout::write(&path, &saved) {
            eprintln!("노트 창 목록을 못 적었다: {e}");
        }
    }
}
pub fn forget<R: Runtime>(app: &tauri::AppHandle<R>, id: &str) {
    change(app, |saved| {
        for entries in saved.values_mut() {
            entries.retain(|e| e.id != id);
        }
        saved.retain(|_, v| !v.is_empty());
    })
}
fn record<R: Runtime>(app: &tauri::AppHandle<R>, id: &str, g: Geometry) {
    let Some(vault) = app.state::<HostState>().vault.lock().unwrap().clone() else {
        return;
    };
    change(app, |saved| {
        let k = key(saved, &vault);
        let entries = saved.entry(k).or_default();
        if let Some(e) = entries.iter_mut().find(|e| e.id == id) {
            e.geometry = g
        } else {
            entries.push(Saved {
                id: id.into(),
                geometry: g,
            });
        }
    })
}
fn saved<R: Runtime>(app: &tauri::AppHandle<R>) -> Vec<Saved> {
    let state = app.state::<HostState>();
    let Some(vault) = state.vault.lock().unwrap().clone() else {
        return vec![];
    };
    let _lock = state.note_file_lock.lock().unwrap();
    let Some(path) = layout::file(app, "note-windows") else {
        return vec![];
    };
    let all: SavedWindows = layout::load(&path);
    all.get(&key(&all, &vault)).cloned().unwrap_or_default()
}
pub fn broadcast<R: Runtime>(app: &tauri::AppHandle<R>, exclude: Option<&str>) {
    let ids = ids(app, exclude);
    let _ = app.emit(ipc::NOTE_WINDOWS, ids);
}
fn ids<R: Runtime>(app: &tauri::AppHandle<R>, exclude: Option<&str>) -> Vec<String> {
    let mut ids = app
        .webview_windows()
        .keys()
        .filter(|l| Some(l.as_str()) != exclude)
        .filter_map(|l| l.strip_prefix("note-").map(str::to_owned))
        .collect::<Vec<_>>();
    ids.sort();
    ids
}
pub fn on_main<R: Runtime, T: Send + 'static>(
    app: &tauri::AppHandle<R>,
    f: impl FnOnce(tauri::AppHandle<R>) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let a = app.clone();
    app.run_on_main_thread(move || {
        let _ = tx.send(f(a));
    })
    .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?
}
fn create<R: Runtime>(
    app: &tauri::AppHandle<R>,
    id: &str,
    title: Option<&str>,
    g: Geometry,
    focus: bool,
) -> Result<tauri::WebviewWindow<R>, String> {
    let mut b = tauri::WebviewWindowBuilder::new(
        app,
        label(id)?,
        tauri::WebviewUrl::App(format!("note-window/{id}").into()),
    )
    .title(title.filter(|t| !t.trim().is_empty()).unwrap_or("withlog"))
    .min_inner_size(520., 420.)
    .inner_size(g.width, g.height)
    .focused(focus);
    if let (Some(x), Some(y)) = (g.x, g.y) {
        b = b.position(x, y)
    } else {
        b = b.center()
    };
    b.build().map_err(|e| e.to_string())
}
#[tauri::command]
pub async fn open_note_window<R: Runtime>(
    app: tauri::AppHandle<R>,
    doc_id: String,
    title: Option<String>,
    geometry: Option<Geometry>,
) -> Result<(), String> {
    let name = label(&doc_id)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<HostState>();
        let _lock = state.open_lock.lock().map_err(|e| e.to_string())?;
        let a = app.clone();
        on_main(&a, move |app| {
            if let Some(w) = app.get_webview_window(&name) {
                w.unminimize().map_err(|e| e.to_string())?;
                w.set_focus().map_err(|e| e.to_string())?;
                return Ok(());
            }
            let ms = layout::monitors(&app);
            let g = layout::place(
                layout::remembered(&app, &ms, "note:last")
                    .map(|v| v.0)
                    .or(geometry),
                &ms,
                ids(&app, None).len(),
            );
            let w = create(&app, &doc_id, title.as_deref(), g.clone(), true)?;
            record(&app, &doc_id, layout::geometry(&w).unwrap_or(g));
            broadcast(&app, None);
            Ok(())
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn note_windows<R: Runtime>(app: tauri::AppHandle<R>) -> Vec<String> {
    ids(&app, None)
}
#[tauri::command]
pub fn close_note_windows<R: Runtime>(app: tauri::AppHandle<R>) {
    for w in app
        .webview_windows()
        .values()
        .filter(|w| w.label().starts_with("note-"))
    {
        let _ = w.close();
    }
}
#[tauri::command]
pub fn destroy_current_window<R: Runtime>(window: tauri::WebviewWindow<R>) -> Result<(), String> {
    if !window.label().starts_with("note-") {
        return Err("노트 창만 이 길로 닫는다".into());
    }
    window.destroy().map_err(|e| e.to_string())
}
#[tauri::command]
pub fn saved_note_windows<R: Runtime>(app: tauri::AppHandle<R>) -> Vec<String> {
    saved(&app).into_iter().map(|e| e.id).collect()
}
#[tauri::command]
pub fn remember_note_window<R: Runtime>(window: tauri::WebviewWindow<R>, geometry: Geometry) {
    let Some(id) = window.label().strip_prefix("note-") else {
        return;
    };
    change(window.app_handle(), |saved| {
        for entries in saved.values_mut() {
            for e in entries.iter_mut().filter(|e| e.id == id) {
                e.geometry = geometry.clone();
            }
        }
    });
}
#[tauri::command]
pub async fn restore_note_windows<R: Runtime>(
    app: tauri::AppHandle<R>,
    docs: Vec<Doc>,
) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<HostState>();
        let _lock = state.open_lock.lock().map_err(|e| e.to_string())?;
        let saved = saved(&app);
        let a = app.clone();
        on_main(&a, move |app| {
            let ms = layout::monitors(&app);
            let mut opened = Vec::new();
            for doc in docs {
                let Ok(name) = label(&doc.doc_id) else {
                    continue;
                };
                let Some(entry) = saved.iter().find(|e| e.id == doc.doc_id) else {
                    continue;
                };
                if app.get_webview_window(&name).is_some() {
                    continue;
                }
                let remembered = layout::remembered(&app, &ms, &format!("note:{}", doc.doc_id));
                let g = layout::place(
                    remembered
                        .as_ref()
                        .map(|v| v.0.clone())
                        .or_else(|| Some(entry.geometry.clone())),
                    &ms,
                    0,
                );
                match create(&app, &doc.doc_id, doc.title.as_deref(), g.clone(), false) {
                    Ok(w) => {
                        record(&app, &doc.doc_id, layout::geometry(&w).unwrap_or(g));
                        if let Some((_, l)) = remembered {
                            crate::desktop::restore(&app, &name, l, ms.clone());
                        }
                        opened.push(doc.doc_id)
                    }
                    Err(e) => eprintln!("노트 창을 못 되살렸다: {e}"),
                }
            }
            if !opened.is_empty() {
                broadcast(&app, None);
                lifecycle::bring_main_forward(&app);
            }
            Ok(opened)
        })
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn show_in_main<R: Runtime>(
    app: tauri::AppHandle<R>,
    path: String,
    rail: Option<String>,
    sub: Option<String>,
) -> Result<(), String> {
    if !valid_route(&path) {
        return Err(format!("main 에 넘길 경로가 아니다: {path}"));
    }
    ipc::emit_window(
        &app,
        "main",
        ipc::NAVIGATE,
        ipc::Navigate { path, rail, sub },
    )
    .map_err(|e| e.to_string())?;
    lifecycle::bring_main_forward(&app);
    Ok(())
}
pub fn valid_route(path: &str) -> bool {
    path.starts_with('/')
        && !path.starts_with("//")
        && path != "/note-window"
        && !path.starts_with("/note-window/")
        && !path.starts_with("/note-window?")
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn labels_and_routes() {
        for id in ["", "a/b", "한글", "a:b"] {
            assert!(label(id).is_err())
        }
        assert!(label(&"a".repeat(64)).is_ok());
        assert!(label(&"a".repeat(65)).is_err());
        for p in [
            "https://x",
            "//x",
            "/note-window",
            "/note-window/x",
            "/note-window?x",
        ] {
            assert!(!valid_route(p))
        }
        assert!(valid_route("/doc/x?sub=y"));
    }
    #[test]
    fn restore_keys_keep_first_existing_spelling() {
        let t = tempfile::tempdir().unwrap();
        let s = t.path().to_str().unwrap();
        let raw = format!("{s}//");
        let mut all = SavedWindows::new();
        all.insert(raw.clone(), vec![]);
        assert_eq!(key(&all, s), raw);
    }
}
