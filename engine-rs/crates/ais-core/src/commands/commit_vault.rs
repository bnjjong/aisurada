//! `commit_vault`: 대기 중인 변경을 지금 한 번 커밋해 본다(재시도 시각을 기다리지 않는다). 앱을 열고 닫을 때 부른다.

use std::path::Path;

use serde::Serialize;

use crate::error::EngineError;
use crate::write::{autocommit::AutoCommit, pipeline::acquire_lock};

#[derive(Debug, Serialize)]
pub struct CommitVaultResponse {
    /// `committed`, `nothing`, `skipped`, `failed`, `disabled`
    pub outcome: &'static str,
    /// 건너뛰었거나 꺼졌을 때의 이유(`off`, `not-repo`, `index-lock`, `in-progress`, `not-main`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<&'static str>,
}

/// 쓰기와 같은 vault 잠금을 잡고 커밋한다. 다른 쪽이 여러 파일을 쓰는 중에 커밋하면 쓰다 만 상태가 커밋에 들어가므로, 쓰는 중이면
/// 그 쓰기가 끝나기를 기다린다. 유휴 타이머의 정비 커밋은 같은 이유로 잠금을 잡되 기다리지 않고 건너뛴다.
pub fn execute(vault: &Path) -> Result<CommitVaultResponse, EngineError> {
    let _lock = acquire_lock(vault)?;
    let result = AutoCommit::serving(vault).commit("restore");
    Ok(CommitVaultResponse {
        outcome: result.outcome,
        blocked: result.blocked,
    })
}
