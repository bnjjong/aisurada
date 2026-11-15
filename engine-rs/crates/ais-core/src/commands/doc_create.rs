//! 새 노트를 만드는 쓰기 `create_doc`. 아젠다 찾기·만들기, 날짜와 시각 배정, 노트 폴더 쓰기, 경고까지 한 번에 한다.
//! 인자 이름, 검사 순서, 오류 문구와 응답 필드는 Java 엔진과 같고, 파일 글은 바이트까지 같다.

use crate::kernel::doc_type::DocType;
use crate::kernel::link_kind::LinkKind;
use crate::kernel::tier::Tier;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value as Json;

use super::Caller;
use super::doc_write::parse_tags;
use crate::error::EngineError;
use crate::json::{self, JsonValue};
use crate::kernel::java::to_json_value;
use crate::kernel::token::compute_token;
use crate::text::{NOTE_ID, compare_utf16, encode_uri_component, is_blank, trim, utf16_len};
use crate::vault::agenda::AgendaInfo;
use crate::vault::note_meta::{Link, TEMPLATE_ID};
use crate::vault::scanner::NoteRecord;
use crate::write::apparatus::{next_id, next_note_id, now_millis};
use crate::write::atomic::write_atomic;
use crate::write::confine::{check_confinement, has_unusable_path_characters};
use crate::write::doc_place::{
    CivilDate, folder_name, parent_folder, parse_date_arg, parse_time_arg,
};
use crate::write::local_time::local_clock;
use crate::write::meta_writer::MAX_FILE_UNITS;
use crate::write::note_front::NoteFront;
use crate::write::pipeline::{WriteContext, WriteScope};
use crate::write::{agenda_file, slug, warnings};

const USAGE: &str = "create-doc agenda=<slug 또는 이름> type=<유형> title=<제목> body=<본문> [date=<YYYY-MM-DD>] [time=<HH:MM>] [format=markdown] [links=<JSON 배열>] [tags=<JSON 배열>] [template=<템플릿 id>] [tier=reading|record]";

/// `create_doc` 요청. `links`와 `tags`는 호출 객체에서는 배열, 명령줄에서는 JSON 배열 글로 온다.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CreateDocRequest {
    /// 아젠다 slug 또는 이름. 없으면 새로 만든다
    pub agenda: Option<String>,
    #[serde(rename = "type")]
    pub doc_type: Option<String>,
    pub title: Option<String>,
    pub body: Option<String>,
    /// `YYYY-MM-DD`. 안 주면 오늘
    pub date: Option<String>,
    /// `HH:MM`. 안 주면 지금 시각에서 그날 겹치지 않는 5분 칸
    pub time: Option<String>,
    pub format: Option<String>,
    pub links: Option<Json>,
    pub tags: Option<Json>,
    pub template: Option<String>,
    /// `reading`(기본)이나 `record`. AI 호출에서는 받지 않는다.
    pub tier: Option<String>,
    #[serde(flatten)]
    pub context: WriteContext,
}

/// 새 노트를 만든 결과. `create_doc`의 응답이고, 쓰기 범위를 이미 잡은 `plan_day`가 같은 길로 노트를 만들 때 이어받는 값이다
/// (예전에는 JSON에서 키 네 개를 뽑아 썼다. 명령끼리의 약속에 타입을 준다).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatedDoc {
    pub id: String,
    pub agenda: String,
    pub agenda_created: bool,
    pub date: String,
    pub path: String,
    pub precondition: String,
    pub deep_link: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

/// 새 노트 한 건을 만든다.
pub fn create_doc(
    vault: &Path,
    request: &CreateDocRequest,
    scope: &WriteScope,
) -> Result<CreatedDoc, EngineError> {
    create_doc_in(vault, scope, request)
}

