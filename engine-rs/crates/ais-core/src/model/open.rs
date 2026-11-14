//! 노트 하나를 열 때 쓰는 응답 줄: 본문과 코멘트가 든 노트, 후속 노트, 첨부, 사실층.

use serde::Serialize;

use super::notes::LinkRow;
use crate::vault::comments::CommentsEnvelope;
use crate::vault::scanner::{Format, NoteRecord};

/// `get_doc`의 응답: 노트 줄에 본문(늘)과 코멘트가 붙는다.
#[derive(Debug, Serialize)]
pub struct DocRow {
    #[serde(flatten)]
    pub note: super::NoteRow,
    pub comments: CommentsEnvelope,
}

/// 후속 노트 한 줄. 메인 노트와 달리 아젠다·세션·즐겨찾기가 없고, 어느 메인 노트의 어느 노트 아래인지를 `rootId`·`parentId`로 알린다.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubNoteRow {
    pub id: String,
    /// 이 후속 노트가 속한 메인 노트
    pub root_id: String,
    /// 바로 위 노트(메인 노트이거나 다른 후속 노트)
    pub parent_id: String,
    #[serde(rename = "type")]
    pub doc_type: String,
    pub title: String,
    pub created: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_session_agent: Option<String>,
    pub format: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precondition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comments: Option<CommentsEnvelope>,
    pub links: Vec<LinkRow>,
    pub tags: Vec<String>,
    pub path: String,
    pub folder: bool,
}

impl SubNoteRow {
    /// 훑은 후속 노트로 줄을 만듭니다. `body`가 참이면 본문을 싣고, `comments`가 있으면 코멘트를 붙입니다.
    pub fn from_record(sub: &NoteRecord, body: bool, comments: Option<CommentsEnvelope>) -> Self {
        let meta = &sub.meta;
        // JSON 후속 노트는 본문·확인 값·도장을 내지 않는다
        let hidden = sub.format == Format::Json;
        let stamped = !hidden && meta.edited_by.is_some() && meta.edited_at.is_some();
        let app_edit = meta.edited_by.as_deref() == Some("app");
        let show_session = stamped && !app_edit;
        SubNoteRow {
            id: sub.id.clone(),
            root_id: sub.root_note_id.clone(),
            parent_id: sub
                .direct_parent_id
                .clone()
                .unwrap_or_else(|| sub.root_note_id.clone()),
            doc_type: meta.doc_type.clone(),
            title: meta.title.clone(),
            created: meta.created.clone(),
            edited_by: stamped.then(|| meta.edited_by.clone()).flatten(),
            edited_at: stamped.then(|| meta.edited_at.clone()).flatten(),
            edited_session_id: show_session
                .then(|| meta.edited_session_id.clone())
                .flatten(),
            edited_session_agent: (show_session && meta.edited_session_id.is_some())
                .then(|| meta.edited_session_agent.clone())
                .flatten(),
            format: sub.format.as_str(),
            precondition: if hidden { None } else { sub.token.clone() },
            body: body.then(|| {
                if hidden {
                    String::new()
                } else {
                    sub.body.clone()
                }
            }),
            comments,
            links: meta.links.iter().map(LinkRow::from).collect(),
            tags: meta.tags.clone(),
            path: sub.rel_path.clone(),
            folder: !sub.is_single_file,
        }
    }
}

/// 첨부 한 줄: 파일 이름과, 본문에 그대로 붙여 넣을 수 있는 마크다운 참조
#[derive(Debug, Serialize)]
pub struct AttachmentRow {
    pub name: String,
    #[serde(rename = "ref")]
    pub reference: String,
}

/// `get_agenda_facts`의 응답
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FactsRow {
    pub id: String,
    pub slug: String,
    pub path: String,
    pub limit: usize,
    pub edit_limit: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub facts: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub too_large_to_edit: bool,
}

/// `search_docs`의 한 줄: 노트 줄(본문 없이)에 어디에서 걸렸는지와, 본문에서 걸렸으면 맥락 글이 붙는다.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRow {
    #[serde(flatten)]
    pub note: super::NoteRow,
    /// `id`, `title`, `meta`, `body` 가운데 하나
    pub matched_in: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
}
