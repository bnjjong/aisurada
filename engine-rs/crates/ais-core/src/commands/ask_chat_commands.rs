//! 노트에 붙는 문답과 세션 레일 대화를 다루는 명령 4개: `get_ask`, `append_ask_turn`, `list_chats`, `append_chat_turn`.
//! 외부 설명서 2장(문답 표식과 `ask.json`)과 8장(레일 대화 `chats/<대화 id>.json`)을 따른다. 인자 이름, 검사 순서, 오류 문구와 응답 필드는
//! Java 엔진의 `AskCommands`·`ChatCommands`와 같고, 파일 글은 바이트까지 같다.
//!
//! 두 파일은 노트(또는 후속 노트) 폴더 안에 둔다. 읽을 때는 너그럽다: 모르는 칸은 버리고, 깨진 UTF-8은 대체 글자로 읽고, 틀린 모양의 칸은
//! 기본값으로 읽는다. 쓸 때는 읽은 것(이 모양으로 다듬은 것)에 새 회차를 붙여 다시 쓴다.

use crate::kernel::time::parse_instant;
use crate::ports::fs;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;
use serde_json::{Map, Value as Json, json};

use super::option_text::text_of;
use crate::error::EngineError;
use crate::json::{self, JsonValue};
use crate::text::{NOTE_ID, SUB_NOTE_ID, compare_utf16, is_blank, trim, utf16_len};
use crate::vault::index::NoteIndex;
use crate::vault::scanner::NoteRecord;
use crate::write::atomic::write_atomic;
use crate::write::json_out::{literal, string, to_file_bytes};
use crate::write::pipeline::WriteScope;

const MAX_FILE_BYTES: u64 = 1_000_000;
const MAX_QUOTE_CHARS: usize = 2_000;
const MAX_TEXT_CHARS: usize = 20_000;
const MAX_IMAGES: usize = 8;
const MAX_ATTACHMENT_NAME_LENGTH: usize = 120;
const IMAGE_EXTENSIONS: [&str; 8] = ["svg", "png", "jpg", "jpeg", "gif", "webp", "avif", "bmp"];

