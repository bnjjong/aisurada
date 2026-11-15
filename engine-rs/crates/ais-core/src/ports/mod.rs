//! 포트: 안쪽 코드가 바깥(시계, 난수, 환경 변수, 파일 잠금, git, sqlite3)에 바라는 일을 적은 약속(`trait`).
//!
//! 규칙과 순서를 담은 코드는 `SystemTime::now`, `std::env::var`, `Command::new`, `tzset`을 **직접 부르지 않고** 이 약속을 거친다.
//! 약속을 실제로 해내는 바깥 코드(어댑터)는 `adapters/`에 있다. 어댑터 밖에서 위 호출이 0건인지는 `tests/architecture.rs`가 막는다.
//!
//! **어떻게 건네나.** `Engine`이 약속 묶음([`Ports`])을 들고 있다가 명령을 부르는 동안 현재 스레드에 걸어 둔다([`install`]). 안쪽 코드는
//! [`clock`], [`ids`], [`env`], [`lock`], [`vcs`], [`sqlite`]로 현재 묶음을 꺼내 쓴다. 묶음을 걸지 않으면 기본 어댑터(지금처럼 환경 변수와 실제
//! 시계, 정해 둔 난수 목록을 읽는다)를 쓴다. 그래서 맞대기 하네스와 골든 시험이 그대로 돈다.
//! 명령 함수마다 맥락(`&Ctx`)을 넘기는 방식은 호출부가 백 곳이 넘어서 택하지 않았고, 호출자 맥락을 요청 밖으로 빼는 7단계(`CallContext`)에서 다시 본다.
//! 스레드를 새로 만들어 일을 시키는 코드(자동 커밋의 주기 스레드)는 걸린 묶음을 물려받지 않고 기본 어댑터를 쓴다.

use std::cell::RefCell;
use std::ffi::OsString;
use std::path::Path;
use std::sync::{Arc, LazyLock};

use crate::error::EngineError;
use crate::kernel::time::LocalClock;

/// 시계: 지금과, 시각을 이 프로세스의 시간대로 푼 날짜·시각
pub trait Clock: Send + Sync {
    /// 지금의 유닉스 밀리초
    fn now_millis(&self) -> i64;
    /// 유닉스 밀리초를 이 프로세스의 시간대로 풀어 날짜와 시, 분, 초(초 아래는 버림)를 돌려준다. 시간대를 못 읽으면 UTC로 본다.
    fn local_clock(&self, millis: i64) -> LocalClock;
}

/// 난수: id 꼬리로 쓸 값과 임시 파일 이름에 쓸 값
pub trait IdSource: Send + Sync {
    /// 소문자 16진수 8자리
    fn next_random_hex(&self) -> String;
    /// 64비트 난수
    fn next_random_u64(&self) -> u64;
    /// 시험용 난수 목록을 다시 읽는다. 목록이 없는 구현은 아무것도 하지 않는다.
    fn reset(&self) {}
}

/// 환경 변수(홈 폴더와 시험용·설정용 변수)
pub trait EnvSource: Send + Sync {
    fn var_os(&self, name: &str) -> Option<OsString>;

    /// 유니코드가 아니면 `None`이다(`std::env::var(..).ok()`와 같다).
    fn var(&self, name: &str) -> Option<String> {
        self.var_os(name).and_then(|v| v.into_string().ok())
    }
}

/// 쥐고 있는 동안 vault에 쓸 수 있는 잠금. 놓으면(drop) 풀린다.
pub struct LockHandle {
    _guard: Box<dyn Send>,
}

impl LockHandle {
    pub fn new(guard: impl Send + 'static) -> Self {
        LockHandle {
            _guard: Box::new(guard),
        }
    }
}

/// vault별 쓰기 잠금
pub trait VaultLock: Send + Sync {
    /// 잠금을 잡을 때까지 기다린다.
    fn acquire(&self, vault: &Path) -> Result<LockHandle, EngineError>;
    /// 기다리지 않고 한 번 잡아 본다. 다른 쪽이 쥐고 있으면 `None`이다.
    fn try_acquire(&self, vault: &Path) -> Result<Option<LockHandle>, EngineError>;
}

