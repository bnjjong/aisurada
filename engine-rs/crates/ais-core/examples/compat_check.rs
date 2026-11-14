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

/// `search_docs` 응답 `{query, total, returned, truncated?, hint?, docs}`을 새 목록 모양으로 바꾼다.
fn legacy_search(vault: &Path, java_response: &Value) -> Value {
    let items: Vec<Value> = java_response
        .get("docs")
        .and_then(Value::as_array)
        .map(|rows| rows.iter().map(|r| note_row(vault, r)).collect())
        .unwrap_or_default();
    json!({
        "items": items,
        "total": java_response.get("total").cloned().unwrap_or(json!(0)),
        "returned": items.len(),
        "truncated": java_response.get("truncated") == Some(&Value::Bool(true)),
    })
}

/// `list_tags`: Java는 상한을 건 뒤의 배열만 주므로, 상한 없이 센 전체 개수를 `total`로 넣는다.
fn legacy_tags(total: usize) -> impl Fn(&Path, &Value) -> Value {
    move |_, java_response| {
        let items = java_response.as_array().cloned().unwrap_or_default();
        json!({
            "items": items,
            "total": total,
            "returned": items.len(),
            "truncated": total > items.len(),
        })
    }
}

/// 검색어로 쓸 글에 Java 인자 해석이 건드릴 만한 것(`=`, 맨 앞 `-`)이 없는지
fn is_safe_argument(text: &str) -> bool {
    !text.contains('=') && !text.starts_with('-')
}

/// 검색어 사례: 노트의 제목·아이디·태그·아젠다 이름·본문 낱말에서 뽑고, 모서리 사례를 더한다.
fn search_queries(rows: &[Value]) -> Vec<String> {
    let mut queries: Vec<String> = Vec::new();
    let text = |row: &Value, key: &str| {
        row.get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    };
    for row in rows.iter().take(10) {
        let title = text(row, "title");
        let id = text(row, "id");
        queries.push(title.clone());
        queries.push(title.to_uppercase());
        queries.push(title.to_lowercase());
        if let Some(word) = title.split_whitespace().next() {
            queries.push(word.to_string());
        }
        queries.push(id.clone());
        queries.push(id.to_uppercase());
        queries.push(format!("#doc:{id}"));
        queries.push(format!("#doc:{id}/하위"));
        queries.push(id.chars().take(8).collect());
        queries.push(text(row, "agendaTitle"));
        if let Some(tag) = row
            .get("tags")
            .and_then(Value::as_array)
            .and_then(|t| t.first())
            .and_then(Value::as_str)
        {
            queries.push(tag.to_string());
        }
        let body = text(row, "body");
        let words: Vec<&str> = body.split_whitespace().collect();
        for at in [3, 17, 41, 97] {
            if let Some(word) = words.get(at).filter(|w| w.chars().count() >= 2) {
                queries.push(word.to_string());
            }
        }
    }
    for edge in [
        "한글",
        "  앞뒤 공백  ",
        "\u{FEFF}회의",
        "zzzz없음-none",
        "ß",
        "İ",
        "ǆ",
        "e",
        "의",
        // add-search-notes.sh가 더하는 노트용(진짜 vault에서는 결과가 비어도 같아야 한다)
        "alpha",
        "ALPHA",
        "알파",
        "mixed case",
        "SEARCH",
        "검색태그",
        "mkS1",
        "mkA",
        "hidden word",
        "codeonly",
        "inlinecode",
        "alt text",
        "link label",
        "mkB",
        "Heading Text",
        "quoted words",
        "item two",
        "numbered one",
        "cell1",
        "bold",
        "under",
        "mkC",
        "needleword",
        "needlehead",
        "needleemoji",
        "tiedword",
        "회의 안건",
        "회의",
        "둘째 &",
        "첫째 문서",
        "문서 끝",
        "htmlword",
        "scriptword",
        "jsonword",
        "검색 HTML",
    ] {
        queries.push(edge.to_string());
    }
    let mut seen = std::collections::HashSet::new();
    queries
        .into_iter()
        .filter(|q| !q.trim().is_empty() && is_safe_argument(q) && seen.insert(q.clone()))
        .collect()
}

