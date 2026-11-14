//! 노트 머리만 고쳐 쓰기: 즐겨찾기, 평가, 연결을 바꿀 때 본문은 그대로 두고 머리를 다시 짓는다.
//!
//! 고치기 전 확인 값(`f1`)을 검사하고, 머리를 새로 지은 글이 파일의 옛 머리를 그대로 옮긴 글과 같으면 편집 도장
//! (`editedBy`, `editedAt`, …)을 건드리지 않고, 다르면 지금 시각과 쓴 쪽으로 도장을 새로 찍는다.

use crate::commands::Caller;
use crate::error::EngineError;
use crate::frontmatter::{Value, dump_general_map};
use crate::text::{cut_utf16, trim, utf16_len};
use crate::vault::note_file::{compute_token, split_text};
use crate::vault::note_meta::{Evaluation, Link, agent_for_file, get};
use crate::vault::scanner::{Format, NoteRecord};
use crate::write::apparatus::now_millis;
use crate::write::atomic::write_atomic;
use crate::write::note_front::NoteFront;
use crate::write::pipeline::WriteContext;
use crate::write::precondition;

/// 저장 상한(UTF-16 글자)
const MAX_FILE_UNITS: usize = 1_000_000;

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
    context: &WriteContext,
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
    let front = |edited_by: Option<&str>,
                 edited_at: Option<&str>,
                 edited_session: Option<&str>,
                 edited_agent: Option<&str>| {
        NoteFront {
            id: &meta.id,
            agenda: &agenda,
            doc_type: &meta.doc_type,
            title: &meta.title,
            created: &meta.created,
            session_id: session_id.as_deref(),
            session_agent,
            tier: &meta.tier,
            edited_by,
            edited_at,
            edited_session_id: edited_session,
            edited_session_agent: edited_agent,
            tags: &meta.tags,
            favorite: state.favorite,
            favorite_category: state.favorite_category,
            template: meta.template.as_deref(),
            evaluation: state.evaluation,
            links: state.links,
        }
        .render()
    };
    let (open, close) = if note.format == Format::Html {
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
    let dumped = dump_general_map(&raw).map_err(|e| EngineError::invalid_argument(e.0))?;
    let form_b = format!("{open}{dumped}{close}{body}");
    let text = if form_a == form_b {
        form_a
    } else {
        let tool = context.caller == Caller::Ai;
        let edited_at = kst_minute(now_millis());
        let edited_session = if tool {
            context.session.as_deref()
        } else {
            None
        };
        let edited_agent =
            edited_session.and(context.session_agent.as_deref().and_then(agent_for_file));
        format!(
            "{open}{}{close}{body}",
            front(
                Some(if tool { "tool" } else { "app" }),
                Some(&edited_at),
                edited_session,
                edited_agent.as_deref()
            )
        )
    };
    if utf16_len(&text) > MAX_FILE_UNITS {
        return Err(EngineError::internal(format!(
            "노트가 저장 상한을 넘어 저장하지 않았다: {rel}"
        )));
    }
    write_atomic(&note.content_file, text.as_bytes())?;
    Ok(MetaWritten {
        path: note.rel_path.clone(),
        precondition: compute_token(text.as_bytes()),
    })
}

/// 한국 시각(+09:00)으로 분까지 적은 글 `yyyy-MM-ddTHH:mm:00+09:00`
fn kst_minute(millis: i64) -> String {
    let local = millis + 9 * 3_600_000;
    let (year, month, day) = crate::vault::note_meta::civil_from_days(local.div_euclid(86_400_000));
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