static CHAT_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_-]{1,64}$").unwrap());

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NoteRefRequest {
    /// 노트 id
    pub id: String,
    /// 후속 노트 id(없으면 노트에 붙는다)
    pub sub_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppendAskTurnRequest {
    pub id: String,
    pub sub_id: Option<String>,
    #[serde(rename = "session-id", alias = "sessionId")]
    pub session_id: Option<Json>,
    #[serde(rename = "turn-id", alias = "turnId")]
    pub turn_id: Option<Json>,
    #[serde(rename = "mark-id", alias = "markId")]
    pub mark_id: Option<Json>,
    #[serde(rename = "asked-at", alias = "askedAt")]
    pub asked_at: Option<Json>,
    pub quote: Option<Json>,
    pub question: Option<Json>,
    pub answer: Option<Json>,
    pub error: Option<Json>,
    pub model: Option<Json>,
    pub effort: Option<Json>,
    #[serde(rename = "elapsed-ms", alias = "elapsedMs")]
    pub elapsed_ms: Option<Json>,
    #[serde(rename = "session-lines", alias = "sessionLines")]
    pub session_lines: Option<Json>,
    /// 호출 객체에서는 배열, 명령줄에서는 쉼표로 이은 글
    pub images: Option<Json>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppendChatTurnRequest {
    pub id: String,
    pub sub_id: Option<String>,
    #[serde(rename = "chat-id", alias = "chatId")]
    pub chat_id: Option<Json>,
    #[serde(rename = "source-session", alias = "sourceSession")]
    pub source_session: Option<Json>,
    #[serde(rename = "turn-id", alias = "turnId")]
    pub turn_id: Option<Json>,
    #[serde(rename = "asked-at", alias = "askedAt")]
    pub asked_at: Option<Json>,
    pub question: Option<Json>,
    pub answer: Option<Json>,
    pub error: Option<Json>,
    pub model: Option<Json>,
    pub effort: Option<Json>,
    #[serde(rename = "elapsed-ms", alias = "elapsedMs")]
    pub elapsed_ms: Option<Json>,
    pub images: Option<Json>,
}

// ---------------------------------------------------------------- 대상 노트 찾기

/// 자리 인자의 있고 없음과 id 모양만 검사한다. 색인을 안 보므로 이름 인자 검사 앞에 세울 수 있다.
fn check_ids(
    id: &str,
    sub_id: &Option<String>,
    command: &str,
) -> Result<(String, Option<String>), EngineError> {
    if is_blank(id) {
        return Err(EngineError::invalid_argument(format!(
            "인자가 하나 필요하다: {command} <노트 id> [하위 노트 id]"
        )));
    }
    if let Some(sub) = sub_id
        && is_blank(sub)
    {
        return Err(EngineError::invalid_argument(
            "하위 노트 id 칸이 비어 있다. 노트에 붙이려면 그 칸을 빼라",
        ));
    }
    if !NOTE_ID.is_match(id) {
        return Err(EngineError::invalid_argument(format!(
            "노트 id 모양이 아니다: {id}"
        )));
    }
    if let Some(sub) = sub_id
        && !SUB_NOTE_ID.is_match(sub)
    {
        return Err(EngineError::invalid_argument(format!(
            "하위 노트 id 모양이 아니다: {sub}"
        )));
    }
    Ok((id.to_string(), sub_id.clone()))
}

/// 대상 노트의 id와 폴더. `what`은 단일 파일 노트에서 둘 수 없다고 알릴 때 쓰는 말(조사까지: 「문답을」)이다.
struct Target {
    note_id: String,
    dir: PathBuf,
}

fn locate(
    index: &NoteIndex,
    ids: &(String, Option<String>),
    what: &str,
) -> Result<Target, EngineError> {
    let root = index
        .note(&ids.0)
        .ok_or_else(|| EngineError::not_found(format!("Note not found: {}", ids.0)))?;
    let (note, label): (&NoteRecord, &str) = match &ids.1 {
        None => (root, "노트"),
        Some(sub_id) => (
            root.sub_notes
                .iter()
                .find(|s| &s.id == sub_id)
                .ok_or_else(|| EngineError::not_found(format!("SubNote not found: {sub_id}")))?,
            "하위 노트",
        ),
    };
    if note.is_single_file {
        return Err(EngineError::internal(format!(
            "폴더로 안 올라간 단일 파일 {label}에는 {what} 둘 수 없다: {}",
            note.content_file.display()
        )));
    }
    Ok(Target {
        note_id: note.id.clone(),
        dir: note
            .content_file
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default(),
    })
}

fn rel(vault: &Path, file: &Path) -> String {
    file.strip_prefix(vault)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

// ---------------------------------------------------------------- Java 값 읽기 규칙

/// `Long.parseLong`: 부호(선택)와 숫자 글자뿐이고 64비트 안이어야 한다.
fn parse_java_long(text: &str) -> Option<i64> {
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse::<i64>().ok()
}

/// `Integer.parseInt`: 부호(선택)와 숫자 글자뿐이고 32비트 안이어야 한다.
fn parse_java_int(text: &str) -> Option<i32> {
    parse_java_long(text).and_then(|n| i32::try_from(n).ok())
}

fn field<'a>(entries: &'a [(String, JsonValue)], key: &str) -> Option<&'a JsonValue> {
    entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// 글 칸. 글이 아니면 `None`이다.
fn str_field<'a>(entries: &'a [(String, JsonValue)], key: &str) -> Option<&'a str> {
    match field(entries, key) {
        Some(JsonValue::Str(s)) => Some(s),
        _ => None,
    }
}

/// 정수 칸. 정수 글자(`7600`)와 정수가 든 글(`"7600"`)만 읽고 `7600.5`나 `1e3`는 안 읽는다(없는 것은 0).
fn elapsed_field(entries: &[(String, JsonValue)]) -> i64 {
    match field(entries, "elapsedMs") {
        Some(JsonValue::Literal(raw)) => parse_java_long(raw).unwrap_or(0),
        Some(JsonValue::Str(s)) => parse_java_long(s).unwrap_or(0),
        _ => 0,
    }
}

/// 그림 첨부 이름(2.8)인지 잰다. 비지 않고, 120자를 넘지 않고, 빗금과 역빗금과 `..`이 없고, 점으로 시작하지 않고,
/// 제어 글자가 없고, 확장자가 그림 8가지 가운데 하나여야 한다. 문답 파일과 레일 대화 파일이 같은 규칙을 쓴다.
fn is_valid_attachment_name(name: &str) -> bool {
    if name.is_empty() || utf16_len(name) > MAX_ATTACHMENT_NAME_LENGTH {
        return false;
    }
    if name.starts_with('.') || name.contains('/') || name.contains('\\') || name.contains("..") {
        return false;
    }
    if name.chars().any(|c| (c as u32) < 32) {
        return false;
    }
    match name.rfind('.') {
        None => false,
        Some(dot) => IMAGE_EXTENSIONS.contains(&name[dot + 1..].to_lowercase().as_str()),
    }
}

