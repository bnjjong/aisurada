//! 노트 머리 쓰기. Java 엔진 `FrontmatterWriter`가 만드는 글과 같은 모양을 냅니다.
//!
//! 라이브러리를 거치지 않는 직접 작성기입니다. 키 차례는 읽은 차례를 지키고, 따옴표·블록 문자열 규칙은
//! Java 쪽과 같습니다. 이미 저장된 노트를 다시 쓸 때 건드리지 않은 부분의 글자가 바뀌지 않게 하려는 것입니다.

use std::fmt;
use std::sync::LazyLock;

use regex::Regex;

use super::value::Value;

/// 머리에 쓸 수 없는 값(실수, 매우 큰 정수, 명시 태그로 읽은 값, 문자열이 아닌 안쪽 키)을 만났을 때의 오류입니다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DumpError(pub String);

impl fmt::Display for DumpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "frontmatter 에 쓸 수 없는 값이다: {}", self.0)
    }
}

impl std::error::Error for DumpError {}

const BOOLEAN_KEYWORDS: [&str; 16] = [
    "y", "Y", "yes", "Yes", "YES", "on", "On", "ON", "n", "N", "no", "No", "NO", "off", "Off",
    "OFF",
];
const NULL_LITERALS: [&str; 4] = ["~", "null", "Null", "NULL"];
const BOOLEAN_LITERALS: [&str; 6] = ["true", "True", "TRUE", "false", "False", "FALSE"];
const PLAIN_FORBIDDEN_START: &str = " -?:,[]{}#&*!|=>'\"%@`";

static BASE60: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[-+]?[0-9_]+(:[0-9_]+)+(\.[0-9_]*)?$").unwrap());
static INTEGER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[+-]?(?:0|0b[01_]+|0x[0-9a-fA-F_]+|0o[0-7_]+|[0-9][0-9_]*)$").unwrap()
});
static FLOAT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?:[-+]?(?:[0-9][0-9_]*)(?:\.[0-9_]*)?(?:[eE][-+]?[0-9]+)?|[-+]?\.[0-9_]+(?:[eE][-+]?[0-9]+)?|[-+]?\.(?:inf|Inf|INF)|\.(?:nan|NaN|NAN))$",
    )
    .unwrap()
});
static DATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}$").unwrap());
static TIMESTAMP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^[0-9]{4}-[0-9][0-9]?-[0-9][0-9]?(?:[Tt]|[ \t]+)[0-9][0-9]?:[0-9]{2}:[0-9]{2}(?:\.[0-9]*)?(?:[ \t]*(?:Z|[-+][0-9][0-9]?(?::[0-9]{2})?))?$",
    )
    .unwrap()
});

/// 읽은 맵을 머리 YAML 글로 다시 씁니다(읽은 차례 그대로).
///
/// 최상위 키가 문자열이 아닌 쌍은 건너뜁니다. 쓸 수 없는 값이 하나라도 있으면 오류입니다.
pub fn dump_general_map(map: &[(Value, Value)]) -> Result<String, DumpError> {
    if map.is_empty() {
        return Ok("{}\n".to_string());
    }
    let mut out = String::new();
    for (key, value) in map {
        let Value::Str(key) = key else { continue };
        check_valid(value)?;
        out.push_str(&dump_key(key));
        out.push(':');
        append_value(&mut out, value, 1)?;
    }
    Ok(out)
}

fn check_valid(value: &Value) -> Result<(), DumpError> {
    match value {
        Value::Null | Value::Bool(_) | Value::Str(_) | Value::List(_) | Value::Map(_) => Ok(()),
        // Java의 Integer와 Long은 쓰고, 그보다 큰 BigInteger는 못 쓴다
        Value::Int(text) if text.parse::<i64>().is_ok() => Ok(()),
        Value::Int(text) => Err(DumpError(format!("BigInteger ({text})"))),
        Value::Float(x) => Err(DumpError(format!("Double ({x})"))),
        Value::Other => Err(DumpError("지원하지 않는 형".to_string())),
    }
}

fn string_key(key: &Value) -> Result<&str, DumpError> {
    match key {
        Value::Str(text) => Ok(text),
        _ => Err(DumpError(
            "frontmatter 의 키는 문자열이어야 한다".to_string(),
        )),
    }
}

