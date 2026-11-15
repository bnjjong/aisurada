//! 4부 세션 기록 읽기: 에이전트(Claude, Codex, Gemini CLI, Antigravity)가 남긴 대화 기록 파일을 같은 모양의 회차 목록으로 바꾼다.
//! 규칙은 외부 설명서 7장과 Java 엔진의 `session` 패키지를 따른다. 기록은 vault 밖(사용자 홈 아래)에 있고 읽기만 한다.
//!
//! Claude 기록은 `claude`가 읽고, 세션 id로 Claude 기록을 못 찾았을 때만 `codex`, `gemini`, `antigravity`를 차례로 찾는다.

pub mod antigravity;
pub mod claude;
pub mod codex;
pub mod gemini;
pub mod protobuf;

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value as Json, json};

use crate::json::{self, JsonValue};
use crate::task::format::{java_blank, java_trim};
use crate::text::{is_blank, trim, utf16_len};

pub const MAX_TEXT_CHARS: usize = 2000;
const MAX_TITLE_CHARS: usize = 60;

pub const CODEX: &str = "codex";
pub const GEMINI: &str = "gemini";
/// Antigravity(agy)의 대화 기록임을 알리는 `client` 값. Gemini CLI와 같은 agent(gemini)지만 이어 여는 명령이 다르다.
pub const CLIENT_ANTIGRAVITY: &str = "antigravity";

static URL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"https?://[^ \t\n\x0B\x0C\r<>"'`)\]}]+"#).unwrap());
const LOCAL_HOSTS: [&str; 4] = ["localhost", "ipc.localhost", "asset.localhost", "127.0.0.1"];

/// 한 회차. `who`는 user, assistant, tool 가운데 하나다.
#[derive(Clone, Default)]
pub struct Turn {
    pub index: usize,
    pub who: &'static str,
    pub at: Option<String>,
    /// 자르기 전의 글. 응답에는 2000자까지만 싣고 `chars`는 전체 길이다.
    pub text: String,
    pub tool_name: Option<String>,
    pub tool_arg: Option<String>,
    pub tool_seconds: Option<f64>,
    pub tool_failed: bool,
    /// Claude: 도구 호출 id와 입력(결과 줄과 짝지을 때 쓴다)
    pub tool_use_id: Option<String>,
    pub tool_input: Option<Vec<(String, JsonValue)>>,
}

/// 파서가 한 기록 파일에서 읽어 낸 것
pub struct Parsed {
    pub turns: Vec<Turn>,
    pub lines: usize,
    pub skipped: usize,
    pub cwd: Option<String>,
    pub model: Option<String>,
}

/// 찾은 기록. `agent`는 codex 또는 gemini다.
pub struct Located {
    pub agent: &'static str,
    pub file: PathBuf,
}

impl Located {
    /// 기록을 연 프로그램을 agent보다 더 좁게 가를 때의 이름. Antigravity면 `antigravity`, 아니면 `None`이다.
    pub fn client(&self) -> Option<&'static str> {
        antigravity::is_antigravity_file(&self.file).then_some(CLIENT_ANTIGRAVITY)
    }
}

// ---------------------------------------------------------------- 위치

fn home_for(variable: &str) -> Option<PathBuf> {
    let custom = std::env::var(variable).unwrap_or_default();
    if !custom.trim().is_empty() {
        return Some(PathBuf::from(custom.trim()));
    }
    let home = std::env::var("HOME").unwrap_or_default();
    (!home.trim().is_empty()).then(|| PathBuf::from(home))
}

/// Claude 프로젝트 폴더(`~/.claude/projects`). 환경 변수 `AISURADA_CLAUDE_HOME`으로 홈을 바꿀 수 있다.
pub fn claude_projects_dir() -> Option<PathBuf> {
    home_for("AISURADA_CLAUDE_HOME").map(|h| h.join(".claude/projects"))
}

