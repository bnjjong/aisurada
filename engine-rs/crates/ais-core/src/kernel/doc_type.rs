//! 노트(문서)의 유형 여섯 가지. 유형 목록은 한때 영역마다 열 벌이 따로 있었다(`vault/note_meta.rs`, `vault/filter.rs`, `vault/template.rs`,
//! `vault/trash.rs`, `commands/doc_create.rs`, `commands/sub_note_write.rs`, `commands/template_write.rs`, `commands/list_templates.rs`,
//! `commands/init_workspace.rs`와 오류 문구 안의 글자 목록). 이제 이 타입이 정본이다.
//!
//! 읽는 쪽과 쓰는 쪽이 비교하는 방식이 달라서(글자가 정확히 같아야 하는 곳, 대소문자를 무시하는 곳) 두 가지 읽는 길을 둔다.

/// 노트 유형
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocType {
    Plan,
    Build,
    Report,
    Research,
    Meeting,
    Free,
}

impl DocType {
    /// 모든 유형. 오류 문구와 목록의 차례가 이 차례다.
    pub const ALL: [DocType; 6] = [
        DocType::Plan,
        DocType::Build,
        DocType::Report,
        DocType::Research,
        DocType::Meeting,
        DocType::Free,
    ];

    /// 오류 문구에 찍는 목록 글(`[...]` 안): 「쓸 수 있는 것은 [plan, build, report, research, meeting, free] 뿐이다.」
    pub const NAMES_TEXT: &'static str = "plan, build, report, research, meeting, free";

    /// 파일과 응답에 적는 글자(소문자)
    pub const fn as_str(self) -> &'static str {
        match self {
            DocType::Plan => "plan",
            DocType::Build => "build",
            DocType::Report => "report",
            DocType::Research => "research",
            DocType::Meeting => "meeting",
            DocType::Free => "free",
        }
    }

    /// 글자가 정확히 같을 때만 읽는다(소문자만). 파일에서 읽을 때 쓴다.
    pub fn parse(text: &str) -> Option<DocType> {
        Self::ALL.into_iter().find(|t| t.as_str() == text)
    }

    /// 대소문자를 무시하고 읽는다(ASCII). 사용자가 입력한 유형 인자를 받을 때 쓴다.
    pub fn parse_ignore_ascii_case(text: &str) -> Option<DocType> {
        Self::ALL
            .into_iter()
            .find(|t| t.as_str().eq_ignore_ascii_case(text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_text_lists_every_type_in_order() {
        let names: Vec<&str> = DocType::ALL.iter().map(|t| t.as_str()).collect();
        assert_eq!(names.join(", "), DocType::NAMES_TEXT);
        assert_eq!(
            names,
            ["plan", "build", "report", "research", "meeting", "free"]
        );
    }

    #[test]
    fn exact_and_case_insensitive_reading_differ() {
        assert_eq!(DocType::parse("plan"), Some(DocType::Plan));
        assert_eq!(DocType::parse("Plan"), None);
        assert_eq!(DocType::parse(" plan"), None);
        assert_eq!(
            DocType::parse_ignore_ascii_case("MEETING"),
            Some(DocType::Meeting)
        );
        assert_eq!(DocType::parse_ignore_ascii_case("zzz"), None);
    }
}
