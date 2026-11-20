//! 아젠다별 목록에서 서브 아젠다를 만들거나 이름과 보관 상태를 바꾼다.

use std::path::Path;

use serde::Deserialize;

use super::agenda_write;
use super::note_meta_write::{BoolArg, bool_arg};
use crate::error::EngineError;
use crate::text::trim;
use crate::vault::sub_agenda::{self, SubAgenda};
use crate::write::apparatus::next_id;
use crate::write::atomic::write_atomic;
use crate::write::pipeline::WriteScope;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRequest {
    agenda_id: String,
    title: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRequest {
    agenda_id: String,
    id: String,
    title: Option<String>,
    archived: Option<BoolArg>,
}

fn check_title(items: &[SubAgenda], title: &str, except: Option<&str>) -> Result<(), EngineError> {
    if title.is_empty() || title.contains(['\n', '\r']) {
        return Err(EngineError::invalid_argument(
            "서브 아젠다 이름은 비어 있지 않은 한 줄이어야 한다",
        ));
    }
    if items
        .iter()
        .any(|item| Some(item.id.as_str()) != except && trim(&item.title) == title)
    {
        return Err(EngineError::invalid_argument(format!(
            "같은 이름의 서브 아젠다가 있다: {title}"
        )));
    }
    Ok(())
}

fn save(vault: &Path, agenda_id: &str, items: &[SubAgenda]) -> Result<(), EngineError> {
    let mut bytes =
        serde_json::to_vec_pretty(items).map_err(|e| EngineError::internal(e.to_string()))?;
    bytes.push(b'\n');
    write_atomic(&sub_agenda::path(vault, agenda_id), &bytes)
}

pub fn create(
    vault: &Path,
    request: &CreateRequest,
    scope: &WriteScope,
) -> Result<SubAgenda, EngineError> {
    let agenda = agenda_write::find(scope, &request.agenda_id)?;
    let mut items = sub_agenda::read(vault, &agenda.id)?;
    let title = trim(&request.title);
    check_title(&items, title, None)?;
    let id = next_id("sa");
    if items.iter().any(|item| item.id == id) {
        return Err(EngineError::invalid_argument(
            "서브 아젠다 ID가 겹쳤다. 다시 시도한다",
        ));
    }
    let item = SubAgenda {
        id,
        title: title.to_string(),
        archived: false,
    };
    items.push(item.clone());
    save(vault, &agenda.id, &items)?;
    Ok(item)
}

pub fn update(
    vault: &Path,
    request: &UpdateRequest,
    scope: &WriteScope,
) -> Result<SubAgenda, EngineError> {
    let agenda = agenda_write::find(scope, &request.agenda_id)?;
    let mut items = sub_agenda::read(vault, &agenda.id)?;
    let archived = bool_arg("archived", &request.archived)?;
    let title = request.title.as_deref().map(trim);
    if let Some(title) = title {
        check_title(&items, title, Some(&request.id))?;
    }
    let item = items
        .iter_mut()
        .find(|item| item.id == request.id)
        .ok_or_else(|| {
            EngineError::not_found(format!("서브 아젠다를 찾지 못했다: {}", request.id))
        })?;
    if let Some(title) = title {
        item.title = title.to_string();
    }
    if let Some(archived) = archived {
        item.archived = archived;
    }
    let result = item.clone();
    save(vault, &agenda.id, &items)?;
    Ok(result)
}
