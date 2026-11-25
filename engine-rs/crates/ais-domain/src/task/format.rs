//! 태스크 파일의 모양(머리말 + 설명)을 읽고 쓰고, 인자로 들어온 값을 검사한다. 규칙과 오류 문구는 Java 엔진의 `TaskFormat`과 같다.
//!
//! 머리말은 SnakeYAML이 쓰는 모양이라(`write/snake_dump.rs`) 파일 글이 Java와 바이트까지 같다. 모르는 칸은 `extra`에 보존한다.
//! 본문은 파일에서 늘 끝에 줄바꿈 하나가 붙은 채 적히고, 읽을 때 그 하나를 뗀다.

pub use crate::kernel::ids::TASK_ID as ID;
use crate::kernel::task_link::TaskLinkKind;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::Value as Json;

use super::{Criterion, Link, Ref, RefKind, Task, TaskPriority, TaskStatus, TaskType};
use crate::error::EngineError;
use crate::frontmatter::{self, Value};
use crate::json::{self, JsonValue};
use crate::kernel::{actor::valid_manager_name, token::compute_token};
use crate::snake_dump::dump_map;
use crate::text::{NOTE_ID, SUB_NOTE_ID, trim, utf16_len};

pub const MAX_FILE_BYTES: usize = 400_000;
pub const MAX_TITLE: usize = 200;
pub const MAX_BODY: usize = 100_000;
pub const MAX_LABELS: usize = 20;
pub const MAX_LABEL: usize = 30;
pub const MAX_CRITERIA: usize = 50;
pub const MAX_CRITERION: usize = 500;
pub const MAX_REFS: usize = 50;
pub const MAX_LINKS: usize = 50;
pub const MAX_COMMENT: usize = 20_000;

pub const TYPES: [&str; 3] = ["feature", "bug", "chore"];
pub const PRIORITIES: [&str; 3] = ["high", "medium", "low"];
pub const DEFAULT_TYPE: &str = "feature";
pub const DEFAULT_PRIORITY: &str = "medium";

pub static KEY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^TSK-[1-9][0-9]{0,8}$").unwrap());
static REPO: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_.-]{1,100}/[A-Za-z0-9_.-]{1,100}$").unwrap());
static BRANCH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9][A-Za-z0-9._/-]{0,99}$").unwrap());
static GITHUB_SHORT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Za-z0-9_.-]{1,100}/[A-Za-z0-9_.-]{1,100}#[1-9][0-9]{0,8}$").unwrap()
});
static GITHUB_URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^https://github\.com/[A-Za-z0-9_.-]{1,100}/[A-Za-z0-9_.-]{1,100}/(issues|pull)/[1-9][0-9]{0,8}$",
    )
    .unwrap()
});

/// 머리말의 알려진 칸. 이 밖의 칸은 `extra`로 보존한다.
static SLACK_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^https://[a-z0-9][a-z0-9-]*\.slack\.com/archives/([CGD][A-Z0-9]{2,})/p([0-9]{10})([0-9]{6})(?:\?([^#\s]+))?$").unwrap()
});

/// URL에서 채널과 답글을 달 원글 시각을 얻는다. 답글 주소면 thread_ts를 쓴다.
pub fn slack_thread(url: &str) -> Option<(String, String)> {
    let parts = SLACK_LINK.captures(url)?;
    let channel = &parts[1];
    let mut thread = format!("{}.{}", &parts[2], &parts[3]);
    if let Some(query) = parts.get(4) {
        let mut provided_thread = false;
        for (key, value) in query
            .as_str()
            .split('&')
            .filter_map(|pair| pair.split_once('='))
        {
            match key {
                "cid" if value != channel => return None,
                "thread_ts" => {
                    let (seconds, micros) = value.split_once('.')?;
                    if provided_thread
                        || seconds.len() != 10
                        || micros.len() != 6
                        || !seconds
                            .bytes()
                            .chain(micros.bytes())
                            .all(|byte| byte.is_ascii_digit())
                    {
                        return None;
                    }
                    thread = value.to_string();
                    provided_thread = true;
                }
                _ => {}
            }
        }
    }
    Some((channel.to_string(), thread))
}

