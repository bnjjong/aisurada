//! 쓰기 응답에 붙는 경고 글. 저장은 됐지만 사용자가 알아야 하는 것을 알려 준다.

use std::sync::LazyLock;

use regex::Regex;

use crate::text::{is_whitespace, trim};

static CALLOUT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\[![a-zA-Z0-9_]+\]").unwrap());

/// 줄 맨 앞이 아니라 들여써서 편집기가 변환하지 못하는 블록(제목, 인용, 콜아웃, 하위 페이지 카드, 표)을 찾아 알린다.
/// 코드 울타리 안은 보지 않는다. 없으면 `None`이다.
pub fn detect_indented_blocks_warning(body: Option<&str>) -> Option<String> {
    let body = body.filter(|b| !b.is_empty())?;
    let normalized = body.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();

    let mut places: Vec<String> = Vec::new();
    let mut total = 0usize;
    let mut in_fence = false;
    for (i, line) in lines.iter().enumerate() {
        let first_non_space = line.bytes().take_while(|b| is_ascii_space(*b)).count();
        if line[first_non_space..].starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence || line.is_empty() || !is_ascii_space(line.as_bytes()[0]) {
            continue;
        }
        if let Some(kind) = classify(&line[first_non_space..], &lines, i) {
            total += 1;
            if places.len() < 5 {
                places.push(format!("{}번째 줄 {kind}", i + 1));
            }
        }
    }
    if total == 0 {
        return None;
    }
    let mut position = places.join(", ");
    if total > 5 {
        position.push_str(&format!(" 외 {}곳", total - 5));
    }
    Some(format!(
        "본문에 들여쓴 블록이 {total}곳 있어요({position}) — 표·콜아웃·제목·하위 페이지 카드는 줄 맨 앞에서 시작해야 편집기가 변환합니다. 지금 상태로는 저장은 됐지만 화면에 그냥 문단 글자로 보입니다. 들여쓰기를 떼고 update_doc({{ id, body }})으로 다시 저장하세요. 리스트 항목 안에는 표를 넣을 수 없으니, 항목 문장에서 \"아래 표\"라고 가리키고 표는 들여쓰기 없이 다음 줄에 두세요."
    ))
}

fn is_ascii_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | 0x0B | 0x0C)
}

fn classify(r: &str, lines: &[&str], index: usize) -> Option<&'static str> {
    let hashes = r.bytes().take_while(|b| *b == b'#').count();
    if (1..=6).contains(&hashes) && r.as_bytes().get(hashes).is_some_and(|b| is_ascii_space(*b)) {
        return Some("제목");
    }
    if let Some(after) = r.strip_prefix('>') {
        let after = match after.as_bytes().first() {
            Some(b) if is_ascii_space(*b) => &after[1..],
            _ => after,
        };
        return Some(if is_callout(after) {
            "콜아웃"
        } else {
            "인용"
        });
    }
    if r.starts_with('[')
        && let Some(close) = r.find(']')
        && close > 0
        && r[close..].starts_with("](#sub:")
    {
        return Some("하위 페이지 카드");
    }
    if r.contains('|')
        && let Some(next) = lines.get(index + 1)
        && is_table_separator(trim(next))
    {
        return Some("표");
    }
    None
}

/// `[!이름]`으로 시작하고, 그 뒤 글의 맨 끝이 아닌 자리에 U+0085, U+2028, U+2029가 없으면 콜아웃이다.
fn is_callout(text: &str) -> bool {
    let Some(found) = CALLOUT.find(text) else {
        return false;
    };
    let rest: Vec<char> = text[found.end()..].chars().collect();
    let body = &rest[..rest.len().saturating_sub(1)];
    !body
        .iter()
        .any(|c| matches!(c, '\u{85}' | '\u{2028}' | '\u{2029}'))
}

fn is_table_separator(trimmed: &str) -> bool {
    if trimmed.is_empty() {
        return false;
    }
    let mut hyphen = false;
    for c in trimmed.chars() {
        if is_whitespace(c) || c == ':' || c == '|' {
            continue;
        }
        if c == '-' {
            hyphen = true;
        } else {
            return false;
        }
    }
    hyphen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_indented_blocks_outside_fences() {
        assert_eq!(detect_indented_blocks_warning(None), None);
        assert_eq!(
            detect_indented_blocks_warning(Some("# 제목\n\n본문\n- 항목")),
            None
        );
        let body = "  ## 들여쓴 제목\n```\n  # 울타리 안\n```\n\t> 인용\n  > [!note] 콜아웃\n - 항목\n  | a | b |\n  |---|---|\n  [카드](#sub:x)";
        let warning = detect_indented_blocks_warning(Some(body)).unwrap();
        assert!(warning.starts_with("본문에 들여쓴 블록이 5곳 있어요(1번째 줄 제목, 5번째 줄 인용, 6번째 줄 콜아웃, 8번째 줄 표, 10번째 줄 하위 페이지 카드) — "), "{warning}");
        assert!(warning.contains("update_doc({ id, body })"));
    }

    #[test]
    fn counts_beyond_five_places() {
        let body = (0..7).map(|_| " # 제목").collect::<Vec<_>>().join("\n");
        let warning = detect_indented_blocks_warning(Some(&body)).unwrap();
        assert!(warning.contains("7곳 있어요(1번째 줄 제목, 2번째 줄 제목, 3번째 줄 제목, 4번째 줄 제목, 5번째 줄 제목 외 2곳)"), "{warning}");
    }
}
