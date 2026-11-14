//! `commit_vault`: 대기 중인 변경을 지금 한 번 커밋해 본다(재시도 시각을 기다리지 않는다). 앱을 열고 닫을 때 부른다.

use std::path::Path;

use serde::Serialize;

use crate::write::autocommit::AutoCommit;

#[derive(Debug, Serialize)]
pub struct CommitVaultResponse {
    /// `committed`, `nothing`, `skipped`, `failed`, `disabled`
    pub outcome: &'static str,
    /// 건너뛰었거나 꺼졌을 때의 이유(`off`, `not-repo`, `index-lock`, `in-progress`, `not-main`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked: Option<&'static str>,
}

pub fn execute(vault: &Path) -> CommitVaultResponse {
    let result = AutoCommit::serving(vault).commit("restore");
    CommitVaultResponse {
        outcome: result.outcome,
        blocked: result.blocked,
    }
}
