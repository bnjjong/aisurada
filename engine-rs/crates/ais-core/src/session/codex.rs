//! Codex 기록(`~/.codex/sessions/연/월/일/rollout-*.jsonl`) 읽기.
//!
//! 한 줄이 `{timestamp, type, payload}` 하나다. 새 형식은 `event_msg`의 `item_completed`에 사용자 메시지, 에이전트 메시지, 명령 실행,
//! 도구 호출이 정리되어 담기고, 예전 형식은 `user_message`, `agent_message`와 `response_item`의 함수 호출이다. 둘이 한 파일에 섞이지
//! 않으므로 한 번 훑으면서 두 갈래를 따로 모으고, 새 형식이 있으면 그것을 쓴다.

use crate::ports::fs;
use std::path::{Path, PathBuf};

use super::{
    Parsed, Turn, add_text, add_tool, arr_of, get, number_of, obj_of, read_lines, read_object,
    str_key, str_of,
};
use crate::json::JsonValue;
use crate::kernel::java::java_blank;

type Object = [(String, JsonValue)];

/// `rollout-<시각>-<id>.jsonl`. 연/월/일 폴더 아래에 있어서 깊이 4까지 본다.
pub fn find(root: Option<&Path>, session_id: &str) -> Option<PathBuf> {
    let root = root.filter(|r| fs::metadata(r).map(|m| m.is_dir()).unwrap_or(false))?;
    let suffix = format!("-{}.jsonl", session_id.to_lowercase());
    let mut stack: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        entries.sort();
        for path in entries {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let is_dir = fs::metadata(&path).map(|m| m.is_dir()).unwrap_or(false);
            if is_dir && depth < 3 {
                stack.push((path.clone(), depth + 1));
            }
            if name.starts_with("rollout-")
                && name.to_lowercase().ends_with(&suffix)
                && fs::metadata(&path).map(|m| m.is_file()).unwrap_or(false)
            {
                return Some(path);
            }
        }
    }
    None
}

pub fn parse(file: &Path) -> Result<Parsed, String> {
    let mut items: Vec<Turn> = Vec::new();
    let mut legacy: Vec<Turn> = Vec::new();
    let mut cwd: Option<String> = None;
    let mut model: Option<String> = None;
    let mut line_count = 0;
    let mut skipped = 0;
    for line in read_lines(file)? {
        line_count += 1;
        if java_blank(&line) {
            continue;
        }
        // 필요한 줄만 읽는다: 대화, 도구, 위치와 모델이 든 줄
        let wanted = [
            "\"item_completed\"",
            "\"user_message\"",
            "\"agent_message\"",
            "\"function_call\"",
            "\"session_meta\"",
            "\"turn_context\"",
        ]
        .iter()
        .any(|m| line.contains(m));
        if !wanted {
            continue;
        }
        let Some(record) = read_object(&line) else {
            skipped += 1;
            continue;
        };
        let (Some(kind), Some(payload)) =
            (str_key(&record, "type"), obj_of(get(&record, "payload")))
        else {
            continue;
        };
        let at = str_key(&record, "timestamp");
        match kind {
            "session_meta" => {
                if cwd.is_none() {
                    cwd = str_key(payload, "cwd").map(str::to_string);
                }
            }
            "turn_context" => {
                if let Some(m) = str_key(payload, "model") {
                    model = Some(m.to_string());
                }
                if cwd.is_none() {
                    cwd = str_key(payload, "cwd").map(str::to_string);
                }
            }
            "event_msg" => match str_key(payload, "type") {
                Some("item_completed") => item(obj_of(get(payload, "item")), at, &mut items),
                Some("user_message") => add_text(
                    &mut legacy,
                    "user",
                    at,
                    str_key(payload, "message").map(str::to_string),
                ),
                Some("agent_message") => add_text(
                    &mut legacy,
                    "assistant",
                    at,
                    str_key(payload, "message").map(str::to_string),
                ),
                _ => {}
            },
            "response_item" if str_key(payload, "type") == Some("function_call") => add_tool(
                &mut legacy,
                at,
                str_key(payload, "name").map(str::to_string),
                legacy_argument(str_key(payload, "arguments")),
                None,
                false,
            ),
            _ => {}
        }
    }
    let turns = if items.is_empty() { legacy } else { items };
    Ok(Parsed {
        turns,
        lines: line_count,
        skipped,
        cwd,
        model,
    })
}

