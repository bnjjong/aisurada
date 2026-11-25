//! 노트 머리(YAML) 만들기와 편집 도장 판정(`judge`). 저장할 때 생성·수정 시각을 함께 쓴다.

use super::meta::{Evaluation, Format, Link, MetaData, agent_for_file};
use crate::error::EngineError;
use crate::frontmatter::dump_string;
use crate::frontmatter::{Value, dump_general_map};
use crate::kernel::tier::Tier;
use crate::kernel::time::{civil_from_days, format_utc_millis};

pub struct NoteFront<'a> {
    pub id: &'a str,
    pub agenda: &'a str,
    pub doc_type: &'a str,
    pub title: &'a str,
    pub created: &'a str,
    pub created_at: &'a str,
    pub session_id: Option<&'a str>,
    pub session_agent: Option<&'a str>,
    pub tier: Tier,
    pub decision_confirmation: Option<&'a str>,
    pub edited_by: Option<&'a str>,
    pub edited_at: &'a str,
    pub edited_session_id: Option<&'a str>,
    pub edited_session_agent: Option<&'a str>,
    pub tags: &'a [String],
    pub sub_agenda_ids: &'a [String],
    pub favorite: bool,
    pub favorite_category: Option<&'a str>,
    pub template: Option<&'a str>,
    pub evaluation: Option<&'a Evaluation>,
    pub links: &'a [Link],
    /// 엔진이 모르는 머리 칸을 [`extra_front`]로 미리 글로 만든 것. 머리 맨 끝에 그대로 붙는다. 없으면 빈 글이다.
    pub extra: &'a str,
}

/// 이 엔진이 머리에 직접 쓰는 칸. 이 밖의 칸은 모르는 칸이라 다시 쓸 때 그대로 둔다.
pub const KNOWN_KEYS: [&str; 20] = [
    "id",
    "agenda",
    "type",
    "title",
    "created",
    "createdAt",
    "sessionId",
    "sessionAgent",
    "decisionConfirmation",
    "tier",
    "editedBy",
    "editedAt",
    "editedSessionId",
    "editedSessionAgent",
    "tags",
    "subAgendaIds",
    "favorite",
    "favoriteCategory",
    "template",
    "evaluation",
];

/// 읽은 머리 `raw`에서 엔진이 모르는 칸만 골라 읽은 차례 그대로 YAML 글로 만든다(`links`도 아는 칸이라 뺀다).
///
/// 새 버전이 더한 칸(옛 프로세스가 노트를 다시 쓰면 지우던 것)이나 사람이 손으로 적어 둔 칸을 다시 쓸 때 잃지 않으려는 것이다.
/// 쓸 수 없는 값(실수, 아주 큰 정수 따위)이 든 칸은 [`judge`]가 먼저 거절하므로 여기서는 건너뛴다.
pub fn extra_front(raw: &[(Value, Value)]) -> String {
    let unknown: Vec<(Value, Value)> = raw
        .iter()
        .filter(|(key, _)| match key {
            Value::Str(name) => !KNOWN_KEYS.contains(&name.as_str()) && name != "links",
            _ => false,
        })
        .cloned()
        .collect();
    if unknown.is_empty() {
        return String::new();
    }
    dump_general_map(&unknown).unwrap_or_default()
}

