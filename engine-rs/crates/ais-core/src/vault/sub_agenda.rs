//! 아젠다별 서브 아젠다 목록과 노트가 참조하는 ID를 읽는다.

use std::collections::HashSet;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value as Json;

use crate::error::EngineError;
use crate::kernel::confine::check_confinement;
use crate::ports::fs;
use crate::text::trim;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SubAgenda {
    pub id: String,
    pub title: String,
    pub archived: bool,
}

pub(crate) const DIR: &str = ".aisurada/sub-agendas";

pub(crate) fn path(vault: &Path, agenda_id: &str) -> PathBuf {
    vault.join(DIR).join(format!("{agenda_id}.json"))
}

pub(crate) fn read(vault: &Path, agenda_id: &str) -> Result<Vec<SubAgenda>, EngineError> {
    let file = path(vault, agenda_id);
    check_confinement(vault, &file)?;
    for candidate in [vault.join(".aisurada"), vault.join(DIR), file.clone()] {
        if fs::symlink_metadata(&candidate).is_ok_and(|metadata| metadata.is_symlink()) {
            return Err(EngineError::invalid_argument(format!(
                "서브 아젠다 목록 경로에 심볼릭 링크가 있다: {}",
                candidate.display()
            )));
        }
    }
    let bytes = match fs::read(&file) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            return Err(EngineError::internal(format!(
                "서브 아젠다 목록을 읽지 못했다({agenda_id}): {e}"
            )));
        }
    };
    let items: Vec<SubAgenda> = serde_json::from_slice(&bytes).map_err(|e| {
        EngineError::invalid_argument(format!(
            "서브 아젠다 목록 형식이 올바르지 않다({agenda_id}): {e}"
        ))
    })?;
    let mut ids = HashSet::new();
    let mut titles = HashSet::new();
    if items.iter().any(|item| {
        trim(&item.id).is_empty()
            || !ids.insert(&item.id)
            || trim(&item.title).is_empty()
            || !titles.insert(trim(&item.title))
    }) {
        return Err(EngineError::invalid_argument(format!(
            "서브 아젠다 목록에 빈 값이나 중복이 있다: {agenda_id}"
        )));
    }
    Ok(items)
}

pub(crate) fn parse_ids(raw: &Json) -> Result<Vec<String>, EngineError> {
    let ids: Vec<String> = match raw {
        Json::String(text) => serde_json::from_str(text),
        value => serde_json::from_value(value.clone()),
    }
    .map_err(|_| {
        EngineError::invalid_argument("subAgendaIds는 서브 아젠다 ID 문자열 배열이어야 한다")
    })?;
    let mut seen = HashSet::new();
    Ok(ids
        .into_iter()
        .map(|id| trim(&id).to_string())
        .filter(|id| seen.insert(id.clone()))
        .collect())
}

/// 다른 아젠다의 분류는 거절한다. 이미 붙은 보관 분류는 그대로 둘 수 있다.
pub(crate) fn validate(
    ids: &[String],
    items: &[SubAgenda],
    previous: &[String],
    restoring_move: bool,
) -> Result<(), EngineError> {
    for id in ids {
        let Some(item) = items.iter().find(|item| &item.id == id) else {
            return Err(EngineError::invalid_argument(format!(
                "이 아젠다의 서브 아젠다가 아니다: {id}"
            )));
        };
        if item.archived && !restoring_move && !previous.contains(id) {
            return Err(EngineError::invalid_argument(format!(
                "보관한 서브 아젠다는 새로 붙일 수 없다: {id}"
            )));
        }
    }
    Ok(())
}
