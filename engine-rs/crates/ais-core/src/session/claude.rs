//! Claude Code 세션 기록(`~/.claude/projects/<프로젝트>/<세션 id>.jsonl`) 읽개. 설명서 7장의 읽기 규칙과 D18(깨진 UTF-8은 대체 글자)을 따른다.

use crate::ports::fs;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Value as Json, json};

use super::{
    Turn, add_text_turn, extract_urls, get, obj_of, read_lines, slice, str_key, turn_json,
};
use crate::json::{self, JsonValue};
use crate::kernel::time::parse_instant_strict;
use crate::text::{compare_utf16, is_blank, trim};

pub const SESSION_ID: &str = r"^[0-9a-fA-F-]{8,64}$";
pub const NOTE_ID: &str = r"^[0-9a-z-]{10,64}$";
const CHUNK_BYTES: usize = 65536;

static ENTRYPOINT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""entrypoint"[ \t\n\x0B\x0C\r]*:[ \t\n\x0B\x0C\r]*"([^"]*)""#).unwrap()
});
static TIMESTAMP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""timestamp"[ \t\n\x0B\x0C\r]*:[ \t\n\x0B\x0C\r]*"([^"]*)""#).unwrap()
});
static GIT_BRANCH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#""gitBranch"[ \t\n\x0B\x0C\r]*:[ \t\n\x0B\x0C\r]*"([^"]*)""#).unwrap()
});
static CWD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""cwd"[ \t\n\x0B\x0C\r]*:[ \t\n\x0B\x0C\r]*"([^"]*)""#).unwrap());
static TASK_CREATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^Task #([0-9]+) created successfully:[ \t\n\x0B\x0C\r]*([^\n\r\x{85}\x{2028}\x{2029}]*)$",
    )
    .unwrap()
});
static TASK_UPDATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^Updated task #([0-9]+)[^\n\r\x{85}\x{2028}\x{2029}]*$").unwrap()
});

/// 세션 id로 기록 파일을 찾는다. 프로젝트 폴더마다 그 이름의 파일이 보통 파일로 있는지만 본다(7.1).
pub fn find_transcript(projects_dir: Option<&Path>, session_id: &str) -> Option<PathBuf> {
    let dir = projects_dir?;
    let target = format!("{session_id}.jsonl");
    for project in fs::read_dir(dir).ok()?.flatten() {
        let path = project.path();
        if fs::metadata(&path).map(|m| m.is_dir()).unwrap_or(false) {
            let candidate = path.join(&target);
            if fs::metadata(&candidate)
                .map(|m| m.is_file())
                .unwrap_or(false)
            {
                return Some(candidate);
            }
        }
    }
    None
}

fn first_line(text: &str) -> &str {
    text.find(['\n', '\r']).map_or(text, |i| &text[..i])
}

fn primitive_text(value: Option<&JsonValue>) -> Option<String> {
    match value {
        Some(JsonValue::Str(s)) => Some(s.clone()),
        Some(JsonValue::Literal(raw)) => Some(raw.clone()),
        _ => None,
    }
}

fn long_prop(object: &[(String, JsonValue)], key: &str) -> Option<i64> {
    match get(object, key) {
        Some(JsonValue::Literal(raw)) => parse_long(raw),
        _ => None,
    }
}

fn parse_long(text: &str) -> Option<i64> {
    let digits = text.strip_prefix(['+', '-']).unwrap_or(text);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    text.parse::<i64>().ok()
}

fn bool_prop(object: &[(String, JsonValue)], key: &str) -> bool {
    matches!(get(object, key), Some(JsonValue::Literal(raw)) if raw == "true")
}

/// 도구 입력에서 사람이 읽을 만한 한 줄을 고른다. 처음 찾은 글 값이 이기고, 빈 글도 글 값이다(7.4).
fn summarize_tool_arg(input: Option<&[(String, JsonValue)]>) -> Option<String> {
    let input = input.filter(|i| !i.is_empty())?;
    let found = [
        "description",
        "file_path",
        "pattern",
        "query",
        "command",
        "url",
    ]
    .iter()
    .find_map(|k| str_key(input, k))?;
    let line = trim(first_line(trim(found))).to_string();
    if line.is_empty() {
        return None;
    }
    let units: Vec<u16> = line.encode_utf16().collect();
    if units.len() <= 120 {
        return Some(line);
    }
    if line.starts_with('/') {
        Some(format!(
            "…{}",
            String::from_utf16_lossy(&units[units.len() - 119..])
        ))
    } else {
        Some(format!("{}…", String::from_utf16_lossy(&units[..119])))
    }
}