/// Codex 기록 폴더(`~/.codex/sessions`). 환경 변수 `AISURADA_CODEX_HOME`으로 홈을 바꿀 수 있다.
pub fn codex_sessions_dir() -> Option<PathBuf> {
    home_for("AISURADA_CODEX_HOME").map(|h| h.join(".codex/sessions"))
}

/// Gemini 기록 폴더(`~/.gemini/tmp`). 환경 변수 `AISURADA_GEMINI_HOME`으로 홈을 바꿀 수 있다.
pub fn gemini_tmp_dir() -> Option<PathBuf> {
    home_for("AISURADA_GEMINI_HOME").map(|h| h.join(".gemini/tmp"))
}

/// Antigravity 대화 기록 폴더. 지금 쓰는 antigravity-cli를 먼저, 예전 antigravity를 그다음에 본다.
pub fn antigravity_dirs() -> Vec<PathBuf> {
    match home_for("AISURADA_GEMINI_HOME") {
        None => Vec::new(),
        Some(home) => {
            let gemini = home.join(".gemini");
            vec![
                gemini.join("antigravity-cli/conversations"),
                gemini.join("antigravity/conversations"),
            ]
        }
    }
}

/// 세션 id로 Codex나 Gemini 기록을 찾는다. 없으면 `None`이다.
pub fn locate(session_id: &str) -> Option<Located> {
    if let Some(file) = codex::find(codex_sessions_dir().as_deref(), session_id) {
        return Some(Located { agent: CODEX, file });
    }
    if let Some(file) = gemini::find(gemini_tmp_dir().as_deref(), session_id) {
        return Some(Located {
            agent: GEMINI,
            file,
        });
    }
    antigravity::find(&antigravity_dirs(), session_id).map(|file| Located {
        agent: GEMINI,
        file,
    })
}

fn parse_located(located: &Located) -> Result<Parsed, String> {
    if located.agent == CODEX {
        codex::parse(&located.file)
    } else if antigravity::is_antigravity_file(&located.file) {
        antigravity::parse(&located.file)
    } else {
        gemini::parse(&located.file, gemini_tmp_dir().as_deref())
    }
}

/// `session-titles` 한 건. 읽을 수 없으면 `found=false`다.
pub struct Info {
    pub agent: &'static str,
    pub found: bool,
    pub title: Option<String>,
    pub cwd: Option<String>,
}

pub fn info(located: &Located) -> Info {
    match parse_located(located) {
        Ok(parsed) => {
            let title = parsed
                .turns
                .iter()
                .find(|t| t.who == "user")
                .and_then(|t| first_line_title(&t.text));
            Info {
                agent: located.agent,
                found: true,
                title,
                cwd: parsed.cwd,
            }
        }
        Err(_) => Info {
            agent: located.agent,
            found: false,
            title: None,
            cwd: None,
        },
    }
}

fn first_line_title(text: &str) -> Option<String> {
    text.split('\n')
        .map(trim)
        .find(|line| !line.is_empty())
        .map(|line| cut(line, MAX_TITLE_CHARS))
}

/// `read-session` 한 건(Claude가 아닌 에이전트). 쪽 나누기는 Claude와 같고, `author`는 지원하지 않는다.
pub fn read_located(
    located: &Located,
    limit: i32,
    before: Option<i32>,
    include_urls: bool,
    note_id: Option<&str>,
) -> Result<ReadResult, String> {
    let parsed = parse_located(located)?;
    let bytes = std::fs::metadata(&located.file)
        .map_err(|e| e.to_string())?
        .len();
    let turns = parsed.turns;
    let total = turns.len();
    let effective_limit = limit.clamp(1, 500) as usize;
    let end = before.map_or(total, |b| b.clamp(0, total as i32) as usize);
    let page = slice(&turns, effective_limit, end);
    let has_more = page.first().is_some_and(|t| t.index > 0);

    let mut urls: Vec<(String, i64)> = Vec::new();
    if include_urls {
        for turn in &turns {
            extract_urls(&turn.text, &mut urls);
        }
        urls.sort_by_key(|u| std::cmp::Reverse(u.1));
    }
    let run_state = parsed
        .model
        .as_ref()
        .map(|m| json!({"model": m, "quotaBlocked": false}));
    let mentions = note_id.map(|id| {
        turns
            .iter()
            .filter(|t| t.who != "tool" && t.text.contains(id))
            .map(|t| t.index)
            .collect::<Vec<_>>()
    });
    Ok(ReadResult {
        session_id: String::new(),
        file: Some(located.file.to_string_lossy().into_owned()),
        bytes,
        lines: parsed.lines,
        skipped: parsed.skipped,
        messages: total,
        has_more,
        urls,
        running_agents: Vec::new(),
        todos: Vec::new(),
        run_state,
        note_mentions: mentions,
        author: None,
        turns: page.iter().map(turn_json).collect(),
    })
}

