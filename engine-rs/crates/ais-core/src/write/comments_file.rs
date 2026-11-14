//! 코멘트 파일(`comments.json`, `sub-comments/<id>.json`) 쓰기와 본문의 코멘트 앵커 표시 걷어내기.
//! 규칙은 Java 엔진의 `CommentsWriter`, `CommentAnchors`와 같다.

use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::{Captures, Regex};

use super::atomic::write_atomic;
use super::confine::absolute_normalized;
use super::json_out::{literal, string, to_file_bytes};
use crate::error::EngineError;
use crate::json::JsonValue;
use crate::text::compare_utf16;
use crate::vault::comments::Comment;

static MARKER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)<!--(/?)aisc:(cm[a-z0-9]+-[a-z0-9]{1,8})-->").unwrap());

/// 코멘트 목록을 파일에 쓴다. 비었으면 파일을 지운다. 아니면 만든 시각과 id 차례로 늘어놓고 `{"v": 1, "comments": […]}`로 쓴다.
pub fn write_comments(file: &Path, comments: &mut [Comment]) -> Result<(), EngineError> {
    let failed = || {
        EngineError::internal(format!(
            "파일을 쓰지 못했다: {}",
            absolute_normalized(file).display()
        ))
    };
    if comments.is_empty() {
        return match fs::remove_file(file) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(failed()),
        };
    }
    comments.sort_by(|a, b| {
        compare_utf16(&a.created_at, &b.created_at).then_with(|| compare_utf16(&a.id, &b.id))
    });
    let rows: Vec<JsonValue> = comments
        .iter()
        .map(|comment| {
            let mut row = vec![
                ("id".to_string(), string(&comment.id)),
                ("createdAt".to_string(), string(&comment.created_at)),
            ];
            if let Some(edited) = &comment.edited_at {
                row.push(("editedAt".to_string(), string(edited)));
            }
            if let Some(anchor) = &comment.anchor {
                row.push((
                    "anchor".to_string(),
                    JsonValue::Object(vec![("quote".to_string(), string(&anchor.quote))]),
                ));
            }
            row.push(("body".to_string(), string(&comment.body)));
            JsonValue::Object(row)
        })
        .collect();
    let document = JsonValue::Object(vec![
        ("v".to_string(), literal("1")),
        ("comments".to_string(), JsonValue::Array(rows)),
    ]);
    write_atomic(file, &to_file_bytes(&document))
}

/// 본문에서 `comment_id`의 앵커 표시(`<!--aisc:id-->`와 `<!--/aisc:id-->`)를 걷어낸다. 다른 코멘트의 표시는 그대로 둔다.
pub fn strip_anchors(body: &str, comment_id: &str) -> String {
    MARKER
        .replace_all(body, |captures: &Captures| {
            if &captures[2] == comment_id {
                String::new()
            } else {
                captures[0].to_string()
            }
        })
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_only_the_markers_of_that_comment() {
        let body = "앞<!--aisc:cm1-ab-->인용<!--/aisc:cm1-ab--> 사이 <!--aisc:cm2-cd-->둘<!--/AISC:cm2-cd-->";
        assert_eq!(
            strip_anchors(body, "cm1-ab"),
            "앞인용 사이 <!--aisc:cm2-cd-->둘<!--/AISC:cm2-cd-->"
        );
        assert_eq!(strip_anchors(body, "CM1-AB"), body);
        assert_eq!(strip_anchors("<!--AISC:cm1-ab-->x", "cm1-ab"), "x");
    }
}
