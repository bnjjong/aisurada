//! 명령 표면의 해석 결과를 엔진 호출(`snake_case` 이름과 JSON 인자)로 바꾼다.
//!
//! 대부분의 명령은 옵션 이름과 값을 글 그대로 넘긴다. Rust 엔진의 요청이 글을 스스로 읽고 Java와 같은 오류 글을 내기 때문이다.
//! 형이 정해진 읽기 명령(`list-docs` 따위)만 여기서 글을 수·불리언·목록으로 바꾼다.

use serde_json::{Map, Value, json};

use ais_core::text::{is_blank, trim};
use ais_core::{CallContext, Caller};

use crate::env::{SessionSource, agent_for_file};
use crate::error::CliError;
use crate::parse::ParsedCommand;

/// 엔진에 보낼 호출 하나
#[derive(Debug, Clone, PartialEq)]
pub struct EngineCall {
    /// `snake_case` 명령 이름
    pub command: String,
    pub args: Value,
    /// 호출 맥락: 누가(앱 또는 AI 도구), 어느 세션에서 보냈는지. 인자 밖에서 엔진에 넘긴다.
    pub context: CallContext,
}

/// 자리 인자가 들어갈 인자 이름. 앞에서부터 차례로 채운다.
fn positional_names(command: &str) -> &'static [&'static str] {
    match command {
        "get-doc"
        | "get-template"
        | "update-agenda"
        | "delete-agenda"
        | "update-project"
        | "delete-project"
        | "get-agenda-facts"
        | "update-agenda-facts"
        | "save-ritual-template"
        | "delete-doc-template"
        | "set-favorite"
        | "evaluate-doc"
        | "update-doc"
        | "delete-doc"
        | "get-task"
        | "update-task"
        | "transition-task"
        | "add-task-comment"
        | "delete-task"
        | "restore-task"
        | "get-resource"
        | "delete-resource"
        | "restore-resource"
        | "list-resource-versions"
        | "get-resource-change"
        | "approve-resource-change"
        | "reject-resource-change"
        | "resolve-resource-edit"
        | "read-session" => &["id"],
        "list-sub-notes" | "create-sub-note" => &["parent"],
        "get-sub-note" | "update-sub-note" | "delete-sub-note" => &["parent", "id"],
        "list-comments" | "list-decisions" | "list-attachments" | "create-comment"
        | "update-comment" | "delete-comment" | "write-attachment" | "get-ask"
        | "append-ask-turn" | "list-chats" | "append-chat-turn" => &["id", "subId"],
        "history" => &["id", "number"],
        "link-docs" | "link-tasks" => &["from", "to"],
        "restore-doc" | "purge-doc" => &["trashId"],
        "usable-resources" | "get-c4-model" | "apply-c4-changes" | "list-c4-diagrams"
        | "create-c4-diagram" | "rename-c4-diagram" | "delete-c4-diagram" => &["project"],
        "set-resource-ref" => &["project", "id"],
        "search-docs" => &["query"],
        _ => &[],
    }
}

/// 호출자와 세션 표식으로 호출 맥락을 만든다. 쓰기 명령이 노트에 누가 고쳤는지 적을 때 읽는다.
/// 에이전트 이름은 노트에 적는 값(`claude`는 적지 않는다)일 때만 싣는다.
pub fn context_for(caller: Caller, session: Option<&SessionSource>) -> CallContext {
    CallContext {
        caller,
        session: session.map(|source| source.id.clone()),
        session_agent: session.and_then(|source| agent_for_file(&source.agent)),
        manager: None,
    }
}

/// 명령 표면 호출을 엔진 호출로 바꾼다.
pub fn to_call(
    parsed: &ParsedCommand,
    session: Option<&SessionSource>,
) -> Result<EngineCall, CliError> {
    let command = parsed.name.replace('-', "_");
    let args = match parsed.name.as_str() {
        "list-docs" => list_docs_args(parsed)?,
        "search-docs" => search_docs_args(parsed)?,
        "list-tags" => list_tags_args(parsed)?,
        "list-templates" => list_templates_args(parsed)?,
        "list-sub-notes" => list_sub_notes_args(parsed)?,
        "get-sub-note" => {
            let mut args = plain_args(parsed);
            if let Some(raw) = parsed.option("includeIdPath") {
                let include = trim(raw).parse::<bool>().map_err(|_| {
                    CliError::new(
                        400,
                        format!("includeIdPath 은 true 나 false 여야 한다: {raw}"),
                    )
                })?;
                args.insert("includeIdPath".into(), json!(include));
            }
            args
        }
        "history" => history_args(parsed)?,
        "session-titles" => {
            let mut args = Map::new();
            args.insert("ids".into(), json!(parsed.positional));
            args
        }
        "reindex-paths" => {
            let mut args = Map::new();
            args.insert("paths".into(), json!(parsed.positional));
            args
        }
        _ => plain_args(parsed),
    };
    Ok(EngineCall {
        command,
        args: Value::Object(args),
        context: context_for(Caller::App, session),
    })
}

