//! `list_decisions`·`pending_decisions`: 노트의 결정 블록과 사람이 남긴 답을 읽는다.
//!
//! AI는 「정할 것」을 결정 블록으로 쓰고 `ais-cli wait-decisions`로 사람의 답을 기다린다. 기다리는 쪽은 `list_decisions`를
//! 되풀이해 부르고, 앱은 `pending_decisions`로 답을 기다리는 노트 수(레일 배지)와 알림을 만든다. 형식과 끝났는지의 규칙은
//! `note::decision`에 있다. 읽기만 한다.

use serde::{Deserialize, Serialize};

use super::subnote::{check_note_id, check_readable};
use crate::error::EngineError;
use crate::kernel::actor::CallContext;
use crate::note::decision::{self, Decision, DecisionSheet};
use crate::text::{SUB_NOTE_ID, is_blank};
use crate::vault::index::NoteIndex;
use crate::vault::scanner::NoteRecord;

const USAGE: &str = "list-decisions <노트 id> [하위 노트 id]";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ListDecisionsRequest {
    /// 메인 노트 id
    pub id: String,
    /// 후속 노트 id. 주면 그 후속 노트의 결정 블록을 읽는다.
    pub sub_id: Option<String>,
}

/// 노트 하나의 결정과 답
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionsRow {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_id: Option<String>,
    pub title: String,
    pub decisions: Vec<Decision>,
    /// 답이 찬 결정 수
    pub answered: usize,
    pub total: usize,
    /// 「답 다 했음」 블록이 있는가와 체크했는가
    pub done_block: bool,
    pub done_checked: bool,
    /// 사람이 답을 끝냈다(「답 다 했음」을 체크해 저장함. 블록이 없으면 모두 답하고 저장함)
    pub finished: bool,
    /// 결정이 있고 아직 끝나지 않았다
    pub waiting: bool,
    /// 마지막으로 고친 쪽(`app`은 사람, `tool`은 AI)과 시각
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<String>,
    /// 묻는 결정들의 지문. 같은 묶음을 두 번 알리지 않으려고 쓴다
    pub signature: String,
    /// 이 노트를 고칠 때 넘길 쓰기 전 토큰
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precondition: Option<String>,
}

pub fn list(
    index: &NoteIndex,
    request: &ListDecisionsRequest,
    context: &CallContext,
) -> Result<DecisionsRow, EngineError> {
    check_note_id(&request.id, USAGE)?;
    let sub_id = request.sub_id.as_deref().filter(|s| !is_blank(s));
    if let Some(sub_id) = sub_id
        && !SUB_NOTE_ID.is_match(sub_id)
    {
        return Err(EngineError::invalid_argument(format!(
            "하위 노트 id 모양이 아니다: {sub_id}"
        )));
    }
    let root = index
        .note(&request.id)
        .ok_or_else(|| EngineError::not_found(format!("Note not found: {}", request.id)))?;
    check_readable(root, context.caller)?;
    let owner = match sub_id {
        None => root,
        Some(sub_id) => root
            .sub_notes
            .iter()
            .find(|s| s.id == sub_id)
            .ok_or_else(|| EngineError::not_found(format!("SubNote not found: {sub_id}")))?,
    };
    let sheet = decision::parse(&owner.body);
    Ok(row(root, sub_id.map(|_| owner), &sheet))
}

fn row(root: &NoteRecord, sub: Option<&NoteRecord>, sheet: &DecisionSheet) -> DecisionsRow {
    let owner = sub.unwrap_or(root);
    let human = decision::edited_by_human(owner.meta.edited_by.as_deref());
    DecisionsRow {
        id: root.id.clone(),
        sub_id: sub.map(|s| s.id.clone()),
        title: owner.meta.title.clone(),
        decisions: sheet.decisions.clone(),
        answered: sheet.answered_count(),
        total: sheet.decisions.len(),
        done_block: sheet.done_block,
        done_checked: sheet.done_checked,
        finished: sheet.finished(human),
        waiting: sheet.waiting(human),
        edited_by: owner.meta.edited_by.clone(),
        edited_at: owner.meta.edited_at.clone(),
        signature: sheet.signature(),
        precondition: owner.token.clone(),
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PendingDecisionsRequest {}

/// vault 전체에서 사람의 답을 기다리는 노트
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingDecisionsRow {
    /// 기다리는 노트(후속 노트 포함) 수. 레일 배지가 쓴다
    pub count: usize,
    pub notes: Vec<PendingNote>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingNote {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_id: Option<String>,
    pub title: String,
    pub answered: usize,
    pub total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<String>,
    pub signature: String,
}

/// 노트와 후속 노트를 모두 훑어 답을 기다리는 것을 모은다. 본문에 `[!decision`이 없는 노트는 읽지 않고 건너뛴다.
/// 호출자가 못 보는 노트(AI에게 기록 층)는 세지 않는다.
pub fn pending(index: &NoteIndex, context: &CallContext) -> PendingDecisionsRow {
    let mut notes = Vec::new();
    for root in index.root_notes() {
        if check_readable(root, context.caller).is_err() {
            continue;
        }
        for (owner, sub) in
            std::iter::once((root, None)).chain(root.sub_notes.iter().map(|s| (s, Some(s))))
        {
            if !mentions_decision(&owner.body) {
                continue;
            }
            let sheet = decision::parse(&owner.body);
            let human = decision::edited_by_human(owner.meta.edited_by.as_deref());
            if !sheet.waiting(human) {
                continue;
            }
            notes.push(PendingNote {
                id: root.id.clone(),
                sub_id: sub.map(|s| s.id.clone()),
                title: owner.meta.title.clone(),
                answered: sheet.answered_count(),
                total: sheet.decisions.len(),
                edited_at: owner.meta.edited_at.clone(),
                signature: sheet.signature(),
            });
        }
    }
    PendingDecisionsRow {
        count: notes.len(),
        notes,
    }
}

/// 결정 블록이 있을 수도 있는 본문인가(대소문자 무시). 대부분의 노트를 해석 없이 건너뛰려는 거름이다.
fn mentions_decision(body: &str) -> bool {
    body.as_bytes()
        .windows(10)
        .any(|w| w.eq_ignore_ascii_case(b"[!decision"))
}
