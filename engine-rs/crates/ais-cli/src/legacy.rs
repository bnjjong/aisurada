//! 엔진 응답을 Java `ais-cli`가 내던 옛 모양으로 되돌린다.
//!
//! Rust 엔진은 앱을 위해 읽기 응답 16개를 새 모양(`{items, total, returned, truncated}` 따위)으로 바꿨다. `ais-cli`와 MCP를 읽는
//! 에이전트의 프롬프트·스킬은 옛 모양에 맞춰져 있으므로 여기서 거꾸로 바꿔 낸다. 새 모양에서 빠진 값(`date`, `time`,
//! `agendaTitle`, `deepLink`, `ref`, `parent`)은 요청 인자와 vault의 메타정보를 읽어 채운다.
//!
//! 옛 `list-trash`·`list-templates`는 못 읽은 항목이 있으면 `{trash|templates, incomplete}` 봉투로 감싸고 표준 오류에 그 내용을 적었다.
//! 새 응답에는 그 정보가 없으므로 건너뛴 항목이 있는지 확인하여 복원한다.

use std::path::Path;

use serde_json::{Map, Value, json};

use ais_core::Caller;
use ais_core::text::{encode_uri_component, trim};
use ais_core::vault::agenda::{AgendaTable, scan_agendas};
use ais_core::vault::facts::read_facts;
use ais_core::vault::note_meta::parse_created;
use ais_core::vault::template::{Category, scan_templates};
use ais_core::vault::trash::scan_trash;

use crate::map::EngineCall;

/// 못 읽은 항목이 있을 때 `incomplete.note`에 적는 글
const INCOMPLETE_NOTE: &str =
    "일부 파일을 읽지 못했어요(형식 오류·크기 초과) — 목록이 불완전할 수 있어요.";

/// 검색 결과가 잘렸을 때 `hint`에 적는 글
const SEARCH_HINT: &str = "agenda·type·tag·dateFrom으로 좁히거나 limit을 올리세요";

/// 되돌린 응답과, 표준 오류에 덧붙일 진단 글
#[derive(Debug, Clone, PartialEq)]
pub struct Legacy {
    pub value: Value,
    /// 옛 엔진이 표준 오류에 적던 알림(못 읽어 뺀 항목 따위). 없으면 빈 글
    pub diagnostics: String,
}

impl Legacy {
    fn plain(value: Value) -> Legacy {
        Legacy {
            value,
            diagnostics: String::new(),
        }
    }
}

/// 새 모양의 응답을 옛 모양으로 되돌린다. 모양이 바뀌지 않은 명령은 그대로 돌려준다.
pub fn to_legacy(call: &EngineCall, vault: &str, response: Value) -> Legacy {
    match call.command.as_str() {
        "list_docs" => Legacy::plain(list_docs(vault, response)),
        "list_agendas" => Legacy::plain(list_agendas(vault, response)),
        "list_projects" | "list_tags" => Legacy::plain(items(response)),
        "list_templates" => list_templates(call, vault, response),
        "list_trash" => list_trash(vault, response),
        "get_doc" => Legacy::plain(get_doc(call, vault, response)),
        "search_docs" => Legacy::plain(search_docs(call, vault, response)),
        "list_sub_notes" => Legacy::plain(list_sub_notes(call, response)),
        "get_sub_note" => Legacy::plain(get_sub_note(call, response)),
        "list_attachments" => Legacy::plain(list_attachments(call, response)),
        _ => Legacy::plain(response),
    }
}

/// `{items, …}`에서 항목 배열만 꺼낸다.
fn items(response: Value) -> Value {
    match response {
        Value::Object(mut map) => map.remove("items").unwrap_or(Value::Array(Vec::new())),
        other => other,
    }
}

fn rows(response: Value) -> Vec<Value> {
    match items(response) {
        Value::Array(rows) => rows,
        _ => Vec::new(),
    }
}

fn text<'a>(call: &'a EngineCall, key: &str) -> &'a str {
    call.args.get(key).and_then(Value::as_str).unwrap_or("")
}

/// `created` 문자열로부터 날짜(`YYYY-MM-DD`)와 시각(`HH:MM`)을 꺼낸다.
fn extract_date_time(created_str: &str) -> Option<(String, String)> {
    parse_created(created_str).map(|c| (c.date, c.time))
}

