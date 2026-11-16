//! 노트를 놓을 자리 정책: 날짜·시각 인자 읽기, 폴더 이름, 부모 폴더(배치는 입력). 파일도 vault도 모른다.
//! 규칙은 Java 엔진의 `UpdateDoc`·`CreateDoc`·`ShardLayout`과 같다. 폴더를 실제로 옮기는 일은 `write/doc_place.rs`가 한다.

use std::sync::LazyLock;

use regex::Regex;

use super::meta::parse_iso_date;
use super::slug;
use crate::error::EngineError;
use crate::kernel::time::CivilDate;

static DATE_SYNTAX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?:[0-9]{4}|\+[0-9]{5,10}|-[0-9]{4,10})-(0[1-9]|1[0-2])-(0[1-9]|[12][0-9]|3[01])$",
    )
    .unwrap()
});
static TIME_SYNTAX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([01][0-9]|2[0-3]):[0-5][0-9]$").unwrap());

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => 0,
    }
}

/// `date` 인자를 읽는다. 모양이 틀렸거나 달력에 없거나 `java.time`이 받지 못하는 연도면 400이다.
pub fn parse_date_arg(text: &str) -> Result<CivilDate, EngineError> {
    let bad =
        || EngineError::invalid_argument(format!("date 은 YYYY-MM-DD 모양이어야 한다: {text}"));
    if !DATE_SYNTAX.is_match(text) || text.starts_with("-0000-") {
        return Err(bad());
    }
    // 부호가 있으면 부호 뒤의 첫 `-`가 연도의 끝이다
    let start = usize::from(text.starts_with('+') || text.starts_with('-'));
    let first = start + text[start..].find('-').ok_or_else(bad)?;
    let second = first + 1 + text[first + 1..].find('-').ok_or_else(bad)?;
    let year: i64 = text[..first].parse().map_err(|_| bad())?;
    let month: u32 = text[first + 1..second].parse().map_err(|_| bad())?;
    let day: u32 = text[second + 1..].parse().map_err(|_| bad())?;
    // Java는 연도를 int로 읽고 LocalDate가 ±999,999,999년까지만 받는다
    if i32::try_from(year).is_err() || year.abs() > 999_999_999 {
        return Err(bad());
    }
    if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
        return Err(bad());
    }
    Ok(CivilDate { year, month, day })
}

/// `time` 인자(`HH:MM`)를 읽어 (시, 분)을 돌려준다.
pub fn parse_time_arg(text: &str) -> Result<(u32, u32), EngineError> {
    if !TIME_SYNTAX.is_match(text) {
        return Err(EngineError::invalid_argument(format!(
            "time 은 HH:MM 모양이어야 한다: {text}"
        )));
    }
    let (hour, minute) = text.split_once(':').unwrap_or_default();
    Ok((hour.parse().unwrap_or(0), minute.parse().unwrap_or(0)))
}

/// 색인의 `created`(`yyyy-MM-ddTHH:mm:ss` + 오프셋 글)를 날짜, 시, 분, 오프셋 글로 가른다.
pub struct CreatedParts {
    pub date: CivilDate,
    pub hour: u32,
    pub minute: u32,
    pub offset: String,
}

pub fn read_created(created: &str) -> Option<CreatedParts> {
    let (year, month, day) = parse_iso_date(created.get(0..10)?)?;
    if created.as_bytes().get(10) != Some(&b'T') {
        return None;
    }
    Some(CreatedParts {
        date: CivilDate { year, month, day },
        hour: created.get(11..13)?.parse().ok()?,
        minute: created.get(14..16)?.parse().ok()?,
        offset: created.get(19..)?.to_string(),
    })
}

/// 노트 id에서 소문자·숫자만 남겨 뒤 네 글자를 쓴다. 네 글자가 안 되면 있는 대로, 하나도 없으면 `0`이다.
pub fn id_tail(id: &str) -> String {
    let kept: Vec<char> = id
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        .collect();
    match kept.len() {
        0 => "0".to_string(),
        n if n >= 4 => kept[n - 4..].iter().collect(),
        _ => kept.iter().collect(),
    }
}

