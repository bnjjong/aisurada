use serde::Serialize;

use crate::commands::Caller;
use crate::vault::agenda::AgendaInfo;
use crate::vault::note_meta::{Evaluation, Link};
use crate::vault::scanner::NoteRecord;

/// 목록 응답
#[derive(Debug, Serialize)]
pub struct Page<T> {
    pub items: Vec<T>,
    /// 조건에 맞은 전체 건수
    pub total: usize,
    pub returned: usize,
    /// 건수 제한 때문에 `total`보다 적게 돌려줬는가
    pub truncated: bool,
}

impl<T> Page<T> {
    pub fn new(items: Vec<T>, total: usize) -> Self {
        let returned = items.len();
        Page {
            items,
            total,
            returned,
            truncated: returned < total,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct LinkRow {
    pub to: String,
    pub kind: String,
}

impl From<&Link> for LinkRow {
    fn from(link: &Link) -> Self {
        LinkRow {
            to: link.to.clone(),
            kind: link.kind.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct EvaluationRow {
    pub score: u8,
    pub note: String,
    pub at: String,
}

impl From<&Evaluation> for EvaluationRow {
    fn from(e: &Evaluation) -> Self {
        EvaluationRow {
            score: e.score,
            note: e.note.clone(),
            at: e.at.clone(),
        }
    }
}

/// 노트 한 줄
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteRow {
    pub id: String,
    pub agenda: String,
    #[serde(rename = "type")]
    pub doc_type: String,
    pub title: String,
    pub created: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_agent: Option<String>,
    /// 기록 층 노트에만(앱에게만) `record`가 붙는다.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_session_agent: Option<String>,
    pub format: &'static str,
    /// 고치기 전 확인 값(`f1:…`)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precondition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attach_text: Option<String>,
    pub links: Vec<LinkRow>,
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub favorite: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favorite_category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evaluation: Option<EvaluationRow>,
    /// vault 기준 상대 경로
    pub path: String,
    /// 폴더로 된 노트인가(후속 노트·코멘트·첨부를 가질 수 있다)
    pub folder: bool,
}

impl NoteRow {
    /// 훑은 노트로 목록 줄을 만듭니다. `body`가 `Some`이면 본문을 싣습니다.
    pub fn from_record(
        note: &NoteRecord,
        caller: Caller,
        body: bool,
        attach_text: Option<String>,
    ) -> Self {
        let meta = &note.meta;
        // 앱에게는 기록 층 노트에 `record`를 붙이고, AI에게는 기록 층 노트를 아예 보여 주지 않는다(호출 쪽에서 거른다).
        let tier = (caller == Caller::App && meta.tier == "record").then_some("record");
        // 편집 세션 id는 앱이 고친 노트에는 싣지 않는다
        let app_edit = meta.edited_by.as_deref() == Some("app");
        NoteRow {
            id: meta.id.clone(),
            agenda: meta.agenda_slug.clone(),
            doc_type: meta.doc_type.clone(),
            title: meta.title.clone(),
            created: meta.created.clone(),
            session_id: meta.session_id.clone(),
            session_agent: meta.session_agent.clone(),
            tier,
            edited_by: meta.edited_by.clone(),
            edited_at: meta.edited_at.clone(),
            edited_session_id: if app_edit {
                None
            } else {
                meta.edited_session_id.clone()
            },
            edited_session_agent: if app_edit {
                None
            } else {
                meta.edited_session_agent.clone()
            },
            format: note.format.as_str(),
            precondition: note.token.clone(),
            body: body.then(|| note.body.clone()),
            attach_text: attach_text.filter(|t| !t.is_empty()),
            links: meta.links.iter().map(LinkRow::from).collect(),
            tags: meta.tags.clone(),
            favorite: meta.favorite,
            favorite_category: meta.favorite_category.clone(),
            template: meta.template.clone(),
            evaluation: meta.evaluation.as_ref().map(EvaluationRow::from),
            path: note.rel_path.clone(),
            folder: !note.is_single_file,
        }
    }
}

/// 아젠다 한 줄
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgendaRow {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub hue: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_repo: Option<String>,
    pub summary: String,
}

impl From<&AgendaInfo> for AgendaRow {
    fn from(a: &AgendaInfo) -> Self {
        AgendaRow {
            id: a.id.clone(),
            slug: a.slug.clone(),
            title: a.title.clone(),
            hue: a.hue.clone(),
            repo: a.repo.clone(),
            project: a.project.clone(),
            project_repo: a.project_repo.clone(),
            summary: a.summary.clone(),
        }
    }
}