struct Todo {
    number: i32,
    title: String,
    status: String,
}

fn find_note_mentions(
    line: &str,
    notes: &[String],
    last_turn: usize,
    mentions: &mut HashMap<String, Vec<usize>>,
) {
    for id in notes {
        if line.contains(id.as_str()) {
            let list = mentions.entry(id.clone()).or_default();
            if list.last() != Some(&last_turn) {
                list.push(last_turn);
            }
        }
    }
}

/// 전사를 훑어 `read-session`의 응답 칸으로 만든다. 기록이 없으면 빈 결과다(7.9).
pub fn read_session(
    projects_dir: Option<&Path>,
    session_id: &str,
    limit: i32,
    before: Option<i32>,
    include_urls: bool,
    note_id: Option<&str>,
    author_notes: Option<&[String]>,
) -> Result<super::ReadResult, String> {
    let Some(file) = find_transcript(projects_dir, session_id) else {
        return Ok(super::ReadResult {
            session_id: session_id.to_string(),
            file: None,
            bytes: 0,
            lines: 0,
            skipped: 0,
            messages: 0,
            has_more: false,
            urls: Vec::new(),
            running_agents: Vec::new(),
            todos: Vec::new(),
            run_state: None,
            note_mentions: note_id.map(|_| Vec::new()),
            author: None,
            turns: Vec::new(),
        });
    };
    let file_bytes = fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
    let lines = read_lines(&file)?;

    let mut line_count = 0usize;
    let mut skipped = 0usize;
    let mut turns: Vec<Turn> = Vec::new();
    let mut tool_use: HashMap<String, usize> = HashMap::new();
    let mut matched: HashSet<String> = HashSet::new();
    let mut todos: BTreeMap<i32, Todo> = BTreeMap::new();
    let mut urls: Vec<(String, i64)> = Vec::new();

    let mut notes: Vec<String> = Vec::new();
    if let Some(n) = note_id {
        notes.push(n.to_string());
    }
    if let Some(authors) = author_notes {
        for a in authors {
            if !notes.contains(a) {
                notes.push(a.clone());
            }
        }
    }
    let mut mentions: HashMap<String, Vec<usize>> =
        notes.iter().map(|n| (n.clone(), Vec::new())).collect();

    let mut model: Option<String> = None;
    let mut effort: Option<String> = None;
    let mut context_tokens: Option<i64> = None;
    let mut quota_blocked = false;
    let mut quota_resets_at: Option<i64> = None;

    for line in &lines {
        line_count += 1;
        if is_blank(line) {
            continue;
        }
        let Ok(value) = json::parse_file(line) else {
            skipped += 1;
            continue;
        };
        let JsonValue::Object(props) = value else {
            skipped += 1;
            continue;
        };
        let Some(kind) = str_key(&props, "type") else {
            continue;
        };
        let user_or_assistant = kind == "user" || kind == "assistant";
        let timestamp = str_key(&props, "timestamp");

        if kind == "user" {
            if let Some(message) = obj_of(get(&props, "message")) {
                match get(message, "content") {
                    Some(JsonValue::Str(text)) => {
                        if include_urls {
                            extract_urls(text, &mut urls);
                        }
                        add_text_turn(&mut turns, "user", timestamp, text);
                    }
                    Some(JsonValue::Array(blocks)) => {
                        for block in blocks {
                            let JsonValue::Object(b) = block else {
                                continue;
                            };
                            match str_key(b, "type") {
                                Some("text") => {
                                    if let Some(text) = str_key(b, "text") {
                                        if include_urls {
                                            extract_urls(text, &mut urls);
                                        }
                                        add_text_turn(&mut turns, "user", timestamp, text);
                                    }
                                }
                                Some("tool_result") => {
                                    tool_result(
                                        b,
                                        timestamp,
                                        &mut turns,
                                        &tool_use,
                                        &mut matched,
                                        &mut todos,
                                        &mut skipped,
                                    );
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }
        } else if kind == "assistant"
            && let Some(message) = obj_of(get(&props, "message"))
        {
            let m = str_key(message, "model");
            if let Some(m) = m.filter(|m| *m != "<synthetic>") {
                model = Some(m.to_string());
                effort = str_key(&props, "effort").map(str::to_string);
                context_tokens = match obj_of(get(message, "usage")) {
                    Some(usage) => long_prop(usage, "input_tokens").map(|input| {
                        let creation = long_prop(usage, "cache_creation_input_tokens").unwrap_or(0);
                        let read = long_prop(usage, "cache_read_input_tokens").unwrap_or(0);
                        input.wrapping_add(creation).wrapping_add(read)
                    }),
                    None => None,
                };
            }
            if let Some(quota) = obj_of(get(&props, "quotaLimits"))
                && model.is_some()
            {
                quota_blocked = true;
                if let Some(resets) = long_prop(quota, "resetsAt") {
                    quota_resets_at = Some(resets);
                }
            }
            match get(message, "content") {
                Some(JsonValue::Str(text)) => {
                    if include_urls {
                        extract_urls(text, &mut urls);
                    }
                    add_text_turn(&mut turns, "assistant", timestamp, text);
                }
                Some(JsonValue::Array(blocks)) => {
                    let objects: Vec<&[(String, JsonValue)]> = blocks
                        .iter()
                        .filter_map(|b| match b {
                            JsonValue::Object(o) => Some(o.as_slice()),
                            _ => None,
                        })
                        .collect();
                    // 글 블록 먼저, 그 뒤 tool_use 블록(7.4)
                    for b in objects
                        .iter()
                        .filter(|b| str_key(b, "type") == Some("text"))
                    {
                        if let Some(text) = str_key(b, "text") {
                            if include_urls {
                                extract_urls(text, &mut urls);
                            }
                            add_text_turn(&mut turns, "assistant", timestamp, text);
                        }
                    }
                    for b in objects
                        .iter()
                        .filter(|b| str_key(b, "type") == Some("tool_use"))
                    {
                        let (Some(id), Some(name)) = (str_key(b, "id"), str_key(b, "name")) else {
                            continue;
                        };
                        let input = obj_of(get(b, "input"));
                        turns.push(Turn {
                            index: turns.len(),
                            who: "tool",
                            at: timestamp.map(str::to_string),
                            tool_name: Some(name.to_string()),
                            tool_arg: summarize_tool_arg(input),
                            tool_use_id: Some(id.to_string()),
                            tool_input: input.map(<[_]>::to_vec),
                            ..Turn::default()
                        });
                        tool_use.insert(id.to_string(), turns.len() - 1);
                    }
                }
                _ => {}
            }
        }

        // 노트 id가 나온 자리는 그 줄의 글 회차와 도구 회차를 다 담은 뒤의 마지막 순번이다(7.8)
        if user_or_assistant && !notes.is_empty() && !turns.is_empty() {
            find_note_mentions(line, &notes, turns.len() - 1, &mut mentions);
        }
    }

    let total = turns.len();
    let effective_limit = limit.clamp(1, 500) as usize;
    let effective_before = before.map_or(total, |b| b.clamp(0, total as i32) as usize);
    let page = slice(&turns, effective_limit, effective_before);
    let has_more = page.first().is_some_and(|t| t.index > 0);

    let running_agents: Vec<Json> = turns
        .iter()
        .filter(|t| t.who == "tool" && t.tool_name.as_deref() == Some("Agent"))
        .filter(|t| {
            !t.tool_use_id
                .as_ref()
                .is_some_and(|id| matched.contains(id))
        })
        .map(|t| {
            let mut m = serde_json::Map::new();
            if let Some(arg) = &t.tool_arg {
                m.insert("what".into(), json!(arg));
            }
            if let Some(at) = &t.at {
                m.insert("at".into(), json!(at));
            }
            Json::Object(m)
        })
        .collect();
    if include_urls {
        urls.sort_by_key(|u| std::cmp::Reverse(u.1));
    }
    let todos_json: Vec<Json> = todos
        .values()
        .filter(|t| t.status != "deleted")
        .map(|t| json!({"number": t.number, "title": t.title, "status": t.status}))
        .collect();

    let run_state = model.as_ref().map(|m| {
        let mut state = serde_json::Map::new();
        state.insert("model".into(), json!(m));
        if let Some(e) = &effort {
            state.insert("effort".into(), json!(e));
        }
        if let Some(c) = context_tokens {
            state.insert("contextTokens".into(), json!(c));
        }
        state.insert("quotaBlocked".into(), json!(quota_blocked));
        if quota_blocked && let Some(r) = quota_resets_at {
            state.insert("quotaResetsAt".into(), json!(r));
        }
        Json::Object(state)
    });
    let note_mentions = note_id.map(|id| mentions.get(id).cloned().unwrap_or_default());

    let author = author_notes.filter(|a| !a.is_empty()).and_then(|authors| {
        let (selected, list) = authors
            .iter()
            .find_map(|a| mentions.get(a).filter(|m| !m.is_empty()).map(|m| (a, m)))?;
        let last = *list.last()?;
        let latest_raw = slice(&turns, effective_limit, last + 1);
        let mut out = serde_json::Map::new();
        out.insert("note".into(), json!(selected));
        out.insert("mentions".into(), json!(list));
        let starts_at_zero = latest_raw.first().is_some_and(|t| t.index == 0);
        if list.len() > 1 && !starts_at_zero {
            let creation = slice(&turns, effective_limit, list[0] + 1);
            out.insert(
                "creation".into(),
                Json::Array(creation.iter().map(turn_json).collect()),
            );
        }
        out.insert(
            "latest".into(),
            Json::Array(latest_raw.iter().map(turn_json).collect()),
        );
        Some(Json::Object(out))
    });

    Ok(super::ReadResult {
        session_id: session_id.to_string(),
        file: Some(file.to_string_lossy().into_owned()),
        bytes: file_bytes,
        lines: line_count,
        skipped,
        messages: total,
        has_more,
        urls,
        running_agents,
        todos: todos_json,
        run_state,
        note_mentions,
        author,
        turns: page.iter().map(turn_json).collect(),
    })
}

fn tool_result(
    block: &[(String, JsonValue)],
    timestamp: Option<&str>,
    turns: &mut [Turn],
    tool_use: &HashMap<String, usize>,
    matched: &mut HashSet<String>,
    todos: &mut BTreeMap<i32, Todo>,
    skipped: &mut usize,
) {
    let Some(tool_use_id) = str_key(block, "tool_use_id") else {
        return;
    };
    let Some(&at_index) = tool_use.get(tool_use_id) else {
        return;
    };
    matched.insert(tool_use_id.to_string());
    let is_error = bool_prop(block, "is_error");
    let turn = &mut turns[at_index];
    if is_error {
        turn.tool_failed = true;
    }
    if let (Some(start), Some(end)) = (
        turn.at.as_deref().and_then(parse_instant_strict),
        timestamp.and_then(parse_instant_strict),
    ) {
        // 결과 줄이 호출 줄보다 앞서면 음수 그대로다(7.4)
        let millis = (end - start).div_euclid(1_000_000);
        turn.tool_seconds = Some(millis as f64 / 1000.0);
    }
    // TaskCreate / TaskUpdate 분석(7.6). 결과 글은 수, 참거짓, null도 글자로 읽고 배열과 객체만 건너뛴다
    let Some(result) = primitive_text(get(block, "content")) else {
        return;
    };
    let line = first_line(&result);
    match turn.tool_name.as_deref() {
        Some("TaskCreate") => {
            let number = TASK_CREATE.captures(line).and_then(|m| {
                m[1].parse::<i32>()
                    .ok()
                    .map(|n| (n, trim(&m[2]).to_string()))
            });
            match number {
                Some((n, title)) => {
                    todos.insert(
                        n,
                        Todo {
                            number: n,
                            title,
                            status: "pending".into(),
                        },
                    );
                }
                // `Task #N created successfully` 꼴이 아니거나 번호가 32비트를 넘으면 그 줄만 지나간다(7.4)
                None => *skipped += 1,
            }
        }
        Some("TaskUpdate") => {
            let number = TASK_UPDATE
                .captures(line)
                .and_then(|m| m[1].parse::<i32>().ok());
            if let Some(n) = number
                && let Some(input) = &turn.tool_input
                && let Some(todo) = todos.get_mut(&n)
            {
                let status = str_key(input, "status");
                let subject = str_key(input, "subject");
                if let Some(subject) = subject {
                    todo.title = subject.to_string();
                }
                // deleted도 상태 값이다. 목록에서만 걷고 번호는 둬서, 뒤의 고침이 다시 세우면 그 할 일이 되살아난다
                if let Some(status) = status
                    && ["pending", "in_progress", "completed", "deleted"].contains(&status)
                {
                    todo.status = status.to_string();
                }
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------- session-titles와 recent-sessions

pub struct TitleInfo {
    pub found: bool,
    pub title: Option<String>,
    pub branch: Option<String>,
    pub cwd: Option<String>,
}

struct HeadAndTail {
    head_text: String,
    tail_text: String,
    head_lines: Vec<String>,
    tail_lines: Vec<String>,
}

fn split_lines(text: &str) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    text.split('\n')
        .map(|s| s.strip_suffix('\r').unwrap_or(s).to_string())
        .collect()
}

fn decode(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn head_and_tail(path: &Path, size: u64) -> HeadAndTail {
    let empty = || HeadAndTail {
        head_text: String::new(),
        tail_text: String::new(),
        head_lines: Vec::new(),
        tail_lines: Vec::new(),
    };
    if size <= CHUNK_BYTES as u64 {
        return match fs::read(path) {
            Ok(all) => {
                let text = decode(&all);
                let lines = split_lines(&text);
                HeadAndTail {
                    head_text: text.clone(),
                    tail_text: text,
                    head_lines: lines.clone(),
                    tail_lines: lines,
                }
            }
            Err(_) => empty(),
        };
    }
    // 머리와 꼬리를 각각 읽는다. 줄은 가장자리의 불완전한 줄을 버린 바이트로 나누고,
    // 글자 대체(정규식)는 버린 가장자리 조각까지 든 창의 바이트 전체에서 찾는다(7.10)
    let Ok(head) = fs::read_range(path, 0, CHUNK_BYTES) else {
        return empty();
    };
    let valid_head = head
        .iter()
        .rposition(|b| *b == b'\n')
        .map_or(&head[..0], |i| &head[..=i]);
    let Ok(tail) = fs::read_range(path, size - CHUNK_BYTES as u64, CHUNK_BYTES) else {
        return empty();
    };
    let valid_tail = match tail.iter().position(|b| *b == b'\n') {
        Some(i) if i + 1 < tail.len() => &tail[i + 1..],
        _ => &tail[..0],
    };
    HeadAndTail {
        head_text: decode(&head),
        tail_text: decode(&tail),
        head_lines: split_lines(&decode(valid_head)),
        tail_lines: split_lines(&decode(valid_tail)),
    }
}

fn title_from_line(line: &str, expected_type: &str, key: &str) -> Option<String> {
    if !line.contains(expected_type) {
        return None;
    }
    let JsonValue::Object(object) = json::parse_file(line).ok()? else {
        return None;
    };
    if str_key(&object, "type") != Some(expected_type) {
        return None;
    }
    str_key(&object, key)
        .filter(|t| !is_blank(t))
        .map(str::to_string)
}

fn resolve_title(transcript: &Path, ht: &HeadAndTail) -> Option<String> {
    // 1. 꼬리 창 마지막 custom-title 줄의 customTitle
    if let Some(t) = ht
        .tail_lines
        .iter()
        .rev()
        .find_map(|l| title_from_line(l, "custom-title", "customTitle"))
    {
        return Some(t);
    }
    // 2. <프로젝트 폴더>/<세션 id>/custom-title.json
    if let Some(project) = transcript.parent() {
        let name = transcript
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let id = name.strip_suffix(".jsonl").unwrap_or(&name);
        let file = project.join(id).join("custom-title.json");
        if fs::metadata(&file).map(|m| m.is_file()).unwrap_or(false)
            && let Some(text) = fs::read(&file).ok().and_then(|b| String::from_utf8(b).ok())
            && let Ok(JsonValue::Object(object)) = json::parse(&text)
            && let Some(title) = str_key(&object, "customTitle").filter(|t| !is_blank(t))
        {
            return Some(title.to_string());
        }
    }
    // 3. 머리 창 마지막 custom-title 줄
    if let Some(t) = ht
        .head_lines
        .iter()
        .rev()
        .find_map(|l| title_from_line(l, "custom-title", "customTitle"))
    {
        return Some(t);
    }
    // 4. 꼬리 창 마지막 ai-title 줄의 aiTitle
    if let Some(t) = ht
        .tail_lines
        .iter()
        .rev()
        .find_map(|l| title_from_line(l, "ai-title", "aiTitle"))
    {
        return Some(t);
    }
    // 5. 머리 창 마지막 ai-title 줄
    ht.head_lines
        .iter()
        .rev()
        .find_map(|l| title_from_line(l, "ai-title", "aiTitle"))
}

fn prop_from_line(line: &str, key: &str) -> Option<String> {
    if !line.contains(&format!("\"{key}\"")) {
        return None;
    }
    let JsonValue::Object(object) = json::parse_file(line).ok()? else {
        return None;
    };
    str_key(&object, key)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
}

fn first_match(text: &str, pattern: &Regex) -> Option<String> {
    pattern.captures(text).map(|c| c[1].to_string())
}

fn last_match(text: &str, pattern: &Regex) -> Option<String> {
    pattern.captures_iter(text).last().map(|c| c[1].to_string())
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.is_empty())
}

/// 정규식으로 꺼낸 JSON 글의 탈출을 푼다
fn unescape_json(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            let simple = match next {
                '"' => Some('"'),
                '\\' => Some('\\'),
                '/' => Some('/'),
                'b' => Some('\u{8}'),
                'f' => Some('\u{c}'),
                'n' => Some('\n'),
                'r' => Some('\r'),
                't' => Some('\t'),
                _ => None,
            };
            if let Some(ch) = simple {
                out.push(ch);
                i += 2;
                continue;
            }
            if next == 'u' && i + 5 < chars.len() {
                let hex: String = chars[i + 2..i + 6].iter().collect();
                if let Ok(unit) = u16::from_str_radix(&hex, 16) {
                    out.push_str(&String::from_utf16_lossy(&[unit]));
                    i += 6;
                    continue;
                }
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn resolve_branch(ht: &HeadAndTail) -> Option<String> {
    // 1. 꼬리 창에서 마지막 gitBranch 줄
    if let Some(b) = ht
        .tail_lines
        .iter()
        .rev()
        .find_map(|l| prop_from_line(l, "gitBranch"))
    {
        return Some(b);
    }
    // 2. 꼬리 창 글자에서 마지막 "gitBranch":"…"
    if let Some(b) = non_empty(last_match(&ht.tail_text, &GIT_BRANCH).map(|s| unescape_json(&s))) {
        return Some(b);
    }
    // 3. 머리 창에서 첫 gitBranch 줄
    if let Some(b) = ht
        .head_lines
        .iter()
        .find_map(|l| prop_from_line(l, "gitBranch"))
    {
        return Some(b);
    }
    // 4. 머리 창 글자에서 첫 "gitBranch":"…"
    non_empty(first_match(&ht.head_text, &GIT_BRANCH).map(|s| unescape_json(&s)))
}

fn resolve_cwd(ht: &HeadAndTail) -> Option<String> {
    // 머리 창에서 첫 cwd. 꼬리는 안 본다
    if let Some(c) = ht.head_lines.iter().find_map(|l| prop_from_line(l, "cwd")) {
        return Some(c);
    }
    non_empty(first_match(&ht.head_text, &CWD).map(|s| unescape_json(&s)))
}

/// 세션의 머리와 꼬리를 읽어 제목, 브랜치, 처음 작업 폴더를 꺼낸다(7.10).
pub fn read_title_info(projects_dir: Option<&Path>, session_id: &str) -> TitleInfo {
    let none = TitleInfo {
        found: false,
        title: None,
        branch: None,
        cwd: None,
    };
    let Some(file) = find_transcript(projects_dir, session_id) else {
        return none;
    };
    let Ok(size) = fs::metadata(&file).map(|m| m.len()) else {
        return none;
    };
    let ht = head_and_tail(&file, size);
    TitleInfo {
        found: true,
        title: resolve_title(&file, &ht),
        branch: resolve_branch(&ht),
        cwd: resolve_cwd(&ht),
    }
}

pub struct RecentEntry {
    pub session_id: String,
    pub last_active_at: i64,
    pub started_at: Option<i64>,
    pub title: Option<String>,
    pub branch: Option<String>,
    pub cwd: Option<String>,
}

pub struct Recent {
    pub days: i32,
    pub dropped: usize,
    pub sessions: Vec<RecentEntry>,
}

fn modified_millis(path: &Path) -> Option<i64> {
    let modified = fs::metadata(path).ok()?.modified().ok()?;
    Some(match modified.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_millis() as i64,
        Err(e) => -(e.duration().as_millis() as i64),
    })
}

/// 최근 `days`일 안의 CLI 세션 목록(7.11)
pub fn recent_sessions(projects_dir: Option<&Path>, days: i32) -> Recent {
    let empty = Recent {
        days,
        dropped: 0,
        sessions: Vec::new(),
    };
    let Some(dir) = projects_dir.filter(|d| fs::metadata(d).map(|m| m.is_dir()).unwrap_or(false))
    else {
        return empty;
    };
    let now = crate::ports::clock().now_millis();
    let cutoff = now - i64::from(days) * 86_400_000;
    let id_pattern = Regex::new(SESSION_ID).unwrap();

    let mut candidates: Vec<(PathBuf, i64)> = Vec::new();
    if let Ok(projects) = fs::read_dir(dir) {
        for project in projects.flatten() {
            let project_path = project.path();
            if !fs::metadata(&project_path)
                .map(|m| m.is_dir())
                .unwrap_or(false)
            {
                continue;
            }
            let Ok(files) = fs::read_dir(&project_path) else {
                continue;
            };
            for f in files.flatten() {
                let path = f.path();
                let name = f.file_name().to_string_lossy().into_owned();
                let Some(id) = name.strip_suffix(".jsonl") else {
                    continue;
                };
                if !fs::metadata(&path).map(|m| m.is_file()).unwrap_or(false)
                    || !id_pattern.is_match(id)
                {
                    continue;
                }
                if let Some(modified) = modified_millis(&path)
                    && modified >= cutoff
                {
                    candidates.push((path, modified));
                }
            }
        }
    }
    // 마지막 수정 시각 내림차순, 같으면 파일 이름 차례
    candidates.sort_by(|a, b| {
        b.1.cmp(&a.1).then_with(|| {
            compare_utf16(
                &a.0.file_name().unwrap_or_default().to_string_lossy(),
                &b.0.file_name().unwrap_or_default().to_string_lossy(),
            )
        })
    });
    // 머리 64KB에서 entrypoint가 cli인 것만 남긴다
    let mut cli: Vec<(PathBuf, i64, String)> = Vec::new();
    for (path, modified) in candidates {
        let head = fs::read_range(&path, 0, CHUNK_BYTES)
            .map(|b| decode(&b))
            .unwrap_or_default();
        if first_match(&head, &ENTRYPOINT).as_deref() == Some("cli") {
            cli.push((path, modified, head));
        }
    }
    let total = cli.len();
    let limit = total.min(200);
    let sessions = cli
        .into_iter()
        .take(limit)
        .map(|(path, modified, head)| {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let id = name.strip_suffix(".jsonl").unwrap_or(&name).to_string();
            let started_at = first_match(&head, &TIMESTAMP)
                .and_then(|ts| parse_instant_strict(&ts))
                .map(|nanos| nanos.div_euclid(1_000_000) as i64);
            let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let ht = head_and_tail(&path, size);
            RecentEntry {
                session_id: id,
                last_active_at: modified,
                started_at,
                title: resolve_title(&path, &ht),
                branch: resolve_branch(&ht),
                cwd: resolve_cwd(&ht),
            }
        })
        .collect();
    Recent {
        days,
        dropped: total - limit,
        sessions,
    }
}
