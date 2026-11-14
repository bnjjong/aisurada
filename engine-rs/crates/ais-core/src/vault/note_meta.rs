//! 노트 머리 맵에서 메타데이터를 꺼내 다듬습니다. 키마다 모양을 검사하고 틀린 값은 기본값으로 물러섭니다.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use super::agenda::AgendaTable;
use crate::frontmatter::Value;
use crate::text::{NOTE_ID, SUB_NOTE_ID, cut_utf16, is_blank, trim};

static TEMPLATE_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9._-]{1,128}$").unwrap());
static AGENT_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z][a-z0-9-]{0,31}$").unwrap());

const ALLOWED_TYPES: [&str; 6] = ["plan", "build", "report", "research", "meeting", "free"];
const ALLOWED_KINDS: [&str; 3] = ["direct", "indirect", "attach"];

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
    pub tier: String,
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

/// 메타를 못 만든 까닭. 훑기가 건너뜀 이유로 옮겨 적는다.
#[derive(Debug, PartialEq, Eq)]
pub enum MetaError {
    /// id가 없거나 모양이 어긋난다
    BadId,
    /// `created`가 없거나 달력에 없는 날짜다
    BadCreated,
    /// 아젠다 slug를 식별자로 바꾸지 못한다
    UnknownAgenda(String),
    /// 평가 점수가 숫자 아님(NaN)이라 읽을 수 없다
    BadEvaluation,
}

fn get<'a>(map: &'a [(Value, Value)], key: &str) -> Option<&'a Value> {
    map.iter()
        .find(|(k, _)| matches!(k, Value::Str(s) if s == key))
        .map(|(_, v)| v)
}

fn text<'a>(map: &'a [(Value, Value)], key: &str) -> Option<&'a str> {
    match get(map, key) {
        Some(Value::Str(s)) => Some(s),
        _ => None,
    }
}

