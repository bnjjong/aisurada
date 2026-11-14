//! `list_tags`: 노트에 붙은 태그와 쓰인 횟수를 많이 쓰인 것부터 돌려준다. 기록 층 노트의 태그는 세지 않는다.

use std::collections::HashMap;

use serde::Deserialize;

use crate::error::EngineError;
use crate::model::{Page, TagRow};
use crate::text::trim;
use crate::vault::index::NoteIndex;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ListTagsRequest {
    /// 태그에 이 글이 들어 있는 것만(영문 대소문자는 가리지 않는다)
    pub query: Option<String>,
    /// 돌려줄 최대 개수(1 이상). 없으면 전부
    pub limit: Option<i64>,
}

pub fn execute(index: &NoteIndex, request: &ListTagsRequest) -> Result<Page<TagRow>, EngineError> {
    let limit = match request.limit {
        Some(n) if n < 1 => {
            return Err(EngineError::invalid_argument(format!(
                "태그 개수 상한은 1 이상이다: {n}"
            )));
        }
        Some(n) => Some(n as usize),
        None => None,
    };
    let query = request
        .query
        .as_deref()
        .map(trim)
        .filter(|q| !q.is_empty())
        .map(str::to_ascii_lowercase);

    let mut counts: HashMap<&str, usize> = HashMap::new();
    for note in index.root_notes().filter(|n| n.meta.tier != "record") {
        for tag in &note.meta.tags {
            *counts.entry(tag).or_default() += 1;
        }
    }
    let mut tags: Vec<(&str, usize)> = counts
        .into_iter()
        .filter(|(tag, _)| {
            query
                .as_deref()
                .is_none_or(|q| tag.to_ascii_lowercase().contains(q))
        })
        .collect();
    // 많이 쓰인 것부터, 같으면 글의 UTF-8 바이트 차례
    tags.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then_with(|| a.0.as_bytes().cmp(b.0.as_bytes()))
    });

    let total = tags.len();
    Ok(Page::new(
        tags.into_iter()
            .take(limit.unwrap_or(usize::MAX))
            .map(|(tag, count)| TagRow {
                tag: tag.to_string(),
                count,
            })
            .collect(),
        total,
    ))
}