/// 이미 쓰기 범위를 잡은 명령(`plan_day`)이 같은 길로 노트를 만든다.
pub fn create_doc_in(
    vault: &Path,
    scope: &WriteScope,
    request: &CreateDocRequest,
) -> Result<CreatedDoc, EngineError> {
    let context = &request.context;
    let tool = context.caller == Caller::Ai;

    if let Some(format) = request.format.as_deref()
        && format != "markdown"
    {
        return Err(EngineError::invalid_argument(format!(
            "모르는 본문 포맷 이다: {format}. 쓸 수 있는 것은 [markdown] 뿐이다."
        )));
    }
    let raw_agenda = request
        .agenda
        .as_deref()
        .ok_or_else(|| EngineError::invalid_argument(format!("agenda 이 필요하다: {USAGE}")))?;
    let raw_type = request
        .doc_type
        .as_deref()
        .ok_or_else(|| EngineError::invalid_argument(format!("type 이 필요하다: {USAGE}")))?;
    let canonical_type = DocType::parse_ignore_ascii_case(raw_type)
        .map(DocType::as_str)
        .ok_or_else(|| {
            EngineError::invalid_argument(format!(
                "모르는 유형 이다: {raw_type}. 쓸 수 있는 것은 [{}] 뿐이다.",
                DocType::NAMES_TEXT
            ))
        })?;
    let raw_title = request
        .title
        .as_deref()
        .ok_or_else(|| EngineError::invalid_argument(format!("title 이 필요하다: {USAGE}")))?;
    let raw_body = request
        .body
        .as_deref()
        .ok_or_else(|| EngineError::invalid_argument(format!("body 이 필요하다: {USAGE}")))?;
    let given_date = request.date.as_deref().map(parse_date_arg).transpose()?;
    let given_time = request.time.as_deref().map(parse_time_arg).transpose()?;
    let links = match &request.links {
        Some(raw) => parse_links(raw)?,
        None => Vec::new(),
    };
    let tags = match &request.tags {
        Some(raw) => parse_tags(raw)?,
        None => Vec::new(),
    };
    // 노트 층은 명령 표면만 받는다
    let mut tier = Tier::Reading;
    if !tool && let Some(raw) = request.tier.as_deref() {
        tier = Tier::parse_ignore_ascii_case(raw).ok_or_else(|| {
            EngineError::invalid_argument(format!(
                "모르는 노트 층 이다: {raw}. 쓸 수 있는 것은 [{}] 뿐이다.",
                Tier::NAMES_TEXT
            ))
        })?;
    }
    if is_blank(raw_title) {
        return Err(EngineError::invalid_argument("노트 제목이 비었다"));
    }
    let template = request
        .template
        .as_deref()
        .map(trim)
        .filter(|t| TEMPLATE_ID.is_match(t));
    let agenda_key = trim(raw_agenda);
    if agenda_key.is_empty() {
        return Err(EngineError::invalid_argument("아젠다 이름이 비었다"));
    }

    let (agenda_slug, agenda_created) = lookup_or_create_agenda(vault, scope, agenda_key)?;

    // 날짜와 시각 배정
    let now = local_clock(now_millis());
    let date = given_date.unwrap_or(now.date);
    let (hour, minute) = match given_time {
        Some(time) => time,
        None => schedule_time(&date, scope, now.hour, now.minute),
    };
    let created = format!("{}T{hour:02}:{minute:02}:00+09:00", date.era_text());

    let same_agenda: Vec<&NoteRecord> = scope
        .index
        .root_notes()
        .filter(|note| note.meta.agenda_slug == agenda_slug)
        .collect();
    let warning = warnings::detect_warnings(tool, raw_body, raw_title, &same_agenda);

    // 노트 폴더 자리
    let parent = parent_folder(vault, &agenda_slug, &date);
    let note_id = next_note_id();
    let folder = folder_name(&date, hour, minute, raw_title, &note_id);
    let note_rel = format!("{parent}/{folder}");
    if has_unusable_path_characters(&note_rel) {
        return Err(EngineError::invalid_argument(format!(
            "경로로 쓸 수 없는 값이다: {note_rel}"
        )));
    }
    let note_dir = vault.join(&note_rel);
    check_confinement(vault, &note_dir)?;

    let session_id = context.session.as_deref().filter(|_| tool);
    let session_agent = session_id.and(
        context
            .session_agent
            .as_deref()
            .and_then(crate::vault::note_meta::agent_for_file),
    );
    let front = NoteFront {
        id: &note_id,
        agenda: &agenda_slug,
        doc_type: canonical_type,
        title: raw_title,
        created: &created,
        session_id,
        session_agent: session_agent.as_deref(),
        tier,
        edited_by: None,
        edited_at: None,
        edited_session_id: None,
        edited_session_agent: None,
        tags: &tags,
        favorite: false,
        favorite_category: None,
        template,
        evaluation: None,
        links: &links,
    }
    .render();
    let text = format!("---\n{front}---\n{raw_body}");
    if utf16_len(&text) > MAX_FILE_UNITS {
        return Err(EngineError::internal(format!(
            "노트가 저장 상한을 넘어 저장하지 않았다: {note_rel}"
        )));
    }
    write_atomic(&note_dir.join("note.md"), text.as_bytes())?;

    Ok(CreatedDoc {
        deep_link: format!(
            "aisurada://doc/{}?ws={}",
            encode_uri_component(&note_id),
            encode_uri_component(&scope.index.vault_root.to_string_lossy())
        ),
        precondition: compute_token(text.as_bytes()),
        id: note_id,
        agenda: agenda_slug,
        agenda_created,
        date: date.iso(),
        path: note_rel,
        warning,
    })
}

/// 그날의 겹치지 않는 시각. 지금 시각(분)에서 시작해, 같은 날 읽기 층 노트가 이미 쓰는 시각이면 23:55 앞까지 5분씩 미룬다.
fn schedule_time(date: &CivilDate, scope: &WriteScope, hour: u32, minute: u32) -> (u32, u32) {
    let day = date.iso();
    let occupied: std::collections::HashSet<&str> = scope
        .index
        .root_notes()
        .filter(|note| !note.meta.tier.is_record())
        .filter_map(|note| {
            let created = note.meta.created.as_str();
            (created.get(0..10) == Some(day.as_str()))
                .then(|| created.get(11..16))
                .flatten()
        })
        .collect();
    let (mut h, mut m) = (hour, minute);
    while occupied.contains(format!("{h:02}:{m:02}").as_str()) && (h, m) < (23, 55) {
        m += 5;
        if m >= 60 {
            m -= 60;
            h = (h + 1) % 24;
        }
    }
    (h, m)
}

