//! `create_doc`의 규칙: 새 노트를 만들 때 지켜야 할 조건과 날짜·시각 배정, 파일 글 계산. 파일도 vault도 읽고 쓰지 않는다.
//!
//! 두 걸음이다. 아젠다를 찾거나 만드는 일(vault를 읽고 쓴다)이 그 사이에 끼어 있어서 따로 부른다.
//! 1. [`ParsedNewDoc::parse`]: 인자 검사. 오류가 나오는 차례를 그대로 지킨다(본문 형식 → 아젠다 → 유형 → 제목 → 본문 → 날짜 → 시각 → 연결 → 태그 → 층 → 제목 비었나 → 아젠다 이름 비었나).
//! 2. [`ParsedNewDoc::slot`], [`ParsedNewDoc::render`]: 날짜와 시각을 배정하고 파일 글 전체를 짓는다.

use std::collections::HashSet;

use serde_json::Value as Json;

use super::args::{parse_links, parse_tags};
use super::front::NoteFront;
use super::meta::{Link, TEMPLATE_ID, agent_for_file};
use super::placement::{created_text, parse_date_arg, parse_time_arg};
use crate::error::EngineError;
use crate::kernel::doc_type::DocType;
use crate::kernel::tier::Tier;
use crate::kernel::time::{CivilDate, LocalClock};
use crate::text::{is_blank, trim};

pub const CREATE_USAGE: &str = "create-doc agenda=<slug 또는 이름> type=<유형> title=<제목> body=<본문> [date=<YYYY-MM-DD>] [time=<HH:MM>] [format=markdown] [links=<JSON 배열>] [tags=<JSON 배열>] [template=<템플릿 id>] [tier=reading|record]";

/// `create_doc`에 받은 값(검사 전)
pub struct NewDocInput<'a> {
    pub agenda: Option<&'a str>,
    pub doc_type: Option<&'a str>,
    pub title: Option<&'a str>,
    pub body: Option<&'a str>,
    pub date: Option<&'a str>,
    pub time: Option<&'a str>,
    pub format: Option<&'a str>,
    pub links: Option<&'a Json>,
    pub tags: Option<&'a Json>,
    pub template: Option<&'a str>,
    pub tier: Option<&'a str>,
    /// AI 도구 호출인가(AI는 노트 층을 정하지 못한다)
    pub tool: bool,
}

/// 검사를 마친 새 노트의 재료
#[derive(Debug, Clone)]
pub struct ParsedNewDoc {
    pub sub_agenda_ids: Vec<String>,
    /// 다듬은 아젠다 slug 또는 이름(비지 않았다)
    pub agenda_key: String,
    pub doc_type: &'static str,
    pub title: String,
    pub body: String,
    date: Option<CivilDate>,
    time: Option<(u32, u32)>,
    links: Vec<Link>,
    tags: Vec<String>,
    tier: Tier,
    template: Option<String>,
    tool: bool,
}

impl ParsedNewDoc {
    pub fn parse(input: NewDocInput) -> Result<ParsedNewDoc, EngineError> {
        if let Some(format) = input.format
            && format != "markdown"
        {
            return Err(EngineError::invalid_argument(format!(
                "모르는 본문 포맷 이다: {format}. 쓸 수 있는 것은 [markdown] 뿐이다."
            )));
        }
        let raw_agenda = input.agenda.ok_or_else(|| {
            EngineError::invalid_argument(format!("agenda 이 필요하다: {CREATE_USAGE}"))
        })?;
        let raw_type = input.doc_type.ok_or_else(|| {
            EngineError::invalid_argument(format!("type 이 필요하다: {CREATE_USAGE}"))
        })?;
        let doc_type = DocType::parse_ignore_ascii_case(raw_type)
            .map(DocType::as_str)
            .ok_or_else(|| {
                EngineError::invalid_argument(format!(
                    "모르는 유형 이다: {raw_type}. 쓸 수 있는 것은 [{}] 뿐이다.",
                    DocType::NAMES_TEXT
                ))
            })?;
        let raw_title = input.title.ok_or_else(|| {
            EngineError::invalid_argument(format!("title 이 필요하다: {CREATE_USAGE}"))
        })?;
        let raw_body = input.body.ok_or_else(|| {
            EngineError::invalid_argument(format!("body 이 필요하다: {CREATE_USAGE}"))
        })?;
        let date = input.date.map(parse_date_arg).transpose()?;
        let time = input.time.map(parse_time_arg).transpose()?;
        let links = match input.links {
            Some(raw) => parse_links(raw)?,
            None => Vec::new(),
        };
        let tags = match input.tags {
            Some(raw) => parse_tags(raw)?,
            None => Vec::new(),
        };
        // 노트 층은 명령 표면만 받는다
        let mut tier = Tier::Reading;
        if !input.tool
            && let Some(raw) = input.tier
        {
            tier = Tier::parse_ignore_ascii_case(raw).ok_or_else(|| {
                EngineError::invalid_argument(format!(
                    "모르는 노트 층 이다: {raw}. 쓸 수 있는 것은 [{}] 뿐이다.",
                    Tier::NAMES_TEXT
                ))
            })?;
        }
        if is_blank(raw_title) {
            return Err(EngineError::invalid_argument("노트 제목이 비었다"));
        }
        let template = input
            .template
            .map(trim)
            .filter(|t| TEMPLATE_ID.is_match(t))
            .map(str::to_string);
        let agenda_key = trim(raw_agenda);
        if agenda_key.is_empty() {
            return Err(EngineError::invalid_argument("아젠다 이름이 비었다"));
        }
        Ok(ParsedNewDoc {
            sub_agenda_ids: Vec::new(),
            agenda_key: agenda_key.to_string(),
            doc_type,
            title: raw_title.to_string(),
            body: raw_body.to_string(),
            date,
            time,
            links,
            tags,
            tier,
            template,
            tool: input.tool,
        })
    }

