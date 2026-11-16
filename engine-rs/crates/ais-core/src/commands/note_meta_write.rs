//! 노트 머리만 고치는 쓰기: `set_favorite`(즐겨찾기), `evaluate_doc`(평가), `link_docs`(연결 더하기·빼기).
//! 인자 이름, 검사 순서, 오류 문구와 응답 필드는 Java 엔진과 같다.

use crate::kernel::java::java_double_string;
use crate::kernel::link_kind::LinkKind;
use std::path::Path;

use serde::Deserialize;
use serde_json::{Value, json};

use super::Caller;
use super::subnote::check_writable;
use crate::error::EngineError;
use crate::kernel::time::format_utc_millis;
use crate::text::{NOTE_ID, cut_utf16, is_blank, trim};
use crate::vault::note_meta::{Evaluation, Link};
use crate::vault::scanner::NoteRecord;
use crate::write::apparatus::now_millis;
use crate::write::meta_writer::{MetaState, write_meta};
use crate::write::pipeline::{WriteContext, WriteScope};

/// 참/거짓 인자. 호출 객체에서는 `true`, 명령줄에서는 `"true"` 글로 온다.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum BoolArg {
    Bool(bool),
    Text(String),
}

/// 숫자 인자. 호출 객체에서는 숫자, 명령줄에서는 글로 온다.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum NumberArg {
    Number(f64),
    Text(String),
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SetFavoriteRequest {
    pub id: String,
    /// 안 주면 지금 값을 뒤집는다(`category`만 주면 값을 그대로 둔다)
    pub favorite: Option<BoolArg>,
    /// 라벨. 빈 값이면 지운다
    pub category: Option<String>,
    /// 직전에 읽은 확인 값
    pub f1: Option<String>,
    #[serde(flatten)]
    pub context: WriteContext,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EvaluateDocRequest {
    pub id: String,
    pub score: Option<NumberArg>,
    pub note: Option<String>,
    pub clear: Option<BoolArg>,
    pub f1: Option<String>,
    #[serde(flatten)]
    pub context: WriteContext,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LinkDocsRequest {
    pub from: String,
    pub to: String,
    pub kind: Option<String>,
    pub remove: Option<BoolArg>,
    pub f1: Option<String>,
    #[serde(flatten)]
    pub context: WriteContext,
}

fn bool_arg(name: &str, value: &Option<BoolArg>) -> Result<Option<bool>, EngineError> {
    match value {
        None => Ok(None),
        Some(BoolArg::Bool(b)) => Ok(Some(*b)),
        Some(BoolArg::Text(text)) => match trim(text) {
            "" => Ok(None),
            "true" => Ok(Some(true)),
            "false" => Ok(Some(false)),
            other => Err(EngineError::invalid_argument(format!(
                "{name} 은 true 나 false 여야 한다: {other}"
            ))),
        },
    }
}

fn f1_arg(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(trim).filter(|t| !t.is_empty())
}

fn require_note_id(id: &str, usage: &str) -> Result<(), EngineError> {
    if is_blank(id) {
        return Err(EngineError::invalid_argument(format!(
            "인자가 하나 필요하다: {usage}"
        )));
    }
    if !NOTE_ID.is_match(id) {
        return Err(EngineError::invalid_argument(format!(
            "노트 id 모양이 아니다: {id}"
        )));
    }
    Ok(())
}

/// 고칠 노트를 찾는다. AI 호출자는 기록 층 노트를 고칠 수 없다.
fn find<'a>(
    scope: &'a WriteScope,
    id: &str,
    caller: Caller,
) -> Result<&'a NoteRecord, EngineError> {
    let note = scope
        .index
        .note(id)
        .ok_or_else(|| EngineError::not_found(format!("Note not found: {id}")))?;
    check_writable(note, caller)?;
    Ok(note)
}

pub fn set_favorite(
    vault: &Path,
    request: &SetFavoriteRequest,
    scope: &WriteScope,
) -> Result<Value, EngineError> {
    const USAGE: &str = "set-favorite <노트 id> [favorite=true|false. 안 주면 지금 값을 뒤집는다] [category=<라벨. 빈 값이면 지운다>] [f1=<직전에 읽은 토큰>]";
    require_note_id(&request.id, USAGE)?;
    let favorite = bool_arg("favorite", &request.favorite)?;
    let token = f1_arg(&request.f1);
    let note = find(scope, &request.id, request.context.caller)?;
    let meta = &note.meta;

    let new_favorite = match (favorite, request.category.is_some()) {
        (Some(f), _) => f,
        (None, true) => meta.favorite,
        (None, false) => !meta.favorite,
    };
    let new_category: Option<String> = match &request.category {
        Some(raw) if is_blank(raw) => None,
        Some(raw) => Some(trim(raw).to_string()),
        None => meta.favorite_category.clone(),
    };
    let unchanged = new_favorite == meta.favorite && new_category == meta.favorite_category;
    let mut response = json!({"id": request.id, "favorite": new_favorite});
    if let Some(category) = &new_category {
        response["category"] = json!(category);
    }
    if unchanged {
        response["changed"] = json!(false);
        return Ok(response);
    }
    let written = write_meta(
        vault,
        note,
        token,
        &MetaState {
            favorite: new_favorite,
            favorite_category: new_category.as_deref(),
            evaluation: meta.evaluation.as_ref(),
            links: &meta.links,
        },
        &request.context,
    )?;
    response["changed"] = json!(true);
    response["path"] = json!(written.path);
    response["precondition"] = json!(written.precondition);
    Ok(response)
}

