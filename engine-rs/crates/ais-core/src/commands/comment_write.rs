//! 코멘트 쓰기: `create_comment`(만들기), `update_comment`(고치기), `delete_comment`(지우기).
//! 인자 이름, 검사 순서, 오류 문구와 응답 필드는 Java 엔진과 같고, 코멘트 파일은 바이트까지 같다.
//!
//! 코멘트는 노트 폴더의 `comments.json`에, 후속 노트의 코멘트는 그 노트의 바로 위 노트 폴더의 `sub-comments/<후속 노트 id>.json`에 있다.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::{Value as Json, json};

use super::subnote::check_writable;
use crate::error::EngineError;
use crate::kernel::token::compute_token;
use crate::note::comment::{self, CommentInput, CommentOp};
use crate::text::{is_blank, utf16_len};
use crate::vault::comments::{Anchor, Comment, CommentsEnvelope, read_comments};
use crate::vault::index::NoteIndex;
use crate::vault::scanner::{Format, NoteRecord};
use crate::write::apparatus::{next_id, now_millis};
use crate::write::comments_file::{strip_anchors, write_comments};
use crate::write::local_time::local_clock;
use crate::write::pipeline::{WriteContext, WriteScope};
use crate::write::sub_note::{read_derived, rewrite_root, rewrite_sub};

const CREATE_USAGE: &str = "create-comment <노트 id> [하위 노트 id] body=<본문> [anchor_quote=<가리킨 글>] [comment_id=<식별자. 안 주면 발급한다>]";
const UPDATE_USAGE: &str =
    "update-comment <노트 id> [하위 노트 id] comment_id=<코멘트 id> body=<본문>";
const DELETE_USAGE: &str = "delete-comment <노트 id> [하위 노트 id] comment_id=<코멘트 id>";

/// 코멘트 쓰기 요청. 세 명령이 같은 인자를 쓰고, 안 쓰는 인자는 무시한다.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CommentWriteRequest {
    /// 메인 노트 id
    pub id: String,
    /// 후속 노트 id. 후속 노트의 코멘트일 때만
    #[serde(rename = "subId")]
    pub sub_id: Option<String>,
    pub body: Option<String>,
    /// 만들 때는 안 주면 발급하고, 고치기와 지우기에서는 필수
    pub comment_id: Option<String>,
    /// 만들 때만. 코멘트가 가리킨 본문의 글
    pub anchor_quote: Option<String>,
    #[serde(flatten)]
    pub context: WriteContext,
}

type Kind = CommentOp;

/// 풀어 낸 코멘트 자리
struct Target<'a> {
    root: &'a NoteRecord,
    /// 코멘트가 달린 노트(메인 노트이거나 후속 노트)
    owner: &'a NoteRecord,
    file: PathBuf,
}

fn resolve<'a>(
    vault: &Path,
    index: &'a NoteIndex,
    root_id: &str,
    sub_id: Option<&str>,
) -> Result<Target<'a>, EngineError> {
    let root = index
        .note(root_id)
        .ok_or_else(|| EngineError::not_found(format!("Note not found: {root_id}")))?;
    let Some(sub_id) = sub_id else {
        return Ok(Target {
            root,
            owner: root,
            file: vault.join(&root.rel_path).join("comments.json"),
        });
    };
    let sub = root
        .sub_notes
        .iter()
        .find(|s| s.id == sub_id)
        .ok_or_else(|| EngineError::not_found(format!("SubNote not found: {sub_id}")))?;
    let parent_id = sub.direct_parent_id.as_deref().unwrap_or("");
    let container = if parent_id.is_empty() || parent_id == root.id {
        root
    } else {
        root.sub_notes
            .iter()
            .find(|s| s.id == parent_id)
            .ok_or_else(|| EngineError::not_found(format!("SubNote not found: {parent_id}")))?
    };
    Ok(Target {
        root,
        owner: sub,
        file: vault
            .join(&container.rel_path)
            .join("sub-comments")
            .join(format!("{sub_id}.json")),
    })
}

/// 지금을 `yyyy-MM-ddTHH:mm:ss+09:00`로(시각은 이 프로세스의 시간대 그대로이고 오프셋 글만 `+09:00`이다)
fn now_text() -> String {
    let now = local_clock(now_millis());
    format!(
        "{}T{:02}:{:02}:{:02}+09:00",
        now.date.iso(),
        now.hour,
        now.minute,
        now.second
    )
}

pub fn create_comment(
    vault: &Path,
    request: &CommentWriteRequest,
    scope: &WriteScope,
) -> Result<Json, EngineError> {
    execute(vault, request, Kind::Create, scope)
}

