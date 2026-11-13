//! 러너의 파일 저장소. 설정과 실행 기록을 앱 지원 폴더에 둔다(vault 안이 아니다).
//!
//! 실행 기록은 이 컴퓨터에서만 뜻이 있고(작업 폴더 경로, 세션 id) vault의 git 이력을 부풀리면 안 되므로
//! `~/Library/Application Support/aisurada/runs/<vault 해시>/<TSK-n>/<실행 id>/{run.json, events.jsonl}`에 둔다(결정 3).
use crate::notifications::vault_key;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

/// 한꺼번에 돌릴 수 있는 실행 수의 범위.
pub const MIN_CONCURRENCY: u32 = 1;
pub const MAX_CONCURRENCY: u32 = 6;
/// 사람에게 물어볼 수 있는 질문의 수. 넘으면 지금까지 안 것으로 보고하게 한다.
pub const MAX_QUESTIONS: u32 = 5;
/// 한 실행이 일하는 데 쓸 수 있는 시간(분). 사람의 답을 기다린 시간은 세지 않는다.
pub const MAX_WORK_MINUTES: u64 = 120;
/// 이벤트 파일에서 화면에 돌려줄 마지막 줄 수.
pub const EVENT_TAIL: usize = 80;

/// 러너 설정. 모든 vault가 같이 쓴다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// 켜면 「Ready For Develop」이 된 태스크를 러너가 스스로 집는다. 끄면 「지금 실행」을 눌렀을 때만 돈다.
    pub auto_pickup: bool,
    /// 동시에 돌릴 실행 수
    pub concurrency: u32,
    /// `claude --model`에 줄 값. 비면 claude의 기본 모델을 쓴다.
    pub model: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { auto_pickup: false, concurrency: 3, model: "sonnet".to_string() }
    }
}

impl Settings {
    /// 범위를 벗어난 값을 바로잡는다. 손으로 고친 설정 파일이 러너를 멈추게 하지 않으려는 것이다.
    pub fn clamped(mut self) -> Settings {
        self.concurrency = self.concurrency.clamp(MIN_CONCURRENCY, MAX_CONCURRENCY);
        self.model = self.model.trim().to_string();
        self
    }
}

/// 실행이 어디까지 왔는지.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Phase {
    Running,
    WaitingAnswer,
    Done,
    Failed,
    Cancelled,
    /// 사람이 태스크를 손으로 옮겨서 러너가 더는 이어 가지 않는 실행
    Abandoned,
}

impl Phase {
    /// 아직 끝나지 않아 자원(작업 폴더, 세션)을 쥐고 있는 단계인지.
    pub fn is_open(self) -> bool {
        matches!(self, Phase::Running | Phase::WaitingAnswer)
    }
}

/// 한 번의 실행. 같은 태스크가 여러 번 돌 수 있어 실행마다 하나씩 둔다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RunRecord {
    pub run_id: String,
    pub task_key: String,
    pub repo: String,
    /// claude 세션 id(UUID). 질문에 답이 오거나 앱을 다시 켰을 때 `--resume`으로 이어 간다.
    pub session_id: String,
    pub phase: Phase,
    /// 일곱 걸음 가운데 지금 걸음: 받기 집기 판단 받아오기 일하기 내보내기 정리
    pub step: String,
    pub started_at: u64,
    pub ended_at: Option<u64>,
    /// claude를 부른 횟수
    pub turns: u32,
    pub questions: u32,
    /// 사람의 답을 기다리는 질문
    pub question: Option<String>,
    /// 도착했지만 아직 claude에 건네지 않은 답
    pub answer: Option<String>,
    /// 읽기 전용 작업 폴더
    pub workdir: Option<String>,
    /// 일한 시간(초). 기다린 시간은 빼고 센다.
    pub worked_secs: u64,
    pub cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// 보고 앞부분(목록에 보일 요약)
    pub summary: Option<String>,
    pub error: Option<String>,
}

impl Default for RunRecord {
    fn default() -> Self {
        RunRecord {
            run_id: String::new(),
            task_key: String::new(),
            repo: String::new(),
            session_id: String::new(),
            phase: Phase::Running,
            step: "받기".to_string(),
            started_at: 0,
            ended_at: None,
            turns: 0,
            questions: 0,
            question: None,
            answer: None,
            workdir: None,
            worked_secs: 0,
            cost_usd: 0.0,
            input_tokens: 0,
            output_tokens: 0,
            summary: None,
            error: None,
        }
    }
}

/// 이벤트 파일의 한 줄. 화면의 「실행」 탭이 걸음별로 보여 준다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub at: u64,
    pub step: String,
    pub text: String,
}