pub fn evaluate_doc(
    vault: &Path,
    request: &EvaluateDocRequest,
    scope: &WriteScope,
) -> Result<Value, EngineError> {
    const USAGE: &str = "evaluate-doc <노트 id> [score=<0~100>] [note=<평가 메모>] [clear=true] [f1=<직전에 읽은 토큰>]";
    require_note_id(&request.id, USAGE)?;
    let clear = bool_arg("clear", &request.clear)?.unwrap_or(false);
    let mut score = 0.0;
    if !clear {
        let missing = || {
            EngineError::invalid_argument(format!(
                "score 를 주거나 clear=true 를 지정한다: {USAGE}"
            ))
        };
        score = match &request.score {
            None => return Err(missing()),
            Some(NumberArg::Number(n)) => *n,
            Some(NumberArg::Text(text)) => {
                let trimmed = trim(text);
                if trimmed.is_empty() {
                    return Err(missing());
                }
                trimmed.parse::<f64>().map_err(|_| {
                    EngineError::invalid_argument(format!("score 은 숫자여야 한다: {trimmed}"))
                })?
            }
        };
        if score.is_nan() || !(0.0..=100.0).contains(&score) {
            return Err(EngineError::invalid_argument(format!(
                "평가 점수는 0 에서 100 사이여야 한다: {}",
                java_double_string(score)
            )));
        }
    }
    let token = f1_arg(&request.f1);
    let note = find(scope, &request.id, request.context.caller)?;
    let evaluation = (!clear).then(|| Evaluation {
        // 반올림(0.5는 올림)한 뒤 0~100으로 맞춘다
        score: (score + 0.5).floor().clamp(0.0, 100.0) as u8,
        note: request
            .note
            .as_deref()
            .map_or_else(String::new, |n| cut_utf16(trim(n), 1000)),
        at: format_utc_millis(now_millis()),
    });
    let meta = &note.meta;
    let written = write_meta(
        vault,
        note,
        token,
        &MetaState {
            favorite: meta.favorite,
            favorite_category: meta.favorite_category.as_deref(),
            evaluation: evaluation.as_ref(),
            links: &meta.links,
        },
        &request.context,
    )?;
    Ok(json!({
        "id": request.id,
        "path": written.path,
        "precondition": written.precondition,
        "evaluation": evaluation.map(|e| json!({"score": e.score, "note": e.note, "at": e.at})),
    }))
}

pub fn link_docs(
    vault: &Path,
    request: &LinkDocsRequest,
    scope: &WriteScope,
) -> Result<Value, EngineError> {
    const USAGE: &str = "link-docs <출발 노트 id> <대상 노트 id> [kind=direct|indirect|attach] [remove=true] [f1=<직전에 읽은 토큰>]";
    if is_blank(&request.from) {
        return Err(EngineError::invalid_argument(format!(
            "인자가 하나 필요하다: {USAGE}"
        )));
    }
    if is_blank(&request.to) {
        return Err(EngineError::invalid_argument(format!(
            "대상 노트 id 가 필요하다: {USAGE}"
        )));
    }
    for id in [&request.from, &request.to] {
        if !NOTE_ID.is_match(id) {
            return Err(EngineError::invalid_argument(format!(
                "노트 id 모양이 아니다: {id}"
            )));
        }
    }
    let mut kind = LinkKind::Direct.as_str().to_string();
    if let Some(raw) = &request.kind
        && !trim(raw).is_empty()
    {
        let lower = trim(raw).to_lowercase();
        if LinkKind::parse(&lower).is_none() {
            return Err(EngineError::invalid_argument(format!(
                "모르는 관계 종류 이다: {}. 쓸 수 있는 것은 [{}] 뿐이다.",
                trim(raw),
                LinkKind::NAMES_TEXT
            )));
        }
        kind = lower;
    }
    let remove = bool_arg("remove", &request.remove)?.unwrap_or(false);
    if request.from == request.to {
        return Err(EngineError::invalid_argument(format!(
            "자기 자신과는 연결할 수 없다: {}",
            request.from
        )));
    }
    let token = f1_arg(&request.f1);
    let note = find(scope, &request.from, request.context.caller)?;
    let same = |l: &Link| l.to == request.to && l.kind.eq_ignore_ascii_case(&kind);
    let exists = note.meta.links.iter().any(same);
    if !remove && exists {
        return Ok(json!({"id": request.from, "changed": false, "note": "이미 있는 연결이에요."}));
    }
    if remove && !exists {
        return Ok(json!({"id": request.from, "changed": false, "note": "제거할 연결이 없어요."}));
    }
    let mut links: Vec<Link> = note
        .meta
        .links
        .iter()
        .filter(|l| !same(l))
        .cloned()
        .collect();
    if !remove {
        links.push(Link {
            to: request.to.clone(),
            kind: kind.clone(),
        });
    }
    let meta = &note.meta;
    let written = write_meta(
        vault,
        note,
        token,
        &MetaState {
            favorite: meta.favorite,
            favorite_category: meta.favorite_category.as_deref(),
            evaluation: meta.evaluation.as_ref(),
            links: &links,
        },
        &request.context,
    )?;
    let mut response = json!({
        "id": request.from,
        "to": request.to,
        "kind": kind,
        "removed": remove,
        "changed": true,
        "path": written.path,
        "precondition": written.precondition,
    });
    if !remove && scope.index.note(&request.to).is_none() {
        response["warning"] = json!(format!(
            "대상 문서 id를 vault에서 찾지 못했어요({}) — 연결은 저장되지만 대상이 생길 때까지 그래프엔 안 보일 수 있어요.",
            request.to
        ));
    }
    Ok(response)
}
