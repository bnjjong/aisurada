//! `search_docs`: 검색어가 아이디, 제목, 메타(아젠다 이름·태그), 본문 가운데 어디에 걸리는지 가려 찾는다.
//!
//! 걸린 자리의 순위(아이디, 제목, 메타, 본문)가 앞선 노트가 먼저고, 같으면 만든 시각이 늦은 것, 그다음 id가 큰 것이 먼저다.
//! 위치와 길이는 Java와 같게 UTF-16 코드 단위로 센다(맥락 글이 같은 자리에서 잘리게 하려는 것이다).

use serde::Deserialize;

use super::Caller;
use crate::error::EngineError;
use crate::model::{NoteRow, Page, SearchRow};
use crate::text::{compare_utf16, find_region_match, is_blank, trim_search};
use crate::vault::attach_text;
use crate::vault::filter::{self, DocFilter};
use crate::vault::index::NoteIndex;
use crate::vault::note_meta::parse_created;
use crate::vault::plain_text;
use crate::vault::scanner::NoteRecord;

const USAGE: &str = "search-docs <검색어> [agenda=<slug 또는 이름>] [type=<유형>] [tag=<태그,태그>] [favorite=true] [favoriteCategory=<라벨>] [date=<YYYY-MM-DD>] [from=<YYYY-MM-DD>] [to=<YYYY-MM-DD>] [limit=<건수. 기본 20, 최대 200>]";
const DEFAULT_LIMIT: usize = 20;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SearchDocsRequest {
    pub query: String,
    #[serde(flatten)]
    pub filter: DocFilter,
    /// 받을 건수(1~200). 없으면 20
    pub limit: Option<i64>,
    pub caller: Caller,
}

struct Hit<'a> {
    note: &'a NoteRecord,
    /// 0 아이디, 1 제목, 2 메타, 3 본문
    rank: u8,
    matched_in: &'static str,
    snippet: Option<String>,
}

pub fn execute(
    index: &NoteIndex,
    request: &SearchDocsRequest,
) -> Result<Page<SearchRow>, EngineError> {
    if is_blank(&request.query) {
        return Err(EngineError::invalid_argument(format!(
            "검색어가 필요하다: {USAGE}"
        )));
    }
    let criteria = filter::build(&index.agendas.table, &request.filter)?;
    let limit = match request.limit {
        None => DEFAULT_LIMIT,
        Some(n) if !(1..=200).contains(&n) => {
            return Err(EngineError::invalid_argument(format!(
                "받을 건수가 범위를 벗어났다: {n}"
            )));
        }
        Some(n) => n as usize,
    };

    let needle: Vec<u16> = trim_search(&request.query).encode_utf16().collect();
    let mut hits: Vec<Hit> = Vec::new();
    if !needle.is_empty() {
        // 아이디로 찾을 때는 `#doc:` 머리와 `/` 뒤를 떼고 소문자로 맞춘다
        let lowered = trim_search(&request.query).to_lowercase();
        let id_target = lowered.strip_prefix("#doc:").unwrap_or(&lowered);
        let id_target = id_target.split('/').next().unwrap_or("");

        for note in index.root_notes() {
            if (request.caller == Caller::Ai && note.meta.tier == "record")
                || !filter::matches(note, &criteria)
                || trim_search(&note.meta.title).is_empty()
            {
                continue;
            }
            let agenda_title = index.agendas.table.title_of(&note.meta.agenda_slug);
            if id_target == note.id.to_lowercase() {
                hits.push(Hit {
                    note,
                    rank: 0,
                    matched_in: "id",
                    snippet: None,
                });
            } else if find_region_match(&utf16(&note.meta.title), &needle).is_some() {
                hits.push(Hit {
                    note,
                    rank: 1,
                    matched_in: "title",
                    snippet: None,
                });
            } else if find_region_match(&utf16(agenda_title), &needle).is_some()
                || note
                    .meta
                    .tags
                    .iter()
                    .any(|tag| find_region_match(&utf16(tag), &needle).is_some())
            {
                hits.push(Hit {
                    note,
                    rank: 2,
                    matched_in: "meta",
                    snippet: None,
                });
            } else if let Some(snippet) = body_snippet(index, note, &needle) {
                hits.push(Hit {
                    note,
                    rank: 3,
                    matched_in: "body",
                    snippet: Some(snippet),
                });
            }
        }
    }

    hits.sort_by(|a, b| {
        a.rank
            .cmp(&b.rank)
            .then_with(|| created_desc(a.note, b.note))
            .then_with(|| compare_utf16(&b.note.id, &a.note.id))
    });

    let total = hits.len();
    let items = hits
        .into_iter()
        .take(limit)
        .map(|hit| SearchRow {
            note: NoteRow::from_record(hit.note, request.caller, false, None),
            matched_in: hit.matched_in,
            snippet: hit.snippet,
        })
        .collect();
    Ok(Page::new(items, total))
}

fn utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// 본문과 HTML 첨부 글에서 검색어를 찾아 앞 24, 뒤 76 코드 단위의 맥락 글을 만든다.
fn body_snippet(index: &NoteIndex, note: &NoteRecord, needle: &[u16]) -> Option<String> {
    let plain = plain_text::extract(note.format, &note.body);
    let haystack = match attach_text::extract(&index.vault_root, note) {
        Some(attach) => format!("{plain} {attach}"),
        None => plain,
    };
    let text = utf16(&haystack);
    let at = find_region_match(&text, needle)?;
    let start = at.saturating_sub(24);
    let end = (at + needle.len() + 76).min(text.len());
    let piece = String::from_utf16_lossy(&text[start..end]);
    Some(format!(
        "{}{}{}",
        if start > 0 { "…" } else { "" },
        trim_search(&piece),
        if end < text.len() { "…" } else { "" }
    ))
}

/// 만든 시각이 늦은 쪽이 먼저
fn created_desc(a: &NoteRecord, b: &NoteRecord) -> std::cmp::Ordering {
    let instant = |n: &NoteRecord| parse_created(&n.meta.created).map(|c| c.utc_instant);
    match (instant(a), instant(b)) {
        (Some(x), Some(y)) => y
            .cmp(&x)
            .then_with(|| compare_utf16(&b.meta.created, &a.meta.created)),
        _ => compare_utf16(&b.meta.created, &a.meta.created),
    }
}
