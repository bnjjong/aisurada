//! 노트를 놓을 자리 계산: 날짜·시각 인자 읽기, 부모 폴더(월별 배치 여부), 폴더 이름, 폴더 옮기기.
//! 규칙은 Java 엔진의 `UpdateDoc`·`CreateDoc`·`NoteMover`·`ShardLayout`과 같다.

use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::confine::{absolute_normalized, check_confinement, has_unusable_path_characters};
use super::slug;
use crate::error::EngineError;
use crate::vault::scanner::is_sharded;

static DATE_SYNTAX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?:[0-9]{4}|\+[0-9]{5,10}|-[0-9]{4,10})-(0[1-9]|1[0-2])-(0[1-9]|[12][0-9]|3[01])$",
    )
    .unwrap()
});
static TIME_SYNTAX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([01][0-9]|2[0-3]):[0-5][0-9]$").unwrap());

/// 그레고리력 날짜. 연도는 Java `LocalDate`처럼 0과 음수, 다섯 자리 이상도 쓴다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CivilDate {
    pub year: i64,
    pub month: u32,
    pub day: u32,
}

impl CivilDate {
    /// `LocalDate.toString()`: 폴더 이름과 응답에 쓰는 ISO 글. 0년은 `0000`, 기원전 1년은 `-0001`, 9999년 뒤는 `+12345`.
    pub fn iso(&self) -> String {
        format!("{}-{:02}-{:02}", iso_year(self.year), self.month, self.day)
    }

    /// `yyyy-MM-dd`로 찍은 글: `created`에 쓴다. 연도는 기원 연도라 0년은 `0001`, 기원전 1년은 `0002`다.
    pub fn era_text(&self) -> String {
        format!("{}-{:02}-{:02}", era_year(self.year), self.month, self.day)
    }

    /// `yyyy-MM`로 찍은 글: 월별 폴더 이름
    pub fn era_month(&self) -> String {
        format!("{}-{:02}", era_year(self.year), self.month)
    }
}

fn iso_year(year: i64) -> String {
    let abs = year.abs();
    if abs < 1000 {
        if year < 0 {
            format!("-{abs:04}")
        } else {
            format!("{abs:04}")
        }
    } else if year > 9999 {
        format!("+{year}")
    } else {
        year.to_string()
    }
}

fn era_year(year: i64) -> String {
    let era = if year >= 1 { year } else { 1 - year };
    if era > 9999 {
        format!("+{era}")
    } else {
        format!("{era:04}")
    }
}

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
    format!(
        "{}-{hour:02}{minute:02}-{}-{}",
        date.iso(),
        slug::build(title, 50),
        id_tail(id)
    )
}

/// 아젠다 아래 부모 폴더: 월별 배치면 `notes/<아젠다>/<yyyy-MM>`, 아니면 `notes/<아젠다>`
pub fn parent_folder(vault: &Path, agenda: &str, date: &CivilDate) -> String {
    if is_sharded(vault) {
        format!("notes/{agenda}/{}", date.era_month())
    } else {
        format!("notes/{agenda}")
    }
}

/// 경로로 못 짓는 글과 vault 밖을 가리키는 아젠다 글자를 거절한다.
pub fn check_path(vault: &Path, rel: &str) -> Result<(), EngineError> {
    if has_unusable_path_characters(rel) {
        return Err(EngineError::invalid_argument(format!(
            "경로로 쓸 수 없는 값이다: {rel}"
        )));
    }
    check_confinement(vault, &vault.join(rel))
}

/// 폴더 노트를 정식 자리로 옮기고 최종 상대 경로를 돌려준다. 같은 자리면 그대로 두고, 이미 무엇이 있으면 `-2`부터 `-99`까지
/// 붙여 빈자리를 찾는다. 지금 폴더가 이미 없으면 옮기지 않고 정식 자리만 돌려준다(그 자리에 새로 쓰게 된다).
pub fn place_folder(
    vault: &Path,
    current_rel: &str,
    canonical_rel: &str,
) -> Result<String, EngineError> {
    if canonical_rel == current_rel {
        return Ok(current_rel.to_string());
    }
    let current = vault.join(current_rel);
    if !current.exists() {
        return Ok(canonical_rel.to_string());
    }
    let chosen = (1..=99)
        .map(|n| {
            if n == 1 {
                canonical_rel.to_string()
            } else {
                format!("{canonical_rel}-{n}")
            }
        })
        .find(|candidate| !vault.join(candidate).exists())
        .ok_or_else(|| {
            EngineError::internal(format!(
                "옮길 자리를 못 찾았다(같은 이름이 99 개까지 차 있다): {canonical_rel}"
            ))
        })?;
    move_path(&current, &vault.join(&chosen))?;
    Ok(chosen)
}

/// 파일이나 폴더를 옮긴다. 도착 자리에 무엇이 있으면 옮기지 않고, 도착 폴더의 부모가 없으면 만든다.
pub fn move_path(source: &Path, target: &Path) -> Result<(), EngineError> {
    let source = absolute_normalized(source);
    let target = absolute_normalized(target);
    if target.exists() {
        return Err(EngineError::internal(format!(
            "옮길 자리에 이미 무엇이 있어 옮기지 않았다: {}",
            target.display()
        )));
    }
    if let Some(parent) = target.parent()
        && !parent.is_dir()
        && fs::create_dir_all(parent).is_err()
    {
        return Err(EngineError::internal(format!(
            "폴더를 만들지 못했다: {}",
            parent.display()
        )));
    }
    fs::rename(&source, &target).map_err(|_| {
        EngineError::internal(format!(
            "옮기지 못했다: {} -> {}",
            source.display(),
            target.display()
        ))
    })
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
