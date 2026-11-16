//! 34장 받아들임 묶음(`fixtures/expect`)을 Rust `ais-cli`로 돈다(이슈 #66).
//!
//! 묶음의 실행기(`fixtures/expect/run.mjs`, node 20 이상)가 케이스마다 `ais-cli`를 셸 없이 불러 종료 코드와 stdout 바이트를 맞댄다.
//! Rust는 JSON 객체 키를 이름 차례로 내서 Java 엔진(기대값의 출처)과 키 차례가 다르다. 그래서 어댑터
//! `fixtures/aisurada-rust.adapter.json`이 `compare.jsonKeyOrder=ignore`를 밝히고, 실행기는 키 차례만 빼고
//! 값과 들여쓰기·글자 꼴은 그대로 바이트로 맞댄다. 기대값은 설명서 원문 그대로다.
//!
//! node가 없는 기기에서는 건너뛴다(엔진만 만지는 사람이 node를 깔 필요는 없다). CI 러너에는 node가 있다.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn node_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

#[test]
fn rust_ais_cli_passes_every_acceptance_case() {
    if !node_available() {
        eprintln!("node가 없어 34장 받아들임 시험을 건너뛴다");
        return;
    }
    let root = repo_root();
    let out = std::env::temp_dir().join(format!("ais-acceptance-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);

    let output = Command::new("node")
        .current_dir(&root)
        .arg("fixtures/expect/run.mjs")
        .args(["--engine", "fixtures/aisurada-rust.adapter.json", "--out"])
        .arg(&out)
        .env("AISX_IMPL_BIN", env!("CARGO_BIN_EXE_ais-cli"))
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .output()
        .expect("node를 못 돌렸다");
    let log = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "34장 묶음이 Rust ais-cli와 어긋났다(종료 코드 {:?}).\n{log}",
        output.status.code()
    );

    let report: Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("report.json")).unwrap())
            .expect("report.json");
    let totals = &report["totals"];
    assert!(
        totals["cases"].as_u64().unwrap() >= 27,
        "케이스가 너무 적다: {totals}"
    );
    assert_eq!(totals["failed"], 0, "{log}");
    assert_eq!(totals["passed"], totals["cases"], "{log}");
    assert_eq!(report["engine"], "aisurada-rust");
    let _ = std::fs::remove_dir_all(&out);
}