/// 노트 줄에서 새 모양에만 있는 키(`folder`)를 빼고, 옛 모양에 있던 `agendaTitle`·`date`·`time`을 채운다.
fn restore_note_row(row: &mut Map<String, Value>, agendas: Option<&AgendaTable>) {
    row.remove("folder");
    let slug = row
        .get("agenda")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    row.insert(
        "agendaTitle".into(),
        json!(agendas.map_or("", |table| table.title_of(&slug))),
    );
    if let Some(created_str) = row.get("created").and_then(Value::as_str)
        && let Some((date, time)) = extract_date_time(created_str)
    {
        row.insert("date".into(), json!(date));
        row.insert("time".into(), json!(time));
    }
}

/// 하위 노트 줄: 새 모양의 `rootId`·`parentId`·`folder`를 빼고 `date`·`time`을 채운다.
fn restore_sub_row(row: &mut Map<String, Value>) {
    for key in ["rootId", "parentId", "folder"] {
        row.remove(key);
    }
    if let Some(created_str) = row.get("created").and_then(Value::as_str)
        && let Some((date, time)) = extract_date_time(created_str)
    {
        row.insert("date".into(), json!(date));
        row.insert("time".into(), json!(time));
    }
}

/// 목록의 줄은 `tags`가 비면 키째 뺐다(상세와 하위 노트 줄은 늘 싣는다).
fn drop_empty_tags(row: &mut Map<String, Value>) {
    if row
        .get("tags")
        .and_then(Value::as_array)
        .is_some_and(Vec::is_empty)
    {
        row.remove("tags");
    }
}

fn load_agendas(vault: &str) -> Option<AgendaTable> {
    Some(scan_agendas(Path::new(vault)).table)
}

fn list_docs(vault: &str, response: Value) -> Value {
    let agendas = load_agendas(vault);
    Value::Array(
        rows(response)
            .into_iter()
            .map(|row| match row {
                Value::Object(mut map) => {
                    restore_note_row(&mut map, agendas.as_ref());
                    drop_empty_tags(&mut map);
                    Value::Object(map)
                }
                other => other,
            })
            .collect(),
    )
}

fn list_agendas(vault: &str, response: Value) -> Value {
    Value::Array(
        rows(response)
            .into_iter()
            .map(|row| match row {
                Value::Object(mut map) => {
                    let slug = map
                        .get("slug")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    map.insert(
                        "deepLink".into(),
                        json!(format!(
                            "aisurada://agenda/{}?ws={}",
                            encode_uri_component(&slug),
                            encode_uri_component(vault)
                        )),
                    );
                    Value::Object(map)
                }
                other => other,
            })
            .collect(),
    )
}

/// 템플릿 응답. 못 읽은 템플릿이 있으면 `{templates, incomplete}` 봉투와 표준 오류 진단을 되살린다.
fn list_templates(call: &EngineCall, vault: &str, response: Value) -> Legacy {
    let rows = Value::Array(rows(response));
    let category = match trim(text(call, "category")).to_lowercase().as_str() {
        "doc" => Some(Category::Doc),
        "ritual" => Some(Category::Ritual),
        _ => None,
    };
    let filter_type = !trim(text(call, "type")).is_empty();
    let skipped: Vec<_> = scan_templates(Path::new(vault))
        .skipped
        .into_iter()
        .filter(|s| category.is_none_or(|c| c == s.category))
        .filter(|s| !filter_type || s.category == Category::Doc)
        .collect();
    if skipped.is_empty() {
        return Legacy::plain(rows);
    }
    let mut diagnostics = format!("템플릿 {}건을 못 읽어 목록에서 뺐다.\n", skipped.len());
    for entry in &skipped {
        diagnostics.push_str(&format!("  {} — {}\n", entry.rel_path, entry.reason));
    }
    Legacy {
        value: json!({
            "templates": rows,
            "incomplete": {"skipped": skipped.len(), "conflicts": 0, "note": INCOMPLETE_NOTE},
        }),
        diagnostics,
    }
}

