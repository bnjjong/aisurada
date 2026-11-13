//! 태스크 시스템 2단계: 알림. 알림 목록, 채널 설정, macOS 배너를 이 모듈 한 곳에서 맡는다.
//! 러너(3단계)도 사람이 쓰는 화면도 같은 `push`를 거치므로 「어디로 보낼지」가 한 곳에서만 정해진다.
//!
//! - 목록은 vault마다 `~/Library/Application Support/aisurada/notifications/items-<vault 해시>.json`에, 설정은 모든 vault가 같이 쓰는
//!   `…/notifications/settings.json`에 둔다. vault 안에 두지 않는 까닭은 이 컴퓨터에서만 뜻이 있는 기록이고 vault의 git 이력을 부풀리지 않으려는 것이다.
//! - 앱 안 채널을 꺼도 알림은 목록(알림 센터)에 쌓인다. 끄는 것은 토스트와 배지의 소리·팝업이지 기록이 아니다.
//! - macOS 집중 모드는 OS가 처리한다(사용자 결정). 집중 모드에서 aisurada를 허용하지 않으면 OS가 배너를 막고 알림 센터에 쌓는다.
//!   앱이 집중 모드 상태를 읽어 토스트까지 숨기는 일은 하지 않는다.
use crate::{
    ipc::NOTIFICATIONS,
    lifecycle::HostState,
};
use ring::digest;
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Runtime, State};

/// 알림 종류. 화면의 소식별 켜고 끄기가 이 이름을 쓴다. `test`는 설정의 「시험 알림」이다.
pub const KINDS: [&str; 5] = ["question", "pr", "failed", "done", "test"];
/// 목록에 두는 알림의 수. 넘으면 오래된 것부터 버린다.
pub const MAX_ITEMS: usize = 200;
const MAX_TITLE: usize = 120;
const MAX_BODY: usize = 400;

/// 알림 하나.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    /// 눌렀을 때 열 태스크 키(`TSK-14`). 없으면 null
    #[serde(default)]
    pub task_key: Option<String>,
    /// 만든 시각(유닉스 밀리초)
    pub at: u64,
    pub read: bool,
}

/// 새 알림. `id`를 주면 같은 id의 알림이 이미 있을 때 새로 만들지 않는다(같은 일을 두 번 알리지 않으려는 것이다).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewItem {
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub task_key: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
}

/// 채널 하나(앱 안, macOS)를 켜는지.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub in_app: bool,
    pub macos: bool,
}

/// 소식별 켜고 끄기. 채널 전체가 꺼져 있으면 여기서 켜져 있어도 가지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Events {
    #[serde(default = "on_both")]
    pub question: Channel,
    #[serde(default = "on_both")]
    pub pr: Channel,
    #[serde(default = "on_both")]
    pub failed: Channel,
    #[serde(default = "done_default")]
    pub done: Channel,
}

fn on_both() -> Channel {
    Channel { in_app: true, macos: true }
}

/// Done은 조용히 알려도 된다(시안): 앱 안만 켜 둔다.
fn done_default() -> Channel {
    Channel { in_app: true, macos: false }
}
fn yes() -> bool {
    true
}

/// 알림 설정. 못 읽거나 칸이 모자라면 기본값으로 채운다.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// 앱 안(토스트, 배지) 채널
    #[serde(default = "yes")]
    pub in_app: bool,
    /// macOS 알림센터 채널
    #[serde(default = "yes")]
    pub macos: bool,
    #[serde(default)]
    pub events: Events,
}

impl Default for Events {
    fn default() -> Self {
        Events { question: on_both(), pr: on_both(), failed: on_both(), done: done_default() }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Settings { in_app: true, macos: true, events: Events::default() }
    }
}

/// 알림 하나가 어디로 가는지.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Route {
    pub toast: bool,
    pub banner: bool,
}

/// 설정으로 이 종류의 알림이 갈 곳을 정한다. 목록에는 늘 쌓이므로 여기에 없다. `test`는 채널 전체 스위치만 따른다.
pub fn route(settings: &Settings, kind: &str) -> Route {
    let event = match kind {
        "question" => settings.events.question,
        "pr" => settings.events.pr,
        "failed" => settings.events.failed,
        "done" => settings.events.done,
        _ => Channel { in_app: true, macos: true },
    };
    Route { toast: settings.in_app && event.in_app, banner: settings.macos && event.macos }
}