/// 자리 인자는 이름표를 붙이고, 옵션은 글 그대로 넘긴다.
pub(crate) fn plain_args(parsed: &ParsedCommand) -> Map<String, Value> {
    let mut args = Map::new();
    for (key, value) in &parsed.options {
        args.insert(key.clone(), json!(value));
    }
    for (name, value) in positional_names(&parsed.name)
        .iter()
        .zip(parsed.positional.iter())
    {
        args.insert((*name).into(), json!(value));
    }
    args
}

/// 위치 인자 `index`번의 값. 없으면 빈 글.
fn positional(parsed: &ParsedCommand, index: usize) -> &str {
    parsed
        .positional
        .get(index)
        .map(String::as_str)
        .unwrap_or("")
}

/// `bodies` 옵션: `true`면 참, `false`나 빈 글이면 거짓, 그 밖은 오류
fn bodies_flag(parsed: &ParsedCommand) -> Result<bool, CliError> {
    match parsed.option("bodies").map(trim) {
        None | Some("") | Some("false") => Ok(false),
        Some("true") => Ok(true),
        Some(other) => Err(CliError::new(
            400,
            format!("bodies 은 true 나 false 여야 한다: {other}"),
        )),
    }
}

/// `limit` 글을 32비트 정수로 읽는다. 숫자가 아니거나 범위를 넘으면 오류다. 빈 글은 `None`.
fn limit_number(parsed: &ParsedCommand) -> Result<Option<i32>, CliError> {
    let Some(raw) = parsed.option("limit") else {
        return Ok(None);
    };
    let text = trim(raw);
    if text.is_empty() {
        return Ok(None);
    }
    text.parse::<i64>()
        .ok()
        .and_then(|number| i32::try_from(number).ok())
        .map(Some)
        .ok_or_else(|| CliError::new(400, format!("limit 은 숫자여야 한다: {text}")))
}

