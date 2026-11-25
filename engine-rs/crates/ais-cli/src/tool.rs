//! 도구 표면(`get_doc '{"id":"…"}'`)의 인자 해석. Java `ArgsParser.parseTool`과 같은 검증 순서와 오류 글이다.
//!
//! 도구는 JSON 객체 한 덩이를 받는다. 알려진 키만 받고(모르는 키가 있으면 거절), 스키마의 키 차례대로 필수 여부와 형을 본다.
//! 도구 표면의 호출자는 AI라서 엔진 호출의 호출 맥락을 AI로 정한다(기록 층 노트가 보이지 않고, 목록은 늘 전부다).

use serde_json::{Map, Value, json};

use ais_core::json::{self, JsonValue};
use ais_core::text::{compare_utf16, is_blank, trim, utf16_len};

use crate::env::SessionSource;
use crate::error::CliError;
use crate::map::{EngineCall, context_for, plain_args};
use crate::parse::ParsedCommand;
use ais_core::Caller;

/// 본문 칸의 글자 수 상한(UTF-16 코드 단위)
const BODY_LIMIT: usize = 200_000;

type Props = Vec<(String, JsonValue)>;

fn fail(tool: &str, message: impl AsRef<str>) -> CliError {
    CliError::new(400, format!("인자 검증 실패({tool}): {}", message.as_ref()))
}

fn get<'a>(props: &'a Props, key: &str) -> Option<&'a JsonValue> {
    props.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

/// 모르는 키가 있으면 거절한다(모르는 키는 UTF-16 차례로 늘어놓는다).
fn validate_keys(tool: &str, props: &Props, allowed: &[&str]) -> Result<(), CliError> {
    let mut unknown: Vec<&str> = props
        .iter()
        .map(|(k, _)| k.as_str())
        .filter(|k| !allowed.contains(k))
        .collect();
    if unknown.is_empty() {
        return Ok(());
    }
    unknown.sort_by(|a, b| compare_utf16(a, b));
    Err(fail(
        tool,
        format!(
            "모르는 키다: [{}]. 쓸 수 있는 것은 [{}] 뿐이다.",
            unknown.join(", "),
            allowed.join(", ")
        ),
    ))
}

/// 있는 문자열 칸의 값. 문자열이 아니면(JSON null 포함) 거절한다. 다듬지 않는다.
fn string_value(tool: &str, key: &str, value: &JsonValue) -> Result<String, CliError> {
    match value {
        JsonValue::Str(text) => Ok(text.clone()),
        other => Err(fail(
            tool,
            format!("{key} 는 문자열 여야 한다: {}", json::compact(other)),
        )),
    }
}

fn optional_string(tool: &str, props: &Props, key: &str) -> Result<Option<String>, CliError> {
    get(props, key)
        .map(|value| string_value(tool, key, value))
        .transpose()
}

fn required_string(tool: &str, props: &Props, key: &str) -> Result<String, CliError> {
    match get(props, key) {
        None => Err(fail(tool, format!("{key} 가 필요하다."))),
        Some(value) => string_value(tool, key, value),
    }
}

/// `true`·`false` 리터럴만 받는다.
fn bool_value(tool: &str, key: &str, value: &JsonValue) -> Result<bool, CliError> {
    match value {
        JsonValue::Literal(raw) if raw == "true" => Ok(true),
        JsonValue::Literal(raw) if raw == "false" => Ok(false),
        other => Err(fail(
            tool,
            format!("{key} 는 true 나 false 여야 한다: {}", json::compact(other)),
        )),
    }
}

fn optional_bool(tool: &str, props: &Props, key: &str) -> Result<Option<bool>, CliError> {
    get(props, key)
        .map(|value| bool_value(tool, key, value))
        .transpose()
}

/// 본문 칸의 글자 수 상한(B.8)
fn within_body_limit(tool: &str, key: &str, body: String) -> Result<String, CliError> {
    let length = utf16_len(&body);
    if length > BODY_LIMIT {
        return Err(fail(
            tool,
            format!("{key} 가 너무 길다(최대 {BODY_LIMIT}자): {length}자"),
        ));
    }
    Ok(body)
}

