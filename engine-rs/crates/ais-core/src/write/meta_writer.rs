//! 노트 머리만 고쳐 쓰기: 즐겨찾기, 평가, 연결을 바꿀 때 본문은 그대로 두고 머리를 다시 짓는다.
//!
//! 고치기 전 확인 값(`f1`)을 검사하고, 머리를 새로 지은 글이 파일의 옛 머리를 그대로 옮긴 글과 같으면 편집 도장
//! (`editedBy`, `editedAt`, …)을 건드리지 않고, 다르면 지금 시각과 쓴 쪽으로 도장을 새로 찍는다.

use crate::error::EngineError;
use crate::frontmatter::Value;
use crate::kernel::actor::CallContext;
use crate::kernel::token::compute_token;
use crate::note::front::check_size;
use crate::note::front::judge;
pub use crate::note::front::{Draft, Editor, MAX_FILE_UNITS};
use crate::text::{cut_utf16, trim};
use crate::vault::note_file::split_text;
use crate::vault::note_meta::{Evaluation, Link, get};
use crate::vault::scanner::NoteRecord;
use crate::write::apparatus::now_millis;
use crate::write::atomic::write_atomic;
use crate::write::precondition;

/// 바꾼 뒤의 메타 값
pub struct MetaState<'a> {
    pub favorite: bool,
    pub favorite_category: Option<&'a str>,
    pub evaluation: Option<&'a Evaluation>,
    pub links: &'a [Link],
}

pub struct MetaWritten {
    /// vault 상대 경로
    pub path: String,
    /// 쓴 파일의 새 확인 값
    pub precondition: String,
}

pub fn write_meta(
    vault: &std::path::Path,
    note: &NoteRecord,
    provided_token: Option<&str>,
    state: &MetaState,
    context: &CallContext,
) -> Result<MetaWritten, EngineError> {
    let rel = note
        .content_file
        .strip_prefix(vault)
        .unwrap_or(&note.content_file)
        .to_string_lossy()
        .replace('\\', "/");
    let bytes = precondition::check(vault, note, provided_token, "메타를 저장하지")?;
    let content = String::from_utf8(bytes).map_err(|_| {
        EngineError::internal(format!(
            "노트 본문 파일을 못 읽어 메타를 저장하지 않았다: {rel}"
        ))
    })?;
    let split = split_text(&content);
    let Some(raw) = split.frontmatter else {
        return Err(EngineError::internal(format!(
            "프론트매터가 없어 메타를 저장하지 않았다: {rel}"
        )));
    };
    let body = split.body;

    let agenda = match get(&raw, "agenda") {
        Some(Value::Str(s)) if !s.is_empty() => s.clone(),
        _ => {
            let parts: Vec<&str> = rel.split('/').collect();
            if parts.len() >= 2 {
                parts[1].to_string()
            } else {
                note.meta.agenda_slug.clone()
            }
        }
    };
    let session_id = match get(&raw, "sessionId") {
        Some(Value::Str(s)) if !trim(s).is_empty() => Some(cut_utf16(trim(s), 128)),
        _ => None,
    };
    let session_agent = session_id.as_ref().and(note.meta.session_agent.as_deref());

    let meta = &note.meta;
    let text = judge(
        note.format,
        &Draft {
            meta,
            agenda: &agenda,
            title: &meta.title,
            created: &meta.created,
            session_id: session_id.as_deref(),
            session_agent,
            tier: meta.tier,
            tags: &meta.tags,
            favorite: state.favorite,
            favorite_category: state.favorite_category,
            template: meta.template.as_deref(),
            evaluation: state.evaluation,
            links: state.links,
        },
        &body,
        &body,
        &raw,
        &editor_of(context),
        now_millis(),
    )?;
    check_size(&text, &rel)?;
    write_atomic(&note.content_file, text.as_bytes())?;
    Ok(MetaWritten {
        path: note.rel_path.clone(),
        precondition: compute_token(text.as_bytes()),
    })
}

/// 쓰기 맥락에서 편집자를 뽑는다(AI 도구면 세션과 에이전트가 도장에 남는다).
pub fn editor_of(context: &CallContext) -> Editor<'_> {
    Editor {
        tool: context.caller == crate::kernel::actor::Caller::Ai,
        session: context.session.as_deref(),
        session_agent: context.session_agent.as_deref(),
    }
}
