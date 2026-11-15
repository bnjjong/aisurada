//! 시험용 시계와 난수, id 발급.
//!
//! 시계와 난수는 포트(`ports::Clock`, `ports::IdSource`)가 준다. 기본 어댑터(`adapters/system_clock.rs`, `adapters/queue_ids.rs`)는
//! 환경 변수 `AISURADA_TEST_NOW`(유닉스 밀리초)와 `AISURADA_TEST_RANDOM`(쉼표로 가른 8자리 16진수 목록)이 있으면 그 값을 쓰고,
//! 없으면 실제 시계와 무작위 값을 쓴다. Java 엔진의 JVM 속성 `aisurada.test.now`, `aisurada.test.random`과 같은 뜻이라,
//! 같은 값을 주면 두 엔진이 같은 id와 시각을 낸다. 이 파일은 그 값으로 id와 시각 글을 짓는다.

use crate::kernel::time::format_utc_millis;
use crate::ports;

/// 지금의 유닉스 밀리초
pub fn now_millis() -> i64 {
    ports::clock().now_millis()
}

/// 시험 난수 목록을 환경 변수에서 다시 읽는다.
pub fn reset_random_queue() {
    ports::ids().reset();
}

/// id 꼬리로 쓸 소문자 16진수 8자리. 시험 난수 목록이 있으면 차례로 쓰고, 다 쓰면 무작위 값이다.
pub fn next_random_hex() -> String {
    ports::ids().next_random_hex()
}

/// 임시 파일 이름에 쓸 64비트 난수
pub fn next_random_u64() -> u64 {
    ports::ids().next_random_u64()
}

/// `접두어 + 36진수 밀리초 + '-' + 16진수 8자리`로 새 id를 만든다(예: `nd` → `ndabc123-0123abcd`).
pub fn next_id(prefix: &str) -> String {
    format!("{prefix}{}-{}", to_base36(now_millis()), next_random_hex())
}

/// 노트 id(`nd`)
pub fn next_note_id() -> String {
    next_id("nd")
}

/// 후속 노트 id(`sn`)
pub fn next_sub_note_id() -> String {
    next_id("sn")
}

/// Java `Instant.toString()`: UTC `yyyy-MM-ddTHH:mm:ss`에 밀리초가 0이 아니면 `.mmm`을 붙이고 `Z`로 끝낸다.
pub fn instant_text(millis: i64) -> String {
    let base = format_utc_millis(millis);
    match base.strip_suffix(".000Z") {
        Some(head) => format!("{head}Z"),
        None => base,
    }
}

/// 부호 있는 정수를 소문자 36진수 글로 바꾼다(Java `Long.toString(n, 36)`).
pub fn to_base36(value: i64) -> String {
    to_base36_unsigned(value.unsigned_abs(), value < 0)
}

/// 부호 없는 64비트 정수를 소문자 36진수 글로 바꾼다(Java `Long.toUnsignedString(n, 36)`).
pub fn to_base36_u64(value: u64) -> String {
    to_base36_unsigned(value, false)
}

fn to_base36_unsigned(mut value: u64, negative: bool) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".to_string();
    }
    let mut out = Vec::new();
    while value > 0 {
        out.push(DIGITS[(value % 36) as usize]);
        value /= 36;
    }
    if negative {
        out.push(b'-');
    }
    out.reverse();
    String::from_utf8(out).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base36_matches_java() {
        assert_eq!(to_base36(0), "0");
        assert_eq!(to_base36(35), "z");
        assert_eq!(to_base36(36), "10");
        assert_eq!(to_base36(1_794_649_612_772), "mwg81jok");
        assert_eq!(to_base36(-36), "-10");
        assert_eq!(to_base36_u64(u64::MAX), "3w5e11264sgsf");
    }

    #[test]
    fn formats_utc_millis() {
        assert_eq!(format_utc_millis(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            format_utc_millis(1_794_700_000_123),
            "2026-11-14T23:46:40.123Z"
        );
    }

    #[test]
    fn random_hex_is_eight_lowercase_hex_digits() {
        let value = next_random_hex();
        assert_eq!(value.len(), 8);
        assert!(
            value
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        );
        assert_ne!(next_random_u64(), next_random_u64());
    }
}
