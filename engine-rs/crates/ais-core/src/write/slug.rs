//! 제목에서 폴더·파일 이름(slug)을 만든다. 글자와 숫자만 남기고 나머지는 하이픈 하나로 줄인다.

use crate::text::{cut_utf16, trim};

/// `raw`를 slug로 만든다: 앞뒤 공백을 떼고 소문자로 바꾸고, 따옴표를 빼고, 글자·숫자가 아닌 묶음을 하이픈 하나로 줄이고,
/// 앞뒤 하이픈을 떼고, 비면 `untitled`, 길면 UTF-16 `max_units`에서 자른다.
///
/// Java 엔진은 유니코드 일반 범주(글자 `L*`, 숫자 `N*`)로 가르는데 Rust 표준 라이브러리에는 범주 조회가 없어서
/// `is_alphabetic`과 `is_numeric`을 쓴다. 한글·영문·숫자는 같고, 다른 문자 체계의 결합 부호(`Mn`, `Mc`)만 다르다
/// (Java는 구분자로 보고 여기서는 글자로 본다).
pub fn build(raw: &str, max_units: usize) -> String {
    let lowered = trim(raw).to_lowercase();
    let mut slug = String::new();
    let mut in_dash = false;
    for c in lowered.chars().filter(|c| *c != '\'' && *c != '"') {
        if c.is_alphabetic() || c.is_numeric() {
            slug.push(c);
            in_dash = false;
        } else if !in_dash {
            slug.push('-');
            in_dash = true;
        }
    }
    let trimmed = slug.trim_matches('-');
    if trimmed.is_empty() {
        return "untitled".to_string();
    }
    cut_utf16(trimmed, max_units)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_slugs_like_the_java_engine() {
        assert_eq!(build("  Hello,  World!! ", 50), "hello-world");
        assert_eq!(build("새 아젠다", 50), "새-아젠다");
        assert_eq!(build("It's a \"Quote\"", 50), "its-a-quote");
        assert_eq!(build("😀 ✨", 50), "untitled");
        assert_eq!(build("---", 50), "untitled");
        assert_eq!(build("a".repeat(80).as_str(), 50).len(), 50);
        assert_eq!(build("한글２", 50), "한글２");
    }
}