/// 문자열 배열 칸(`tags`): 모두 문자열이고 50개 이하
fn string_array(tool: &str, props: &Props, key: &str) -> Result<Option<Vec<String>>, CliError> {
    let Some(value) = get(props, key) else {
        return Ok(None);
    };
    let wrong = || {
        fail(
            tool,
            format!("{key} 는 문자열 배열 여야 한다: {}", json::compact(value)),
        )
    };
    let JsonValue::Array(items) = value else {
        return Err(wrong());
    };
    let mut out = Vec::new();
    for item in items {
        match item {
            JsonValue::Str(text) => out.push(text.clone()),
            _ => return Err(wrong()),
        }
    }
    if out.len() > 50 {
        return Err(fail(
            tool,
            format!("{key} 항목이 너무 많다(최대 50개): {}개", out.len()),
        ));
    }
    Ok(Some(out))
}

/// JSON 읽개의 값을 `serde_json` 값으로 옮긴다. 따옴표 없는 글(`true`, 숫자)은 형을 되살린다.
fn to_value(value: &JsonValue) -> Value {
    match value {
        JsonValue::Str(text) => Value::String(text.clone()),
        JsonValue::Literal(raw) => match raw.as_str() {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            "null" => Value::Null,
            other => other
                .parse::<i64>()
                .map(Value::from)
                .or_else(|_| other.parse::<f64>().map(Value::from))
                .unwrap_or_else(|_| Value::String(other.to_string())),
        },
        JsonValue::Array(items) => Value::Array(items.iter().map(to_value).collect()),
        JsonValue::Object(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, item)| (key.clone(), to_value(item)))
                .collect(),
        ),
    }
}

/// `links` 칸: 객체 배열이고 200개 이하
fn link_array(tool: &str, props: &Props) -> Result<Option<Value>, CliError> {
    let Some(value) = get(props, "links") else {
        return Ok(None);
    };
    let wrong = || {
        fail(
            tool,
            format!("links 는 객체 배열 여야 한다: {}", json::compact(value)),
        )
    };
    let JsonValue::Array(items) = value else {
        return Err(wrong());
    };
    if items
        .iter()
        .any(|item| !matches!(item, JsonValue::Object(_)))
    {
        return Err(wrong());
    }
    if items.len() > 200 {
        return Err(fail(
            tool,
            format!("links 항목이 너무 많다(최대 200개): {}개", items.len()),
        ));
    }
    Ok(Some(to_value(value)))
}

/// `template` 칸: 문자열이고 200자 이하
fn template_value(tool: &str, props: &Props) -> Result<Option<String>, CliError> {
    let Some(template) = optional_string(tool, props, "template")? else {
        return Ok(None);
    };
    let length = utf16_len(&template);
    if length > 200 {
        return Err(fail(
            tool,
            format!("template 가 너무 길다(최대 200자): {length}자"),
        ));
    }
    Ok(Some(template))
}

/// `limit` 칸: 1 이상의 정수 리터럴
fn tool_limit(tool: &str, value: &JsonValue) -> Result<i64, CliError> {
    let wrong = || {
        fail(
            tool,
            format!("limit 는 1 이상의 정수 여야 한다: {}", json::compact(value)),
        )
    };
    let JsonValue::Literal(raw) = value else {
        return Err(wrong());
    };
    if matches!(raw.as_str(), "true" | "false" | "null") {
        return Err(wrong());
    }
    let number: f64 = raw.parse().map_err(|_| wrong())?;
    if !number.is_finite() || number.trunc() != number || number < 1.0 || number > i64::MAX as f64 {
        return Err(wrong());
    }
    Ok(number as i64)
}

/// 부모 참조(`노트id` 또는 `노트id/하위노트id`)를 칸으로 쪼갠다. 빈 조각은 버리고, 조각은 다듬지 않는다.
fn parent_segments(tool: &str, raw: &str) -> Result<Vec<String>, CliError> {
    let segments: Vec<String> = raw
        .split('/')
        .filter(|segment| !is_blank(segment))
        .map(str::to_string)
        .collect();
    if segments.is_empty() || segments.len() >= 3 {
        return Err(fail(
            tool,
            format!("parent 는 `노트id` 이거나 `노트id/하위노트id` 여야 한다: {raw}"),
        ));
    }
    Ok(segments)
}