impl NoteFront<'_> {
    /// 머리 줄들(여닫는 울타리는 뺀다). 마지막 줄도 줄바꿈으로 끝난다.
    pub fn render(&self) -> String {
        self.render_with_times(true, true)
    }

    // 예전 파일과 편집 여부를 비교할 때만 머리에 없던 시각을 뺀다.
    fn render_with_times(&self, has_created_at: bool, has_edited_at: bool) -> String {
        let mut out = String::new();
        let mut line =
            |key: &str, value: &str| out.push_str(&format!("{key}: {}\n", dump_string(value, 1)));
        line("id", self.id);
        line("agenda", self.agenda);
        line("type", &self.doc_type.to_lowercase());
        line("title", self.title);
        line("created", self.created);
        if has_created_at {
            line("createdAt", self.created_at);
        }
        if let Some(session) = self.session_id {
            line("sessionId", session);
            if let Some(agent) = self.session_agent {
                line("sessionAgent", agent);
            }
        }
        if let Some(confirmation) = self.decision_confirmation {
            line("decisionConfirmation", confirmation);
        }
        if self.tier.is_record() {
            out.push_str(&format!("tier: {}\n", Tier::Record.as_str()));
        }
        if let (Some(by), true) = (self.edited_by, has_edited_at) {
            let mut line = |key: &str, value: &str| {
                out.push_str(&format!("{key}: {}\n", dump_string(value, 1)))
            };
            line("editedBy", by);
            line("editedAt", self.edited_at);
            if let Some(session) = self.edited_session_id {
                line("editedSessionId", session);
                if let Some(agent) = self.edited_session_agent {
                    line("editedSessionAgent", agent);
                }
            }
        } else if has_edited_at {
            out.push_str(&format!("editedAt: {}\n", dump_string(self.edited_at, 1)));
        }
        if !self.tags.is_empty() {
            out.push_str("tags:\n");
            for tag in self.tags {
                out.push_str(&format!("  - {}\n", dump_string(tag, 2)));
            }
        }
        if !self.sub_agenda_ids.is_empty() {
            out.push_str("subAgendaIds:\n");
            for id in self.sub_agenda_ids {
                out.push_str(&format!("  - {}\n", dump_string(id, 2)));
            }
        }
        if self.favorite {
            out.push_str("favorite: true\n");
        }
        if let Some(category) = self.favorite_category.filter(|c| !c.is_empty()) {
            out.push_str(&format!("favoriteCategory: {}\n", dump_string(category, 1)));
        }
        if let Some(template) = self.template.filter(|t| !t.is_empty()) {
            out.push_str(&format!("template: {}\n", dump_string(template, 1)));
        }
        if let Some(evaluation) = self.evaluation {
            out.push_str("evaluation:\n");
            out.push_str(&format!("  score: {}\n", evaluation.score));
            out.push_str(&format!("  note: {}\n", dump_string(&evaluation.note, 2)));
            out.push_str(&format!("  at: {}\n", dump_string(&evaluation.at, 2)));
        }
        if !self.links.is_empty() {
            out.push_str("links:\n");
            for link in self.links {
                out.push_str(&format!("  - to: {}\n", dump_string(&link.to, 3)));
                out.push_str(&format!(
                    "    kind: {}\n",
                    dump_string(link.kind.as_str(), 3)
                ));
            }
        }
        out.push_str(self.extra);
        out
    }
}

/// 쓴 쪽. AI 도구(`tool`)가 고쳤으면 편집 도장에 세션과 에이전트가 남고, 앱이 고쳤으면 `app`만 남는다.
pub struct Editor<'a> {
    pub tool: bool,
    pub session: Option<&'a str>,
    pub session_agent: Option<&'a str>,
}

/// 저장 상한(UTF-16 글자)
pub const MAX_FILE_UNITS: usize = 1_000_000;

/// 쓸 파일 글이 저장 상한을 넘으면 `rel`(쓸 경로)을 붙인 오류를 낸다.
pub fn check_size(text: &str, rel: &str) -> Result<(), EngineError> {
    if crate::text::utf16_len(text) > MAX_FILE_UNITS {
        return Err(EngineError::internal(format!(
            "노트가 저장 상한을 넘어 저장하지 않았다: {rel}"
        )));
    }
    Ok(())
}

