//! sqlite3 실행 어댑터(Antigravity 대화 기록 읽기 전용 질의). 셸 없이 인자를 그대로 넘기고, 시간이 지나면 죽인다.
//!
//! sqlite3는 시험용 환경 변수 `AISURADA_SQLITE_BIN`, 기본 자리 `/usr/bin/sqlite3`, `PATH` 차례로 찾는다.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use crate::ports::SqliteReader;

/// sqlite3가 이 시간 안에 끝나지 않으면 죽인다
const SQLITE_TIMEOUT_SECONDS: u64 = 60;

pub struct SqliteCli;

impl SqliteReader for SqliteCli {
    fn query(&self, uri: &str, sql: &str) -> Result<String, String> {
        run_sqlite(uri, sql)
    }
}

fn run_sqlite(uri: &str, sql: &str) -> Result<String, String> {
    let sqlite = sqlite_binary().ok_or_else(|| {
        "Antigravity 대화 기록은 sqlite3로 읽는데 찾지 못했다(macOS는 /usr/bin/sqlite3). AISURADA_SQLITE_BIN으로 자리를 알려 줄 수 있다.".to_string()
    })?;
    let mut child = Command::new(sqlite)
        .args([
            "-batch",
            "-readonly",
            "-cmd",
            ".timeout 3000",
            "-separator",
            "|",
            uri,
            sql,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut stdout = child.stdout.take().ok_or("sqlite3 출력을 못 열었다")?;
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        let _ = sender.send(bytes);
    });
    let bytes = match receiver.recv_timeout(Duration::from_secs(SQLITE_TIMEOUT_SECONDS)) {
        Ok(bytes) => bytes,
        Err(_) => {
            let _ = child.kill();
            return Err(format!(
                "sqlite3가 {SQLITE_TIMEOUT_SECONDS}초 안에 끝나지 않았다"
            ));
        }
    };
    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!(
            "sqlite3가 대화 기록을 열지 못했다(종료 코드 {})",
            status.code().unwrap_or(-1)
        ));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// 시험용 환경 변수, 기본 자리, PATH 차례로 sqlite3를 찾는다. 없으면 `None`.
fn sqlite_binary() -> Option<PathBuf> {
    fn executable(path: &Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    let custom = std::env::var("AISURADA_SQLITE_BIN").unwrap_or_default();
    if !custom.trim().is_empty() {
        let path = PathBuf::from(custom.trim());
        return executable(&path).then_some(path);
    }
    let system = PathBuf::from("/usr/bin/sqlite3");
    if executable(&system) {
        return Some(system);
    }
    std::env::var("PATH")
        .ok()?
        .split(':')
        .filter(|d| !d.trim().is_empty())
        .map(|d| Path::new(d).join("sqlite3"))
        .find(|c| executable(c))
}
