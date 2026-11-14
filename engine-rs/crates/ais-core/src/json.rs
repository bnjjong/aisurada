//! Java 엔진의 JSON 읽개(`JsonReader`)와 압축 쓰개(`JsonCompact`)를 그대로 옮긴 것.
//!
//! 쓰기 명령이 사용자가 준 JSON 인자(프로젝트 저장소 목록, 워크스페이스 템플릿 목록)를 읽고 오류 글에 그 값을 되풀이하려면
//! Java와 같은 규칙이 필요하다: 객체는 적힌 차례를 지키고(같은 키가 또 나오면 첫 자리에 값만 바꾼다), 리터럴은 구분 글자가 나올
//! 때까지의 아무 글이나 그대로 받고(`true`, `null`, 숫자, 심지어 `5x`도 리터럴이다), 문자열 안의 제어 글자도 허용한다.
//! `serde_json`은 객체를 이름 차례로 정렬하고 숫자 글을 바꾸므로 쓰지 않는다.

use crate::text::trim;

/// 읽은 JSON 값
#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Str(String),
    /// 따옴표 없는 글 그대로(`true`, `false`, `null`, 숫자 …)
    Literal(String),
    Object(Vec<(String, JsonValue)>),
    Array(Vec<JsonValue>),
}

impl JsonValue {
    pub fn is_null(&self) -> bool {
        matches!(self, JsonValue::Literal(raw) if raw == "null")
    }

    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        match self {
            JsonValue::Object(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

/// 앞뒤 공백을 떼고 읽는다. 비면 빈 객체(`{}`)로 본다(Java `JsonReader.parse`).
pub fn parse(raw: &str) -> Result<JsonValue, String> {
    let trimmed = trim(raw);
    parse_file(if trimmed.is_empty() { "{}" } else { trimmed })
}

/// 글 전체를 값 하나로 읽는다. 앞뒤의 공백(`' '`, 탭, 줄바꿈)만 건너뛰고 남는 글이 있으면 오류다.
pub fn parse_file(raw: &str) -> Result<JsonValue, String> {
    let mut reader = Reader {
        src: raw.encode_utf16().collect(),
        pos: 0,
    };
    reader.skip_spaces();
    let value = reader.read_value()?;
    reader.skip_spaces();
    if reader.pos < reader.src.len() {
        return Err(format!(
            "Unexpected trailing character at pos {}",
            reader.pos
        ));
    }
    Ok(value)
}

struct Reader {
    /// Java 글자(UTF-16 코드 단위)로 읽어서 위치 번호가 오류 글에 그대로 나온다
    src: Vec<u16>,
    pos: usize,
}

impl Reader {
    fn peek(&self) -> Option<u16> {
        self.src.get(self.pos).copied()
    }

    fn skip_spaces(&mut self) {
        while matches!(self.peek(), Some(0x20 | 0x09 | 0x0A | 0x0D)) {
            self.pos += 1;
        }
    }

    fn read_value(&mut self) -> Result<JsonValue, String> {
        match self.peek() {
            None => Err("Unexpected end of input".to_string()),
            Some(0x22) => Ok(JsonValue::Str(self.read_string()?)),
            Some(0x7B) => self.read_object(),
            Some(0x5B) => self.read_array(),
            Some(_) => self.read_literal(),
        }
    }

    fn read_string(&mut self) -> Result<String, String> {
        self.pos += 1; // 여는 따옴표
        let mut units: Vec<u16> = Vec::new();
        while self.pos < self.src.len() {
            let c = self.src[self.pos];
            self.pos += 1;
            if c == 0x22 {
                return Ok(String::from_utf16_lossy(&units));
            }
            if c != 0x5C {
                units.push(c);
                continue;
            }
            let Some(esc) = self.peek() else {
                return Err("Unterminated escape sequence in string".to_string());
            };
            self.pos += 1;
            match char::from_u32(u32::from(esc)).unwrap_or('\u{0}') {
                '"' => units.push(0x22),
                '\\' => units.push(0x5C),
                '/' => units.push(0x2F),
                'b' => units.push(0x08),
                'f' => units.push(0x0C),
                'n' => units.push(0x0A),
                'r' => units.push(0x0D),
                't' => units.push(0x09),
                'u' => {
                    if self.pos + 4 > self.src.len() {
                        return Err("Invalid unicode escape".to_string());
                    }
                    let mut code: u16 = 0;
                    for i in 0..4 {
                        let h = self.src[self.pos + i];
                        let digit = match h {
                            0x30..=0x39 => h - 0x30,
                            0x61..=0x66 => h - 0x61 + 10,
                            0x41..=0x46 => h - 0x41 + 10,
                            _ => {
                                let text =
                                    String::from_utf16_lossy(&self.src[self.pos..self.pos + 4]);
                                return Err(format!("Invalid hex in unicode escape: {text}"));
                            }
                        };
                        code = (code << 4) | digit;
                    }
                    self.pos += 4;
                    units.push(code);
                }
                other => return Err(format!("Unsupported escape: \\{other}")),
            }
        }
        Err("Unterminated string".to_string())
    }

    fn read_object(&mut self) -> Result<JsonValue, String> {
        self.pos += 1; // {
        self.skip_spaces();
        let mut entries: Vec<(String, JsonValue)> = Vec::new();
        if self.peek() == Some(0x7D) {
            self.pos += 1;
            return Ok(JsonValue::Object(entries));
        }
        while self.pos < self.src.len() {
            self.skip_spaces();
            match self.peek() {
                None => return Err("Unterminated object".to_string()),
                Some(0x2C) => return Err("Leading or unexpected comma in object".to_string()),
                Some(0x22) => {}
                Some(_) => return Err("Object keys must be quoted strings".to_string()),
            }
            let key = self.read_string()?;
            self.skip_spaces();
            if self.peek() != Some(0x3A) {
                return Err("Expected ':' after key".to_string());
            }
            self.pos += 1;
            self.skip_spaces();
            let value = self.read_value()?;
            // 같은 키가 또 나오면 처음 자리에 값만 바꾼다(LinkedHashMap.put)
            match entries.iter_mut().find(|(k, _)| *k == key) {
                Some(slot) => slot.1 = value,
                None => entries.push((key, value)),
            }
            self.skip_spaces();
            match self.peek() {
                Some(0x2C) => {
                    self.pos += 1;
                    self.skip_spaces();
                    if self.peek() == Some(0x7D) {
                        return Err("Trailing comma in object".to_string());
                    }
                }
                Some(0x7D) => {
                    self.pos += 1;
                    return Ok(JsonValue::Object(entries));
                }
                _ => return Err("Expected ',' or '}' in object".to_string()),
            }
        }
        Err("Unterminated object".to_string())
    }

    fn read_array(&mut self) -> Result<JsonValue, String> {
        self.pos += 1; // [
        self.skip_spaces();
        let mut items = Vec::new();
        if self.peek() == Some(0x5D) {
            self.pos += 1;
            return Ok(JsonValue::Array(items));
        }
        while self.pos < self.src.len() {
            self.skip_spaces();
            match self.peek() {
                None => return Err("Unterminated array".to_string()),
                Some(0x2C) => return Err("Leading or unexpected comma in array".to_string()),
                Some(_) => {}
            }
            items.push(self.read_value()?);
            self.skip_spaces();
            match self.peek() {
                Some(0x2C) => {
                    self.pos += 1;
                    self.skip_spaces();
                    if self.peek() == Some(0x5D) {
                        return Err("Trailing comma in array".to_string());
                    }
                }
                Some(0x5D) => {
                    self.pos += 1;
                    return Ok(JsonValue::Array(items));
                }
                _ => return Err("Expected ',' or ']' in array".to_string()),
            }
        }
        Err("Unterminated array".to_string())
    }

    fn read_literal(&mut self) -> Result<JsonValue, String> {
        let start = self.pos;
        while let Some(c) = self.peek() {
            let delimiter = matches!(
                c,
                0x20 | 0x09 | 0x0A | 0x0D | 0x2C | 0x3A | 0x7B | 0x7D | 0x5B | 0x5D | 0x22 | 0x5C
            );
            if delimiter || c <= 0x1F {
                break;
            }
            self.pos += 1;
        }
        if self.pos == start {
            return Err(format!(
                "Unexpected character for literal at pos {}",
                self.pos
            ));
        }
        Ok(JsonValue::Literal(String::from_utf16_lossy(
            &self.src[start..self.pos],
        )))
    }
}

/// 공백 없는 한 줄 JSON으로 되돌린다(Java `JsonCompact.compact`).
pub fn compact(value: &JsonValue) -> String {
    let mut out = String::new();
    append(&mut out, value);
    out
}

fn append(out: &mut String, value: &JsonValue) {
    match value {
        JsonValue::Str(s) => append_string(out, s),
        JsonValue::Literal(raw) => out.push_str(raw),
        JsonValue::Object(entries) => {
            out.push('{');
            for (i, (key, v)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                append_string(out, key);
                out.push(':');
                append(out, v);
            }
            out.push('}');
        }
        JsonValue::Array(items) => {
            out.push('[');
            for (i, v) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                append(out, v);
            }
            out.push(']');
        }
    }
}

/// 따옴표로 감싼 JSON 문자열: `"`, `\`, 제어 문자만 이스케이프하고 나머지는 그대로 쓴다.
pub fn append_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\u{c}' => out.push_str("\\f"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) <= 0x1f => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_order_raw_literals_and_first_position_of_duplicate_keys() {
        let value = parse_file(r#" {"b":1,"a":[1.0e3, x5 ,null],"b":"새", "c":{}} "#).unwrap();
        assert_eq!(compact(&value), r#"{"b":"새","a":[1.0e3,x5,null],"c":{}}"#);
        assert!(value.get("a").unwrap() != &JsonValue::Array(vec![]));
        assert!(
            parse_file("[null]").unwrap()
                == JsonValue::Array(vec![JsonValue::Literal("null".into())])
        );
        assert!(
            JsonValue::Literal("null".into()).is_null() && !JsonValue::Str("null".into()).is_null()
        );
    }

    #[test]
    fn reads_escapes_and_reports_java_messages() {
        let value = parse_file(r#""a\n\u0041\ud83d\ude00\/""#).unwrap();
        assert_eq!(value, JsonValue::Str("a\nA😀/".into()));
        let err = |text: &str| parse_file(text).unwrap_err();
        assert_eq!(err(""), "Unexpected end of input");
        assert_eq!(err("[1,]"), "Trailing comma in array");
        assert_eq!(err("[,1]"), "Leading or unexpected comma in array");
        assert_eq!(err("{\"a\":1,}"), "Trailing comma in object");
        assert_eq!(err("{a:1}"), "Object keys must be quoted strings");
        assert_eq!(err("{\"a\" 1}"), "Expected ':' after key");
        assert_eq!(err("{\"a\":1 2}"), "Expected ',' or '}' in object");
        assert_eq!(err("[1 2]"), "Expected ',' or ']' in array");
        assert_eq!(err("[1"), "Expected ',' or ']' in array");
        assert_eq!(err("\"abc"), "Unterminated string");
        assert_eq!(err("\"\\q\""), "Unsupported escape: \\q");
        assert_eq!(err("\"\\u12G4\""), "Invalid hex in unicode escape: 12G4");
        assert_eq!(err("1 2"), "Unexpected trailing character at pos 2");
        assert_eq!(err("]"), "Unexpected character for literal at pos 0");
        assert_eq!(parse("  ").unwrap(), JsonValue::Object(vec![]));
    }
}
