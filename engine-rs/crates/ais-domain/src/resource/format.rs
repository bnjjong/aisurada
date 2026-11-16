//! 자원 파일의 모양(머리말 + 지침)을 읽고 쓰고, 인자로 들어온 값을 검사한다. 규칙과 오류 문구는 Java 엔진의 `ResourceFormat`과 같다.
//! 스킬은 `SKILL.md`, 에이전트 정의는 `<이름>.md`이고 둘 다 `---` 머리말(YAML)과 지침 본문이다. 머리말은 SnakeYAML이 쓰는 모양이다.

use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value as Json;

use super::{Resource, ResourceKind};
use crate::error::EngineError;
use crate::frontmatter::{self, Value};
use crate::json::{self, JsonValue};
use crate::kernel::confine::has_unusable_path_characters;
use crate::kernel::java::{java_blank, java_trim, value_to_string};
use crate::kernel::token::compute_token;
use crate::note::slug;
use crate::snake_dump::dump_map;
use crate::text::{is_blank, trim, utf16_len};

pub const MAX_FILE_BYTES: usize = 300_000;
pub const MAX_NAME: usize = 60;
pub const MAX_DESCRIPTION: usize = 1000;
pub const MAX_BODY: usize = 100_000;
pub const MAX_SKILLS: usize = 50;
pub const MAX_TOOL_KEYS: usize = 20;
pub const MAX_TOOLS_CHARS: usize = 20_000;
pub const PERMISSIONS: [&str; 3] = ["read", "edit", "publish"];

static TOOL_KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-z][a-z0-9_-]{0,31}$").unwrap());

fn get<'a>(map: &'a [(Value, Value)], key: &str) -> Option<&'a Value> {
    map.iter()
        .find(|(k, _)| matches!(k, Value::Str(s) if s == key))
        .map(|(_, v)| v)
}

fn str_of(map: &[(Value, Value)], key: &str) -> Option<String> {
    get(map, key).and_then(value_to_string)
}

/// 파일 바이트를 자원으로 읽는다. 머리말이 없거나 못 읽거나 id가 종류와 안 맞으면 이유 글을 돌려준다.
pub fn parse(
    bytes: &[u8],
    kind: ResourceKind,
    project: Option<&str>,
    rel_path: &str,
) -> Result<Resource, String> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(format!("파일이 {MAX_FILE_BYTES}바이트를 넘는다"));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| "UTF-8 글이 아니다".to_string())?
        .replace("\r\n", "\n");
    if !text.starts_with("---\n") {
        return Err("머리말(---)로 시작하지 않는다".to_string());
    }
    let mut close: Option<usize> = None;
    let mut from = 3usize;
    while let Some(found) = text[from..].find("\n---") {
        let i = from + found;
        let after = i + 4;
        if after == text.len() || text.as_bytes()[after] == b'\n' {
            close = Some(i);
            break;
        }
        from = i + 1;
    }
    let Some(close) = close else {
        return Err("머리말을 닫는 ---가 없다".to_string());
    };
    let yaml_text = &text[4..(close + 1).max(4)];
    let mut body_start = (close + 4).min(text.len());
    if body_start < text.len() && text.as_bytes()[body_start] == b'\n' {
        body_start += 1;
    }
    let mut body = text[body_start..].to_string();
    if body.ends_with('\n') {
        body.pop();
    }
    let Some(map) = frontmatter::parse(yaml_text) else {
        return Err("머리말을 YAML로 읽지 못했다".to_string());
    };
    let id = str_of(&map, "id").filter(|id| ResourceKind::of_id(id) == Some(kind));
    let Some(id) = id else {
        return Err(format!(
            "id 가 없거나 {} 모양({}…)이 아니다",
            kind.label(),
            kind.id_prefix()
        ));
    };
    let name = str_of(&map, "name").filter(|n| !java_blank(n));
    let Some(name) = name else {
        return Err("name 이 없다".to_string());
    };
    let description = str_of(&map, "description");
    let (mut permission, mut skills, mut repo) = (None, Vec::new(), None);
    if kind == ResourceKind::Agent {
        permission = Some(match str_of(&map, "permission") {
            Some(p) if PERMISSIONS.contains(&java_trim(&p).to_lowercase().as_str()) => {
                java_trim(&p).to_lowercase()
            }
            _ => "read".to_string(),
        });
        skills = skill_list(get(&map, "skills"));
        repo = str_of(&map, "repo")
            .filter(|r| !java_blank(r))
            .map(|r| java_trim(&r).to_string());
    }
    let mut tools: Vec<(String, Value)> = Vec::new();
    if let Some(Value::Map(entries)) = get(&map, "tools") {
        for (key, value) in entries {
            let key = value_to_string(key).unwrap_or_else(|| "null".to_string());
            match tools.iter_mut().find(|(k, _)| *k == key) {
                Some(slot) => slot.1 = value.clone(),
                None => tools.push((key, value.clone())),
            }
        }
    }
    Ok(Resource {
        id,
        kind,
        project: project.map(str::to_string),
        name: java_trim(&name).to_string(),
        description: description.map_or_else(String::new, |d| java_trim(&d).to_string()),
        permission,
        skills,
        repo,
        tools,
        body,
        edited_at: str_of(&map, "editedAt"),
        edited_session: str_of(&map, "editedSession"),
        edited_agent: str_of(&map, "editedAgent"),
        rel_path: rel_path.to_string(),
        token: Some(compute_token(bytes)),
    })
}