/// 세션 에이전트 이름을 다듬습니다. claude이거나 이름 모양이 아니면 `None`(줄이 없으면 claude로 본다).
fn read_agent(raw: Option<&Value>) -> Option<String> {
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

/// 머리 맵에서 노트 메타를 꺼냅니다.
///
/// `folder_agenda_slug`는 노트가 놓인 폴더의 아젠다 slug, `agendas`는 아젠다 표(후속 노트는 `None`),
/// `root_note_id`는 후속 노트가 속한 메인 노트 id입니다.
pub fn parse_meta(
    map: &[(Value, Value)],
    folder_agenda_slug: &str,
    agendas: Option<&AgendaTable>,
    is_sub_note: bool,
    root_note_id: Option<&str>,
) -> Result<MetaData, MetaError> {
    // 1. id
    let id = match get(map, "id") {
        Some(Value::Str(id))
            if (if is_sub_note { &SUB_NOTE_ID } else { &NOTE_ID }).is_match(id) =>
        {
            id.clone()
        }
        _ => return Err(MetaError::BadId),
    };

    // 2. 아젠다
    let agenda_slug = if is_sub_note {
        folder_agenda_slug.to_string()
    } else {
        let slug = match text(map, "agenda") {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => folder_agenda_slug.to_string(),
        };
        if let Some(table) = agendas
            && !table.has_slug(&slug)
        {
            return Err(MetaError::UnknownAgenda(slug));
        }
        slug
    };

    // 3. type
    let doc_type = match text(map, "type") {
        Some(t) if ALLOWED_TYPES.contains(&t) => t.to_string(),
        _ => "build".to_string(),
    };

    // 4. title
    let title = match text(map, "title") {
        Some(t) if !is_blank(t) => t.to_string(),
        _ => id.clone(),
    };

    // 5. created
    let created = match text(map, "created") {
        Some(s) => parse_created(s).ok_or(MetaError::BadCreated)?,
        None => return Err(MetaError::BadCreated),
    };

    // 6. sessionId
    let (mut session_id, mut session_agent) = (None, None);
    if !is_sub_note && let Some(raw) = text(map, "sessionId") {
        let trimmed = trim(raw);
        if !trimmed.is_empty() {
            session_id = Some(cut_utf16(trimmed, 128));
            session_agent = read_agent(get(map, "sessionAgent"));
        }
    }

    // 8. tier
    let mut tier = "reading".to_string();
    if !is_sub_note && let Some(raw) = text(map, "tier") {
        let t = trim(raw);
        if t != "reading" && !t.is_empty() {
            tier = "record".to_string();
        }
    }

    // 9. 도장
    let (mut edited_by, mut edited_at, mut edited_session_id, mut edited_session_agent) =
        (None, None, None, None);
    if let (Some(by), Some(at)) = (text(map, "editedBy"), text(map, "editedAt")) {
        let (by, at) = (trim(by), trim(at));
        if !by.is_empty() && !at.is_empty() {
            edited_by = Some(by.to_string());
            edited_at = Some(at.to_string());
            if let Some(raw) = text(map, "editedSessionId") {
                let sid = trim(raw);
                if !sid.is_empty() {
                    edited_session_id = Some(sid.to_string());
                    edited_session_agent = read_agent(get(map, "editedSessionAgent"));
                }
            }
        }
    }

    // 10. tags: 다듬고, 겹침 없이 50개까지 모은 뒤 줄바꿈이 든 것을 뺀다
    let mut tags: Vec<String> = Vec::new();
    if let Some(Value::List(items)) = get(map, "tags") {
        let mut seen = HashSet::new();
        for item in items {
            if let Value::Str(tag) = item {
                let tag = trim(tag);
                if !tag.is_empty() && seen.insert(tag.to_string()) {
                    tags.push(tag.to_string());
                    if tags.len() == 50 {
                        break;
                    }
                }
            }
        }
        tags.retain(|t| !t.contains('\n') && !t.contains('\r'));
    }

    // 11. links
    let mut links: Vec<Link> = Vec::new();
    if let Some(Value::List(items)) = get(map, "links") {
        let mut seen = HashSet::new();
        for item in items {
            let Value::Map(entries) = item else { continue };
            let (Some(to), Some(kind)) = (text(entries, "to"), text(entries, "kind")) else {
                continue;
            };
            if !NOTE_ID.is_match(to) || !ALLOWED_KINDS.contains(&kind) {
                continue;
            }
            let self_ref = to == id || (is_sub_note && root_note_id == Some(to));
            if !self_ref && seen.insert(format!("{to}:{kind}")) {
                links.push(Link {
                    to: to.to_string(),
                    kind: kind.to_string(),
                });
            }
        }
    }

    // 12. favorite
    let (mut favorite, mut favorite_category) = (false, None);
    if !is_sub_note {
        if let Some(Value::Bool(b)) = get(map, "favorite") {
            favorite = *b;
        }
        if let Some(raw) = text(map, "favoriteCategory") {
            let t = trim(raw);
            if !t.is_empty() {
                favorite_category = Some(cut_utf16(t, 100));
            }
        }
    }

    // 13. template
    let mut template = None;
    if !is_sub_note && let Some(raw) = text(map, "template") {
        let cut = cut_utf16(trim(raw), 128);
        let retrimmed = trim(&cut);
        if TEMPLATE_ID.is_match(retrimmed) {
            template = Some(retrimmed.to_string());
        }
    }

    // 14. evaluation
    let mut evaluation = None;
    if !is_sub_note && let Some(Value::Map(entries)) = get(map, "evaluation") {
        let score = match get(entries, "score") {
            Some(Value::Int(n)) => n.parse::<f64>().ok(),
            Some(Value::Float(f)) => Some(*f),
            _ => None,
        };
        if let Some(score) = score {
            if score.is_nan() {
                return Err(MetaError::BadEvaluation);
            }
            // 반올림(0.5는 올림)한 뒤 0~100으로 맞춘다
            let rounded = (score + 0.5).floor().clamp(0.0, 100.0) as u8;
            let note = match text(entries, "note") {
                Some(n) => cut_utf16(trim(n), 1000),
                None => String::new(),
            };
            let at = match text(entries, "at") {
                Some(at) if !is_blank(at) => parse_created(at)
                    .map(|c| c.utc_instant)
                    .unwrap_or_else(|| created.utc_instant.clone()),
                _ => created.utc_instant.clone(),
            };
            evaluation = Some(Evaluation {
                score: rounded,
                note,
                at,
            });
        }
    }

    Ok(MetaData {
        id,
        agenda_slug,
        doc_type,
        title,
        created: created.formatted,
        date: created.date,
        time: created.time,
        session_id,
        session_agent,
        tier,
        edited_by,
        edited_at,
        edited_session_id,
        edited_session_agent,
        tags,
        links,
        favorite,
        favorite_category,
        template,
        evaluation,
    })
}

/// `created`를 읽은 결과
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Created {
    /// `YYYY-MM-DDTHH:MM:SS` + 오프셋(`Z` 또는 `±HH:MM`)으로 맞춘 글
    pub formatted: String,
    pub date: String,
    /// `HH:MM`
    pub time: String,
    /// UTC 순간 `YYYY-MM-DDTHH:MM:SS.mmmZ`
    pub utc_instant: String,
}

/// `created` 글을 읽어 맞춥니다. 날짜만 있으면 한국 표준시 0시로, 오프셋이 없으면 한국 표준시(+09:00)로 봅니다.
/// 연도 0000은 0001로 바꿉니다(Java `java.time`과 같은 결과를 내기 위한 규칙).
pub fn parse_created(s: &str) -> Option<Created> {
    const KST: i64 = 9 * 3600;
    if s.is_empty() || s.starts_with(' ') || s.ends_with(' ') || s.chars().count() < 10 {
        return None;
    }
    let date_part = s.get(0..10)?;
    let (year, month, day) = parse_iso_date(date_part)?;
    let out_year = if year == 0 { 1 } else { year };
    let out_date = format!("{out_year:04}{}", &date_part[4..]);

    if s.len() == 10 {
        let instant = utc_instant(out_year, month, day, 0, 0, 0, 0, KST);
        return Some(Created {
            formatted: format!("{out_date}T00:00:00+09:00"),
            date: out_date,
            time: "00:00".to_string(),
            utc_instant: instant,
        });
    }

    let bytes = s.as_bytes();
    if bytes[10] != b'T' && bytes[10] != b't' {
        return None;
    }
    let (hour, minute, second, nanos, offset) = parse_time_and_offset(&s[11..])?;
    let offset = offset.unwrap_or(KST);

    let offset_text = if offset == 0 {
        "Z".to_string()
    } else {
        let abs = offset.abs();
        format!(
            "{}{:02}:{:02}",
            if offset >= 0 { '+' } else { '-' },
            abs / 3600,
            (abs % 3600) / 60
        )
    };
    Some(Created {
        formatted: format!("{out_date}T{hour:02}:{minute:02}:{second:02}{offset_text}"),
        date: out_date,
        time: format!("{hour:02}:{minute:02}"),
        utc_instant: utc_instant(out_year, month, day, hour, minute, second, nanos, offset),
    })
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
fn parse_time_and_offset(rest: &str) -> Option<(u32, u32, u32, u32, Option<i64>)> {
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
fn utc_instant(
    year: i64,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    nanos: u32,
    offset_seconds: i64,
) -> String {
    let local = days_from_civil(year, month, day) * 86_400
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

/// 1970-01-01부터 센 날수(양력 proleptic)
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (i64::from(month) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn created_forms() {
        let c = parse_created("2026-11-05T16:08:00+09:00").unwrap();
        assert_eq!(
            (c.formatted.as_str(), c.date.as_str(), c.time.as_str()),
            ("2026-11-05T16:08:00+09:00", "2026-11-05", "16:08")
        );
        assert_eq!(c.utc_instant, "2026-11-05T07:08:00.000Z");
        // 날짜만: 한국 표준시 0시
        let d = parse_created("2026-11-05").unwrap();
        assert_eq!(d.formatted, "2026-11-05T00:00:00+09:00");
        assert_eq!(d.utc_instant, "2026-11-04T15:00:00.000Z");
        // 오프셋 없음: +09:00으로 보고, 초와 소수는 다듬는다
        assert_eq!(
            parse_created("2026-11-05T16:08").unwrap().formatted,
            "2026-11-05T16:08:00+09:00"
        );
        assert_eq!(
            parse_created("2026-11-05t16:08:09.123456z")
                .unwrap()
                .formatted,
            "2026-11-05T16:08:09Z"
        );
        assert_eq!(
            parse_created("2026-11-05T16:08:09.123456Z")
                .unwrap()
                .utc_instant,
            "2026-11-05T16:08:09.123Z"
        );
        assert_eq!(
            parse_created("2026-11-05T16:08:00-05:30")
                .unwrap()
                .formatted,
            "2026-11-05T16:08:00-05:30"
        );
        // 연도 0000은 0001로
        assert_eq!(parse_created("0000-03-01").unwrap().date, "0001-03-01");
    }

    #[test]
    fn rejects_bad_created() {
        for bad in [
            "",
            " 2026-11-05",
            "2026-11-05 ",
            "2026-13-05",
            "2026-02-30",
            "2025-02-29",
            "2026-11-05 16:08",
            "2026-11-05T",
            "2026-11-05T24:00",
            "2026-11-05T16:60",
            "20261105",
            "2026-11-05T16:08:00+19:00",
        ] {
            assert!(parse_created(bad).is_none(), "{bad:?}");
        }
        assert!(parse_created("2024-02-29").is_some());
    }

    #[test]
    fn instant_math_handles_leap_and_negative_offsets() {
        assert_eq!(
            parse_created("2024-02-29T23:30:00-02:00")
                .unwrap()
                .utc_instant,
            "2024-03-01T01:30:00.000Z"
        );
        assert_eq!(
            parse_created("1969-12-31T23:59:59Z").unwrap().utc_instant,
            "1969-12-31T23:59:59.000Z"
        );
    }
}
