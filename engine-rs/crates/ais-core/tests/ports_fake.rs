//! 포트 바꿔 끼우기 시험: 시계, 난수, 환경 변수, 잠금, git, sqlite3를 가짜로 끼운 엔진이 **환경 변수 없이** 결정적으로 일한다.
//!
//! 3단계(포트)의 목적을 보인다. 규칙을 담은 코드는 `SystemTime::now`나 `std::env::var`를 직접 부르지 않고 포트를 거치므로, 약속 묶음을 바꾸면
//! 시계와 난수와 설정을 시험이 쥔다. 이 파일은 시험이 하나뿐이고 프로세스 환경 변수를 하나도 건드리지 않는다.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use ais_core::adapters::flock_lock::FlockLock;
use ais_core::adapters::local_store::LocalStore;
use ais_core::engine::Engine;
use ais_core::kernel::time::CivilDate;
use ais_core::kernel::time::{LocalClock, civil_from_days};
use ais_core::ports::{
    Clock, CommandResult, EnvSource, GitMode, IdSource, Ports, SqliteReader, VersionControl,
};
use serde_json::json;

/// 2026-11-14T23:46:40Z
const FIXED_NOW: i64 = 1_794_700_000_000;

struct FixedClock;

impl Clock for FixedClock {
    fn now_millis(&self) -> i64 {
        FIXED_NOW
    }

    /// 가짜 시간대: UTC
    fn local_clock(&self, millis: i64) -> LocalClock {
        let (year, month, day) = civil_from_days(millis.div_euclid(86_400_000));
        let rest = millis.rem_euclid(86_400_000);
        LocalClock {
            date: CivilDate { year, month, day },
            hour: (rest / 3_600_000) as u32,
            minute: (rest % 3_600_000 / 60_000) as u32,
            second: (rest % 60_000 / 1000) as u32,
            offset_seconds: 0,
        }
    }
}

/// 00000001, 00000002, … 차례로 낸다
struct CountingIds(AtomicU64);

impl IdSource for CountingIds {
    fn next_random_hex(&self) -> String {
        format!("{:08x}", self.0.fetch_add(1, Ordering::SeqCst) + 1)
    }

    fn next_random_u64(&self) -> u64 {
        self.0.fetch_add(1, Ordering::SeqCst) + 1
    }
}

struct MapEnv(HashMap<String, OsString>);

impl EnvSource for MapEnv {
    fn var_os(&self, name: &str) -> Option<OsString> {
        self.0.get(name).cloned()
    }
}

/// git을 부르지 않는다(자동 커밋이 꺼져 있어 부를 일도 없지만, 불리면 알 수 있게 센다)
struct CountingVcs(Mutex<u32>);

impl VersionControl for CountingVcs {
    fn run(&self, _: &Path, _: GitMode, _: &[&str], _: &[u8]) -> CommandResult {
        *self.0.lock().unwrap() += 1;
        CommandResult::FAILED
    }
}

struct NoSqlite;

impl SqliteReader for NoSqlite {
    fn query(&self, _: &str, _: &str) -> Result<String, String> {
        Err("가짜 sqlite3".to_string())
    }
}

fn make_fixture(dir: &Path) {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/compat/make-fixture.sh");
    let out = Command::new("bash")
        .arg(script)
        .arg(dir)
        .output()
        .expect("bash를 못 돌렸다");
    assert!(
        out.status.success(),
        "합성 vault를 못 만들었다: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    std::fs::write(dir.join(".aisurada/workspace.json"), "{}\n").unwrap();
}

#[test]
fn an_engine_with_fake_ports_works_without_any_environment_variable() {
    // 이 프로세스에 시험용 변수가 없어야 한다(있으면 기본 어댑터와 구별이 안 된다)
    for name in ["AISURADA_TEST_NOW", "AISURADA_TEST_RANDOM"] {
        assert!(std::env::var_os(name).is_none(), "{name}이 설정돼 있다");
    }
    let root: PathBuf = std::env::temp_dir().join(format!("ais-ports-fake-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let vault = root.join("vault");
    make_fixture(&vault);

    let env = MapEnv(HashMap::from([
        (
            "AISURADA_VAULT_AUTOCOMMIT_ENABLED".to_string(),
            OsString::from("false"),
        ),
        (
            "AISURADA_APPROVALS_DIR".to_string(),
            root.join("approvals").into_os_string(),
        ),
    ]));
    let vcs = Arc::new(CountingVcs(Mutex::new(0)));
    let ports = Ports {
        clock: Arc::new(FixedClock),
        ids: Arc::new(CountingIds(AtomicU64::new(0))),
        env: Arc::new(env),
        lock: Arc::new(FlockLock::in_dir(root.join("locks"))),
        vcs: vcs.clone(),
        sqlite: Arc::new(NoSqlite),
        store: Arc::new(LocalStore),
    };
    let engine = Engine::with_ports(&vault, ports);

    let created = engine
        .call(
            "create_doc",
            json!({"agenda": "demo", "type": "free", "title": "가짜 포트", "body": "# 가짜 포트\n"}),
        )
        .unwrap();

    // 노트 id는 가짜 시계(36진수 밀리초)와 가짜 난수로 정해진다
    let base36 = ais_core::write::apparatus::to_base36(FIXED_NOW);
    assert_eq!(created["id"], format!("nd{base36}-00000001"), "{created}");
    // 날짜와 시각은 가짜 시계와 가짜 시간대(UTC)로 정해진다
    assert_eq!(created["date"], "2026-11-14", "{created}");
    assert!(
        created["path"]
            .as_str()
            .unwrap()
            .contains("2026-11-14-2346-"),
        "{created}"
    );
    // 잠금 파일은 환경 변수(`AISURADA_LOCK_DIR`)가 아니라 약속에 준 폴더에 생겼다
    let locks = std::fs::read_dir(root.join("locks")).expect("가짜 잠금 폴더");
    assert!(
        locks
            .flatten()
            .any(|e| e.file_name().to_string_lossy().ends_with(".lock"))
    );
    // 자동 커밋은 가짜 환경 변수로 꺼졌으니 git을 부르지 않았다
    assert_eq!(*vcs.0.lock().unwrap(), 0);
    // 같은 프로세스의 기본 어댑터에는 영향이 없다(걸었던 묶음은 호출이 끝나면 풀린다)
    let now = ais_core::ports::clock().now_millis();
    assert_ne!(now, FIXED_NOW, "가짜 시계가 호출 밖으로 샜다");
    let _ = std::fs::remove_dir_all(&root);
}
