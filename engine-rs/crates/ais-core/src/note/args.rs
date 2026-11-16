//! 노트 명령의 목록 인자(`tags`, `links`)를 읽어 검사하는 규칙. 호출 객체에서는 배열, 명령줄에서는 JSON 배열 글로 온다.
//! 값만 다루고 파일도 vault도 모른다.

use serde_json::Value as Json;

use super::meta::Link;
use crate::error::EngineError;
use crate::json::{self, JsonValue};
use crate::kernel::java::to_json_value;
use crate::kernel::link_kind::LinkKind;
use crate::text::{NOTE_ID, compare_utf16, is_blank, trim, utf16_len};

/// `tags` 인자를 읽어 다듬는다. 공백뿐인 항목은 버리고, 다듬은 뒤 줄바꿈이 있으면 거절하고, 같은 글은 처음 하나만 남긴다.
pub fn parse_tags(raw: &Json) -> Result<Vec<String>, EngineError> {
    let items = match raw {
        Json::String(text) => {
            let not_array =
                || EngineError::invalid_argument(format!("tags 은 JSON 배열이어야 한다: {text}"));
            match json::parse(text).map_err(|_| not_array())? {
                JsonValue::Array(items) => items,
                _ => return Err(not_array()),
            }
        }
        Json::Array(items) => items.iter().map(to_json_value).collect(),
        other => {
            return Err(EngineError::invalid_argument(format!(
                "tags 은 JSON 배열이어야 한다: {other}"
            )));
        }
    };
    let mut result: Vec<String> = Vec::new();
    for item in items {
        let JsonValue::Str(text) = item else {
            return Err(EngineError::invalid_argument(format!(
                "tags 항목이 문자열이 아니다: {}",
                json::compact(&item)
            )));
        };
        if is_blank(&text) {
            continue;
        }
        let trimmed = trim(&text);
        if trimmed.contains(['\n', '\r']) {
            return Err(EngineError::invalid_argument(format!(
                "태그에 줄바꿈이 있다: '{trimmed}'"
            )));
        }
        if !result.iter().any(|seen| seen == trimmed) {
            result.push(trimmed.to_string());
        }
    }
    Ok(result)
}

/// `links` 인자를 읽어 검사한다. 같은 `to`와 `kind`는 처음 하나만 남긴다.
pub fn parse_links(raw: &Json) -> Result<Vec<Link>, EngineError> {
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