pub fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// 무작위 UUID(v4). claude의 `--session-id`는 UUID만 받고, 실행 id도 이것으로 만든다.
pub fn new_uuid() -> String {
    let mut b = [0u8; 16];
    if SystemRandom::new().fill(&mut b).is_err() {
        // 난수원이 막힌 경우에도 겹치지 않게 시각으로 채운다
        let t = now_ms().to_be_bytes();
        b[..8].copy_from_slice(&t);
        b[8..].copy_from_slice(&std::process::id().to_be_bytes().repeat(2));
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

/// 앱 지원 폴더 아래의 러너 폴더. 시험은 임시 폴더를 직접 준다.
pub fn default_root() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    home.join("Library/Application Support/aisurada/runs")
}

fn settings_path(root: &Path) -> PathBuf {
    root.join("settings.json")
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("러너 폴더를 못 만들었다: {e}"))?;
    }
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&tmp, bytes).map_err(|e| format!("러너 파일을 못 썼다: {e}"))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("러너 파일을 제자리로 못 옮겼다: {e}")
    })
}

/// 설정을 읽는다. 없거나 망가졌으면 기본값이다.
pub fn load_settings(root: &Path) -> Settings {
    fs::read(settings_path(root))
        .ok()
        .and_then(|b| serde_json::from_slice::<Settings>(&b).ok())
        .unwrap_or_default()
        .clamped()
}

pub fn save_settings(root: &Path, settings: &Settings) -> Result<Settings, String> {
    let clean = settings.clone().clamped();
    let bytes = serde_json::to_vec_pretty(&clean).map_err(|e| format!("설정을 글로 못 바꿨다: {e}"))?;
    write_atomic(&settings_path(root), &bytes)?;
    Ok(clean)
}

/// 이 vault의 실행 기록 폴더
pub fn vault_dir(root: &Path, vault: &str) -> PathBuf {
    root.join(vault_key(vault))
}

/// 태스크 키가 폴더 이름으로 안전한지(`TSK-14`). 화면이 보낸 값을 경로에 쓰기 전에 거른다.
pub fn is_task_key(key: &str) -> bool {
    let Some(n) = key.strip_prefix("TSK-") else { return false };
    !n.is_empty() && n.len() <= 9 && n.chars().all(|c| c.is_ascii_digit())
}

pub fn run_dir(root: &Path, vault: &str, key: &str, run_id: &str) -> PathBuf {
    vault_dir(root, vault).join(key).join(run_id)
}

pub fn save_run(root: &Path, vault: &str, rec: &RunRecord) -> Result<(), String> {
    if !is_task_key(&rec.task_key) {
        return Err(format!("태스크 키가 이상하다: {}", rec.task_key));
    }
    let bytes = serde_json::to_vec_pretty(rec).map_err(|e| format!("실행 기록을 글로 못 바꿨다: {e}"))?;
    write_atomic(&run_dir(root, vault, &rec.task_key, &rec.run_id).join("run.json"), &bytes)
}

pub fn load_run(root: &Path, vault: &str, key: &str, run_id: &str) -> Option<RunRecord> {
    if !is_task_key(key) || run_id.contains('/') || run_id.contains("..") {
        return None;
    }
    fs::read(run_dir(root, vault, key, run_id).join("run.json")).ok().and_then(|b| serde_json::from_slice(&b).ok())
}

/// 태스크 하나의 실행 기록. 최근이 앞이다. 망가진 파일은 건너뛴다.
pub fn list_runs(root: &Path, vault: &str, key: &str) -> Vec<RunRecord> {
    if !is_task_key(key) {
        return Vec::new();
    }
    let mut out: Vec<RunRecord> = fs::read_dir(vault_dir(root, vault).join(key))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| fs::read(e.path().join("run.json")).ok())
        .filter_map(|b| serde_json::from_slice::<RunRecord>(&b).ok())
        .collect();
    out.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    out
}

/// 이 vault에서 아직 끝나지 않은 실행 전부(앱을 다시 켰을 때 이어 가거나 정리할 것들).
pub fn list_open_runs(root: &Path, vault: &str) -> Vec<RunRecord> {
    let mut out = Vec::new();
    for task in fs::read_dir(vault_dir(root, vault)).into_iter().flatten().flatten() {
        let name = task.file_name().to_string_lossy().into_owned();
        if is_task_key(&name) {
            out.extend(list_runs(root, vault, &name).into_iter().filter(|r| r.phase.is_open()));
        }
    }
    out.sort_by(|a, b| a.started_at.cmp(&b.started_at));
    out
}

/// 이벤트를 한 줄 덧붙인다. 쓰지 못해도 실행을 멈추지 않는다(기록은 보조다).
pub fn append_event(root: &Path, vault: &str, rec: &RunRecord, step: &str, text: &str) {
    let dir = run_dir(root, vault, &rec.task_key, &rec.run_id);
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let line = serde_json::to_string(&Event { at: now_ms(), step: step.to_string(), text: text.to_string() }).unwrap_or_default();
    if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(dir.join("events.jsonl")) {
        let _ = writeln!(f, "{line}");
    }
}

