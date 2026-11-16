//! Antigravity(agy) 대화 기록 읽기. 파일은 `~/.gemini/antigravity-cli/conversations/<대화 id>.db` 하나이고, 안에 든 SQLite의 `steps` 표에
//! 단계가 한 줄씩 있다. 단계의 내용은 protobuf라서 `protobuf`로 필요한 필드만 읽는다.
//!
//! 읽는 단계는 셋뿐이다. 사용자 입력(step_type 14), 모델 답(15), 도구 호출(132). 시스템 알림(101), 이어쓰기 요약(23), API 오류 알림(17)은
//! 대화 내용이 아니라서 건너뛴다. 오래된 단계는 CLI가 내용을 비워 둔 채 남기므로(status 5) 글이 없는 단계는 회차로 세지 않는다.
//!
//! 엔진에는 SQLite 라이브러리가 없어서 시스템의 `sqlite3`(macOS는 `/usr/bin/sqlite3`)를 읽기 전용으로 부른다. 큰 도구 결과까지
//! 가져오지 않도록, 단계마다 필요한 앞부분만 SQL로 잘라서 받는다.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::{Regex, RegexBuilder};

use super::protobuf::{self, Field};
use super::{Parsed, Turn, add_text, add_tool};
use crate::json::{self, JsonValue};
use crate::kernel::java::java_blank;

/// 대화 id는 UUID다. 파일 이름으로 쓰므로 이 모양만 받는다.
static CONVERSATION_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$")
        .unwrap()
});

const META_PREFIX_BYTES: usize = 1200;
const MCP_TAIL_BYTES: usize = 160;
const ID_TAIL_BYTES: usize = 60;

fn steps_sql() -> String {
    // 줄을 잇는 `\`는 다음 줄의 앞 공백을 먹으므로, 낱말 사이 공백은 앞 줄 끝에 둔다
    format!(
        "select idx, step_type, status, ifnull(length(error_details),0), hex(substr(metadata,1,{META_PREFIX_BYTES})), \
         case when step_type=132 and instr(metadata, cast('\"ServerName\"' as blob))>0 \
         then hex(substr(metadata, instr(metadata, cast('\"ServerName\"' as blob)), {MCP_TAIL_BYTES})) else '' end, \
         case when step_type=132 and instr(metadata, cast('\"id\":\"' as blob))>0 \
         then hex(substr(metadata, instr(metadata, cast('\"id\":\"' as blob)), {ID_TAIL_BYTES})) else '' end, \
         case when step_type in (14,15) then hex(step_payload) else '' end \
         from steps order by idx"
    )
}

const META_SQL: &str = "select 'T', hex(substr(data,1,1500)) from trajectory_metadata_blob limit 1; \
     select 'E', hex(data) from executor_metadata order by idx desc limit 1; \
     select 'G', hex(data) from gen_metadata order by idx desc limit 1;";

/// 대화 id로 기록 파일을 찾는다. 폴더 두 곳(antigravity-cli, antigravity)을 차례로 본다. 없거나 id 모양이 틀리면 `None`.
pub fn find(directories: &[PathBuf], session_id: &str) -> Option<PathBuf> {
    if !CONVERSATION_ID.is_match(session_id) {
        return None;
    }
    let file_name = format!("{}.db", session_id.to_lowercase());
    directories.iter().map(|d| d.join(&file_name)).find(|f| {
        crate::ports::fs::metadata(f)
            .map(|m| m.is_file())
            .unwrap_or(false)
    })
}

pub fn is_antigravity_file(file: &Path) -> bool {
    file.file_name()
        .is_some_and(|n| n.to_string_lossy().ends_with(".db"))
}

pub fn parse(db: &Path) -> Result<Parsed, String> {
    let mut turns: Vec<Turn> = Vec::new();
    let mut lines = 0;
    let mut skipped = 0;
    for row in query(db, &steps_sql(), 8)? {
        lines += 1;
        if step(&row, &mut turns).is_none() {
            skipped += 1;
        }
    }
    for (i, turn) in turns.iter_mut().enumerate() {
        turn.index = i;
    }
    let (mut cwd, mut model, mut generated) = (None, None, None);
    for row in query(db, META_SQL, 2)? {
        let data = unhex(&row[1]);
        match row[0].as_str() {
            "T" => cwd = working_directory(&protobuf::parse(&data, true)),
            "E" => model = executor_model(&protobuf::parse(&data, true)),
            "G" => generated = generated_model(&protobuf::parse(&data, true)),
            _ => {}
        }
    }
    Ok(Parsed {
        turns,
        lines,
        skipped,
        cwd,
        model: model.or(generated),
    })
}

