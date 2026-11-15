//! 어댑터: `ports`의 약속을 실제로 해내는 바깥 코드. 시계와 시간대, 난수, 환경 변수, 파일 잠금, git 실행, sqlite3 실행이 여기 있다.
//!
//! 이 모듈 밖에서는 `SystemTime::now`, `std::env::var`, `Command::new`, `tzset`·`localtime_r`을 부르지 않는다(`tests/architecture.rs`).
//! 기본 어댑터는 지금까지 엔진이 하던 대로 읽는다: 시계는 `AISURADA_TEST_NOW`가 있으면 그 값, 난수는 `AISURADA_TEST_RANDOM` 목록을 차례로 쓰고
//! 다 쓰면 운영체제 난수다. 그래서 맞대기 하네스가 그대로 돈다.

pub mod flock_lock;
pub mod git_cli;
pub mod process_env;
pub mod queue_ids;
pub mod sqlite_cli;
pub mod system_clock;

use std::sync::Arc;

use crate::ports::Ports;

/// 기본 어댑터 묶음(환경 변수와 실제 시계)
pub fn default_ports() -> Ports {
    Ports {
        clock: Arc::new(system_clock::SystemClock),
        ids: Arc::new(queue_ids::QueueIds),
        env: Arc::new(process_env::ProcessEnv),
        lock: Arc::new(flock_lock::FlockLock::default()),
        vcs: Arc::new(git_cli::GitCli),
        sqlite: Arc::new(sqlite_cli::SqliteCli),
    }
}
