//! 이 프로세스의 시간대로 본 날짜와 시각. Java의 `ZoneId.systemDefault()`와 같이 `TZ` 환경 변수나 시스템 시간대를 따른다.
//! 새 노트의 기본 날짜(오늘)와 시각(지금) 배정에 쓴다.

use super::doc_place::CivilDate;

unsafe extern "C" {
    /// `TZ` 환경 변수를 다시 읽는다(`libc` 크레이트가 모든 플랫폼에 내보내지 않는다).
    fn tzset();
}

/// 어느 순간을 시간대로 풀어 본 날짜와 시, 분, 초
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalClock {
    pub date: CivilDate,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    /// UTC와의 차(초). 동쪽이 양수
    pub offset_seconds: i64,
}

/// 유닉스 밀리초를 이 프로세스의 시간대로 풀어 날짜와 시, 분, 초(초 아래는 버림)를 돌려준다.
/// 시간대를 못 읽으면 UTC로 본다.
pub fn local_clock(millis: i64) -> LocalClock {
    let seconds = millis.div_euclid(1000) as libc::time_t;
    // SAFETY: `tm`은 0으로 채운 값을 `localtime_r`가 채워 넣는 출력 자리이고, 두 포인터는 이 함수 안에서만 산다.
    let tm = unsafe {
        tzset();
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&seconds, &mut tm).is_null() {
            None
        } else {
            Some(tm)
        }
    };
    match tm {
        Some(tm) => LocalClock {
            date: CivilDate {
                year: i64::from(tm.tm_year) + 1900,
                month: (tm.tm_mon + 1) as u32,
                day: tm.tm_mday as u32,
            },
            hour: tm.tm_hour as u32,
            minute: tm.tm_min as u32,
            second: tm.tm_sec.min(59) as u32,
            // `tm_gmtoff`의 형은 플랫폼마다 다르다(`c_long`)
            #[allow(clippy::useless_conversion)]
            offset_seconds: i64::from(tm.tm_gmtoff),
        },
        None => {
            let (year, month, day) =
                crate::vault::note_meta::civil_from_days(millis.div_euclid(86_400_000));
            let rest = millis.rem_euclid(86_400_000);
            LocalClock {
                date: CivilDate { year, month, day },
                hour: (rest / 3_600_000) as u32,
                minute: (rest % 3_600_000 / 60_000) as u32,
                second: (rest % 60_000 / 1000) as u32,
                offset_seconds: 0,
            }
        }
    }
}

/// 이 프로세스의 시간대로 본 `yyyy-MM-ddTHH:mm:ss±HH:MM`(Java `ISO_OFFSET_DATE_TIME`, 초 아래는 버림). 오프셋이 0이면 `Z`다.
pub fn iso_offset_date_time(millis: i64) -> String {
    let clock = local_clock(millis);
    let offset = if clock.offset_seconds == 0 {
        "Z".to_string()
    } else {
        let abs = clock.offset_seconds.abs();
        let mut text = format!(
            "{}{:02}:{:02}",
            if clock.offset_seconds < 0 { '-' } else { '+' },
            abs / 3600,
            abs % 3600 / 60
        );
        if abs % 60 != 0 {
            text.push_str(&format!(":{:02}", abs % 60));
        }
        text
    };
    format!(
        "{}T{:02}:{:02}:{:02}{offset}",
        clock.date.iso(),
        clock.hour,
        clock.minute,
        clock.second
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follows_the_tz_variable() {
        // SAFETY: 환경 변수를 바꾸는 시험은 이것 하나뿐이다.
        unsafe {
            std::env::set_var("TZ", "Asia/Seoul");
        }
        let seoul = local_clock(1_794_700_000_000);
        assert_eq!(
            (seoul.date.iso(), seoul.hour, seoul.minute),
            ("2026-11-15".into(), 8, 46)
        );
        unsafe {
            std::env::set_var("TZ", "America/Los_Angeles");
        }
        let la = local_clock(1_794_700_000_000);
        assert_eq!(
            (la.date.iso(), la.hour, la.minute),
            ("2026-11-14".into(), 15, 46)
        );
        unsafe {
            std::env::set_var("TZ", "UTC");
        }
        assert_eq!(local_clock(0).date.iso(), "1970-01-01");
    }
}
