//! 후속 노트(메인 노트 아래에 달리는 노트) 쓰기의 규칙: 만들 때의 인자 검사와 파일 글, 고칠 때의 제목 정하기.
//! 파일도 vault도 읽고 쓰지 않는다. 부모 사슬 풀기와 폴더 옮기기는 vault를 읽어야 해서 명령에 남았다.

use super::front::{NoteFront, check_size};
use super::heading;
use super::meta::Format;
use super::placement::created_text;
use crate::error::EngineError;
use crate::kernel::doc_type::DocType;
use crate::kernel::tier::Tier;
use crate::kernel::time::LocalClock;
use crate::text::is_blank;

/// 검사를 마친 새 후속 노트의 재료
#[derive(Debug, Clone)]
pub struct NewSub {
    pub title: String,
    pub body: String,
    pub doc_type: &'static str,
}

impl NewSub {
    /// `title`·`body`는 꼭 있어야 하고, `type`은 안 주면 `free`다. 제목이 비면 거절한다.
    /// 오류가 나오는 차례: 제목 없음 → 본문 없음 → 모르는 유형 → 제목 비었음.
    pub fn parse(
        title: Option<&str>,
        body: Option<&str>,
        doc_type: Option<&str>,
        usage: &str,
    ) -> Result<NewSub, EngineError> {
        let title = title
            .ok_or_else(|| EngineError::invalid_argument(format!("title 이 필요하다: {usage}")))?;
        let body = body
            .ok_or_else(|| EngineError::invalid_argument(format!("body 이 필요하다: {usage}")))?;
        let raw_type = doc_type.unwrap_or("free");
        let doc_type = DocType::parse_ignore_ascii_case(raw_type)
            .map(DocType::as_str)
            .ok_or_else(|| {
                EngineError::invalid_argument(format!(
                    "모르는 유형 이다: {raw_type}. 쓸 수 있는 것은 [{}] 뿐이다.",
                    DocType::NAMES_TEXT
                ))
            })?;
        if is_blank(title) {
            return Err(EngineError::invalid_argument("하위 노트 제목이 비었다"));
        }
        Ok(NewSub {
            title: title.to_string(),
            body: body.to_string(),
            doc_type,
        })
    }

    /// 지금(`now`, 한국 시각)에 만든 후속 노트의 `created` 글
    pub fn created(&self, now: &LocalClock) -> String {
        created_text(&now.date, now.hour, now.minute)
    }

    /// 파일 글 전체(머리 + 본문). 후속 노트는 읽기 층이고 세션·태그·연결이 없다. 저장 상한을 넘으면 `rel`을 붙여 거절한다.
    pub fn render(
        &self,
        sub_id: &str,
        agenda: &str,
        created: &str,
        at: &str,
        rel: &str,
    ) -> Result<String, EngineError> {
        let front = NoteFront {
            id: sub_id,
            agenda,
            doc_type: self.doc_type,
            title: &self.title,
            created,
            created_at: at,
            session_id: None,
            session_agent: None,
            tier: Tier::Reading,
            decision_confirmation: None,
            edited_by: None,
            edited_at: at,
            edited_session_id: None,
            edited_session_agent: None,
            tags: &[],
            favorite: false,
            sub_agenda_ids: &[],
            favorite_category: None,
            template: None,
            evaluation: None,
            links: &[],
            extra: "",
        }
        .render();
        let text = format!("---\n{front}---\n{}", self.body);
        check_size(&text, rel)?;
        Ok(text)
    }
}

/// 고칠 때 준 제목이 비었으면 거절한다(`sub_id`를 붙인다).
pub fn check_given_title(given: Option<&str>, sub_id: &str) -> Result<(), EngineError> {
    if given.is_some_and(is_blank) {
        return Err(EngineError::invalid_argument(format!(
            "하위 노트 제목이 비었다: {sub_id}"
        )));
    }
    Ok(())
}

