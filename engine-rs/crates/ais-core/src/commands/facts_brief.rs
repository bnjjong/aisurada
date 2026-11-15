//! `facts_brief`: 창 안 읽기 층 루트 노트를 아젠다별로 세고 사실층 프롬프트 절을 짓는다(22.5, 22.6).

use std::path::Path;

use serde::Deserialize;
use serde_json::{Map, Value as Json, json};

use crate::kernel::time::days_from_civil;
use super::option_text::text_of;
use crate::error::EngineError;
use crate::text::{compare_utf16, is_blank, is_whitespace, trim};
use crate::vault::facts;
use crate::kernel::time::civil_from_days;
use crate::write::apparatus::now_millis;
use crate::write::local_time::local_clock;
use crate::write::pipeline::{WriteContext, WriteScope};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FactsBriefRequest {
    pub from: Option<Json>,
    pub to: Option<Json>,
    #[serde(rename = "min-new-file", alias = "minNewFile")]
    pub min_new_file: Option<Json>,
    #[serde(flatten)]
    pub context: WriteContext,
}

/// `LocalDate.parse`: `YYYY-MM-DD`이고 달력에 있는 날짜. 날 수로 돌려준다.
pub(crate) fn parse_iso_date(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    let shape = bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit());
    if !shape {
        return None;
    }
    let (y, m, d): (i64, i64, i64) = (
        text[..4].parse().ok()?,
        text[5..7].parse().ok()?,
        text[8..].parse().ok()?,
    );
    let days = days_from_civil(y, m, d);
    // 달력에 없는 날(2월 30일 따위)은 되돌려 만들어 보면 달라진다
    (civil_from_days(days) == (y, m as u32, d as u32)).then_some(days)
}

pub(crate) fn iso_text(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

fn date(raw: &Option<Json>, field: &str) -> Result<Option<i64>, EngineError> {
    let value = text_of(raw)
        .map(|r| trim(&r).to_string())
        .unwrap_or_default();
    if value.is_empty() {
        return Ok(None);
    }
    parse_iso_date(&value).map(Some).ok_or_else(|| {
        EngineError::invalid_argument(format!("{field} 은 YYYY-MM-DD 모양이어야 한다: {value}"))
    })
}

fn minimum(raw: &Option<Json>) -> Result<i32, EngineError> {
    let value = text_of(raw)
        .map(|r| trim(&r).to_string())
        .unwrap_or_default();
    if value.is_empty() {
        return Ok(5);
    }
    let digits = value.strip_prefix(['+', '-']).unwrap_or(&value);
    let number = (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
        .then(|| value.parse::<i32>().ok())
        .flatten()
        .ok_or_else(|| {
            EngineError::invalid_argument(format!("min-new-file 은 숫자여야 한다: {value}"))
        })?;
    if number < 1 {
        return Err(EngineError::invalid_argument(format!(
            "min-new-file 은 1 이상이어야 한다: {number}"
        )));
    }
    Ok(number)
}

/// 사실층 글에서 첫 제목 줄(`# `)과 앞쪽 빈 줄, 끝의 공백을 뗀다.
fn body(facts: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    let mut rest = facts;
    loop {
        match rest.find(['\r', '\n']) {
            Some(i) => {
                lines.push(&rest[..i]);
                let skip = if rest[i..].starts_with("\r\n") { 2 } else { 1 };
                rest = &rest[i + skip..];
            }
            None => {
                lines.push(rest);
                break;
            }
        }
    }
    let mut first = usize::from(lines[0].starts_with("# "));
    while first < lines.len() && is_blank(lines[first]) {
        first += 1;
    }
    let joined = lines[first..].join("\n");
    joined.trim_end_matches(is_whitespace).to_string()
}

pub fn facts_brief(vault: &Path, request: &FactsBriefRequest, scope: &WriteScope) -> Result<Json, EngineError> {
    let from = date(&request.from, "from")?;
    let to = date(&request.to, "to")?;
    let min_new_file = minimum(&request.min_new_file)?;
    let to = to.unwrap_or_else(|| {
        let today = local_clock(now_millis()).date;
        days_from_civil(today.year, i64::from(today.month), i64::from(today.day))
    });
    let from = from.unwrap_or(to - 6);
    if from > to {
        return Err(EngineError::invalid_argument(format!(
            "날짜 구간이 뒤집혔다: {} ~ {}",
            iso_text(from),
            iso_text(to)
        )));
    }
    let (first, last) = (iso_text(from), iso_text(to));
    let table = &scope.index.agendas.table;
    // 날짜를 다시 파싱하지 않는다. 색인의 날짜 글자와 바이트로 맞댄다(22.9의 13번)
    let mut counts: Vec<(String, usize)> = Vec::new();
    for note in scope.index.root_notes() {
        let meta = &note.meta;
        if !meta.tier.is_reading()
            || meta.date.as_bytes() < first.as_bytes()
            || meta.date.as_bytes() > last.as_bytes()
            || !table.has_slug(&meta.agenda_slug)
        {
            continue;
        }
        match counts
            .iter_mut()
            .find(|(slug, _)| *slug == meta.agenda_slug)
        {
            Some(slot) => slot.1 += 1,
            None => counts.push((meta.agenda_slug.clone(), 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| compare_utf16(&a.0, &b.0)));
    let mut items: Vec<Json> = Vec::new();
    let mut blocks: Vec<String> = Vec::new();
    for (slug, notes) in &counts {
        let facts = facts::read_facts(vault, slug);
        if facts.is_none() && (*notes as i64) < i64::from(min_new_file) {
            continue;
        }
        let mut item = Map::new();
        item.insert("agenda".into(), json!(slug));
        item.insert("notes".into(), json!(notes));
        let heading = format!("### {slug} — 노트 {notes} 건");
        match &facts {
            None => blocks.push(format!("{heading}, 사실층 파일이 아직 없다")),
            Some(text) => {
                item.insert("facts".into(), json!(text));
                blocks.push(format!("{heading}\n\n{}", body(text)));
            }
        }
        items.push(Json::Object(item));
    }
    Ok(json!({
        "from": first,
        "to": last,
        "count": items.len(),
        "section": blocks.join("\n\n"),
        "items": items,
    }))
}
