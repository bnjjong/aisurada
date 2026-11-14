//! 파일에 쓰는 JSON 글. Java 엔진의 `JsonWriter.toFileBytes`와 같은 모양이다: 들여쓰기 두 칸, `"키": 값`, 빈 객체·배열은 `{}`·`[]`,
//! 끝에 줄바꿈 하나. 객체의 키 차례는 넣은 차례 그대로다.

use crate::json::JsonValue;

/// 글자 하나를 따옴표로 감싸 내보낸다. `"`, `\`, 제어 문자는 이스케이프하고 나머지(U+007F 이상 포함)는 그대로 둔다.
fn append_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) <= 0x1F => out.push_str(&format!("\\u00{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn append_value(out: &mut String, value: &JsonValue, depth: usize) {
    match value {
        JsonValue::Str(text) => append_string(out, text),
        JsonValue::Literal(raw) => out.push_str(raw),
        JsonValue::Object(entries) if entries.is_empty() => out.push_str("{}"),
        JsonValue::Object(entries) => {
            out.push_str("{\n");
            for (i, (key, item)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push_str(",\n");
                }
                out.push_str(&" ".repeat((depth + 1) * 2));
                append_string(out, key);
                out.push_str(": ");
                append_value(out, item, depth + 1);
            }
            out.push('\n');
            out.push_str(&" ".repeat(depth * 2));
            out.push('}');
        }
        JsonValue::Array(items) if items.is_empty() => out.push_str("[]"),
        JsonValue::Array(items) => {
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push_str(",\n");
                }
                out.push_str(&" ".repeat((depth + 1) * 2));
                append_value(out, item, depth + 1);
            }
            out.push('\n');
            out.push_str(&" ".repeat(depth * 2));
            out.push(']');
        }
    }
}

/// 값을 파일에 쓸 바이트(UTF-8)로 바꾼다.
pub fn to_file_bytes(value: &JsonValue) -> Vec<u8> {
    let mut out = String::new();
    append_value(&mut out, value, 0);
    out.push('\n');
    out.into_bytes()
}

/// 글 값
pub fn string(text: impl Into<String>) -> JsonValue {
    JsonValue::Str(text.into())
}

/// 따옴표 없는 글 값(`null`, `true`, 숫자)
pub fn literal(raw: impl Into<String>) -> JsonValue {
    JsonValue::Literal(raw.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_two_space_json_with_a_trailing_newline() {
        let value = JsonValue::Object(vec![
            ("v".into(), literal("1")),
            (
                "comments".into(),
                JsonValue::Array(vec![
                    JsonValue::Object(vec![
                        ("id".into(), string("cm1-a")),
                        ("body".into(), string("줄\n\"끝\"\t\\\u{1}\u{7f}😀")),
                    ]),
                    JsonValue::Object(vec![]),
                ]),
            ),
            ("empty".into(), JsonValue::Array(vec![])),
        ]);
        assert_eq!(
            String::from_utf8(to_file_bytes(&value)).unwrap(),
            "{\n  \"v\": 1,\n  \"comments\": [\n    {\n      \"id\": \"cm1-a\",\n      \"body\": \"줄\\n\\\"끝\\\"\\t\\\\\\u0001\u{7f}😀\"\n    },\n    {}\n  ],\n  \"empty\": []\n}\n"
        );
    }
}