/// `read-session`이 내는 것(응답 칸 모양)
pub struct ReadResult {
    pub session_id: String,
    pub file: Option<String>,
    pub bytes: u64,
    pub lines: usize,
    pub skipped: usize,
    pub messages: usize,
    pub has_more: bool,
    pub urls: Vec<(String, i64)>,
    pub running_agents: Vec<Json>,
    pub todos: Vec<Json>,
    pub run_state: Option<Json>,
    pub note_mentions: Option<Vec<usize>>,
    pub author: Option<Json>,
    pub turns: Vec<Json>,
}

// ---------------------------------------------------------------- 쪽 나누기와 응답 모양

/// 끝 자리(`end`)에서 거꾸로 도구가 아닌 회차를 `limit`개 센다.
pub fn slice(turns: &[Turn], limit: usize, end: usize) -> &[Turn] {
    let end = end.min(turns.len());
    if end == 0 {
        return &[];
    }
    let mut start = end;
    let mut non_tool = 0;
    while start > 0 && non_tool < limit {
        start -= 1;
        if turns[start].who != "tool" {
            non_tool += 1;
        }
    }
    &turns[start..end]
}

pub fn turn_json(t: &Turn) -> Json {
    let chars = utf16_len(&t.text);
    let mut m = Map::new();
    m.insert("index".into(), json!(t.index));
    m.insert("who".into(), json!(t.who));
    m.insert("at".into(), json!(t.at));
    m.insert("chars".into(), json!(chars));
    m.insert("truncated".into(), json!(chars > MAX_TEXT_CHARS));
    if t.who == "tool" {
        let mut tool = Map::new();
        tool.insert("name".into(), json!(t.tool_name));
        if let Some(arg) = &t.tool_arg {
            tool.insert("arg".into(), json!(arg));
        }
        if let Some(seconds) = t.tool_seconds {
            tool.insert("seconds".into(), json!(seconds));
        }
        if t.tool_failed {
            tool.insert("failed".into(), json!(true));
        }
        m.insert("tool".into(), Json::Object(tool));
    }
    m.insert(
        "text".into(),
        json!(crate::text::cut_utf16(&t.text, MAX_TEXT_CHARS)),
    );
    Json::Object(m)
}

/// 글에서 주소를 모아 센다. 끝의 `.,;:!?`는 걷고, 10자 이하와 이 컴퓨터를 가리키는 주소는 뺀다.
pub fn extract_urls(text: &str, counts: &mut Vec<(String, i64)>) {
    for m in URL.find_iter(text) {
        let candidate = m.as_str().trim_end_matches(['.', ',', ';', ':', '!', '?']);
        if utf16_len(candidate) <= 10 {
            continue;
        }
        let after_scheme = &candidate[candidate.find("://").map_or(0, |i| i + 3)..];
        let authority = after_scheme.split('/').next().unwrap_or("");
        let host = authority.split(':').next().unwrap_or("");
        if LOCAL_HOSTS.contains(&host) {
            continue;
        }
        match counts.iter_mut().find(|(url, _)| url == candidate) {
            Some(slot) => slot.1 += 1,
            None => counts.push((candidate.to_string(), 1)),
        }
    }
}

pub fn urls_json(urls: &[(String, i64)]) -> Vec<Json> {
    urls.iter()
        .map(|(url, count)| json!({"url": url, "count": count}))
        .collect()
}