pub fn update_comment(
    vault: &Path,
    request: &CommentWriteRequest,
    scope: &WriteScope,
) -> Result<Json, EngineError> {
    execute(vault, request, Kind::Update, scope)
}

pub fn delete_comment(
    vault: &Path,
    request: &CommentWriteRequest,
    scope: &WriteScope,
) -> Result<Json, EngineError> {
    execute(vault, request, Kind::Delete, scope)
}

fn execute(
    vault: &Path,
    request: &CommentWriteRequest,
    kind: Kind,
    scope: &WriteScope,
) -> Result<Json, EngineError> {
    let usage = match kind {
        Kind::Create => CREATE_USAGE,
        Kind::Update => UPDATE_USAGE,
        Kind::Delete => DELETE_USAGE,
    };

    // 인자 검사(규칙은 `note::comment`)
    let parsed = comment::parse(
        kind,
        CommentInput {
            note_id: &request.id,
            sub_id: request.sub_id.as_deref(),
            comment_id: request.comment_id.as_deref(),
            body: request.body.as_deref(),
            anchor_quote: request.anchor_quote.as_deref(),
        },
        usage,
    )?;
    let (sub_id, body, quote) = (parsed.sub_id, parsed.body, parsed.quote);
    let mut id = parsed.comment_id;
    let target = resolve(vault, &scope.index, &request.id, sub_id)?;
    check_writable(target.root, request.context.caller)?;
    let mut comments: Vec<Comment> = match read_comments(vault, &target.file) {
        CommentsEnvelope::Unreadable { reason } => {
            return Err(EngineError::conflict(format!(
                "코멘트 파일을 못 읽어서 아무것도 안 썼다: {reason}. 여기서 쓰면 그 파일에 있던 코멘트가 통째로 사라진다. 파일을 먼저 고치거나 치운다."
            )));
        }
        CommentsEnvelope::Loaded { comments } => comments,
        CommentsEnvelope::Absent => Vec::new(),
    };
    if kind == Kind::Create && id.is_empty() {
        id = next_id("cm");
    }
    let found = comments.iter().position(|c| c.id == id);
    if kind == Kind::Create && found.is_some() {
        return Err(EngineError::conflict(format!(
            "이미 그 식별자를 쓰는 코멘트가 있다: {id}"
        )));
    }
    if kind != Kind::Create && found.is_none() {
        return Err(EngineError::conflict(format!("그 코멘트가 없다: {id}")));
    }
    let rel = target
        .file
        .strip_prefix(vault)
        .unwrap_or(&target.file)
        .to_string_lossy()
        .replace('\\', "/");
    let mut response = json!({"id": id, "path": rel});

    match (kind, found) {
        (Kind::Create, _) => {
            comments.push(Comment {
                id: id.clone(),
                created_at: now_text(),
                edited_at: None,
                anchor: quote.filter(|q| !is_blank(q)).map(|q| Anchor {
                    quote: q.to_string(),
                }),
                body: body.unwrap_or_default().to_string(),
            });
        }
        (Kind::Update, Some(at)) => {
            let mut edited_at = now_text();
            let created_at = comments[at].created_at.clone();
            if utf16_len(&created_at) == 25 && utf16_len(&edited_at) == 25 && edited_at < created_at
            {
                edited_at = created_at;
            }
            comments[at].edited_at = Some(edited_at);
            comments[at].body = body.unwrap_or_default().to_string();
        }
        (Kind::Delete, Some(at)) => {
            let owner = target.owner;
            if owner.format != Format::Markdown {
                return Err(EngineError::conflict(format!(
                    "본문이 markdown 이 아니라 코멘트 앵커를 걷어낼 수 없다(포맷={}): {}",
                    owner.format.as_str().to_uppercase(),
                    owner.id
                )));
            }
            let body_file = if owner.is_extension_folder {
                vault.join(&owner.rel_path)
            } else {
                owner.content_file.clone()
            };
            let (old_body, old_bytes) = read_derived(&body_file);
            let stripped = strip_anchors(&old_body, &id);
            let changed = stripped != old_body;
            response["remaining"] = json!(comments.len() - 1);
            response["markerStripped"] = json!(changed);
            if changed {
                let written = if owner.is_sub_note {
                    rewrite_sub(vault, target.root, owner, &stripped)?.bytes
                } else {
                    rewrite_root(vault, owner, &stripped)?
                };
                response["bodyBefore"] =
                    json!(compute_token(old_bytes.as_deref().unwrap_or_default()));
                response["bodyAfter"] = json!(compute_token(&written));
            }
            comments.remove(at);
        }
        _ => {}
    }
    write_comments(&target.file, &mut comments)?;
    if kind != Kind::Delete {
        response["total"] = json!(comments.len());
    }
    Ok(response)
}