/// 휴지통 응답. 못 읽은 항목이 있으면 `{trash, incomplete}` 봉투와 표준 오류 진단을 되살린다.
fn list_trash(vault: &str, response: Value) -> Legacy {
    let rows = Value::Array(rows(response));
    let skipped = scan_trash(Path::new(vault)).skipped;
    if skipped.is_empty() {
        return Legacy::plain(rows);
    }
    let mut diagnostics = format!("휴지통 항목 {}건을 못 읽어 목록에서 뺐다.\n", skipped.len());
    for entry in &skipped {
        diagnostics.push_str(&format!("  {} — {}\n", entry.location, entry.reason));
    }
    Legacy {
        value: json!({
            "trash": rows,
            "incomplete": {"skipped": skipped.len(), "conflicts": 0, "note": INCOMPLETE_NOTE},
        }),
        diagnostics,
    }
}

fn get_doc(call: &EngineCall, vault: &str, response: Value) -> Value {
    let Value::Object(mut map) = response else {
        return response;
    };
    let agendas = load_agendas(vault);
    restore_note_row(&mut map, agendas.as_ref());
    // 도구 표면(AI)에는 노트가 속한 아젠다의 사실층도 실어 준다
    if call.context.caller == Caller::Ai {
        let slug = map
            .get("agenda")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if (call.context.manager.is_none()
            || agendas.as_ref().is_some_and(|table| table.has_slug(&slug)))
            && let Some(facts) = read_facts(Path::new(vault), &slug)
        {
            map.insert("agendaFacts".into(), json!(facts));
        }
    }
    let id = map
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    map.insert(
        "deepLink".into(),
        json!(format!(
            "aisurada://doc/{}?ws={}",
            encode_uri_component(&id),
            encode_uri_component(vault)
        )),
    );
    Value::Object(map)
}

fn search_docs(call: &EngineCall, vault: &str, response: Value) -> Value {
    let total = response.get("total").cloned().unwrap_or(json!(0));
    let truncated = response.get("truncated") == Some(&Value::Bool(true));
    let agendas = load_agendas(vault);
    let docs: Vec<Value> = rows(response)
        .into_iter()
        .map(|row| match row {
            Value::Object(mut map) => {
                restore_note_row(&mut map, agendas.as_ref());
                drop_empty_tags(&mut map);
                Value::Object(map)
            }
            other => other,
        })
        .collect();
    let mut envelope = Map::new();
    envelope.insert("query".into(), json!(text(call, "query")));
    envelope.insert("total".into(), total);
    envelope.insert("returned".into(), json!(docs.len()));
    if truncated {
        envelope.insert("truncated".into(), json!(true));
        envelope.insert("hint".into(), json!(SEARCH_HINT));
    }
    envelope.insert("docs".into(), Value::Array(docs));
    Value::Object(envelope)
}

/// 부모 참조를 `/`로 쪼개 다듬고 빈 조각을 버린 뒤 다시 잇는다(옛 응답의 `parent`).
fn normalized_parent(raw: &str) -> String {
    raw.split('/')
        .map(trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

fn list_sub_notes(call: &EngineCall, response: Value) -> Value {
    let parent = normalized_parent(text(call, "parent"));
    let sub_notes: Vec<Value> = rows(response)
        .into_iter()
        .map(|row| match row {
            Value::Object(mut map) => {
                restore_sub_row(&mut map);
                let id = map
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                map.insert("ref".into(), json!(format!("{parent}/{id}")));
                Value::Object(map)
            }
            other => other,
        })
        .collect();
    json!({"parent": parent, "subNotes": sub_notes})
}

fn get_sub_note(call: &EngineCall, response: Value) -> Value {
    let Value::Object(mut map) = response else {
        return response;
    };
    restore_sub_row(&mut map);
    map.insert(
        "parent".into(),
        json!(normalized_parent(text(call, "parent"))),
    );
    Value::Object(map)
}

fn list_attachments(call: &EngineCall, response: Value) -> Value {
    let root = text(call, "id");
    let sub = text(call, "subId");
    let parent = if trim(sub).is_empty() {
        root.to_string()
    } else {
        format!("{root}/{sub}")
    };
    json!({"parent": parent, "attachments": items(response)})
}
