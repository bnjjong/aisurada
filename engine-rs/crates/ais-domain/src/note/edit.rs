//! `update_doc`의 규칙: 노트 한 건의 제목·본문·태그·템플릿·날짜·시각·아젠다를 고칠 때 지켜야 할 조건과 새 글·새 자리 계산.
//! 파일을 읽고 쓰지 않는다. 읽은 파일의 내용(`raw` 머리 맵과 옛 본문)과 지금 시각, 편집자를 **인자로 받는다**.
//!
//! 세 걸음이다. 각 걸음은 파일 읽기나 색인 조회 사이에 끼어 있어서(오류가 나오는 차례를 지키려고) 따로 부른다.
//! 1. [`ParsedEdit::parse`]: 인자 검사. AI 도구 호출은 날짜와 시각을 받지 않고, 제목과 옮길 아젠다는 비면 안 된다.
//!    아젠다는 AI 호출도 받는다. 그 아젠다가 있는지는 색인을 아는 호출한 쪽이 보고 [`Slot::move_to`]로 넘긴다.
//! 2. [`ParsedEdit::slot`]: 고칠 수 있는지와 새 `created`·날짜·시각. `created`를 못 읽으면, JSON 노트에 본문을 주면 거절한다.
//! 3. [`Slot::render`]: 읽은 파일의 머리와 옛 본문으로 새 파일 글 전체를 짓는다(편집 도장 판정 포함).

use super::front::{Draft, Editor, judge};
use super::meta::{Format, MetaData, TEMPLATE_ID, get};
use super::placement::{parse_date_arg, parse_time_arg, read_created};
use crate::error::EngineError;
use crate::frontmatter::Value;
use crate::kernel::time::CivilDate;
use crate::text::{cut_utf16, is_blank, trim};

/// 검사를 마친 `update_doc` 인자
#[derive(Debug, Clone)]
pub struct ParsedEdit {
    title: Option<String>,
    body: Option<String>,
    tags: Option<Vec<String>>,
    template: Option<String>,
    date: Option<CivilDate>,
    time: Option<(u32, u32)>,
    agenda: Option<String>,
    token: Option<String>,
}

/// `update_doc`에 받은 값(검사 전)
pub struct EditInput<'a> {
    pub note_id: &'a str,
    pub title: Option<&'a str>,
    pub body: Option<&'a str>,
    /// 이미 읽어 다듬은 태그 목록(전체 교체). 안 주면 그대로 둔다.
    pub tags: Option<Vec<String>>,
    pub template: Option<&'a str>,
    pub date: Option<&'a str>,
    pub time: Option<&'a str>,
    /// 옮길 아젠다(slug나 이름). 이미 있는 아젠다만 받으며, 이 규칙은 아젠다를 새로 만들지 않는다.
    pub agenda: Option<&'a str>,
    /// 직전에 읽은 확인 값(`f1`)
    pub f1: Option<&'a str>,
    /// AI 도구 호출인가
    pub tool: bool,
}

impl ParsedEdit {
    /// 날짜(`date`)와 시각(`time`) 인자를 읽고(AI 도구 호출은 무시한다), 제목과 옮길 아젠다가 비었는지 보고, 확인 값을 다듬는다.
    /// 오류가 나오는 차례: 날짜 → 시각 → 제목 → 아젠다.
    pub fn parse(input: EditInput) -> Result<ParsedEdit, EngineError> {
        let date = input
            .date
            .filter(|_| !input.tool)
            .map(parse_date_arg)
            .transpose()?;
        let time = input
            .time
            .filter(|_| !input.tool)
            .map(parse_time_arg)
            .transpose()?;
        if input.title.is_some_and(is_blank) {
            return Err(EngineError::invalid_argument(format!(
                "노트 제목이 비었다: {}",
                input.note_id
            )));
        }
        if input.agenda.is_some_and(is_blank) {
            return Err(EngineError::invalid_argument(format!(
                "옮길 아젠다가 비었다: {}",
                input.note_id
            )));
        }
        Ok(ParsedEdit {
            title: input.title.map(str::to_string),
            body: input.body.map(str::to_string),
            tags: input.tags,
            template: input.template.map(|t| trim(t).to_string()),
            date,
            time,
            agenda: input.agenda.map(|a| trim(a).to_string()),
            token: input
                .f1
                .map(trim)
                .filter(|t| !t.is_empty())
                .map(str::to_string),
        })
    }

