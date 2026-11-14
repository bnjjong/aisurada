//! `get_template`: 템플릿 하나를 본문까지 읽는다.

use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;

use crate::error::EngineError;
use crate::model::TemplateRow;
use crate::text::is_blank;
use crate::vault::index::NoteIndex;
use crate::vault::template::scan_templates;

static ID_SHAPE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9._-]{1,128}$").unwrap());

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GetTemplateRequest {
    pub id: String,
}

pub fn execute(
    index: &NoteIndex,
    request: &GetTemplateRequest,
) -> Result<TemplateRow, EngineError> {
    let id = request.id.as_str();
    if is_blank(id) {
        return Err(EngineError::invalid_argument(
            "인자가 하나 필요하다: get-template <템플릿 id>",
        ));
    }
    if !ID_SHAPE.is_match(id) {
        return Err(EngineError::invalid_argument(format!(
            "템플릿 id 모양이 아니다: {id}"
        )));
    }
    scan_templates(&index.vault_root)
        .templates
        .iter()
        .find(|t| t.id == id)
        .map(|t| TemplateRow::from_record(t, true))
        .ok_or_else(|| EngineError::not_found(format!("Template not found: {id}")))
}
