//! git 명령 실행. 셸 없이 인자를 그대로 넘기고, 시간이 지나면 죽입니다.
//!
//! 실행하지 못했거나 시간이 넘은 것은 코드 -1에 빈 출력으로 돌려줍니다. 부르는 쪽은 실패를 그 걸음의 빈 결과로 봅니다.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

/// git 한 번의 결과
pub struct CommandResult {
    pub code: i32,
    pub output: Vec<u8>,
}

impl CommandResult {
    const FAILED: CommandResult = CommandResult {
        code: -1,
        output: Vec::new(),
    };

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.output).into_owned()
    }
}

/// 실행 규약
#[derive(Clone, Copy)]
enum Mode {
    /// vault의 `.git`과 작업 트리를 환경 변수로 못 박고, 훅·서명·경로 따옴표를 끈다. 60초 제한.
    Strict,
    /// 환경과 설정을 그대로 물려받는다. 5초 제한. 옛 스냅숏 참조를 읽는 데 쓴다.
    Legacy,
}

pub struct Git {
    vault: PathBuf,
    mode: Mode,
}

impl Git {
    pub fn strict(vault: &Path) -> Self {
        Git {
            vault: vault.to_path_buf(),
            mode: Mode::Strict,
        }
    }

    pub fn legacy(vault: &Path) -> Self {
        Git {
            vault: vault.to_path_buf(),
            mode: Mode::Legacy,
        }
    }

    /// `args`로 git을 돌리고 `input`을 표준 입력으로 준다. 오류 출력은 버린다.
    pub fn run(&self, args: &[&str], input: &[u8]) -> CommandResult {
        let mut command = Command::new("git");
        let timeout = match self.mode {
            Mode::Strict => {
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
                    .env("GIT_DIR", self.vault.join(".git"))
                    .env("GIT_WORK_TREE", &self.vault);
                Duration::from_secs(60)
            }
            Mode::Legacy => Duration::from_secs(5),
        };
        command
            .args(args)
            .current_dir(&self.vault)
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