/// 키의 `:` 뒤에 값을 잇습니다. 비지 않은 맵과 목록은 다음 줄부터 적고, 나머지는 공백 1개 뒤에 적습니다.
fn append_value(out: &mut String, value: &Value, depth: usize) -> Result<(), DumpError> {
    check_valid(value)?;
    let block = match value {
        Value::Map(map) => !map.is_empty(),
        Value::List(list) => !list.is_empty(),
        _ => false,
    };
    if !block {
        out.push(' ');
    }
    match value {
        Value::Map(map) if map.is_empty() => out.push_str("{}\n"),
        Value::Map(map) => {
            out.push('\n');
            let indent = "  ".repeat(depth);
            for (key, value) in map {
                out.push_str(&indent);
                out.push_str(&dump_key(string_key(key)?));
                out.push(':');
                append_value(out, value, depth + 1)?;
            }
        }
        Value::List(list) if list.is_empty() => out.push_str("[]\n"),
        Value::List(list) => {
            out.push('\n');
            let indent = "  ".repeat(depth);
            for item in list {
                check_valid(item)?;
                match item {
                    Value::Map(map) if map.is_empty() => {
                        out.push_str(&indent);
                        out.push_str("- {}\n");
                    }
                    Value::Map(map) => {
                        for (i, (key, value)) in map.iter().enumerate() {
                            out.push_str(&indent);
                            out.push_str(if i == 0 { "- " } else { "  " });
                            out.push_str(&dump_key(string_key(key)?));
                            out.push(':');
                            append_value(out, value, depth + 2)?;
                        }
                    }
                    Value::List(inner) => {
                        out.push_str(&indent);
                        out.push_str("- ");
                        append_nested_list(out, inner, depth + 1)?;
                    }
                    scalar => {
                        out.push_str(&indent);
                        out.push_str("- ");
                        append_scalar_item(out, scalar, depth + 1);
                    }
                }
            }
        }
        scalar => append_scalar_item(out, scalar, depth),
    }
    Ok(())
}

/// 목록 안의 목록입니다. 첫 항목은 이미 쓴 `- ` 뒤에 이어 적습니다.
fn append_nested_list(out: &mut String, list: &[Value], depth: usize) -> Result<(), DumpError> {
    let Some((first, rest)) = list.split_first() else {
        out.push_str("[]\n");
        return Ok(());
    };
    out.push_str("- ");
    check_valid(first)?;
    append_nested_item(out, first, depth)?;

    let indent = "  ".repeat(depth);
    for item in rest {
        check_valid(item)?;
        out.push_str(&indent);
        out.push_str("- ");
        append_nested_item(out, item, depth)?;
    }
    Ok(())
}

fn append_nested_item(out: &mut String, item: &Value, depth: usize) -> Result<(), DumpError> {
    match item {
        Value::List(inner) => append_nested_list(out, inner, depth + 1),
        Value::Map(map) => {
            out.push('\n');
            let indent = "  ".repeat(depth + 1);
            for (key, value) in map {
                out.push_str(&indent);
                out.push_str(&dump_key(string_key(key)?));
                out.push(':');
                append_value(out, value, depth + 2)?;
            }
            Ok(())
        }
        scalar => {
            append_scalar_item(out, scalar, depth + 1);
            Ok(())
        }
    }
}

fn append_scalar_item(out: &mut String, value: &Value, depth: usize) {
    match value {
        Value::Null => out.push_str("null\n"),
        Value::Bool(true) => out.push_str("true\n"),
        Value::Bool(false) => out.push_str("false\n"),
        Value::Int(text) => {
            out.push_str(text);
            out.push('\n');
        }
        Value::Str(text) => {
            out.push_str(&dump_string(text, depth));
            out.push('\n');
        }
        // check_valid가 거른 값이라 여기 오지 않는다
        Value::Float(_) | Value::Other | Value::List(_) | Value::Map(_) => {}
    }
}

/// 맵 키 문자열을 씁니다.
pub(crate) fn dump_key(key: &str) -> String {
    if key.is_empty() {
        return "''".to_string();
    }
    if BOOLEAN_KEYWORDS.contains(&key) || BASE60.is_match(key) {
        return format!("'{key}'");
    }
    if has_non_printable(key, true) {
        return dump_double_quoted(key);
    }
    if is_plain_safe(key) && !is_other_type(key) {
        return key.to_string();
    }
    format!("'{}'", key.replace('\'', "''"))
}

