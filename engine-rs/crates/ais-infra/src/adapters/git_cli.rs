//! git 명령 실행 어댑터. 셸 없이 인자를 그대로 넘기고, 시간이 지나면 죽인다.
//!
//! 실행하지 못했거나 시간이 넘은 것은 코드 -1에 빈 출력으로 돌려준다. 부르는 쪽은 실패를 그 걸음의 빈 결과로 본다.

use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::ports::{CommandResult, GitMode, VersionControl};

/// 실제 `git`을 돌린다. 실행 파일과 제한 시간은 기본값이 쓰이는 값이고, 시간 초과와 git 없음 경로를 시험하려고 바꿀 수 있다.
pub struct GitCli {
    program: OsString,
    strict_timeout: Duration,
    legacy_timeout: Duration,
}

impl Default for GitCli {
    /// `PATH`의 `git`, 엄격한 실행 60초, 옛 스냅숏 읽기 5초
    fn default() -> Self {
        GitCli::with("git", Duration::from_secs(60), Duration::from_secs(5))
    }
}

impl GitCli {
    /// 실행 파일과 제한 시간을 정해 만든다(시험용).
    pub fn with(
        program: impl Into<OsString>,
        strict_timeout: Duration,
        legacy_timeout: Duration,
    ) -> Self {
        GitCli {
            program: program.into(),
            strict_timeout,
            legacy_timeout,
        }
    }
}

impl VersionControl for GitCli {
    fn run(&self, vault: &Path, mode: GitMode, args: &[&str], input: &[u8]) -> CommandResult {
        let mut command = Command::new(&self.program);
        let timeout = match mode {
            GitMode::Strict => {
                command
                    .args([
                        "-c",
                        "core.hooksPath=/dev/null",
                        "-c",
                        "commit.gpgsign=false",
                        "-c",
                        "core.quotePath=false",
                    ])
                    .env_remove("GIT_INDEX_FILE")
                    .env("GIT_DIR", vault.join(".git"))
                    .env("GIT_WORK_TREE", vault);
                // 앱이 남기는 커밋의 작성자는 늘 aisurada-auto다(판 목록이 이것으로 자동·수동을 가른다)
                if args.first() == Some(&"commit") {
                    command
                        .env("GIT_AUTHOR_NAME", "aisurada-auto")
                        .env("GIT_COMMITTER_NAME", "aisurada-auto")
                        .env("GIT_AUTHOR_EMAIL", "noreply@aisurada.local")
                        .env("GIT_COMMITTER_EMAIL", "noreply@aisurada.local");
                }
                self.strict_timeout
            }
            GitMode::Legacy => self.legacy_timeout,
        };
        command
            .args(args)
            .current_dir(vault)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let Ok(mut child) = command.spawn() else {
            return CommandResult::FAILED;
        };

        // 입력 쓰기와 출력 읽기를 따로 돌려 파이프가 차서 서로 막히는 일을 피한다
        let stdin = child.stdin.take();
        let input = input.to_vec();
        let writer = thread::spawn(move || {
            if let Some(mut stdin) = stdin {
                let _ = stdin.write_all(&input);
            }
        });
        let stdout = child.stdout.take();
        let reader = thread::spawn(move || {
            let mut buffer = Vec::new();
            if let Some(mut stdout) = stdout {
                let _ = stdout.read_to_end(&mut buffer);
            }
            buffer
        });

        let deadline = Instant::now() + timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(2)),
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
            }
        };
        let output = reader.join().unwrap_or_default();
        let _ = writer.join();
        match status {
            Some(status) => CommandResult {
                code: status.code().unwrap_or(-1),
                output,
            },
            None => CommandResult::FAILED,
        }
    }
}
