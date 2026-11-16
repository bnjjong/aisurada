//! vault별 쓰기 잠금. 두 겹이다.
//!
//! 1. **프로세스 안:** 같은 vault에 쓰려는 스레드를 도착한 차례(FIFO)로 한 줄에 세운다.
//! 2. **프로세스 사이:** vault 밖에 둔 잠금 파일에 운영체제 파일 잠금(`flock`)을 건다. 앱과 `ais-cli`(명령줄, MCP 서버)처럼
//!    따로 도는 프로세스가 같은 vault에 동시에 쓰지 않게 한다. 지운 Java 엔진의 쓰기 파이프라인도 같은 파일을 잡았다.
//!
//! `ports::VaultLock`의 기본 어댑터(`FlockLock`)다. 잠금 폴더는 환경 변수 `AISURADA_LOCK_DIR`(없으면 홈의 앱 지원 폴더)이거나 만들 때 준 폴더다.
//!
//! 잠금 파일은 vault 내용이 아니므로 vault 밖(사용자 홈의 앱 지원 폴더)에 두고, 이름은 vault 실제 경로의 SHA-256이다.
//! 지우지 않는 까닭과 치우는 법은 개발자 가이드의 「쓰기 잠금과 잠금 파일」, 여러 프로세스의 경합 시험은 `ais-cli/tests/write_contention.rs`다.
//! macOS에서는 Java `FileChannel.lock`(fcntl)과 `flock`이 서로를 막는 것을 확인했다(`tests/lock_interop.rs`, 기준 jar가 있을 때만 돈다).

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, LazyLock, Mutex};

use sha2::{Digest, Sha256};

use crate::error::EngineError;
use crate::kernel::confine::absolute_normalized;
use crate::ports::{LockHandle, VaultLock};

/// 잠금 폴더를 바꾸는 환경 변수(시험용)
pub const LOCK_DIR_ENV: &str = "AISURADA_LOCK_DIR";

struct Ticket {
    next: u64,
    serving: u64,
}

struct Line {
    ticket: Mutex<Ticket>,
    changed: Condvar,
}

static LINES: LazyLock<Mutex<HashMap<String, Arc<Line>>>> = LazyLock::new(Default::default);

/// 쥐고 있는 동안 이 vault에 쓸 수 있다. 놓으면 파일 잠금과 줄이 함께 풀린다.
pub struct WriteGuard {
    line: Arc<Line>,
    file: File,
}

impl Drop for WriteGuard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
        let mut ticket = self.line.ticket.lock().unwrap();
        ticket.serving += 1;
        self.line.changed.notify_all();
    }
}

/// 기본 잠금 폴더: `$AISURADA_LOCK_DIR`, 없으면 `$HOME/Library/Application Support/aisurada/locks`
pub fn default_lock_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os(LOCK_DIR_ENV).filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join("Library/Application Support/aisurada/locks")
}

/// vault의 잠금 파일 경로. 이름은 vault 실제 경로(심볼릭 링크를 푼 것)의 SHA-256이다.
pub fn lock_file_path(vault: &Path, lock_dir: &Path) -> PathBuf {
    let real = fs::canonicalize(vault).unwrap_or_else(|_| absolute_normalized(vault));
    let digest = Sha256::digest(real.to_string_lossy().as_bytes());
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    lock_dir.join(format!("{hex}.lock"))
}

/// 쓰기 잠금을 잡을 때까지 기다린다.
pub fn acquire(vault: &Path, lock_dir: &Path) -> Result<WriteGuard, EngineError> {
    let line = line_of(vault);
    let ticket = {
        let mut state = line.ticket.lock().unwrap();
        let mine = state.next;
        state.next += 1;
        while state.serving != mine {
            state = line.changed.wait(state).unwrap();
        }
        mine
    };
    let _ = ticket;
    match open_lock_file(vault, lock_dir).and_then(|file| lock_file(file, true)) {
        Ok(Some(file)) => Ok(WriteGuard { line, file }),
        Ok(None) => unreachable!("기다리는 잠금은 None을 돌려주지 않는다"),
        Err(error) => {
            release_line(&line);
            Err(error)
        }
    }
}

/// 기다리지 않고 한 번 잡아 본다. 다른 쪽이 쥐고 있으면 `None`이다(자동 커밋 정비가 쓰기 중이면 건너뛸 때 쓴다).
pub fn try_acquire(vault: &Path, lock_dir: &Path) -> Result<Option<WriteGuard>, EngineError> {
    let line = line_of(vault);
    {
        let mut state = line.ticket.lock().unwrap();
        if state.next != state.serving {
            return Ok(None);
        }
        state.next += 1;
    }
    match open_lock_file(vault, lock_dir).and_then(|file| lock_file(file, false)) {
        Ok(Some(file)) => Ok(Some(WriteGuard { line, file })),
        Ok(None) => {
            release_line(&line);
            Ok(None)
        }
        Err(error) => {
            release_line(&line);
            Err(error)
        }
    }
}