/// 이벤트의 마지막 [EVENT_TAIL]줄.
pub fn read_events(root: &Path, vault: &str, key: &str, run_id: &str) -> Vec<Event> {
    if !is_task_key(key) || run_id.contains('/') || run_id.contains("..") {
        return Vec::new();
    }
    let text = fs::read_to_string(run_dir(root, vault, key, run_id).join("events.jsonl")).unwrap_or_default();
    let mut events: Vec<Event> = text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
    if events.len() > EVENT_TAIL {
        events.drain(..events.len() - EVENT_TAIL);
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(key: &str, id: &str, phase: Phase, at: u64) -> RunRecord {
        RunRecord { run_id: id.into(), task_key: key.into(), repo: "o/r".into(), phase, started_at: at, ..Default::default() }
    }

    #[test]
    fn settings_default_clamp_and_survive_bad_files() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_settings(dir.path()), Settings::default());
        assert!(!Settings::default().auto_pickup, "처음에는 스스로 집지 않는다");
        let saved = save_settings(dir.path(), &Settings { auto_pickup: true, concurrency: 99, model: "  opus ".into() }).unwrap();
        assert_eq!((saved.concurrency, saved.model.as_str(), saved.auto_pickup), (MAX_CONCURRENCY, "opus", true));
        assert_eq!(load_settings(dir.path()), saved);
        assert_eq!(Settings { concurrency: 0, ..Default::default() }.clamped().concurrency, MIN_CONCURRENCY);
        fs::write(dir.path().join("settings.json"), "{망가짐").unwrap();
        assert_eq!(load_settings(dir.path()), Settings::default());
        // 모르는 칸이 있어도, 빠진 칸이 있어도 읽힌다
        fs::write(dir.path().join("settings.json"), r#"{"autoPickup":true,"future":1}"#).unwrap();
        let s = load_settings(dir.path());
        assert!(s.auto_pickup && s.concurrency == 3);
    }

    #[test]
    fn uuid_is_v4_shaped_and_unique() {
        let a = new_uuid();
        let b = new_uuid();
        assert_ne!(a, b);
        let parts: Vec<&str> = a.split('-').collect();
        assert_eq!(parts.iter().map(|p| p.len()).collect::<Vec<_>>(), vec![8, 4, 4, 4, 12]);
        assert!(parts[2].starts_with('4'));
        assert!("89ab".contains(parts[3].chars().next().unwrap()));
    }

    #[test]
    fn task_keys_are_checked_before_touching_paths() {
        for ok in ["TSK-1", "TSK-14", "TSK-123456789"] {
            assert!(is_task_key(ok), "{ok}");
        }
        for bad in ["", "TSK-", "TSK-x", "tsk-1", "TSK-1/..", "../TSK-1", "TSK-1234567890", "TSK--1", "TSK-1 "] {
            assert!(!is_task_key(bad), "{bad}");
        }
        let dir = tempfile::tempdir().unwrap();
        assert!(save_run(dir.path(), "/v", &record("../x", "r1", Phase::Running, 1)).is_err());
        assert!(load_run(dir.path(), "/v", "TSK-1", "../r").is_none());
        assert!(read_events(dir.path(), "/v", "TSK-1", "a/b").is_empty());
    }

    #[test]
    fn runs_round_trip_list_newest_first_and_find_open_ones() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        save_run(root, "/v", &record("TSK-1", "r1", Phase::Done, 10)).unwrap();
        save_run(root, "/v", &record("TSK-1", "r2", Phase::WaitingAnswer, 20)).unwrap();
        save_run(root, "/v", &record("TSK-2", "r3", Phase::Running, 5)).unwrap();
        save_run(root, "/other", &record("TSK-1", "r9", Phase::Running, 99)).unwrap();
        let runs = list_runs(root, "/v", "TSK-1");
        assert_eq!(runs.iter().map(|r| r.run_id.as_str()).collect::<Vec<_>>(), vec!["r2", "r1"]);
        assert_eq!(load_run(root, "/v", "TSK-1", "r2").unwrap().phase, Phase::WaitingAnswer);
        assert!(load_run(root, "/v", "TSK-1", "없음").is_none());
        let open: Vec<_> = list_open_runs(root, "/v").into_iter().map(|r| r.run_id).collect();
        assert_eq!(open, vec!["r3", "r2"], "오래된 것이 앞이고, 다른 vault와 끝난 것은 빠진다");
        // 망가진 기록은 건너뛴다
        fs::write(run_dir(root, "/v", "TSK-1", "r1").join("run.json"), "{").unwrap();
        assert_eq!(list_runs(root, "/v", "TSK-1").len(), 1);
        // phase 이름이 화면과 맞다
        let json = serde_json::to_string(&Phase::WaitingAnswer).unwrap();
        assert_eq!(json, "\"waiting-answer\"");
    }

    #[test]
    fn events_append_and_keep_only_the_tail() {
        let dir = tempfile::tempdir().unwrap();
        let rec = record("TSK-3", "r1", Phase::Running, 1);
        assert!(read_events(dir.path(), "/v", "TSK-3", "r1").is_empty());
        for i in 0..(EVENT_TAIL + 5) {
            append_event(dir.path(), "/v", &rec, "일하기", &format!("줄 {i}"));
        }
        let events = read_events(dir.path(), "/v", "TSK-3", "r1");
        assert_eq!(events.len(), EVENT_TAIL);
        assert_eq!(events.last().unwrap().text, format!("줄 {}", EVENT_TAIL + 4));
        assert_eq!(events[0].text, "줄 5");
    }
}