/// 문자열 값을 YAML 스칼라로 씁니다. `depth`는 들여쓰기 깊이이고 줄 폭은 `max(40, 80 - 2 * depth)`입니다.
pub(crate) fn dump_string(s: &str, depth: usize) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    if BOOLEAN_KEYWORDS.contains(&s) || BASE60.is_match(s) {
        return format!("'{s}'");
    }
    if has_non_printable(s, false) {
        return dump_double_quoted(s);
    }

    let width = (80 - 2 * depth as i64).max(40) as usize;
    let lines: Vec<&str> = s.split('\n').collect();
    let has_foldable_line = lines
        .iter()
        .any(|line| utf16_len(line) > width && !line.starts_with(' '));

    if has_foldable_line {
        dump_folded_block(s, &lines, depth, width)
    } else if s.contains('\n') {
        dump_literal_block(s, &lines, depth)
    } else if is_plain_safe(s) && !is_other_type(s) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "''"))
    }
}

/// 줄 폭은 Java와 같게 UTF-16 코드 단위로 센다. 기존 노트를 다시 저장해도 줄 바꿈 자리가 달라지지 않게 하려는 것이다.
fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// 인쇄 가능 범위 밖 글자가 있는지 봅니다. 값에서는 줄바꿈이 블록 문자열로 가므로 제외하고, 키에서는 줄바꿈·탭·CR도 센다.
fn has_non_printable(s: &str, is_key: bool) -> bool {
    s.chars().any(|c| {
        if is_key && matches!(c, '\n' | '\t' | '\r') {
            return true;
        }
        if c == '\n' {
            return false;
        }
        let cp = c as u32;
        if matches!(cp, 0x2028 | 0x2029 | 0xFEFF) {
            return true;
        }
        // BMP 밖 글자(이모지 등)는 인쇄 가능으로 본다
        cp <= 0xFFFF && !is_printable_bmp(cp)
    })
}

fn is_printable_bmp(cp: u32) -> bool {
    (0x20..=0x7E).contains(&cp) || (0xA1..=0xD7FF).contains(&cp) || (0xE000..=0xFFFD).contains(&cp)
}

fn dump_double_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    out.push('"');
    for c in s.chars() {
        let cp = c as u32;
        if cp > 0xFFFF {
            out.push_str(&format!("\\U{cp:08X}"));
            continue;
        }
        match cp {
            0x00 => out.push_str("\\0"),
            0x07 => out.push_str("\\a"),
            0x08 => out.push_str("\\b"),
            0x09 => out.push_str("\\t"),
            0x0A => out.push_str("\\n"),
            0x0B => out.push_str("\\v"),
            0x0C => out.push_str("\\f"),
            0x0D => out.push_str("\\r"),
            0x1B => out.push_str("\\e"),
            0x22 => out.push_str("\\\""),
            0x5C => out.push_str("\\\\"),
            0x85 => out.push_str("\\N"),
            0xA0 => out.push_str("\\_"),
            0x2028 => out.push_str("\\L"),
            0x2029 => out.push_str("\\P"),
            _ if is_printable_bmp(cp) && cp != 0xFEFF => out.push(c),
            _ if cp <= 0xFF => out.push_str(&format!("\\x{cp:02X}")),
            _ => out.push_str(&format!("\\u{cp:04X}")),
        }
    }
    out.push('"');
    out
}

fn dump_literal_block(s: &str, lines: &[&str], depth: usize) -> String {
    let mut out = block_header('|', s);
    out.push('\n');
    let indent = " ".repeat(2 * depth);

    let mut count = lines.len();
    if s.ends_with('\n') && count > 0 && lines[count - 1].is_empty() {
        count -= 1;
    }
    for (i, line) in lines.iter().take(count).enumerate() {
        if !line.is_empty() {
            out.push_str(&indent);
            out.push_str(line);
        }
        if i + 1 < count {
            out.push('\n');
        }
    }
    out
}