fn line_of(vault: &Path) -> Arc<Line> {
    let key = absolute_normalized(vault).to_string_lossy().into_owned();
    LINES
        .lock()
        .unwrap()
        .entry(key)
        .or_insert_with(|| {
            Arc::new(Line {
                ticket: Mutex::new(Ticket {
                    next: 0,
                    serving: 0,
                }),
                changed: Condvar::new(),
            })
        })
        .clone()
}

fn release_line(line: &Line) {
    let mut ticket = line.ticket.lock().unwrap();
    ticket.serving += 1;
    line.changed.notify_all();
}

fn open_lock_file(vault: &Path, lock_dir: &Path) -> Result<File, EngineError> {
    let path = lock_file_path(vault, lock_dir);
    let failure = |e: std::io::Error| {
        EngineError::internal(format!(
            "쓰기 잠금 파일을 못 열었다: {}: {e}",
            path.display()
        ))
    };
    fs::create_dir_all(lock_dir).map_err(failure)?;
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(failure)
}

fn lock_file(file: File, wait: bool) -> Result<Option<File>, EngineError> {
    if wait {
        file.lock()
            .map_err(|e| EngineError::internal(format!("쓰기 잠금을 못 잡았다: {e}")))?;
        return Ok(Some(file));
    }
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(e)) => {
            Err(EngineError::internal(format!("쓰기 잠금을 못 잡았다: {e}")))
        }
    }
}

/// 파일 잠금(`flock`) 어댑터. 잠금 폴더를 만들 때 정하지 않으면 잡을 때마다 [`default_lock_dir`]을 읽는다.
#[derive(Default)]
pub struct FlockLock {
    dir: Option<PathBuf>,
}

impl FlockLock {
    /// 잠금 폴더를 못 박는다(시험이 쓴다).
    #[cfg(feature = "test-support")]
    pub fn in_dir(dir: impl Into<PathBuf>) -> Self {
        FlockLock {
            dir: Some(dir.into()),
        }
    }

    fn dir(&self) -> PathBuf {
        self.dir.clone().unwrap_or_else(default_lock_dir)
    }
}

impl VaultLock for FlockLock {
    fn acquire(&self, vault: &Path) -> Result<LockHandle, EngineError> {
        acquire(vault, &self.dir()).map(LockHandle::new)
    }

    fn try_acquire(&self, vault: &Path) -> Result<Option<LockHandle>, EngineError> {
        Ok(try_acquire(vault, &self.dir())?.map(LockHandle::new))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn scratch() -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ais-lock-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("vault")).unwrap();
        dir
    }

    #[test]
    fn lock_file_name_is_the_real_path_hash() {
        let dir = scratch();
        let direct = lock_file_path(&dir.join("vault"), &dir.join("locks"));
        let via_dots = lock_file_path(&dir.join("vault/../vault"), &dir.join("locks"));
        assert_eq!(direct, via_dots);
        let name = direct.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(name.len(), 64 + ".lock".len());
        assert!(name.ends_with(".lock"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn try_acquire_fails_while_held_and_works_after_release() {
        let dir = scratch();
        let (vault, locks) = (dir.join("vault"), dir.join("locks"));
        let held = acquire(&vault, &locks).unwrap();
        assert!(try_acquire(&vault, &locks).unwrap().is_none());
        drop(held);
        assert!(try_acquire(&vault, &locks).unwrap().is_some());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn another_process_view_is_blocked_by_the_file_lock() {
        // 같은 프로세스의 줄을 거치지 않고 잠금 파일만 직접 열어 프로세스 간 잠금을 흉내 낸다
        let dir = scratch();
        let (vault, locks) = (dir.join("vault"), dir.join("locks"));
        let held = acquire(&vault, &locks).unwrap();
        let other = open_lock_file(&vault, &locks).unwrap();
        assert!(matches!(other.try_lock(), Err(TryLockError::WouldBlock)));
        drop(held);
        assert!(other.try_lock().is_ok());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn threads_take_turns_in_arrival_order() {
        let dir = scratch();
        let (vault, locks) = (dir.join("vault"), dir.join("locks"));
        let order = Arc::new(Mutex::new(Vec::new()));
        let first = acquire(&vault, &locks).unwrap();
        let mut handles = Vec::new();
        for i in 0..3 {
            let (vault, locks, order) = (vault.clone(), locks.clone(), order.clone());
            handles.push(std::thread::spawn(move || {
                let _guard = acquire(&vault, &locks).unwrap();
                order.lock().unwrap().push(i);
                std::thread::sleep(Duration::from_millis(5));
            }));
            // 스레드가 차례로 줄에 서도록 잠깐 기다린다
            std::thread::sleep(Duration::from_millis(60));
        }
        drop(first);
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(*order.lock().unwrap(), vec![0, 1, 2]);
        fs::remove_dir_all(&dir).unwrap();
    }
}
