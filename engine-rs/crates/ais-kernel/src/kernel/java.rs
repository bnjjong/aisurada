//! Java 엔진이 글과 수를 다루던 규칙. 기존 vault가 같은 결과를 내려면 바이트까지 같아야 해서 그대로 옮겼다.
//!
//! 공백 규칙이 둘이다. 이 파일의 `java_blank`는 U+00A0을 공백으로 **보지 않고**(Java `Character.isWhitespace`),
//! `text::is_blank`는 본다. 합치면 바이트가 바뀌므로 합치지 않는다(리팩토링 계획 §3 호환 계약).

use serde_json::Value as Json;

use crate::frontmatter::Value;
use crate::json::JsonValue;

/// Java `String.trim()`: U+0020 이하를 뗀다.
pub fn java_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c <= ' ')
}

/// Java `Character.isWhitespace`
fn java_whitespace(c: char) -> bool {
    matches!(c,
        '\t' | '\n' | '\u{B}' | '\u{C}' | '\r' | '\u{1C}'..='\u{1F}' | ' '
        | '\u{1680}' | '\u{2000}'..='\u{2006}' | '\u{2008}'..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{205F}' | '\u{3000}')
}

/// Java `String.isBlank()`
pub fn java_blank(s: &str) -> bool {
    s.chars().all(java_whitespace)
}

/// Java `String.valueOf(Object)`: 값을 글로. `None`은 Java `null`이다.
pub fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::Str(s) => Some(s.clone()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Int(text) => Some(text.clone()),
        Value::Float(f) => Some(java_double_string(*f)),
        Value::List(items) => {
            let items: Vec<String> = items
                .iter()
                .map(|v| value_to_string(v).unwrap_or_else(|| "null".to_string()))
                .collect();
            Some(format!("[{}]", items.join(", ")))
        }
        Value::Map(entries) => {
            let entries: Vec<String> = entries
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}={}",
                        value_to_string(k).unwrap_or_else(|| "null".to_string()),
                        value_to_string(v).unwrap_or_else(|| "null".to_string())
                    )
                })
                .collect();
            Some(format!("{{{}}}", entries.join(", ")))
        }
        Value::Other => Some("?".to_string()),
    }
}

/// Java `Double.toString`의 흔한 모양: 정수값도 `101.0`처럼 소수점을 달고, 아주 크거나 작으면 `1.0E10`
pub fn java_double_string(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    let abs = value.abs();
    if abs == 0.0 || (1e-3..1e7).contains(&abs) {
        let text = format!("{value}");
        return if text.contains('.') {
            text
        } else {
            format!("{text}.0")
        };
    }
    let scientific = format!("{value:e}");
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let mantissa = if mantissa.contains('.') {
        mantissa.to_string()
    } else {
        format!("{mantissa}.0")
    };
    format!("{mantissa}E{exponent}")
}

/// 호출 객체의 JSON 값을 엔진의 JSON 값으로 바꾼다(숫자는 `serde_json`이 찍는 글 그대로 남는다).
pub fn to_json_value(item: &Json) -> JsonValue {
    match item {
        Json::String(text) => JsonValue::Str(text.clone()),
        Json::Array(items) => JsonValue::Array(items.iter().map(to_json_value).collect()),
        Json::Object(entries) => JsonValue::Object(
            entries
                .iter()
                .map(|(key, value)| (key.clone(), to_json_value(value)))
                .collect(),
        ),
        other => JsonValue::Literal(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prints_doubles_like_java() {
        assert_eq!(java_double_string(101.0), "101.0");
        assert_eq!(java_double_string(-1.0), "-1.0");
        assert_eq!(java_double_string(100.5), "100.5");
        assert_eq!(java_double_string(1e10), "1.0E10");
        assert_eq!(java_double_string(1.5e-5), "1.5E-5");
        assert_eq!(java_double_string(f64::NAN), "NaN");
        assert_eq!(java_double_string(f64::INFINITY), "Infinity");
        assert_eq!(java_double_string(0.0), "0.0");
    }

    #[test]
    fn java_blank_does_not_treat_no_break_space_as_blank() {
        // 두 공백 규칙이 다르다는 사실을 못박는다(합치면 바이트가 바뀐다)
        assert!(!java_blank("\u{A0}"));
        assert!(crate::text::is_blank("\u{A0}"));
        assert!(java_blank(" \t\n"));
        assert_eq!(java_trim("\u{1}  x \u{0}"), "x");
    }
}
