//! git 실행 파일이 없는 기계에서 엔진이 어떻게 무너지는지의 시험. git을 부르는 걸음은 모두 「빈 결과」가 되고, 노트를 읽고 쓰는 길은 그대로다.
//!
//! 이 파일은 시험이 하나뿐이고 프로세스 환경 변수를 한 번만 정한다(자동 커밋을 켠다).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use ais_core::adapters::git_cli::GitCli;
use ais_core::engine::Engine;
use serde_json::json;

/// 노트의 판 이력까지 git으로 만든 합성 vault
fn fixture_with_git_history() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ais-git-missing-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let tools = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/compat");
    for script in ["make-fixture.sh", "add-git-history.sh"] {
        let output = Command::new("bash")
            .arg(tools.join(script))
            .arg(&dir)
            .output()
            .expect("bash를 못 돌렸다");
        assert!(
            output.status.success(),
            "{script}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    dir
}

#[test]
fn a_missing_git_binary_turns_git_steps_into_empty_results() {
    let locks = std::env::temp_dir().join(format!("ais-git-missing-locks-{}", std::process::id()));
    // SAFETY: 이 파일의 유일한 시험이고, 다른 스레드가 환경 변수를 읽기 전에 정한다.
    unsafe {
        std::env::set_var("AISURADA_LOCK_DIR", &locks);
        std::env::set_var("AISURADA_VAULT_AUTOCOMMIT_ENABLED", "true");
    }
    let vault = fixture_with_git_history();
    std::fs::write(vault.join(".aisurada/workspace.json"), "{}\n").unwrap();
    let parent = "ndfx0002-00000002";

    // 같은 vault를 진짜 git으로 읽으면 판이 여섯 개다
    let real = Engine::new(&vault);
    assert_eq!(
        real.call("history", json!({"id": parent})).unwrap()["count"],
        6
    );

    // 실행 파일을 못 찾는 git으로 바꿔 끼운다
    let mut ports = ais_core::adapters::default_ports();
    ports.vcs = Arc::new(GitCli::with(
        vault.join("no-such-git"),
        Duration::from_secs(5),
        Duration::from_secs(5),
    ));
    let engine = Engine::with_ports(&vault, ports);

    // 판 목록은 오류가 아니라 빈 목록이다
    let history = engine.call("history", json!({"id": parent})).unwrap();
    assert_eq!(history["count"], 0, "{history}");
    assert_eq!(history["versions"].as_array().map(Vec::len), Some(0));

    // 즉시 커밋은 건너뛴다고 답하고 죽지 않는다(git을 못 돌려 main인지 알 수 없다)
    let commit = engine.call("commit_vault", json!({})).unwrap();
    assert_eq!(commit["outcome"], "skipped", "{commit}");

    // 노트를 읽고 쓰는 길은 git 없이도 그대로다. 쓰기는 커밋 앞뒤의 git 걸음이 비어도 성공한다
    let doc = engine.call("get_doc", json!({"id": parent})).unwrap();
    let token = doc["precondition"].as_str().unwrap().to_string();
    let written = engine
        .call("set_favorite", json!({"id": parent, "f1": token}))
        .unwrap();
    assert_eq!(written["changed"], true, "{written}");
    let again = engine.call("commit_vault", json!({})).unwrap();
    assert_eq!(again["outcome"], "skipped", "{again}");

    let _ = std::fs::remove_dir_all(&vault);
    let _ = std::fs::remove_dir_all(&locks);
}
