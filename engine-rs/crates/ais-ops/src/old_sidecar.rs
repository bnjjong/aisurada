//! 옛 사이드카(Java 상주 프로세스)의 흔적을 찾고 걷어 낸다.
//!
//! Rust `ais-cli`는 사이드카가 없다. 하지만 Java 시절에 쓴 기기에는 vault마다 launchd에 등록한 사이드카(`co.kcd.aisurada.sidecar.<키>`)가
//! 남아 있을 수 있다. 이것을 그대로 두면 낡은 JVM이 계속 떠 있고, 옛 엔진이 vault를 고칠 수 있다. 그래서 **우리 레이블만**(접두
//! `co.kcd.aisurada.sidecar.`) 내리고 plist와 상태 파일을 지운다. 잠금 파일(`locks/`)과 vault, 다른 프로그램의 launchd 항목은 건드리지 않는다.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use ais_core::json::{self, JsonValue};

use crate::env::Env;

pub const LABEL_PREFIX: &str = "co.kcd.aisurada.sidecar.";

/// 옛 사이드카 하나의 흔적
#[derive(Debug, Clone)]
pub struct Trace {
    /// launchd 레이블(`co.kcd.aisurada.sidecar.<키>`)
    pub label: String,
    pub plist: Option<PathBuf>,
    pub state: Option<PathBuf>,
    pub vault: Option<String>,
    pub pid: Option<i64>,
    /// 상태 파일의 pid로 도는 프로세스가 있는가
    pub alive: bool,
}

/// 이 기기에 남은 옛 사이드카 관련 파일 전부
#[derive(Debug, Default)]
pub struct Inventory {
    pub traces: Vec<Trace>,
    /// `install`이 복사해 둔 jar(`Application Support/aisurada/bin/ais-*.jar`)
    pub jars: Vec<PathBuf>,
    /// 사이드카 로그(`logs/sidecar-*.log`)
    pub logs: Vec<PathBuf>,
    /// 하루 청소 시각 기록(`Caches/aisurada/last-sweep`)
    pub last_sweep: Option<PathBuf>,
}

impl Inventory {
    pub fn is_empty(&self) -> bool {
        self.traces.is_empty()
            && self.jars.is_empty()
            && self.logs.is_empty()
            && self.last_sweep.is_none()
    }
}

fn app_support(home: &Path) -> PathBuf {
    home.join("Library/Application Support/aisurada")
}

fn files_in(dir: &Path, accept: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| accept(&entry.file_name().to_string_lossy()))
        .map(|entry| entry.path())
        .collect();
    files.sort();
    files
}

/// 프로세스가 도는가(`kill(pid, 0)`: 권한이 없어도 있는 것이다)
pub fn is_alive(pid: i64) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    if pid <= 0 {
        return false;
    }
    // SAFETY: 신호 0은 아무 신호도 보내지 않고 프로세스가 있는지만 묻는다.
    let alive = unsafe { libc::kill(pid, 0) == 0 };
    alive || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// plist에서 `AISURADA_VAULT` 값을 읽는다(키 줄 바로 뒤의 `<string>`).
fn vault_from_plist(plist: &Path) -> Option<String> {
    let text = std::fs::read_to_string(plist).ok()?;
    let mut found_key = false;
    for raw in text.lines() {
        let line = raw.trim();
        if line == "<key>AISURADA_VAULT</key>" {
            found_key = true;
        } else if found_key
            && let Some(value) = line
                .strip_prefix("<string>")
                .and_then(|l| l.strip_suffix("</string>"))
        {
            return Some(
                value
                    .replace("&lt;", "<")
                    .replace("&gt;", ">")
                    .replace("&amp;", "&"),
            );
        }
    }
    None
}

/// 상태 파일에서 (pid, vault)를 읽는다. 못 읽으면 둘 다 없다.
fn read_state(state: &Path) -> (Option<i64>, Option<String>) {
    let Ok(text) = std::fs::read_to_string(state) else {
        return (None, None);
    };
    let Ok(parsed) = json::parse(&text) else {
        return (None, None);
    };
    let pid = match parsed.get("pid") {
        Some(JsonValue::Literal(raw)) => raw.parse().ok(),
        _ => None,
    };
    let vault = match parsed.get("vault") {
        Some(JsonValue::Str(vault)) => Some(vault.clone()),
        _ => None,
    };
    (pid, vault)
}

/// 이 기기의 옛 사이드카 흔적을 모은다. 아무것도 고치지 않는다.
pub fn inventory(home: &Path) -> Inventory {
    let support = app_support(home);
    let mut traces: Vec<Trace> = Vec::new();

    for plist in files_in(&home.join("Library/LaunchAgents"), |name| {
        name.starts_with(LABEL_PREFIX) && name.ends_with(".plist")
    }) {
        let name = plist.file_name().unwrap().to_string_lossy().into_owned();
        let label = name.trim_end_matches(".plist").to_string();
        traces.push(Trace {
            vault: vault_from_plist(&plist),
            label,
            plist: Some(plist),
            state: None,
            pid: None,
            alive: false,
        });
    }
    for state in files_in(&support.join("run"), |name| name.ends_with(".json")) {
        let key = state.file_stem().unwrap().to_string_lossy().into_owned();
        let label = format!("{LABEL_PREFIX}{key}");
        let (pid, vault) = read_state(&state);
        let alive = pid.is_some_and(is_alive);
        match traces.iter_mut().find(|t| t.label == label) {
            Some(trace) => {
                trace.state = Some(state);
                trace.pid = pid;
                trace.alive = alive;
                trace.vault = trace.vault.take().or(vault);
            }
            None => traces.push(Trace {
                label,
                plist: None,
                state: Some(state),
                vault,
                pid,
                alive,
            }),
        }
    }
    traces.sort_by(|a, b| a.label.cmp(&b.label));

    let last_sweep = home.join("Library/Caches/aisurada/last-sweep");
    Inventory {
        traces,
        jars: files_in(&support.join("bin"), |name| {
            name.starts_with("ais-") && name.ends_with(".jar")
        }),
        logs: files_in(&support.join("logs"), |name| {
            name.starts_with("sidecar-") && name.ends_with(".log")
        }),
        last_sweep: last_sweep.is_file().then_some(last_sweep),
    }
}