    /// 날짜와 시각을 배정한다. 안 준 날짜는 오늘(`now`), 안 준 시각은 지금 시각에서 시작해 그날 겹치지 않는 5분 칸이다.
    /// `reading_created`는 이미 있는 읽기 층 메인 노트들의 `created` 글이다(기록 층은 시각을 차지하지 않는다).
    /// 돌려주는 글은 `created`(`yyyy-MM-ddTHH:mm:00+09:00`)다.
    pub fn slot<'a>(
        &self,
        now: &LocalClock,
        reading_created: impl Iterator<Item = &'a str>,
    ) -> (CivilDate, u32, u32, String) {
        let date = self.date.unwrap_or(now.date);
        let (hour, minute) = match self.time {
            Some(time) => time,
            None => schedule_time(&date, reading_created, now.hour, now.minute),
        };
        let created = created_text(&date, hour, minute);
        (date, hour, minute, created)
    }

    /// 파일 글 전체(머리 + 본문). AI 도구가 만들었으면 세션과 에이전트가 머리에 남는다.
    /// `session`·`session_agent`는 요청 맥락의 값 그대로이고, 도구 호출이 아니면 쓰지 않는다.
    pub fn render(
        &self,
        note_id: &str,
        agenda_slug: &str,
        created: &str,
        at: &str,
        session: Option<&str>,
        session_agent: Option<&str>,
    ) -> String {
        let session_id = session.filter(|_| self.tool);
        let session_agent = session_id.and(session_agent.and_then(agent_for_file));
        let front = NoteFront {
            id: note_id,
            agenda: agenda_slug,
            doc_type: self.doc_type,
            title: &self.title,
            created,
            created_at: at,
            session_id,
            session_agent: session_agent.as_deref(),
            tier: self.tier,
            decision_confirmation: None,
            edited_by: None,
            edited_at: at,
            edited_session_id: None,
            edited_session_agent: None,
            tags: &self.tags,
            sub_agenda_ids: &self.sub_agenda_ids,
            favorite: false,
            favorite_category: None,
            template: self.template.as_deref(),
            evaluation: None,
            links: &self.links,
            extra: "",
        }
        .render();
        format!("---\n{front}---\n{}", self.body)
    }
}