fn compare_search(
    report: &mut Report,
    java: &Java,
    engine: &Engine,
    vault: &Path,
    rows: &[Value],
    filter_cases: &[(String, Vec<String>, Value)],
) {
    let queries = search_queries(rows);
    for query in &queries {
        compare(
            report,
            vault,
            format!("search_docs {}", clip(query)),
            java.run(vault, &["search-docs".to_string(), query.clone()]),
            engine.call("search_docs", json!({"query": query})),
            legacy_search,
        );
    }
    // 같은 검색어를 AI 쪽으로 불러도 기록 층 노트가 빠지는 것까지 같아야 한다(Java 도구 표면은 별도 경로라 여기선 앱 쪽만 맞댄다)
    let probe = queries.first().cloned().unwrap_or_else(|| "e".to_string());
    for (label, args, limit) in [
        ("limit=1", "limit=1", json!(1)),
        ("limit=2", "limit=2", json!(2)),
        ("limit=200", "limit=200", json!(200)),
        ("limit=0", "limit=0", json!(0)),
        ("limit=201", "limit=201", json!(201)),
        ("limit=-1", "limit=-1", json!(-1)),
    ] {
        for query in [probe.as_str(), "e"] {
            compare(
                report,
                vault,
                format!("search_docs {label} {}", clip(query)),
                java.run(
                    vault,
                    &[
                        "search-docs".to_string(),
                        query.to_string(),
                        args.to_string(),
                    ],
                ),
                engine.call("search_docs", json!({"query": query, "limit": limit})),
                legacy_search,
            );
        }
    }
    for blank in ["", " ", "\t"] {
        compare(
            report,
            vault,
            format!("search_docs 빈 검색어 {blank:?}"),
            java.run(vault, &["search-docs".to_string(), blank.to_string()]),
            engine.call("search_docs", json!({"query": blank})),
            legacy_search,
        );
    }
    // 조건과 함께
    let word = probe.split_whitespace().next().unwrap_or("e").to_string();
    for (label, java_filter, rust_filter) in filter_cases {
        for query in [word.as_str(), "e"] {
            let mut java_args = vec!["search-docs".to_string(), query.to_string()];
            java_args.extend(java_filter.iter().cloned());
            let mut rust_args = rust_filter.clone();
            rust_args["query"] = json!(query);
            compare(
                report,
                vault,
                format!("search_docs {label} {}", clip(query)),
                java.run(vault, &java_args),
                engine.call("search_docs", rust_args),
                legacy_search,
            );
        }
    }
}

