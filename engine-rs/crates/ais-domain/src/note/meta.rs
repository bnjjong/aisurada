//! 노트의 메타데이터와 머리 맵 도우미. 노트 도메인이 쓰는 값만 있고 파일도 vault도 모른다.
//!
//! 머리 맵(`frontmatter`)에서 값을 꺼내 다듬는 일(`parse`)은 `vault/note_meta.rs`가 하고, 그 결과를 담는 타입과 값 하나를 읽는 도우미는 여기 있다.

use std::sync::LazyLock;

use regex::Regex;

use crate::frontmatter::Value;
use crate::kernel::tier::Tier;
use crate::kernel::time::{civil_from_days, days_from_civil};
use crate::text::trim;

pub static TEMPLATE_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9._-]{1,128}$").unwrap());
static AGENT_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z][a-z0-9-]{0,31}$").unwrap());

/// 본문 형식
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Markdown,
    Json,
    Html,
}

impl Format {
    pub fn as_str(self) -> &'static str {
        match self {
            Format::Markdown => "markdown",
            Format::Json => "json",
            Format::Html => "html",
        }
    }

    pub fn from_file_name(name: &str) -> Self {
        if name.ends_with(".json") {
            Format::Json
        } else if name.ends_with(".html") {
            Format::Html
        } else {
            Format::Markdown
        }
    }
}

/// 노트 사이의 연결 하나
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub to: String,
    pub kind: String,
}

/// 노트 평가
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evaluation {
    pub score: u8,
    pub note: String,
    pub at: String,
}

/// 노트 메타데이터. `Option`이 `None`이면 머리에 그 줄이 없거나 쓸 수 없는 값이다.
#[derive(Debug, Clone, PartialEq)]
pub struct MetaData {
    pub id: String,
    pub agenda_slug: String,
    pub doc_type: String,
    pub title: String,
    pub created: String,
    pub date: String,
    pub time: String,
    pub session_id: Option<String>,
    /// 세션을 연 에이전트. 줄이 없으면(claude) `None`
    pub session_agent: Option<String>,
    /// `reading` 또는 `record`
    pub tier: Tier,
    pub edited_by: Option<String>,
    pub edited_at: Option<String>,
    pub edited_session_id: Option<String>,
    pub edited_session_agent: Option<String>,
    pub tags: Vec<String>,
    pub links: Vec<Link>,
    pub favorite: bool,
    pub favorite_category: Option<String>,
    pub template: Option<String>,
    pub evaluation: Option<Evaluation>,
}

pub fn get<'a>(map: &'a [(Value, Value)], key: &str) -> Option<&'a Value> {
    map.iter()
        .find(|(k, _)| matches!(k, Value::Str(s) if s == key))
        .map(|(_, v)| v)
}

pub fn text<'a>(map: &'a [(Value, Value)], key: &str) -> Option<&'a str> {
    match get(map, key) {
        Some(Value::Str(s)) => Some(s),
        _ => None,
    }
}

/// 파일에 적을 에이전트 이름: 소문자로 바꿔 이름 모양이면 그것, claude이거나 모양이 아니면 `None`(Java `SessionAgents.forFile`)
pub fn agent_for_file(agent: &str) -> Option<String> {
    let lower = trim(agent).to_lowercase();
    (lower != "claude" && AGENT_NAME.is_match(&lower)).then_some(lower)
}

/// 세션 에이전트 이름을 다듬습니다. claude이거나 이름 모양이 아니면 `None`(줄이 없으면 claude로 본다).
pub fn read_agent(raw: Option<&Value>) -> Option<String> {
    let Some(Value::Str(agent)) = raw else {
        return None;
    };
    let lower = trim(agent).to_lowercase();
    if lower == "claude" || !AGENT_NAME.is_match(&lower) {
        None
    } else {
        Some(lower)
    }
}

/// `YYYY-MM-DD`를 읽어 달력에 있는 날짜인지 봅니다.
pub fn parse_iso_date(s: &str) -> Option<(i64, u32, u32)> {
    let b = s.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let digits = |r: std::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
    if !(digits(0..4) && digits(5..7) && digits(8..10)) {
        return None;
    }
    let year: i64 = s[0..4].parse().ok()?;
    let month: u32 = s[5..7].parse().ok()?;
    let day: u32 = s[8..10].parse().ok()?;
    if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
        return None;
    }
    Some((year, month, day))
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// `HH:MM[:SS[.fraction]]` 뒤에 오프셋(`Z`, `±HH`, `±HHMM`, `±HH:MM`, `±HH:MM:SS`)이 올 수 있습니다.
pub fn parse_time_and_offset(rest: &str) -> Option<(u32, u32, u32, u32, Option<i64>)> {
    let b = rest.as_bytes();
    let num = |from: usize, len: usize| -> Option<u32> {
        let part = rest.get(from..from + len)?;
        part.bytes()
            .all(|c| c.is_ascii_digit())
            .then(|| part.parse().ok())
            .flatten()
    };
    let hour = num(0, 2)?;
    if b.get(2) != Some(&b':') {
        return None;
    }
    let minute = num(3, 2)?;
    let mut pos = 5;
    let (mut second, mut nanos) = (0, 0);
    if b.get(pos) == Some(&b':') {
        second = num(pos + 1, 2)?;
        pos += 3;
        if b.get(pos) == Some(&b'.') {
            let frac: String = rest[pos + 1..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();
            if frac.is_empty() || frac.len() > 9 {
                return None;
            }
            nanos = format!("{frac:0<9}").parse().ok()?;
            pos += 1 + frac.len();
        }
    }
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }

    let tail = &rest[pos..];
    if tail.is_empty() {
        return Some((hour, minute, second, nanos, None));
    }
    if tail == "Z" || tail == "z" {
        return Some((hour, minute, second, nanos, Some(0)));
    }
    let sign: i64 = match tail.as_bytes()[0] {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let digits: String = tail[1..].chars().filter(|c| *c != ':').collect();
    if !digits.bytes().all(|c| c.is_ascii_digit()) || !matches!(digits.len(), 2 | 4 | 6) {
        return None;
    }
    let part = |i: usize| {
        digits
            .get(i..i + 2)
            .map(|d| d.parse::<i64>().unwrap_or(0))
            .unwrap_or(0)
    };
    let (oh, om, os) = (part(0), part(2), part(4));
    if oh > 18 || om > 59 || os > 59 || oh * 3600 + om * 60 + os > 18 * 3600 {
        return None;
    }
    Some((
        hour,
        minute,
        second,
        nanos,
        Some(sign * (oh * 3600 + om * 60 + os)),
    ))
}

/// 날짜·시각·오프셋을 UTC 순간 글(`YYYY-MM-DDTHH:MM:SS.mmmZ`)로 바꿉니다.
#[allow(clippy::too_many_arguments)]
pub fn utc_instant(
    year: i64,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    nanos: u32,
    offset_seconds: i64,
) -> String {
    let local = days_from_civil(year, i64::from(month), i64::from(day)) * 86_400
        + i64::from(hour) * 3600
        + i64::from(minute) * 60
        + i64::from(second);
    let epoch = local - offset_seconds;
    let days = epoch.div_euclid(86_400);
    let secs = epoch.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        secs / 3600,
        (secs % 3600) / 60,
        secs % 60,
        nanos / 1_000_000
    )
}
