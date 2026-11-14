//! 노트의 `attach/` 폴더에 든 HTML 첨부에서 글만 뽑아 한 줄로 잇습니다(검색과 본문 미리보기용).

use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::scanner::NoteRecord;
use crate::text::{compare_utf16, trim, utf16_len};

const MAX_FILE_BYTES: u64 = 4_194_304;
const MAX_TEXT_UNITS: usize = 60_000;

static SCRIPT_STYLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<script[\s\S]*?</script>|<style[\s\S]*?</style>").unwrap());
static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").unwrap());
static APOS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"&#0*39;").unwrap());
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[ \t\n\x0B\x0C\r]+").unwrap());

/// 노트의 HTML 첨부 글을 뽑습니다. 파일 하나로 된 노트이거나 HTML 첨부가 없으면 `None`입니다.
pub fn extract(vault_root: &Path, note: &NoteRecord) -> Option<String> {
    if note.is_single_file {
        return None;
    }
    let attach_dir = vault_root.join(&note.rel_path).join("attach");
    let mut names: Vec<String> = fs::read_dir(&attach_dir)
        .ok()?
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| is_html_attachment_name(n))
        .collect();
    if names.is_empty() {
        return None;
    }
    names.sort_by(|a, b| compare_utf16(a, b));

    let mut texts: Vec<String> = Vec::new();
    for name in names {
        let file = attach_dir.join(&name);
        // 심볼릭 링크는 따라간다. 일반 파일이 아니거나 너무 크면 건너뛴다
        let Ok(meta) = fs::metadata(&file) else {
            continue;
        };
        if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
            continue;
        }
        let Ok(bytes) = fs::read(&file) else { continue };
        let plain = html_to_plain_text(&String::from_utf8_lossy(&bytes));
        if !plain.is_empty() {
            texts.push(plain);
        }
    }
    if texts.is_empty() {
        return None;
    }
    let joined = texts.join(" ");
    Some(if utf16_len(&joined) > MAX_TEXT_UNITS {
        crate::text::cut_utf16(&joined, MAX_TEXT_UNITS)
    } else {
        joined
    })
}

fn is_html_attachment_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    if !lower.ends_with(".html") && !lower.ends_with(".htm") {
        return false;
    }
    let Some(dot) = name.rfind('.') else {
        return false;
    };
    if dot == 0 {
        return false;
    }
    // 점 바로 앞에 U+0085, U+2028, U+2029가 든 이름은 제외
    !matches!(
        name[..dot].chars().last(),
        Some('\u{85}' | '\u{2028}' | '\u{2029}')
    )
}

/// HTML에서 script·style을 빼고 태그를 지운 뒤, 엔티티를 풀고 공백을 하나로 모읍니다.
pub fn html_to_plain_text(html: &str) -> String {
    let step1 = SCRIPT_STYLE.replace_all(html, " ");
    let step2 = TAG.replace_all(&step1, " ");
    let step3 = step2
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"");
    let step3 = APOS
        .replace_all(&step3, "'")
        .replace("&apos;", "'")
        .replace("&nbsp;", " ");
    trim(&SPACES.replace_all(&step3, " ")).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_tags_scripts_and_entities() {
        let html = "<html><style>p{}</style><body><p>a &amp; b</p><script>x()</script><p>&quot;q&quot;&nbsp;&#39;</p></body></html>";
        assert_eq!(html_to_plain_text(html), "a & b \"q\" '");
    }

    #[test]
    fn html_attachment_names() {
        assert!(is_html_attachment_name("map.html"));
        assert!(is_html_attachment_name("MAP.HTM"));
        assert!(!is_html_attachment_name(".html"));
        assert!(!is_html_attachment_name("a.txt"));
    }
}
