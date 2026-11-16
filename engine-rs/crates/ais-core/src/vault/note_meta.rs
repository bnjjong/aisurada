//! 노트 머리 맵에서 메타데이터를 꺼내 다듬습니다. 키마다 모양을 검사하고 틀린 값은 기본값으로 물러섭니다.

use crate::kernel::doc_type::DocType;
use crate::kernel::link_kind::LinkKind;
use crate::kernel::tier::Tier;
pub use crate::note::meta::{Evaluation, Link, MetaData, parse_iso_date};
pub(crate) use crate::note::meta::{TEMPLATE_ID, get, read_agent, text};
pub(crate) use crate::note::meta::{parse_time_and_offset, utc_instant};
use std::collections::HashSet;

use super::agenda::AgendaTable;
use crate::frontmatter::Value;
use crate::text::{NOTE_ID, SUB_NOTE_ID, cut_utf16, is_blank, trim};

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
        Some(t) if DocType::parse(t).is_some() => t.to_string(),
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
    let mut tier = Tier::Reading;
    if !is_sub_note && let Some(raw) = text(map, "tier") {
        tier = Tier::from_front(trim(raw));
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
            if !NOTE_ID.is_match(to) || LinkKind::parse(kind).is_none() {
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
