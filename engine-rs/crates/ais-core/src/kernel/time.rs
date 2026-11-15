//! 날짜 계산(그레고리력)과 ISO 시각 읽기·쓰기. 연도는 Java `LocalDate`·`Instant`와 같게 다룬다.
//!
//! `days_from_civil`은 한때 두 벌이었다(`commands/ask_chat_commands.rs`와 `vault/note_meta.rs`). 같은 알고리즘이라 하나로 합쳤고,
//! 합치기 전 두 식과 같은 값을 내는지 아래 시험이 넓은 범위에서 견준다.

use std::sync::LazyLock;

use regex::Regex;

use crate::text::is_blank;

// `Instant.parse`와 `OffsetDateTime.parse`가 받는 모양(대소문자 무시, 초와 소수는 있어도 없어도, 끝은 `Z`나 `+09:00`)
static INSTANT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(?::(\d{2})(?:\.(\d{1,9}))?)?(Z|[+-]\d{2}:\d{2}(?::\d{2})?)$").unwrap()
});

/// `Instant.parse`처럼 초가 반드시 있는 시각만 읽는다(Claude 기록의 시각).
pub fn parse_instant_strict(text: &str) -> Option<i128> {
    INSTANT.captures(text)?.get(6)?;
    parse_instant(text)
}

/// `Instant.parse`나 `OffsetDateTime.parse`로 읽히는 시각을 나노초로(읽히지 않으면 `None`)
pub fn parse_instant(text: &str) -> Option<i128> {
    if is_blank(text) {
        return None;
    }
    let c = INSTANT.captures(text)?;
    let num = |i: usize| c.get(i).and_then(|m| m.as_str().parse::<i64>().ok());
    let (year, month, day) = (num(1)?, num(2)?, num(3)?);
    let (hour, minute, second) = (num(4)?, num(5)?, num(6).unwrap_or(0));
    if !(1..=12).contains(&month) || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if day < 1 || day > days_in_month[month as usize - 1] {
        return None;
    }
    let nanos = c.get(7).map_or(0, |m| {
        let digits = m.as_str();
        digits.parse::<i64>().unwrap_or(0) * 10_i64.pow(9 - digits.len() as u32)
    });
    let offset = c.get(8)?.as_str();
    let offset_seconds: i64 = if offset.eq_ignore_ascii_case("z") {
        0
    } else {
        let sign = if offset.starts_with('-') { -1 } else { 1 };
        let part = |from: usize| {
            offset
                .get(from..from + 2)
                .and_then(|p| p.parse::<i64>().ok())
        };
        let (h, m, s) = (part(1)?, part(4)?, part(7).unwrap_or(0));
        if m > 59 || s > 59 || h * 3600 + m * 60 + s > 18 * 3600 {
            return None;
        }
        sign * (h * 3600 + m * 60 + s)
    };
    let seconds = days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second
        - offset_seconds;
    Some(i128::from(seconds) * 1_000_000_000 + i128::from(nanos))
}

/// 1970-01-01부터 센 날수(그레고리력)
pub fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
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

/// 유닉스 밀리초를 UTC `yyyy-MM-ddTHH:mm:ss.SSSZ`로 보여 준다(워크스페이스 표식의 `createdAt`).
pub fn format_utc_millis(millis: i64) -> String {
    let days = millis.div_euclid(86_400_000);
    let rest = millis.rem_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3_600_000,
        rest % 3_600_000 / 60_000,
        rest % 60_000 / 1000,
        rest % 1000
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 합치기 전 `vault/note_meta.rs`의 식(월·일이 `u32`)
    fn days_from_civil_before_merge(year: i64, month: u32, day: u32) -> i64 {
        let y = if month <= 2 { year - 1 } else { year };
        let era = y.div_euclid(400);
        let yoe = y.rem_euclid(400);
        let mp = (i64::from(month) + 9) % 12;
        let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    #[test]
    fn merged_days_from_civil_matches_the_removed_copy_and_roundtrips() {
        for year in -1200..=4000_i64 {
            for month in 1..=12_u32 {
                for day in [1_u32, 15, 28] {
                    let merged = days_from_civil(year, i64::from(month), i64::from(day));
                    assert_eq!(merged, days_from_civil_before_merge(year, month, day));
                    assert_eq!(civil_from_days(merged), (year, month, day));
                }
            }
        }
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
    }

    #[test]
    fn formats_utc_millis_and_reads_instants_strictly() {
        assert_eq!(format_utc_millis(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(format_utc_millis(1_794_700_000_123), "2026-11-14T23:46:40.123Z");
        // 초가 없는 시각은 느슨하게는 읽히고 엄격하게는 안 읽힌다
        assert!(parse_instant("2026-11-14T23:46Z").is_some());
        assert!(parse_instant_strict("2026-11-14T23:46Z").is_none());
        assert_eq!(
            parse_instant_strict("2026-11-14T23:46:40Z"),
            Some(1_794_700_000_i128 * 1_000_000_000)
        );
    }
}
