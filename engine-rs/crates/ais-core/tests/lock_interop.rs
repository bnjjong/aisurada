//! Rust 쓰기 잠금과 Java 엔진의 쓰기 파이프라인이 서로를 막는지 실제 `ais-engine.jar`로 확인합니다.
//!
//! `AISURADA_JAR`(기준 엔진 `engine-rs/target/oracle/ais-engine.jar`의 경로, `tools/compat/build-oracle.sh`가 짓는다)가 없으면 건너뜁니다. 사용:
//! `AISURADA_JAR=$PWD/target/oracle/ais-engine.jar cargo test -p ais-core --test lock_interop -- --nocapture`

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

use ais_core::adapters::flock_lock as lock;

#[test]
fn a_java_write_waits_while_rust_holds_the_write_lock() {
    let Some(jar) = std::env::var_os("AISURADA_JAR") else {
        eprintln!("AISURADA_JAR가 없어 건너뛴다");
        return;
    };
    let dir = std::env::temp_dir().join(format!("ais-lock-interop-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (vault, locks, home): (PathBuf, PathBuf, PathBuf) =
        (dir.join("vault"), dir.join("locks"), dir.join("home"));
    for d in [&vault, &home] {
        std::fs::create_dir_all(d).unwrap();
    }
    let java = |args: &[&str]| {
        Command::new("java")
            .arg(format!("-Duser.home={}", home.display()))
            .arg(format!("-Daisurada.lock.dir={}", locks.display()))
            .arg("-jar")
            .arg(&jar)
            .args(args)
            .arg(format!("--aisurada.vault={}", vault.display()))
            .output()
            .unwrap()
    };
    assert!(java(&["init-workspace", "name=lock"]).status.success());

    // 잠금이 비어 있으면 Java 쓰기가 바로 끝난다
    let started = Instant::now();
    assert!(java(&["create-agenda", "title=빠른"]).status.success());
    let free = started.elapsed();

    // Rust가 쥔 동안에는 놓을 때까지 기다린다
    let guard = lock::acquire(&vault, &locks).unwrap();
    let holder = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(2500));
        drop(guard);
    });
    let started = Instant::now();
    assert!(java(&["create-agenda", "title=기다린"]).status.success());
    let waited = started.elapsed();
    holder.join().unwrap();
    assert!(
        waited >= Duration::from_millis(2000),
        "Java 쓰기가 잠금을 기다리지 않았다: {waited:?} (잠금이 비었을 때 {free:?})"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
