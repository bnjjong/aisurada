//! git 실행 어댑터(`GitCli`)의 시간 초과, git 없음, 인자·환경·표준 입력 전달 시험. 셸 스크립트로 만든 가짜 `git`을 쓴다.
//! 이슈 #57. 어댑터의 시험이지만 의존 기준(`architecture-baseline.txt`)이 `src`의 `fs::`를 세므로 `tests/`에 둔다.

use ais_core::adapters::git_cli::GitCli;
use ais_core::ports::{GitMode, VersionControl};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// 임시 폴더에 가짜 `git`(셸 스크립트)을 만든다. `body`가 스크립트 본문이다.
fn fake_git(body: &str) -> (PathBuf, PathBuf) {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "ais-gitcli-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let program = dir.join("git");
    fs::write(&program, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    (dir, program)
}

fn cli(program: &Path, strict_ms: u64, legacy_ms: u64) -> GitCli {
    GitCli::with(
        program,
        Duration::from_millis(strict_ms),
        Duration::from_millis(legacy_ms),
    )
}

#[test]
fn a_missing_git_binary_is_an_empty_failed_result() {
    let (dir, _) = fake_git("exit 0");
    let missing = dir.join("no-such-git");
    for mode in [GitMode::Strict, GitMode::Legacy] {
        let result = cli(&missing, 5_000, 5_000).run(&dir, mode, &["status"], b"");
        assert_eq!((result.code, result.output.len()), (-1, 0));
    }
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_slow_git_is_killed_at_the_strict_limit() {
    // exec로 sleep이 셸 자리를 이어받아야 kill이 그 프로세스를 죽인다
    let (dir, program) = fake_git("echo started\nexec sleep 30");
    let started = Instant::now();
    let result = cli(&program, 300, 30_000).run(&dir, GitMode::Strict, &["log"], b"");
    let took = started.elapsed();
    assert_eq!(
        (result.code, result.output.len()),
        (-1, 0),
        "시간이 넘으면 빈 실패"
    );
    assert!(
        took >= Duration::from_millis(300) && took < Duration::from_secs(10),
        "제한 시간 근처에서 끝나야 한다: {took:?}"
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_legacy_mode_has_its_own_shorter_limit() {
    let (dir, program) = fake_git("exec sleep 30");
    // 엄격한 쪽은 길고 옛 스냅숏 쪽만 짧다: 옛 쪽만 먼저 죽는다
    let started = Instant::now();
    let result = cli(&program, 30_000, 300).run(&dir, GitMode::Legacy, &["show"], b"");
    assert_eq!(result.code, -1);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn output_exit_code_and_stdin_pass_through() {
    // 인자를 한 줄씩, 표준 입력을 그대로 되돌리고 코드 3으로 끝난다
    let (dir, program) = fake_git("for a in \"$@\"; do echo \"arg:$a\"; done\ncat\nexit 3");
    let result = cli(&program, 10_000, 10_000).run(
        &dir,
        GitMode::Legacy,
        &["commit", "-F", "-"],
        "메시지\n".as_bytes(),
    );
    assert_eq!(result.code, 3);
    assert_eq!(result.text(), "arg:commit\narg:-F\narg:-\n메시지\n");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn strict_mode_pins_the_repository_and_turns_hooks_and_signing_off() {
    let (dir, program) = fake_git(
        "for a in \"$@\"; do echo \"arg:$a\"; done\necho \"dir:$GIT_DIR\"\necho \"tree:$GIT_WORK_TREE\"\necho \"author:$GIT_AUTHOR_NAME\"",
    );
    let git = cli(&program, 10_000, 10_000);
    let strict = git.run(&dir, GitMode::Strict, &["status"], b"").text();
    assert!(strict.starts_with("arg:-c\narg:core.hooksPath=/dev/null\narg:-c\narg:commit.gpgsign=false\narg:-c\narg:core.quotePath=false\narg:status\n"), "{strict}");
    assert!(
        strict.contains(&format!("dir:{}\n", dir.join(".git").display())),
        "{strict}"
    );
    assert!(
        strict.contains(&format!("tree:{}\n", dir.display())),
        "{strict}"
    );
    // 작성자 환경은 commit에만 붙는다(자동 커밋의 표지)
    let commit = git
        .run(&dir, GitMode::Strict, &["commit", "-q"], b"")
        .text();
    assert!(commit.contains("author:aisurada-auto\n"), "{commit}");
    // 옛 스냅숏 읽기는 환경과 설정을 그대로 물려받아 인자 앞에 설정을 붙이지 않는다
    let legacy = git.run(&dir, GitMode::Legacy, &["status"], b"").text();
    assert!(
        legacy.starts_with("arg:status\n") && !legacy.contains("hooksPath"),
        "{legacy}"
    );
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_large_input_and_output_do_not_deadlock_the_pipes() {
    // 파이프 용량(수십 KB)을 훨씬 넘는 입력을 그대로 되돌린다. 입력 쓰기와 출력 읽기가 따로 돌아야 막히지 않는다.
    let (dir, program) = fake_git("cat");
    let input = vec![b'x'; 3_000_000];
    let started = Instant::now();
    let result = cli(&program, 30_000, 30_000).run(&dir, GitMode::Legacy, &["x"], &input);
    assert_eq!(result.code, 0);
    assert_eq!(result.output.len(), input.len());
    assert!(started.elapsed() < Duration::from_secs(20));
    fs::remove_dir_all(&dir).unwrap();
}