const KNOWN: [&str; 18] = [
    "key",
    "id",
    "title",
    "type",
    "priority",
    "status",
    "repo",
    "project",
    "baseBranch",
    "manager",
    "labels",
    "criteria",
    "links",
    "refs",
    "createdAt",
    "editedAt",
    "editedSession",
    "editedAgent",
];

/// `TSK-14`의 번호 부분. 키 모양이 아니면 0이다.
pub fn number_of(key: &str) -> u32 {
    if KEY.is_match(key) {
        key[4..].parse().unwrap_or(0)
    } else {
        0
    }
}

// ---------------------------------------------------------------- Java 글 규칙(공용 커널이 정의한다)

pub use crate::kernel::java::{java_blank, java_trim, value_to_string};

fn get<'a>(map: &'a [(Value, Value)], key: &str) -> Option<&'a Value> {
    map.iter()
        .find(|(k, _)| matches!(k, Value::Str(s) if s == key))
        .map(|(_, v)| v)
}

fn str_of(map: &[(Value, Value)], key: &str) -> Option<String> {
    get(map, key).and_then(value_to_string)
}

fn blank_to_null(s: Option<String>) -> Option<String> {
    s.filter(|s| !java_blank(s))
        .map(|s| java_trim(&s).to_string())
}

// ---------------------------------------------------------------- 읽기

/// 파일 바이트를 태스크로 읽는다. 필수 머리말을 읽을 수 없으면 그 이유를 돌려준다.
/// 목록을 만들 때는 이런 파일을 건너뛰고 이유를 알린다.
pub fn parse(bytes: &[u8], rel_path: &str) -> Result<Task, String> {
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
    let key = str_of(&map, "key").filter(|k| KEY.is_match(k));
    let Some(key) = key else {
        return Err("key 가 없거나 TSK-<번호> 모양이 아니다".to_string());
    };
    let id = str_of(&map, "id").filter(|i| ID.is_match(i));
    let Some(id) = id else {
        return Err("id 가 없거나 tk… 모양이 아니다".to_string());
    };
    let title = str_of(&map, "title").filter(|t| !java_blank(t));
    let Some(title) = title else {
        return Err("title 이 없다".to_string());
    };
    let Some(status) = str_of(&map, "status").and_then(|s| TaskStatus::parse(&s)) else {
        return Err(format!(
            "status 를 모른다: {}",
            get(&map, "status")
                .and_then(value_to_string)
                .unwrap_or_else(|| "null".to_string())
        ));
    };
    let required_time = |name: &str| match get(&map, name) {
        Some(Value::Str(value)) if !java_blank(value) => Ok(value.clone()),
        _ => Err(format!("{name} 에 일시가 필요하다")),
    };
    let created_at = required_time("createdAt")?;
    let edited_at = required_time("editedAt")?;
    let task_type = str_of(&map, "type")
        .and_then(|v| TaskType::parse(&v))
        .unwrap_or(TaskType::DEFAULT);
    let priority = str_of(&map, "priority")
        .and_then(|v| TaskPriority::parse(&v))
        .unwrap_or(TaskPriority::DEFAULT);
    let extra: Vec<(String, Value)> = map
        .iter()
        .filter_map(|(k, v)| match k {
            Value::Str(name) if !KNOWN.contains(&name.as_str()) => Some((name.clone(), v.clone())),
            _ => None,
        })
        .collect();
    Ok(Task {
        key,
        id,
        title: java_trim(&title).to_string(),
        task_type,
        priority,
        status,
        repo: blank_to_null(str_of(&map, "repo")),
        project: blank_to_null(str_of(&map, "project")),
        base_branch: blank_to_null(str_of(&map, "baseBranch")),
        manager: str_of(&map, "manager").filter(|value| valid_manager_name(value)),
        labels: label_list(get(&map, "labels")),
        criteria: criteria_list(get(&map, "criteria")),
        links: link_list(get(&map, "links")),
        refs: ref_list(get(&map, "refs")),
        body,
        extra,
        created_at,
        edited_at,
        edited_session: str_of(&map, "editedSession"),
        edited_agent: str_of(&map, "editedAgent"),
        rel_path: rel_path.to_string(),
        token: Some(compute_token(bytes)),
    })
}

