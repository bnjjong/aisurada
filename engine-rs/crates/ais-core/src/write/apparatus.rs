//! 시험용 시계와 난수, id 발급.
//!
//! 환경 변수 `AISURADA_TEST_NOW`(유닉스 밀리초)와 `AISURADA_TEST_RANDOM`(쉼표로 가른 8자리 16진수 목록)이 있으면
//! 그 값을 쓰고, 없으면 실제 시계와 무작위 값을 씁니다. Java 엔진의 JVM 속성 `aisurada.test.now`,
//! `aisurada.test.random`과 같은 뜻이라, 같은 값을 주면 두 엔진이 같은 id와 시각을 냅니다.
//! 시험용 장치이므로 환경 변수를 안 주면 아무 일도 하지 않습니다.

use std::collections::VecDeque;
use std::fs::File;
use std::io::Read;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::text::trim;

/// 정해 둔 난수 목록. `None`이면 아직 환경 변수를 읽지 않았다.
static RANDOM_QUEUE: Mutex<Option<VecDeque<String>>> = Mutex::new(None);

/// 지금의 유닉스 밀리초
pub fn now_millis() -> i64 {
    if let Ok(value) = std::env::var("AISURADA_TEST_NOW")
        && let Ok(millis) = trim(&value).parse::<i64>()
    {
        return millis;
    }
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 시험 난수 목록을 환경 변수에서 다시 읽는다.
pub fn reset_random_queue() {
    *RANDOM_QUEUE.lock().unwrap() = Some(read_queue());
}

fn read_queue() -> VecDeque<String> {
    std::env::var("AISURADA_TEST_RANDOM")
        .map(|value| {
            value
                .split(',')
                .map(|part| trim(part).to_lowercase())
                .filter(|part| !part.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// id 꼬리로 쓸 소문자 16진수 8자리. 시험 난수 목록이 있으면 차례로 쓰고, 다 쓰면 무작위 값이다.
pub fn next_random_hex() -> String {
    {
        let mut guard = RANDOM_QUEUE.lock().unwrap();
        let queue = guard.get_or_insert_with(read_queue);
        if let Some(next) = queue.pop_front() {
            return next;
        }
    }
    random_bytes::<4>()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 임시 파일 이름에 쓸 64비트 난수
pub fn next_random_u64() -> u64 {
    u64::from_be_bytes(random_bytes::<8>())
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

/// 유닉스 밀리초를 UTC `yyyy-MM-ddTHH:mm:ss.SSSZ`로 보여 준다(워크스페이스 표식의 `createdAt`).
pub fn format_utc_millis(millis: i64) -> String {
    let days = millis.div_euclid(86_400_000);
    let rest = millis.rem_euclid(86_400_000);
    let (year, month, day) = crate::vault::note_meta::civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3_600_000,
        rest % 3_600_000 / 60_000,
        rest % 60_000 / 1000,
        rest % 1000
    )
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

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buffer = [0u8; N];
    if let Ok(mut file) = File::open("/dev/urandom")
        && file.read_exact(&mut buffer).is_ok()
    {
        return buffer;
    }
    // 운영체제 난수를 못 읽으면 프로세스마다 다른 해시 씨앗과 시각으로 채운다
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    for chunk in buffer.chunks_mut(8) {
        hasher.write_u8(1);
        let bytes = hasher.finish().to_be_bytes();
        chunk.copy_from_slice(&bytes[..chunk.len()]);
    }
    buffer
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
