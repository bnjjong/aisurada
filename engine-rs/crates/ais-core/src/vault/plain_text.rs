//! 노트 본문에서 검색할 글만 뽑는다: 마크다운 기호를 벗기고, HTML은 태그를 지우고, JSON은 비운다.

use std::sync::LazyLock;

use regex::{Captures, Regex};

use super::attach_text::html_to_plain_text;
use super::scanner::Format;
use crate::text::trim;

// Java의 `\s`는 ASCII 공백뿐이라 같은 글자 모임을 직접 적는다.
const WS: &str = r"[ \t\n\x0B\x0C\r]";

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap()
}

static COMMENT: LazyLock<Regex> = LazyLock::new(|| re(r"<!--[\s\S]*?-->"));
static FENCE: LazyLock<Regex> = LazyLock::new(|| re(r"```[\s\S]*?```"));
static INLINE_CODE: LazyLock<Regex> = LazyLock::new(|| re(r"`([^`]*)`"));
static IMAGE: LazyLock<Regex> = LazyLock::new(|| re(r"!\[[^\]]*\]\([^)]*\)"));
static LINK: LazyLock<Regex> = LazyLock::new(|| re(r"\[([^\]]*)\]\([^)]*\)"));
static HEADING: LazyLock<Regex> = LazyLock::new(|| re(&format!(r"(?m)^{WS}{{0,3}}#{{1,6}}{WS}+")));
static QUOTE: LazyLock<Regex> =
    LazyLock::new(|| re(&format!(r"(?mi)^{WS}*>{WS}?(\[![a-z]+\])?{WS}*")));
static LIST: LazyLock<Regex> = LazyLock::new(|| {
    re(&format!(
        r"(?m)^{WS}*(?:[-*+]|[0-9]+\.){WS}+(?:\[[ x/-]\]{WS}*)?"
    ))
});
static TABLE_RULE: LazyLock<Regex> =
    LazyLock::new(|| re(&format!(r"(?m)^{WS}*\|[ \t\n\x0B\x0C\r:|-]*\|{WS}*$")));
static TABLE_ROW: LazyLock<Regex> = LazyLock::new(|| re(&format!(r"(?m)^{WS}*\|.*\|{WS}*$")));
static RULE: LazyLock<Regex> = LazyLock::new(|| re(&format!(r"(?m)^{WS}*[-*_]{{3,}}{WS}*$")));
static EMPHASIS: LazyLock<Regex> =
    LazyLock::new(|| re(r"\*\*([^*]+)\*\*|\*([^*]+)\*|__([^_]+)__|~~([^~]+)~~"));
static SPACES: LazyLock<Regex> = LazyLock::new(|| re(&format!(r"{WS}+")));

/// 본문 글에서 검색 대상 글을 뽑는다.
pub fn extract(format: Format, raw_body: &str) -> String {
    if raw_body.is_empty() {
        return String::new();
    }
    match format {
        Format::Json => return String::new(),
        Format::Html => return html_to_plain_text(raw_body),
        Format::Markdown => {}
    }
    let s = COMMENT.replace_all(raw_body, " ");
    let s = FENCE.replace_all(&s, " ");
    let s = INLINE_CODE.replace_all(&s, "${1}");
    let s = IMAGE.replace_all(&s, " ");
    let s = LINK.replace_all(&s, "${1}");
    let s = HEADING.replace_all(&s, "");
    let s = QUOTE.replace_all(&s, "");
    let s = LIST.replace_all(&s, "");
    let s = TABLE_RULE.replace_all(&s, " ");
    let s = TABLE_ROW.replace_all(&s, |caps: &Captures| caps[0].replace('|', " "));
    let s = RULE.replace_all(&s, " ");
    let s = EMPHASIS.replace_all(&s, |caps: &Captures| {
        (1..=4)
            .find_map(|g| caps.get(g))
            .map(|m| m.as_str().to_string())
            .unwrap_or_default()
    });
    trim(&SPACES.replace_all(&s, " ")).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_markdown_marks() {
        let body = "# 제목\n\n> 인용 **굵게** 와 *기울임*\n- 항목 `코드`\n1. 둘째 [링크](http://x) ![그림](a.png)\n\n| a | b |\n|---|---|\n| c | d |\n\n---\n```\nskip\n```\n<!-- 주석 -->끝";
        assert_eq!(
            extract(Format::Markdown, body),
            "제목 인용 굵게 와 기울임 항목 코드 둘째 링크 a b c d 끝"
        );
    }

    #[test]
    fn json_is_empty_and_html_loses_tags() {
        assert_eq!(extract(Format::Json, "{\"a\":1}"), "");
        assert_eq!(extract(Format::Html, "<p>a &amp; b</p>"), "a & b");
        assert_eq!(extract(Format::Markdown, ""), "");
    }
}
