//! 새 노트를 만드는 쓰기 `create_doc`. 아젠다 찾기·만들기, 날짜와 시각 배정, 노트 폴더 쓰기, 경고까지 한 번에 한다.
//! 인자 이름, 검사 순서, 오류 문구와 응답 필드는 Java 엔진과 같고, 파일 글은 바이트까지 같다.

use std::path::Path;

use serde::Deserialize;
use serde_json::Value as Json;

use super::Caller;
use crate::error::EngineError;
use crate::kernel::token::compute_token;
use crate::note::create::{NewDocInput, ParsedNewDoc};
use crate::note::front::check_size;
use crate::text::encode_uri_component;
use crate::vault::agenda::AgendaInfo;
use crate::vault::scanner::NoteRecord;
use crate::write::apparatus::{next_id, next_note_id, now_millis};
use crate::write::atomic::write_atomic;
use crate::write::confine::{check_confinement, has_unusable_path_characters};
use crate::write::doc_place::{folder_name, parent_folder};
use crate::write::local_time::local_clock;
use crate::write::pipeline::{WriteContext, WriteScope};
use crate::write::{agenda_file, slug, warnings};

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

    // 인자 검사(규칙은 `note::create`). 오류가 나오는 차례는 그대로다.
    let parsed = ParsedNewDoc::parse(NewDocInput {
        agenda: request.agenda.as_deref(),
        doc_type: request.doc_type.as_deref(),
        title: request.title.as_deref(),
        body: request.body.as_deref(),
        date: request.date.as_deref(),
        time: request.time.as_deref(),
        format: request.format.as_deref(),
        links: request.links.as_ref(),
        tags: request.tags.as_ref(),
        template: request.template.as_deref(),
        tier: request.tier.as_deref(),
        tool,
    })?;
    let (raw_title, raw_body) = (parsed.title.as_str(), parsed.body.as_str());

    let (agenda_slug, agenda_created) = lookup_or_create_agenda(vault, scope, &parsed.agenda_key)?;

    // 날짜와 시각 배정: 오늘과 그날 겹치지 않는 5분 칸
    let now = local_clock(now_millis());
    let (date, hour, minute, created) = parsed.slot(
        &now,
        scope
            .index
            .root_notes()
            .filter(|note| !note.meta.tier.is_record())
            .map(|note| note.meta.created.as_str()),
    );

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

    let text = parsed.render(
        &note_id,
        &agenda_slug,
        &created,
        context.session.as_deref(),
        context.session_agent.as_deref(),
    );
    check_size(&text, &note_rel)?;
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