/// 파일에서 읽은 `images`: `attach/<이름>` 꼴이고 이름이 맞는 것만 남긴다. 하나도 없으면 `None`이다.
fn images_field(entries: &[(String, JsonValue)]) -> Option<Vec<String>> {
    let Some(JsonValue::Array(items)) = field(entries, "images") else {
        return None;
    };
    let valid: Vec<String> = items
        .iter()
        .filter_map(|item| match item {
            JsonValue::Str(s) => s
                .strip_prefix("attach/")
                .filter(|name| is_valid_attachment_name(name))
                .map(|_| s.clone()),
            _ => None,
        })
        .collect();
    (!valid.is_empty()).then_some(valid)
}

/// 명령줄의 `images=<attach/이름,…>`를 읽는다. 쉼표로 가르고 조각마다 앞뒤 공백을 걷고 빈 조각을 버린 뒤에 센다.
fn images_option(
    raw: &Option<Json>,
    too_many: impl Fn(usize) -> String,
) -> Result<Option<Vec<String>>, EngineError> {
    let text = match raw {
        Some(Json::Array(items)) => Some(
            items
                .iter()
                .map(|i| match i {
                    Json::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .collect::<Vec<_>>()
                .join(","),
        ),
        other => text_of(other),
    };
    let Some(text) = text else {
        return Ok(None);
    };
    let valid: Vec<String> = text
        .split(',')
        .map(trim)
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect();
    if valid.len() > MAX_IMAGES {
        return Err(EngineError::invalid_argument(too_many(valid.len())));
    }
    for img in &valid {
        let ok = img
            .strip_prefix("attach/")
            .is_some_and(is_valid_attachment_name);
        if !ok {
            return Err(EngineError::invalid_argument(format!(
                "그림 자리가 attach/<그림 파일 이름> 꼴이 아니다: {img}"
            )));
        }
    }
    Ok((!valid.is_empty()).then_some(valid))
}

fn require_trimmed(value: &Option<Json>, message: &str) -> Result<String, EngineError> {
    match text_of(value) {
        Some(v) if !is_blank(&v) => Ok(trim(&v).to_string()),
        _ => Err(EngineError::invalid_argument(message)),
    }
}

fn require_raw(value: &Option<Json>, message: &str) -> Result<String, EngineError> {
    text_of(value).ok_or_else(|| EngineError::invalid_argument(message))
}

fn read_text(path: &Path) -> Option<String> {
    fs::read(path)
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
}

fn images_json(images: &Option<Vec<String>>) -> Option<JsonValue> {
    images
        .as_ref()
        .map(|v| JsonValue::Array(v.iter().map(string).collect()))
}

// ---------------------------------------------------------------- 문답(ask.json)

/// 문답 한 회차
struct AskTurn {
    id: String,
    mark_id: Option<String>,
    asked_at: String,
    quote: Option<String>,
    question: String,
    answer: String,
    error: Option<String>,
    model: String,
    effort: String,
    elapsed_ms: i64,
    session_lines: Option<i32>,
    images: Option<Vec<String>>,
}

impl AskTurn {
    fn read(entries: &[(String, JsonValue)]) -> AskTurn {
        let text = |key: &str| str_field(entries, key).unwrap_or("").to_string();
        let non_empty = |key: &str| {
            str_field(entries, key)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        // 정수와 정수 글을 Int 범위로 읽고 양수만 남긴다
        let session_lines = match field(entries, "sessionLines") {
            Some(JsonValue::Literal(raw)) => {
                parse_java_long(raw).and_then(|n| i32::try_from(n).ok())
            }
            Some(JsonValue::Str(s)) => parse_java_int(s),
            _ => None,
        }
        .filter(|n| *n > 0);
        AskTurn {
            id: text("id"),
            mark_id: non_empty("markId"),
            asked_at: text("askedAt"),
            quote: non_empty("quote"),
            question: text("question"),
            answer: text("answer"),
            error: non_empty("error"),
            model: text("model"),
            effort: text("effort"),
            elapsed_ms: elapsed_field(entries),
            session_lines,
            images: images_field(entries),
        }
    }

    fn to_file(&self) -> JsonValue {
        let mut m: Vec<(String, JsonValue)> = vec![("id".into(), string(&self.id))];
        if let Some(mark) = &self.mark_id {
            m.push(("markId".into(), string(mark)));
        }
        m.push(("askedAt".into(), string(&self.asked_at)));
        if let Some(quote) = &self.quote {
            m.push(("quote".into(), string(quote)));
        }
        m.push(("question".into(), string(&self.question)));
        m.push(("answer".into(), string(&self.answer)));
        if let Some(error) = &self.error {
            m.push(("error".into(), string(error)));
        }
        m.push(("model".into(), string(&self.model)));
        m.push(("effort".into(), string(&self.effort)));
        m.push(("elapsedMs".into(), literal(self.elapsed_ms.to_string())));
        if let Some(lines) = self.session_lines {
            m.push(("sessionLines".into(), literal(lines.to_string())));
        }
        if let Some(images) = images_json(&self.images) {
            m.push(("images".into(), images));
        }
        JsonValue::Object(m)
    }

    fn to_response(&self) -> Json {
        let mut m = Map::new();
        m.insert("id".into(), json!(self.id));
        if let Some(mark) = &self.mark_id {
            m.insert("markId".into(), json!(mark));
        }
        m.insert("askedAt".into(), json!(self.asked_at));
        if let Some(quote) = &self.quote {
            m.insert("quote".into(), json!(quote));
        }
        m.insert("question".into(), json!(self.question));
        m.insert("answer".into(), json!(self.answer));
        if let Some(error) = &self.error {
            m.insert("error".into(), json!(error));
        }
        m.insert("model".into(), json!(self.model));
        m.insert("effort".into(), json!(self.effort));
        m.insert("elapsedMs".into(), json!(self.elapsed_ms));
        if let Some(lines) = self.session_lines {
            m.insert("sessionLines".into(), json!(lines));
        }
        if let Some(images) = &self.images {
            m.insert("images".into(), json!(images));
        }
        Json::Object(m)
    }
}

struct AskThread {
    note_id: String,
    session_id: String,
    turns: Vec<AskTurn>,
}

/// `ask.json`을 너그럽게 읽는다.
fn read_ask(file: &Path, fallback_note_id: &str) -> Result<AskThread, EngineError> {
    let failed =
        |message: String| EngineError::internal(format!("문답 파일을 못 읽었다: {message}"));
    let size = fs::metadata(file).map_err(|e| failed(e.to_string()))?.len();
    if size > MAX_FILE_BYTES {
        return Err(failed(format!(
            "파일이 너무 크다({size}바이트). 상한은 1000000바이트다."
        )));
    }
    // 깨진 UTF-8 바이트는 대체 글자로 읽는다. 바이트 하나로 문답 전체를 잃지 않게 한다(2.5)
    let content = read_text(file).ok_or_else(|| failed("파일을 못 읽었다".to_string()))?;
    let JsonValue::Object(entries) = json::parse_file(&content).map_err(failed)? else {
        return Err(failed("최상위가 객체가 아니다".to_string()));
    };
    let note_id = str_field(&entries, "noteId")
        .filter(|s| !s.is_empty())
        .unwrap_or(fallback_note_id)
        .to_string();
    let session_id = str_field(&entries, "sessionId").unwrap_or("").to_string();
    let turns = match field(&entries, "turns") {
        Some(JsonValue::Array(items)) => items
            .iter()
            .filter_map(|item| match item {
                JsonValue::Object(m) => Some(AskTurn::read(m)),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    Ok(AskThread {
        note_id,
        session_id,
        turns,
    })
}

fn ask_response(thread: &AskThread) -> Map<String, Json> {
    let mut m = Map::new();
    m.insert("noteId".into(), json!(thread.note_id));
    m.insert("sessionId".into(), json!(thread.session_id));
    m.insert(
        "turns".into(),
        Json::Array(thread.turns.iter().map(AskTurn::to_response).collect()),
    );
    m
}

/// 노트의 문답을 읽는다. 문답 파일이 없으면 `null`이다.
pub fn get_ask(vault: &Path, request: &NoteRefRequest) -> Result<Json, EngineError> {
    let ids = check_ids(&request.id, &request.sub_id, "get-ask")?;
    let index = NoteIndex::fresh(vault);
    let target = locate(&index, &ids, "문답을")?;
    let file = target.dir.join("ask.json");
    if !fs::exists(&file) {
        return Ok(Json::Null);
    }
    Ok(Json::Object(ask_response(&read_ask(
        &file,
        &target.note_id,
    )?)))
}

/// 문답에 회차 하나를 붙인다. 인자를 모두 검사한 뒤에야 노트를 찾고, 파일이 100만 바이트를 넘게 되면 붙이지 않는다.
pub fn append_ask_turn(
    vault: &Path,
    request: &AppendAskTurnRequest,
    scope: &WriteScope,
) -> Result<Json, EngineError> {
    // 자리 인자 모양, 이름 인자 순으로 먼저 재고 나서야 색인에서 노트를 찾는다(2.7의 하는 일)
    let ids = check_ids(&request.id, &request.sub_id, "append-ask-turn")?;
    let required =
        |value: &Option<Json>, key: &str| require_trimmed(value, &format!("{key} 이 필요하다"));
    let session_id = required(&request.session_id, "session-id")?;
    let turn_id = required(&request.turn_id, "turn-id")?;
    let mark_id = required(&request.mark_id, "mark-id")?;
    let asked_at = required(&request.asked_at, "asked-at")?;
    let question = require_raw(&request.question, "question 이 필요하다")?;
    let answer = require_raw(&request.answer, "answer 이 필요하다")?;
    let model = required(&request.model, "model")?;
    let effort = required(&request.effort, "effort")?;
    let raw_elapsed = required(&request.elapsed_ms, "elapsed-ms")?;
    // 정수는 32비트다. 2.3의 elapsedMs 와 같은 범위로 쓰고 읽는다
    let elapsed_ms = parse_java_int(&raw_elapsed)
        .filter(|n| *n >= 0)
        .ok_or_else(|| {
            EngineError::invalid_argument(format!("elapsed-ms 은 숫자여야 한다: {raw_elapsed}"))
        })?;

    if utf16_len(&question) > MAX_TEXT_CHARS {
        return Err(EngineError::invalid_argument(format!(
            "질문이 상한을 넘었다: {}자. 상한은 20000자다.",
            utf16_len(&question)
        )));
    }
    if utf16_len(&answer) > MAX_TEXT_CHARS {
        return Err(EngineError::invalid_argument(format!(
            "답변이 상한을 넘었다: {}자. 상한은 20000자다.",
            utf16_len(&answer)
        )));
    }
    let quote = text_of(&request.quote);
    if let Some(quote) = &quote
        && utf16_len(quote) > MAX_QUOTE_CHARS
    {
        return Err(EngineError::invalid_argument(format!(
            "인용이 상한을 넘었다: {}자. 상한은 2000자다.",
            utf16_len(quote)
        )));
    }
    let error = text_of(&request.error);

    let mut session_lines: Option<i32> = None;
    if let Some(raw) = text_of(&request.session_lines).filter(|r| !is_blank(r)) {
        // 걷어서 비면 안 준 인자다
        session_lines = Some(
            parse_java_int(trim(&raw))
                .filter(|n| *n >= 1)
                .ok_or_else(|| {
                    EngineError::invalid_argument(format!(
                        "session-lines 은 1 이상이어야 한다: {raw}"
                    ))
                })?,
        );
    }
    let images = images_option(&request.images, |n| {
        format!("그림은 한 물음에 8장까지다: {n}장")
    })?;

    // 인자가 다 맞은 뒤에야 노트를 찾는다. 없으면 AIS-404, 단일 파일 노트면 AIS-500
    let target = locate(&scope.index, &ids, "문답을")?;
    let file = target.dir.join("ask.json");
    let mut thread = if fs::exists(&file) {
        read_ask(&file, &target.note_id)?
    } else {
        AskThread {
            note_id: target.note_id.clone(),
            session_id,
            turns: Vec::new(),
        }
    };
    thread.turns.push(AskTurn {
        id: turn_id,
        mark_id: Some(mark_id),
        asked_at,
        quote,
        question,
        answer,
        error,
        model,
        effort,
        elapsed_ms: i64::from(elapsed_ms),
        session_lines,
        images,
    });

    let file_map = JsonValue::Object(vec![
        ("v".into(), literal("1")),
        ("noteId".into(), string(&thread.note_id)),
        ("sessionId".into(), string(&thread.session_id)),
        (
            "turns".into(),
            JsonValue::Array(thread.turns.iter().map(AskTurn::to_file).collect()),
        ),
    ]);
    let bytes = to_file_bytes(&file_map);
    // 1,000,000 바이트를 넘는 파일을 쓰기 전에 막는다
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(EngineError::internal(format!(
            "문답 파일이 상한을 넘어 붙일 수 없다: 붙이면 {}바이트다. 상한은 1000000바이트다.",
            bytes.len()
        )));
    }
    write_atomic(&file, &bytes)?;

    let mut out = ask_response(&thread);
    out.insert("path".into(), json!(rel(vault, &file)));
    Ok(Json::Object(out))
}

// ---------------------------------------------------------------- 레일 대화(chats/<id>.json)

/// 레일 대화 한 회차
struct ChatTurn {
    id: String,
    asked_at: String,
    question: String,
    answer: String,
    error: Option<String>,
    model: String,
    effort: String,
    elapsed_ms: i64,
    images: Option<Vec<String>>,
}

impl ChatTurn {
    fn read(entries: &[(String, JsonValue)]) -> ChatTurn {
        let text = |key: &str| str_field(entries, key).unwrap_or("").to_string();
        ChatTurn {
            id: text("id"),
            asked_at: text("askedAt"),
            question: text("question"),
            answer: text("answer"),
            error: str_field(entries, "error")
                .filter(|s| !s.is_empty())
                .map(str::to_string),
            model: text("model"),
            effort: text("effort"),
            elapsed_ms: elapsed_field(entries),
            images: images_field(entries),
        }
    }

    fn to_file(&self) -> JsonValue {
        let mut m: Vec<(String, JsonValue)> = vec![
            ("id".into(), string(&self.id)),
            ("askedAt".into(), string(&self.asked_at)),
            ("question".into(), string(&self.question)),
            ("answer".into(), string(&self.answer)),
        ];
        if let Some(error) = &self.error {
            // 빈 error= 도 "error": "" 로 쓴다. 읽을 때 빈 글은 없는 칸이다
            m.push(("error".into(), string(error)));
        }
        m.push(("model".into(), string(&self.model)));
        m.push(("effort".into(), string(&self.effort)));
        m.push(("elapsedMs".into(), literal(self.elapsed_ms.to_string())));
        if let Some(images) = images_json(&self.images) {
            m.push(("images".into(), images));
        }
        JsonValue::Object(m)
    }

    fn to_response(&self) -> Json {
        let mut m = Map::new();
        m.insert("id".into(), json!(self.id));
        m.insert("askedAt".into(), json!(self.asked_at));
        m.insert("question".into(), json!(self.question));
        m.insert("answer".into(), json!(self.answer));
        if let Some(error) = &self.error {
            m.insert("error".into(), json!(error));
        }
        m.insert("model".into(), json!(self.model));
        m.insert("effort".into(), json!(self.effort));
        m.insert("elapsedMs".into(), json!(self.elapsed_ms));
        if let Some(images) = &self.images {
            m.insert("images".into(), json!(images));
        }
        Json::Object(m)
    }
}

struct Chat {
    note_id: String,
    chat_id: String,
    source_session_id: String,
    created_at: String,
    turns: Vec<ChatTurn>,
}

impl Chat {
    fn to_response(&self) -> Json {
        json!({
            "noteId": self.note_id,
            "chatId": self.chat_id,
            "sourceSessionId": self.source_session_id,
            "createdAt": self.created_at,
            "turns": self.turns.iter().map(ChatTurn::to_response).collect::<Vec<_>>(),
        })
    }
}

/// `chats/<대화 id>.json`을 너그럽게 읽는다. 최상위 `v`는 돌려주지 않고, 대화 id는 파일 이름이 이긴다.
/// 실패하면 이유 글을 돌려준다.
fn read_chat(file: &Path, fallback_note_id: &str, file_chat_id: &str) -> Result<Chat, String> {
    // 깨진 UTF-8 바이트는 대체 글자로 읽는다. 바이트 하나로 대화 전체를 못 읽는 파일로 치지 않는다(8.11)
    let content = read_text(file).ok_or_else(|| "파일을 못 읽었다".to_string())?;
    let JsonValue::Object(entries) = json::parse_file(&content)? else {
        return Err("최상위 객체가 아니다".to_string());
    };
    let note_id = str_field(&entries, "noteId")
        .filter(|s| !is_blank(s))
        .unwrap_or(fallback_note_id)
        .to_string();
    let turns: Vec<ChatTurn> = match field(&entries, "turns") {
        Some(JsonValue::Array(items)) => items
            .iter()
            .filter_map(|item| match item {
                JsonValue::Object(m) => Some(ChatTurn::read(m)),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    let created_at = match str_field(&entries, "createdAt").filter(|s| !is_blank(s)) {
        Some(s) => s.to_string(),
        None => turns
            .first()
            .map(|t| t.asked_at.clone())
            .unwrap_or_default(),
    };
    Ok(Chat {
        note_id,
        chat_id: file_chat_id.to_string(),
        source_session_id: str_field(&entries, "sourceSessionId")
            .unwrap_or("")
            .to_string(),
        created_at,
        turns,
    })
}

/// 노트의 레일 대화 목록. 만든 시각 오름차순(같으면 대화 id 순)이고, 못 읽은 파일 이름은 따로 돌려준다.
pub fn list_chats(vault: &Path, request: &NoteRefRequest) -> Result<Json, EngineError> {
    let ids = check_ids(&request.id, &request.sub_id, "list-chats")?;
    let index = NoteIndex::fresh(vault);
    let target = locate(&index, &ids, "레일 대화를")?;
    let chats_dir = target.dir.join("chats");

    let mut chats: Vec<Chat> = Vec::new();
    let mut unreadable: Vec<String> = Vec::new();
    if fs::exists(&chats_dir) {
        if !fs::metadata(&chats_dir).map(|m| m.is_dir()).unwrap_or(false) {
            return Err(EngineError::internal(format!(
                "chats 경로가 디렉토리가 아니다: {}",
                chats_dir.display()
            )));
        }
        let entries = fs::read_dir(&chats_dir)
            .map_err(|e| EngineError::internal(format!("chats 폴더를 읽지 못했다: {e}")))?;
        for entry in entries.flatten() {
            let path = entry.path();
            // 심볼릭 링크는 따라가지 않는다
            if !fs::symlink_metadata(&path).is_ok_and(|m| m.is_file()) {
                continue;
            }
            let Ok(filename) = entry.file_name().into_string() else {
                continue;
            };
            let Some(chat_id) = filename.strip_suffix(".json") else {
                continue;
            };
            if !CHAT_ID.is_match(chat_id) {
                continue;
            }
            let readable = fs::metadata(&path)
                .is_ok_and(|m| m.len() <= MAX_FILE_BYTES)
                .then(|| read_chat(&path, &target.note_id, chat_id).ok())
                .flatten();
            match readable {
                Some(chat) => chats.push(chat),
                None => unreadable.push(filename),
            }
        }
    }
    chats.sort_by(|a, b| {
        compare_created_at(&a.created_at, &b.created_at)
            .then_with(|| compare_utf16(&a.chat_id, &b.chat_id))
    });
    unreadable.sort_by(|a, b| compare_utf16(a, b));
    Ok(json!({
        "chats": chats.iter().map(Chat::to_response).collect::<Vec<_>>(),
        "unreadable": unreadable,
    }))
}

/// 레일 대화에 회차 하나를 붙인다(대화 파일이 없으면 새로 만든다). 인자를 모두 검사한 뒤에야 노트를 찾는다.
pub fn append_chat_turn(
    vault: &Path,
    request: &AppendChatTurnRequest,
    scope: &WriteScope,
) -> Result<Json, EngineError> {
    // 자리 인자 모양, 이름 인자 순으로 먼저 재고 나서야 색인에서 노트를 찾는다(8.13의 붙이는 차례)
    let ids = check_ids(&request.id, &request.sub_id, "append-chat-turn")?;
    let chat_id = require_trimmed(&request.chat_id, "chat-id 이 필요하다")?;
    if !CHAT_ID.is_match(&chat_id) {
        return Err(EngineError::invalid_argument(format!(
            "대화 id 모양이 아니다: {chat_id}"
        )));
    }
    let source_session = require_trimmed(&request.source_session, "source-session 이 필요하다")?;
    if utf16_len(&source_session) > 128 {
        return Err(EngineError::invalid_argument(
            "source-session 은 128자 이하여야 한다",
        ));
    }
    let turn_id = require_trimmed(&request.turn_id, "turn-id 이 필요하다")?;
    let asked_at = require_trimmed(&request.asked_at, "asked-at 이 필요하다")?;
    let question = require_raw(&request.question, "question 이 필요하다")?;
    if utf16_len(&question) > MAX_TEXT_CHARS {
        return Err(EngineError::invalid_argument(format!(
            "질문은 {MAX_TEXT_CHARS}자 이하여야 한다"
        )));
    }
    let answer = require_raw(&request.answer, "answer 가 필요하다")?;
    if utf16_len(&answer) > MAX_TEXT_CHARS {
        return Err(EngineError::invalid_argument(format!(
            "답은 {MAX_TEXT_CHARS}자 이하여야 한다"
        )));
    }
    let error = text_of(&request.error);
    let model = require_trimmed(&request.model, "model 이 필요하다")?;
    let effort = require_trimmed(&request.effort, "effort 이 필요하다")?;
    let elapsed_text = require_trimmed(&request.elapsed_ms, "elapsed-ms 가 필요하다")?;
    // 정수는 32비트다. 문답 파일의 elapsed-ms 와 같은 범위다(2.7)
    let elapsed_ms = parse_java_int(&elapsed_text)
        .filter(|n| *n >= 0)
        .ok_or_else(|| EngineError::invalid_argument("elapsed-ms 는 0 이상의 정수여야 한다"))?;
    let images = images_option(&request.images, |_| {
        format!("그림 첨부는 {MAX_IMAGES}개까지만 가능하다")
    })?;

    // 인자가 다 맞은 뒤에야 노트를 찾는다. 없으면 AIS-404, 단일 파일 노트면 AIS-500
    let target = locate(&scope.index, &ids, "레일 대화를")?;
    let file = target.dir.join("chats").join(format!("{chat_id}.json"));
    let rel_path = rel(vault, &file);

    let mut chat = if fs::exists(&file) {
        let size = fs::metadata(&file)
            .map_err(|e| {
                EngineError::internal(format!("레일 대화 파일을 못 읽어 붙일 수 없다: {e}"))
            })?
            .len();
        if size > MAX_FILE_BYTES {
            return Err(EngineError::internal(format!(
                "레일 대화 파일을 못 읽어 붙일 수 없다: 파일이 상한을 넘었다: {size}바이트"
            )));
        }
        read_chat(&file, &target.note_id, &chat_id).map_err(|m| {
            EngineError::internal(format!("레일 대화 파일을 못 읽어 붙일 수 없다: {m}"))
        })?
    } else {
        Chat {
            note_id: target.note_id.clone(),
            chat_id: chat_id.clone(),
            source_session_id: source_session,
            created_at: asked_at.clone(),
            turns: Vec::new(),
        }
    };
    chat.turns.push(ChatTurn {
        id: turn_id,
        asked_at,
        question,
        answer,
        error,
        model,
        effort,
        elapsed_ms: i64::from(elapsed_ms),
        images,
    });

    let file_map = JsonValue::Object(vec![
        ("v".into(), literal("1")),
        ("noteId".into(), string(&chat.note_id)),
        ("chatId".into(), string(&chat_id)),
        ("sourceSessionId".into(), string(&chat.source_session_id)),
        ("createdAt".into(), string(&chat.created_at)),
        (
            "turns".into(),
            JsonValue::Array(chat.turns.iter().map(ChatTurn::to_file).collect()),
        ),
    ]);
    let bytes = to_file_bytes(&file_map);
    // 1,000,000 바이트를 넘는 파일을 쓰기 전에 막는다
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(EngineError::internal(format!(
            "레일 대화 파일이 상한을 넘어 붙일 수 없다: 붙이면 {}바이트다. 상한은 1000000바이트다: {rel_path}",
            bytes.len()
        )));
    }
    write_atomic(&file, &bytes)?;
    Ok(json!({"chat": chat.to_response(), "path": rel_path}))
}

// ---------------------------------------------------------------- 시각 차례

/// 만든 시각 비교. 읽을 수 있는 시각이 앞이고, 둘 다 못 읽으면 같은 것으로 본다.
fn compare_created_at(a: &str, b: &str) -> Ordering {
    match (parse_instant(a), parse_instant(b)) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_java_integers_strictly() {
        assert_eq!(parse_java_long("+7"), Some(7));
        assert_eq!(parse_java_long("-0"), Some(0));
        assert_eq!(parse_java_long("7600.5"), None);
        assert_eq!(parse_java_long("1e3"), None);
        assert_eq!(parse_java_long(" 7"), None);
        assert_eq!(parse_java_int("2147483648"), None);
        assert_eq!(parse_java_int("2147483647"), Some(i32::MAX));
    }

    #[test]
    fn orders_instants_across_offsets_and_unreadable_ones_last() {
        assert_eq!(
            compare_created_at("2026-11-14T10:00:00+09:00", "2026-11-14T01:00:00Z"),
            Ordering::Equal
        );
        assert_eq!(
            compare_created_at("2026-11-14T10:00:00+09:00", "2026-11-14T02:00:00Z"),
            Ordering::Less
        );
        assert_eq!(
            compare_created_at("2026-11-14T10:00+09:00", "어제"),
            Ordering::Less
        );
        assert_eq!(compare_created_at("", "어제"), Ordering::Equal);
        assert!(parse_instant("2026-02-30T00:00:00Z").is_none());
        assert!(parse_instant("2024-02-29T00:00:00.5Z").is_some());
        assert_eq!(parse_instant("1970-01-01T00:00:00Z"), Some(0));
    }

    #[test]
    fn checks_attachment_names_like_the_spec() {
        for ok in ["a.png", "한글.JPG", "a.b.svg"] {
            assert!(is_valid_attachment_name(ok), "{ok}");
        }
        for bad in [
            "",
            ".hidden.png",
            "a/b.png",
            "a\\b.png",
            "a..b.png",
            "noext",
            "a.txt",
            "a.",
            "tab\t.png",
        ] {
            assert!(!is_valid_attachment_name(bad), "{bad:?}");
        }
        assert!(!is_valid_attachment_name(&format!(
            "{}.png",
            "a".repeat(117)
        )));
        assert!(is_valid_attachment_name(&format!(
            "{}.png",
            "a".repeat(116)
        )));
    }
}
