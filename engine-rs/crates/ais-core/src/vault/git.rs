//! git 명령 실행. 실제 실행은 버전 관리 포트(`ports::VersionControl`, 기본 어댑터 `adapters/git_cli.rs`)가 한다.
//!
//! 실행하지 못했거나 시간이 넘은 것은 코드 -1에 빈 출력으로 돌려줍니다. 부르는 쪽은 실패를 그 걸음의 빈 결과로 봅니다.

use std::path::{Path, PathBuf};

pub use crate::ports::CommandResult;
use crate::ports::{self, GitMode};

pub struct Git {
    vault: PathBuf,
    mode: GitMode,
}

impl Git {
    /// vault의 `.git`과 작업 트리를 환경 변수로 못 박고, 훅·서명·경로 따옴표를 끈다. 60초 제한.
    pub fn strict(vault: &Path) -> Self {
        Git {
            vault: vault.to_path_buf(),
            mode: GitMode::Strict,
        }
    }

    /// 환경과 설정을 그대로 물려받는다. 5초 제한. 옛 스냅숏 참조를 읽는 데 쓴다.
    pub fn legacy(vault: &Path) -> Self {
        Git {
            vault: vault.to_path_buf(),
            mode: GitMode::Legacy,
        }
    }

    /// `args`로 git을 돌리고 `input`을 표준 입력으로 준다. 오류 출력은 버린다.
    pub fn run(&self, args: &[&str], input: &[u8]) -> CommandResult {
        ports::vcs().run(&self.vault, self.mode, args, input)
    }
}
