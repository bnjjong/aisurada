//! 글 다루는 작은 도구들입니다. 공백의 범위와 정렬 차례는 Java 엔진과 같게 맞춰 기존 vault가 같은 결과를 내게 합니다.

use std::cmp::Ordering;
use std::sync::LazyLock;

use regex::Regex;

/// 노트 id 모양: `nd…-xxxxxxxx`
pub static NOTE_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^nd[0-9a-z]+-[0-9a-f]{8}$").unwrap());
/// 후속 노트 id 모양: `sn…-xxxxxxxx`
pub static SUB_NOTE_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^sn[0-9a-z]+-[0-9a-f]{8}$").unwrap());

/// 엔진이 공백으로 보는 글자입니다(Java `Character.isWhitespace`보다 넓은 집합이 아니라 엔진이 정한 목록).
pub fn is_whitespace(c: char) -> bool {
    matches!(c,
        '\t'..='\r'
        | '\u{1C}'..='\u{1F}'
        | ' '
        | '\u{A0}'
        | '\u{1680}'
        | '\u{2000}'..='\u{200A}'
        | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}'
        | '\u{3000}')
}

/// 앞뒤 공백을 뗍니다.
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_whitespace)
}

/// 비었거나 공백뿐인지 봅니다.
pub fn is_blank(s: &str) -> bool {
    s.chars().all(is_whitespace)
}

/// UTF-16 코드 단위 차례로 비교합니다. 파일 이름을 정렬할 때 쓰며 BMP 밖 글자가 있어도 Java와 같은 차례가 나옵니다.
pub fn compare_utf16(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// UTF-16 코드 단위로 센 길이입니다.
pub fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// 최대 UTF-16 코드 단위 수로 자릅니다. 짝이 깨지는 자리에서 자르면 그 글자까지 뺍니다.
pub fn cut_utf16(s: &str, max_units: usize) -> String {
    let mut units = 0;
    let mut out = String::new();
    for c in s.chars() {
        units += c.len_utf16();
        if units > max_units {
            break;
        }
        out.push(c);
    }
    out
}

/// JavaScript `encodeURIComponent`와 같은 퍼센트 인코딩입니다(`A-Za-z0-9 - _ . ! ~ * ' ( )`만 그대로 둔다).
pub fn encode_uri_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// 검색에서 공백으로 보는 글자: 엔진의 공백에 U+FEFF(BOM)를 더한 것
pub fn is_search_whitespace(c: char) -> bool {
    is_whitespace(c) || c == '\u{FEFF}'
}

/// 앞뒤의 검색 공백을 뗀다.
pub fn trim_search(s: &str) -> &str {
    s.trim_matches(is_search_whitespace)
}

/// 영문 대소문자를 가리지 않고 `needle`이 처음 나오는 자리(UTF-16 코드 단위 번호)
pub fn find_region_match(text: &[u16], needle: &[u16]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > text.len() {
        return None;
    }
    (0..=text.len() - needle.len()).find(|&i| {
        text[i..i + needle.len()]
            .iter()
            .zip(needle)
            .all(|(a, b)| equals_ignore_case(*a, *b))
    })
}

/// Java `String.regionMatches(true, …)`가 글자 하나씩 보는 규칙: 같거나, 대문자가 같거나, 대문자의 소문자가 같다.
fn equals_ignore_case(a: u16, b: u16) -> bool {
    a == b || upper(a) == upper(b) || lower(upper(a)) == lower(upper(b))
}

fn upper(unit: u16) -> u16 {
    map_single(unit, |c| {
        c.to_uppercase()
            .next()
            .filter(|_| c.to_uppercase().count() == 1)
    })
}

fn lower(unit: u16) -> u16 {
    // Java의 `Character.toLowerCase`(한 글자 규칙)는 İ(U+0130)를 i로 본다. Rust의 전체 규칙은 두 글자(i + 결합 점)라 따로 맞춘다.
    if unit == 0x0130 {
        return u16::from(b'i');
    }
    map_single(unit, |c| {
        c.to_lowercase()
            .next()
            .filter(|_| c.to_lowercase().count() == 1)
    })
}

/// 짝이 있는 서로게이트가 아닌 글자 하나에만 대소문자 변환을 적용한다(한 글자로 바뀔 때만).
fn map_single(unit: u16, convert: impl Fn(char) -> Option<char>) -> u16 {
    match char::from_u32(u32::from(unit)).and_then(convert) {
        Some(c) if (c as u32) <= 0xFFFF => c as u16,
        _ => unit,
    }
}

#[cfg(test)]
mod ignore_case_tests {
    use super::*;

    fn utf16(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn ignores_case_per_code_unit() {
        let find = |text: &str, needle: &str| find_region_match(&utf16(text), &utf16(needle));
        assert_eq!(find("Hello World", "WORLD"), Some(6));
        assert_eq!(find("한글 ABC", "abc"), Some(3));
        assert_eq!(find("abc", "abcd"), None);
        assert_eq!(find("abc", ""), Some(0));
        assert!(equals_ignore_case('ǆ' as u16, 'Ǆ' as u16));
        assert!(equals_ignore_case('İ' as u16, 'i' as u16));
    }
}