fn label_list(value: Option<&Value>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(Value::List(items)) = value {
        for item in items {
            if let Some(s) = value_to_string(item)
                && !java_blank(&s)
                && !out.iter().any(|o| o == java_trim(&s))
            {
                out.push(java_trim(&s).to_string());
            }
        }
    }
    out
}

fn criteria_list(value: Option<&Value>) -> Vec<Criterion> {
    let mut out = Vec::new();
    if let Some(Value::List(items)) = value {
        for item in items {
            match item {
                Value::Map(m) => {
                    let Some(text) = str_of(m, "text").filter(|t| !java_blank(t)) else {
                        continue;
                    };
                    let done = str_of(m, "done").is_some_and(|d| d.eq_ignore_ascii_case("true"));
                    out.push(Criterion {
                        text: java_trim(&text).to_string(),
                        done,
                        checked_at: if done { str_of(m, "checkedAt") } else { None },
                        checked_by: if done { str_of(m, "checkedBy") } else { None },
                    });
                }
                other => {
                    if let Some(text) = value_to_string(other).filter(|t| !java_blank(t)) {
                        out.push(Criterion {
                            text: java_trim(&text).to_string(),
                            done: false,
                            checked_at: None,
                            checked_by: None,
                        });
                    }
                }
            }
        }
    }
    out
}

fn link_list(value: Option<&Value>) -> Vec<Link> {
    let mut out: Vec<Link> = Vec::new();
    if let Some(Value::List(items)) = value {
        for item in items {
            let Value::Map(m) = item else { continue };
            let (Some(kind), Some(to)) = (str_of(m, "type"), str_of(m, "to")) else {
                continue;
            };
            let Some(kind) = TaskLinkKind::parse(&kind) else {
                continue;
            };
            if !KEY.is_match(&to) {
                continue;
            }
            let link = Link { kind, to };
            if !out.contains(&link) {
                out.push(link);
            }
        }
    }
    out
}

fn slack_ref_text(kind: &str, value: Option<String>, limit: usize) -> Option<String> {
    if kind != "slack" {
        return None;
    }
    blank_to_null(value.map(|text| text.trim().chars().take(limit).collect()))
}

fn ref_list(value: Option<&Value>) -> Vec<Ref> {
    let mut out: Vec<Ref> = Vec::new();
    if let Some(Value::List(items)) = value {
        for item in items {
            let Value::Map(m) = item else { continue };
            let (Some(kind_str), Some(target)) = (str_of(m, "kind"), str_of(m, "target")) else {
                continue;
            };
            let Some(kind) = RefKind::parse(&kind_str) else {
                continue;
            };
            if !valid_ref(kind, &target) {
                continue;
            }
            let reference = Ref {
                reporter: slack_ref_text(kind.as_str(), str_of(m, "reporter"), 100),
                excerpt: slack_ref_text(kind.as_str(), str_of(m, "excerpt"), 200),
                kind,
                target,
                title: blank_to_null(str_of(m, "title")),
            };
            if !out
                .iter()
                .any(|r| r.kind == reference.kind && r.target == reference.target)
            {
                out.push(reference);
            }
        }
    }
    out
}

fn valid_ref(kind: RefKind, target: &str) -> bool {
    match kind {
        RefKind::Note => NOTE_ID.is_match(target) || SUB_NOTE_ID.is_match(target),
        RefKind::Github => GITHUB_SHORT.is_match(target) || GITHUB_URL.is_match(target),
        RefKind::Slack => slack_thread(target).is_some(),
    }
}

// ---------------------------------------------------------------- 쓰기

fn text(value: &str) -> Value {
    Value::Str(value.to_string())
}