    /// 고치기 전에 읽은 확인 값(비었으면 `None`)
    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }

    /// 새로 준 본문
    pub fn given_body(&self) -> Option<&str> {
        self.body.as_deref()
    }

    /// 옮길 아젠다로 준 값(다듬은 slug나 이름). 안 줬으면 `None`
    pub fn agenda_key(&self) -> Option<&str> {
        self.agenda.as_deref()
    }

    /// 준 칸만 갈아 끼운 다음 값과 새 `created`·날짜·시각. 못 고치는 값은 색인의 값이 따라온다.
    /// 날짜와 시각은 준 쪽만 갈아 끼우고, 오프셋은 노트의 지금 값을 지킨다.
    pub fn slot(
        &self,
        meta: &MetaData,
        format: Format,
        note_id: &str,
    ) -> Result<Slot, EngineError> {
        let title = self.title.clone().unwrap_or_else(|| meta.title.clone());
        let tags = self.tags.clone().unwrap_or_else(|| meta.tags.clone());
        let template = self
            .template
            .as_deref()
            .filter(|t| TEMPLATE_ID.is_match(t))
            .map(str::to_string)
            .or_else(|| meta.template.clone());

        let current = read_created(&meta.created).ok_or_else(|| {
            EngineError::internal(format!("created 를 읽지 못해 고치지 않았다: {note_id}"))
        })?;
        let (created, date, hour, minute) = if self.date.is_none() && self.time.is_none() {
            (
                meta.created.clone(),
                current.date,
                current.hour,
                current.minute,
            )
        } else {
            let date = self.date.unwrap_or(current.date);
            let (hour, minute) = self.time.unwrap_or((current.hour, current.minute));
            (
                format!(
                    "{}T{hour:02}:{minute:02}:00{}",
                    date.era_text(),
                    current.offset
                ),
                date,
                hour,
                minute,
            )
        };

        // json 노트에 본문을 주면 파일을 읽기 전에 선다
        if format == Format::Json && self.body.is_some() {
            return Err(EngineError::internal(format!(
                "Lexical JSON 노트의 본문은 고칠 수 없다(마크다운을 editorState 로 바꾸는 일이라 사이드카에 없다): {note_id}"
            )));
        }
        Ok(Slot {
            title,
            tags,
            sub_agenda_ids: meta.sub_agenda_ids.clone(),
            template,
            agenda: None,
            created,
            date,
            hour,
            minute,
        })
    }
}

/// 새 제목·태그·템플릿·아젠다와 새 `created`·날짜·시각(자리를 정하는 값)
#[derive(Debug, Clone)]
pub struct Slot {
    pub title: String,
    tags: Vec<String>,
    pub sub_agenda_ids: Vec<String>,
    template: Option<String>,
    /// 옮길 아젠다의 slug. `None`이면 파일의 아젠다를 그대로 쓴다
    agenda: Option<String>,
    pub created: String,
    pub date: CivilDate,
    pub hour: u32,
    pub minute: u32,
}

/// 새 파일 글과 그것을 짓는 데 쓴 아젠다와 본문
pub struct Rendered {
    /// 쓸 파일 글 전체(머리 + 본문)
    pub text: String,
    /// 새 자리의 아젠다 slug. 옮기라고 했으면 그 아젠다, 아니면 파일의 `agenda`가 빈 글이 아닌 문자열일 때 그 글자 그대로다
    pub agenda: String,
    /// 쓸 본문(새로 줬으면 그것, 아니면 옛 본문)
    pub body: String,
}