/// 새 형식의 항목 하나를 회차로 바꾼다. 생각(Reasoning)과 맥락 압축 따위는 버린다.
fn item(item: Option<&Object>, at: Option<&str>, turns: &mut Vec<Turn>) {
    let Some(item) = item else {
        return;
    };
    let Some(kind) = str_key(item, "type") else {
        return;
    };
    match kind {
        "UserMessage" => add_text(turns, "user", at, join_text(get(item, "content"))),
        "AgentMessage" => add_text(turns, "assistant", at, join_text(get(item, "content"))),
        "CommandExecution" => {
            let exit = number_of(get(item, "exit_code"));
            add_tool(
                turns,
                at,
                Some("shell".to_string()),
                command_text(get(item, "command")),
                seconds(obj_of(get(item, "duration"))),
                exit.is_some_and(|e| e != 0.0),
            );
        }
        "McpToolCall" => {
            let server = str_key(item, "server");
            let tool = str_key(item, "tool");
            let failed = str_key(item, "status") != Some("completed");
            let name = format!(
                "{}{}",
                server.map_or(String::new(), |s| format!("{s}/")),
                tool.unwrap_or("mcp")
            );
            add_tool(
                turns,
                at,
                Some(name),
                None,
                seconds(obj_of(get(item, "duration"))),
                failed,
            );
        }
        "FileChange" => add_tool(
            turns,
            at,
            Some("apply_patch".to_string()),
            changed_files(obj_of(get(item, "changes"))),
            None,
            str_key(item, "status") != Some("completed"),
        ),
        _ => {}
    }
}

/// content 배열의 글 조각을 이어 붙인다. 조각 종류 이름은 버전마다 text, Text, input_text, output_text로 다르다.
pub fn join_text(content: Option<&JsonValue>) -> Option<String> {
    let Some(parts) = arr_of(content) else {
        return str_of(content).map(str::to_string);
    };
    let mut out = String::new();
    for part in parts {
        if let Some(text) = str_of(obj_of(Some(part)).and_then(|o| get(o, "text"))) {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(text);
        }
    }
    Some(out)
}

/// `["/bin/zsh","-lc","실제 명령"]` 꼴이면 마지막 칸만, 아니면 칸을 공백으로 이어서 낸다.
fn command_text(command: Option<&JsonValue>) -> Option<String> {
    let Some(parts) = arr_of(command).filter(|p| !p.is_empty()) else {
        return str_of(command).map(str::to_string);
    };
    let words: Vec<&str> = parts.iter().filter_map(|p| str_of(Some(p))).collect();
    if words.len() >= 3 && words[1].ends_with('c') && words[1].starts_with('-') {
        return Some(words[words.len() - 1].to_string());
    }
    Some(words.join(" "))
}

fn seconds(duration: Option<&Object>) -> Option<f64> {
    let duration = duration?;
    let secs = number_of(get(duration, "secs"));
    let nanos = number_of(get(duration, "nanos"));
    if secs.is_none() && nanos.is_none() {
        return None;
    }
    let total = secs.unwrap_or(0.0) + nanos.unwrap_or(0.0) / 1e9;
    Some((total * 10.0 + 0.5).floor() / 10.0)
}

fn changed_files(changes: Option<&Object>) -> Option<String> {
    let changes = changes.filter(|c| !c.is_empty())?;
    let names: Vec<&str> = changes
        .iter()
        .map(|(path, _)| {
            path.rsplit_once('/')
                .map_or(path.as_str(), |(_, name)| name)
        })
        .collect();
    Some(names.join(", "))
}

/// 예전 형식의 arguments는 JSON 글이다: `{"command":["bash","-lc","..."]}`.
fn legacy_argument(arguments: Option<&str>) -> Option<String> {
    let arguments = arguments?;
    let Some(parsed) = read_object(arguments) else {
        return Some(arguments.to_string());
    };
    Some(command_text(get(&parsed, "command")).unwrap_or_else(|| arguments.to_string()))
}