/// `launchctl bootout gui/<uid>/<레이블>`. 10초 안에 안 끝나면 실패다. 종료 코드를 돌려준다.
pub fn bootout(env: &Env, label: &str) -> Result<i32, String> {
    let program = env
        .var("AISURADA_LAUNCHCTL_CMD")
        .filter(|cmd| !cmd.is_empty())
        .unwrap_or("/bin/launchctl");
    // SAFETY: 인자 없는 시스템 호출이다.
    let uid = unsafe { libc::getuid() };
    let mut child = Command::new(program)
        .args(["bootout", &format!("gui/{uid}/{label}")])
        .env_clear()
        .envs(&env.vars)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{program} 를 못 돌렸다: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => return Ok(status.code().unwrap_or(-1)),
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{program} 가 10 초 안에 안 끝났다"));
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

/// 걷어 낸 결과 보고
#[derive(Debug, Default)]
pub struct Sweep {
    pub lines: Vec<String>,
    /// 못 내렸거나 못 지운 것. 있으면 사람이 손으로 정리해야 한다.
    pub left: Vec<String>,
}

/// 옛 사이드카를 모두 내리고 흔적을 지운다. `dry_run`이면 아무것도 바꾸지 않고 할 일만 적는다.
pub fn sweep(env: &Env, home: &Path, dry_run: bool) -> Sweep {
    let inventory = inventory(home);
    let mut report = Sweep::default();
    if inventory.is_empty() {
        report
            .lines
            .push("걷어 낼 옛 사이드카 흔적이 없다.".to_string());
        return report;
    }

    report
        .lines
        .push(format!("옛 사이드카 {}개", inventory.traces.len()));
    for trace in &inventory.traces {
        let state = match (trace.state.is_some(), trace.pid, trace.alive) {
            (false, _, _) => "상태 파일 없음".to_string(),
            (true, None, _) => "상태 파일 있음(pid 모름)".to_string(),
            (true, Some(pid), true) => format!("pid {pid} 이 돈다"),
            (true, Some(pid), false) => format!("pid {pid} 로 도는 것이 없다"),
        };
        report.lines.push(format!("  {}", trace.label));
        report.lines.push(format!(
            "    vault={}  plist {}  {state}",
            trace.vault.as_deref().unwrap_or("(못 읽음)"),
            if trace.plist.is_some() {
                "있음"
            } else {
                "없음"
            },
        ));
    }
    if !inventory.jars.is_empty() {
        report
            .lines
            .push(format!("설치해 둔 jar {}개", inventory.jars.len()));
        for jar in &inventory.jars {
            report.lines.push(format!("  {}", jar.display()));
        }
    }
    if dry_run {
        report
            .lines
            .push("  아직 안 내렸다. 진짜 내리려면 --dry-run 를 뺀다.".to_string());
        return report;
    }

    let remove = |path: &Path, what: &str, report: &mut Sweep| match std::fs::remove_file(path) {
        Ok(()) => report
            .lines
            .push(format!("    {what}를 지웠다: {}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => report
            .left
            .push(format!("{what}를 못 지웠다: {} ({e})", path.display())),
    };
    for trace in &inventory.traces {
        // launchd에 올라 있거나 도는 것이 있을 때만 내린다. 상태 파일만 남은 흔적은 내릴 것이 없다.
        if trace.plist.is_some() || trace.alive {
            match bootout(env, &trace.label) {
                Ok(code) => report
                    .lines
                    .push(format!("  내렸다: {} (bootout rc={code})", trace.label)),
                Err(message) => report.lines.push(format!(
                    "  내렸다: {} (bootout 실패: {message})",
                    trace.label
                )),
            }
        }
        // bootout 뒤에도 도는 프로세스가 있으면 파일을 지우지 않는다(손으로 끝내야 한다)
        if let Some(pid) = trace.pid.filter(|pid| is_alive(*pid)) {
            std::thread::sleep(Duration::from_millis(300));
            if is_alive(pid) {
                report.left.push(format!(
                    "{} 의 pid {pid} 가 아직 돈다. `kill {pid}` 로 끝낸 뒤 다시 돌린다",
                    trace.label
                ));
                continue;
            }
        }
        if let Some(plist) = &trace.plist {
            remove(plist, "plist", &mut report);
        }
        if let Some(state) = &trace.state {
            remove(state, "상태 파일", &mut report);
        }
    }
    for path in inventory
        .jars
        .iter()
        .chain(&inventory.logs)
        .chain(&inventory.last_sweep)
    {
        remove(path, "옛 파일", &mut report);
    }
    report
}
