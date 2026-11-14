//! `list_templates`: 템플릿을 문서 템플릿 먼저, 이어서 id 차례로 돌려줍니다.

use std::path::Path;

use serde::Deserialize;

use crate::error::EngineError;
use crate::model::{Page, TemplateRow};
use crate::text::{compare_utf16, trim};
use crate::vault::template::{Category, scan_templates};

const DOC_TYPES: [&str; 6] = ["plan", "build", "report", "research", "meeting", "free"];

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ListTemplatesRequest {
    /// `doc` 또는 `ritual`
    pub category: Option<String>,
    /// 문서 템플릿의 유형
    #[serde(rename = "type")]
    pub doc_type: Option<String>,
    /// 본문까지 싣는가
    pub bodies: bool,
}

fn non_empty(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(trim).filter(|v| !v.is_empty())
}

pub fn execute(
    vault_root: &Path,
    request: &ListTemplatesRequest,
) -> Result<Page<TemplateRow>, EngineError> {
    let category = match non_empty(&request.category).map(str::to_lowercase) {
        None => None,
        Some(c) if c == "doc" => Some(Category::Doc),
        Some(c) if c == "ritual" => Some(Category::Ritual),
        Some(_) => {
            let given = non_empty(&request.category).unwrap_or("");
            return Err(EngineError::invalid_argument(format!(
                "모르는 템플릿 갈래 이다: {given}. 쓸 수 있는 것은 [doc, ritual] 뿐이다."
            )));
        }
    };
    let doc_type = match non_empty(&request.doc_type).map(str::to_lowercase) {
        None => None,
        Some(t) if DOC_TYPES.contains(&t.as_str()) => Some(t),
        Some(_) => {
            let given = non_empty(&request.doc_type).unwrap_or("");
            return Err(EngineError::invalid_argument(format!(
                "모르는 템플릿 유형 이다: {given}. 쓸 수 있는 것은 [plan, build, report, research, meeting, free] 뿐이다."
            )));
        }
    };

    let mut templates: Vec<_> = scan_templates(vault_root)
        .templates
        .into_iter()
        .filter(|t| category.is_none_or(|c| c == t.category))
        .filter(|t| {
            doc_type.as_deref().is_none_or(|wanted| {
                t.category == Category::Doc
                    && t.doc_type
                        .as_deref()
                        .is_some_and(|d| d.eq_ignore_ascii_case(wanted))
            })
        })
        .collect();
    templates.sort_by(|a, b| {
        (a.category != Category::Doc)
            .cmp(&(b.category != Category::Doc))
            .then_with(|| compare_utf16(&a.id, &b.id))
    });

    let total = templates.len();
    Ok(Page::new(
        templates
            .iter()
            .map(|t| TemplateRow::from_record(t, request.bodies))
            .collect(),
        total,
    ))
}
