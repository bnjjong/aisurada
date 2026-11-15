//! 노트와 노트 사이의 연결 종류 셋(`direct`, `indirect`, `attach`).
//!
//! 이 목록은 세 곳에 따로 있었다(`vault/note_meta.rs`의 읽기, `commands/doc_create.rs`와 `commands/note_meta_write.rs`의 인자 검사).
//! 읽을 때는 글자가 정확히 같아야 하고, 인자는 대소문자를 무시한다.

/// 노트 연결 종류
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    Direct,
    Indirect,
    Attach,
}

impl LinkKind {
    pub const ALL: [LinkKind; 3] = [LinkKind::Direct, LinkKind::Indirect, LinkKind::Attach];

    /// 오류 문구의 목록 글: 「쓸 수 있는 것은 [direct, indirect, attach] 뿐이다.」
    pub const NAMES_TEXT: &'static str = "direct, indirect, attach";

    pub const fn as_str(self) -> &'static str {
        match self {
            LinkKind::Direct => "direct",
            LinkKind::Indirect => "indirect",
            LinkKind::Attach => "attach",
        }
    }

    /// 글자가 정확히 같을 때만 읽는다(파일에서 읽을 때)
    pub fn parse(text: &str) -> Option<LinkKind> {
        Self::ALL.into_iter().find(|k| k.as_str() == text)
    }

    /// 대소문자를 무시하고 읽는다(ASCII, 인자를 받을 때)
    pub fn parse_ignore_ascii_case(text: &str) -> Option<LinkKind> {
        Self::ALL
            .into_iter()
            .find(|k| k.as_str().eq_ignore_ascii_case(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_text_lists_every_kind_in_order() {
        let names: Vec<&str> = LinkKind::ALL.iter().map(|k| k.as_str()).collect();
        assert_eq!(names.join(", "), LinkKind::NAMES_TEXT);
        assert_eq!(LinkKind::parse("Direct"), None);
        assert_eq!(
            LinkKind::parse_ignore_ascii_case("ATTACH"),
            Some(LinkKind::Attach)
        );
    }
}
