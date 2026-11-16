//! 노트 머리(YAML) 만들기와 편집 도장 판정(`judge`). 줄 차례와 따옴표 규칙은 Java 엔진의 `FrontmatterWriter.buildFullNoteFrontmatter`와 같아서,
//! 건드리지 않은 부분의 글자가 바뀌지 않는다.

use super::meta::{Evaluation, Format, Link, MetaData, agent_for_file};
use crate::error::EngineError;
use crate::frontmatter::dump_string;
use crate::frontmatter::{Value, dump_general_map};
use crate::kernel::tier::Tier;
use crate::kernel::time::civil_from_days;

pub struct NoteFront<'a> {
    pub id: &'a str,
    pub agenda: &'a str,
    pub doc_type: &'a str,
    pub title: &'a str,
    pub created: &'a str,
    pub session_id: Option<&'a str>,
    pub session_agent: Option<&'a str>,
    pub tier: Tier,
    pub edited_by: Option<&'a str>,
    pub edited_at: Option<&'a str>,
    pub edited_session_id: Option<&'a str>,
    pub edited_session_agent: Option<&'a str>,
    pub tags: &'a [String],
    pub favorite: bool,
    pub favorite_category: Option<&'a str>,
    pub template: Option<&'a str>,
    pub evaluation: Option<&'a Evaluation>,
    pub links: &'a [Link],
}

impl NoteFront<'_> {
    /// 머리 줄들(여닫는 울타리는 뺀다). 마지막 줄도 줄바꿈으로 끝난다.
    pub fn render(&self) -> String {
        let mut out = String::new();
        let mut line =
            |key: &str, value: &str| out.push_str(&format!("{key}: {}\n", dump_string(value, 1)));
        line("id", self.id);
        line("agenda", self.agenda);
        line("type", &self.doc_type.to_lowercase());
        line("title", self.title);
        line("created", self.created);
        if let Some(session) = self.session_id {
            line("sessionId", session);
            if let Some(agent) = self.session_agent {
                line("sessionAgent", agent);
            }
        }
        if self.tier.is_record() {
            out.push_str(&format!("tier: {}\n", Tier::Record.as_str()));
        }
        if let (Some(by), Some(at)) = (self.edited_by, self.edited_at) {
            let mut line = |key: &str, value: &str| {
                out.push_str(&format!("{key}: {}\n", dump_string(value, 1)))
            };
            line("editedBy", by);
            line("editedAt", at);
            if let Some(session) = self.edited_session_id {
                line("editedSessionId", session);
                if let Some(agent) = self.edited_session_agent {
                    line("editedSessionAgent", agent);
                }
            }
        }
        if !self.tags.is_empty() {
            out.push_str("tags:\n");
            for tag in self.tags {
                out.push_str(&format!("  - {}\n", dump_string(tag, 2)));
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
                    dump_string(&link.kind.to_lowercase(), 3)
                ));
            }
        }
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

/// 쓸 파일 글이 저장 상한(UTF-16 단위 [`MAX_FILE_UNITS`])을 넘으면 `rel`(쓸 경로)을 붙인 오류를 낸다.
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
    let front = |edited_by: Option<&str>,
                 edited_at: Option<&str>,
                 edited_session: Option<&str>,
                 edited_agent: Option<&str>| {
        NoteFront {
            id: &meta.id,
            agenda: draft.agenda,
            doc_type: &meta.doc_type,
            title: draft.title,
            created: draft.created,
            session_id: draft.session_id,
            session_agent: draft.session_agent,
            tier: draft.tier,
            edited_by,
            edited_at,
            edited_session_id: edited_session,
            edited_session_agent: edited_agent,
            tags: draft.tags,
            favorite: draft.favorite,
            favorite_category: draft.favorite_category,
            template: draft.template,
            evaluation: draft.evaluation,
            links: draft.links,
        }
        .render()
    };
    let (open, close) = if format == Format::Html {
        ("<!--aisurada\n", "-->\n")
    } else {
        ("---\n", "---\n")
    };
    let form_a = format!(
        "{open}{}{close}{body}",
        front(
            meta.edited_by.as_deref(),
            meta.edited_at.as_deref(),
            meta.edited_session_id.as_deref(),
            meta.edited_session_agent.as_deref()
        )
    );
    let dumped = dump_general_map(raw).map_err(|e| EngineError::invalid_argument(e.0))?;
    let form_b = format!("{open}{dumped}{close}{old_body}");
    if form_a == form_b {
        return Ok(form_a);
    }
    let tool = editor.tool;
    let edited_at = kst_minute(now_millis);
    let edited_session = if tool { editor.session } else { None };
    let edited_agent = edited_session.and(editor.session_agent.and_then(agent_for_file));
    Ok(format!(
        "{open}{}{close}{body}",
        front(
            Some(if tool { "tool" } else { "app" }),
            Some(&edited_at),
            edited_session,
            edited_agent.as_deref()
        )
    ))
}

/// 한국 시각(+09:00)으로 분까지 적은 글 `yyyy-MM-ddTHH:mm:00+09:00`
pub(crate) fn kst_minute(millis: i64) -> String {
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
    use super::*;

    #[test]
    fn formats_korean_standard_time_to_the_minute() {
        assert_eq!(kst_minute(1_794_700_000_000), "2026-11-15T08:46:00+09:00");
        assert_eq!(kst_minute(0), "1970-01-01T09:00:00+09:00");
    }
}
