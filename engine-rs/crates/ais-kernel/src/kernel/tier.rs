//! 노트의 층(tier)과 누가 어느 층을 보는지의 정책(`NoteVisibility`).
//!
//! 층은 둘이다. **읽기 층**(`reading`, 기본)은 앱과 AI가 모두 보고, **기록 층**(`record`)은 사람이 앱에서만 보는 노트로 AI에게는 목록·검색·열기에서
//! 보이지 않는다. 이 규칙은 한때 아홉 곳에 세 가지 표기로 흩어져 있었다(`== "record"`, `!= "reading"`, `eq_ignore_ascii_case("record")`).
//! 파일 머리에서 읽을 때 층이 정확히 `reading` 또는 `record` 둘 중 하나로 정규화되므로(`from_front`) 세 표기는 같은 뜻이었고, 이제 이 타입이 정본이다.
//!
//! 읽기만이 아니라 쓰기와 휴지통도 같은 정책을 따른다. AI 호출자는 기록 층 노트를 고치지 못하고(`commands::subnote::check_writable`, 이슈 #74), 휴지통에서는 보관된 노트의 층을 읽어 가린다(`vault::trash::archived_tier`, 이슈 #80). 앱은 모두 한다.

use super::actor::Caller;

/// 노트의 층
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tier {
    /// 읽기 층(기본)
    #[default]
    Reading,
    /// 기록 층: 앱에서만 보인다
    Record,
}

impl Tier {
    /// 명령 인자의 오류 문구에 찍는 목록 글: 「쓸 수 있는 것은 [reading, record] 뿐이다.」
    pub const NAMES_TEXT: &'static str = "reading, record";

    /// 파일과 응답에 적는 글자
    pub const fn as_str(self) -> &'static str {
        match self {
            Tier::Reading => "reading",
            Tier::Record => "record",
        }
    }

    pub const fn is_record(self) -> bool {
        matches!(self, Tier::Record)
    }

    pub const fn is_reading(self) -> bool {
        matches!(self, Tier::Reading)
    }

    /// 파일 머리의 `tier` 칸 값을 읽는다. 앞뒤 공백을 뗀 값이 `reading`이거나 비었으면 읽기 층이고, 그 밖의 값은 모두 기록 층이다.
    /// (칸이 없거나 후속 노트이면 호출하는 쪽이 이 함수를 부르지 않고 기본값을 쓴다.)
    pub fn from_front(trimmed: &str) -> Tier {
        if trimmed != "reading" && !trimmed.is_empty() {
            Tier::Record
        } else {
            Tier::Reading
        }
    }

    /// 명령 인자(`tier=`)를 대소문자 무시로 읽는다. 둘 중 하나가 아니면 `None`이다.
    pub fn parse_ignore_ascii_case(raw: &str) -> Option<Tier> {
        if raw.eq_ignore_ascii_case("reading") {
            Some(Tier::Reading)
        } else if raw.eq_ignore_ascii_case("record") {
            Some(Tier::Record)
        } else {
            None
        }
    }

    /// 노트 가시성 정책: 기록 층 노트는 AI에게 보이지 않는다. 앱에는 모두 보인다.
    pub const fn visible_to(self, caller: Caller) -> bool {
        !(self.is_record() && matches!(caller, Caller::Ai))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_values_normalize_to_two_tiers() {
        assert_eq!(Tier::from_front("reading"), Tier::Reading);
        assert_eq!(Tier::from_front(""), Tier::Reading);
        assert_eq!(Tier::from_front("record"), Tier::Record);
        // `reading`이 아닌 비어 있지 않은 값은 모두 기록 층이다(Java 엔진과 같다)
        assert_eq!(Tier::from_front("Reading"), Tier::Record);
        assert_eq!(Tier::from_front("anything"), Tier::Record);
    }

    #[test]
    fn only_the_app_sees_record_notes() {
        assert!(Tier::Reading.visible_to(Caller::App));
        assert!(Tier::Reading.visible_to(Caller::Ai));
        assert!(Tier::Record.visible_to(Caller::App));
        assert!(!Tier::Record.visible_to(Caller::Ai));
    }

    #[test]
    fn argument_reading_ignores_ascii_case() {
        assert_eq!(Tier::parse_ignore_ascii_case("RECORD"), Some(Tier::Record));
        assert_eq!(
            Tier::parse_ignore_ascii_case("Reading"),
            Some(Tier::Reading)
        );
        assert_eq!(Tier::parse_ignore_ascii_case("x"), None);
        assert_eq!(Tier::Record.as_str(), "record");
    }
}