/// 쓸 노트 머리의 값. 줄이 아닌 값으로 받아 두고, 옛 도장은 `meta`에서 읽는다.
pub struct Draft<'a> {
    /// 지금 색인에 있는 노트(id, 유형, 층과 옛 편집 도장을 여기서 읽는다)
    pub meta: &'a MetaData,
    pub agenda: &'a str,
    pub title: &'a str,
    pub created: &'a str,
    pub session_id: Option<&'a str>,
    pub session_agent: Option<&'a str>,
    /// 머리에 적을 층(`record`일 때만 줄이 생긴다). 후속 노트는 층을 안 적는 값을 준다
    pub tier: Tier,
    pub tags: &'a [String],
    pub sub_agenda_ids: &'a [String],
    pub favorite: bool,
    pub favorite_category: Option<&'a str>,
    pub template: Option<&'a str>,
    pub evaluation: Option<&'a Evaluation>,
    pub links: &'a [Link],
}

/// 편집 도장 판정(1.10)을 하고 쓸 파일 글 전체를 돌려준다.
///
/// A는 옛 도장을 넣어 새 값으로 지은 글(본문은 `body`), B는 파일의 옛 머리 맵을 그대로 다시 쓴 글(본문은 `old_body`)이다.
/// 둘이 같으면 도장을 건드리지 않고 A를 쓴다. 다르면 지금 시각과 쓴 쪽으로 새 도장을 찍은 글을 쓴다.
/// 그래서 본문을 바꾸는 쓰기는 늘 A와 B가 달라 새 도장이 찍힌다.
pub fn judge(
    format: Format,
    draft: &Draft,
    body: &str,
    old_body: &str,
    raw: &[(Value, Value)],
    editor: &Editor,
    now_millis: i64,
) -> Result<String, EngineError> {
    let meta = draft.meta;
    // 엔진이 모르는 머리 칸은 그대로 이어 쓴다. 맨 끝에 붙으므로 모르는 칸이 가운데 있던 노트는 처음 한 번 순서가 바뀌고, 그 쓰기는 편집 도장이 찍힌다
    let extra = extra_front(raw);
    let now = format_utc_millis(now_millis);
    let mut front = NoteFront {
        id: &meta.id,
        agenda: draft.agenda,
        doc_type: &meta.doc_type,
        title: draft.title,
        created: draft.created,
        created_at: meta.created_at.as_deref().unwrap_or(&now),
        session_id: draft.session_id,
        session_agent: draft.session_agent,
        tier: draft.tier,
        decision_confirmation: meta.decision_confirmation.as_deref(),
        edited_by: meta.edited_by.as_deref(),
        edited_at: meta.edited_at.as_deref().unwrap_or(&now),
        edited_session_id: meta.edited_session_id.as_deref(),
        edited_session_agent: meta.edited_session_agent.as_deref(),
        tags: draft.tags,
        sub_agenda_ids: draft.sub_agenda_ids,
        favorite: draft.favorite,
        favorite_category: draft.favorite_category,
        template: draft.template,
        evaluation: draft.evaluation,
        links: draft.links,
        extra: &extra,
    };
    let (open, close) = if format == Format::Html {
        ("<!--aisurada\n", "-->\n")
    } else {
        ("---\n", "---\n")
    };
    let without_missing_times =
        front.render_with_times(meta.created_at.is_some(), meta.edited_at.is_some());
    // 누락을 채운 것만으로 기존 수정 시각이 바뀌지 않게 비교한다.
    let mut original = raw.to_vec();
    original.retain(|(key, _)| {
        !matches!(key, Value::Str(name)
            if (name == "createdAt" && meta.created_at.is_none())
                || (name == "editedAt" && meta.edited_at.is_none()))
    });
    let original = dump_general_map(&original).map_err(|e| EngineError::invalid_argument(e.0))?;
    if without_missing_times != original || body != old_body {
        let tool = editor.tool;
        let confirmation = super::decision::confirmation_after_edit(meta, old_body, body, tool);
        let edited_session = if tool { editor.session } else { None };
        let edited_agent = edited_session.and(editor.session_agent.and_then(agent_for_file));
        let legacy_at;
        let edited_at = if meta.created_at.is_some() {
            &now
        } else {
            legacy_at = kst_minute(now_millis);
            &legacy_at
        };
        front.decision_confirmation = confirmation.as_deref();
        front.edited_by = Some(if tool { "tool" } else { "app" });
        front.edited_at = edited_at;
        front.edited_session_id = edited_session;
        front.edited_session_agent = edited_agent.as_deref();
        return Ok(format!("{open}{}{close}{body}", front.render()));
    }
    Ok(format!("{open}{}{close}{body}", front.render()))
}