/// slug가 같은 아젠다, 없으면 이름이 같은(대소문자 무시) 아젠다, 없으면 이름을 slug로 만든 것과 같은 아젠다를 찾는다.
/// 어느 것도 없으면 아젠다 파일을 새로 만든다(저장소·프로젝트 없음, 요약 비움). `(slug, 새로 만들었는가)`
fn lookup_or_create_agenda(
    vault: &Path,
    scope: &WriteScope,
    key: &str,
) -> Result<(String, bool), EngineError> {
    let table = &scope.index.agendas.table;
    if let Some(found) = table.by_slug(key) {
        return Ok((found.slug.clone(), false));
    }
    let lower = key.to_lowercase();
    if let Some((slug, _)) = table
        .slug_title_pairs()
        .find(|(_, title)| title.to_lowercase() == lower)
    {
        return Ok((slug.to_string(), false));
    }
    let slug_key = slug::build(key, 50);
    if has_unusable_path_characters(&format!("{slug_key}.md")) {
        return Err(EngineError::invalid_argument(format!(
            "경로로 쓸 수 없는 값이다: {slug_key}.md"
        )));
    }
    if let Some(found) = table.by_slug(&slug_key) {
        return Ok((found.slug.clone(), false));
    }
    let info = AgendaInfo {
        id: next_id("ag"),
        slug: slug_key.clone(),
        title: key.to_string(),
        hue: agenda_file::least_used_hue(table.infos().map(|a| a.hue.as_str())).to_string(),
        repo: None,
        project: None,
        project_repo: None,
        summary: String::new(),
    };
    write_atomic(
        &vault.join(agenda_file::rel_path(&info.slug)),
        &agenda_file::render(&info),
    )?;
    Ok((slug_key, true))
}

/// `links` 인자를 읽어 검사한다. 같은 `to`와 `kind`는 처음 하나만 남긴다.
fn parse_links(raw: &Json) -> Result<Vec<Link>, EngineError> {
    let items = match raw {
        Json::String(text) => {
            let not_array =
                || EngineError::invalid_argument(format!("links 은 JSON 배열이어야 한다: {text}"));
            match json::parse(text).map_err(|_| not_array())? {
                JsonValue::Array(items) => items,
                _ => return Err(not_array()),
            }
        }
        Json::Array(items) => items.iter().map(to_json_value).collect(),
        other => {
            return Err(EngineError::invalid_argument(format!(
                "links 은 JSON 배열이어야 한다: {other}"
            )));
        }
    };
    let mut result: Vec<Link> = Vec::new();
    for item in &items {
        let JsonValue::Object(entries) = item else {
            return Err(EngineError::invalid_argument(format!(
                "links 항목이 객체가 아니다: {}",
                json::compact(item)
            )));
        };
        let link = parse_link(entries)?;
        if !result.contains(&link) {
            result.push(link);
        }
    }
    Ok(result)
}

fn parse_link(entries: &[(String, JsonValue)]) -> Result<Link, EngineError> {
    let mut unknown: Vec<&str> = entries
        .iter()
        .map(|(key, _)| key.as_str())
        .filter(|key| *key != "to" && *key != "kind")
        .collect();
    if !unknown.is_empty() {
        unknown.sort_by(|a, b| compare_utf16(a, b));
        return Err(EngineError::invalid_argument(format!(
            "links 항목에 모르는 키가 있다: [{}]. 쓸 수 있는 것은 [to, kind] 뿐이다.",
            unknown.join(", ")
        )));
    }
    let field = |name: &str| {
        entries
            .iter()
            .rev()
            .find(|(key, _)| key == name)
            .map(|(_, v)| v)
    };
    let text_of = |name: &str| -> Result<String, EngineError> {
        match field(name) {
            Some(JsonValue::Str(text)) => {
                let length = utf16_len(text);
                if length > 200 {
                    return Err(EngineError::invalid_argument(format!(
                        "links 항목의 {name} 가 너무 길다(최대 200자): {length}자"
                    )));
                }
                Ok(text.clone())
            }
            other => Err(EngineError::invalid_argument(format!(
                "links 항목의 {name} 가 문자열이 아니다: {}",
                other.map_or_else(|| "null".to_string(), json::compact)
            ))),
        }
    };
    let to = text_of("to")?;
    let kind = text_of("kind")?;
    let kind = LinkKind::parse_ignore_ascii_case(&kind)
        .map(LinkKind::as_str)
        .ok_or_else(|| {
            EngineError::invalid_argument(format!(
                "모르는 관계 종류 이다: {kind}. 쓸 수 있는 것은 [{}] 뿐이다.",
                LinkKind::NAMES_TEXT
            ))
        })?;
    if !NOTE_ID.is_match(&to) {
        return Err(EngineError::invalid_argument(format!(
            "노트 id 모양이 아니다: {to}"
        )));
    }
    Ok(Link {
        to,
        kind: kind.to_string(),
    })
}
