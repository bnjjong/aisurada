//! 노트를 놓을 자리의 실행: vault의 월별 배치를 알아내고(`parent_folder`), 경로를 검사하고, 폴더를 옮긴다.
//! 자리 정책(날짜·시각 인자 읽기, 폴더 이름, 부모 폴더 규칙)은 순수한 `note::placement`에 있고 여기서 다시 내보낸다.
//! 규칙은 Java 엔진의 `NoteMover`·`ShardLayout`과 같다.

pub use crate::kernel::time::CivilDate;
use crate::note::placement::{self, Layout};
pub use crate::note::placement::{
    CreatedParts, folder_name, folder_name_from, id_tail, parse_date_arg, parse_time_arg,
    read_created,
};
use crate::ports::fs;
use std::path::Path;

use super::confine::{absolute_normalized, has_unusable_path_characters};
use crate::ports::fs::check_confinement;
use crate::error::EngineError;
use crate::vault::scanner::is_sharded;

/// 경로로 못 짓는 글과 vault 밖을 가리키는 아젠다 글자를 거절한다.
pub fn check_path(vault: &Path, rel: &str) -> Result<(), EngineError> {
    if has_unusable_path_characters(rel) {
        return Err(EngineError::invalid_argument(format!(
            "경로로 쓸 수 없는 값이다: {rel}"
        )));
    }
    check_confinement(vault, &vault.join(rel))
}

/// 폴더 노트를 정식 자리로 옮기고 최종 상대 경로를 돌려준다. 같은 자리면 그대로 두고, 이미 무엇이 있으면 `-2`부터 `-99`까지
/// 붙여 빈자리를 찾는다. 지금 폴더가 이미 없으면 옮기지 않고 정식 자리만 돌려준다(그 자리에 새로 쓰게 된다).
pub fn place_folder(
    vault: &Path,
    current_rel: &str,
    canonical_rel: &str,
) -> Result<String, EngineError> {
    if canonical_rel == current_rel {
        return Ok(current_rel.to_string());
    }
    let current = vault.join(current_rel);
    if !current.exists() {
        return Ok(canonical_rel.to_string());
    }
    let chosen = (1..=99)
        .map(|n| {
            if n == 1 {
                canonical_rel.to_string()
            } else {
                format!("{canonical_rel}-{n}")
            }
        })
        .find(|candidate| !vault.join(candidate).exists())
        .ok_or_else(|| {
            EngineError::internal(format!(
                "옮길 자리를 못 찾았다(같은 이름이 99 개까지 차 있다): {canonical_rel}"
            ))
        })?;
    move_path(&current, &vault.join(&chosen))?;
    Ok(chosen)
}

/// 파일이나 폴더를 옮긴다. 도착 자리에 무엇이 있으면 옮기지 않고, 도착 폴더의 부모가 없으면 만든다.
pub fn move_path(source: &Path, target: &Path) -> Result<(), EngineError> {
    let source = absolute_normalized(source);
    let target = absolute_normalized(target);
    if target.exists() {
        return Err(EngineError::internal(format!(
            "옮길 자리에 이미 무엇이 있어 옮기지 않았다: {}",
            target.display()
        )));
    }
    if let Some(parent) = target.parent()
        && !parent.is_dir()
        && fs::create_dir_all(parent).is_err()
    {
        return Err(EngineError::internal(format!(
            "폴더를 만들지 못했다: {}",
            parent.display()
        )));
    }
    fs::rename(&source, &target).map_err(|_| {
        EngineError::internal(format!(
            "옮기지 못했다: {} -> {}",
            source.display(),
            target.display()
        ))
    })
}

/// 아젠다 아래 부모 폴더. 이 vault가 월별 배치인지는 여기서 알아내(`is_sharded`: 환경 변수 또는 디스크) 정책에 입력으로 넘긴다.
pub fn parent_folder(vault: &Path, agenda: &str, date: &CivilDate) -> String {
    placement::parent_folder(Layout::of(is_sharded(vault)), agenda, date)
}