/// 도구 이름과 명령줄 칸에서 엔진 호출을 만든다.
pub fn parse_tool(
    tool: &str,
    raw: &[String],
    session: Option<&SessionSource>,
) -> Result<EngineCall, CliError> {
    // `--`로 시작하는 칸은 버린다. 남은 칸은 JSON 한 덩이여야 한다.
    let cells: Vec<&String> = raw.iter().filter(|cell| !cell.starts_with("--")).collect();
    if cells.len() >= 2 {
        return Err(CliError::new(
            400,
            format!(
                "{tool} 이 받는 인자는 JSON 한 덩어리인데 {} 개를 받았다. 셸에서 따옴표로 감쌌는지 본다: {tool} '{{\"키\":\"값\"}}'",
                cells.len()
            ),
        ));
    }
    let text = cells.first().map(|cell| trim(cell)).unwrap_or("{}");
    let text = if text.is_empty() { "{}" } else { text };
    let parsed = json::parse(text)
        .map_err(|_| CliError::new(400, format!("인자 JSON 파싱 실패({tool}): {text}")))?;
    let JsonValue::Object(props) = parsed else {
        return Err(fail(tool, format!("인자는 JSON 객체여야 한다: {text}")));
    };

    let (command, args) = build(tool, &props)?;
    Ok(EngineCall {
        command,
        args: Value::Object(args),
        // 도구 표면의 호출자는 AI다
        context: context_for(Caller::Ai, session),
    })
}

/// 자리 인자와 글 옵션으로 정해지는 도구를 명령 표면과 같은 길로 엔진 인자로 바꾼다.
fn plain(
    tool: &str,
    positional: Vec<String>,
    options: Vec<(String, String)>,
) -> (String, Map<String, Value>) {
    let name = tool.replace('_', "-");
    let parsed = ParsedCommand {
        name: name.clone(),
        positional,
        options,
    };
    (tool.to_string(), plain_args(&parsed))
}

fn put(args: &mut Map<String, Value>, key: &str, value: Option<impl Into<Value>>) {
    if let Some(value) = value {
        args.insert(key.into(), value.into());
    }
}

/// 노트 필터 칸를 스키마 차례로 검사해 담는다.
fn filter_fields(tool: &str, props: &Props, args: &mut Map<String, Value>) -> Result<(), CliError> {
    for key in FILTER_KEYS {
        let Some(value) = get(props, key) else {
            continue;
        };
        if key == "favorite" {
            if bool_value(tool, key, value)? {
                args.insert("favorite".into(), json!(true));
            }
            continue;
        }
        let text = string_value(tool, key, value)?;
        match key {
            "dateFrom" => args.insert("from".into(), json!(text)),
            "dateTo" => args.insert("to".into(), json!(text)),
            "tag" => {
                let tags: Vec<&str> = trim(&text).split(',').collect();
                args.insert("tags".into(), json!(tags))
            }
            other => args.insert(other.into(), json!(text)),
        };
    }
    Ok(())
}

const FILTER_KEYS: [&str; 9] = [
    "subAgendaId",
    "agenda",
    "date",
    "dateFrom",
    "dateTo",
    "type",
    "tag",
    "favorite",
    "favoriteCategory",
];