/// 노트 필터 옵션(`agenda`, `type`, `tag`, `favorite`, `favoriteCategory`, `date`, `from`, `to`)을 엔진 인자로 옮긴다.
fn filter_args(parsed: &ParsedCommand, args: &mut Map<String, Value>) -> Result<(), CliError> {
    for key in [
        "agenda",
        "type",
        "favoriteCategory",
        "subAgendaId",
        "date",
        "from",
        "to",
    ] {
        if let Some(value) = parsed.option(key) {
            args.insert(key.into(), json!(value));
        }
    }
    if let Some(tag) = parsed.option("tag") {
        let tags: Vec<&str> = trim(tag).split(',').collect();
        args.insert("tags".into(), json!(tags));
    }
    if let Some(raw) = parsed.option("favorite") {
        let text = trim(raw);
        if !text.is_empty() {
            match text.to_lowercase().as_str() {
                "true" => {
                    args.insert("favorite".into(), json!(true));
                }
                // false는 조건을 안 건 것과 같다
                "false" => {}
                other => {
                    return Err(CliError::new(
                        400,
                        format!(
                            "모르는 즐겨찾기 값 이다: {other}. 쓸 수 있는 것은 [true, false] 뿐이다."
                        ),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn list_docs_args(parsed: &ParsedCommand) -> Result<Map<String, Value>, CliError> {
    let mut args = Map::new();
    filter_args(parsed, &mut args)?;
    // 명령 표면의 기본은 100건이다. 0 이하는 전부, 빈 글은 기본 100건이다.
    if parsed.option("limit").is_some() {
        let limit = limit_number(parsed)?.map_or(100, |n| if n <= 0 { 0 } else { n });
        args.insert("limit".into(), json!(limit));
    }
    if bodies_flag(parsed)? {
        args.insert("bodies".into(), json!(true));
    }
    Ok(args)
}

fn search_docs_args(parsed: &ParsedCommand) -> Result<Map<String, Value>, CliError> {
    let mut args = Map::new();
    filter_args(parsed, &mut args)?;
    // 검색어: 비지 않은 첫 자리 인자(다듬지 않는다), 없으면 `query=` 옵션(다듬는다)
    let first = positional(parsed, 0);
    let query = if !is_blank(first) {
        first.to_string()
    } else {
        parsed.option("query").map(trim).unwrap_or("").to_string()
    };
    args.insert("query".into(), json!(query));
    if let Some(limit) = limit_number(parsed)? {
        args.insert("limit".into(), json!(limit));
    }
    Ok(args)
}

fn list_tags_args(parsed: &ParsedCommand) -> Result<Map<String, Value>, CliError> {
    let mut args = Map::new();
    if let Some(query) = parsed.option("query") {
        args.insert("query".into(), json!(query));
    }
    if let Some(limit) = limit_number(parsed)? {
        args.insert("limit".into(), json!(limit));
    }
    Ok(args)
}

fn list_templates_args(parsed: &ParsedCommand) -> Result<Map<String, Value>, CliError> {
    let mut args = Map::new();
    for key in ["category", "type"] {
        if let Some(value) = parsed.option(key) {
            args.insert(key.into(), json!(value));
        }
    }
    if bodies_flag(parsed)? {
        args.insert("bodies".into(), json!(true));
    }
    Ok(args)
}

fn list_sub_notes_args(parsed: &ParsedCommand) -> Result<Map<String, Value>, CliError> {
    let mut args = Map::new();
    args.insert("parent".into(), json!(positional(parsed, 0)));
    if bodies_flag(parsed)? {
        args.insert("bodies".into(), json!(true));
    }
    Ok(args)
}

fn history_args(parsed: &ParsedCommand) -> Result<Map<String, Value>, CliError> {
    let mut args = Map::new();
    args.insert("id".into(), json!(positional(parsed, 0)));
    if let Some(raw) = parsed.positional.get(1) {
        // 판 번호는 다듬지 않고 읽는다. 읽지 못하면 1 미만과 같은 오류다.
        let number = raw.parse::<i32>().ok().ok_or_else(|| {
            CliError::new(400, format!("판 번호는 1 이상의 정수여야 한다: {raw}"))
        })?;
        args.insert("number".into(), json!(number));
    }
    for key in ["commit", "query"] {
        if let Some(value) = parsed.option(key) {
            args.insert(key.into(), json!(value));
        }
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{parse, registry};

    fn call(name: &str, args: &[&str]) -> Result<EngineCall, CliError> {
        let spec = registry::command(name).unwrap();
        let parsed = parse::parse_command(
            spec,
            &args.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        )?;
        to_call(&parsed, None)
    }

    /// 명령 표면의 호출자는 앱이다. 세션 표식은 인자가 아니라 호출 맥락에 실린다.
    #[test]
    fn the_command_surface_calls_as_the_app_with_the_session_in_the_context() {
        let spec = registry::command("get-doc").unwrap();
        let parsed = parse::parse_command(spec, &["n1".to_string()]).unwrap();
        let plain = to_call(&parsed, None).unwrap();
        assert_eq!(plain.context, CallContext::app());
        let source = SessionSource {
            agent: "gemini".to_string(),
            id: "g-1".to_string(),
        };
        let with = to_call(&parsed, Some(&source)).unwrap();
        assert_eq!(with.args, plain.args);
        assert_eq!(
            with.context,
            CallContext {
                caller: Caller::App,
                session: Some("g-1".into()),
                session_agent: Some("gemini".into()),
                manager: None,
            }
        );
    }

    #[test]
    fn positionals_get_their_names() {
        let call = call("get-sub-note", &["p", "s"]).unwrap();
        assert_eq!(call.command, "get_sub_note");
        assert_eq!(call.args, json!({"parent": "p", "id": "s"}));
    }

    #[test]
    fn optional_second_positional_is_left_out() {
        let call = call("list-comments", &["n1"]).unwrap();
        assert_eq!(call.args, json!({"id": "n1"}));
    }

    #[test]
    fn list_docs_limit_follows_the_command_surface_rules() {
        assert_eq!(call("list-docs", &[]).unwrap().args, json!({}));
        assert_eq!(
            call("list-docs", &["limit=0"]).unwrap().args,
            json!({"limit": 0})
        );
        assert_eq!(
            call("list-docs", &["limit=-3"]).unwrap().args,
            json!({"limit": 0})
        );
        assert_eq!(
            call("list-docs", &["limit= "]).unwrap().args,
            json!({"limit": 100})
        );
        assert_eq!(
            call("list-docs", &["limit=abc"]).unwrap_err().message,
            "limit 은 숫자여야 한다: abc"
        );
        assert_eq!(
            call("list-docs", &["limit=99999999999"])
                .unwrap_err()
                .message,
            "limit 은 숫자여야 한다: 99999999999"
        );
    }

    #[test]
    fn filters_become_typed_arguments() {
        let call = call("list-docs", &["tag= a, b ,", "favorite=TRUE", "type=plan"]).unwrap();
        assert_eq!(
            call.args,
            json!({"tags": ["a", " b ", ""], "favorite": true, "type": "plan"})
        );
        assert_eq!(
            super::tests::call("list-docs", &["favorite=maybe"])
                .unwrap_err()
                .message,
            "모르는 즐겨찾기 값 이다: maybe. 쓸 수 있는 것은 [true, false] 뿐이다."
        );
    }

    #[test]
    fn search_query_prefers_a_non_blank_positional() {
        assert_eq!(
            call("search-docs", &["찾기", "query=무시"]).unwrap().args,
            json!({"query": "찾기"})
        );
        assert_eq!(
            call("search-docs", &["query= 옵션 "]).unwrap().args,
            json!({"query": "옵션"})
        );
    }

    #[test]
    fn history_number_must_be_an_integer() {
        assert_eq!(
            call("history", &["n1", "2"]).unwrap().args,
            json!({"id": "n1", "number": 2})
        );
        assert_eq!(
            call("history", &["n1", "x"]).unwrap_err().message,
            "판 번호는 1 이상의 정수여야 한다: x"
        );
    }

    #[test]
    fn session_titles_collects_all_positionals() {
        assert_eq!(
            call("session-titles", &["a", "b"]).unwrap().args,
            json!({"ids": ["a", "b"]})
        );
    }
}
