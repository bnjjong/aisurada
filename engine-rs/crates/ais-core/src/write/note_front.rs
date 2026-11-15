//! 노트 머리(YAML) 만들기. 줄 차례와 따옴표 규칙은 Java 엔진의 `FrontmatterWriter.buildFullNoteFrontmatter`와 같아서,
//! 건드리지 않은 부분의 글자가 바뀌지 않는다.

use crate::frontmatter::dump_string;
use crate::vault::note_meta::{Evaluation, Link};
use crate::kernel::tier::Tier;

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