// ---------------------------------------------------------------- 단계 하나

/// 한 줄을 회차로 바꾼다. 줄이 틀렸으면 `None`(건너뛴 줄로 센다).
fn step(row: &[String], turns: &mut Vec<Turn>) -> Option<()> {
    let kind: i32 = row[1].parse().ok()?;
    if kind != 14 && kind != 15 && kind != 132 {
        return Some(());
    }
    let meta = protobuf::parse(&unhex(&row[4]), true);
    let at = timestamp(&protobuf::message(&meta, 1, true));
    if kind == 132 {
        tool(row, &meta, at.as_deref(), turns);
        return Some(());
    }
    let payload = protobuf::parse(&unhex(&row[7]), false);
    if kind == 14 {
        // 사용자 입력: 19번 메시지의 2번이 글이다
        add_text(
            turns,
            "user",
            at.as_deref(),
            protobuf::text(&protobuf::message(&payload, 19, false), 2),
        );
    } else {
        // 모델 답: 20번 메시지의 1번이 사용자에게 보이는 글, 3번은 생각이라 뺀다
        add_text(
            turns,
            "assistant",
            at.as_deref(),
            protobuf::text(&protobuf::message(&payload, 20, false), 1),
        );
    }
    Some(())
}

fn tool(row: &[String], meta: &Option<Vec<Field>>, at: Option<&str>, turns: &mut Vec<Turn>) {
    let call = protobuf::message(meta, 4, true);
    let Some(mut name) = protobuf::text(&call, 2) else {
        return;
    };
    let args = protobuf::text(&call, 3);
    let failed = row[2] == "7" || row[3] != "0";
    if name == "call_mcp_tool" {
        let tail = String::from_utf8_lossy(&unhex(&row[5])).into_owned();
        let server = json_string(&tail, "ServerName");
        let tool = json_string(&tail, "ToolName");
        name = format!(
            "{}{}",
            server.map_or(String::new(), |s| format!("{s}/")),
            tool.unwrap_or_else(|| "mcp".to_string())
        );
    }
    let id_tail = String::from_utf8_lossy(&unhex(&row[6])).into_owned();
    let merged = match &args {
        Some(a) => format!("{a} {id_tail}"),
        None => id_tail,
    };
    add_tool(turns, at, Some(name), summarize(&merged), None, failed);
}

/// 인자 가운데 사람이 읽을 만한 것 하나. Gemini CLI 쪽 요약과 같은 생각이고, 키 이름이 대문자로 시작하는 것을 함께 본다.
fn summarize(args: &str) -> Option<String> {
    [
        "description",
        "file_path",
        "path",
        "absolutepath",
        "commandline",
        "command",
        "query",
        "url",
        "id",
        "title",
    ]
    .iter()
    .find_map(|key| json_string(args, key).filter(|v| !java_blank(v)))
}

/// 잘린 JSON에서도 `"키":"값"`을 정규식으로 꺼낸다. 키는 대소문자를 가리지 않는다.
fn json_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!(
        r#""{}"[ \t\n\x0B\x0C\r]*:[ \t\n\x0B\x0C\r]*"((?:[^"\\]|\\[^\n\r\x{{85}}\x{{2028}}\x{{2029}}])*)""#,
        regex::escape(key)
    );
    let regex = RegexBuilder::new(&pattern)
        .case_insensitive(true)
        .build()
        .ok()?;
    let raw = regex.captures(json)?[1].to_string();
    match json::parse(&format!("\"{raw}\"")) {
        Ok(JsonValue::Str(s)) => Some(s),
        _ => Some(raw),
    }
}

fn timestamp(time: &Option<Vec<Field>>) -> Option<String> {
    let seconds = protobuf::number(time, 1)?;
    let nanos = protobuf::number(time, 2).unwrap_or(0);
    // `Instant.ofEpochSecond(seconds, nanos)`는 나노초를 초에 올려 맞춘다
    let seconds = seconds.checked_add(nanos.div_euclid(1_000_000_000))?;
    let nanos = nanos.rem_euclid(1_000_000_000);
    Some(instant_text(seconds, nanos as u32))
}