fn skill_list(value: Option<&Value>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(Value::List(items)) = value {
        for item in items {
            if let Some(s) = value_to_string(item)
                && ResourceKind::of_id(&s) == Some(ResourceKind::Skill)
                && !out.contains(&s)
            {
                out.push(s);
            }
        }
    }
    out
}

pub fn render(resource: &Resource) -> Result<Vec<u8>, EngineError> {
    let text = |s: &str| Value::Str(s.to_string());
    let mut map: Vec<(Value, Value)> = vec![
        (text("id"), text(&resource.id)),
        (text("name"), text(&resource.name)),
        (text("description"), text(&resource.description)),
    ];
    if resource.kind == ResourceKind::Agent {
        map.push((
            text("permission"),
            text(resource.permission.as_deref().unwrap_or("read")),
        ));
        if !resource.skills.is_empty() {
            map.push((
                text("skills"),
                Value::List(resource.skills.iter().map(|s| text(s)).collect()),
            ));
        }
        if let Some(repo) = resource.repo.as_deref().filter(|r| !r.is_empty()) {
            map.push((text("repo"), text(repo)));
        }
    }
    if !resource.tools.is_empty() {
        map.push((
            text("tools"),
            Value::Map(
                resource
                    .tools
                    .iter()
                    .map(|(k, v)| (text(k), v.clone()))
                    .collect(),
            ),
        ));
    }
    for (key, value) in [
        ("editedAt", &resource.edited_at),
        ("editedSession", &resource.edited_session),
        ("editedAgent", &resource.edited_agent),
    ] {
        if let Some(value) = value {
            map.push((text(key), text(value)));
        }
    }
    let yaml = dump_map(&map)
        .map_err(|e| EngineError::internal(format!("자원을 YAML로 쓰지 못했다: {e}")))?;
    Ok(format!("---\n{yaml}---\n{}\n", resource.body).into_bytes())
}

// ---------------------------------------------------------------- 인자 검사

fn bad(message: impl Into<String>) -> EngineError {
    EngineError::invalid_argument(message)
}

pub fn name(raw: &str) -> Result<String, EngineError> {
    let n = trim(raw);
    if n.is_empty() {
        return Err(bad("name 이 필요하다"));
    }
    if n.chars().count() > MAX_NAME {
        return Err(bad(format!("name 은 {MAX_NAME}자까지다: {n}")));
    }
    for c in n.chars() {
        if c.is_control() {
            return Err(bad("name 에 제어 문자를 쓸 수 없다"));
        }
        if c == '/' || c == '\\' {
            return Err(bad(format!("name 에는 / 와 \\ 를 쓸 수 없다: {n}")));
        }
    }
    slug_of(n)?;
    Ok(n.to_string())
}

/// 이름에서 만든 파일(폴더) 이름
pub fn slug_of(name: &str) -> Result<String, EngineError> {
    let s = slug::build(name, 50);
    if s.is_empty() || s.starts_with('.') || has_unusable_path_characters(&s) {
        return Err(bad(format!("name 에서 파일 이름을 만들 수 없다: {name}")));
    }
    Ok(s)
}