/// 태스크를 파일 바이트로 만든다. 같은 값이면 늘 같은 바이트다.
pub fn render(task: &Task) -> Result<Vec<u8>, EngineError> {
    let mut map: Vec<(Value, Value)> = Vec::new();
    let mut put = |key: &str, value: Value| map.push((text(key), value));
    put("key", text(&task.key));
    put("id", text(&task.id));
    put("title", text(&task.title));
    put("type", text(task.task_type.as_str()));
    put("priority", text(task.priority.as_str()));
    put("status", text(task.status.label()));
    if let Some(repo) = &task.repo {
        put("repo", text(repo));
    }
    if let Some(project) = &task.project {
        put("project", text(project));
    }
    if let Some(base) = &task.base_branch {
        put("baseBranch", text(base));
    }
    if let Some(manager) = &task.manager {
        put("manager", text(manager));
    }
    if !task.labels.is_empty() {
        put(
            "labels",
            Value::List(task.labels.iter().map(|l| text(l)).collect()),
        );
    }
    if !task.criteria.is_empty() {
        let rows = task
            .criteria
            .iter()
            .map(|c| {
                let mut row = vec![
                    (text("text"), text(&c.text)),
                    (text("done"), Value::Bool(c.done)),
                ];
                if c.done {
                    if let Some(at) = &c.checked_at {
                        row.push((text("checkedAt"), text(at)));
                    }
                    if let Some(by) = &c.checked_by {
                        row.push((text("checkedBy"), text(by)));
                    }
                }
                Value::Map(row)
            })
            .collect();
        put("criteria", Value::List(rows));
    }
    if !task.links.is_empty() {
        let rows = task
            .links
            .iter()
            .map(|l| {
                Value::Map(vec![
                    (text("type"), text(l.kind.as_str())),
                    (text("to"), text(&l.to)),
                ])
            })
            .collect();
        put("links", Value::List(rows));
    }
    if !task.refs.is_empty() {
        let rows = task
            .refs
            .iter()
            .map(|r| {
                let mut row = vec![
                    (text("kind"), text(r.kind.as_str())),
                    (text("target"), text(&r.target)),
                ];
                for (key, value) in [
                    ("title", &r.title),
                    ("reporter", &r.reporter),
                    ("excerpt", &r.excerpt),
                ] {
                    if let Some(value) = value {
                        row.push((text(key), text(value)));
                    }
                }
                Value::Map(row)
            })
            .collect();
        put("refs", Value::List(rows));
    }
    put("createdAt", text(&task.created_at));
    put("editedAt", text(&task.edited_at));
    for (key, value) in [
        ("editedSession", &task.edited_session),
        ("editedAgent", &task.edited_agent),
    ] {
        if let Some(value) = value {
            put(key, text(value));
        }
    }
    for (key, value) in &task.extra {
        if !map
            .iter()
            .any(|(k, _)| matches!(k, Value::Str(s) if s == key))
        {
            map.push((text(key), value.clone()));
        }
    }
    let yaml = dump_map(&map)
        .map_err(|e| EngineError::internal(format!("태스크를 YAML로 쓰지 못했다: {e}")))?;
    Ok(format!("---\n{yaml}---\n{}\n", task.body).into_bytes())
}

// ---------------------------------------------------------------- 인자 검사

fn bad(message: impl Into<String>) -> EngineError {
    EngineError::invalid_argument(message)
}

fn has_control(s: &str) -> bool {
    s.chars().any(char::is_control)
}

fn list_text(items: &[&str]) -> String {
    format!("[{}]", items.join(", "))
}

pub fn title(raw: &str) -> Result<String, EngineError> {
    let t = trim(raw);
    if t.is_empty() {
        return Err(bad("title 이 필요하다"));
    }
    if t.chars().count() > MAX_TITLE {
        return Err(bad(format!("title 은 {MAX_TITLE}자까지다")));
    }
    if has_control(t) {
        return Err(bad("title 에 제어 문자를 쓸 수 없다"));
    }
    Ok(t.to_string())
}

pub fn task_type(raw: &str) -> Result<TaskType, EngineError> {
    let v = java_trim(raw).to_lowercase();
    let Some(t) = TaskType::parse(&v) else {
        return Err(bad(format!(
            "type 은 {} 가운데 하나여야 한다: {raw}",
            list_text(&TYPES)
        )));
    };
    Ok(t)
}

pub fn priority(raw: &str) -> Result<TaskPriority, EngineError> {
    let v = java_trim(raw).to_lowercase();
    let Some(p) = TaskPriority::parse(&v) else {
        return Err(bad(format!(
            "priority 는 {} 가운데 하나여야 한다: {raw}",
            list_text(&PRIORITIES)
        )));
    };
    Ok(p)
}

