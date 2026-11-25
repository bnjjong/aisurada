//! 노트 수정 후 내용 경고 수집: 들여쓴 블록, 제목과 첫 헤딩 불일치, 깨진 문자(U+FFFD) 검출.

use super::*;

/// 본문 변경에 따른 내용 경고(들여쓴 블록, 제목 불일치, 깨진 글자)를 수집한다.
pub(crate) fn collect_content_warnings(
    format: Format,
    given_body: Option<&str>,
    old_body: &str,
    rendered_body: &str,
    title: &str,
    warnings: &mut Vec<String>,
) {
    if let Some(given) = given_body.filter(|_| format != Format::Html) {
        warnings.extend(warnings::detect_indented_blocks_warning(Some(given)));
    }
    if format == Format::Markdown {
        warnings.extend(warnings::detect_title_mismatch_warning(rendered_body, title));
    }
    if let Some(given) = given_body {
        let grown = warnings::count_broken_chars(given) as i64
            - warnings::count_broken_chars(old_body) as i64;
        if grown > 0 {
            warnings.push(warnings::broken_char_warning(given, grown as usize));
        }
    }
}
