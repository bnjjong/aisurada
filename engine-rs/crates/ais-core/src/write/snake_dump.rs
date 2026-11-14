//! SnakeYAML 2.4 `Yaml.dump`와 같은 글을 내는 YAML 방출기. 태스크와 자원 파일이 쓰는 모양이다.
//!
//! Java 엔진은 이 파일들을 `DumperOptions`(블록 형식, 유니코드 허용, 줄 나누기 끔, 들여쓰기 2, 목록 표시 들여쓰기 2,
//! 표시와 함께 들여쓰기)로 SnakeYAML이 쓴다. 노트 머리를 쓰는 `dump_string`·`dump_general_map`은 설명서 1.9 규칙의 모방이라
//! 따옴표 규칙이 다르므로(예: `:a`, 키 `n`) 이 파일 종류에는 쓸 수 없다. 여기서는 SnakeYAML의 `Emitter`(스칼라 분석, 스타일 선택,
//! 일반·작은따옴표·큰따옴표·리터럴 쓰기)와 `Resolver`(글이 다른 형으로 읽히면 따옴표를 붙이는 규칙)를 같은 차례로 옮겼다.
//! 값은 맵, 목록, 글, 정수, 실수, 참거짓, 없음만 다룬다. 앵커·별칭·주석·흐름 형식(빈 목록·맵 `[]`·`{}` 말고)은 쓰지 않는다.

use std::sync::LazyLock;

use regex::Regex;

use crate::frontmatter::Value;
use crate::text::utf16_len;

const BEST_INDENT: usize = 2;
const INDICATOR_INDENT: usize = 2;
const MAX_SIMPLE_KEY_LENGTH: usize = 128;

static BOOL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:yes|Yes|YES|no|No|NO|true|True|TRUE|false|False|FALSE|on|On|ON|off|Off|OFF)$")
        .unwrap()
});
static INT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?:[-+]?0b_*[0-1][0-1_]*|[-+]?0_*[0-7][0-7_]*|[-+]?(?:0|[1-9][0-9_]*)|[-+]?0x_*[0-9a-fA-F][0-9a-fA-F_]*|[-+]?[1-9][0-9_]*(?::[0-5]?[0-9])+)$",
    )
    .unwrap()
});
static FLOAT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?:[-+]?(?:[0-9][0-9_]*)\.[0-9_]*(?:[eE][-+]?[0-9]+)?|[-+]?(?:[0-9][0-9_]*)(?:[eE][-+]?[0-9]+)|[-+]?\.[0-9_]+(?:[eE][-+]?[0-9]+)?|[-+]?[0-9][0-9_]*(?::[0-5]?[0-9])+\.[0-9_]*|[-+]?\.(?:inf|Inf|INF)|\.(?:nan|NaN|NAN))$",
    )
    .unwrap()
});
static NULL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(?:~|null|Null|NULL| )$").unwrap());
static TIMESTAMP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?:[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]|[0-9][0-9][0-9][0-9]-[0-9][0-9]?-[0-9][0-9]?(?:[Tt]|[ \t]+)[0-9][0-9]?:[0-9][0-9]:[0-9][0-9](?:\.[0-9]*)?(?:[ \t]*(?:Z|[-+][0-9][0-9]?(?::[0-9][0-9])?))?)$",
    )
    .unwrap()
});

/// 값의 형(태그). 글이 `Str`이 아닌 형으로 읽히면 일반(따옴표 없는) 글로 못 쓴다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tag {
    Str,
    Int,
    Float,
    Bool,
    Null,
    Binary,
    Merge,
    Timestamp,
    Yaml,
}

impl Tag {
    /// 방출기가 키 길이에 더하는 준비된 태그(`!!str` 따위)의 길이
    fn prepared_len(self) -> usize {
        match self {
            Tag::Str | Tag::Int => 5,
            Tag::Float => 7,
            Tag::Bool | Tag::Null => 6,
            Tag::Binary => 8,
            Tag::Merge => 7,
            Tag::Timestamp => 11,
            Tag::Yaml => 6,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Tag::Str => "!!str",
            Tag::Int => "!!int",
            Tag::Float => "!!float",
            Tag::Bool => "!!bool",
            Tag::Null => "!!null",
            Tag::Binary => "!!binary",
            Tag::Merge => "!!merge",
            Tag::Timestamp => "!!timestamp",
            Tag::Yaml => "!!yaml",
        }
    }
}