fn compare_tags(report: &mut Report, java: &Java, engine: &Engine, vault: &Path, rows: &[Value]) {
    let mut tags: Vec<String> = Vec::new();
    for row in rows {
        for tag in row
            .get("tags")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            if let Some(tag) = tag.as_str()
                && is_safe_argument(tag)
                && !tags.iter().any(|t| t == tag)
            {
                tags.push(tag.to_string());
            }
        }
    }
    // (이름, Java 인자, Rust 인자, 상한 없이 센 Java 인자)
    let mut cases: Vec<(String, Vec<String>, Value, Vec<String>)> = vec![
        ("전부".into(), vec![], json!({}), vec![]),
        (
            "없는 태그".into(),
            vec!["query=zzz-none".into()],
            json!({"query": "zzz-none"}),
            vec!["query=zzz-none".into()],
        ),
        (
            "빈 query".into(),
            vec!["query=  ".into()],
            json!({"query": "  "}),
            vec![],
        ),
    ];
    for tag in tags.iter().take(3) {
        let q = tag.to_uppercase();
        cases.push((
            format!("query={tag}"),
            vec![format!("query={q}")],
            json!({"query": q}),
            vec![format!("query={q}")],
        ));
        let head: String = tag.chars().take(2).collect();
        cases.push((
            format!("query(앞)={head}"),
            vec![format!("query={head}")],
            json!({"query": head}),
            vec![format!("query={head}")],
        ));
    }
    for limit in [1, 2, 5] {
        cases.push((
            format!("limit={limit}"),
            vec![format!("limit={limit}")],
            json!({"limit": limit}),
            vec![],
        ));
    }
    if let Some(tag) = tags.first() {
        cases.push((
            format!("query={tag} limit=1"),
            vec![format!("query={tag}"), "limit=1".into()],
            json!({"query": tag, "limit": 1}),
            vec![format!("query={tag}")],
        ));
    }
    for (label, java_args, rust_args, count_args) in cases {
        let mut full = vec!["list-tags".to_string()];
        full.extend(count_args);
        let total = java
            .run(vault, &full)
            .ok()
            .and_then(|v| v.as_array().map(Vec::len))
            .unwrap_or(0);
        let mut args = vec!["list-tags".to_string()];
        args.extend(java_args);
        compare(
            report,
            vault,
            format!("list_tags {label}"),
            java.run(vault, &args),
            engine.call("list_tags", rust_args),
            legacy_tags(total),
        );
    }
    for limit in [0, -3] {
        compare(
            report,
            vault,
            format!("list_tags limit={limit}"),
            java.run(vault, &["list-tags".to_string(), format!("limit={limit}")]),
            engine.call("list_tags", json!({"limit": limit})),
            legacy_tags(0),
        );
    }
}

fn compare_get_template(report: &mut Report, java: &Java, engine: &Engine, vault: &Path) {
    let ids: Vec<String> = java
        .run(vault, &["list-templates".to_string()])
        .ok()
        .and_then(|v| {
            legacy_page(vault, &v)
                .get("items")
                .and_then(Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .filter_map(|r| r.get("id").and_then(Value::as_str).map(str::to_string))
                        .collect()
                })
        })
        .unwrap_or_default();
    let mut all = ids;
    for extra in ["zzz-none", "a b", "../x", "a/b", "", "  "] {
        all.push(extra.to_string());
    }
    for id in all {
        compare(
            report,
            vault,
            format!("get_template {id:?}"),
            java.run(vault, &["get-template".to_string(), id.clone()]),
            engine.call("get_template", json!({"id": id})),
            |_, v| v.clone(),
        );
    }
}

