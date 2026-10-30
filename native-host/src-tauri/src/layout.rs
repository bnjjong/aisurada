//! Chapters 11–13: shared geometry and atomic, monitor-specific persistence.
use crate::lifecycle::HostState;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::Duration,
};
use tauri::{Manager, Runtime};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Geometry {
    pub width: f64,
    pub height: f64,
    pub x: Option<f64>,
    pub y: Option<f64>,
}
#[derive(Clone, Debug)]
pub struct Monitor {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
impl Monitor {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Layout {
    pub monitor: String,
    pub dx: f64,
    pub dy: f64,
    pub width: f64,
    pub height: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop: Option<u64>,
}
pub type Layouts = BTreeMap<String, BTreeMap<String, Layout>>;
pub fn file<R: Runtime>(app: &tauri::AppHandle<R>, name: &str) -> Option<PathBuf> {
    Some(
        app.path()
            .app_data_dir()
            .ok()?
            .join(if cfg!(debug_assertions) {
                format!("{name}.dev.json")
            } else {
                format!("{name}.json")
            }),
    )
}
pub fn load<T: serde::de::DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
pub fn write<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    std::fs::create_dir_all(path.parent().ok_or("폴더가 없다")?).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(
        &tmp,
        serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::rename(tmp, path).map_err(|e| e.to_string())
}
pub fn monitors_from(list: Vec<tauri::Monitor>) -> Vec<Monitor> {
    list.into_iter()
        .map(|m| {
            let scale = m.scale_factor();
            let (p, s) = (m.position(), m.size());
            let (w, h) = (s.width as f64 / scale, s.height as f64 / scale);
            Monitor {
                id: format!(
                    "{}|{:.0}x{:.0}",
                    m.name().map(String::as_str).unwrap_or("Monitor"),
                    w.round(),
                    h.round()
                ),
                x: p.x as f64 / scale,
                y: p.y as f64 / scale,
                width: w,
                height: h,
            }
        })
        .collect()
}
pub fn monitors<R: Runtime>(app: &tauri::AppHandle<R>) -> Vec<Monitor> {
    app.get_webview_window("main")
        .and_then(|w| w.available_monitors().ok())
        .map(monitors_from)
        .unwrap_or_default()
}
pub fn environment(monitors: &[Monitor]) -> String {
    let mut ids = monitors.iter().map(|m| m.id.as_str()).collect::<Vec<_>>();
    ids.sort();
    ids.join(" + ")
}
pub fn place(g: Option<Geometry>, monitors: &[Monitor], count: usize) -> Geometry {
    let mut g = g
        .filter(|g| g.width.is_finite() && g.height.is_finite())
        .unwrap_or(Geometry {
            width: 900.,
            height: 820.,
            x: None,
            y: None,
        });
    g.width = g.width.max(520.);
    g.height = g.height.max(420.);
    if let (Some(x), Some(y)) = (g.x, g.y) {
        let (x, y) = (x + 28. * count as f64, y + 28. * count as f64);
        if let Some(m) = monitors.iter().find(|m| m.contains(x, y)) {
            g.width = g.width.min(m.width);
            g.height = g.height.min((m.height - 28.).max(0.));
            g.x = Some(x.clamp(m.x, m.x + m.width - g.width));
            g.y = Some(y.clamp(m.y + 28., m.y + m.height - g.height));
            return g;
        }
    }
    g.x = None;
    g.y = None;
    g
}
pub fn lookup(layouts: &Layouts, monitors: &[Monitor], slot: &str) -> Option<(Geometry, Layout)> {
    let l = layouts.get(&environment(monitors))?.get(slot)?;
    let m = monitors.iter().find(|m| m.id == l.monitor)?;
    let (x, y) = (m.x + l.dx, m.y + l.dy);
    if !m.contains(x, y) || ![l.width, l.height].iter().all(|v| v.is_finite()) {
        return None;
    }
    Some((
        Geometry {
            width: l.width,
            height: l.height,
            x: Some(x),
            y: Some(y),
        },
        l.clone(),
    ))
}
pub fn remembered<R: Runtime>(
    app: &tauri::AppHandle<R>,
    ms: &[Monitor],
    slot: &str,
) -> Option<(Geometry, Layout)> {
    let state = app.state::<HostState>();
    let _lock = state.layout_lock.lock().ok()?;
    lookup(&load(&file(app, "window-layouts")?), ms, slot)
}
pub fn geometry<R: Runtime>(window: &tauri::WebviewWindow<R>) -> Option<Geometry> {
    let scale = window.scale_factor().ok()?;
    let s = window.inner_size().ok()?;
    let p = window.inner_position().ok()?;
    Some(Geometry {
        width: s.width as f64 / scale,
        height: s.height as f64 / scale,
        x: Some(p.x as f64 / scale),
        y: Some(p.y as f64 / scale),
    })
}
pub fn update(
    layouts: &mut Layouts,
    ms: &[Monitor],
    slot: &str,
    g: &Geometry,
    desktop: Option<u64>,
) -> bool {
    let (Some(x), Some(y)) = (g.x, g.y) else {
        return false;
    };
    let Some(m) = ms.iter().find(|m| m.contains(x, y)) else {
        return false;
    };
    let slots = layouts.entry(environment(ms)).or_default();
    let old = slots.get(slot);
    let desktop = desktop.or_else(|| old.filter(|l| l.monitor == m.id).and_then(|l| l.desktop));
    let l = Layout {
        monitor: m.id.clone(),
        dx: x - m.x,
        dy: y - m.y,
        width: g.width,
        height: g.height,
        desktop,
    };
    let mut changed = old != Some(&l);
    slots.insert(slot.into(), l.clone());
    if slot.starts_with("note:") {
        let last = Layout { desktop: None, ..l };
        changed |= slots.get("note:last") != Some(&last);
        slots.insert("note:last".into(), last);
    }
    changed
}
pub fn schedule(window: &tauri::Window) {
    let app = window.app_handle().clone();
    let label = window.label().to_owned();
    if label != "main" && !label.starts_with("note-") {
        return;
    }
    let state = app.state::<HostState>();
    let generation = state.layout_generation.fetch_add(1, Ordering::Relaxed) + 1;
    state
        .positions
        .lock()
        .unwrap()
        .insert(label.clone(), generation);
    let number = crate::desktop::window_number(window);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(400));
        let state = app.state::<HostState>();
        if state.positions.lock().unwrap().get(&label) != Some(&generation) {
            return;
        }
        let Some(w) = app.get_webview_window(&label) else {
            return;
        };
        if w.is_fullscreen().unwrap_or(true) || w.is_minimized().unwrap_or(true) {
            return;
        }
        let Some(g) = geometry(&w) else { return };
        let ms = monitors(&app);
        let Some(m) = ms.iter().find(|m| m.contains(g.x.unwrap(), g.y.unwrap())) else {
            return;
        };
        let desktop = number.and_then(|n| crate::desktop::current(n, m));
        let _lock = state.layout_lock.lock().unwrap();
        if let Some(path) = file(&app, "window-layouts") {
            let mut layouts = load(&path);
            let slot = label
                .strip_prefix("note-")
                .map(|id| format!("note:{id}"))
                .unwrap_or_else(|| "main".into());
            if update(&mut layouts, &ms, &slot, &g, desktop) {
                if let Err(e) = write(&path, &layouts) {
                    eprintln!("창 자리를 못 적었다: {e}");
                }
            }
        }
        if label == "main" {
            if let Some(path) = file(&app, "main-window") {
                let _ = write(&path, &g);
            }
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    fn ms() -> Vec<Monitor> {
        vec![Monitor {
            id: "M|1000x800".into(),
            x: 0.,
            y: 0.,
            width: 1000.,
            height: 800.,
        }]
    }
    #[test]
    fn geometry_clamps_titlebar_and_retains_offscreen_note_size() {
        let g = Geometry {
            width: 1500.,
            height: 1000.,
            x: Some(900.),
            y: Some(700.),
        };
        assert_eq!(
            place(Some(g.clone()), &ms(), 0),
            Geometry {
                width: 1000.,
                height: 772.,
                x: Some(0.),
                y: Some(28.)
            }
        );
        let p = place(Some(g), &ms(), 4);
        assert_eq!(p.width, 1500.);
        assert_eq!(p.x, None);
    }
    #[test]
    fn layouts_preserve_desktop_only_on_same_monitor_and_last_omits_it() {
        let ms = ms();
        let g = Geometry {
            width: 600.,
            height: 500.,
            x: Some(10.),
            y: Some(30.),
        };
        let mut ls = Layouts::new();
        assert!(update(&mut ls, &ms, "note:x", &g, Some(2)));
        assert!(!update(&mut ls, &ms, "note:x", &g, None));
        assert_eq!(lookup(&ls, &ms, "note:x").unwrap().1.desktop, Some(2));
        assert_eq!(lookup(&ls, &ms, "note:last").unwrap().1.desktop, None);
        assert!(lookup(&ls, &[], "note:x").is_none());
    }
    #[test]
    fn persistence_is_pretty_atomic_and_corruption_is_empty() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("layouts.json");
        let mut ls = Layouts::new();
        update(
            &mut ls,
            &ms(),
            "main",
            &Geometry {
                width: 600.,
                height: 500.,
                x: Some(10.),
                y: Some(30.),
            },
            None,
        );
        write(&p, &ls).unwrap();
        assert!(std::fs::read_to_string(&p).unwrap().starts_with("{\n  "));
        assert_eq!(load::<Layouts>(&p), ls);
        assert!(!p.with_extension("json.tmp").exists());
        std::fs::write(&p, "bad").unwrap();
        assert!(load::<Layouts>(&p).is_empty());
    }
}
