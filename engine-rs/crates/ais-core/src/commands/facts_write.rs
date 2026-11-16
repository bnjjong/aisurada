//! `update_agenda_facts`: 아젠다의 사실층(지금도 참인 것)을 통째로 바꾸거나 지운다. 인자 이름과 응답 필드는 Java 엔진과 같다.

use crate::ports::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::agenda_write::{find, require_id};
use crate::error::EngineError;
use crate::text::{trim, utf16_len};
use crate::vault::facts::{EDIT_LIMIT, LIMIT, relative_path};
use crate::write::atomic::write_atomic;
use crate::write::confine::absolute_normalized;
use crate::write::pipeline::WriteScope;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UpdateAgendaFactsRequest {
    pub id: String,
    /// 새 사실층. 앞뒤 공백을 떼고, 비면 지운다. 없으면 오류다.
    pub facts: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAgendaFactsResponse {
    pub id: String,
    pub slug: String,
    pub path: String,
    pub limit: usize,
    pub edit_limit: usize,
    /// 지웠으면 빠진다
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facts: Option<String>,
}

pub fn update(
    vault: &Path,
    request: &UpdateAgendaFactsRequest,
    scope: &WriteScope,
) -> Result<UpdateAgendaFactsResponse, EngineError> {
    const USAGE: &str = "update-agenda-facts <아젠다 id> facts=<지금도 참인 것. 빈 값이면 지운다>";
    require_id(&request.id, USAGE)?;
    let Some(raw) = &request.facts else {
        return Err(EngineError::invalid_argument(
            "facts 가 필요하다. 사실층을 지우려면 빈 값을 준다.",
        ));
    };
    let facts = trim(raw);
    if utf16_len(facts) > LIMIT {
        return Err(EngineError::invalid_argument(format!(
            "사실층이 상한을 넘었다: {} 자. 상한은 {LIMIT} 자다.",
            utf16_len(facts)
        )));
    }
    let agenda = find(&scope, &request.id)?;
    let path = relative_path(&agenda.slug);
    let file = vault.join(&path);
    if facts.is_empty() {
        match fs::remove_file(&file) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            // Java `Files.deleteIfExists`처럼 그 자리의 빈 폴더도 지운다(안이 있는 폴더는 지우지 못한다)
            Err(_)
                if fs::symlink_metadata(&file).is_ok_and(|m| m.is_dir())
                    && fs::remove_dir(&file).is_ok() => {}
            Err(_) => {
                return Err(EngineError::internal(format!(
                    "파일을 지우지 못했다: {}",
                    absolute_normalized(&file).display()
                )));
            }
        }
    } else {
        write_atomic(&file, format!("{facts}\n").as_bytes())?;
    }
    Ok(UpdateAgendaFactsResponse {
        id: agenda.id.clone(),
        slug: agenda.slug.clone(),
        path,
        limit: LIMIT,
        edit_limit: EDIT_LIMIT,
        facts: (!facts.is_empty()).then(|| facts.to_string()),
    })
}