/// 노트 폴더 이름 `<날짜>-<HHmm>-<제목 slug>-<id 꼬리>`
pub fn folder_name(date: &CivilDate, hour: u32, minute: u32, title: &str, id: &str) -> String {
    folder_name_from(date, hour, minute, title, id)
}

/// `folder_name`과 같다. 후속 노트가 만든 시각에서 이름을 지을 때 쓴다.
pub fn folder_name_from(date: &CivilDate, hour: u32, minute: u32, title: &str, id: &str) -> String {
    format!(
        "{}-{hour:02}{minute:02}-{}-{}",
        date.iso(),
        slug::build(title, 50),
        id_tail(id)
    )
}

/// 노트 폴더의 배치. 월별로 나누는지는 vault를 훑거나 환경 변수를 읽어야 알 수 있어서 **입력으로 받는다**(`write/doc_place.rs`가 정해 준다).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layout {
    /// `notes/<아젠다>` 바로 아래에 노트 폴더를 둔다
    Flat,
    /// `notes/<아젠다>/<yyyy-MM>` 아래에 둔다
    MonthlyShards,
}

impl Layout {
    pub fn of(sharded: bool) -> Layout {
        if sharded {
            Layout::MonthlyShards
        } else {
            Layout::Flat
        }
    }
}

/// 아젠다 아래 부모 폴더: 월별 배치면 `notes/<아젠다>/<yyyy-MM>`, 아니면 `notes/<아젠다>`
pub fn parent_folder(layout: Layout, agenda: &str, date: &CivilDate) -> String {
    match layout {
        Layout::MonthlyShards => format!("notes/{agenda}/{}", date.era_month()),
        Layout::Flat => format!("notes/{agenda}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dates_like_java_time() {
        let date = parse_date_arg("2026-02-28").unwrap();
        assert_eq!(date.iso(), "2026-02-28");
        assert_eq!(date.era_text(), "2026-02-28");
        assert!(parse_date_arg("2026-02-29").is_err());
        assert!(parse_date_arg("2024-02-29").is_ok());
        assert!(parse_date_arg("2026-13-01").is_err());
        assert!(parse_date_arg("26-01-01").is_err());
        assert!(parse_date_arg("2026-01-01\n").is_err());
        assert!(parse_date_arg("-0000-01-01").is_err());
        let message = parse_date_arg("x").unwrap_err().message;
        assert_eq!(message, "date 은 YYYY-MM-DD 모양이어야 한다: x");
    }

    #[test]
    fn handles_years_outside_four_digits() {
        let zero = parse_date_arg("-00000-03-04").unwrap();
        assert_eq!(
            (zero.iso(), zero.era_text()),
            ("0000-03-04".into(), "0001-03-04".into())
        );
        let before = parse_date_arg("-0001-03-04").unwrap();
        assert_eq!(
            (before.iso(), before.era_text()),
            ("-0001-03-04".into(), "0002-03-04".into())
        );
        assert_eq!(before.era_month(), "0002-03");
        let far = parse_date_arg("+12345-03-04").unwrap();
        assert_eq!(
            (far.iso(), far.era_text()),
            ("+12345-03-04".into(), "+12345-03-04".into())
        );
        let deep = parse_date_arg("-1234-03-04").unwrap();
        assert_eq!(deep.iso(), "-1234-03-04");
        assert!(parse_date_arg("+1000000000-01-01").is_err());
        assert!(parse_date_arg("+9999999999-01-01").is_err());
        assert!(parse_date_arg("+999999999-01-01").is_ok());
    }

    #[test]
    fn parses_times() {
        assert_eq!(parse_time_arg("09:05").unwrap(), (9, 5));
        assert_eq!(parse_time_arg("23:59").unwrap(), (23, 59));
        assert!(parse_time_arg("24:00").is_err());
        assert!(parse_time_arg("9:05").is_err());
        assert_eq!(
            parse_time_arg("9:05").unwrap_err().message,
            "time 은 HH:MM 모양이어야 한다: 9:05"
        );
    }

    #[test]
    fn takes_the_last_four_lowercase_or_digit_characters_of_an_id() {
        assert_eq!(id_tail("ndfx229um-e59223de"), "23de");
        assert_eq!(id_tail("nd-A-b"), "ndb");
        assert_eq!(id_tail("---"), "0");
    }
}