// ---------------------------------------------------------------- 파서가 같이 쓰는 도우미

/// `BufferedReader.readLine`처럼 `\n`, `\r\n`, `\r` 어느 것으로도 줄을 끊는다. 끝에 줄바꿈이 없는 마지막 조각도 한 줄이다.
pub fn reader_lines(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\n' => out.push(std::mem::take(&mut current)),
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push(std::mem::take(&mut current));
            }
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// 파일을 줄로 읽는다. UTF-8이 깨져도 대체 글자로 읽는다(D18).
pub fn read_lines(path: &Path) -> Result<Vec<String>, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    Ok(reader_lines(&String::from_utf8_lossy(&bytes)))
}

/// Claude 쪽: 글 하나를 회차에 더한다(공백뿐이면 버린다).
pub fn add_text_turn(turns: &mut Vec<Turn>, who: &'static str, at: Option<&str>, text: &str) {
    add_text(turns, who, at, Some(text.to_string()));
}

/// 공백뿐인 글은 버리고 순번을 붙여 회차에 더한다(7.4).
pub fn add_text(turns: &mut Vec<Turn>, who: &'static str, at: Option<&str>, text: Option<String>) {
    let Some(text) = text else {
        return;
    };
    if is_blank(&text) {
        return;
    }
    turns.push(Turn {
        index: turns.len(),
        who,
        at: at.map(str::to_string),
        text,
        ..Turn::default()
    });
}

pub fn add_tool(
    turns: &mut Vec<Turn>,
    at: Option<&str>,
    name: Option<String>,
    arg: Option<String>,
    seconds: Option<f64>,
    failed: bool,
) {
    turns.push(Turn {
        index: turns.len(),
        who: "tool",
        at: at.map(str::to_string),
        tool_name: name,
        tool_arg: arg.filter(|a| !java_blank(a)).map(|a| cut(&a, 160)),
        tool_seconds: seconds,
        tool_failed: failed,
        ..Turn::default()
    });
}

/// 줄바꿈을 공백으로 바꿔 한 줄로 만들고, 앞뒤를 걷고, `max`(UTF-16 단위)를 넘으면 자르고 `…`를 붙인다.
pub fn cut(text: &str, max: usize) -> String {
    let one_line = text.replace(['\n', '\r'], " ");
    let one_line = java_trim(&one_line);
    if utf16_len(one_line) <= max {
        one_line.to_string()
    } else {
        format!("{}…", crate::text::cut_utf16(one_line, max - 1))
    }
}

/// 한 줄을 JSON 객체로 읽는다. 객체가 아니거나 못 읽으면 `None`이다.
pub fn read_object(line: &str) -> Option<Vec<(String, JsonValue)>> {
    match json::parse(line) {
        Ok(JsonValue::Object(entries)) => Some(entries),
        _ => None,
    }
}

pub fn get<'a>(object: &'a [(String, JsonValue)], key: &str) -> Option<&'a JsonValue> {
    object.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

pub fn str_of(value: Option<&JsonValue>) -> Option<&str> {
    match value {
        Some(JsonValue::Str(s)) => Some(s),
        _ => None,
    }
}

pub fn str_key<'a>(object: &'a [(String, JsonValue)], key: &str) -> Option<&'a str> {
    str_of(get(object, key))
}

pub fn obj_of(value: Option<&JsonValue>) -> Option<&[(String, JsonValue)]> {
    match value {
        Some(JsonValue::Object(entries)) => Some(entries),
        _ => None,
    }
}

pub fn arr_of(value: Option<&JsonValue>) -> Option<&[JsonValue]> {
    match value {
        Some(JsonValue::Array(items)) => Some(items),
        _ => None,
    }
}

/// 수 글자(`Double.parseDouble`)
pub fn number_of(value: Option<&JsonValue>) -> Option<f64> {
    match value {
        Some(JsonValue::Literal(raw)) => raw.parse::<f64>().ok(),
        _ => None,
    }
}
