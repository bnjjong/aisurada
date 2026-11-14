//! Java 엔진(단발 모드)과 Rust 엔진이 같은 vault에서 같은 읽기 결과를 내는지 맞댑니다.
//!
//! 사용: `cargo run -p ais-core --example compat_check -- <ais-engine.jar> <vault 폴더>... [--verbose]`
//!
//! Java 응답은 정리안의 새 모양으로 바꿔서 견줍니다: `date`·`time`·`agendaTitle`·`deepLink`를 빼고, 목록은
//! `{items, total, returned, truncated}`로 감싸고, 빈 `tags`를 채우고, `folder`를 더합니다. vault는 읽기만 합니다.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::{env, fs};

use ais_core::engine::Engine;
use serde_json::{Map, Value, json};

struct Java {
    jar: String,
    home: PathBuf,
}

impl Java {
    fn run(&self, vault: &Path, args: &[String]) -> Result<Value, String> {
        let output = Command::new("java")
            .arg(format!("-Duser.home={}", self.home.display()))
            .arg("-jar")
            .arg(&self.jar)
            .args(args)
            .arg(format!("--aisurada.vault={}", vault.display()))
            .output()
            .map_err(|e| format!("java를 못 돌렸다: {e}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|e| format!("Java 응답이 JSON이 아니다: {e}"))
    }
}

/// Java 노트 줄을 새 모양으로 바꾼다. `folder`는 Java 응답에 없어서 디스크를 보고 정한다.
fn note_row(vault: &Path, row: &Value) -> Value {
    let mut map = row.as_object().cloned().unwrap_or_default();
    for key in ["date", "time", "agendaTitle", "deepLink"] {
        map.remove(key);
    }
    map.entry("tags").or_insert_with(|| json!([]));
    let path = map.get("path").and_then(Value::as_str).unwrap_or("");
    map.insert("folder".into(), Value::Bool(vault.join(path).is_dir()));
    Value::Object(map)
}

fn agenda_row(row: &Value) -> Value {
    let mut map = row.as_object().cloned().unwrap_or_default();
    map.remove("deepLink");
    Value::Object(map)
}

fn page(items: Vec<Value>) -> Value {
    let n = items.len();
    json!({"items": items, "total": n, "returned": n, "truncated": false})
}

fn legacy_docs(vault: &Path, java_response: &Value) -> Value {
    page(
        java_response
            .as_array()
            .map(|rows| rows.iter().map(|r| note_row(vault, r)).collect())
            .unwrap_or_default(),
    )
}

/// 프로젝트·템플릿·휴지통: 배열이거나, 일부를 못 읽었을 때의 `{templates|trash: […], incomplete}` 봉투다.
fn legacy_page(_vault: &Path, java_response: &Value) -> Value {
    let items = match java_response {
        Value::Array(rows) => rows.clone(),
        Value::Object(o) => o
            .get("templates")
            .or(o.get("trash"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    page(items)
}

fn legacy_agendas(_vault: &Path, java_response: &Value) -> Value {
    page(
        java_response
            .as_array()
            .map(|rows| rows.iter().map(agenda_row).collect())
            .unwrap_or_default(),
    )
}

#[derive(Default)]
struct Report {
    checked: usize,
    failures: Vec<String>,
}

impl Report {
    fn record(&mut self, label: String, ok: bool, detail: impl FnOnce() -> String) {
        self.checked += 1;
        if !ok {
            self.failures.push(format!("{label}\n{}", detail()));
        }
    }
}

/// 두 JSON의 첫 번째 다른 자리를 찾아 글로 돌려준다.
fn first_diff(a: &Value, b: &Value, path: &str) -> String {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for key in x.keys().chain(y.keys()) {
                match (x.get(key), y.get(key)) {
                    (Some(l), Some(r)) if l != r => {
                        return first_diff(l, r, &format!("{path}.{key}"));
                    }
                    (Some(_), None) => {
                        return format!(
                            "{path}.{key}: Java에만 있다 ({})",
                            clip(&x[key].to_string())
                        );
                    }
                    (None, Some(_)) => {
                        return format!(
                            "{path}.{key}: Rust에만 있다 ({})",
                            clip(&y[key].to_string())
                        );
                    }
                    _ => {}
                }
            }
            String::new()
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return format!("{path}: 길이가 다르다 (Java {}, Rust {})", x.len(), y.len());
            }
            for (i, (l, r)) in x.iter().zip(y).enumerate() {
                if l != r {
                    return first_diff(l, r, &format!("{path}[{i}]"));
                }
            }
            String::new()
        }
        _ => format!(
            "{path}: Java {} / Rust {}",
            clip(&a.to_string()),
            clip(&b.to_string())
        ),
    }
}

fn clip(text: &str) -> String {
    let mut s: String = text.chars().take(160).collect();
    if text.chars().count() > 160 {
        s.push('…');
    }
    s
}

fn compare(
    report: &mut Report,
    vault: &Path,
    label: String,
    java: Result<Value, String>,
    rust: Result<Value, ais_core::error::EngineError>,
    convert: impl Fn(&Path, &Value) -> Value,
) {
    match (java, rust) {
        (Ok(j), Ok(r)) => {
            let want = convert(vault, &j);
            report.record(label, want == r, || first_diff(&want, &r, "$"));
        }
        (Err(j), Err(r)) => {
            // 오류는 Java의 `AIS-코드 메시지` 글과 Rust의 코드·메시지가 같은지 본다
            let rust_text = format!("{} {}", r.code, r.message);
            report.record(label, j == rust_text, || {
                format!("Java 오류: {j}\nRust 오류: {rust_text}")
            });
        }
        (Ok(_), Err(r)) => report.record(label, false, || format!("Java는 성공, Rust는 오류: {r}")),
        (Err(j), Ok(_)) => report.record(label, false, || format!("Java는 오류({j}), Rust는 성공")),
    }
}

/// 후속 노트 줄을 새 모양으로 바꾼다. `ref`·`parent`·`date`·`time`을 빼고 `rootId`·`parentId`·`folder`를 더한다.
fn sub_row(vault: &Path, row: &Value, root: &str, parent: &str) -> Value {
    let mut map = row.as_object().cloned().unwrap_or_default();
    for key in ["date", "time", "ref", "parent"] {
        map.remove(key);
    }
    map.entry("tags").or_insert_with(|| json!([]));
    map.insert("rootId".into(), json!(root));
    map.insert("parentId".into(), json!(parent));
    let path = map.get("path").and_then(Value::as_str).unwrap_or("");
    map.insert("folder".into(), Value::Bool(vault.join(path).is_dir()));
    Value::Object(map)
}

/// `{parent, attachments}` 봉투를 목록으로 바꾼다.
fn legacy_attachments(_vault: &Path, java_response: &Value) -> Value {
    page(
        java_response
            .get("attachments")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
    )
}

fn pairs(java: &Java, vault: &Path, args: &[&str]) -> Result<Value, String> {
    java.run(
        vault,
        &args.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
    )
}

/// 노트 하나를 여는 길의 명령(`get_doc`, 후속 노트, 코멘트, 첨부, 사실층)을 모든 노트와 후속 노트에서 맞댄다.
fn compare_open(
    report: &mut Report,
    java: &Java,
    engine: &Engine,
    vault: &Path,
    root_ids: &[String],
    agenda_ids: &[String],
) {
    for id in root_ids {
        compare(
            report,
            vault,
            format!("get_doc {id}"),
            pairs(java, vault, &["get-doc", id]),
            engine.call("get_doc", json!({"id": id})),
            note_row,
        );
        compare(
            report,
            vault,
            format!("list_comments {id}"),
            pairs(java, vault, &["list-comments", id]),
            engine.call("list_comments", json!({"id": id})),
            |_, v| v.clone(),
        );
        compare(
            report,
            vault,
            format!("list_attachments {id}"),
            pairs(java, vault, &["list-attachments", id]),
            engine.call("list_attachments", json!({"id": id})),
            legacy_attachments,
        );
        walk_subs(report, java, engine, vault, id, id, id);
    }
    for id in agenda_ids {
        compare(
            report,
            vault,
            format!("get_agenda_facts {id}"),
            pairs(java, vault, &["get-agenda-facts", id]),
            engine.call("get_agenda_facts", json!({"id": id})),
            |_, v| v.clone(),
        );
    }

    // 오류: 같은 코드와 같은 글이어야 한다
    let missing = "ndzzzz-00000000";
    let root = root_ids
        .first()
        .cloned()
        .unwrap_or_else(|| missing.to_string());
    let cases: Vec<(String, Vec<String>, &str, Value)> = vec![
        (
            "get_doc 모양 아님".into(),
            vec!["get-doc".into(), "없는-id".into()],
            "get_doc",
            json!({"id": "없는-id"}),
        ),
        (
            "get_doc 없음".into(),
            vec!["get-doc".into(), missing.into()],
            "get_doc",
            json!({"id": missing}),
        ),
        (
            "get_doc 인자 없음".into(),
            vec!["get-doc".into()],
            "get_doc",
            json!({}),
        ),
        (
            "list_sub_notes 모양 아님".into(),
            vec!["list-sub-notes".into(), "bad".into()],
            "list_sub_notes",
            json!({"parent": "bad"}),
        ),
        (
            "list_sub_notes 없는 메인".into(),
            vec!["list-sub-notes".into(), missing.into()],
            "list_sub_notes",
            json!({"parent": missing}),
        ),
        (
            "list_sub_notes 없는 후속".into(),
            vec!["list-sub-notes".into(), format!("{root}/snzzzz-00000000")],
            "list_sub_notes",
            json!({"parent": format!("{root}/snzzzz-00000000")}),
        ),
        (
            "list_sub_notes 후속 모양 아님".into(),
            vec!["list-sub-notes".into(), format!("{root}/bad")],
            "list_sub_notes",
            json!({"parent": format!("{root}/bad")}),
        ),
        (
            "get_sub_note 없음".into(),
            vec![
                "get-sub-note".into(),
                root.clone(),
                "snzzzz-00000000".into(),
            ],
            "get_sub_note",
            json!({"parent": root, "id": "snzzzz-00000000"}),
        ),
        (
            "get_sub_note 모양 아님".into(),
            vec!["get-sub-note".into(), root.clone(), "bad".into()],
            "get_sub_note",
            json!({"parent": root, "id": "bad"}),
        ),
        (
            "list_comments 모양 아님".into(),
            vec!["list-comments".into(), "bad".into()],
            "list_comments",
            json!({"id": "bad"}),
        ),
        (
            "list_comments 없음".into(),
            vec!["list-comments".into(), missing.into()],
            "list_comments",
            json!({"id": missing}),
        ),
        (
            "list_comments 후속 없음".into(),
            vec![
                "list-comments".into(),
                root.clone(),
                "snzzzz-00000000".into(),
            ],
            "list_comments",
            json!({"id": root, "subId": "snzzzz-00000000"}),
        ),
        (
            "list_attachments 없음".into(),
            vec!["list-attachments".into(), missing.into()],
            "list_attachments",
            json!({"id": missing}),
        ),
        (
            "list_attachments 후속 없음".into(),
            vec![
                "list-attachments".into(),
                root.clone(),
                "snzzzz-00000000".into(),
            ],
            "list_attachments",
            json!({"id": root, "subId": "snzzzz-00000000"}),
        ),
        (
            "get_agenda_facts 모양 아님".into(),
            vec!["get-agenda-facts".into(), "x".into()],
            "get_agenda_facts",
            json!({"id": "x"}),
        ),
        (
            "get_agenda_facts 없음".into(),
            vec!["get-agenda-facts".into(), "agzzzz-00000000".into()],
            "get_agenda_facts",
            json!({"id": "agzzzz-00000000"}),
        ),
    ];
    for (label, java_args, command, args) in cases {
        compare(
            report,
            vault,
            label,
            java.run(vault, &java_args),
            engine.call(command, args),
            |_, v| v.clone(),
        );
    }
}

/// `container`(메인 노트이거나 후속 노트) 바로 아래 후속 노트를 사슬(`chain`)로 부르고, 각 후속 노트로 내려간다.
fn walk_subs(
    report: &mut Report,
    java: &Java,
    engine: &Engine,
    vault: &Path,
    root: &str,
    chain: &str,
    container: &str,
) {
    let java_list = pairs(java, vault, &["list-sub-notes", chain, "bodies=true"]);
    let (root_s, container_s) = (root.to_string(), container.to_string());
    let convert = move |v: &Path, j: &Value| {
        page(
            j.get("subNotes")
                .and_then(Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .map(|r| sub_row(v, r, &root_s, &container_s))
                        .collect()
                })
                .unwrap_or_default(),
        )
    };
    compare(
        report,
        vault,
        format!("list_sub_notes {chain}"),
        java_list.clone(),
        engine.call("list_sub_notes", json!({"parent": chain, "bodies": true})),
        &convert,
    );
    if chain != root {
        // 후속 노트 id 하나만 줘도 같은 결과여야 한다
        compare(
            report,
            vault,
            format!("list_sub_notes {container}(id만)"),
            java_list.clone(),
            engine.call(
                "list_sub_notes",
                json!({"parent": container, "bodies": true}),
            ),
            &convert,
        );
    }
    let Ok(list) = &java_list else { return };
    for row in list
        .get("subNotes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let Some(sub) = row.get("id").and_then(Value::as_str) else {
            continue;
        };
        let (root_s, container_s) = (root.to_string(), container.to_string());
        let convert_one = move |v: &Path, j: &Value| sub_row(v, j, &root_s, &container_s);
        let java_sub = pairs(java, vault, &["get-sub-note", chain, sub]);
        compare(
            report,
            vault,
            format!("get_sub_note {chain}/{sub}"),
            java_sub.clone(),
            engine.call("get_sub_note", json!({"parent": chain, "id": sub})),
            &convert_one,
        );
        compare(
            report,
            vault,
            format!("get_sub_note {sub}(id만)"),
            java_sub,
            engine.call("get_sub_note", json!({"id": sub})),
            &convert_one,
        );
        compare(
            report,
            vault,
            format!("list_comments {root}/{sub}"),
            pairs(java, vault, &["list-comments", root, sub]),
            engine.call("list_comments", json!({"id": root, "subId": sub})),
            |_, v| v.clone(),
        );
        compare(
            report,
            vault,
            format!("list_attachments {root}/{sub}"),
            pairs(java, vault, &["list-attachments", root, sub]),
            engine.call("list_attachments", json!({"id": root, "subId": sub})),
            legacy_attachments,
        );
        if row
            .get("path")
            .and_then(Value::as_str)
            .is_some_and(|p| vault.join(p).is_dir())
        {
            walk_subs(
                report,
                java,
                engine,
                vault,
                root,
                &format!("{chain}/{sub}"),
                sub,
            );
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let verbose = args.iter().any(|a| a == "--verbose");
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    if positional.len() < 2 {
        eprintln!("사용법: compat_check <ais-engine.jar> <vault 폴더>... [--verbose]");
        return ExitCode::from(2);
    }
    let home = env::temp_dir().join("ais-compat-home");
    fs::create_dir_all(&home).ok();
    let java = Java {
        jar: positional[0].clone(),
        home,
    };

    let mut total = Report::default();
    for vault in &positional[1..] {
        let vault = Path::new(vault);
        let engine = Engine::new(vault);
        let mut report = Report::default();
        let name = vault
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // 기본 목록: 메타만, 본문까지
        for bodies in [false, true] {
            let mut java_args = vec!["list-docs".to_string(), "limit=0".to_string()];
            if bodies {
                java_args.push("bodies=true".to_string());
            }
            compare(
                &mut report,
                vault,
                format!("list_docs bodies={bodies}"),
                java.run(vault, &java_args),
                engine.call("list_docs", json!({"limit": 0, "bodies": bodies})),
                legacy_docs,
            );
        }
        compare(
            &mut report,
            vault,
            "list_agendas".to_string(),
            java.run(vault, &["list-agendas".to_string()]),
            engine.call("list_agendas", Value::Null),
            legacy_agendas,
        );

        // 프로젝트, 템플릿(본문 유무와 갈래·유형 조건), 휴지통
        compare(
            &mut report,
            vault,
            "list_projects".to_string(),
            java.run(vault, &["list-projects".to_string()]),
            engine.call("list_projects", Value::Null),
            legacy_page,
        );
        compare(
            &mut report,
            vault,
            "list_trash".to_string(),
            java.run(vault, &["list-trash".to_string()]),
            engine.call("list_trash", Value::Null),
            legacy_page,
        );
        let template_cases: [(&str, Vec<&str>, Value); 7] = [
            ("메타만", vec![], json!({})),
            ("bodies", vec!["bodies=true"], json!({"bodies": true})),
            (
                "category=doc",
                vec!["category=doc", "bodies=true"],
                json!({"category": "doc", "bodies": true}),
            ),
            (
                "category=RITUAL",
                vec!["category=RITUAL"],
                json!({"category": "RITUAL"}),
            ),
            ("type=plan", vec!["type=plan"], json!({"type": "plan"})),
            (
                "잘못된 갈래",
                vec!["category=zzz"],
                json!({"category": "zzz"}),
            ),
            ("잘못된 유형", vec!["type=zzz"], json!({"type": "zzz"})),
        ];
        for (label, java_extra, rust_args) in template_cases {
            let mut java_args = vec!["list-templates".to_string()];
            java_args.extend(java_extra.iter().map(|a| a.to_string()));
            compare(
                &mut report,
                vault,
                format!("list_templates {label}"),
                java.run(vault, &java_args),
                engine.call("list_templates", rust_args),
                legacy_page,
            );
        }
        // reindex: 걸린 시간만 다르므로 빼고 견준다
        let without_elapsed = |mut v: Value| {
            if let Some(o) = v.as_object_mut() {
                o.remove("elapsedMs");
            }
            v
        };
        compare(
            &mut report,
            vault,
            "reindex".to_string(),
            java.run(vault, &["reindex".to_string()])
                .map(without_elapsed),
            engine.call("reindex", Value::Null).map(without_elapsed),
            |_, v| v.clone(),
        );

        // 노트 하나를 여는 길: 모든 노트와 후속 노트, 아젠다 사실층
        let root_ids: Vec<String> = java
            .run(vault, &["list-docs".into(), "limit=0".into()])
            .ok()
            .and_then(|v| {
                v.as_array().map(|rows| {
                    rows.iter()
                        .filter_map(|r| r.get("id").and_then(Value::as_str).map(str::to_string))
                        .collect()
                })
            })
            .unwrap_or_default();
        let agenda_ids: Vec<String> = java
            .run(vault, &["list-agendas".into()])
            .ok()
            .and_then(|v| {
                v.as_array().map(|rows| {
                    rows.iter()
                        .filter_map(|r| r.get("id").and_then(Value::as_str).map(str::to_string))
                        .collect()
                })
            })
            .unwrap_or_default();
        compare_open(&mut report, &java, &engine, vault, &root_ids, &agenda_ids);

        // 조건: vault의 실제 값에서 뽑는다
        let docs = java
            .run(vault, &["list-docs".into(), "limit=0".into()])
            .unwrap_or(Value::Null);
        let rows = docs.as_array().cloned().unwrap_or_default();
        let mut cases: Vec<(String, Vec<String>, Value)> = Vec::new();
        let distinct = |key: &str| -> Vec<String> {
            let mut seen: Vec<String> = Vec::new();
            for row in &rows {
                if let Some(v) = row.get(key).and_then(Value::as_str)
                    && !seen.iter().any(|s| s == v)
                {
                    seen.push(v.to_string());
                }
            }
            seen
        };
        for agenda in distinct("agenda") {
            cases.push((
                format!("agenda={agenda}"),
                vec![format!("agenda={agenda}")],
                json!({"agenda": agenda}),
            ));
        }
        for title in distinct("agendaTitle").into_iter().take(3) {
            cases.push((
                format!("agenda(제목)={title}"),
                vec![format!("agenda={}", title.to_uppercase())],
                json!({"agenda": title.to_uppercase()}),
            ));
        }
        for doc_type in distinct("type") {
            cases.push((
                format!("type={doc_type}"),
                vec![format!("type={}", doc_type.to_uppercase())],
                json!({"type": doc_type.to_uppercase()}),
            ));
        }
        let tags: Vec<String> = rows
            .iter()
            .flat_map(|r| {
                r.get("tags")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
            })
            .filter_map(|t| t.as_str().map(str::to_string))
            .collect();
        for tag in tags.iter().take(8) {
            cases.push((
                format!("tag={tag}"),
                vec![format!("tag={tag}")],
                json!({"tags": [tag]}),
            ));
        }
        if tags.len() >= 2 {
            cases.push((
                "tag=둘".to_string(),
                vec![format!("tag={},{}", tags[0], tags[1])],
                json!({"tags": [tags[0], tags[1]]}),
            ));
        }
        cases.push((
            "favorite=true".to_string(),
            vec!["favorite=true".to_string()],
            json!({"favorite": true}),
        ));
        let dates = distinct("date");
        if let (Some(first), Some(last)) = (dates.iter().min(), dates.iter().max()) {
            cases.push((
                format!("date={first}"),
                vec![format!("date={first}")],
                json!({"date": first}),
            ));
            cases.push((
                format!("from={last}"),
                vec![format!("from={last}")],
                json!({"from": last}),
            ));
            cases.push((
                format!("to={first}"),
                vec![format!("to={first}")],
                json!({"to": first}),
            ));
            cases.push((
                "from..to".to_string(),
                vec![format!("from={first}"), format!("to={last}")],
                json!({"from": first, "to": last}),
            ));
            cases.push((
                "뒤집힌 구간".to_string(),
                vec![format!("from={last}"), format!("to={first}")],
                json!({"from": last, "to": first}),
            ));
        }
        // 오류 사례
        cases.push((
            "모르는 유형".to_string(),
            vec!["type=zzz".to_string()],
            json!({"type": "zzz"}),
        ));
        cases.push((
            "없는 아젠다".to_string(),
            vec!["agenda=없는-아젠다".to_string()],
            json!({"agenda": "없는-아젠다"}),
        ));
        cases.push((
            "잘못된 날짜".to_string(),
            vec!["date=2026-13-40".to_string()],
            json!({"date": "2026-13-40"}),
        ));

        for (label, java_filter, rust_filter) in cases {
            let mut java_args = vec!["list-docs".to_string(), "limit=0".to_string()];
            java_args.extend(java_filter);
            let mut rust_args = rust_filter;
            rust_args["limit"] = json!(0);
            compare(
                &mut report,
                vault,
                format!("list_docs {label}"),
                java.run(vault, &java_args),
                engine.call("list_docs", rust_args),
                legacy_docs,
            );
        }

        println!(
            "{name}: {}건 확인, 어긋남 {}건 (노트 {}건)",
            report.checked,
            report.failures.len(),
            rows.len()
        );
        for failure in &report.failures {
            println!(
                "  - {}",
                if verbose {
                    failure.replace('\n', "\n    ")
                } else {
                    failure.lines().next().unwrap_or("").to_string()
                }
            );
        }
        total.checked += report.checked;
        total.failures.extend(report.failures);
    }

    let _ = Map::<String, Value>::new();
    println!(
        "\n합계: {}건 확인, 어긋남 {}건",
        total.checked,
        total.failures.len()
    );
    if total.failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