/// 한국 시각(+09:00)으로 분까지 적은 글 `yyyy-MM-ddTHH:mm:00+09:00`
pub fn kst_minute(millis: i64) -> String {
    let local = millis + 9 * 3_600_000;
    let (year, month, day) = civil_from_days(local.div_euclid(86_400_000));
    let rest = local.rem_euclid(86_400_000);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:00+09:00",
        rest / 3_600_000,
        rest % 3_600_000 / 60_000
    )
}

#[cfg(test)]
mod tests {
    use super::meta::LinkKind;
    use super::*;

    fn pair(key: &str, value: Value) -> (Value, Value) {
        (Value::Str(key.to_string()), value)
    }

    #[test]
    fn keeps_only_unknown_header_keys_in_the_order_they_were_read() {
        let raw = vec![
            pair("id", Value::Str("ndzz".into())),
            pair("futureField", Value::Str("미래".into())),
            pair("subAgendaIds", Value::List(vec![Value::Str("sa1".into())])),
            pair("links", Value::List(Vec::new())),
            pair(
                "labels",
                Value::List(vec![Value::Str("하나".into()), Value::Str("둘".into())]),
            ),
            pair("flag", Value::Bool(true)),
        ];
        // 아는 칸(`subAgendaIds`, `links` 포함)은 빠지고 모르는 칸만 읽은 차례 그대로 남는다
        assert_eq!(
            extra_front(&raw),
            "futureField: 미래\nlabels:\n  - 하나\n  - 둘\nflag: true\n"
        );
        assert_eq!(extra_front(&[]), "");
        assert_eq!(extra_front(&raw[..1]), "");
    }

    #[test]
    fn every_key_the_renderer_writes_is_a_known_key() {
        // 렌더러가 쓰는 칸이 `KNOWN_KEYS`에 없으면 다시 쓸 때 같은 칸이 두 번 적힌다
        let draft = NoteFront {
            id: "ndzz0001-0000aaaa",
            agenda: "demo",
            doc_type: "free",
            title: "제목",
            created: "2026-11-12T09:00:00+09:00",
            created_at: "2026-11-12T00:00:00.000Z",
            session_id: Some("s1"),
            session_agent: Some("codex"),
            tier: Tier::Record,
            decision_confirmation: Some("c"),
            edited_by: Some("app"),
            edited_at: "2026-11-12T09:00:00+09:00",
            edited_session_id: Some("s2"),
            edited_session_agent: Some("gemini"),
            tags: &["t".to_string()],
            sub_agenda_ids: &["sa1".to_string()],
            favorite: true,
            favorite_category: Some("보관"),
            template: Some("tpl"),
            evaluation: Some(&Evaluation {
                score: 3,
                note: "n".into(),
                at: "2026-11-12T09:00:00+09:00".into(),
            }),
            links: &[Link {
                to: "ndzz0002-0000bbbb".into(),
                kind: LinkKind::Direct,
            }],
            extra: "",
        }
        .render();
        for line in draft.lines().filter(|l| !l.starts_with(' ')) {
            let key = line.split(':').next().unwrap();
            assert!(
                KNOWN_KEYS.contains(&key) || key == "links",
                "알려지지 않은 칸을 쓴다: {key}"
            );
        }
        for key in KNOWN_KEYS {
            assert!(
                draft.lines().any(|l| l.starts_with(&format!("{key}:"))),
                "{key} 칸을 쓰는 입력이 시험에 없다"
            );
        }
    }

    #[test]
    fn formats_korean_standard_time_to_the_minute() {
        assert_eq!(kst_minute(1_794_700_000_000), "2026-11-15T08:46:00+09:00");
        assert_eq!(kst_minute(0), "1970-01-01T09:00:00+09:00");
    }
}
