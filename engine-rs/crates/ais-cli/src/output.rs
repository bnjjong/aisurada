//! 표준 출력 JSON 기록기. Java `JsonWriter`와 같은 모양(들여쓰기 2칸, LF, 빈 묶음은 한 줄, 끝에 LF 하나)이다.
//!
//! 키 차례만 다르다. Java는 넣은 차례를 지키고, 여기서는 `serde_json`이 이름 차례로 정렬한 그대로 찍는다.
//! JSON을 읽는 쪽(앱, MCP 클라이언트, 에이전트)에는 차례가 뜻을 갖지 않는다.

use serde_json::Value;

/// 값을 규격 JSON 글로 만든다. 끝에 줄바꿈 하나가 붙는다.
pub fn stringify(value: &Value) -> String {
    let mut out = String::new();
    append_value(&mut out, value, 0);
    out.push('\n');
    out
}

fn append_value(out: &mut String, value: &Value, depth: usize) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        Value::Number(number) => out.push_str(&number.to_string()),
        Value::String(text) => append_string(out, text),
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push_str("[\n");
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(",\n");
                }
                indent(out, (depth + 1) * 2);
                append_value(out, item, depth + 1);
            }
            out.push('\n');
            indent(out, depth * 2);
            out.push(']');
        }
        Value::Object(map) => {
            if map.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push_str("{\n");
            for (index, (key, item)) in map.iter().enumerate() {
                if index > 0 {
                    out.push_str(",\n");
                }
                indent(out, (depth + 1) * 2);
                append_string(out, key);
                out.push_str(": ");
                append_value(out, item, depth + 1);
            }
            out.push('\n');
            indent(out, depth * 2);
            out.push('}');
        }
    }
}

fn indent(out: &mut String, spaces: usize) {
    for _ in 0..spaces {
        out.push(' ');
    }
}

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
            c if (c as u32) <= 0x1f => out.push_str(&format!("\\u00{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn writes_two_space_indent_and_trailing_newline() {
        let text = stringify(&json!({"a": [1, 2], "b": {}, "c": []}));
        assert_eq!(
            text,
            "{\n  \"a\": [\n    1,\n    2\n  ],\n  \"b\": {},\n  \"c\": []\n}\n"
        );
    }

    #[test]
    fn escapes_control_characters_like_java() {
        let text = stringify(&json!("a\u{8}\t\n\u{c}\r\u{1}\"\\é"));
        assert_eq!(text, "\"a\\b\\t\\n\\f\\r\\u0001\\\"\\\\é\"\n");
    }
}