// ------------------------------------------------------------------ 파일

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// 앱 지원 폴더 아래의 알림 폴더. 시험은 임시 폴더를 직접 준다.
pub fn default_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join("Library/Application Support/aisurada/notifications")
}

/// vault 경로에서 파일 이름에 쓸 짧은 해시(SHA-256 앞 8바이트).
pub fn vault_key(vault: &str) -> String {
    let d = digest::digest(&digest::SHA256, vault.as_bytes());
    d.as_ref()[..8].iter().map(|b| format!("{b:02x}")).collect()
}

fn items_path(dir: &Path, vault: &str) -> PathBuf {
    dir.join(format!("items-{}.json", vault_key(vault)))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("알림 폴더를 못 만들었다: {e}"))?;
    }
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&tmp, bytes).map_err(|e| format!("알림 파일을 못 썼다: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("알림 파일을 제자리로 못 옮겼다: {e}")
    })
}

/// 목록을 읽는다. 없거나 망가졌으면 빈 목록이다(알림은 지워져도 큰일이 아니고, 앱이 죽으면 안 된다).
pub fn load_items(dir: &Path, vault: &str) -> Vec<Item> {
    fs::read(items_path(dir, vault)).ok().and_then(|b| serde_json::from_slice::<Vec<Item>>(&b).ok()).unwrap_or_default()
}

fn save_items(dir: &Path, vault: &str, items: &[Item]) -> Result<(), String> {
    let bytes = serde_json::to_vec(items).map_err(|e| format!("알림을 글로 못 바꿨다: {e}"))?;
    write_atomic(&items_path(dir, vault), &bytes)
}

pub fn load_settings(dir: &Path) -> Settings {
    fs::read(dir.join("settings.json")).ok().and_then(|b| serde_json::from_slice::<Settings>(&b).ok()).unwrap_or_default()
}

fn save_settings(dir: &Path, settings: &Settings) -> Result<(), String> {
    let bytes = serde_json::to_vec(settings).map_err(|e| format!("설정을 글로 못 바꿨다: {e}"))?;
    write_atomic(&dir.join("settings.json"), &bytes)
}

// ------------------------------------------------------------------ 계산

fn clip(s: &str, max: usize) -> String {
    let t = s.trim();
    if t.chars().count() <= max {
        t.to_string()
    } else {
        let mut out: String = t.chars().take(max - 1).collect();
        out.push('…');
        out
    }
}

/// 새 알림을 검사하고 다듬어 알림으로 만든다. 종류를 모르거나 제목이 비면 거절한다.
pub fn build_item(new: NewItem, at: u64, seq: usize) -> Result<Item, String> {
    if !KINDS.contains(&new.kind.as_str()) {
        return Err(format!("알림 종류를 모른다: {}", new.kind));
    }
    let title = clip(&new.title, MAX_TITLE);
    if title.is_empty() {
        return Err("알림 제목이 필요하다".to_string());
    }
    let task_key = match new.task_key {
        Some(k) if !k.trim().is_empty() => {
            let k = k.trim().to_string();
            let ok = k.strip_prefix("TSK-").is_some_and(|n| !n.is_empty() && n.len() <= 9 && n.chars().all(|c| c.is_ascii_digit()));
            if !ok {
                return Err(format!("태스크 키 모양이 아니다: {k}"));
            }
            Some(k)
        }
        _ => None,
    };
    let id = match new.id {
        Some(id) if !id.trim().is_empty() => {
            let id = id.trim().to_string();
            if id.len() > 100 || id.chars().any(|c| c.is_control()) {
                return Err("알림 id 모양이 아니다".to_string());
            }
            id
        }
        _ => format!("n{at:x}-{seq:x}"),
    };
    Ok(Item { id, kind: new.kind, title, body: clip(&new.body, MAX_BODY), task_key, at, read: false })
}

/// 목록에 알림을 더한다. 같은 id가 이미 있으면 그대로 두고 false를 돌려준다. 새것이 앞이고 {@link MAX_ITEMS}개를 넘으면 오래된 것을 버린다.
pub fn push_item(items: &mut Vec<Item>, item: Item) -> bool {
    if items.iter().any(|i| i.id == item.id) {
        return false;
    }
    items.insert(0, item);
    items.truncate(MAX_ITEMS);
    true
}