/// `history`: Java와 응답 모양이 같다. 저장소가 없는 vault에서는 판이 없는 응답과 오류만 맞댄다.
fn compare_history(
    report: &mut Report,
    java: &Java,
    engine: &Engine,
    vault: &Path,
    root_ids: &[String],
) {
    let mut ids: Vec<String> = root_ids.to_vec();
    // 지금은 없는 노트(커밋 꼬리 줄로만 찾는다), 없는 id, 안전하지 않은 id
    for extra in [
        "ndfx0040-00000040",
        "ndfx0041-00000041",
        "ndzz9999-99999999",
        "a b",
        "../x",
    ] {
        ids.push(extra.to_string());
    }
    let identity = |_: &Path, v: &Value| v.clone();
    for id in &ids {
        let base = engine.call("history", json!({"id": id}));
        compare(
            report,
            vault,
            format!("history {id}"),
            java.run(vault, &["history".to_string(), id.clone()]),
            base.clone(),
            identity,
        );
        let commits: Vec<String> = base
            .as_ref()
            .ok()
            .and_then(|v| v.get("versions"))
            .and_then(Value::as_array)
            .map(|rows| {
                rows.iter()
                    .filter_map(|r| r.get("commit").and_then(Value::as_str).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if commits.is_empty() {
            continue;
        }
        // 번호, 커밋, 글자 찾기(대소문자 무시), 범위를 넘는 번호
        for number in [1, 2, commits.len(), commits.len() + 1] {
            compare(
                report,
                vault,
                format!("history {id} 번호={number}"),
                java.run(
                    vault,
                    &["history".to_string(), id.clone(), number.to_string()],
                ),
                engine.call("history", json!({"id": id, "number": number})),
                identity,
            );
        }
        for commit in commits.iter().take(3) {
            compare(
                report,
                vault,
                format!("history {id} commit={}", &commit[..7]),
                java.run(
                    vault,
                    &[
                        "history".to_string(),
                        id.clone(),
                        format!("commit={commit}"),
                    ],
                ),
                engine.call("history", json!({"id": id, "commit": commit})),
                identity,
            );
        }
        for query in [
            "keyword",
            "한글 검색어",
            "수정",
            "zzz-none",
            "  유령  ",
            "본문",
        ] {
            compare(
                report,
                vault,
                format!("history {id} query={query:?}"),
                java.run(
                    vault,
                    &["history".to_string(), id.clone(), format!("query={query}")],
                ),
                engine.call("history", json!({"id": id, "query": query})),
                identity,
            );
        }
        compare(
            report,
            vault,
            format!("history {id} 번호+글자"),
            java.run(
                vault,
                &[
                    "history".to_string(),
                    id.clone(),
                    "2".to_string(),
                    "query=수정".to_string(),
                ],
            ),
            engine.call("history", json!({"id": id, "number": 2, "query": "수정"})),
            identity,
        );
    }
    // 오류 사례(어느 vault에서나)
    let id = ids
        .first()
        .cloned()
        .unwrap_or_else(|| "ndfx0001-00000001".to_string());
    let missing_commit = "0".repeat(40);
    let upper_commit = "A".repeat(40);
    let cases: Vec<(String, Vec<String>, Value)> = vec![
        (
            "번호 0".into(),
            vec![id.clone(), "0".into()],
            json!({"id": id, "number": 0}),
        ),
        (
            "번호 -1".into(),
            vec![id.clone(), "-1".into()],
            json!({"id": id, "number": -1}),
        ),
        (
            "번호 큼".into(),
            vec![id.clone(), "99999999999".into()],
            json!({"id": id, "number": 99_999_999_999_i64}),
        ),
        (
            "짧은 커밋".into(),
            vec![id.clone(), "commit=abc".into()],
            json!({"id": id, "commit": "abc"}),
        ),
        (
            "대문자 커밋".into(),
            vec![id.clone(), format!("commit={upper_commit}")],
            json!({"id": id, "commit": upper_commit}),
        ),
        (
            "목록에 없는 커밋".into(),
            vec![id.clone(), format!("commit={missing_commit}")],
            json!({"id": id, "commit": missing_commit}),
        ),
        (
            "번호와 커밋".into(),
            vec![id.clone(), "1".into(), format!("commit={missing_commit}")],
            json!({"id": id, "number": 1, "commit": missing_commit}),
        ),
        (
            "번호 1".into(),
            vec![id.clone(), "1".into()],
            json!({"id": id, "number": 1}),
        ),
        ("빈 id".into(), vec![String::new()], json!({"id": ""})),
        ("공백 id".into(), vec!["  ".into()], json!({"id": "  "})),
        ("id 없음".into(), vec![], json!({})),
    ];
    for (label, java_args, rust_args) in cases {
        let mut args = vec!["history".to_string()];
        args.extend(java_args);
        compare(
            report,
            vault,
            format!("history {label}"),
            java.run(vault, &args),
            engine.call("history", rust_args),
            identity,
        );
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

        for (label, java_filter, rust_filter) in &cases {
            let mut java_args = vec!["list-docs".to_string(), "limit=0".to_string()];
            java_args.extend(java_filter.iter().cloned());
            let mut rust_args = rust_filter.clone();
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

        // 검색·태그·템플릿 하나 읽기. 검색어는 본문까지 있는 줄에서 뽑는다
        let rows_with_bodies = java
            .run(
                vault,
                &["list-docs".into(), "limit=0".into(), "bodies=true".into()],
            )
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        compare_search(
            &mut report,
            &java,
            &engine,
            vault,
            &rows_with_bodies,
            &cases,
        );
        compare_tags(&mut report, &java, &engine, vault, &rows);
        compare_get_template(&mut report, &java, &engine, vault);
        compare_history(&mut report, &java, &engine, vault, &root_ids);

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
