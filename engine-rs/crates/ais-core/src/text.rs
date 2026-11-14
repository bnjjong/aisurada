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