/// 고친 뒤의 제목. 제목을 줬으면 그것, 아니면 본문을 새로 줬을 때(JSON 본문 노트는 제외) 본문의 첫 헤딩, 그것도 없으면 지금 제목이다.
pub fn updated_title(
    given_title: Option<&str>,
    given_body: Option<&str>,
    format: Format,
    current: &str,
) -> String {
    given_title
        .map(str::to_string)
        .or_else(|| {
            given_body
                .filter(|_| format != Format::Json)
                .and_then(heading::extract)
        })
        .unwrap_or_else(|| current.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::time::CivilDate;

    const USAGE: &str = "U";

    fn err(title: Option<&str>, body: Option<&str>, ty: Option<&str>) -> String {
        NewSub::parse(title, body, ty, USAGE).err().unwrap().message
    }

    #[test]
    fn new_sub_arguments_are_checked_in_order() {
        assert_eq!(err(None, None, Some("zzz")), "title 이 필요하다: U");
        assert_eq!(err(Some(" "), None, Some("zzz")), "body 이 필요하다: U");
        assert!(err(Some(" "), Some("b"), Some("zzz")).starts_with("모르는 유형 이다: zzz"));
        assert_eq!(err(Some(" "), Some("b"), None), "하위 노트 제목이 비었다");
        assert_eq!(
            NewSub::parse(Some("t"), Some("b"), Some("PLAN"), USAGE)
                .unwrap()
                .doc_type,
            "plan"
        );
        // 유형을 안 주면 free
        assert_eq!(
            NewSub::parse(Some("t"), Some("b"), None, USAGE)
                .unwrap()
                .doc_type,
            "free"
        );
    }

    #[test]
    fn a_new_sub_is_a_reading_note_with_the_korean_clock() {
        let sub = NewSub::parse(Some("제목"), Some("본문\n"), None, USAGE).unwrap();
        let now = LocalClock {
            date: CivilDate {
                year: 2026,
                month: 11,
                day: 15,
            },
            hour: 8,
            minute: 5,
            second: 59,
            offset_seconds: 32_400,
        };
        let created = sub.created(&now);
        assert_eq!(created, "2026-11-15T08:05:00+09:00");
        let text = sub
            .render(
                "nd0001-sub00001",
                "demo",
                &created,
                "2026-11-15T00:00:00.000Z",
                "notes/x/sub/y",
            )
            .unwrap();
        assert!(text.starts_with("---\nid: nd0001-sub00001\n"), "{text}");
        assert!(text.ends_with("---\n본문\n"));
        assert!(!text.contains("sessionId") && !text.contains("tags"));
    }

    #[test]
    fn a_new_sub_over_the_size_limit_is_refused_with_the_path() {
        let huge = "가".repeat(1_000_001);
        let sub = NewSub::parse(Some("t"), Some(&huge), None, USAGE).unwrap();
        let e = sub
            .render(
                "x",
                "demo",
                "2026-11-15T08:05:00+09:00",
                "2026-11-15T00:00:00.000Z",
                "notes/x/sub/y",
            )
            .err()
            .unwrap();
        assert_eq!(
            e.message,
            "노트가 저장 상한을 넘어 저장하지 않았다: notes/x/sub/y"
        );
    }

    #[test]
    fn the_title_follows_the_given_title_then_the_first_heading_then_stays() {
        let t = |gt, gb, f| updated_title(gt, gb, f, "옛 제목");
        assert_eq!(t(Some("새"), Some("# 헤딩\n"), Format::Markdown), "새");
        assert_eq!(t(None, Some("# 헤딩\n본문"), Format::Markdown), "헤딩");
        // JSON 본문 노트는 헤딩을 따르지 않는다
        assert_eq!(t(None, Some("# 헤딩\n"), Format::Json), "옛 제목");
        assert_eq!(t(None, Some("헤딩 없음"), Format::Markdown), "옛 제목");
        assert_eq!(t(None, None, Format::Markdown), "옛 제목");
        assert_eq!(
            check_given_title(Some(" "), "s1").err().unwrap().message,
            "하위 노트 제목이 비었다: s1"
        );
        assert!(check_given_title(None, "s1").is_ok());
    }
}
