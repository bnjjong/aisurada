//! 본문의 첫 헤딩 글. 제목과 달라진 본문을 경고하려고 마크다운 `#` 헤딩, 없으면 HTML `<h1>`~`<h6>`에서 뽑는다.
//! 규칙은 Java 엔진의 `FirstHeading`과 같다.

use std::sync::LazyLock;

use regex::Regex;

use crate::text::trim;

/// Java의 `.`은 `\n`, `\r`, U+0085, U+2028, U+2029를 잡지 않으므로 같은 글자 모음으로 쓴다.
static MARKDOWN_HEADING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^[ \t\n\x0B\f\r]{0,3}#{1,6}[ \t\n\x0B\f\r]+([^\n\r\x{85}\x{2028}\x{2029}]+?)[ \t\n\x0B\f\r]*#*[ \t\n\x0B\f\r]*$",
    )
    .unwrap()
});
static HTML_HEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<h[1-6][^>]*>([\s\S]*?)</h[1-6]>").unwrap());
static HTML_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").unwrap());
static NUMERIC_APOSTROPHE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"&#0*39;").unwrap());
static ASCII_SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t\n\x0B\f\r]+").unwrap());

/// 본문의 첫 헤딩 글. 헤딩이 없거나 비었거나 `[제목]`이면 `None`이다.
pub fn extract(body: &str) -> Option<String> {
    if body.is_empty() {
        return None;
    }
    let heading = markdown_heading(body).or_else(|| html_heading(body))?;
    if heading.is_empty() || heading == "[제목]" {
        return None;
    }
    Some(heading)
}

/// 처음 맞는 `#` 헤딩 줄 하나만 본다. 그 줄의 글이 비었으면 다음 줄을 보지 않고 없는 것으로 친다.
fn markdown_heading(body: &str) -> Option<String> {
    for line in body.split('\n') {
        if let Some(captures) = MARKDOWN_HEADING.captures(line) {
            let text = trim(&captures[1]);
            return (!text.is_empty()).then(|| text.to_string());
        }
    }
    None
}

fn html_heading(body: &str) -> Option<String> {
    let inner = HTML_HEADING.captures(body)?.get(1)?.as_str();
    let without_tags = HTML_TAG.replace_all(inner, "");
    let unescaped = without_tags
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"");
    let unescaped = NUMERIC_APOSTROPHE.replace_all(&unescaped, "'");
    let unescaped = unescaped.replace("&apos;", "'").replace("&nbsp;", " ");
    let collapsed = ASCII_SPACES.replace_all(&unescaped, " ");
    Some(trim(&collapsed).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_first_markdown_heading() {
        assert_eq!(
            extract("본문\n\n## 두 번째 ##\n# 첫째"),
            Some("두 번째".into())
        );
        assert_eq!(extract("   # 들여쓴 헤딩  \n"), Some("들여쓴 헤딩".into()));
        assert_eq!(extract("    # 네 칸은 코드"), None);
        assert_eq!(extract("#해시 뒤 공백 없음"), None);
        assert_eq!(extract("# [제목]"), None);
        assert_eq!(extract(""), None);
    }

    #[test]
    fn empty_heading_stops_the_markdown_search_but_not_the_html_fallback() {
        assert_eq!(extract("# ##\n# 다음"), Some("#".into()));
        assert_eq!(extract("#   \n# 다음"), None);
        assert_eq!(
            extract("#   \n<h2>HTML 헤딩</h2>"),
            Some("HTML 헤딩".into())
        );
    }

    #[test]
    fn falls_back_to_html_headings() {
        assert_eq!(
            extract("<p>x</p>\n<H2 class=a>제목 &amp; <b>굵게</b>&#39;&nbsp;\n끝</H2>"),
            Some("제목 & 굵게' 끝".into())
        );
        assert_eq!(extract("<h1></h1>"), None);
    }
}