/// 글을 읽을 때 일반 글이 어느 형으로 읽히는지(SnakeYAML `Resolver.resolve`)
fn resolve(value: &str) -> Tag {
    let len = utf16_len(value);
    if let Some(first) = value.chars().next() {
        if "yYnNtTfFoO".contains(first) && len <= 10 && BOOL.is_match(value) {
            return Tag::Bool;
        }
        if "-+0123456789".contains(first) && len <= 1024 && INT.is_match(value) {
            return Tag::Int;
        }
        if "-+0123456789.".contains(first) && len <= 1024 && FLOAT.is_match(value) {
            return Tag::Float;
        }
        if first == '<' && len <= 10 && value == "<<" {
            return Tag::Merge;
        }
        if "~nN".contains(first) && len <= 10 && NULL.is_match(value) {
            return Tag::Null;
        }
        if first.is_ascii_digit() && len <= 50 && TIMESTAMP.is_match(value) {
            return Tag::Timestamp;
        }
        if "!&*".contains(first) && len <= 10 && matches!(value, "!" | "&" | "*") {
            return Tag::Yaml;
        }
    }
    // 첫 글자로 가르지 않는 목록: 널 글(`~`, `null`, 한 칸 공백)과 빈 글
    if len <= 10 && (NULL.is_match(value) || value.is_empty()) {
        return Tag::Null;
    }
    Tag::Str
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Style {
    Plain,
    Single,
    Double,
    Literal,
}

struct Analysis {
    text: Vec<char>,
    empty: bool,
    multiline: bool,
    allow_flow_plain: bool,
    allow_block_plain: bool,
    allow_single_quoted: bool,
    allow_block: bool,
}

fn is_linebr(c: char) -> bool {
    matches!(c, '\n' | '\u{85}' | '\u{2028}' | '\u{2029}')
}

fn is_null_bl_t(c: char) -> bool {
    matches!(c, '\0' | ' ' | '\t')
}

fn is_null_bl_t_linebr(c: char) -> bool {
    is_null_bl_t(c) || c == '\r' || is_linebr(c)
}

fn has_leading_zero(text: &[char]) -> bool {
    text.len() > 1 && text[0] == '0' && text[1..].iter().all(|c| c.is_ascii_digit() || *c == '_')
}

/// SnakeYAML `Emitter.analyzeScalar`(유니코드 허용)
fn analyze(scalar: &str) -> Analysis {
    let text: Vec<char> = scalar.chars().collect();
    if text.is_empty() {
        return Analysis {
            text,
            empty: true,
            multiline: false,
            allow_flow_plain: false,
            allow_block_plain: true,
            allow_single_quoted: true,
            allow_block: false,
        };
    }
    let total_units: usize = text.iter().map(|c| c.len_utf16()).sum();
    let mut block_indicators = false;
    let mut flow_indicators = false;
    let mut line_breaks = false;
    let mut special_characters = false;
    let leading_zero_number = has_leading_zero(&text);
    let (mut leading_space, mut leading_break, mut trailing_space, mut trailing_break) =
        (false, false, false, false);
    let (mut break_space, mut space_break) = (false, false);

    if scalar.starts_with("---") || scalar.starts_with("...") {
        block_indicators = true;
        flow_indicators = true;
    }
    let mut preceded_by_whitespace = true;
    // 첫 글자가 보충 평면 글자면 UTF-16 둘째 칸은 낮은 서로게이트라 공백이 아니다
    let mut followed_by_whitespace = if text[0].len_utf16() == 2 {
        false
    } else {
        text.len() == 1 || is_null_bl_t_linebr(text[1])
    };
    let mut previous_space = false;
    let mut previous_break = false;
    let mut unit = 0usize;
    for (i, &c) in text.iter().enumerate() {
        if i == 0 {
            if "#,[]{}&*!|>'\"%@`".contains(c) {
                flow_indicators = true;
                block_indicators = true;
            }
            if c == '?' || c == ':' {
                flow_indicators = true;
                if followed_by_whitespace {
                    block_indicators = true;
                }
            }
            if c == '-' && followed_by_whitespace {
                flow_indicators = true;
                block_indicators = true;
            }
        } else {
            if ",?[]{}".contains(c) {
                flow_indicators = true;
            }
            if c == ':' {
                flow_indicators = true;
                if followed_by_whitespace {
                    block_indicators = true;
                }
            }
            if c == '#' && preceded_by_whitespace {
                flow_indicators = true;
                block_indicators = true;
            }
        }
        let is_break = is_linebr(c);
        if is_break {
            line_breaks = true;
        }
        let code = c as u32;
        if !(c == '\n' || (0x20..=0x7E).contains(&code))
            && !(code == 0x85
                || (0xA0..=0xD7FF).contains(&code)
                || (0xE000..=0xFFFD).contains(&code)
                || (0x10000..=0x10FFFF).contains(&code))
        {
            special_characters = true;
        }
        if c == ' ' {
            if i == 0 {
                leading_space = true;
            }
            if i == text.len() - 1 {
                trailing_space = true;
            }
            if previous_break {
                break_space = true;
            }
            previous_space = true;
            previous_break = false;
        } else if is_break {
            if i == 0 {
                leading_break = true;
            }
            if i == text.len() - 1 {
                trailing_break = true;
            }
            if previous_space {
                space_break = true;
            }
            previous_space = false;
            previous_break = true;
        } else {
            previous_space = false;
            previous_break = false;
        }
        unit += c.len_utf16();
        preceded_by_whitespace = is_null_bl_t(c) || is_break;
        followed_by_whitespace = true;
        if unit + 1 < total_units {
            let next_unit = unit + text[i + 1].len_utf16();
            if next_unit < total_units {
                followed_by_whitespace = is_null_bl_t(text[i + 2]) || is_break;
            }
        }
    }
    let mut allow_flow_plain = true;
    let mut allow_block_plain = true;
    let mut allow_single_quoted = true;
    let mut allow_block = true;
    if leading_space || leading_break || trailing_space || trailing_break || leading_zero_number {
        allow_flow_plain = false;
        allow_block_plain = false;
    }
    if trailing_space {
        allow_block = false;
    }
    if break_space {
        allow_flow_plain = false;
        allow_block_plain = false;
        allow_single_quoted = false;
    }
    if space_break || special_characters {
        allow_flow_plain = false;
        allow_block_plain = false;
        allow_single_quoted = false;
        allow_block = false;
    }
    if line_breaks {
        allow_flow_plain = false;
    }
    if flow_indicators {
        allow_flow_plain = false;
    }
    if block_indicators {
        allow_block_plain = false;
    }
    Analysis {
        text,
        empty: false,
        multiline: line_breaks,
        allow_flow_plain,
        allow_block_plain,
        allow_single_quoted,
        allow_block,
    }
}

fn is_printable(c: char) -> bool {
    let code = c as u32;
    (0x20..=0x7E).contains(&code)
        || code == 0x9
        || code == 0xA
        || code == 0xD
        || code == 0x85
        || (0xA0..=0xD7FF).contains(&code)
        || (0xE000..=0xFFFD).contains(&code)
        || (0x10000..=0x10FFFF).contains(&code)
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// 스칼라 값을 (글, 형, 대표가 요청하는 스타일)로 바꾼다(SnakeYAML `SafeRepresenter`).
fn represent(value: &Value) -> Result<(String, Tag, Style), String> {
    Ok(match value {
        Value::Null => ("null".to_string(), Tag::Null, Style::Plain),
        Value::Bool(b) => (
            (if *b { "true" } else { "false" }).to_string(),
            Tag::Bool,
            Style::Plain,
        ),
        Value::Int(text) => (text.clone(), Tag::Int, Style::Plain),
        Value::Float(f) => {
            let text = if f.is_nan() {
                ".NaN".to_string()
            } else if f.is_infinite() {
                (if *f > 0.0 { ".inf" } else { "-.inf" }).to_string()
            } else {
                crate::commands::note_meta_write::java_double_string(*f)
            };
            (text, Tag::Float, Style::Plain)
        }
        Value::Str(text) => {
            if !text.chars().all(is_printable) {
                (base64(text.as_bytes()), Tag::Binary, Style::Literal)
            } else if text.chars().any(is_linebr) {
                (text.clone(), Tag::Str, Style::Literal)
            } else {
                (text.clone(), Tag::Str, Style::Plain)
            }
        }
        Value::List(_) | Value::Map(_) | Value::Other => {
            return Err("쓸 수 없는 값이다".to_string());
        }
    })
}

struct Emitter {
    out: String,
    column: usize,
    whitespace: bool,
    indention: bool,
    indent: Option<usize>,
    indents: Vec<Option<usize>>,
    flow_level: usize,
    mapping_context: bool,
    simple_key_context: bool,
}

impl Emitter {
    fn new() -> Self {
        Emitter {
            out: String::new(),
            column: 0,
            whitespace: true,
            indention: true,
            indent: None,
            indents: Vec::new(),
            flow_level: 0,
            mapping_context: false,
            simple_key_context: false,
        }
    }

    fn increase_indent(&mut self, flow: bool, indentless: bool) {
        self.indents.push(self.indent);
        match self.indent {
            None => self.indent = Some(if flow { BEST_INDENT } else { 0 }),
            Some(indent) if !indentless => self.indent = Some(indent + BEST_INDENT),
            Some(_) => {}
        }
    }

    fn pop_indent(&mut self) {
        self.indent = self.indents.pop().flatten();
    }

    fn write_indicator(
        &mut self,
        indicator: &str,
        need_whitespace: bool,
        whitespace: bool,
        indentation: bool,
    ) {
        if !self.whitespace && need_whitespace {
            self.column += 1;
            self.out.push(' ');
        }
        self.whitespace = whitespace;
        self.indention = self.indention && indentation;
        self.column += utf16_len(indicator);
        self.out.push_str(indicator);
    }

    fn write_indent(&mut self) {
        let indent = self.indent.unwrap_or(0);
        if !self.indention || self.column > indent || (self.column == indent && !self.whitespace) {
            self.write_line_break(None);
        }
        self.write_whitespace(indent.saturating_sub(self.column));
    }

    fn write_whitespace(&mut self, length: usize) {
        if length == 0 {
            return;
        }
        self.whitespace = true;
        self.column += length;
        self.out.push_str(&" ".repeat(length));
    }

    fn write_line_break(&mut self, data: Option<char>) {
        self.whitespace = true;
        self.indention = true;
        self.column = 0;
        self.out.push(data.unwrap_or('\n'));
    }

    fn write_text(&mut self, text: &[char]) {
        self.column += text.iter().map(|c| c.len_utf16()).sum::<usize>();
        self.out.extend(text.iter());
    }

    fn choose_style(&self, requested: Style, can_omit_plain: bool, a: &Analysis) -> Style {
        if requested == Style::Plain
            && can_omit_plain
            && !(self.simple_key_context && (a.empty || a.multiline))
            && ((self.flow_level != 0 && a.allow_flow_plain)
                || (self.flow_level == 0 && a.allow_block_plain))
        {
            return Style::Plain;
        }
        if requested == Style::Literal
            && self.flow_level == 0
            && !self.simple_key_context
            && a.allow_block
        {
            return Style::Literal;
        }
        if matches!(requested, Style::Plain | Style::Single)
            && a.allow_single_quoted
            && !(self.simple_key_context && a.multiline)
        {
            return Style::Single;
        }
        Style::Double
    }

    fn emit_scalar(&mut self, value: &Value) -> Result<(), String> {
        let (text, tag, requested) = represent(value)?;
        let analysis = analyze(&text);
        let can_omit_plain = tag == resolve(&text);
        let style = self.choose_style(requested, can_omit_plain, &analysis);
        // 태그: 일반 글이고 읽으면 같은 형이거나, 일반 글이 아니고 글(`Str`)이면 적지 않는다
        let omit =
            (style == Style::Plain && can_omit_plain) || (style != Style::Plain && tag == Tag::Str);
        if !omit {
            self.write_indicator(tag.name(), true, false, false);
        }
        self.increase_indent(true, false);
        match style {
            Style::Plain => self.write_plain(&analysis.text),
            Style::Double => self.write_double_quoted(&analysis.text),
            Style::Single => self.write_single_quoted(&analysis.text),
            Style::Literal => self.write_literal(&analysis.text),
        }
        self.pop_indent();
        Ok(())
    }

    fn write_plain(&mut self, text: &[char]) {
        if text.is_empty() {
            return;
        }
        if !self.whitespace {
            self.column += 1;
            self.out.push(' ');
        }
        self.whitespace = false;
        self.indention = false;
        let (mut spaces, mut breaks) = (false, false);
        let (mut start, mut end) = (0usize, 0usize);
        while end <= text.len() {
            let ch = text.get(end).copied();
            if spaces {
                if ch != Some(' ') {
                    self.write_text(&text[start..end]);
                    start = end;
                }
            } else if breaks {
                if !ch.is_some_and(is_linebr) {
                    if text[start] == '\n' {
                        self.write_line_break(None);
                    }
                    for &br in &text[start..end] {
                        self.write_line_break(if br == '\n' { None } else { Some(br) });
                    }
                    self.write_indent();
                    self.whitespace = false;
                    self.indention = false;
                    start = end;
                }
            } else if ch.is_none_or(|c| is_linebr(c) || c == ' ') {
                self.write_text(&text[start..end]);
                start = end;
            }
            if let Some(c) = ch {
                spaces = c == ' ';
                breaks = is_linebr(c);
            }
            end += 1;
        }
    }

    fn write_single_quoted(&mut self, text: &[char]) {
        self.write_indicator("'", true, false, false);
        let (mut spaces, mut breaks) = (false, false);
        let (mut start, mut end) = (0usize, 0usize);
        while end <= text.len() {
            let ch = text.get(end).copied();
            if spaces {
                if ch != Some(' ') {
                    self.write_text(&text[start..end]);
                    start = end;
                }
            } else if breaks {
                if !ch.is_some_and(is_linebr) {
                    if text[start] == '\n' {
                        self.write_line_break(None);
                    }
                    for &br in &text[start..end] {
                        self.write_line_break(if br == '\n' { None } else { Some(br) });
                    }
                    self.write_indent();
                    start = end;
                }
            } else if ch.is_none_or(|c| is_linebr(c) || c == ' ' || c == '\'') && start < end {
                self.write_text(&text[start..end]);
                start = end;
            }
            if ch == Some('\'') {
                self.column += 2;
                self.out.push_str("''");
                start = end + 1;
            }
            if let Some(c) = ch {
                spaces = c == ' ';
                breaks = is_linebr(c);
            }
            end += 1;
        }
        self.write_indicator("'", false, false, false);
    }

    fn write_double_quoted(&mut self, text: &[char]) {
        self.write_indicator("\"", true, false, false);
        let (mut start, mut end) = (0usize, 0usize);
        while end <= text.len() {
            let ch = text.get(end).copied();
            let special = match ch {
                None => true,
                Some(c) => {
                    "\"\\\u{85}\u{2028}\u{2029}\u{FEFF}".contains(c)
                        || !('\u{20}'..='\u{7E}').contains(&c)
                }
            };
            if special {
                if start < end {
                    self.write_text(&text[start..end]);
                    start = end;
                }
                if let Some(c) = ch {
                    let data = match c {
                        '\0' => "\\0".to_string(),
                        '\u{7}' => "\\a".to_string(),
                        '\u{8}' => "\\b".to_string(),
                        '\t' => "\\t".to_string(),
                        '\n' => "\\n".to_string(),
                        '\u{B}' => "\\v".to_string(),
                        '\u{C}' => "\\f".to_string(),
                        '\r' => "\\r".to_string(),
                        '\u{1B}' => "\\e".to_string(),
                        '"' => "\\\"".to_string(),
                        '\\' => "\\\\".to_string(),
                        '\u{85}' => "\\N".to_string(),
                        '\u{A0}' => "\\_".to_string(),
                        '\u{2028}' => "\\L".to_string(),
                        '\u{2029}' => "\\P".to_string(),
                        c if is_printable(c) => c.to_string(),
                        c if (c as u32) <= 0xFF => format!("\\x{:02x}", c as u32),
                        c if c.len_utf16() == 2 => format!("\\U{:08x}", c as u32),
                        c => format!("\\u{:04x}", c as u32),
                    };
                    self.column += utf16_len(&data);
                    self.out.push_str(&data);
                    start = end + 1;
                }
            }
            end += 1;
        }
        self.write_indicator("\"", false, false, false);
    }

    fn write_literal(&mut self, text: &[char]) {
        let hints = determine_block_hints(text);
        self.write_indicator(&format!("|{hints}"), true, false, false);
        self.write_line_break(None);
        let mut breaks = true;
        let (mut start, mut end) = (0usize, 0usize);
        while end <= text.len() {
            let ch = text.get(end).copied();
            if breaks {
                if !ch.is_some_and(is_linebr) {
                    for &br in &text[start..end] {
                        self.write_line_break(if br == '\n' { None } else { Some(br) });
                    }
                    if ch.is_some() {
                        self.write_indent();
                    }
                    start = end;
                }
            } else if ch.is_none_or(is_linebr) {
                self.out.extend(text[start..end].iter());
                if ch.is_none() {
                    self.write_line_break(None);
                }
                start = end;
            }
            if let Some(c) = ch {
                breaks = is_linebr(c);
            }
            end += 1;
        }
    }

    fn check_simple_key(&self, key: &Value) -> Result<bool, String> {
        let (text, tag, _) = represent(key)?;
        let analysis = analyze(&text);
        let length = tag.prepared_len() + utf16_len(&text);
        Ok(length < MAX_SIMPLE_KEY_LENGTH && !analysis.empty && !analysis.multiline)
    }

    fn emit_node(&mut self, value: &Value, mapping: bool, simple_key: bool) -> Result<(), String> {
        self.mapping_context = mapping;
        self.simple_key_context = simple_key;
        match value {
            Value::List(items) if items.is_empty() => self.empty_flow("[", "]"),
            Value::Map(entries) if entries.is_empty() => self.empty_flow("{", "}"),
            Value::List(items) => self.block_sequence(items)?,
            Value::Map(entries) => self.block_mapping(entries)?,
            scalar => self.emit_scalar(scalar)?,
        }
        Ok(())
    }

    fn empty_flow(&mut self, open: &str, close: &str) {
        self.write_indicator(open, true, true, false);
        self.flow_level += 1;
        self.increase_indent(true, false);
        self.pop_indent();
        self.flow_level -= 1;
        self.write_indicator(close, false, false, false);
    }

    fn block_sequence(&mut self, items: &[Value]) -> Result<(), String> {
        let indentless = self.mapping_context && !self.indention;
        self.increase_indent(false, indentless);
        for (i, item) in items.iter().enumerate() {
            let first = i == 0;
            self.write_indent();
            if first {
                self.write_whitespace(INDICATOR_INDENT);
            }
            self.write_indicator("-", true, false, true);
            if first {
                self.indent = Some(self.indent.unwrap_or(0) + INDICATOR_INDENT);
            }
            self.emit_node(item, false, false)?;
        }
        self.pop_indent();
        Ok(())
    }

    fn block_mapping(&mut self, entries: &[(Value, Value)]) -> Result<(), String> {
        self.increase_indent(false, false);
        for (key, value) in entries {
            self.write_indent();
            if self.check_simple_key(key)? {
                self.emit_node(key, true, true)?;
                self.write_indicator(":", false, false, false);
                self.emit_node(value, true, false)?;
            } else {
                self.write_indicator("?", true, false, true);
                self.emit_node(key, true, false)?;
                self.write_indent();
                self.write_indicator(":", true, false, true);
                self.emit_node(value, true, false)?;
            }
        }
        self.pop_indent();
        Ok(())
    }
}

fn determine_block_hints(text: &[char]) -> String {
    let mut hints = String::new();
    if is_linebr(text[0]) || text[0] == ' ' {
        hints.push_str(&BEST_INDENT.to_string());
    }
    let last = text[text.len() - 1];
    if !is_linebr(last) {
        hints.push('-');
    } else if text.len() == 1 || is_linebr(text[text.len() - 2]) {
        hints.push('+');
    }
    hints
}

/// 맵을 SnakeYAML `Yaml.dump`와 같은 YAML 글로 쓴다(끝 줄바꿈 포함). 빈 맵은 `{}\n`이다.
/// 쓸 수 없는 값(`Other`, 목록·맵인 키)이 있으면 오류 글을 돌려준다.
pub fn dump_map(map: &[(Value, Value)]) -> Result<String, String> {
    let mut emitter = Emitter::new();
    emitter.emit_node(&Value::Map(map.to_vec()), false, false)?;
    emitter.write_indent();
    Ok(emitter.out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dump_title(title: &str) -> String {
        dump_map(&[(Value::Str("title".into()), Value::Str(title.into()))]).unwrap()
    }

    /// 아래 기대값은 진짜 SnakeYAML 2.4 `Yaml.dump`가 낸 글이다.
    #[test]
    fn scalars_are_quoted_exactly_like_snakeyaml() {
        for (input, expected) in [
            (":a", "title: :a\n"),
            ("yes", "title: 'yes'\n"),
            ("null", "title: 'null'\n"),
            ("007", "title: '007'\n"),
            ("", "title: ''\n"),
            (" lead", "title: ' lead'\n"),
            ("it's", "title: it's\n"),
            ("say \"hi\"", "title: say \"hi\"\n"),
            ("한글: 값", "title: '한글: 값'\n"),
            ("a\nb", "title: |-\n  a\n  b\n"),
            ("line1\nline2\n", "title: |\n  line1\n  line2\n"),
            ("x\n\n", "title: |+\n  x\n\n"),
            ("a\n b", "title: |-\n  a\n   b\n"),
            ("a\u{0}b", "title: !!binary |-\n  YQBi\n"),
        ] {
            assert_eq!(dump_title(input), expected, "입력 {input:?}");
        }
    }

    #[test]
    fn collections_use_the_indicator_indent_layout() {
        let text = |s: &str| Value::Str(s.into());
        let criterion = Value::Map(vec![
            (text("text"), text("a")),
            (text("done"), Value::Bool(true)),
            (text("checkedAt"), text("2026-11-01T09:00:00+09:00")),
        ]);
        let map = vec![
            (text("n"), Value::Int("5".into())),
            (text("y"), Value::Bool(true)),
            (text("z"), Value::Null),
            (text("f"), Value::Float(1.5)),
            (text("criteria"), Value::List(vec![criterion])),
            (text("labels"), Value::List(vec![text("x"), text("y")])),
            (text("empty"), Value::List(vec![])),
            (text("none"), Value::Map(vec![])),
        ];
        assert_eq!(
            dump_map(&map).unwrap(),
            "n: 5\ny: true\nz: null\nf: 1.5\ncriteria:\n  - text: a\n    done: true\n    checkedAt: '2026-11-01T09:00:00+09:00'\nlabels:\n  - x\n  - y\nempty: []\nnone: {}\n"
        );
        assert_eq!(dump_map(&[]).unwrap(), "{}\n");
        assert!(dump_map(&[(text("k"), Value::Other)]).is_err());
    }
}