/// Java `Instant.toString()`: 나노초가 0이 아니면 3, 6, 9자리 가운데 필요한 만큼 소수를 붙인다.
fn instant_text(seconds: i64, nanos: u32) -> String {
    let base = crate::kernel::time::format_utc_millis(seconds.saturating_mul(1000));
    let head = &base[..base.len() - 5]; // `.000Z`를 뗀다
    let fraction = if nanos == 0 {
        String::new()
    } else if nanos.is_multiple_of(1_000_000) {
        format!(".{:03}", nanos / 1_000_000)
    } else if nanos.is_multiple_of(1000) {
        format!(".{:06}", nanos / 1000)
    } else {
        format!(".{nanos:09}")
    };
    format!("{head}{fraction}Z")
}

// ---------------------------------------------------------------- 대화 전체 정보

/// trajectory_metadata_blob의 7번(작업 폴더 `file:` 주소), 없으면 1번 안의 1번.
fn working_directory(metadata: &Option<Vec<Field>>) -> Option<String> {
    let url = protobuf::text(metadata, 7)
        .or_else(|| protobuf::text(&protobuf::message(metadata, 1, true), 1))?;
    let rest = url.strip_prefix("file:")?;
    // `URI.create(url).getPath()`: 권한 부분을 건너뛰고 질의·조각을 떼어 퍼센트 부호를 푼다. 허용되지 않는 글자는 null이다
    let after_authority = match rest.strip_prefix("//") {
        Some(r) => r.find('/').map_or("", |i| &r[i..]),
        None => rest,
    };
    let path = after_authority.split(['?', '#']).next().unwrap_or("");
    if path.chars().any(|c| {
        matches!(
            c,
            ' ' | '"' | '<' | '>' | '\\' | '^' | '`' | '{' | '|' | '}'
        ) || c.is_control()
    }) {
        return None;
    }
    percent_decode(path)
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = text.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// executor_metadata의 10번 > 1번 > 28번(모델 id, 예: gemini-3.8-flash-high).
fn executor_model(executor: &Option<Vec<Field>>) -> Option<String> {
    let config = protobuf::message(&protobuf::message(executor, 10, true), 1, true);
    protobuf::text(&config, 28)
}

/// gen_metadata의 1번 > 21번(화면에 쓰는 모델 이름). executor에서 못 찾았을 때만 쓴다.
fn generated_model(generated: &Option<Vec<Field>>) -> Option<String> {
    protobuf::text(&protobuf::message(generated, 1, true), 21)
}

// ---------------------------------------------------------------- sqlite3

/// 읽기 전용으로 질의하고 줄마다 `columns`칸으로 나눠 돌려준다. 칸은 16진 글과 숫자뿐이라 `|`로 나눠도 안전하다.
/// 평소 열기가 안 되면(쓰는 쪽이 잠갔거나 읽기 전용 폴더) 같은 파일을 immutable로 한 번 더 시도한다.
fn query(db: &Path, sql: &str, columns: usize) -> Result<Vec<Vec<String>>, String> {
    let absolute = crate::ports::fs::canonicalize(db).unwrap_or_else(|_| db.to_path_buf());
    let uri = format!("file:{}", percent_encode_path(&absolute.to_string_lossy()));
    let mut first: Option<String> = None;
    for options in ["?mode=ro", "?mode=ro&immutable=1"] {
        match run(&format!("{uri}{options}"), sql, columns) {
            Ok(rows) => return Ok(rows),
            Err(e) => {
                first.get_or_insert(e);
            }
        }
    }
    Err(first.unwrap_or_default())
}

fn percent_encode_path(path: &str) -> String {
    let mut out = String::new();
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'.' | b'_' | b'~' | b'-') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn run(uri: &str, sql: &str, columns: usize) -> Result<Vec<Vec<String>>, String> {
    let text = crate::ports::sqlite().query(uri, sql)?;
    Ok(super::reader_lines(&text)
        .into_iter()
        .filter(|line| !line.is_empty())
        .map(|line| line.split('|').map(str::to_string).collect::<Vec<_>>())
        .filter(|cells| cells.len() == columns)
        .collect())
}

fn unhex(hex: &str) -> Vec<u8> {
    let bytes = hex.as_bytes();
    (0..bytes.len() / 2)
        .map(|i| {
            let digit = |b: u8| (b as char).to_digit(16).unwrap_or(0xf) as u8;
            (digit(bytes[2 * i]) << 4) | digit(bytes[2 * i + 1])
        })
        .collect()
}