pub fn description(raw: &str) -> Result<String, EngineError> {
    let d = trim(raw);
    if d.is_empty() {
        return Err(bad("description 이 필요하다"));
    }
    if d.chars().count() > MAX_DESCRIPTION {
        return Err(bad(format!("description 은 {MAX_DESCRIPTION}자까지다")));
    }
    Ok(d.to_string())
}

pub fn body(raw: &str) -> Result<String, EngineError> {
    if utf16_len(raw) > MAX_BODY {
        return Err(bad(format!("body 는 {MAX_BODY}자까지다")));
    }
    if raw.contains('\0') {
        return Err(bad("body 에 널 문자를 쓸 수 없다"));
    }
    Ok(raw.to_string())
}

pub fn permission(raw: &str) -> Result<String, EngineError> {
    let p = java_trim(raw).to_lowercase();
    if !PERMISSIONS.contains(&p.as_str()) {
        return Err(bad(format!(
            "permission 은 [{}] 가운데 하나여야 한다: {raw}",
            PERMISSIONS.join(", ")
        )));
    }
    Ok(p)
}

/// 호출 객체의 JSON 값이나 명령줄의 JSON 글을 읽어 엔진의 JSON 값으로 만든다. 빈 글은 `None`이다.
fn json_arg(raw: &Json, field: &str) -> Result<Option<(JsonValue, usize)>, EngineError> {
    match raw {
        Json::Null => Ok(None),
        Json::String(text) => {
            if is_blank(text) {
                return Ok(None);
            }
            match json::parse(text) {
                Ok(value) => Ok(Some((value, utf16_len(text)))),
                Err(message) => Err(bad(format!("{field} 는 JSON 이어야 한다: {message}"))),
            }
        }
        other => {
            let length = utf16_len(&other.to_string());
            Ok(Some((crate::kernel::java::to_json_value(other), length)))
        }
    }
}

/// `skills` 값(스킬 id의 JSON 배열). 빈 값은 빈 목록이다.
pub fn skill_ids(raw: &Json) -> Result<Vec<String>, EngineError> {
    let Some((parsed, _)) = json_arg(raw, "skills")? else {
        return Ok(Vec::new());
    };
    let JsonValue::Array(items) = parsed else {
        return Err(bad("skills 는 JSON 배열이어야 한다"));
    };
    if items.len() > MAX_SKILLS {
        return Err(bad(format!("skills 는 {MAX_SKILLS}개까지다")));
    }
    let mut seen: Vec<String> = Vec::new();
    for item in &items {
        let valid = match item {
            JsonValue::Str(s) if ResourceKind::of_id(s) == Some(ResourceKind::Skill) => s,
            _ => {
                return Err(bad(format!(
                    "skills 의 항목은 스킬 ID(sk…)여야 한다: {}",
                    json::java_record_string(item)
                )));
            }
        };
        if seen.contains(valid) {
            return Err(bad(format!("skills 에 같은 ID 가 겹친다: {valid}")));
        }
        seen.push(valid.clone());
    }
    Ok(seen)
}

/// `tools` 값(JSON 객체)을 도구별 선택 칸으로 읽는다. 칸 이름은 소문자 도구 이름이다. 빈 값은 빈 맵(비우기)이다.
pub fn tools(raw: &Json) -> Result<Vec<(String, Value)>, EngineError> {
    let Some((parsed, length)) = json_arg(raw, "tools")? else {
        return Ok(Vec::new());
    };
    let JsonValue::Object(entries) = parsed else {
        return Err(bad("tools 는 JSON 객체여야 한다"));
    };
    if entries.len() > MAX_TOOL_KEYS {
        return Err(bad(format!("tools 는 {MAX_TOOL_KEYS}칸까지다")));
    }
    let mut out: Vec<(String, Value)> = Vec::new();
    for (key, value) in &entries {
        if !TOOL_KEY.is_match(key) {
            return Err(bad(format!(
                "tools 의 칸 이름은 소문자 도구 이름이어야 한다(예: claude, codex): {key}"
            )));
        }
        out.push((key.clone(), to_value(value)));
    }
    if length > MAX_TOOLS_CHARS {
        return Err(bad(format!("tools 는 {MAX_TOOLS_CHARS}자까지다")));
    }
    Ok(out)
}