impl Slot {
    /// 노트를 `slug` 아젠다로 옮기게 한다. `slug`는 호출한 쪽이 이미 있는 아젠다에서 찾은 값이다.
    pub fn move_to(&mut self, slug: &str) {
        self.agenda = Some(slug.to_string());
    }

    /// 읽은 파일의 머리(`raw`)와 옛 본문으로 새 파일 글을 짓는다. 편집 도장은 `judge`가 정하고, 시각과 편집자는 인자다.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &self,
        format: Format,
        meta: &MetaData,
        old_rel: &str,
        raw: &[(Value, Value)],
        old_body: &str,
        given_body: Option<&str>,
        editor: &Editor,
        now_millis: i64,
    ) -> Result<Rendered, EngineError> {
        // 새 자리의 아젠다 slug. 옮기라고 했으면 그 아젠다, 아니면 파일의 agenda가 빈 글이 아닌 문자열일 때 그 글자 그대로다
        let agenda = match (&self.agenda, get(raw, "agenda")) {
            (Some(target), _) => target.clone(),
            (None, Some(Value::Str(s))) if !s.is_empty() => s.clone(),
            _ => {
                let parts: Vec<&str> = old_rel.split('/').collect();
                if parts.len() >= 2 {
                    parts[1].to_string()
                } else {
                    meta.agenda_slug.clone()
                }
            }
        };
        let body = given_body.unwrap_or(old_body).to_string();
        let session_id = match get(raw, "sessionId") {
            Some(Value::Str(s)) if !trim(s).is_empty() => Some(cut_utf16(trim(s), 128)),
            _ => None,
        };
        let session_agent = session_id.as_ref().and(meta.session_agent.as_deref());
        let text = judge(
            format,
            &Draft {
                meta,
                agenda: &agenda,
                title: &self.title,
                created: &self.created,
                session_id: session_id.as_deref(),
                session_agent,
                tier: meta.tier,
                tags: &self.tags,
                sub_agenda_ids: &self.sub_agenda_ids,
                favorite: meta.favorite,
                favorite_category: meta.favorite_category.as_deref(),
                template: self.template.as_deref(),
                evaluation: meta.evaluation.as_ref(),
                links: &meta.links,
            },
            &body,
            old_body,
            raw,
            editor,
            now_millis,
        )?;
        Ok(Rendered { text, agenda, body })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::tier::Tier;

    const NOW: i64 = 1_794_700_000_000; // 2026-11-14T23:46:40Z = 한국 2026-11-15 08:46

    fn meta() -> MetaData {
        MetaData {
            id: "ndzz0001-0000aaaa".into(),
            agenda_slug: "demo".into(),
            doc_type: "free".into(),
            title: "옛 제목".into(),
            created: "2026-11-12T09:00:00+09:00".into(),
            date: "2026-11-12".into(),
            time: "09:00".into(),
            session_id: None,
            session_agent: None,
            tier: Tier::Reading,
            decision_confirmation: None,
            edited_by: None,
            edited_at: None,
            edited_session_id: None,
            edited_session_agent: None,
            tags: vec!["a".into()],
            sub_agenda_ids: Vec::new(),
            links: Vec::new(),
            favorite: false,
            favorite_category: None,
            template: None,
            evaluation: None,
        }
    }

    fn input() -> EditInput<'static> {
        EditInput {
            note_id: "ndzz0001-0000aaaa",
            title: None,
            body: None,
            tags: None,
            template: None,
            date: None,
            time: None,
            agenda: None,
            f1: None,
            tool: false,
        }
    }

    fn raw(entries: &[(&str, &str)]) -> Vec<(Value, Value)> {
        entries
            .iter()
            .map(|(k, v)| (Value::Str((*k).into()), Value::Str((*v).into())))
            .collect()
    }

    const APP: Editor<'static> = Editor {
        tool: false,
        session: None,
        session_agent: None,
    };

    #[test]
    fn date_time_and_blank_title_are_checked_in_that_order() {
        let err = |i: EditInput| ParsedEdit::parse(i).err().unwrap().message;
        let bad_date = EditInput {
            date: Some("x"),
            time: Some("99:99"),
            title: Some(" "),
            ..input()
        };
        assert_eq!(err(bad_date), "date 은 YYYY-MM-DD 모양이어야 한다: x");
        let bad_time = EditInput {
            time: Some("99:99"),
            title: Some(" "),
            ..input()
        };
        assert_eq!(err(bad_time), "time 은 HH:MM 모양이어야 한다: 99:99");
        let blank = EditInput {
            title: Some(" "),
            ..input()
        };
        assert_eq!(err(blank), "노트 제목이 비었다: ndzz0001-0000aaaa");
        // 제목이 비었으면 아젠다보다 제목 오류가 먼저다
        let blank_both = EditInput {
            title: Some(" "),
            agenda: Some(" "),
            ..input()
        };
        assert_eq!(err(blank_both), "노트 제목이 비었다: ndzz0001-0000aaaa");
        let blank_agenda = EditInput {
            agenda: Some("  "),
            ..input()
        };
        assert_eq!(err(blank_agenda), "옮길 아젠다가 비었다: ndzz0001-0000aaaa");
    }

    #[test]
    fn an_ai_tool_can_move_the_note_to_another_agenda() {
        // 날짜·시각과 달리 아젠다는 AI 호출에서도 받는다(사용자 결정 2026-11-20). 값은 앞뒤 공백을 다듬는다
        let parsed = ParsedEdit::parse(EditInput {
            agenda: Some(" 다른 아젠다 "),
            tool: true,
            ..input()
        })
        .unwrap();
        assert_eq!(parsed.agenda_key(), Some("다른 아젠다"));
        assert_eq!(ParsedEdit::parse(input()).unwrap().agenda_key(), None);
    }

    #[test]
    fn an_ai_tool_cannot_change_the_date_or_time() {
        let parsed = ParsedEdit::parse(EditInput {
            date: Some("2030-01-01"),
            time: Some("23:59"),
            tool: true,
            ..input()
        })
        .unwrap();
        let slot = parsed
            .slot(&meta(), Format::Markdown, "ndzz0001-0000aaaa")
            .unwrap();
        assert_eq!(slot.created, "2026-11-12T09:00:00+09:00");
        assert_eq!(
            (slot.date.iso(), slot.hour, slot.minute),
            ("2026-11-12".into(), 9, 0)
        );
        // 앱은 바꿀 수 있고 오프셋은 노트의 지금 값을 지킨다
        let parsed = ParsedEdit::parse(EditInput {
            date: Some("2030-01-01"),
            time: Some("23:59"),
            ..input()
        })
        .unwrap();
        let slot = parsed
            .slot(&meta(), Format::Markdown, "ndzz0001-0000aaaa")
            .unwrap();
        assert_eq!(slot.created, "2030-01-01T23:59:00+09:00");
    }

    #[test]
    fn the_slot_refuses_a_json_body_and_an_unreadable_created() {
        let with_body = ParsedEdit::parse(EditInput {
            body: Some("x"),
            ..input()
        })
        .unwrap();
        let err = with_body
            .slot(&meta(), Format::Json, "ndzz0001-0000aaaa")
            .err()
            .unwrap();
        assert!(
            err.message
                .starts_with("Lexical JSON 노트의 본문은 고칠 수 없다")
        );
        let mut broken = meta();
        broken.created = "깨짐".into();
        let err = ParsedEdit::parse(input())
            .unwrap()
            .slot(&broken, Format::Markdown, "ndzz0001-0000aaaa")
            .err()
            .unwrap();
        assert!(
            err.message
                .starts_with("created 를 읽지 못해 고치지 않았다")
        );
    }

    #[test]
    fn only_a_valid_template_id_replaces_the_current_one() {
        let slot = |t: &'static str| {
            ParsedEdit::parse(EditInput {
                template: Some(t),
                ..input()
            })
            .unwrap()
            .slot(&meta(), Format::Markdown, "ndzz0001-0000aaaa")
            .unwrap()
        };
        assert_eq!(slot(" my-tpl ").template.as_deref(), Some("my-tpl"));
        assert_eq!(slot("모양 아님!").template, None);
    }

    #[test]
    fn a_changed_note_gets_a_fresh_stamp_from_the_injected_clock_and_editor() {
        let m = meta();
        let parsed = ParsedEdit::parse(EditInput {
            title: Some("새 제목"),
            ..input()
        })
        .unwrap();
        let slot = parsed
            .slot(&m, Format::Markdown, "ndzz0001-0000aaaa")
            .unwrap();
        let old_raw = raw(&[
            ("id", "ndzz0001-0000aaaa"),
            ("agenda", "demo"),
            ("title", "옛 제목"),
        ]);
        let app = slot
            .render(
                Format::Markdown,
                &m,
                "notes/demo/x",
                &old_raw,
                "본문\n",
                None,
                &APP,
                NOW,
            )
            .unwrap();
        assert_eq!(app.agenda, "demo");
        assert_eq!(app.body, "본문\n");
        assert!(app.text.contains("title: 새 제목\n"), "{}", app.text);
        assert!(app.text.contains("editedBy: app\n"));
        assert!(
            app.text.contains("editedAt: '2026-11-15T08:46:00+09:00'\n"),
            "{}",
            app.text
        );
        // 같은 입력을 AI 도구가 고치면 세션과 에이전트가 남는다
        let tool = Editor {
            tool: true,
            session: Some("sess-1"),
            session_agent: Some("Codex"),
        };
        let ai = slot
            .render(
                Format::Markdown,
                &m,
                "notes/demo/x",
                &old_raw,
                "본문\n",
                None,
                &tool,
                NOW,
            )
            .unwrap();
        assert!(ai.text.contains("editedBy: tool\n"));
        assert!(ai.text.contains("editedSessionId: sess-1\n"));
        assert!(ai.text.contains("editedSessionAgent: codex\n"));
        // 시각만 다르면 도장도 다르다(시계는 인자다)
        let later = slot
            .render(
                Format::Markdown,
                &m,
                "notes/demo/x",
                &old_raw,
                "본문\n",
                None,
                &APP,
                NOW + 3_600_000,
            )
            .unwrap();
        assert!(later.text.contains("2026-11-15T09:46:00+09:00"));
    }

    #[test]
    fn the_agenda_comes_from_the_file_or_from_the_path() {
        let m = meta();
        let slot = ParsedEdit::parse(input())
            .unwrap()
            .slot(&m, Format::Markdown, "x")
            .unwrap();
        let from_file = raw(&[("agenda", "other")]);
        let r = slot
            .render(
                Format::Markdown,
                &m,
                "notes/demo/x",
                &from_file,
                "b",
                None,
                &APP,
                NOW,
            )
            .unwrap();
        assert_eq!(r.agenda, "other");
        // 파일의 agenda가 비면 경로의 둘째 조각이다
        let r = slot
            .render(
                Format::Markdown,
                &m,
                "notes/pathagenda/x",
                &raw(&[("agenda", "")]),
                "b",
                None,
                &APP,
                NOW,
            )
            .unwrap();
        assert_eq!(r.agenda, "pathagenda");
    }

    #[test]
    fn moving_overrides_the_agenda_from_the_file_and_the_path() {
        let m = meta();
        let mut slot = ParsedEdit::parse(input())
            .unwrap()
            .slot(&m, Format::Markdown, "x")
            .unwrap();
        slot.move_to("homeserver");
        let r = slot
            .render(
                Format::Markdown,
                &m,
                "notes/demo/x",
                &raw(&[("agenda", "demo")]),
                "b",
                None,
                &APP,
                NOW,
            )
            .unwrap();
        assert_eq!(r.agenda, "homeserver");
        assert!(r.text.contains("\nagenda: homeserver\n"), "{}", r.text);
    }
}