fn build(tool: &str, props: &Props) -> Result<(String, Map<String, Value>), CliError> {
    let mut args = Map::new();
    match tool {
        "get_doc" => {
            validate_keys(tool, props, &["id"])?;
            put(&mut args, "id", Some(required_string(tool, props, "id")?));
        }
        "get_sub_note" => {
            validate_keys(tool, props, &["parent", "id"])?;
            let parent = required_string(tool, props, "parent")?;
            let id = required_string(tool, props, "id")?;
            args.insert("parent".into(), json!(parent));
            args.insert("id".into(), json!(id));
        }
        "list_attachments" | "list_comments" | "list_decisions" => {
            validate_keys(tool, props, &["parent"])?;
            let parent = required_string(tool, props, "parent")?;
            let segments = parent_segments(tool, &parent)?;
            args.insert("id".into(), json!(segments[0]));
            if let Some(sub) = segments.get(1) {
                args.insert("subId".into(), json!(sub));
            }
        }
        "write_attachment" => {
            validate_keys(tool, props, &["parent", "name", "content"])?;
            let parent = required_string(tool, props, "parent")?;
            let segments = parent_segments(tool, &parent)?;
            args.insert("id".into(), json!(segments[0]));
            if let Some(sub) = segments.get(1) {
                args.insert("subId".into(), json!(sub));
            }
            for key in ["name", "content"] {
                let value = required_string(tool, props, key)?;
                let value = if key == "content" {
                    within_body_limit(tool, key, value)?
                } else {
                    value
                };
                args.insert(key.into(), json!(value));
            }
        }
        "list_sub_notes" => {
            validate_keys(tool, props, &["parent"])?;
            args.insert(
                "parent".into(),
                json!(required_string(tool, props, "parent")?),
            );
        }
        "list_docs" => {
            validate_keys(tool, props, &FILTER_KEYS)?;
            filter_fields(tool, props, &mut args)?;
        }
        "list_tags" => {
            validate_keys(tool, props, &["query", "limit"])?;
            put(&mut args, "query", optional_string(tool, props, "query")?);
            if let Some(value) = get(props, "limit") {
                let limit = tool_limit(tool, value)?;
                if limit > i64::from(i32::MAX) {
                    return Err(CliError::new(
                        400,
                        format!("limit 은 숫자여야 한다: {limit}"),
                    ));
                }
                args.insert("limit".into(), json!(limit));
            }
        }
        "search_docs" => {
            let mut allowed = vec!["query", "limit"];
            allowed.extend(FILTER_KEYS);
            validate_keys(tool, props, &allowed)?;
            let query = required_string(tool, props, "query")?;
            args.insert("query".into(), json!(trim(&query)));
            if let Some(value) = get(props, "limit") {
                let limit = tool_limit(tool, value)?;
                if limit > i64::from(i32::MAX) {
                    return Err(CliError::new(
                        400,
                        format!("limit 은 숫자여야 한다: {limit}"),
                    ));
                }
                args.insert("limit".into(), json!(limit));
            }
            filter_fields(tool, props, &mut args)?;
        }
        "list_templates" => {
            validate_keys(tool, props, &["category", "type"])?;
            put(
                &mut args,
                "category",
                optional_string(tool, props, "category")?,
            );
            put(&mut args, "type", optional_string(tool, props, "type")?);
        }
        "get_template" => {
            validate_keys(tool, props, &["id"])?;
            args.insert("id".into(), json!(required_string(tool, props, "id")?));
        }
        "list_trash" => validate_keys(tool, props, &[])?,
        "create_doc" => {
            validate_keys(
                tool,
                props,
                &[
                    "subAgendaIds",
                    "agenda",
                    "type",
                    "title",
                    "body",
                    "date",
                    "time",
                    "format",
                    "links",
                    "tags",
                    "template",
                ],
            )?;
            for key in ["agenda", "type", "title"] {
                args.insert(key.into(), json!(required_string(tool, props, key)?));
            }
            let body = required_string(tool, props, "body")?;
            args.insert("body".into(), json!(within_body_limit(tool, "body", body)?));
            for key in ["date", "time", "format"] {
                put(&mut args, key, optional_string(tool, props, key)?);
            }
            put(&mut args, "links", link_array(tool, props)?);
            put(&mut args, "tags", string_array(tool, props, "tags")?);
            put(&mut args, "template", template_value(tool, props)?);
            put(
                &mut args,
                "subAgendaIds",
                string_array(tool, props, "subAgendaIds")?,
            );
        }
        "update_doc" => {
            validate_keys(
                tool,
                props,
                &[
                    "id",
                    "body",
                    "title",
                    "tags",
                    "template",
                    "subAgendaIds",
                    "agenda",
                    "f1",
                ],
            )?;
            args.insert("id".into(), json!(required_string(tool, props, "id")?));
            if let Some(body) = optional_string(tool, props, "body")? {
                args.insert("body".into(), json!(within_body_limit(tool, "body", body)?));
            }
            put(&mut args, "title", optional_string(tool, props, "title")?);
            put(&mut args, "tags", string_array(tool, props, "tags")?);
            put(&mut args, "template", template_value(tool, props)?);
            put(
                &mut args,
                "subAgendaIds",
                string_array(tool, props, "subAgendaIds")?,
            );
            put(&mut args, "agenda", optional_string(tool, props, "agenda")?);
            put(&mut args, "f1", optional_string(tool, props, "f1")?);
        }
        "set_favorite" => {
            validate_keys(tool, props, &["id", "favorite", "category", "f1"])?;
            args.insert("id".into(), json!(required_string(tool, props, "id")?));
            put(
                &mut args,
                "favorite",
                optional_bool(tool, props, "favorite")?,
            );
            put(
                &mut args,
                "category",
                optional_string(tool, props, "category")?,
            );
            put(&mut args, "f1", optional_string(tool, props, "f1")?);
        }
        "evaluate_doc" => {
            validate_keys(tool, props, &["id", "score", "note", "clear", "f1"])?;
            args.insert("id".into(), json!(required_string(tool, props, "id")?));
            if let Some(value) = get(props, "score") {
                let wrong = || {
                    fail(
                        tool,
                        format!("score 는 숫자 여야 한다: {}", json::compact(value)),
                    )
                };
                let JsonValue::Literal(raw) = value else {
                    return Err(wrong());
                };
                if matches!(raw.as_str(), "true" | "false" | "null") {
                    return Err(wrong());
                }
                let score: f64 = raw.parse().map_err(|_| wrong())?;
                args.insert("score".into(), json!(score));
            }
            put(&mut args, "note", optional_string(tool, props, "note")?);
            put(&mut args, "clear", optional_bool(tool, props, "clear")?);
            put(&mut args, "f1", optional_string(tool, props, "f1")?);
        }
        "link_docs" => {
            validate_keys(tool, props, &["from", "to", "kind", "remove", "f1"])?;
            args.insert("from".into(), json!(required_string(tool, props, "from")?));
            args.insert("to".into(), json!(required_string(tool, props, "to")?));
            put(&mut args, "kind", optional_string(tool, props, "kind")?);
            put(&mut args, "remove", optional_bool(tool, props, "remove")?);
            put(&mut args, "f1", optional_string(tool, props, "f1")?);
        }
        "create_sub_note" => {
            validate_keys(tool, props, &["parent", "title", "body", "type"])?;
            let parent = required_string(tool, props, "parent")?;
            let mut options = vec![("title".to_string(), required_string(tool, props, "title")?)];
            let body = required_string(tool, props, "body")?;
            options.push(("body".to_string(), within_body_limit(tool, "body", body)?));
            if let Some(kind) = optional_string(tool, props, "type")? {
                options.push(("type".to_string(), kind));
            }
            return Ok(plain(tool, vec![parent], options));
        }
        "update_sub_note" => {
            validate_keys(tool, props, &["parent", "id", "body", "title", "f1"])?;
            let parent = required_string(tool, props, "parent")?;
            let id = required_string(tool, props, "id")?;
            let mut options = Vec::new();
            if let Some(body) = optional_string(tool, props, "body")? {
                options.push(("body".to_string(), within_body_limit(tool, "body", body)?));
            }
            for key in ["title", "f1"] {
                if let Some(value) = optional_string(tool, props, key)? {
                    options.push((key.to_string(), value));
                }
            }
            return Ok(plain(tool, vec![parent, id], options));
        }
        "delete_sub_note" => {
            validate_keys(tool, props, &["parent", "id"])?;
            let parent = required_string(tool, props, "parent")?;
            let id = required_string(tool, props, "id")?;
            return Ok(plain(tool, vec![parent, id], Vec::new()));
        }
        "save_template" => {
            let keys = ["id", "name", "type", "desc", "body"];
            validate_keys(tool, props, &keys)?;
            let mut options = Vec::new();
            for key in keys {
                if let Some(value) = optional_string(tool, props, key)? {
                    let value = if key == "body" {
                        within_body_limit(tool, "body", value)?
                    } else {
                        value
                    };
                    options.push((key.to_string(), value));
                }
            }
            return Ok(plain(tool, Vec::new(), options));
        }
        "delete_doc" | "restore_doc" | "purge_doc" => {
            let key = if tool == "delete_doc" {
                "id"
            } else {
                "trashId"
            };
            validate_keys(tool, props, &[key])?;
            return Ok(plain(
                tool,
                vec![required_string(tool, props, key)?],
                Vec::new(),
            ));
        }
        "export_markdown" => {
            validate_keys(tool, props, &[])?;
            return Ok(plain(tool, Vec::new(), Vec::new()));
        }
        "plan_day" => {
            validate_keys(tool, props, &["date", "agenda", "dryRun"])?;
            let mut options = Vec::new();
            for key in ["date", "agenda", "dryRun"] {
                let Some(value) = get(props, key) else {
                    continue;
                };
                if key == "dryRun" {
                    options.push((key.to_string(), bool_value(tool, key, value)?.to_string()));
                } else {
                    options.push((key.to_string(), string_value(tool, key, value)?));
                }
            }
            return Ok(plain(tool, Vec::new(), options));
        }
        _ => return resource_or_task_tool(tool, props),
    }
    Ok((tool.to_string(), args))
}

