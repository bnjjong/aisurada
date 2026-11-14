//! 노트 본문 파일 읽기: 울타리로 머리와 본문을 가르고, 파일 전체의 SHA-256으로 고치기 전 확인 값(`f1:…`)을 만듭니다.

use std::fs;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::frontmatter::{self, Value};
use crate::text::utf16_len;

/// 본문 파일 크기 상한입니다.
const MAX_NOTE_FILE_BYTES: u64 = 1_000_000;
/// 머리 YAML 길이 상한(UTF-16 코드 단위)입니다.
const MAX_FRONTMATTER_UNITS: usize = 16_000;

/// 읽은 본문 파일입니다.
pub struct ParsedNoteFile {
    /// 머리가 맵으로 읽혔을 때만 있다.
    pub frontmatter: Option<Vec<(Value, Value)>>,
    pub body: String,
    /// 파일 전체 바이트의 SHA-256(`f1:` + 소문자 16진수 64자)
    pub token: String,
}

/// 본문 파일을 읽지 못했다: 없거나, 1MB를 넘거나, UTF-8이 아니다.
#[derive(Debug)]
pub struct UnreadableFile;

/// 파일 전체 바이트의 SHA-256으로 고치기 전 확인 값을 만듭니다.
pub fn compute_token(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut token = String::with_capacity(3 + 64);
    token.push_str("f1:");
    for byte in digest {
        token.push_str(&format!("{byte:02x}"));
    }
    token
}

/// 본문 파일을 읽어 머리, 본문, 확인 값을 돌려줍니다.
pub fn read_note_file(path: &Path) -> Result<ParsedNoteFile, UnreadableFile> {
    let size = fs::metadata(path).map_err(|_| UnreadableFile)?.len();
    if size > MAX_NOTE_FILE_BYTES {
        return Err(UnreadableFile);
    }
    parse_note_bytes(&fs::read(path).map_err(|_| UnreadableFile)?)
}

pub fn parse_note_bytes(bytes: &[u8]) -> Result<ParsedNoteFile, UnreadableFile> {
    let token = compute_token(bytes);
    let content = std::str::from_utf8(bytes).map_err(|_| UnreadableFile)?;
    let split = split_text(content);
    Ok(ParsedNoteFile {
        frontmatter: split.frontmatter,
        body: split.body,
        token,
    })
}

/// 글을 머리와 본문으로 가른 결과입니다.
pub struct SplitText {
    /// 울타리가 있고 머리가 맵으로 읽혔을 때만 있다.
    pub frontmatter: Option<Vec<(Value, Value)>>,
    /// 울타리 뒤의 본문. 울타리가 없으면 빈 글이다.
    pub body: String,
}

/// 이미 UTF-8 글로 읽은 노트 내용을 머리와 본문으로 가릅니다. 맨 앞 BOM은 떼고 보고,
/// 머리가 길이 상한(UTF-16 16,000자)을 넘으면 머리를 읽지 않습니다. 파일 크기와 UTF-8 검사는 부르는 쪽 몫입니다.
pub fn split_text(content: &str) -> SplitText {
    let content = content.strip_prefix('\u{FEFF}').unwrap_or(content);

    let Some((yaml, raw_body)) = split_fence(content) else {
        return SplitText {
            frontmatter: None,
            body: String::new(),
        };
    };

    // 본문 앞 줄바꿈을 정리한다: \r\n 1개를 떼고, 이어서 \n 1개를 뗀다
    let body = raw_body.strip_prefix("\r\n").unwrap_or(raw_body);
    let body = body.strip_prefix('\n').unwrap_or(body).to_string();

    if utf16_len(yaml) > MAX_FRONTMATTER_UNITS {
        return SplitText {
            frontmatter: None,
            body,
        };
    }
    SplitText {
        frontmatter: frontmatter::parse(yaml),
        body,
    }
}

/// 울타리로 머리 YAML과 본문을 가릅니다. 마크다운·JSON 노트는 `---` 울타리, HTML 노트는 `<!--aisurada … -->` 울타리입니다.
/// 울타리가 없으면 `None`입니다.
pub fn split_fence(content: &str) -> Option<(&str, &str)> {
    const HTML_OPEN: &str = "<!--aisurada";
    if content.starts_with(HTML_OPEN) {
        let start = HTML_OPEN.len();
        let close = start + content[start..].find("\n-->")?;
        return Some((&content[start..close], &content[close + 4..]));
    }
    if let Some(after_open) = content.strip_prefix("---") {
        let first_lf = 3 + after_open.find('\n')?;
        // 여는 줄 바로 다음 줄이 닫는 울타리일 수도 있어서 첫 줄바꿈 자리부터 찾는다
        let close = first_lf + content[first_lf..].find("\n---")?;
        return Some((&content[first_lf + 1..close + 1], &content[close + 4..]));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_markdown_fence_and_trims_one_leading_newline() {
        let parsed = parse_note_bytes("---\nid: a\n---\n\n# 제목\n".as_bytes()).unwrap();
        assert!(parsed.frontmatter.is_some());
        // 닫는 울타리 뒤 줄바꿈 1개만 뗀다
        assert_eq!(parsed.body, "\n# 제목\n");
    }

    #[test]
    fn splits_html_fence() {
        let parsed = parse_note_bytes("<!--aisurada\nid: a\n-->\n<p>x</p>".as_bytes()).unwrap();
        assert!(parsed.frontmatter.is_some());
        assert_eq!(parsed.body, "<p>x</p>");
    }

    #[test]
    fn handles_bom_crlf_and_missing_fence() {
        let parsed = parse_note_bytes("\u{FEFF}---\nid: a\n---\r\nbody".as_bytes()).unwrap();
        assert_eq!(parsed.body, "body");
        let none = parse_note_bytes("# 머리 없음\n".as_bytes()).unwrap();
        assert!(none.frontmatter.is_none());
    }

    #[test]
    fn empty_frontmatter_block_is_found_but_not_a_map() {
        let parsed = parse_note_bytes("---\n---\nbody".as_bytes()).unwrap();
        assert!(parsed.frontmatter.is_none());
        assert_eq!(parsed.body, "body");
    }

    #[test]
    fn rejects_invalid_utf8_and_makes_a_stable_token() {
        assert!(parse_note_bytes(&[0xff, 0xfe]).is_err());
        let a = compute_token(b"abc");
        assert_eq!(
            a,
            "f1:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