/// 부를 때마다 현재 스레드의 잠금 약속으로 넘기는 잠금. 프로세스에 오래 사는 객체(자동 커밋 기록)가 어느 묶음에 매이지 않게 한다.
pub struct CurrentLock;

impl VaultLock for CurrentLock {
    fn acquire(&self, vault: &Path) -> Result<LockHandle, EngineError> {
        lock().acquire(vault)
    }

    fn try_acquire(&self, vault: &Path) -> Result<Option<LockHandle>, EngineError> {
        lock().try_acquire(vault)
    }
}

/// git 한 번의 결과. 실행하지 못했거나 시간이 넘은 것은 코드 -1에 빈 출력이다.
pub struct CommandResult {
    pub code: i32,
    pub output: Vec<u8>,
}

impl CommandResult {
    pub const FAILED: CommandResult = CommandResult {
        code: -1,
        output: Vec::new(),
    };

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.output).into_owned()
    }
}

/// git 실행 규약
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitMode {
    /// vault의 `.git`과 작업 트리를 못 박고 훅·서명·경로 따옴표를 끈다(60초 제한)
    Strict,
    /// 환경과 설정을 그대로 물려받는다(5초 제한). 옛 스냅숏 참조를 읽는 데 쓴다.
    Legacy,
}

/// 버전 관리(git)
pub trait VersionControl: Send + Sync {
    /// `vault`에서 git을 `args`로 돌리고 `input`을 표준 입력으로 준다. 오류 출력은 버린다.
    fn run(&self, vault: &Path, mode: GitMode, args: &[&str], input: &[u8]) -> CommandResult;
}

/// sqlite3로 읽기 전용 질의를 한다(Antigravity 대화 기록)
pub trait SqliteReader: Send + Sync {
    /// 열(`|`로 가른) 줄들의 표준 출력 글을 돌려준다. 못 찾거나 못 열거나 시간이 넘으면 사람이 읽을 이유가 든 `Err`다.
    fn query(&self, uri: &str, sql: &str) -> Result<String, String>;
}

/// 약속 묶음. `Engine`이 들고 명령을 부르는 동안 현재 스레드에 건다.
#[derive(Clone)]
pub struct Ports {
    pub clock: Arc<dyn Clock>,
    pub ids: Arc<dyn IdSource>,
    pub env: Arc<dyn EnvSource>,
    pub lock: Arc<dyn VaultLock>,
    pub vcs: Arc<dyn VersionControl>,
    pub sqlite: Arc<dyn SqliteReader>,
}

static DEFAULT: LazyLock<Arc<Ports>> = LazyLock::new(|| Arc::new(crate::adapters::default_ports()));

thread_local! {
    static CURRENT: RefCell<Option<Arc<Ports>>> = const { RefCell::new(None) };
}

/// 현재 스레드의 약속 묶음. 걸어 둔 것이 없으면 기본 어댑터다.
pub fn current() -> Arc<Ports> {
    CURRENT
        .with(|c| c.borrow().clone())
        .unwrap_or_else(|| DEFAULT.clone())
}

pub fn clock() -> Arc<dyn Clock> {
    current().clock.clone()
}

pub fn ids() -> Arc<dyn IdSource> {
    current().ids.clone()
}

pub fn env() -> Arc<dyn EnvSource> {
    current().env.clone()
}

pub fn lock() -> Arc<dyn VaultLock> {
    current().lock.clone()
}

pub fn vcs() -> Arc<dyn VersionControl> {
    current().vcs.clone()
}

pub fn sqlite() -> Arc<dyn SqliteReader> {
    current().sqlite.clone()
}

/// 약속 묶음을 현재 스레드에 건 동안 살아 있다. 놓으면(drop) 앞에 걸려 있던 묶음으로 돌아간다.
pub struct Installed {
    previous: Option<Arc<Ports>>,
}

impl Drop for Installed {
    fn drop(&mut self) {
        CURRENT.with(|c| *c.borrow_mut() = self.previous.take());
    }
}

/// 이 묶음을 현재 스레드에 건다.
pub fn install(ports: Arc<Ports>) -> Installed {
    let previous = CURRENT.with(|c| c.borrow_mut().replace(ports));
    Installed { previous }
}
