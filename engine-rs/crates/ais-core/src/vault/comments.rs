//! 노트와 후속 노트의 코멘트 파일(`comments.json`, `sub-comments/<id>.json`) 읽기.

use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;
use serde_json::Value as Json;

use crate::text::{compare_utf16, cut_utf16};

static COMMENT_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^cm[a-z0-9]+-[a-z0-9]{1,8}$").unwrap());

const MAX_FILE_BYTES: u64 = 200_000;
const MAX_BODY_UNITS: usize = 20_000;
const MAX_QUOTE_UNITS: usize = 2_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anchor: Option<Anchor>,
    pub body: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Anchor {
    /// 코멘트가 가리킨 본문의 글
    pub quote: String,
}

/// 코멘트 파일을 읽은 결과. JSON으로는 `{state: "absent"}`, `{state: "unreadable", reason}`, `{state: "loaded", comments}`가 된다.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum CommentsEnvelope {
    /// 파일이 없다
    Absent,
    /// 파일이 있는데 읽지 못했다(너무 크거나 JSON이 아니다)
    Unreadable {
        reason: String,
    },
    Loaded {
        comments: Vec<Comment>,
    },
}

fn string_field<'a>(object: &'a serde_json::Map<String, Json>, key: &str) -> Option<&'a str> {
    object.get(key).and_then(Json::as_str)
}

/// 코멘트 파일을 읽습니다. 규칙에 맞지 않는 항목은 조용히 빼고, 같은 id는 처음 것만 두고, 만든 시각과 id 차례로 늘어놓는다.
pub fn read_comments(vault_root: &Path, file: &Path) -> CommentsEnvelope {
    if !fs::metadata(file).map(|m| m.is_file()).unwrap_or(false) {
        return CommentsEnvelope::Absent;
    }
    let rel = file
        .strip_prefix(vault_root)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/");
    let size = fs::metadata(file).map(|m| m.len()).unwrap_or(0);
    if size > MAX_FILE_BYTES {
        return CommentsEnvelope::Unreadable {
            reason: format!("파일이 너무 크다({size}바이트). 상한은 200000바이트다."),
        };
    }
    let Ok(bytes) = fs::read(file) else {
        return CommentsEnvelope::Unreadable {
            reason: format!("파일을 못 읽었다: {rel}"),
        };
    };
    let text = String::from_utf8_lossy(&bytes);
    let Ok(json) = serde_json::from_str::<Json>(text.strip_prefix('\u{FEFF}').unwrap_or(&text))
    else {
        return CommentsEnvelope::Unreadable {
            reason: format!("내용이 JSON 이 아니다: {rel}"),
        };
    };

    // 배열이면 그 항목들, 객체면 `comments` 배열의 항목들 가운데 객체만 본다
    let candidates: Vec<&Json> = match &json {
        Json::Array(items) => items.iter().collect(),
        Json::Object(object) => object
            .get("comments")
            .and_then(Json::as_array)
            .map(|a| a.iter().collect())
            .unwrap_or_default(),
        _ => Vec::new(),
    };

    let mut claimed: Vec<String> = Vec::new();
    let mut comments: Vec<Comment> = Vec::new();
    for candidate in candidates {
        let Some(object) = candidate.as_object() else {
            continue;
        };
        let Some(id) = string_field(object, "id").filter(|id| COMMENT_ID.is_match(id)) else {
            continue;
        };
        if claimed.iter().any(|c| c == id) {
            continue;
        }
        claimed.push(id.to_string());
        let Some(created_at) = string_field(object, "createdAt").filter(|c| !c.is_empty()) else {
            continue;
        };
        let Some(body) = string_field(object, "body") else {
            continue;
        };
        let quote = object
            .get("anchor")
            .and_then(Json::as_object)
            .and_then(|a| string_field(a, "quote"))
            .filter(|q| !q.is_empty())
            .map(|q| Anchor {
                quote: cut_utf16(q, MAX_QUOTE_UNITS),
            });
        comments.push(Comment {
            id: id.to_string(),
            created_at: created_at.to_string(),
            edited_at: string_field(object, "editedAt")
                .filter(|e| !e.is_empty())
                .map(str::to_string),
            anchor: quote,
            body: cut_utf16(body, MAX_BODY_UNITS),
        });
    }
    comments.sort_by(|a, b| {
        compare_utf16(&a.created_at, &b.created_at).then_with(|| compare_utf16(&a.id, &b.id))
    });
    CommentsEnvelope::Loaded { comments }
}
