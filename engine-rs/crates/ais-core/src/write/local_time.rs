//! 이 프로세스의 시간대로 본 날짜와 시각. 새 노트의 기본 날짜(오늘)와 시각(지금) 배정에 쓴다.
//!
//! 시간대 풀이는 시계 포트(`ports::Clock::local_clock`)가 한다. 기본 어댑터는 `TZ` 환경 변수나 시스템 시간대를 따른다(`adapters/system_clock.rs`).

pub use crate::kernel::time::LocalClock;

/// 유닉스 밀리초를 이 프로세스의 시간대로 풀어 날짜와 시, 분, 초(초 아래는 버림)를 돌려준다. 시간대를 못 읽으면 UTC로 본다.
pub fn local_clock(millis: i64) -> LocalClock {
    crate::ports::clock().local_clock(millis)
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