/// 그날의 겹치지 않는 시각. 지금 시각(분)에서 시작해, 같은 날 읽기 층 노트가 이미 쓰는 시각이면 23:55 앞까지 5분씩 미룬다.
pub fn schedule_time<'a>(
    date: &CivilDate,
    reading_created: impl Iterator<Item = &'a str>,
    hour: u32,
    minute: u32,
) -> (u32, u32) {
    let day = date.iso();
    let occupied: HashSet<&str> = reading_created
        .filter_map(|created| {
            (created.get(0..10) == Some(day.as_str()))
                .then(|| created.get(11..16))
                .flatten()
        })
        .collect();
    let (mut h, mut m) = (hour, minute);
    while occupied.contains(format!("{h:02}:{m:02}").as_str()) && (h, m) < (23, 55) {
        m += 5;
        if m >= 60 {
            m -= 60;
            h = (h + 1) % 24;
        }
    }
    (h, m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn input() -> NewDocInput<'static> {
        NewDocInput {
            agenda: Some("demo"),
            doc_type: Some("PLAN"),
            title: Some("제목"),
            body: Some("# 제목\n"),
            date: None,
            time: None,
            format: None,
            links: None,
            tags: None,
            template: None,
            tier: None,
            tool: false,
        }
    }

    fn err(i: NewDocInput) -> String {
        ParsedNewDoc::parse(i).err().unwrap().message
    }

    fn now() -> LocalClock {
        LocalClock {
            date: CivilDate {
                year: 2026,
                month: 11,
                day: 15,
            },
            hour: 8,
            minute: 46,
            second: 0,
            offset_seconds: 32_400,
        }
    }

    #[test]
    fn required_arguments_are_checked_in_order() {
        let mut i = input();
        i.format = Some("html");
        assert!(err(i).starts_with("모르는 본문 포맷 이다: html"));
        let mut i = input();
        i.agenda = None;
        i.doc_type = None;
        assert!(err(i).starts_with("agenda 이 필요하다: create-doc "));
        let mut i = input();
        i.doc_type = Some("zzz");
        i.title = None;
        assert!(err(i).starts_with("모르는 유형 이다: zzz"));
        let mut i = input();
        i.title = None;
        i.body = None;
        assert!(err(i).starts_with("title 이 필요하다"));
        let mut i = input();
        i.body = None;
        assert!(err(i).starts_with("body 이 필요하다"));
        // 날짜가 시각보다, 시각이 연결보다, 연결이 태그보다 먼저 선다
        let bad_links = json!("not json");
        let mut i = input();
        i.date = Some("x");
        i.time = Some("99:99");
        i.links = Some(&bad_links);
        assert!(err(i).starts_with("date 은 YYYY-MM-DD"));
        let mut i = input();
        i.time = Some("99:99");
        i.links = Some(&bad_links);
        assert!(err(i).starts_with("time 은 HH:MM"));
        let bad_tags = json!(5);
        let mut i = input();
        i.links = Some(&bad_links);
        i.tags = Some(&bad_tags);
        assert!(err(i).starts_with("links 은 JSON 배열이어야 한다"));
        // 제목이 비었다는 오류는 층 검사 뒤, 아젠다 이름 검사 앞이다
        let mut i = input();
        i.title = Some("  ");
        i.agenda = Some(" ");
        assert_eq!(err(i), "노트 제목이 비었다");
        let mut i = input();
        i.agenda = Some("  ");
        assert_eq!(err(i), "아젠다 이름이 비었다");
    }

    #[test]
    fn only_the_app_can_choose_the_tier() {
        let tier = |tool: bool, raw: &'static str| {
            let mut i = input();
            i.tool = tool;
            i.tier = Some(raw);
            ParsedNewDoc::parse(i).map(|p| p.tier)
        };
        assert_eq!(tier(false, "RECORD").unwrap(), Tier::Record);
        assert!(tier(false, "x").is_err());
        // AI 도구가 보낸 층은 검사도 하지 않고 읽기 층이다
        assert_eq!(tier(true, "record").unwrap(), Tier::Reading);
        assert_eq!(tier(true, "x").unwrap(), Tier::Reading);
    }

    #[test]
    fn time_is_pushed_in_five_minute_steps_past_taken_slots() {
        let parsed = ParsedNewDoc::parse(input()).unwrap();
        // 그날 08:46이 이미 쓰이면 08:51
        let taken = ["2026-11-15T08:46:00+09:00", "2026-11-14T08:51:00+09:00"];
        let (date, hour, minute, created) = parsed.slot(&now(), taken.into_iter());
        assert_eq!((date.iso(), hour, minute), ("2026-11-15".into(), 8, 51));
        assert_eq!(created, "2026-11-15T08:51:00+09:00");
        // 준 시각은 겹쳐도 그대로다
        let mut i = input();
        i.time = Some("08:46");
        let (_, h, m, _) = ParsedNewDoc::parse(i)
            .unwrap()
            .slot(&now(), taken.into_iter());
        assert_eq!((h, m), (8, 46));
        // 23:55에서는 더 밀지 않는다
        let (h, m) = schedule_time(
            &now().date,
            ["2026-11-15T23:55:00+09:00"].into_iter(),
            23,
            55,
        );
        assert_eq!((h, m), (23, 55));
    }

    #[test]
    fn rendering_records_the_session_only_for_ai_tools() {
        let mut i = input();
        i.tool = true;
        let ai = ParsedNewDoc::parse(i).unwrap();
        let text = ai.render(
            "ndzz0001-0000aaaa",
            "demo",
            "2026-11-15T08:46:00+09:00",
            "2026-11-15T00:00:00.000Z",
            Some("s-1"),
            Some("Codex"),
        );
        assert!(text.starts_with("---\nid: ndzz0001-0000aaaa\n"), "{text}");
        assert!(text.contains("sessionId: s-1\n") && text.contains("sessionAgent: codex\n"));
        assert!(text.ends_with("---\n# 제목\n"));
        let app = ParsedNewDoc::parse(input()).unwrap();
        let text = app.render(
            "ndzz0001-0000aaaa",
            "demo",
            "2026-11-15T08:46:00+09:00",
            "2026-11-15T00:00:00.000Z",
            Some("s-1"),
            Some("Codex"),
        );
        assert!(!text.contains("sessionId"), "{text}");
        assert!(text.contains("type: plan\n"));
    }
}
