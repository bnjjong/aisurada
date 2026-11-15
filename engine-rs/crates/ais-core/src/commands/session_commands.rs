//! 4부 세션 기록 읽기 명령 3개: `read_session`, `session_titles`, `recent_sessions`. 외부 설명서 7장을 따른다.
//! 인자 이름, 검사 순서, 오류 문구와 응답 필드는 Java 엔진의 `SessionCommands`와 같다. 기록은 vault 밖(사용자 홈)에 있고 읽기만 한다.
//!
//! 세션 id로 Claude 기록을 못 찾으면 Codex, Gemini CLI, Antigravity 기록을 차례로 찾는다. 찾으면 응답에 `agent`(Antigravity는 `client`도)를 더한다.

use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;
use serde_json::{Map, Value as Json, json};

use super::option_text::text_of;
use crate::error::EngineError;
use crate::session::claude::{self, NOTE_ID, SESSION_ID};
use crate::session::{self, urls_json};
use crate::text::{is_blank, trim};

static SESSION_ID_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(SESSION_ID).unwrap());
static NOTE_ID_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(NOTE_ID).unwrap());

const READ_SESSION_USAGE: &str = "read-session <세션 id> [limit=<개수>] [before=<순번>] [urls=true] [note=<노트 id>] [author=<노트 id>,...]";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ReadSessionRequest {
    /// 세션 id
    pub id: Option<String>,
    pub limit: Option<serde_json::Value>,
    pub before: Option<serde_json::Value>,
    pub urls: Option<serde_json::Value>,
    pub note: Option<serde_json::Value>,
    pub author: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SessionTitlesRequest {
    /// 세션 id들(명령줄에서는 자리 인자 여럿)
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RecentSessionsRequest {
    pub days: Option<serde_json::Value>,
}

/// 앞뒤를 걷은 옵션 글. 걷어서 비면 안 준 것이다.
fn given(value: &Option<Json>) -> Option<(String, String)> {
    let raw = text_of(value)?;
    let trimmed = trim(&raw).to_string();
    (!trimmed.is_empty()).then_some((raw, trimmed))
}

/// `Integer.parseInt`: 부호(선택)와 ASCII 숫자뿐이고 32비트 안이어야 한다.
fn parse_int(text: &str) -> Option<i32> {
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse::<i32>().ok()
}

fn validate_session_id(id: &str) -> Result<(), EngineError> {
    if !SESSION_ID_PATTERN.is_match(id) {
        return Err(EngineError::invalid_argument(format!(
            "세션 식별자 모양이 아니에요: {id}"
        )));
    }
    Ok(())
}

fn validate_note_id(id: &str) -> Result<(), EngineError> {
    if !NOTE_ID_PATTERN.is_match(id) {
        return Err(EngineError::invalid_argument(format!(
            "노트 id 모양이 아니에요: {id}"
        )));
    }
    Ok(())
}

pub fn read_session(request: &ReadSessionRequest) -> Result<Json, EngineError> {
    let Some(session_id) = request.id.as_deref() else {
        return Err(EngineError::invalid_argument(format!(
            "인자가 하나 필요하다: {READ_SESSION_USAGE}"
        )));
    };
    // 자리 인자는 안 걷는다. 앞뒤에 공백이 붙은 세션 id는 꼴이 아니라서 AIS-400이다(7.2)
    validate_session_id(session_id)?;

    let mut limit = 100;
    if let Some((raw, trimmed)) = given(&request.limit) {
        limit = parse_int(&trimmed).ok_or_else(|| {
            EngineError::invalid_argument(format!("limit 은 숫자여야 한다: {raw}"))
        })?;
    }
    let mut before: Option<i32> = None;
    if let Some((raw, trimmed)) = given(&request.before) {
        before = Some(parse_int(&trimmed).ok_or_else(|| {
            EngineError::invalid_argument(format!("before 은 숫자여야 한다: {raw}"))
        })?);
    }
    let mut include_urls = false;
    if let Some((raw, trimmed)) = given(&request.urls) {
        include_urls = match trimmed.as_str() {
            "true" => true,
            "false" => false,
            _ => {
                return Err(EngineError::invalid_argument(format!(
                    "urls 은 true 나 false 여야 한다: {raw}"
                )));
            }
        };
    }
    let mut note_id: Option<String> = None;
    if let Some((_, trimmed)) = given(&request.note) {
        validate_note_id(&trimmed)?;
        note_id = Some(trimmed);
    }
    let mut author_notes: Option<Vec<String>> = None;
    if let Some((_, trimmed)) = given(&request.author) {
        let parts: Vec<&str> = trimmed.split(',').collect();
        if parts.len() > 8 {
            return Err(EngineError::invalid_argument(format!(
                "author 의 노트 id 는 1개에서 8개까지예요: {}",
                parts.len()
            )));
        }
        // 7.8, 7.15.7: 조각의 앞뒤 공백은 안 걷는다
        for part in &parts {
            validate_note_id(part)?;
        }
        author_notes = Some(parts.iter().map(|p| p.to_string()).collect());
    }

    let projects_dir = session::claude_projects_dir();
    // Claude 기록이 없으면 Codex와 Gemini 기록을 찾는다. 찾으면 응답에 agent를 더한다(claude는 키가 없다)
    let other = if claude::find_transcript(projects_dir.as_deref(), session_id).is_none() {
        session::locate(session_id)
    } else {
        None
    };
    let result = match &other {
        Some(located) => {
            let mut r =
                session::read_located(located, limit, before, include_urls, note_id.as_deref())
                    .map_err(|m| {
                        EngineError::internal(format!(
                            "사이드카 안에서 실패했다: {m}. 자세한 것은 사이드카 로그에 있다."
                        ))
                    })?;
            r.session_id = session_id.to_string();
            r
        }
        None => claude::read_session(
            projects_dir.as_deref(),
            session_id,
            limit,
            before,
            include_urls,
            note_id.as_deref(),
            author_notes.as_deref(),
        )
        .map_err(|m| {
            EngineError::internal(format!(
                "사이드카 안에서 실패했다: {m}. 자세한 것은 사이드카 로그에 있다."
            ))
        })?,
    };

    let mut resp = Map::new();
    resp.insert("sessionId".into(), json!(result.session_id));
    resp.insert("file".into(), json!(result.file));
    if let Some(located) = &other {
        resp.insert("agent".into(), json!(located.agent));
        if let Some(client) = located.client() {
            resp.insert("client".into(), json!(client));
        }
    }
    resp.insert(
        "source".into(),
        json!({"bytes": result.bytes, "lines": result.lines, "skipped": result.skipped, "messages": result.messages}),
    );
    resp.insert("hasMore".into(), json!(result.has_more));
    resp.insert("urls".into(), Json::Array(urls_json(&result.urls)));
    resp.insert("runningAgents".into(), Json::Array(result.running_agents));
    resp.insert("todos".into(), Json::Array(result.todos));
    resp.insert("runState".into(), result.run_state.unwrap_or(Json::Null));
    if text_of(&request.note).is_some_and(|n| !is_blank(&n)) {
        resp.insert("noteMentions".into(), json!(result.note_mentions));
    }
    if text_of(&request.author).is_some_and(|a| !is_blank(&a)) {
        resp.insert("author".into(), result.author.unwrap_or(Json::Null));
    }
    resp.insert("turns".into(), Json::Array(result.turns));
    Ok(Json::Object(resp))
}

pub fn session_titles(request: &SessionTitlesRequest) -> Result<Json, EngineError> {
    let ids = &request.ids;
    if ids.len() > 200 {
        return Err(EngineError::invalid_argument(format!(
            "세션을 한 번에 200개까지 물을 수 있어요: {}",
            ids.len()
        )));
    }
    // 읽기 전에 모두 검증(7.10)
    for id in ids {
        validate_session_id(id)?;
    }
    if ids.is_empty() {
        return Ok(json!({"sessions": []}));
    }
    let projects_dir = session::claude_projects_dir();
    let mut sessions: Vec<Json> = Vec::new();
    for id in ids {
        let info = claude::read_title_info(projects_dir.as_deref(), id);
        if !info.found
            && let Some(located) = session::locate(id)
        {
            let found = session::info(&located);
            let mut item = Map::new();
            item.insert("sessionId".into(), json!(id));
            item.insert("found".into(), json!(found.found));
            item.insert("title".into(), json!(found.title));
            item.insert("branch".into(), Json::Null);
            item.insert("cwd".into(), json!(found.cwd));
            item.insert("agent".into(), json!(found.agent));
            if let Some(client) = located.client() {
                item.insert("client".into(), json!(client));
            }
            sessions.push(Json::Object(item));
            continue;
        }
        sessions.push(json!({
            "sessionId": id,
            "found": info.found,
            "title": info.title,
            "branch": info.branch,
            "cwd": info.cwd,
        }));
    }
    Ok(json!({"sessions": sessions}))
}

pub fn recent_sessions(request: &RecentSessionsRequest) -> Result<Json, EngineError> {
    let mut days = 7;
    if let Some((raw, trimmed)) = given(&request.days) {
        days = parse_int(&trimmed).ok_or_else(|| {
            EngineError::invalid_argument(format!("days 은 숫자여야 한다: {raw}"))
        })?;
        if !(1..=30).contains(&days) {
            return Err(EngineError::invalid_argument(format!(
                "days 는 1 부터 30 사이여야 해요: {raw}"
            )));
        }
    }
    let recent = claude::recent_sessions(session::claude_projects_dir().as_deref(), days);
    let sessions: Vec<Json> = recent
        .sessions
        .iter()
        .map(|e| {
            json!({
                "sessionId": e.session_id,
                "lastActiveAt": e.last_active_at,
                "startedAt": e.started_at,
                "title": e.title,
                "branch": e.branch,
                "cwd": e.cwd,
            })
        })
        .collect();
    Ok(json!({"days": recent.days, "dropped": recent.dropped, "sessions": sessions}))
}