/// JSON 값을 YAML 값으로. 따옴표 없는 글은 참거짓·null·정수(18자리까지)·실수 순으로 읽고, 그 밖은 글 그대로 둔다.
pub fn to_value(value: &JsonValue) -> Value {
    match value {
        JsonValue::Str(s) => Value::Str(s.clone()),
        JsonValue::Array(items) => Value::List(items.iter().map(to_value).collect()),
        JsonValue::Object(entries) => Value::Map(
            entries
                .iter()
                .map(|(k, v)| (Value::Str(k.clone()), to_value(v)))
                .collect(),
        ),
        JsonValue::Literal(raw) => match raw.as_str() {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            "null" => Value::Null,
            text if is_long(text) => Value::Int(
                text.parse::<i64>()
                    .map_or_else(|_| text.to_string(), |n| n.to_string()),
            ),
            text if is_decimal(text) => text
                .parse::<f64>()
                .map_or_else(|_| Value::Str(text.to_string()), Value::Float),
            text => Value::Str(text.to_string()),
        },
    }
}

/// `-?\d{1,18}`
fn is_long(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    (1..=18).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit())
}

/// `-?\d+(\.\d+)?([eE][-+]?\d+)?`
fn is_decimal(text: &str) -> bool {
    static DECIMAL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^-?[0-9]+(\.[0-9]+)?([eE][-+]?[0-9]+)?$").unwrap());
    DECIMAL.is_match(text)
}

/// YAML 값을 파일에 쓰는 JSON 값으로(대기 변경의 보여 주기용 칸). 정수는 글 그대로, 실수는 Java `Double.toString` 모양이다.
pub fn value_to_json(value: &Value) -> JsonValue {
    match value {
        Value::Null | Value::Other => JsonValue::Literal("null".to_string()),
        Value::Bool(b) => JsonValue::Literal(b.to_string()),
        Value::Int(digits) => JsonValue::Literal(digits.clone()),
        Value::Float(f) => JsonValue::Literal(crate::kernel::java::java_double_string(*f)),
        Value::Str(s) => JsonValue::Str(s.clone()),
        Value::List(items) => JsonValue::Array(items.iter().map(value_to_json).collect()),
        Value::Map(entries) => JsonValue::Object(
            entries
                .iter()
                .map(|(k, v)| {
                    (
                        value_to_string(k).unwrap_or_else(|| "null".to_string()),
                        value_to_json(v),
                    )
                })
                .collect(),
        ),
    }
}

/// YAML 값을 응답 JSON으로
pub fn value_to_serde(value: &Value) -> Json {
    match value {
        Value::Null | Value::Other => Json::Null,
        Value::Bool(b) => Json::Bool(*b),
        Value::Int(digits) => digits
            .parse::<i64>()
            .map(Json::from)
            .or_else(|_| digits.parse::<u64>().map(Json::from))
            .or_else(|_| digits.parse::<f64>().map(Json::from))
            .unwrap_or_else(|_| Json::String(digits.clone())),
        Value::Float(f) => Json::from(*f),
        Value::Str(s) => Json::String(s.clone()),
        Value::List(items) => Json::Array(items.iter().map(value_to_serde).collect()),
        Value::Map(entries) => Json::Object(
            entries
                .iter()
                .map(|(k, v)| {
                    (
                        value_to_string(k).unwrap_or_else(|| "null".to_string()),
                        value_to_serde(v),
                    )
                })
                .collect(),
        ),
    }
}

/// JSON 값(대기 변경 파일에서 읽은 것)을 응답 JSON으로. 따옴표 없는 글은 참거짓·null·정수·실수로 읽는다.
pub fn json_to_serde(value: &JsonValue) -> Json {
    match value {
        JsonValue::Str(s) => Json::String(s.clone()),
        JsonValue::Array(items) => Json::Array(items.iter().map(json_to_serde).collect()),
        JsonValue::Object(entries) => Json::Object(
            entries
                .iter()
                .map(|(k, v)| (k.clone(), json_to_serde(v)))
                .collect(),
        ),
        JsonValue::Literal(raw) => match raw.as_str() {
            "true" => Json::Bool(true),
            "false" => Json::Bool(false),
            "null" => Json::Null,
            text => text
                .parse::<i64>()
                .map(Json::from)
                .or_else(|_| text.parse::<f64>().map(Json::from))
                .unwrap_or(Json::Null),
        },
    }
}
