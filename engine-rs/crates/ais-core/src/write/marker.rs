//! 워크스페이스 표식(`.aisurada/workspace.json`) 검사. 쓰기 명령은 표식이 없는 폴더에서 일하지 않습니다.

use std::path::Path;

use super::confine::absolute_normalized;
use crate::error::EngineError;

/// `vault_root`에 표식이 있는지 본다(끊긴 링크는 없는 것으로 본다). 없으면 `AIS-400`이다.
pub fn check_workspace_marker(vault_root: &Path) -> Result<(), EngineError> {
    if vault_root.join(".aisurada/workspace.json").exists() {
        return Ok(());
    }
    Err(EngineError::invalid_argument(format!(
        "이 폴더는 아직 워크스페이스가 아니다: {}. .aisurada/workspace.json 이 없다. 경로가 맞는지 보고, 새로 만들 자리면 init-workspace 를 먼저 부른다.",
        absolute_normalized(vault_root).display()
    )))
}