/// `owner/name`. 빈 값이면 `None`(지운다)이다.
pub fn repo(raw: &str) -> Result<Option<String>, EngineError> {
    let v = trim(raw);
    if v.is_empty() {
        return Ok(None);
    }
    if !REPO.is_match(v) || v.contains("..") {
        return Err(bad(format!("repo 는 owner/name 모양이어야 한다: {raw}")));
    }
    Ok(Some(v.to_string()))
}

/// 기준 브랜치. 빈 값이면 `None`(저장소의 기본 브랜치)이다.
pub fn base_branch(raw: &str) -> Result<Option<String>, EngineError> {
    let v = trim(raw);
    if v.is_empty() {
        return Ok(None);
    }
    if !BRANCH.is_match(v)
        || v.contains("..")
        || v.ends_with('/')
        || v.ends_with(".lock")
        || v.contains("//")
    {
        return Err(bad(format!("base 는 브랜치 이름이어야 한다: {raw}")));
    }
    Ok(Some(v.to_string()))
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

pub fn comment(raw: &str) -> Result<String, EngineError> {
    let b = trim(raw);
    if b.is_empty() {
        return Err(bad("body 가 필요하다"));
    }
    if utf16_len(b) > MAX_COMMENT {
        return Err(bad(format!("body 는 {MAX_COMMENT}자까지다")));
    }
    if b.contains('\0') {
        return Err(bad("body 에 널 문자를 쓸 수 없다"));
    }
    Ok(b.to_string())
}

/// 목록 인자를 읽는다. 호출 객체에서는 배열, 명령줄에서는 JSON 글로 온다. 빈 글은 빈 목록(비우기)이다.
fn array_arg(raw: &Json, field: &str) -> Result<Vec<JsonValue>, EngineError> {
    match raw {
        Json::Null => Ok(Vec::new()),
        Json::String(text) => {
            if trim(text).is_empty() {
                return Ok(Vec::new());
            }
            match json::parse(text) {
                Ok(JsonValue::Array(items)) => Ok(items),
                Ok(_) => Err(bad(format!("{field} 는 JSON 배열이어야 한다"))),
                Err(message) => Err(bad(format!("{field} 는 JSON 이어야 한다: {message}"))),
            }
        }
        Json::Array(items) => Ok(items
            .iter()
            .map(crate::kernel::java::to_json_value)
            .collect()),
        _ => Err(bad(format!("{field} 는 JSON 배열이어야 한다"))),
    }
}

/// `labels` 값. 같은 라벨은 처음 하나만 남는다.
pub fn labels(raw: &Json) -> Result<Vec<String>, EngineError> {
    let items = array_arg(raw, "labels")?;
    if items.len() > MAX_LABELS {
        return Err(bad(format!("labels 는 {MAX_LABELS}개까지다")));
    }
    let mut seen: Vec<String> = Vec::new();
    for item in &items {
        let JsonValue::Str(s) = item else {
            return Err(bad(format!(
                "labels 의 항목은 글이어야 한다: {}",
                json::java_record_string(item)
            )));
        };
        let label = trim(s);
        if label.is_empty() || label.chars().count() > MAX_LABEL {
            return Err(bad(format!("라벨은 1~{MAX_LABEL}자다: {s}")));
        }
        if has_control(label) {
            return Err(bad("라벨에 제어 문자를 쓸 수 없다"));
        }
        if !seen.iter().any(|x| x == label) {
            seen.push(label.to_string());
        }
    }
    Ok(seen)
}

fn string_of(entries: &[(String, JsonValue)], key: &str) -> Option<String> {
    entries
        .iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, v)| match v {
            JsonValue::Str(s) => Some(s.clone()),
            _ => None,
        })
}

