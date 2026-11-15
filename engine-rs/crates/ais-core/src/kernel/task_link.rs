//! 태스크끼리의 연결 종류 셋(`blocks`, `relates`, `duplicates`)과 반대쪽에서 부르는 이름.
//!
//! 연결은 건 쪽 파일에만 적는다. 받는 쪽에서 보여 줄 때는 반대쪽 이름(`blocked-by`, `duplicated-by`, `relates`)을 쓴다.

/// 태스크 연결 종류
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskLinkKind {
    /// 건 쪽이 받는 쪽을 막는다
    Blocks,
    /// 서로 관련 있다(양쪽 같은 이름)
    Relates,
    /// 건 쪽이 받는 쪽의 중복이다
    Duplicates,
}

impl TaskLinkKind {
    pub const ALL: [TaskLinkKind; 3] = [
        TaskLinkKind::Blocks,
        TaskLinkKind::Relates,
        TaskLinkKind::Duplicates,
    ];

    /// 오류 문구의 목록 글(`blocks, relates, duplicates`)
    pub const NAMES_TEXT: &'static str = "blocks, relates, duplicates";

    /// 파일에 적는 글자
    pub const fn as_str(self) -> &'static str {
        match self {
            TaskLinkKind::Blocks => "blocks",
            TaskLinkKind::Relates => "relates",
            TaskLinkKind::Duplicates => "duplicates",
        }
    }

    /// 받는 쪽에서 보여 주는 반대쪽 이름
    pub const fn inverse_label(self) -> &'static str {
        match self {
            TaskLinkKind::Blocks => "blocked-by",
            TaskLinkKind::Relates => "relates",
            TaskLinkKind::Duplicates => "duplicated-by",
        }
    }

    /// 글자가 정확히 같을 때만 읽는다
    pub fn parse(text: &str) -> Option<TaskLinkKind> {
        Self::ALL.into_iter().find(|k| k.as_str() == text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_inverse_labels() {
        let names: Vec<&str> = TaskLinkKind::ALL.iter().map(|k| k.as_str()).collect();
        assert_eq!(names.join(", "), TaskLinkKind::NAMES_TEXT);
        assert_eq!(TaskLinkKind::Blocks.inverse_label(), "blocked-by");
        assert_eq!(TaskLinkKind::Duplicates.inverse_label(), "duplicated-by");
        assert_eq!(TaskLinkKind::Relates.inverse_label(), "relates");
        assert_eq!(TaskLinkKind::parse("Blocks"), None);
    }
}
