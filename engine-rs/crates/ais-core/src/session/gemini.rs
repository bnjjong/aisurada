//! Gemini 기록(`~/.gemini/tmp/<프로젝트>/chats/session-*.jsonl`) 읽기.
//!
//! 첫 줄이 `{sessionId, projectHash, startTime, kind}` 머리이고, 그 뒤로 `user`와 `gemini` 메시지가 한 줄씩 오고, `$set`(갱신 시각)과
//! `$rewindTo`(그 메시지부터 되감기)가 끼어든다. 기록 파일에는 작업 폴더가 없어서, 프로젝트 폴더 이름을 `~/.gemini/projects.json`으로
//! 거꾸로 찾는다.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::codex::join_text;
use super::{
    Parsed, Turn, add_text, add_tool, arr_of, get, obj_of, read_lines, read_object, str_key, str_of,
};
use crate::json::{self, JsonValue};
use crate::kernel::java::java_blank;

/// 대화가 시작될 때 CLI가 사용자 줄로 끼워 넣는 환경 설명. 사용자가 쓴 글이 아니다.
const INJECTED_PREFIX: &str = "<session_context>";

/// 첫 줄 머리의 sessionId. 못 읽으면 `None`이다.
fn header_session_id(file: &Path) -> Option<String> {
    let lines = read_lines(file).ok()?;
    let first = lines.first()?;
    str_key(&read_object(first)?, "sessionId").map(str::to_string)
}

/// `session-<시각>-<id 앞 8자>.jsonl`. 이름만 믿지 않고 첫 줄의 sessionId를 맞대어 본다.
pub fn find(root: Option<&Path>, session_id: &str) -> Option<PathBuf> {
    let root = root.filter(|r| fs::metadata(r).map(|m| m.is_dir()).unwrap_or(false))?;
    if session_id.chars().count() < 8 {
        return None;
    }
    let tail = format!(
        "-{}.jsonl",
        session_id
            .chars()
            .take(8)
            .collect::<String>()
            .to_lowercase()
    );
    for project in fs::read_dir(root).ok()?.flatten() {
        let chats = project.path().join("chats");
        if !fs::metadata(&chats).map(|m| m.is_dir()).unwrap_or(false) {
            continue;
        }
        let Ok(files) = fs::read_dir(&chats) else {
            continue;
        };
        for file in files.flatten() {
            let name = file.file_name().to_string_lossy().into_owned();
            if !name.starts_with("session-") || !name.to_lowercase().ends_with(".jsonl") {
                continue;
            }
            if !name.to_lowercase().ends_with(&tail) {
                continue;
            }
            if header_session_id(&file.path())
                .is_some_and(|id| id.to_lowercase() == session_id.to_lowercase())
            {
                return Some(file.path());
            }
        }
    }
    None
}

pub fn parse(file: &Path, tmp_dir: Option<&Path>) -> Result<Parsed, String> {
    let mut turns: Vec<Turn> = Vec::new();
    let mut first_turn_of_message: HashMap<String, usize> = HashMap::new();
    let mut model: Option<String> = None;
    let mut line_count = 0;
    let mut skipped = 0;
    for line in read_lines(file)? {
        line_count += 1;
        if java_blank(&line) {
            continue;
        }
        let Some(record) = read_object(&line) else {
            skipped += 1;
            continue;
        };
        if let Some(rewind) = str_key(&record, "$rewindTo") {
            if let Some(&from) = first_turn_of_message.get(rewind)
                && from <= turns.len()
            {
                turns.truncate(from);
            }
            continue;
        }
        let Some(kind) = str_key(&record, "type") else {
            continue; // 머리와 $set 줄
        };
        let at = str_key(&record, "timestamp");
        if let Some(id) = str_key(&record, "id") {
            first_turn_of_message.insert(id.to_string(), turns.len());
        }
        if kind == "user" {
            let text = join_text(get(&record, "content"));
            if let Some(text) = text.filter(|t| !t.starts_with(INJECTED_PREFIX)) {
                add_text(&mut turns, "user", at, Some(text));
            }
        } else if kind == "gemini" {
            if let Some(m) = str_key(&record, "model") {
                model = Some(m.to_string());
            }
            add_text(
                &mut turns,
                "assistant",
                at,
                join_text(get(&record, "content")),
            );
            if let Some(calls) = arr_of(get(&record, "toolCalls")) {
                for call in calls {
                    tool(obj_of(Some(call)), at, &mut turns);
                }
            }
        }
    }
    for (i, turn) in turns.iter_mut().enumerate() {
        turn.index = i;
    }
    Ok(Parsed {
        turns,
        lines: line_count,
        skipped,
        cwd: working_directory(file, tmp_dir),
        model,
    })
}

fn tool(call: Option<&[(String, JsonValue)]>, at: Option<&str>, turns: &mut Vec<Turn>) {
    let Some(call) = call else {
        return;
    };
    let Some(name) = str_key(call, "name") else {
        return;
    };
    // 상태 칸이 있으면(null 글자도 값이다) `success`가 아닐 때 실패다
    let failed = get(call, "status").is_some() && str_key(call, "status") != Some("success");
    add_tool(
        turns,
        at,
        Some(name.to_string()),
        summarize(obj_of(get(call, "args"))),
        None,
        failed,
    );
}

/// 인자 가운데 사람이 읽을 만한 것 하나를 고른다. Claude 쪽 도구 요약과 같은 키 차례다.
fn summarize(args: Option<&[(String, JsonValue)]>) -> Option<String> {
    let args = args?;
    [
        "description",
        "file_path",
        "pattern",
        "query",
        "command",
        "url",
        "dir_path",
    ]
    .iter()
    .find_map(|key| str_key(args, key).filter(|v| !java_blank(v)))
    .map(str::to_string)
}

/// `…/tmp/<프로젝트 이름>/chats/파일`에서 프로젝트 이름을 꺼내 `projects.json`을 거꾸로 찾는다.
fn working_directory(file: &Path, tmp_dir: Option<&Path>) -> Option<String> {
    let project = file.parent()?.parent()?;
    let name = project.file_name()?.to_string_lossy().into_owned();
    let tmp_dir = tmp_dir?;
    let projects_file = tmp_dir.parent()?.join("projects.json");
    if !fs::metadata(&projects_file)
        .map(|m| m.is_file())
        .unwrap_or(false)
    {
        return None;
    }
    let text = String::from_utf8(fs::read(&projects_file).ok()?).ok()?;
    let JsonValue::Object(root) = json::parse(&text).ok()? else {
        return None;
    };
    let projects = obj_of(get(&root, "projects"))?;
    projects
        .iter()
        .find(|(_, v)| str_of(Some(v)) == Some(name.as_str()))
        .map(|(k, _)| k.clone())
}