/// 자원 도구와 태스크 읽기 도구. 값이 있는 칸만 명령줄과 같은 글 옵션으로 옮기고, 자리 인자가 되는 칸은 옵션에서 뺀다.
fn resource_or_task_tool(
    tool: &str,
    props: &Props,
) -> Result<(String, Map<String, Value>), CliError> {
    const RESOURCE_BOOLS: [&str; 4] = ["bodies", "trash", "remove", "withSkills"];
    const TASK_BOOLS: [&str; 3] = ["blocked", "bodies", "trash"];
    let (keys, bools): (&[&str], &[&str]) = match tool {
        "list_resources" => (
            &["kind", "scope", "project", "bodies", "trash"],
            &RESOURCE_BOOLS,
        ),
        "get_resource" => (&["id", "version"], &RESOURCE_BOOLS),
        "save_resource" => (
            &[
                "id",
                "kind",
                "scope",
                "project",
                "name",
                "description",
                "body",
                "permission",
                "skills",
                "repo",
                "tools",
                "f1",
            ],
            &RESOURCE_BOOLS,
        ),
        "delete_resource"
        | "restore_resource"
        | "list_resource_versions"
        | "get_resource_change" => (&["id"], &RESOURCE_BOOLS),
        "set_resource_ref" => (&["project", "id", "remove", "withSkills"], &RESOURCE_BOOLS),
        "usable_resources" => (&["project", "bodies"], &RESOURCE_BOOLS),
        "list_resource_changes" => (&[], &RESOURCE_BOOLS),
        "list_tasks" => (
            &[
                "status", "type", "repo", "label", "q", "blocked", "bodies", "trash",
            ],
            &TASK_BOOLS,
        ),
        "get_task" => (&["id", "activity"], &TASK_BOOLS),
        "get_c4_model" => (&["project", "diagram"], &TASK_BOOLS),
        "apply_c4_changes" => (&["project", "diagram", "f1", "operations"], &TASK_BOOLS),
        "list_c4_diagrams" => (&["project"], &TASK_BOOLS),
        "create_c4_diagram" => (&["project", "name"], &TASK_BOOLS),
        "rename_c4_diagram" => (&["project", "diagram", "name"], &TASK_BOOLS),
        "delete_c4_diagram" => (&["project", "diagram"], &TASK_BOOLS),
        "add_task_comment" => (&["id", "body"], &TASK_BOOLS),
        "create_task" => (
            &[
                "title", "repo", "criteria", "type", "priority", "project", "base", "body",
                "labels", "refs",
            ],
            &TASK_BOOLS,
        ),
        _ => return Err(CliError::new(400, format!("미지원 도구: {tool}"))),
    };
    validate_keys(tool, props, keys)?;

    if tool == "add_task_comment" {
        required_string(tool, props, "id")?;
        required_string(tool, props, "body")?;
    }
    if tool == "create_task" {
        required_string(tool, props, "title")?;
        required_string(tool, props, "repo")?;
        match get(props, "criteria") {
            None => return Err(fail(tool, "criteria 가 필요하다.")),
            Some(JsonValue::Array(a)) if a.is_empty() => {
                return Err(fail(tool, "criteria 는 1개 이상이어야 한다."));
            }
            Some(JsonValue::Array(_)) => {}
            Some(other) => {
                return Err(fail(
                    tool,
                    format!("criteria 는 배열이어야 한다: {}", json::compact(other)),
                ));
            }
        }
    }

    let mut options: Vec<(String, String)> = Vec::new();
    for key in keys {
        let Some(value) = get(props, key) else {
            continue;
        };
        let text = if bools.contains(key) {
            bool_value(tool, key, value)?.to_string()
        } else if *key == "activity" {
            match value {
                JsonValue::Literal(raw)
                    if (1..=6).contains(&raw.len()) && raw.bytes().all(|b| b.is_ascii_digit()) =>
                {
                    raw.clone()
                }
                other => {
                    return Err(fail(
                        tool,
                        format!(
                            "activity 는 0 이상의 정수여야 한다: {}",
                            json::compact(other)
                        ),
                    ));
                }
            }
        } else if (*key == "skills" && matches!(value, JsonValue::Array(_)))
            || (*key == "tools" && matches!(value, JsonValue::Object(_)))
            || ((*key == "criteria" || *key == "labels" || *key == "refs" || *key == "operations")
                && matches!(value, JsonValue::Array(_)))
        {
            json::compact(value)
        } else {
            string_value(tool, key, value)?
        };
        let name = if *key == "withSkills" {
            "with-skills"
        } else {
            key
        };
        options.push((name.to_string(), text));
    }

    // 자리 인자: 프로젝트 slug와 자원·태스크 id는 명령줄에서 자리 인자로 받는다
    let mut positional = Vec::new();
    let positional_keys: &[&str] = match tool {
        "get_resource"
        | "delete_resource"
        | "restore_resource"
        | "list_resource_versions"
        | "get_resource_change"
        | "get_task"
        | "add_task_comment" => &["id"],
        "usable_resources" | "get_c4_model" | "apply_c4_changes" | "list_c4_diagrams"
        | "create_c4_diagram" | "rename_c4_diagram" | "delete_c4_diagram" => &["project"],
        "set_resource_ref" => &["project", "id"],
        _ => &[],
    };
    for key in positional_keys {
        positional.push(required_string(tool, props, key)?);
        options.retain(|(name, _)| name != key);
    }
    Ok(plain(tool, positional, options))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ais_core::CallContext;

    fn call(tool: &str, json: &str) -> Result<EngineCall, CliError> {
        parse_tool(tool, &[json.to_string()], None)
    }

    #[test]
    fn get_doc_takes_an_id_and_marks_the_caller() {
        let call = call("get_doc", r#"{"id":"n1"}"#).unwrap();
        assert_eq!(call.command, "get_doc");
        assert_eq!(call.args, json!({"id": "n1"}));
        assert_eq!(call.context, CallContext::ai(None, None));
    }

    /// 세션 표식은 인자가 아니라 호출 맥락에 실린다. `claude`는 에이전트 줄에 적지 않는다.
    #[test]
    fn the_session_goes_into_the_context_not_the_arguments() {
        let source = SessionSource {
            agent: "codex".to_string(),
            id: "s-1".to_string(),
        };
        let call = parse_tool("get_doc", &[r#"{"id":"n1"}"#.to_string()], Some(&source)).unwrap();
        assert_eq!(call.args, json!({"id": "n1"}));
        assert_eq!(
            call.context,
            CallContext::ai(Some("s-1".into()), Some("codex".into()))
        );
        let source = SessionSource {
            agent: "claude".to_string(),
            id: "s-2".to_string(),
        };
        let call = parse_tool("get_doc", &[r#"{"id":"n1"}"#.to_string()], Some(&source)).unwrap();
        assert_eq!(call.context, CallContext::ai(Some("s-2".into()), None));
    }

    #[test]
    fn unknown_keys_are_listed_sorted() {
        let error = call("get_doc", r#"{"z":1,"b":2}"#).unwrap_err();
        assert_eq!(
            error.message,
            "인자 검증 실패(get_doc): 모르는 키다: [b, z]. 쓸 수 있는 것은 [id] 뿐이다."
        );
    }

    #[test]
    fn missing_and_wrong_typed_keys_use_the_java_messages() {
        assert_eq!(
            call("get_doc", "{}").unwrap_err().message,
            "인자 검증 실패(get_doc): id 가 필요하다."
        );
        assert_eq!(
            call("get_doc", r#"{"id":5}"#).unwrap_err().message,
            "인자 검증 실패(get_doc): id 는 문자열 여야 한다: 5"
        );
    }

    #[test]
    fn two_cells_are_rejected_with_a_shell_hint() {
        let error = parse_tool("get_doc", &["{}".into(), "{}".into()], None).unwrap_err();
        assert!(
            error
                .message
                .starts_with("get_doc 이 받는 인자는 JSON 한 덩어리인데 2 개를 받았다.")
        );
    }

    #[test]
    fn list_docs_maps_filters_to_the_engine_names() {
        let call = call(
            "list_docs",
            r#"{"agenda":"demo","dateFrom":"2026-01-01","tag":"a,b","favorite":true}"#,
        )
        .unwrap();
        assert_eq!(
            call.args,
            json!({"agenda": "demo", "from": "2026-01-01", "tags": ["a", "b"], "favorite": true})
        );
    }

    #[test]
    fn parent_splits_into_id_and_sub_id() {
        let call = call("list_comments", r#"{"parent":"n1/s1"}"#).unwrap();
        assert_eq!(call.args, json!({"id": "n1", "subId": "s1"}));
        assert!(call_err("list_comments", r#"{"parent":"a/b/c"}"#).contains("노트id/하위노트id"));
    }

    fn call_err(tool: &str, json: &str) -> String {
        call(tool, json).unwrap_err().message
    }

    #[test]
    fn resource_tools_take_positionals_and_text_options() {
        let call = call(
            "set_resource_ref",
            r#"{"project":"p","id":"r1","withSkills":true}"#,
        )
        .unwrap();
        assert_eq!(call.command, "set_resource_ref");
        assert_eq!(
            call.args,
            json!({"project": "p", "id": "r1", "with-skills": "true"})
        );
    }

    #[test]
    fn unknown_tool_is_unsupported() {
        assert_eq!(call_err("nope_tool", "{}"), "미지원 도구: nope_tool");
    }

    #[test]
    fn create_task_parses_fields_and_serializes_arrays() {
        let call = call(
            "create_task",
            r#"{"title":"태스크1","repo":"user/repo","criteria":["기준1","기준2"],"labels":["bug"],"type":"bug"}"#,
        )
        .unwrap();
        assert_eq!(call.command, "create_task");
        assert_eq!(
            call.args,
            json!({
                "title": "태스크1",
                "repo": "user/repo",
                "criteria": "[\"기준1\",\"기준2\"]",
                "labels": "[\"bug\"]",
                "type": "bug",
            })
        );
    }

    #[test]
    fn create_task_validates_required_fields() {
        assert!(
            call_err("create_task", r#"{"repo":"user/repo","criteria":["c1"]}"#).contains("title")
        );
        assert!(call_err("create_task", r#"{"title":"t","criteria":["c1"]}"#).contains("repo"));
        assert!(
            call_err("create_task", r#"{"title":"t","repo":"r"}"#).contains("criteria 가 필요하다")
        );
        assert!(
            call_err("create_task", r#"{"title":"t","repo":"r","criteria":[]}"#)
                .contains("1개 이상")
        );
    }
}
