//! `list_docs`·`search_docs`가 함께 쓰는 노트 조건. 인자를 차례대로 검사하고 노트가 조건에 맞는지 가립니다.

use std::collections::HashSet;

use serde::Deserialize;

use super::agenda::{AgendaTable, is_valid_slug_segment};
use super::note_meta::parse_iso_date;
use super::scanner::NoteRecord;
use crate::error::EngineError;
use crate::text::{compare_utf16, trim};

const VALID_TYPES: [&str; 6] = ["plan", "build", "report", "research", "meeting", "free"];

/// 호출자가 준 조건. 값이 없거나 빈 글이면 그 조건은 걸지 않는다.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DocFilter {
    /// 아젠다 slug나 이름
    pub agenda: Option<String>,
    #[serde(rename = "type")]
    pub doc_type: Option<String>,
    /// 모두 가진 노트만
    pub tags: Vec<String>,
    /// `true`면 즐겨찾기만. `false`는 조건을 안 건 것과 같다.
    pub favorite: Option<bool>,
    pub favorite_category: Option<String>,
    /// `YYYY-MM-DD`: 그날 노트만
    pub date: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
}

/// 검사를 마친 조건
#[derive(Debug, Default)]
pub struct Criteria {
    agenda_slug: Option<String>,
    doc_type: Option<&'static str>,
    tags: Vec<String>,
    favorite_only: bool,
    favorite_category: Option<String>,
    lower_date: Option<String>,
    upper_date: Option<String>,
}

fn non_empty(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(trim).filter(|v| !v.is_empty())
}

/// 조건을 정해진 차례(아젠다, 유형, 태그, 즐겨찾기 라벨, 날짜)로 검사해 만듭니다.
pub fn build(agendas: &AgendaTable, filter: &DocFilter) -> Result<Criteria, EngineError> {
    let mut criteria = Criteria::default();

    if let Some(input) = non_empty(&filter.agenda) {
        criteria.agenda_slug = Some(resolve_agenda(agendas, input)?);
    }

    if let Some(input) = non_empty(&filter.doc_type) {
        let matched = VALID_TYPES.iter().find(|t| t.eq_ignore_ascii_case(input));
        criteria.doc_type = Some(matched.copied().ok_or_else(|| {
            EngineError::invalid_argument(format!("모르는 유형 이다: {input}. 쓸 수 있는 것은 [plan, build, report, research, meeting, free] 뿐이다."))
        })?);
    }

    let mut seen = HashSet::new();
    for raw in &filter.tags {
        let tag = trim(raw);
        if tag.is_empty() {
            continue;
        }
        if tag.contains('\r') || tag.contains('\n') {
            return Err(EngineError::invalid_argument(format!(
                "태그에 줄바꿈이 있다: '{tag}'"
            )));
        }
        if seen.insert(tag.to_string()) {
            criteria.tags.push(tag.to_string());
        }
    }

    criteria.favorite_only = filter.favorite == Some(true);
    criteria.favorite_category = non_empty(&filter.favorite_category).map(str::to_string);

    let date = parse_date(&filter.date, "date")?;
    let from = parse_date(&filter.from, "from")?;
    let to = parse_date(&filter.to, "to")?;
    let (mut lower, mut upper) = (date.clone(), date);
    if let Some(from) = from
        && lower.as_ref().is_none_or(|l| from > *l)
    {
        lower = Some(from);
    }
    if let Some(to) = to
        && upper.as_ref().is_none_or(|u| to < *u)
    {
        upper = Some(to);
    }
    if let (Some(l), Some(u)) = (&lower, &upper)
        && l > u
    {
        return Err(EngineError::invalid_argument(format!(
            "날짜 구간이 뒤집혔다: {l} ~ {u}"
        )));
    }
    criteria.lower_date = lower;
    criteria.upper_date = upper;
    Ok(criteria)
}

fn parse_date(raw: &Option<String>, field: &str) -> Result<Option<String>, EngineError> {
    let Some(text) = non_empty(raw) else {
        return Ok(None);
    };
    parse_iso_date(text)
        .map(|(y, m, d)| Some(format!("{y:04}-{m:02}-{d:02}")))
        .ok_or_else(|| {
            EngineError::invalid_argument(format!("{field} 은 YYYY-MM-DD 모양이어야 한다: {text}"))
        })
}

/// 아젠다 slug와 글자까지 같으면 그것, 아니면 아젠다 제목(대소문자 무시)이 같은 것 가운데 slug가 가장 앞선 것
fn resolve_agenda(agendas: &AgendaTable, input: &str) -> Result<String, EngineError> {
    if is_valid_slug_segment(input) && agendas.has_slug(input) {
        return Ok(input.to_string());
    }
    let wanted = input.to_lowercase();
    let mut matching: Vec<&str> = agendas
        .slug_title_pairs()
        .filter(|(_, title)| title.to_lowercase() == wanted)
        .map(|(slug, _)| slug)
        .collect();
    if matching.is_empty() {
        return Err(EngineError::not_found(format!("아젠다 not found: {input}")));
    }
    matching.sort_by(|a, b| compare_utf16(a, b));
    Ok(matching[0].to_string())
}

/// 노트가 조건을 모두 만족하는지 봅니다.
pub fn matches(note: &NoteRecord, criteria: &Criteria) -> bool {
    let meta = &note.meta;
    if criteria
        .agenda_slug
        .as_deref()
        .is_some_and(|slug| slug != meta.agenda_slug)
    {
        return false;
    }
    if criteria
        .doc_type
        .is_some_and(|t| !t.eq_ignore_ascii_case(&meta.doc_type))
    {
        return false;
    }
    if !criteria.tags.iter().all(|t| meta.tags.contains(t)) {
        return false;
    }
    if criteria.favorite_only && !meta.favorite {
        return false;
    }
    if criteria
        .favorite_category
        .as_deref()
        .is_some_and(|c| Some(c) != meta.favorite_category.as_deref())
    {
        return false;
    }
    if criteria
        .lower_date
        .as_deref()
        .is_some_and(|l| meta.date.as_str() < l)
    {
        return false;
    }
    if criteria
        .upper_date
        .as_deref()
        .is_some_and(|u| meta.date.as_str() > u)
    {
        return false;
    }
    true
}
