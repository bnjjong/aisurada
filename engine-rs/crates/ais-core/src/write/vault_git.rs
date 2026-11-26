use std::{io::ErrorKind, path::Path};

use serde::{Deserialize, Serialize};

use super::{
    autocommit::{AutoCommit, AutoCommitConfig},
    pipeline::{acquire_lock, check_workspace_marker},
    write_atomic,
};
use crate::{error::EngineError, ports::fs, text::trim, vault::git::Git};

const PREFERENCE_PATH: &str = ".aisurada/vault-git.json";
// Git은 무시한 파일의 정확한 경로가 인자에 있으면 실패한다. 한 글자 패턴으로 같은 파일만 제외한다.
pub(super) const EXCLUDE_PREFERENCE: &str = ":(exclude,glob,top).aisurada/vault-git.jso[n]";
pub(super) const SETUP_REASON: &str = "setup";

#[derive(Deserialize)]
struct Preference {
    enabled: bool,
}

fn preference(vault: &Path) -> Result<Option<Preference>, EngineError> {
    match fs::read(vault.join(PREFERENCE_PATH)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|_| {
            EngineError::internal("워크스페이스의 Git 설정 파일을 해석하지 못했어요.")
        }),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(_) => Err(EngineError::internal(
            "워크스페이스의 Git 설정을 읽지 못했어요.",
        )),
    }
}

/// 앱과 명령줄 도구, 이미 실행 중인 MCP 서버가 같은 설정을 읽는다.
pub(super) fn allows_autocommit(vault: &Path) -> bool {
    preference(vault).is_ok_and(|value| value.is_none_or(|value| value.enabled))
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Phase {
    Off,
    Ready,
    NeedsSetup,
    GitUnavailable,
    Unsupported,
    OtherBranch,
    Paused,
}

#[derive(Debug, Serialize)]
pub struct Status {
    enabled: bool,
    phase: Phase,
}

pub fn status(vault: &Path) -> Result<Status, EngineError> {
    check_workspace_marker(vault)?;
    let saved = preference(vault)?;
    let enabled = saved.map_or_else(|| vault.join(".git").is_dir(), |value| value.enabled);
    Ok(Status {
        enabled,
        phase: if enabled {
            enabled_phase(vault)?
        } else {
            Phase::Off
        },
    })
}

fn enabled_phase(vault: &Path) -> Result<Phase, EngineError> {
    if !AutoCommitConfig::from_env().enabled {
        return Ok(Phase::Paused);
    }
    let git = Git::strict(vault);
    if git.run(&["--version"], &[]).code != 0 {
        return Ok(Phase::GitUnavailable);
    }
    let git_dir = vault.join(".git");
    match fs::symlink_metadata(&git_dir) {
        Ok(_) if git_dir.is_dir() => {}
        Ok(_) => return Ok(Phase::Unsupported),
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Phase::NeedsSetup),
        Err(_) => {
            return Err(EngineError::internal(
                "워크스페이스의 .git을 확인하지 못했어요.",
            ));
        }
    }
    let head = git.run(&["rev-parse", "--verify", "--quiet", "HEAD^{commit}"], &[]);
    let branch = git.run(&["symbolic-ref", "-q", "HEAD"], &[]);
    Ok(match (trim(&branch.text()), head.code) {
        ("refs/heads/main", 0) => Phase::Ready,
        ("refs/heads/main", _) => Phase::NeedsSetup,
        (_, 0) => Phase::OtherBranch,
        (_, _) if branch.code == 0 => Phase::OtherBranch,
        _ => Phase::Unsupported,
    })
}

pub fn set_enabled(vault: &Path, enabled: bool) -> Result<Status, EngineError> {
    let _guard = acquire_lock(vault)?;
    check_workspace_marker(vault)?;
    let bytes: &[u8] = if enabled {
        b"{\"enabled\":true}\n"
    } else {
        b"{\"enabled\":false}\n"
    };
    write_atomic(&vault.join(PREFERENCE_PATH), bytes)?;

    let current = status(vault)?;
    if current.phase != Phase::NeedsSetup {
        return Ok(current);
    }

    if !vault.join(".git").is_dir() {
        if Git::strict(vault)
            .run(
                &["init", "--quiet", "--template=", "--initial-branch=main"],
                &[],
            )
            .code
            != 0
        {
            return Err(EngineError::internal(
                "워크스페이스에 Git 저장소를 만들지 못했어요.",
            ));
        }
        write_atomic(
            &vault.join(".git/info/exclude"),
            b"/.aisurada/vault-git.json\n",
        )?;
    }

    let commit = AutoCommit::serving(vault).commit(SETUP_REASON);
    if commit.outcome != "committed" {
        return Err(EngineError::internal(
            "워크스페이스의 첫 Git 커밋을 만들지 못했어요.",
        ));
    }
    status(vault)
}
