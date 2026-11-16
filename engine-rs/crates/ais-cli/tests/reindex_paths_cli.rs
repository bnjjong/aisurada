//! `reindex-paths`의 명령줄 문법. 지운 Java `ReindexPathsTest`의 문법 부분을 실제 `ais-cli` 실행 파일로 옮긴 것이다.
//! (경로 고르기와 진단은 엔진 쪽 `ais-core/tests/reindex_paths.rs`에 있다.)

use std::path::PathBuf;
use std::process::Command;

fn run(vault: &std::path::Path, locks: &std::path::Path, args: &[&str]) -> (i32, String, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_ais-cli"))
        .args(args)
        .env_clear()
        .env("AISURADA_VAULT", vault)
        .env("AISURADA_LOCK_DIR", locks)
        .output()
        .expect("ais-cli를 못 돌렸다");
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn reindex_paths_command_syntax() {
    let base: PathBuf =
        std::env::temp_dir().join(format!("ais-reindex-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let vault = base.join("vault");
    std::fs::create_dir_all(vault.join(".aisurada/agendas")).unwrap();
    std::fs::create_dir_all(vault.join("notes/demo/a")).unwrap();
    std::fs::write(
        vault.join(".aisurada/agendas/demo.md"),
        "---\nid: agfixture-00000001\nslug: demo\ntitle: 데모\n---\n",
    )
    .unwrap();
    std::fs::write(
        vault.join("notes/demo/a/note.md"),
        "---\nid: ndfixture-00000001\nagenda: demo\ntitle: 검사\ntype: free\ncreated: '2026-06-16T10:00:00+09:00'\n---\n본문\n",
    )
    .unwrap();
    let locks = base.join("locks");

    // 모르는 옵션은 AIS-400이고 글이 바이트까지 같다(종료 코드 3)
    let (code, _, stderr) = run(&vault, &locks, &["reindex-paths", "abc=def"]);
    assert_eq!(code, 3);
    assert_eq!(
        stderr,
        "AIS-400 reindex-paths 이 모르는 옵션이다: abc. 쓸 수 있는 것은 [] 뿐이다.\n"
    );

    // `--`로 시작하는 칸은 버리고 나머지 경로로 돌린다
    let (code, stdout, _) = run(
        &vault,
        &locks,
        &["reindex-paths", "--unknown=x", "notes/demo/a"],
    );
    assert_eq!(code, 0);
    assert!(
        stdout.contains("\"mode\": \"paths\"") && stdout.contains("\"indexed\": 1"),
        "{stdout}"
    );

    // 도구 표면(밑줄 이름)은 없다: 사용법 오류(종료 코드 2)
    let (code, _, stderr) = run(&vault, &locks, &["reindex_paths"]);
    assert_eq!(code, 2);
    assert!(
        stderr.starts_with("알 수 없는 이름: reindex_paths"),
        "{stderr}"
    );

    let _ = std::fs::remove_dir_all(&base);
}