/// 읽음으로 바꾼다. `ids`가 None이면 전부. 바뀐 개수를 돌려준다.
pub fn mark_read(items: &mut [Item], ids: Option<&[String]>) -> usize {
    let mut n = 0;
    for i in items.iter_mut() {
        if !i.read && ids.map_or(true, |list| list.contains(&i.id)) {
            i.read = true;
            n += 1;
        }
    }
    n
}

pub fn unread(items: &[Item]) -> usize {
    items.iter().filter(|i| !i.read).count()
}

// ------------------------------------------------------------------ macOS 배너

/// AppleScript 글 상수에 넣을 수 있게 다듬는다(따옴표와 역슬래시를 탈출하고 제어 문자는 공백으로).
pub fn applescript_text(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

/// 배너를 보낸다. 앱에 든 알림 플러그인으로 먼저 보내고, 그것이 실패하면 `osascript`로 한 번 더 시도한다(이때 배너는 Script Editor 이름으로 뜬다).
/// 어느 길로 갔는지를 `sent`, `sent-fallback`, 실패 까닭으로 돌려준다.
pub fn show_banner<R: Runtime>(app: &AppHandle<R>, title: &str, body: &str) -> String {
    use tauri_plugin_notification::NotificationExt;
    match app.notification().builder().title(title).body(body).show() {
        Ok(()) => "sent".to_string(),
        Err(first) => match osascript_banner(title, body) {
            Ok(()) => "sent-fallback".to_string(),
            Err(second) => format!("failed: {first}; {second}"),
        },
    }
}

#[cfg(target_os = "macos")]
fn osascript_banner(title: &str, body: &str) -> Result<(), String> {
    let script = format!("display notification \"{}\" with title \"{}\"", applescript_text(body), applescript_text(title));
    let out = std::process::Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| format!("osascript를 못 띄웠다: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("osascript가 실패했다: {}", String::from_utf8_lossy(&out.stderr).trim()))
    }
}

#[cfg(not(target_os = "macos"))]
fn osascript_banner(_title: &str, _body: &str) -> Result<(), String> {
    Err("macOS 배너는 macOS에서만 보낸다".to_string())
}

// ------------------------------------------------------------------ 명령

/// 화면에 알리는 사건. 목록이 바뀌었거나 새 알림이 왔다.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Changed {
    pub unread: usize,
    /// 방금 온 알림. 읽음 표시만 바뀐 사건이면 null
    pub pushed: Option<Item>,
    /// 방금 온 알림을 앱 안 토스트로 띄울지(설정이 정한다)
    pub toast: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListOut {
    pub items: Vec<Item>,
    pub unread: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushOut {
    /// 새로 쌓였으면 그 알림, 같은 id가 이미 있어 건너뛰었으면 null
    pub item: Option<Item>,
    pub toast: bool,
    /// `off`(안 보냄), `sent`, `sent-fallback`, `failed: …`
    pub banner: String,
    pub unread: usize,
}

/// 알림 파일을 읽고 쓰는 동안의 잠금. 화면과 러너가 동시에 부를 수 있다.
#[derive(Default)]
pub struct NotifyState {
    lock: Mutex<()>,
}

fn current_vault(host: &HostState) -> Result<String, String> {
    host.vault.lock().map_err(|_| "vault 잠금을 얻지 못했다".to_string())?.clone().ok_or_else(|| "열린 vault가 없다".to_string())
}

/// 알림을 쌓고 설정에 따라 배너를 보내고 화면에 알린다. 3단계의 러너도 이 함수를 부른다.
pub fn push<R: Runtime>(app: &AppHandle<R>, dir: &Path, vault: &str, new: NewItem) -> Result<PushOut, String> {
    let settings = load_settings(dir);
    let mut items = load_items(dir, vault);
    let item = build_item(new, now_ms(), items.len())?;
    let r = route(&settings, &item.kind);
    let stored = push_item(&mut items, item.clone());
    if stored {
        save_items(dir, vault, &items)?;
    }
    let unread_count = unread(&items);
    if !stored {
        return Ok(PushOut { item: None, toast: false, banner: "off".to_string(), unread: unread_count });
    }
    let banner = if r.banner {
        // 배너 본문에 태스크 키를 붙여 어느 일인지 알아보게 한다
        let text = match &item.task_key {
            Some(k) if !item.body.is_empty() => format!("{k} · {}", item.body),
            Some(k) => k.clone(),
            None => item.body.clone(),
        };
        show_banner(app, &item.title, &text)
    } else {
        "off".to_string()
    };
    let _ = app.emit(NOTIFICATIONS, Changed { unread: unread_count, pushed: Some(item.clone()), toast: r.toast });
    Ok(PushOut { item: Some(item), toast: r.toast, banner, unread: unread_count })
}

#[tauri::command]
pub fn notifications_list(host: State<'_, HostState>, state: State<'_, NotifyState>) -> Result<ListOut, String> {
    let vault = match current_vault(&host) {
        Ok(v) => v,
        Err(_) => return Ok(ListOut { items: vec![], unread: 0 }),
    };
    let _guard = state.lock.lock().map_err(|_| "알림 잠금을 얻지 못했다")?;
    let items = load_items(&default_dir(), &vault);
    let unread_count = unread(&items);
    Ok(ListOut { items, unread: unread_count })
}

#[tauri::command]
pub fn notifications_push<R: Runtime>(
    app: AppHandle<R>,
    host: State<'_, HostState>,
    state: State<'_, NotifyState>,
    item: NewItem,
) -> Result<PushOut, String> {
    let vault = current_vault(&host)?;
    let _guard = state.lock.lock().map_err(|_| "알림 잠금을 얻지 못했다")?;
    push(&app, &default_dir(), &vault, item)
}

/// 읽음으로 바꾼다. `ids`를 안 주면 전부. 바뀐 뒤의 안 읽은 수를 돌려준다.
#[tauri::command]
pub fn notifications_mark_read<R: Runtime>(
    app: AppHandle<R>,
    host: State<'_, HostState>,
    state: State<'_, NotifyState>,
    ids: Option<Vec<String>>,
) -> Result<usize, String> {
    let vault = current_vault(&host)?;
    let _guard = state.lock.lock().map_err(|_| "알림 잠금을 얻지 못했다")?;
    let dir = default_dir();
    let mut items = load_items(&dir, &vault);
    if mark_read(&mut items, ids.as_deref()) > 0 {
        save_items(&dir, &vault, &items)?;
    }
    let unread_count = unread(&items);
    let _ = app.emit(NOTIFICATIONS, Changed { unread: unread_count, pushed: None, toast: false });
    Ok(unread_count)
}

#[tauri::command]
pub fn notifications_settings_get(state: State<'_, NotifyState>) -> Result<Settings, String> {
    let _guard = state.lock.lock().map_err(|_| "알림 잠금을 얻지 못했다")?;
    Ok(load_settings(&default_dir()))
}

#[tauri::command]
pub fn notifications_settings_set(state: State<'_, NotifyState>, settings: Settings) -> Result<Settings, String> {
    let _guard = state.lock.lock().map_err(|_| "알림 잠금을 얻지 못했다")?;
    save_settings(&default_dir(), &settings)?;
    Ok(settings)
}

// ------------------------------------------------------------------ 시험

#[cfg(test)]
mod tests {
    use super::*;

    fn new(kind: &str, title: &str) -> NewItem {
        NewItem { kind: kind.to_string(), title: title.to_string(), body: String::new(), task_key: None, id: None }
    }

    fn item(id: &str, read: bool) -> Item {
        Item { id: id.to_string(), kind: "test".to_string(), title: id.to_string(), body: String::new(), task_key: None, at: 1, read }
    }

    #[test]
    fn defaults_keep_done_quiet_and_everything_else_on() {
        let s = Settings::default();
        assert!(s.in_app && s.macos);
        assert_eq!(s.events.question, Channel { in_app: true, macos: true });
        assert_eq!(s.events.pr, Channel { in_app: true, macos: true });
        assert_eq!(s.events.failed, Channel { in_app: true, macos: true });
        assert_eq!(s.events.done, Channel { in_app: true, macos: false }, "Done is quiet: in-app only");
    }

    #[test]
    fn route_follows_the_channel_switch_and_the_event_switch() {
        let mut s = Settings::default();
        assert_eq!(route(&s, "question"), Route { toast: true, banner: true });
        assert_eq!(route(&s, "done"), Route { toast: true, banner: false });
        s.in_app = false;
        assert_eq!(route(&s, "question"), Route { toast: false, banner: true }, "channel off beats event on");
        s.macos = false;
        assert_eq!(route(&s, "question"), Route { toast: false, banner: false });
        let mut t = Settings::default();
        t.events.pr = Channel { in_app: false, macos: true };
        assert_eq!(route(&t, "pr"), Route { toast: false, banner: true });
        assert_eq!(route(&Settings::default(), "test"), Route { toast: true, banner: true }, "a test follows the channel switches only");
        t.in_app = false;
        assert_eq!(route(&t, "test"), Route { toast: false, banner: true });
    }

    #[test]
    fn settings_fill_missing_fields_and_ignore_a_broken_file() {
        let partial: Settings = serde_json::from_str(r#"{"macos":false,"events":{"pr":{"inApp":false,"macos":false}}}"#).unwrap();
        assert!(partial.in_app, "a missing channel switch is on");
        assert!(!partial.macos);
        assert_eq!(partial.events.pr, Channel { in_app: false, macos: false });
        assert_eq!(partial.events.done, Channel { in_app: true, macos: false }, "a missing event keeps its own default");
        let empty: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(empty, Settings::default());
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("settings.json"), b"{not json").unwrap();
        assert_eq!(load_settings(dir.path()), Settings::default(), "a broken settings file means the defaults");
        assert_eq!(load_settings(&dir.path().join("none")), Settings::default());
    }

    #[test]
    fn settings_round_trip_through_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Settings::default();
        s.in_app = false;
        s.events.question = Channel { in_app: false, macos: true };
        save_settings(dir.path(), &s).unwrap();
        assert_eq!(load_settings(dir.path()), s);
        let text = fs::read_to_string(dir.path().join("settings.json")).unwrap();
        assert!(text.contains("\"inApp\":false") && text.contains("\"events\""), "camelCase keys: {text}");
    }

    #[test]
    fn build_item_checks_kind_title_key_and_id_and_clips_long_text() {
        assert!(build_item(new("question", "질문"), 10, 0).is_ok());
        for bad in ["", "unknown", "QUESTION"] {
            assert!(build_item(new(bad, "x"), 10, 0).unwrap_err().contains("종류"), "kind {bad}");
        }
        assert!(build_item(new("test", "   "), 10, 0).unwrap_err().contains("제목"));
        let mut n = new("question", "질문");
        n.task_key = Some("TSK-14".to_string());
        assert_eq!(build_item(n.clone(), 10, 0).unwrap().task_key.as_deref(), Some("TSK-14"));
        for bad in ["TSK-", "TSK-x", "tsk-1", "../etc", "TSK-1234567890"] {
            n.task_key = Some(bad.to_string());
            assert!(build_item(n.clone(), 10, 0).unwrap_err().contains("태스크 키"), "key {bad}");
        }
        n.task_key = Some("  ".to_string());
        assert_eq!(build_item(n.clone(), 10, 0).unwrap().task_key, None, "a blank key is no key");
        let long = build_item(NewItem { body: "가".repeat(500), title: "나".repeat(300), ..new("test", "x") }, 10, 0).unwrap();
        assert_eq!(long.title.chars().count(), MAX_TITLE);
        assert!(long.title.ends_with('…'));
        assert_eq!(long.body.chars().count(), MAX_BODY);
        let generated = build_item(new("test", "x"), 255, 3).unwrap();
        assert_eq!(generated.id, "nff-3");
        assert!(!generated.read);
        let mut idn = new("test", "x");
        idn.id = Some("task-TSK-1-waiting".to_string());
        assert_eq!(build_item(idn.clone(), 1, 0).unwrap().id, "task-TSK-1-waiting");
        idn.id = Some("a\nb".to_string());
        assert!(build_item(idn, 1, 0).is_err(), "a control character in an id is refused");
    }

    #[test]
    fn push_puts_the_newest_first_skips_a_known_id_and_caps_the_list() {
        let mut items = vec![];
        assert!(push_item(&mut items, item("a", false)));
        assert!(push_item(&mut items, item("b", false)));
        assert_eq!(items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ["b", "a"]);
        assert!(!push_item(&mut items, item("a", false)), "the same id is not stored twice");
        assert_eq!(items.len(), 2);
        let mut many = vec![];
        for i in 0..(MAX_ITEMS + 5) {
            push_item(&mut many, item(&format!("n{i}"), false));
        }
        assert_eq!(many.len(), MAX_ITEMS);
        assert_eq!(many[0].id, format!("n{}", MAX_ITEMS + 4), "newest kept");
        assert!(many.iter().all(|i| i.id != "n0"), "oldest dropped");
    }

    #[test]
    fn mark_read_changes_only_what_is_asked_and_counts_unread() {
        let mut items = vec![item("a", false), item("b", false), item("c", true)];
        assert_eq!(unread(&items), 2);
        assert_eq!(mark_read(&mut items, Some(&["b".to_string(), "zzz".to_string()])), 1);
        assert_eq!(unread(&items), 1);
        assert_eq!(mark_read(&mut items, Some(&["b".to_string()])), 0, "already read");
        assert_eq!(mark_read(&mut items, None), 1, "all");
        assert_eq!(unread(&items), 0);
    }

    #[test]
    fn items_are_kept_per_vault_and_a_broken_file_reads_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let a = "/Users/me/vault-a";
        let b = "/Users/me/vault-b";
        assert_ne!(vault_key(a), vault_key(b));
        assert_eq!(vault_key(a).len(), 16);
        assert_eq!(vault_key(a), vault_key(a));
        let mut items = vec![];
        push_item(&mut items, item("x", false));
        save_items(dir.path(), a, &items).unwrap();
        assert_eq!(load_items(dir.path(), a), items);
        assert!(load_items(dir.path(), b).is_empty(), "another vault has its own list");
        fs::write(items_path(dir.path(), b), b"[{\"bad\":").unwrap();
        assert!(load_items(dir.path(), b).is_empty());
        // 원자적 쓰기는 임시 파일을 남기지 않는다
        let names: Vec<String> = fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        assert!(names.iter().all(|n| !n.contains("tmp-")), "{names:?}");
    }

    #[test]
    fn applescript_text_escapes_quotes_and_backslashes_and_flattens_control_characters() {
        assert_eq!(applescript_text(r#"a "b" \ c"#), r#"a \"b\" \\ c"#);
        assert_eq!(applescript_text("줄\n바꿈\t탭"), "줄 바꿈 탭");
        assert_eq!(applescript_text("한글 그대로"), "한글 그대로");
    }

    /// 실제 macOS 알림을 한 번 보내 본다(배너가 화면에 뜬다). 개발 실행처럼 터미널 이름으로 뜬다.
    /// `cargo test -- --ignored real_banner_smoke`로만 돈다. 결과 글(`sent`, `sent-fallback`, 실패 까닭)을 찍는다.
    #[test]
    #[ignore = "실제 macOS 알림을 보낸다"]
    fn real_banner_smoke() {
        let app = tauri::test::mock_builder()
            .plugin(tauri_plugin_notification::init())
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let out = show_banner(app.handle(), "aisurada 시험", "알림 시험이에요. 이 배너는 사라져도 돼요.");
        println!("banner result: {out}");
        assert!(out == "sent" || out == "sent-fallback", "{out}");
    }

    #[test]
    fn item_json_uses_camel_case_and_reads_back() {
        let i = Item { id: "a".into(), kind: "question".into(), title: "t".into(), body: "b".into(), task_key: Some("TSK-1".into()), at: 5, read: false };
        let text = serde_json::to_string(&i).unwrap();
        assert!(text.contains("\"taskKey\":\"TSK-1\"") && text.contains("\"at\":5"), "{text}");
        assert_eq!(serde_json::from_str::<Item>(&text).unwrap(), i);
        let old: Item = serde_json::from_str(r#"{"id":"a","kind":"test","title":"t","body":"","at":1,"read":true}"#).unwrap();
        assert_eq!(old.task_key, None, "taskKey may be missing");
    }
}