/// `criteria` 값. 항목은 글이거나 `{"text":"…","done":true}`다. 확인한 시각과 사람은 호출한 쪽이 채운다.
pub fn criteria(raw: &Json) -> Result<Vec<Criterion>, EngineError> {
    let items = array_arg(raw, "criteria")?;
    if items.len() > MAX_CRITERIA {
        return Err(bad(format!("criteria 는 {MAX_CRITERIA}개까지다")));
    }
    let mut out = Vec::new();
    for item in &items {
        let (text, done, checked_at, checked_by);
        match item {
            JsonValue::Str(s) => {
                text = Some(s.clone());
                done = false;
                checked_at = None;
                checked_by = None;
            }
            JsonValue::Object(entries) => {
                for (key, _) in entries {
                    if !["text", "done", "checkedAt", "checkedBy"].contains(&key.as_str()) {
                        return Err(bad(format!("criteria 항목에 모르는 키가 있다: {key}")));
                    }
                }
                text = string_of(entries, "text");
                let mut flag = false;
                if let Some((_, d)) = entries.iter().find(|(k, _)| k == "done") {
                    match d {
                        JsonValue::Literal(raw) if raw == "true" || raw == "false" => {
                            flag = raw == "true";
                        }
                        _ => return Err(bad("criteria 의 done 은 true 나 false 여야 한다")),
                    }
                }
                done = flag;
                checked_at = string_of(entries, "checkedAt");
                checked_by = string_of(entries, "checkedBy");
            }
            other => {
                return Err(bad(format!(
                    "criteria 의 항목은 글이나 객체여야 한다: {}",
                    json::java_record_string(other)
                )));
            }
        }
        let text = text.map_or_else(String::new, |t| trim(&t).to_string());
        if text.is_empty() || text.chars().count() > MAX_CRITERION {
            return Err(bad(format!("완료 기준 한 줄은 1~{MAX_CRITERION}자다")));
        }
        out.push(Criterion {
            text,
            done,
            checked_at: if done { checked_at } else { None },
            checked_by: if done { checked_by } else { None },
        });
    }
    Ok(out)
}

/// `refs` 값. 항목은 `{"kind":"note","target":"nd…"}`나 `{"kind":"github","target":"owner/name#53"}`다.
pub fn refs(raw: &Json) -> Result<Vec<Ref>, EngineError> {
    let items = array_arg(raw, "refs")?;
    if items.len() > MAX_REFS {
        return Err(bad(format!("refs 는 {MAX_REFS}개까지다")));
    }
    let mut out: Vec<Ref> = Vec::new();
    for item in &items {
        let JsonValue::Object(entries) = item else {
            return Err(bad(format!(
                "refs 의 항목은 객체여야 한다: {}",
                json::java_record_string(item)
            )));
        };
        for (key, _) in entries {
            if !["kind", "target", "title", "reporter", "excerpt"].contains(&key.as_str()) {
                return Err(bad(format!("refs 항목에 모르는 키가 있다: {key}")));
            }
        }
        let kind_str = string_of(entries, "kind");
        let target = string_of(entries, "target");
        let (Some(kind_str), Some(target)) = (kind_str, target) else {
            return Err(bad(format!(
                "refs 항목이 틀렸다. note 는 노트 id(nd…), github 는 owner/name#번호나 이슈·PR 주소여야 한다: {}",
                json::java_record_string(item)
            )));
        };
        let Some(kind) = RefKind::parse(java_trim(&kind_str)) else {
            return Err(bad(format!(
                "refs 항목이 틀렸다. note 는 노트 id(nd…), github 는 owner/name#번호나 이슈·PR 주소여야 한다: {}",
                json::java_record_string(item)
            )));
        };
        if !valid_ref(kind, java_trim(&target)) {
            if kind == RefKind::Slack {
                return Err(bad(
                    "refs 항목이 틀렸다. slack 은 Slack 메시지 주소여야 한다",
                ));
            }
            return Err(bad(format!(
                "refs 항목이 틀렸다. note 는 노트 id(nd…), github 는 owner/name#번호나 이슈·PR 주소여야 한다: {}",
                json::java_record_string(item)
            )));
        }
        let title = string_of(entries, "title");
        let reference = Ref {
            reporter: slack_ref_text(kind.as_str(), string_of(entries, "reporter"), 100),
            excerpt: slack_ref_text(kind.as_str(), string_of(entries, "excerpt"), 200),
            kind,
            target: java_trim(&target).to_string(),
            title: title
                .filter(|t| !trim(t).is_empty())
                .map(|t| trim(&t).to_string()),
        };
        if out
            .iter()
            .any(|r| r.kind == reference.kind && r.target == reference.target)
        {
            return Err(bad(format!(
                "refs 에 같은 항목이 겹친다: {}",
                reference.target
            )));
        }
        out.push(reference);
    }
    Ok(out)
}
