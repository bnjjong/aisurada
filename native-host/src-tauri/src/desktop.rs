//! Optional yabai integration; no PATH lookup and never a condition of opening a window.
use crate::{
    layout::{Layout, Monitor},
    process,
};
use serde::Deserialize;
use std::{path::Path, process::Command, time::Duration};
use tauri::{Manager, Runtime};
#[derive(Deserialize)]
struct Frame {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}
#[derive(Deserialize)]
struct Display {
    index: u64,
    frame: Frame,
    spaces: Vec<u64>,
}
#[derive(Deserialize)]
struct Window {
    id: u64,
    space: u64,
    display: u64,
}
fn run(args: &[&str]) -> Option<Vec<u8>> {
    let path = ["/opt/homebrew/bin/yabai", "/usr/local/bin/yabai"]
        .into_iter()
        .find(|p| Path::new(p).exists())?;
    let mut cmd = Command::new(path);
    cmd.args(args);
    if std::env::var_os("USER").is_none() {
        if let Some(user) =
            std::env::var_os("HOME").and_then(|h| Path::new(&h).file_name().map(|s| s.to_owned()))
        {
            cmd.env("USER", user);
        }
    }
    let process::Outcome::Finished(result) =
        process::run(&mut cmd, None, Duration::from_secs(2)).ok()?
    else {
        return None;
    };
    if !result.status.success() {
        return None;
    }
    Some(result.stdout)
}
fn query(args: &[&str]) -> Option<serde_json::Value> {
    serde_json::from_slice(&run(args)?).ok()
}
fn displays() -> Option<Vec<Display>> {
    serde_json::from_value(query(&["-m", "query", "--displays"])?).ok()
}
fn query_window(id: u64) -> Option<Window> {
    serde_json::from_value(query(&[
        "-m",
        "query",
        "--windows",
        "--window",
        &id.to_string(),
    ])?)
    .ok()
}
fn matches(f: &Frame, m: &Monitor) -> bool {
    [(f.x, m.x), (f.y, m.y), (f.w, m.width), (f.h, m.height)]
        .iter()
        .all(|(a, b)| (a - b).abs() <= 1.)
}
pub fn current(id: u64, m: &Monitor) -> Option<u64> {
    let w = query_window(id)?;
    let ds = displays()?;
    let d = ds
        .iter()
        .find(|d| d.index == w.display && matches(&d.frame, m))?;
    d.spaces
        .iter()
        .position(|s| *s == w.space)
        .map(|i| i as u64 + 1)
}
#[cfg(target_os = "macos")]
pub fn window_number<R: Runtime>(window: &tauri::Window<R>) -> Option<u64> {
    use objc2::{msg_send, runtime::AnyObject};
    let ptr = window.ns_window().ok()? as *mut AnyObject;
    let n: isize = unsafe { msg_send![ptr, windowNumber] };
    (n > 0).then_some(n as u64)
}
#[cfg(not(target_os = "macos"))]
pub fn window_number<R: Runtime>(_: &tauri::Window<R>) -> Option<u64> {
    None
}
pub fn restore<R: Runtime>(app: &tauri::AppHandle<R>, label: &str, l: Layout, ms: Vec<Monitor>) {
    let Some(desktop) = l.desktop.filter(|d| *d > 0) else {
        return;
    };
    let app = app.clone();
    let label = label.to_owned();
    std::thread::spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let a = app.clone();
        let _ = app.run_on_main_thread(move || {
            let n = a
                .get_webview_window(&label)
                .and_then(|w| window_number(&w.as_ref().window()));
            let _ = tx.send(n);
        });
        let Some(id) = rx.recv_timeout(Duration::from_secs(2)).ok().flatten() else {
            return;
        };
        let Some(ds) = displays() else { return };
        let Some(m) = ms.iter().find(|m| m.id == l.monitor) else {
            return;
        };
        let Some(target) = ds
            .iter()
            .find(|d| matches(&d.frame, m))
            .and_then(|d| d.spaces.get(desktop as usize - 1))
            .copied()
        else {
            return;
        };
        for _ in 0..20 {
            if let Some(w) = query_window(id) {
                if w.id == id && w.space != target {
                    if run(&[
                        "-m",
                        "window",
                        &id.to_string(),
                        "--space",
                        &target.to_string(),
                    ])
                    .is_none()
                    {
                        eprintln!("창을 기억한 데스크톱으로 옮기지 못했다");
                    }
                }
                return;
            }
            std::thread::sleep(Duration::from_millis(150));
        }
    });
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn display_rect_has_one_pixel_tolerance() {
        let m = Monitor {
            id: "m".into(),
            x: 0.,
            y: 0.,
            width: 100.,
            height: 100.,
        };
        assert!(matches(
            &Frame {
                x: 1.,
                y: -1.,
                w: 100.,
                h: 100.
            },
            &m
        ));
        assert!(!matches(
            &Frame {
                x: 1.1,
                y: 0.,
                w: 100.,
                h: 100.
            },
            &m
        ));
    }
}