fn dump_folded_block(s: &str, lines: &[&str], depth: usize, width: usize) -> String {
    let chomping = chomping_indicator(s);
    let mut out = block_header('>', s);
    out.push('\n');
    let indent = " ".repeat(2 * depth);

    let Some(last) = lines.iter().rposition(|line| !line.is_empty()) else {
        out.push_str(&"\n".repeat(lines.len() - 1));
        return out;
    };

    let mut i = 0;
    while i < lines.len() && lines[i].is_empty() {
        out.push('\n');
        i += 1;
    }

    while i <= last {
        let line = lines[i];
        if line.starts_with(' ') {
            out.push_str(&indent);
            out.push_str(line);
        } else {
            out.push_str(&fold_line(line, width, &indent));
        }

        if i < last {
            let mut next = i + 1;
            while next <= last && lines[next].is_empty() {
                next += 1;
            }
            // 원본 개행 수. 둘 다 일반 줄이면 읽을 때 개행이 접히므로 하나 더 적는다
            let breaks = next - i;
            let both_plain = !line.starts_with(' ') && !lines[next].starts_with(' ');
            out.push_str(&"\n".repeat(if both_plain { breaks + 1 } else { breaks }));
            i = next;
        } else {
            if chomping == '+' {
                let trailing_blanks = lines.len() - 1 - last;
                out.push_str(&"\n".repeat(trailing_blanks.saturating_sub(1)));
            }
            i += 1;
        }
    }
    out
}

/// 폭을 넘는 줄을 공백 자리에서 접습니다. 접는 자리와 폭은 UTF-16 단위로 센다(Java와 같다).
fn fold_line(line: &str, width: usize, indent: &str) -> String {
    let units: Vec<u16> = line.encode_utf16().collect();
    if units.len() <= width {
        return format!("{indent}{line}");
    }
    let space = u16::from(b' ');
    let len = units.len();
    let piece = |from: usize, to: usize| String::from_utf16_lossy(&units[from..to]);

    let mut out = String::new();
    let mut start = 0;
    let mut first = true;
    while start < len {
        if len - start <= width {
            if !first {
                out.push('\n');
            }
            out.push_str(indent);
            out.push_str(&piece(start, len));
            break;
        }

        // 폭 안에서 가장 뒤의 접을 자리, 없으면 폭 뒤에서 가장 앞의 접을 자리
        let can_break = |i: usize| units[i] == space && i + 1 < len && units[i + 1] != space;
        let break_pos = (start..=(start + width).min(len - 1))
            .rev()
            .find(|&i| can_break(i))
            .or_else(|| ((start + width + 1)..len).find(|&i| can_break(i)));

        match break_pos {
            Some(pos) if pos > start => {
                if !first {
                    out.push('\n');
                }
                out.push_str(indent);
                out.push_str(&piece(start, pos));
                first = false;
                start = pos + 1;
            }
            _ => {
                if !first {
                    out.push('\n');
                }
                out.push_str(indent);
                out.push_str(&piece(start, len));
                break;
            }
        }
    }
    out
}

/// 블록 문자열의 첫 줄(`|`나 `>` 뒤에 들여쓰기 표시와 끝 개행 처리 표시)을 만듭니다.
fn block_header(style: char, s: &str) -> String {
    let mut header = style.to_string();
    if starts_with_space_after_newlines(s) {
        header.push('2');
    }
    let chomping = chomping_indicator(s);
    if chomping != ' ' {
        header.push(chomping);
    }
    header
}

fn chomping_indicator(s: &str) -> char {
    if !s.ends_with('\n') {
        '-'
    } else if s == "\n" || s.ends_with("\n\n") {
        '+'
    } else {
        ' '
    }
}

fn starts_with_space_after_newlines(s: &str) -> bool {
    s.trim_start_matches('\n').starts_with(' ')
}

fn is_plain_safe(s: &str) -> bool {
    let Some(first) = s.chars().next() else {
        return false;
    };
    if PLAIN_FORBIDDEN_START.contains(first) {
        return false;
    }
    !(s.ends_with(' ') || s.ends_with(':') || s.contains(": ") || s.contains(" #"))
}

/// 따옴표 없이 쓰면 문자열이 아닌 다른 형으로 읽힐 글인지 봅니다.
fn is_other_type(s: &str) -> bool {
    NULL_LITERALS.contains(&s)
        || BOOLEAN_LITERALS.contains(&s)
        || s == "<<"
        || INTEGER.is_match(s)
        || FLOAT.is_match(s)
        || DATE.is_match(s)
        || TIMESTAMP.is_match(s)
}
