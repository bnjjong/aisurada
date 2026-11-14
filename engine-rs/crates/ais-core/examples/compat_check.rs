//! Java 엔진(단발 모드)과 Rust 엔진이 같은 vault에서 같은 읽기 결과를 내는지 맞댑니다.
//!
//! 사용: `cargo run -p ais-core --example compat_check -- <ais-engine.jar> <vault 폴더>... [--verbose] [--commit-fixture=<git 이력이 있는 합성 vault>] [--write-fixture=<합성 vault>]`
//! `--commit-fixture`는 그 vault를 사본 둘로 복사해 `commit_vault`(vault를 고친다)를 견준다. 진짜 vault에는 주지 않는다.
//! `--write-fixture`는 그 vault를 사본 둘로 복사해 쓰기 명령을 Java와 Rust로 돌리고 응답과 폴더 전체 바이트를 견준다.
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
        self.run_with(vault, args, &[], &[])
    }

    /// JVM 속성(`-Dkey=value`)과 환경 변수를 더해서 돌린다. 쓰기 맞대기가 시계·id·잠금 폴더를 고정할 때 쓴다.
    fn run_with(
        &self,
        vault: &Path,
        args: &[String],
        props: &[String],
        envs: &[(String, String)],
    ) -> Result<Value, String> {
        let output = Command::new("java")
            .arg(format!("-Duser.home={}", self.home.display()))
            .args(props)
            .arg("-jar")
            .arg(&self.jar)
            .args(args)
            .arg(format!("--aisurada.vault={}", vault.display()))
            .envs(envs.iter().map(|(k, v)| (k, v)))
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
            // Java가 예상 밖 예외로 실패하면 오류 글에 스택 추적이 붙으므로, Rust의 메시지가 그 안에 든 것도 같은 실패로 본다
            report.record(
                label,
                j == rust_text
                    || (j.starts_with("Application run failed") && j.contains(&r.message)),
                || format!("Java 오류: {j}\nRust 오류: {rust_text}"),
            );
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

/// `commit_vault`: vault를 고치므로 지정한 합성 vault(git 이력 포함)의 사본 둘에서만 돌린다.
/// 같은 변경을 두 사본에 만들고 Java(단발)와 Rust가 각각 커밋하게 해서 응답, 커밋 메시지, 작성자, 담긴 파일과 트리를 견준다.
fn compare_commit_vault(
    report: &mut Report,
    java: &Java,
    engine_for: &dyn Fn(&Path) -> Engine,
    fixture: &Path,
) {
    let root = env::temp_dir().join(format!("ais-commit-compat-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("임시 폴더");
    let git = |vault: &Path, args: &[&str]| -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(vault)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .output()
            .expect("git");
        String::from_utf8_lossy(&out.stdout).trim_end().to_string()
    };
    let copy = |name: &str| -> PathBuf {
        let dst = root.join(name);
        let status = Command::new("cp")
            .arg("-R")
            .arg(fixture)
            .arg(&dst)
            .status()
            .expect("cp");
        assert!(status.success(), "사본을 못 만들었다");
        dst
    };
    let observe = |vault: &Path| -> Value {
        json!({
            "message": git(vault, &["log", "-1", "--format=%B"]),
            "author": git(vault, &["log", "-1", "--format=%an <%ae> / %cn <%ce>"]),
            "tree": git(vault, &["rev-parse", "HEAD^{tree}"]),
            "files": git(vault, &["show", "--name-status", "--format=", "HEAD"]),
            "count": git(vault, &["rev-list", "--count", "HEAD"]),
        })
    };

    type Scenario<'a> = (&'a str, Box<dyn Fn(&Path) + 'a>);
    let scenarios: Vec<Scenario> = vec![
        ("변경 없음", Box::new(|_| {})),
        (
            "노트 고침·더함·지움과 노트 아닌 파일, .tmp",
            Box::new(|v| {
                let note = v.join("notes/demo/2026-11/first/note.md");
                let text = fs::read_to_string(&note).unwrap();
                fs::write(&note, format!("{text}\n덧붙임\n")).unwrap();
                fs::create_dir_all(v.join("notes/demo/newone")).unwrap();
                fs::write(
                v.join("notes/demo/newone/note.md"),
                "---\nid: ndzz0001-0000aaaa\nagenda: demo\ntype: free\ntitle: 새\ncreated: '2026-11-12T09:00:00+09:00'\n---\n\n새\n",
            )
            .unwrap();
                fs::remove_dir_all(v.join("notes/demo/2026-11/parent/sub")).unwrap();
                fs::write(v.join("notes/readme.txt"), "x").unwrap();
                fs::write(v.join("notes/skip.md.tmp"), "임시").unwrap();
            }),
        ),
        (
            "main이 아닌 가지",
            Box::new(|v| {
                git(v, &["switch", "-q", "-c", "other"]);
                fs::write(v.join("notes/readme.txt"), "x").unwrap();
            }),
        ),
        (
            "새 index.lock",
            Box::new(|v| {
                fs::write(v.join("notes/readme.txt"), "x").unwrap();
                fs::write(v.join(".git/index.lock"), "").unwrap();
            }),
        ),
        (
            "병합 중",
            Box::new(|v| {
                fs::write(v.join("notes/readme.txt"), "x").unwrap();
                fs::write(v.join(".git/MERGE_HEAD"), "").unwrap();
            }),
        ),
    ];
    for (label, mutate) in scenarios {
        let (a, b) = (copy("java"), copy("rust"));
        for vault in [&a, &b] {
            mutate(vault);
        }
        let java_out = java.run(&a, &["commit-vault".to_string()]);
        let rust_out = engine_for(&b).call("commit_vault", Value::Null);
        compare(
            report,
            &a,
            format!("commit_vault {label} 응답"),
            java_out,
            rust_out,
            |_, v| v.clone(),
        );
        report.record(
            format!("commit_vault {label} 결과물"),
            observe(&a) == observe(&b),
            || first_diff(&observe(&a), &observe(&b), "$"),
        );
        let _ = fs::remove_dir_all(&a);
        let _ = fs::remove_dir_all(&b);
    }
    // 저장소가 아닌 폴더
    let plain = root.join("plain");
    fs::create_dir_all(plain.join(".aisurada")).unwrap();
    compare(
        report,
        &plain,
        "commit_vault 저장소 아님".to_string(),
        java.run(&plain, &["commit-vault".to_string()]),
        engine_for(&plain).call("commit_vault", Value::Null),
        |_, v| v.clone(),
    );
    let _ = fs::remove_dir_all(&root);
}

/// 폴더 안의 파일을 (상대 경로 → 내용)으로 모은다. `.git`은 빼고, 심볼릭 링크는 가리키는 곳을 적는다.
fn tree(root: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut std::collections::BTreeMap<String, Vec<u8>>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .to_string();
            if rel == ".git" {
                continue;
            }
            let meta = fs::symlink_metadata(&path).unwrap();
            if meta.file_type().is_symlink() {
                let target = fs::read_link(&path)
                    .map(|t| t.to_string_lossy().to_string())
                    .unwrap_or_default();
                out.insert(rel, format!("-> {target}").into_bytes());
            } else if meta.is_dir() {
                out.insert(format!("{rel}/"), Vec::new());
                walk(root, &path, out);
            } else {
                out.insert(rel, fs::read(&path).unwrap_or_default());
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(root, root, &mut out);
    out
}

/// 두 폴더가 같은지 본다. 다르면 앞의 몇 곳을 글로 돌려준다.
fn tree_diff(a: &Path, b: &Path) -> String {
    let (left, right) = (tree(a), tree(b));
    let mut diffs = Vec::new();
    for (path, bytes) in &left {
        match right.get(path) {
            None => diffs.push(format!("Java에만 있다: {path}")),
            Some(other) if other != bytes => diffs.push(format!(
                "내용이 다르다: {path}\n      Java: {}\n      Rust: {}",
                clip(&String::from_utf8_lossy(bytes)),
                clip(&String::from_utf8_lossy(other))
            )),
            _ => {}
        }
    }
    for path in right.keys().filter(|p| !left.contains_key(*p)) {
        diffs.push(format!("Rust에만 있다: {path}"));
    }
    diffs.truncate(5);
    diffs.join("\n    ")
}

/// 쓰기 맞대기: 합성 vault의 사본 둘에 같은 명령열을 Java(단발)와 Rust로 돌려, 명령마다 응답과 **폴더 전체의 바이트**를 견준다.
/// 시계와 id의 난수는 두 엔진에 같은 값으로 고정한다(명령마다 난수 목록이 다시 시작하므로 명령 번호로 값을 달리 준다).
struct WriteHarness<'a> {
    report: &'a mut Report,
    java: &'a Java,
    a: PathBuf,
    b: PathBuf,
    locks: PathBuf,
    step: u32,
    /// Java가 성공으로 답한 단계 수(양쪽이 똑같이 실패해서 맞아 보이는 일을 가려내는 데 쓴다)
    succeeded: u32,
}

const WRITE_NOW: &str = "1794700000000";

impl WriteHarness<'_> {
    /// 노트의 지금 확인 값(`f1`). 앞 단계에서 두 사본이 같아졌으므로 Rust 사본에서 읽는다.
    fn token(&self, id: &str) -> String {
        Engine::new(&self.b)
            .call("get_doc", json!({"id": id}))
            .ok()
            .and_then(|v| v["precondition"].as_str().map(str::to_string))
            .unwrap_or_default()
    }

    /// 노트 머리 쓰기 한 단계. `named`는 Java의 `key=value` 인자이고, `rust`는 같은 뜻의 호출 객체다.
    /// `token_of`가 있으면 그 노트의 지금 확인 값을 `f1`로 더한다.
    fn note_step(
        &mut self,
        label: &str,
        command: &str,
        positional: &[&str],
        named: &[(&str, &str)],
        rust: Value,
        token_of: Option<&str>,
    ) {
        let java_name = command.replace('_', "-");
        let mut java_args: Vec<String> = vec![java_name];
        java_args.extend(positional.iter().map(|p| p.to_string()));
        java_args.extend(named.iter().map(|(k, v)| format!("{k}={v}")));
        let mut args = rust;
        if let Some(id) = token_of {
            let token = self.token(id);
            java_args.push(format!("f1={token}"));
            args["f1"] = json!(token);
        }
        let refs: Vec<&str> = java_args.iter().map(String::as_str).collect();
        self.step(label, &refs, command, args);
    }

    /// 명령 하나를 두 사본에 돌리고 견준다. Java가 성공했으면 그 응답을 돌려준다.
    fn step(
        &mut self,
        label: &str,
        java_args: &[&str],
        command: &str,
        args: Value,
    ) -> Option<Value> {
        self.step += 1;
        let n = self.step;
        let random = format!("{n:04x}0001,{n:04x}0002,{n:04x}0003,{n:04x}0004");
        let java_args: Vec<String> = java_args.iter().map(|a| a.to_string()).collect();
        let java_out = self.java.run_with(
            &self.a,
            &java_args,
            &[
                format!("-Daisurada.test.now={WRITE_NOW}"),
                format!("-Daisurada.test.random={random}"),
                "-Daisurada.vault.autocommit.enabled=false".to_string(),
            ],
            &[(
                "AISURADA_LOCK_DIR".to_string(),
                self.locks.display().to_string(),
            )],
        );
        // SAFETY: 이 도구는 단일 스레드로 돌고, 환경 변수는 Rust 명령을 부르기 직전에만 바꾼다.
        unsafe {
            env::set_var("AISURADA_TEST_RANDOM", &random);
        }
        ais_core::write::apparatus::reset_random_queue();
        let rust_out = Engine::new(&self.b).call(command, args);
        if env::var_os("HARNESS_TRACE").is_some() {
            match &java_out {
                Ok(_) => eprintln!("TRACE ok  {command} {label}"),
                Err(e) => eprintln!(
                    "TRACE err {command} {label} :: {}",
                    clip(e.lines().next().unwrap_or(""))
                ),
            }
        }
        let ok_value = java_out.as_ref().ok().cloned();
        if ok_value.is_some() {
            self.succeeded += 1;
        }
        let vault = self.a.clone();
        // 두 사본의 폴더 경로가 달라서 딥링크의 `ws=` 값만 다르다. Java 쪽을 Rust 사본 경로로 바꿔 견준다.
        let (from, to) = (
            ais_core::text::encode_uri_component(&self.a.to_string_lossy()),
            ais_core::text::encode_uri_component(&self.b.to_string_lossy()),
        );
        compare(
            self.report,
            &vault,
            format!("{command} {label} 응답"),
            java_out,
            rust_out,
            move |_, v| {
                let mut v = v.clone();
                if let Some(link) = v
                    .get("deepLink")
                    .and_then(Value::as_str)
                    .map(|l| l.replace(&from, &to))
                {
                    v["deepLink"] = Value::String(link);
                }
                v
            },
        );
        let diff = tree_diff(&self.a, &self.b);
        self.report
            .record(format!("{command} {label} 폴더"), diff.is_empty(), || {
                diff.clone()
            });
        ok_value
    }
}

/// 두 사본에 같은 파일을 둔다.
fn put_both(h: &WriteHarness, rel: &str, content: &str) {
    for root in [&h.a, &h.b] {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
}

/// 파일 바이트에서 바로 계산한 확인 값(`get_doc`이 값을 안 주는 확장자 폴더 노트용)
fn file_token(h: &WriteHarness, rel: &str) -> String {
    ais_core::vault::note_file::compute_token(&fs::read(h.b.join(rel)).unwrap())
}

/// `update_doc` 한 단계. `named`는 Java의 `key=value`이고, 호출 객체는 같은 글을 그대로 담는다(`tags`도 명령줄 글 꼴).
/// `rust`가 있으면 그것을 대신 쓴다(`tags`를 배열로 주는 경우).
fn update_step(
    h: &mut WriteHarness,
    label: &str,
    id: &str,
    named: &[(&str, &str)],
    rust: Option<Value>,
    token: bool,
) {
    let mut args = rust.unwrap_or_else(|| {
        let mut map = Map::new();
        for (key, value) in named {
            map.insert(key.to_string(), json!(value));
        }
        Value::Object(map)
    });
    args["id"] = json!(id);
    h.note_step(
        &format!("본문 {label}"),
        "update_doc",
        &[id],
        named,
        args,
        token.then_some(id),
    );
}

/// `update_doc` 맞대기: 값 하나씩 고치기, 태그·날짜·시각 검사, 노트 종류별 자리 옮기기, 빈자리 고르기, 월별 배치 스위치, 저장 상한.
fn compare_doc_updates(h: &mut WriteHarness) {
    let parent = "ndfx0002-00000002";
    let loose = "ndfx0003-00000003";
    let single = "ndfx0004-00000004";
    let html = "ndfx0005-00000005";
    let json_note = "ndfx0006-00000006";
    let ext = "ndfx0007-00000007";
    let record = "ndfx0008-00000008";
    let odd = "ndfx0009-00000009";
    let old = "ndfx000a-0000000a";

    // ---- 폴더 노트 하나로 값마다 ----
    update_step(
        h,
        "제목만",
        loose,
        &[("title", "새 제목: 콜론 / 슬래시")],
        None,
        true,
    );
    update_step(
        h,
        "본문만(헤딩이 제목과 다름)",
        loose,
        &[("body", "# 다른 헤딩\n\n내용\n")],
        None,
        true,
    );
    update_step(
        h,
        "바뀐 것 없음",
        loose,
        &[
            ("title", "새 제목: 콜론 / 슬래시"),
            ("body", "# 다른 헤딩\n\n내용\n"),
        ],
        None,
        true,
    );
    update_step(h, "인자 없이 확인 값만", loose, &[], None, true);
    update_step(h, "본문 비움", loose, &[("body", "")], None, true);
    update_step(
        h,
        "CRLF 본문",
        loose,
        &[("body", "가\r\n나\r\n")],
        None,
        true,
    );
    update_step(
        h,
        "깨진 글자가 늘어남",
        loose,
        &[("body", "깨진\u{FFFD}글자\n\u{FFFD}\n")],
        None,
        true,
    );
    update_step(
        h,
        "깨진 글자가 그대로",
        loose,
        &[("body", "깨진\u{FFFD}글자\n\u{FFFD}\n새 줄\n")],
        None,
        true,
    );
    update_step(
        h,
        "들여쓴 블록",
        loose,
        &[("body", "  ## 들여쓴\n본문\n\t> 인용\n")],
        None,
        true,
    );
    update_step(
        h,
        "헤딩이 제목과 같음",
        loose,
        &[("body", "# 새 제목: 콜론 / 슬래시\n\n본문\n")],
        None,
        true,
    );
    update_step(
        h,
        "제목 양끝 공백이 그대로 들어감",
        loose,
        &[("title", "  공백 제목  ")],
        None,
        true,
    );

    // ---- 태그 ----
    update_step(
        h,
        "tags 교체",
        loose,
        &[("tags", r#"["b","a"]"#)],
        None,
        true,
    );
    update_step(
        h,
        "tags 배열로 교체",
        loose,
        &[("tags", r#"["x","y"]"#)],
        Some(json!({"tags": ["x", "y"]})),
        true,
    );
    update_step(
        h,
        "tags 정리",
        loose,
        &[("tags", r#"[" a ","a",""," ","b","a "]"#)],
        None,
        true,
    );
    update_step(
        h,
        "tags 배열 정리",
        loose,
        &[("tags", r#"[" a ","a",""," ","b","a "]"#)],
        Some(json!({"tags": [" a ", "a", "", " ", "b", "a "]})),
        true,
    );
    update_step(h, "tags 비움", loose, &[("tags", "[]")], None, true);
    update_step(
        h,
        "tags 줄바꿈",
        loose,
        &[("tags", r#"["a\nb"]"#)],
        None,
        true,
    );
    update_step(
        h,
        "tags 줄바꿈(배열)",
        loose,
        &[("tags", r#"["a\nb"]"#)],
        Some(json!({"tags": ["a\nb"]})),
        true,
    );
    update_step(h, "tags 객체", loose, &[("tags", r#"{"a":1}"#)], None, true);
    update_step(h, "tags 글", loose, &[("tags", r#""x""#)], None, true);
    update_step(h, "tags 문법 오류", loose, &[("tags", "[1,")], None, true);
    update_step(h, "tags 항목 숫자", loose, &[("tags", "[1]")], None, true);
    update_step(
        h,
        "tags 항목 객체",
        loose,
        &[("tags", r#"[{"a":1.0e3}]"#)],
        None,
        true,
    );
    update_step(
        h,
        "tags 특수 글자",
        loose,
        &[("tags", r##"["# 해시","콜론: x","'따옴표'","\"큰\"","😀"]"##)],
        None,
        true,
    );

    // ---- 템플릿 ----
    update_step(
        h,
        "템플릿",
        loose,
        &[("template", "my-tpl_1.v2")],
        None,
        true,
    );
    update_step(
        h,
        "템플릿 모양이 틀림(버림)",
        loose,
        &[("template", "bad tpl!")],
        None,
        true,
    );
    update_step(
        h,
        "템플릿 공백을 다듬음",
        loose,
        &[("template", "  spaced  ")],
        None,
        true,
    );
    update_step(
        h,
        "템플릿 빈 글(버림)",
        loose,
        &[("template", "")],
        None,
        true,
    );

    // ---- 날짜와 시각 ----
    update_step(h, "날짜만", loose, &[("date", "2026-12-25")], None, true);
    update_step(h, "시각만", loose, &[("time", "23:59")], None, true);
    update_step(
        h,
        "날짜와 시각과 제목",
        loose,
        &[
            ("date", "2027-01-02"),
            ("time", "03:04"),
            ("title", "새해 제목"),
        ],
        None,
        true,
    );
    for (label, date) in [
        ("달력에 없음", "2026-02-30"),
        ("모양", "26-1-1"),
        ("월", "2026-13-01"),
        ("0000년 앞 부호", "-0000-01-01"),
        ("한 자리 달", "2026-1-01"),
        ("윤일 아님", "2026-02-29"),
        ("윤일", "2028-02-29"),
        ("연도 한도 밖", "+1000000000-01-01"),
        ("연도 int 밖", "+9999999999-01-01"),
    ] {
        update_step(
            h,
            &format!("날짜 {label}"),
            loose,
            &[("date", date)],
            None,
            true,
        );
    }
    for (label, time) in [
        ("24시", "24:00"),
        ("한 자리 시", "9:05"),
        ("60분", "12:60"),
        ("콜론 없음", "1200"),
        ("0시", "00:00"),
    ] {
        update_step(
            h,
            &format!("시각 {label}"),
            loose,
            &[("time", time)],
            None,
            true,
        );
    }

    // ---- 이상한 연도: 쓴 뒤 읽을 수 없게 되는 노트가 있어서 버릴 노트에서 한다 ----
    for (n, years) in [
        (
            0xb0,
            vec![
                ("0년", "-00000-03-04"),
                ("기원전 1년", "-0001-03-04"),
                ("0000년", "0000-01-01"),
            ],
        ),
        (0xb1, vec![("다섯 자리 연도", "+12345-03-04")]),
        (0xb2, vec![("연도 한도 안", "+999999999-01-01")]),
    ] {
        let id = format!("ndfx00{n:02x}-000000{n:02x}");
        put_both(
            h,
            &format!("notes/work/year{n:02x}/note.md"),
            &format!(
                "---\nid: {id}\nagenda: work\ntype: free\ntitle: 연도 시험\ncreated: '2026-11-10T09:00:00+09:00'\n---\n본문\n"
            ),
        );
        for (label, date) in years {
            update_step(
                h,
                &format!("날짜 {label}"),
                &id,
                &[("date", date)],
                None,
                true,
            );
        }
    }

    // ---- 입력 오류 ----
    update_step(h, "제목 공백", loose, &[("title", "   ")], None, true);
    update_step(h, "제목 빈 글", loose, &[("title", "")], None, true);
    update_step(h, "확인 값 없음", loose, &[("title", "x")], None, false);
    update_step(
        h,
        "확인 값 모양",
        loose,
        &[("title", "x"), ("f1", "abc")],
        None,
        false,
    );
    update_step(
        h,
        "확인 값 틀림",
        loose,
        &[("title", "x"), ("f1", "f1:deadbeef")],
        None,
        false,
    );
    h.note_step("id 없음", "update_doc", &[], &[], json!({}), None);
    h.note_step(
        "id 공백",
        "update_doc",
        &[" "],
        &[("title", "x")],
        json!({"id": " ", "title": "x"}),
        None,
    );
    update_step(h, "id 모양", "nope", &[("title", "x")], None, false);
    update_step(
        h,
        "없는 노트",
        "ndzz0001-00000001",
        &[("title", "x")],
        None,
        false,
    );

    // ---- 노트 종류별 ----
    update_step(
        h,
        "폴더 노트 아래 후속 노트가 있는 상위",
        parent,
        &[("date", "2026-11-20"), ("title", "상위 새 제목")],
        None,
        true,
    );
    update_step(
        h,
        "상위 본문만",
        parent,
        &[("body", "# 상위 새 제목\n\n본문\n")],
        None,
        true,
    );
    update_step(
        h,
        "단일 .md 제목(빈 제목이던 노트)",
        single,
        &[("title", "단일 파일 제목")],
        None,
        true,
    );
    update_step(
        h,
        "단일 .md 본문만",
        odd,
        &[("body", "다른 본문\n")],
        None,
        true,
    );
    update_step(h, "단일 .md 시각만", odd, &[("time", "07:07")], None, true);
    update_step(
        h,
        "단일 .html 제목",
        html,
        &[("title", "HTML 새 제목")],
        None,
        true,
    );
    update_step(
        h,
        "단일 .html 본문(들여쓴 블록이어도 경고 없음)",
        html,
        &[("body", "  ## <h2>x</h2>\n<p>본문</p>\n")],
        None,
        true,
    );
    update_step(
        h,
        "단일 .html 날짜",
        html,
        &[("date", "2026-10-01")],
        None,
        true,
    );
    update_step(
        h,
        "JSON 노트 본문",
        json_note,
        &[("body", "x")],
        None,
        false,
    );
    update_step(
        h,
        "JSON 노트 제목",
        json_note,
        &[("title", "JSON 새 제목")],
        None,
        false,
    );
    update_step(
        h,
        "JSON 노트 태그",
        json_note,
        &[("tags", r#"["j"]"#)],
        None,
        false,
    );
    let ext_token = file_token(h, "notes/work/ext.md/note.md");
    update_step(
        h,
        "확장자 폴더 확인 값 없음",
        ext,
        &[("title", "확장자 새 제목")],
        None,
        false,
    );
    update_step(
        h,
        "확장자 폴더 제목",
        ext,
        &[("title", "확장자 새 제목"), ("f1", &ext_token)],
        None,
        false,
    );
    update_step(
        h,
        "기록 층 제목",
        record,
        &[("title", "기록 새 제목"), ("tags", r#"["기록","새"]"#)],
        None,
        true,
    );
    update_step(
        h,
        "오프셋이 -12:00인 노트 시각만",
        old,
        &[("time", "10:30")],
        None,
        true,
    );
    update_step(
        h,
        "오프셋이 -12:00인 노트 날짜만",
        old,
        &[("date", "2026-01-02")],
        None,
        true,
    );

    // ---- 빈자리 고르기: 정식 자리가 이미 차 있으면 -2, -3 ----
    let canonical =
        |h: &WriteHarness, date: &str, hhmm: &str, title: &str, tail: &str, month: &str| {
            let _ = h;
            format!(
                "notes/work/{month}/{date}-{hhmm}-{}-{tail}",
                ais_core::write::slug::build(title, 50)
            )
        };
    let taken = canonical(h, "2026-11-08", "0800", "충돌", "0008", "2026-11");
    put_both(h, &format!("{taken}/keep.txt"), "x\n");
    update_step(h, "빈자리 -2", record, &[("title", "충돌")], None, true);
    // 기록 노트는 이미 `충돌`로 옮겨졌으므로, 같은 이름을 한 번 더 막고 다른 제목에서 되돌려 본다
    put_both(h, &format!("{taken}-2/keep.txt"), "x\n");
    update_step(h, "제목 바꿨다가", record, &[("title", "다른")], None, true);
    update_step(
        h,
        "되돌리기(-3까지 차 있음)",
        record,
        &[("title", "충돌")],
        None,
        true,
    );

    // ---- 저장 상한: 상한 바로 아래 노트의 제목을 길게 ----
    let filler = "a".repeat(999_800);
    put_both(
        h,
        "notes/work/big/note.md",
        &format!(
            "---\nid: ndfx00a0-000000a0\nagenda: work\ntype: free\ntitle: 큰 노트\ncreated: '2026-11-10T09:00:00+09:00'\n---\n{filler}"
        ),
    );
    update_step(
        h,
        "상한 안(제목 조금)",
        "ndfx00a0-000000a0",
        &[("title", "큰 노트 둘")],
        None,
        true,
    );
    update_step(
        h,
        "상한 초과",
        "ndfx00a0-000000a0",
        &[("title", &"긴".repeat(300))],
        None,
        true,
    );

    // ---- 월별 배치 스위치 ----
    // SAFETY: 단일 스레드이고, Java 단발 프로세스는 부모의 환경을 물려받는다.
    unsafe {
        env::set_var("AISURADA_NOTES_SHARD_BY_MONTH", "false");
    }
    update_step(
        h,
        "월별 배치 끔: 제목",
        parent,
        &[("title", "월별 끔")],
        None,
        true,
    );
    update_step(
        h,
        "월별 배치 끔: 날짜",
        loose,
        &[("date", "2026-09-09")],
        None,
        true,
    );
    unsafe {
        env::set_var("AISURADA_NOTES_SHARD_BY_MONTH", "ON");
    }
    update_step(
        h,
        "월별 배치 켬: 제목",
        loose,
        &[("title", "월별 켬")],
        None,
        true,
    );
    unsafe {
        env::remove_var("AISURADA_NOTES_SHARD_BY_MONTH");
    }
}

/// `create_doc` 한 단계. `named`는 Java의 `key=value`이고, 호출 객체는 같은 글을 그대로 담는다.
/// `rust`가 있으면 그것을 대신 쓴다(`tags`와 `links`를 배열로 주는 경우).
fn create_step(h: &mut WriteHarness, label: &str, named: &[(&str, &str)], rust: Option<Value>) {
    let args = rust.unwrap_or_else(|| {
        let mut map = Map::new();
        for (key, value) in named {
            map.insert(key.to_string(), json!(value));
        }
        Value::Object(map)
    });
    h.note_step(
        &format!("만들기 {label}"),
        "create_doc",
        &[],
        named,
        args,
        None,
    );
}

/// `create_doc`의 기본 인자(demo 아젠다, 자유 유형, 짧은 본문)에 `extra`를 덮어쓴 목록
fn create_base<'a>(extra: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut named: Vec<(&'a str, &'a str)> = vec![
        ("agenda", "demo"),
        ("type", "free"),
        ("title", "새 노트"),
        ("body", "# 새 노트\n\n본문\n"),
    ];
    for (key, value) in extra {
        named.retain(|(k, _)| k != key);
        named.push((key, value));
    }
    named
}

/// `create_doc` 맞대기: 필수 인자와 값 검사, 아젠다 찾기·만들기, 시각 배정, 경고, 월별 배치, 시간대, 저장 상한.
fn compare_doc_creates(h: &mut WriteHarness) {
    // SAFETY: 단일 스레드이고, Java 단발 프로세스는 부모의 환경을 물려받는다.
    unsafe {
        env::set_var("TZ", "Asia/Seoul");
    }

    // ---- 기본, 시각 배정(같은 날 겹치면 5분씩 미룸) ----
    create_step(h, "최소", &create_base(&[]), None);
    create_step(h, "같은 제목(경고)", &create_base(&[]), None);
    create_step(
        h,
        "또 같은 제목(시각 미룸)",
        &create_base(&[("title", "  새   노트 ")]),
        None,
    );
    create_step(
        h,
        "대소문자만 다른 제목",
        &create_base(&[("title", "NEW 노트"), ("body", "본문")]),
        None,
    );
    create_step(
        h,
        "시각 지정",
        &create_base(&[("time", "09:00"), ("date", "2026-11-05")]),
        None,
    );
    create_step(
        h,
        "날짜만 지정(그날 노트와 겹침)",
        &create_base(&[("date", "2026-11-05"), ("title", "그날")]),
        None,
    );
    create_step(
        h,
        "즐겨찾기 노트와 같은 제목",
        &create_base(&[("agenda", "demo"), ("title", "제목 ndfx0003-00000003")]),
        None,
    );
    create_step(
        h,
        "모든 값",
        &create_base(&[
            ("date", "2026-12-24"),
            ("time", "10:20"),
            ("format", "markdown"),
            ("template", "my-tpl"),
            ("tags", r#"["b"," a ","a",""]"#),
            (
                "links",
                r#"[{"to":"ndfx0002-00000002","kind":"DIRECT"},{"to":"ndfx0002-00000002","kind":"direct"},{"to":"ndfx0003-00000003","kind":"Attach"}]"#,
            ),
            ("title", "모든 값: 콜론 / 슬래시"),
        ]),
        None,
    );
    create_step(
        h,
        "배열로 태그와 링크",
        &create_base(&[
            ("title", "배열"),
            ("tags", r#"["x","y"]"#),
            ("links", r#"[{"to":"ndfx0002-00000002","kind":"indirect"}]"#),
        ]),
        Some(
            json!({"agenda": "demo", "type": "free", "title": "배열", "body": "# 새 노트\n\n본문\n", "tags": ["x", "y"], "links": [{"to": "ndfx0002-00000002", "kind": "indirect"}]}),
        ),
    );
    create_step(
        h,
        "기록 층",
        &create_base(&[("tier", "RECORD"), ("title", "기록 노트")]),
        None,
    );
    create_step(
        h,
        "읽기 층 지정",
        &create_base(&[("tier", "Reading"), ("title", "읽기 노트")]),
        None,
    );
    create_step(
        h,
        "유형 대소문자",
        &create_base(&[("type", "PLAN"), ("title", "유형")]),
        None,
    );
    create_step(
        h,
        "제목 양끝 공백은 그대로",
        &create_base(&[("title", "  공백  ")]),
        None,
    );
    create_step(
        h,
        "특수 글자 제목",
        &create_base(&[("title", "# 해시: 'a' \"b\" 😀 / \\")]),
        None,
    );
    create_step(
        h,
        "빈 본문",
        &create_base(&[("body", ""), ("title", "빈 본문")]),
        None,
    );
    create_step(
        h,
        "CRLF 본문",
        &create_base(&[("body", "가\r\n나\r\n"), ("title", "CRLF")]),
        None,
    );

    // ---- 아젠다 찾기와 만들기 ----
    create_step(
        h,
        "아젠다 slug",
        &create_base(&[("agenda", "work"), ("title", "업무")]),
        None,
    );
    create_step(
        h,
        "아젠다 이름(대소문자 무시)",
        &create_base(&[("agenda", "데모 아젠다"), ("title", "이름으로")]),
        None,
    );
    create_step(
        h,
        "아젠다 이름 앞뒤 공백",
        &create_base(&[("agenda", "  work  "), ("title", "공백 아젠다")]),
        None,
    );
    create_step(
        h,
        "새 아젠다",
        &create_base(&[("agenda", "신규 아젠다"), ("title", "신규")]),
        None,
    );
    create_step(
        h,
        "방금 만든 아젠다 slug",
        &create_base(&[("agenda", "신규-아젠다"), ("title", "신규 둘")]),
        None,
    );
    create_step(
        h,
        "새 아젠다 영문 이름",
        &create_base(&[("agenda", "Hello World"), ("title", "영문")]),
        None,
    );
    create_step(
        h,
        "새 아젠다 긴 이름",
        &create_base(&[("agenda", &"가".repeat(70)), ("title", "긴 아젠다")]),
        None,
    );
    create_step(
        h,
        "새 아젠다 특수 글자",
        &create_base(&[("agenda", "a/b:c"), ("title", "특수")]),
        None,
    );
    create_step(h, "아젠다 공백", &create_base(&[("agenda", "   ")]), None);

    // ---- 경고 ----
    create_step(
        h,
        "들여쓴 블록과 헤딩 불일치",
        &create_base(&[
            ("title", "경고 모음"),
            ("body", "# 다른 제목\n  ## 들여쓴\n"),
        ]),
        None,
    );
    create_step(
        h,
        "깨진 글자",
        &create_base(&[("title", "깨짐"), ("body", "가\u{FFFD}\n\u{FFFD}")]),
        None,
    );
    create_step(
        h,
        "헤딩 없음",
        &create_base(&[("title", "헤딩 없음"), ("body", "그냥 글")]),
        None,
    );

    // ---- 입력 오류(검사 차례대로) ----
    create_step(h, "format html", &create_base(&[("format", "html")]), None);
    create_step(
        h,
        "format 대문자",
        &create_base(&[("format", "Markdown")]),
        None,
    );
    for (label, drop) in [
        ("agenda 없음", "agenda"),
        ("type 없음", "type"),
        ("title 없음", "title"),
        ("body 없음", "body"),
    ] {
        let named: Vec<(&str, &str)> = create_base(&[])
            .into_iter()
            .filter(|(k, _)| *k != drop)
            .collect();
        create_step(h, label, &named, None);
    }
    create_step(h, "type 모름", &create_base(&[("type", "zzz")]), None);
    create_step(
        h,
        "type 공백이 낌",
        &create_base(&[("type", " plan")]),
        None,
    );
    create_step(h, "제목 공백", &create_base(&[("title", "   ")]), None);
    create_step(h, "제목 빈 글", &create_base(&[("title", "")]), None);
    for (label, date) in [
        ("달력에 없음", "2026-02-30"),
        ("모양", "26-1-1"),
        ("연도 한도 밖", "+1000000000-01-01"),
    ] {
        create_step(
            h,
            &format!("날짜 {label}"),
            &create_base(&[("date", date)]),
            None,
        );
    }
    for (label, time) in [("24시", "24:00"), ("한 자리", "9:05")] {
        create_step(
            h,
            &format!("시각 {label}"),
            &create_base(&[("time", time)]),
            None,
        );
    }
    for (label, tags) in [
        ("객체", r#"{"a":1}"#),
        ("문법", "[1,"),
        ("항목 숫자", "[1]"),
        ("줄바꿈", r#"["a\nb"]"#),
        ("항목 객체", r#"[{"a":1}]"#),
    ] {
        create_step(
            h,
            &format!("tags {label}"),
            &create_base(&[("tags", tags)]),
            None,
        );
    }
    for (label, links) in [
        ("객체", r#"{"a":1}"#),
        ("문법", "[{"),
        ("항목이 글", r#"["x"]"#),
        (
            "모르는 키",
            r#"[{"to":"ndfx0002-00000002","kind":"direct","zz":1,"aa":2}]"#,
        ),
        ("to 없음", r#"[{"kind":"direct"}]"#),
        ("to 숫자", r#"[{"to":5,"kind":"direct"}]"#),
        ("kind 없음", r#"[{"to":"ndfx0002-00000002"}]"#),
        ("kind 모름", r#"[{"to":"ndfx0002-00000002","kind":"zzz"}]"#),
        ("id 모양", r#"[{"to":"nope","kind":"direct"}]"#),
        (
            "중복 키는 뒤가 이김",
            r#"[{"to":"nope","to":"ndfx0002-00000002","kind":"direct"}]"#,
        ),
    ] {
        create_step(
            h,
            &format!("links {label}"),
            &create_base(&[("links", links)]),
            None,
        );
    }
    let long_to = format!(r#"[{{"to":"{}","kind":"direct"}}]"#, "a".repeat(201));
    let long_kind = format!(
        r#"[{{"to":"ndfx0002-00000002","kind":"{}"}}]"#,
        "k".repeat(201)
    );
    create_step(
        h,
        "links to가 김",
        &create_base(&[("links", &long_to)]),
        None,
    );
    create_step(
        h,
        "links kind가 김",
        &create_base(&[("links", &long_kind)]),
        None,
    );
    create_step(h, "tier 모름", &create_base(&[("tier", "zzz")]), None);
    create_step(h, "tier 빈 글", &create_base(&[("tier", "")]), None);
    create_step(
        h,
        "템플릿 모양이 틀림(버림)",
        &create_base(&[("template", "bad tpl!"), ("title", "템플릿 버림")]),
        None,
    );
    create_step(
        h,
        "템플릿 공백을 다듬음",
        &create_base(&[("template", "  spaced "), ("title", "템플릿 다듬음")]),
        None,
    );

    // ---- 월별 배치 스위치 ----
    unsafe {
        env::set_var("AISURADA_NOTES_SHARD_BY_MONTH", "no");
    }
    create_step(
        h,
        "월별 배치 끔",
        &create_base(&[("title", "월별 끔"), ("date", "2026-08-08")]),
        None,
    );
    unsafe {
        env::set_var("AISURADA_NOTES_SHARD_BY_MONTH", "1");
    }
    create_step(
        h,
        "월별 배치 켬",
        &create_base(&[
            ("agenda", "work"),
            ("title", "월별 켬"),
            ("date", "2026-07-07"),
        ]),
        None,
    );
    unsafe {
        env::remove_var("AISURADA_NOTES_SHARD_BY_MONTH");
    }

    // ---- 이상한 연도 ----
    create_step(
        h,
        "0년",
        &create_base(&[
            ("date", "-00000-03-04"),
            ("time", "01:02"),
            ("title", "영년"),
        ]),
        None,
    );
    create_step(
        h,
        "기원전 1년",
        &create_base(&[
            ("date", "-0001-03-04"),
            ("time", "01:03"),
            ("title", "기원전"),
        ]),
        None,
    );
    create_step(
        h,
        "다섯 자리 연도",
        &create_base(&[
            ("date", "+12345-03-04"),
            ("time", "01:04"),
            ("title", "먼 미래"),
        ]),
        None,
    );

    // ---- 시간대: 같은 순간이 서울과 로스앤젤레스에서 다른 날이다 ----
    unsafe {
        env::set_var("TZ", "America/Los_Angeles");
    }
    create_step(
        h,
        "시간대 로스앤젤레스",
        &create_base(&[("title", "엘에이")]),
        None,
    );
    create_step(
        h,
        "시간대 로스앤젤레스 둘째",
        &create_base(&[("title", "엘에이 둘")]),
        None,
    );
    unsafe {
        env::set_var("TZ", "Asia/Seoul");
    }

    // ---- 저장 상한: 새 아젠다를 먼저 만들고 노트를 못 쓰는 경우 ----
    create_step(
        h,
        "상한 초과(새 아젠다가 남음)",
        &create_base(&[
            ("agenda", "상한 시험"),
            ("title", "큼"),
            ("body", &"a".repeat(1_000_000)),
        ]),
        None,
    );
    unsafe {
        env::remove_var("TZ");
    }
}

fn compare_writes(report: &mut Report, java: &Java, fixture: &Path) {
    let root = env::temp_dir().join(format!("ais-write-compat-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("임시 폴더");
    let locks = root.join("locks");
    let copy = |name: &str| -> PathBuf {
        let dst = root.join(name);
        let status = Command::new("cp")
            .arg("-R")
            .arg(fixture)
            .arg(&dst)
            .status()
            .expect("cp");
        assert!(status.success(), "사본을 못 만들었다");
        // 쓰기 명령은 워크스페이스 표식이 있어야 일한다. 두 사본에 같게 만든다.
        fs::write(dst.join(".aisurada/workspace.json"), "{}\n").unwrap();
        dst
    };
    // SAFETY: 단일 스레드에서 한 번만 바꾼다.
    unsafe {
        env::set_var("AISURADA_TEST_NOW", WRITE_NOW);
        env::set_var("AISURADA_LOCK_DIR", &locks);
        env::set_var("AISURADA_VAULT_AUTOCOMMIT_ENABLED", "false");
    }
    let mut h = WriteHarness {
        report,
        java,
        a: copy("java"),
        b: copy("rust"),
        locks,
        step: 0,
        succeeded: 0,
    };
    let id_of = |v: &Option<Value>| {
        v.as_ref()
            .and_then(|r| r["id"].as_str().map(str::to_string))
    };

    // 프로젝트와 그 저장소(있으면): 아젠다에 연결해 본다
    let projects = Engine::new(&h.b).call("list_projects", Value::Null).ok();
    let project = projects
        .as_ref()
        .and_then(|p| p["items"].as_array())
        .and_then(|items| {
            items
                .iter()
                .find(|i| i["repos"].as_array().is_some_and(|r| !r.is_empty()))
        })
        .map(|i| {
            (
                i["slug"].as_str().unwrap().to_string(),
                i["repos"][0]["name"].as_str().unwrap().to_string(),
            )
        });

    // 아젠다 만들기: 최소, 같은 이름(번호 붙임), 모든 값, 이름에 특수 글자, 요약에 여러 줄
    let created = h.step(
        "최소",
        &["create-agenda", "title=새 아젠다"],
        "create_agenda",
        json!({"title": "새 아젠다"}),
    );
    h.step(
        "같은 이름",
        &["create-agenda", "title=새 아젠다"],
        "create_agenda",
        json!({"title": "새 아젠다"}),
    );
    let full = h.step(
        "모든 값",
        &["create-agenda", "title=Hello World", "hue=teal", "repo=owner/repo", "summary=요약\n두 줄"],
        "create_agenda",
        json!({"title": "Hello World", "hue": "teal", "repo": "owner/repo", "summary": "요약\n두 줄"}),
    );
    h.step(
        "특수 글자",
        &["create-agenda", "title=콜론: 해시 # \"따옴표\" 'a' 😀"],
        "create_agenda",
        json!({"title": "콜론: 해시 # \"따옴표\" 'a' 😀"}),
    );
    h.step(
        "긴 이름",
        &["create-agenda", &format!("title={}", "가".repeat(70))],
        "create_agenda",
        json!({"title": "가".repeat(70)}),
    );
    if let Some((slug, repo)) = &project {
        h.step(
            "프로젝트와 저장소",
            &[
                "create-agenda",
                "title=프로젝트 아젠다",
                &format!("project={slug}"),
                &format!("projectRepo={repo}"),
            ],
            "create_agenda",
            json!({"title": "프로젝트 아젠다", "project": slug, "projectRepo": repo}),
        );
    }
    // 만들기 오류
    for (label, java_args, args) in [
        ("제목 없음", vec!["create-agenda"], json!({})),
        (
            "공백 제목",
            vec!["create-agenda", "title=   "],
            json!({"title": "   "}),
        ),
        (
            "모르는 색",
            vec!["create-agenda", "title=x", "hue=zzz"],
            json!({"title": "x", "hue": "zzz"}),
        ),
        (
            "안전하지 않은 프로젝트",
            vec!["create-agenda", "title=x", "project=../x"],
            json!({"title": "x", "project": "../x"}),
        ),
        (
            "없는 프로젝트의 저장소",
            vec!["create-agenda", "title=x", "project=nope", "projectRepo=r"],
            json!({"title": "x", "project": "nope", "projectRepo": "r"}),
        ),
        (
            "프로젝트 없는 저장소",
            vec!["create-agenda", "title=x", "projectRepo=r"],
            json!({"title": "x", "projectRepo": "r"}),
        ),
    ] {
        h.step(label, &java_args, "create_agenda", args);
    }

    // 고치기: 이름과 색, 저장소 지우기와 프로젝트 지우기, 요약, 아무것도 안 바꿈
    if let Some(id) = id_of(&created) {
        h.step(
            "이름과 색",
            &["update-agenda", &id, "title=바뀐 이름", "hue=red"],
            "update_agenda",
            json!({"id": id, "title": "바뀐 이름", "hue": "red"}),
        );
        h.step(
            "요약",
            &["update-agenda", &id, "summary=새 요약\n둘째 줄\n"],
            "update_agenda",
            json!({"id": id, "summary": "새 요약\n둘째 줄\n"}),
        );
        h.step(
            "저장소 설정",
            &["update-agenda", &id, "repo=o/r"],
            "update_agenda",
            json!({"id": id, "repo": "o/r"}),
        );
        h.step(
            "저장소 지움",
            &["update-agenda", &id, "repo="],
            "update_agenda",
            json!({"id": id, "repo": ""}),
        );
        h.step(
            "아무것도 안 줌",
            &["update-agenda", &id],
            "update_agenda",
            json!({"id": id}),
        );
        if let Some((slug, repo)) = &project {
            h.step(
                "프로젝트 설정",
                &[
                    "update-agenda",
                    &id,
                    &format!("project={slug}"),
                    &format!("projectRepo={repo}"),
                ],
                "update_agenda",
                json!({"id": id, "project": slug, "projectRepo": repo}),
            );
            h.step(
                "프로젝트 지움",
                &["update-agenda", &id, "project="],
                "update_agenda",
                json!({"id": id, "project": ""}),
            );
        }
        h.step(
            "모르는 색",
            &["update-agenda", &id, "hue=zzz"],
            "update_agenda",
            json!({"id": id, "hue": "zzz"}),
        );
    }
    for (label, java_args, args) in [
        ("id 없음", vec!["update-agenda"], json!({})),
        (
            "id 모양 아님",
            vec!["update-agenda", "nope"],
            json!({"id": "nope"}),
        ),
        (
            "없는 id",
            vec!["update-agenda", "agzz0001-00000000", "title=x"],
            json!({"id": "agzz0001-00000000", "title": "x"}),
        ),
    ] {
        h.step(label, &java_args, "update_agenda", args);
    }

    // 지우기: 노트가 있는 아젠다는 거절, 비어 있으면 휴지통 폴더로 옮김, 같은 id 두 번
    h.step(
        "노트가 있다",
        &["delete-agenda", "agfx0001-0000000a"],
        "delete_agenda",
        json!({"id": "agfx0001-0000000a"}),
    );
    if let Some(id) = id_of(&full) {
        h.step(
            "비어 있다",
            &["delete-agenda", &id],
            "delete_agenda",
            json!({"id": id}),
        );
        h.step(
            "이미 지웠다",
            &["delete-agenda", &id],
            "delete_agenda",
            json!({"id": id}),
        );
    }
    h.step("id 없음", &["delete-agenda"], "delete_agenda", json!({}));

    // ---- 프로젝트 ----
    let repos_ok = r#"[{"name":"web","path":"/tmp/a b","github":"owner/name","githubId":123},{"name":"api","github":"o/n"}]"#;
    let repos_special = r#"[{"name":"q","path":"/tmp/q\"uote\\back\tab","github":"o/n.x_y-z"}]"#;
    let p_min = h.step(
        "최소",
        &["create-project", "title=새 프로젝트"],
        "create_project",
        json!({"title": "새 프로젝트"}),
    );
    h.step(
        "같은 이름",
        &["create-project", "title=새 프로젝트"],
        "create_project",
        json!({"title": "새 프로젝트"}),
    );
    let p_full = h.step(
        "모든 값",
        &["create-project", "title=Alpha: Beta #1", "hue=teal", "summary=요약\n둘째 줄", &format!("repos={repos_ok}")],
        "create_project",
        json!({"title": "Alpha: Beta #1", "hue": "teal", "summary": "요약\n둘째 줄", "repos": repos_ok}),
    );
    h.step(
        "저장소 특수 글자",
        &[
            "create-project",
            "title=특수",
            &format!("repos={repos_special}"),
        ],
        "create_project",
        json!({"title": "특수", "repos": repos_special}),
    );
    let too_many = format!("[{}]", vec![r#"{"name":"a","path":"/x"}"#; 21].join(","));
    for (label, repos) in [
        ("저장소 객체 아님", "[1]".to_string()),
        ("저장소 배열 아님", "{}".to_string()),
        (
            "모르는 키",
            r#"[{"name":"a","path":"/x","bad":1}]"#.to_string(),
        ),
        ("이름 없음", r#"[{"path":"/x"}]"#.to_string()),
        ("이름이 글 아님", r#"[{"name":1,"path":"/x"}]"#.to_string()),
        (
            "안전하지 않은 이름",
            r#"[{"name":"a/b","path":"/x"}]"#.to_string(),
        ),
        ("상대 경로", r#"[{"name":"a","path":"rel"}]"#.to_string()),
        (
            "github 모양",
            r#"[{"name":"a","github":"nope"}]"#.to_string(),
        ),
        (
            "githubId 0",
            r#"[{"name":"a","github":"o/n","githubId":0}]"#.to_string(),
        ),
        (
            "githubId 글",
            r#"[{"name":"a","github":"o/n","githubId":"5"}]"#.to_string(),
        ),
        (
            "githubId만",
            r#"[{"name":"a","path":"/x","githubId":5}]"#.to_string(),
        ),
        ("path도 github도 없음", r#"[{"name":"a"}]"#.to_string()),
        (
            "이름 겹침",
            r#"[{"name":"a","path":"/x"},{"name":"a","path":"/y"}]"#.to_string(),
        ),
        ("21개", too_many),
    ] {
        h.step(
            &format!("저장소 오류 {label}"),
            &["create-project", "title=오류", &format!("repos={repos}")],
            "create_project",
            json!({"title": "오류", "repos": repos}),
        );
    }
    for (label, java_args, args) in [
        ("제목 없음", vec!["create-project"], json!({})),
        (
            "모르는 색",
            vec!["create-project", "title=x", "hue=zzz"],
            json!({"title": "x", "hue": "zzz"}),
        ),
    ] {
        h.step(label, &java_args, "create_project", args);
    }
    if let Some(id) = id_of(&p_full) {
        h.step(
            "이름과 색",
            &["update-project", &id, "title=바뀐 프로젝트", "hue=red"],
            "update_project",
            json!({"id": id, "title": "바뀐 프로젝트", "hue": "red"}),
        );
        h.step(
            "요약",
            &["update-project", &id, "summary=새 요약\n"],
            "update_project",
            json!({"id": id, "summary": "새 요약\n"}),
        );
        let swapped = r#"[{"name":"cli","path":"/usr/local/cli"}]"#;
        h.step(
            "저장소 바꿈",
            &["update-project", &id, &format!("repos={swapped}")],
            "update_project",
            json!({"id": id, "repos": swapped}),
        );
        h.step(
            "저장소 지움",
            &["update-project", &id, "repos="],
            "update_project",
            json!({"id": id, "repos": ""}),
        );
        h.step(
            "아무것도 안 줌",
            &["update-project", &id],
            "update_project",
            json!({"id": id}),
        );
        h.step(
            "저장소 오류",
            &["update-project", &id, "repos=[1]"],
            "update_project",
            json!({"id": id, "repos": "[1]"}),
        );
    }
    for (label, java_args, args) in [
        ("id 없음", vec!["update-project"], json!({})),
        (
            "id 모양 아님",
            vec!["update-project", "nope"],
            json!({"id": "nope"}),
        ),
        (
            "없는 id",
            vec!["update-project", "pjzz0001-00000000", "title=x"],
            json!({"id": "pjzz0001-00000000", "title": "x"}),
        ),
    ] {
        h.step(
            &format!("프로젝트 {label}"),
            &java_args,
            "update_project",
            args,
        );
    }
    // 지우기: 아젠다가 쓰는 프로젝트는 거절하고, 아젠다를 옮기면 지운다
    if let Some(id) = id_of(&p_full) {
        let slug = p_full
            .as_ref()
            .and_then(|v| v["slug"].as_str().map(str::to_string))
            .unwrap_or_default();
        let user = h.step(
            "프로젝트를 쓰는 아젠다",
            &[
                "create-agenda",
                "title=프로젝트 소속",
                &format!("project={slug}"),
            ],
            "create_agenda",
            json!({"title": "프로젝트 소속", "project": slug}),
        );
        h.step(
            "아젠다가 있다",
            &["delete-project", &id],
            "delete_project",
            json!({"id": id}),
        );
        if let Some(agenda) = id_of(&user) {
            h.step(
                "아젠다를 옮김",
                &["update-agenda", &agenda, "project="],
                "update_agenda",
                json!({"id": agenda, "project": ""}),
            );
        }
        h.step(
            "비어 있다",
            &["delete-project", &id],
            "delete_project",
            json!({"id": id}),
        );
        h.step(
            "이미 지웠다",
            &["delete-project", &id],
            "delete_project",
            json!({"id": id}),
        );
    }
    if let Some(id) = id_of(&p_min) {
        h.step(
            "최소 프로젝트 지움",
            &["delete-project", &id],
            "delete_project",
            json!({"id": id}),
        );
    }
    h.step(
        "프로젝트 id 없음",
        &["delete-project"],
        "delete_project",
        json!({}),
    );

    // ---- 템플릿 ----
    let templates = Engine::new(&h.b).call("list_templates", json!({})).ok();
    let items: Vec<Value> = templates
        .as_ref()
        .and_then(|t| t["items"].as_array().cloned())
        .unwrap_or_default();
    let pick = |category: &str, wanted: &dyn Fn(&Value) -> bool| -> Option<String> {
        items
            .iter()
            .find(|i| i["category"] == category && wanted(i))
            .and_then(|i| i["id"].as_str().map(str::to_string))
    };
    let doc_md = pick("doc", &|i| {
        i["builtin"] != true && i["format"] == "markdown"
    });
    let doc_html = pick("doc", &|i| i["format"] == "html");
    let doc_builtin = pick("doc", &|i| i["builtin"] == true);
    let ritual = pick("ritual", &|_| true);
    let indented = "  # 들여쓴 제목\n본문\n  > 인용\n";
    let made = h.step(
        "새 템플릿",
        &["save-template", "name=새 양식", "body=# 양식\n본문\n"],
        "save_template",
        json!({"name": "새 양식", "body": "# 양식\n본문\n"}),
    );
    h.step(
        "새 템플릿 모든 값",
        &[
            "save-template",
            "name= 이름 ",
            "type=Build",
            "desc=설명: 콜론 # 해시",
            &format!("body={indented}"),
        ],
        "save_template",
        json!({"name": " 이름 ", "type": "Build", "desc": "설명: 콜론 # 해시", "body": indented}),
    );
    for (label, java_args, args) in [
        (
            "body 없음",
            vec!["save-template", "name=x"],
            json!({"name": "x"}),
        ),
        (
            "공백 body",
            vec!["save-template", "name=x", "body=  "],
            json!({"name": "x", "body": "  "}),
        ),
        (
            "name 없음",
            vec!["save-template", "body=b"],
            json!({"body": "b"}),
        ),
        (
            "공백 name",
            vec!["save-template", "name=  ", "body=b"],
            json!({"name": "  ", "body": "b"}),
        ),
        (
            "모르는 유형",
            vec!["save-template", "name=x", "body=b", "type=zzz"],
            json!({"name": "x", "body": "b", "type": "zzz"}),
        ),
        (
            "id 모양 아님",
            vec!["save-template", "id=a b"],
            json!({"id": "a b"}),
        ),
        (
            "없는 id",
            vec!["save-template", "id=nope-none", "name=x"],
            json!({"id": "nope-none", "name": "x"}),
        ),
        (
            "공백 이름으로 고침",
            vec!["save-template", "id=plan-basic", "name=  "],
            json!({"id": "plan-basic", "name": "  "}),
        ),
    ] {
        h.step(
            &format!("템플릿 {label}"),
            &java_args,
            "save_template",
            args,
        );
    }
    if let Some(id) = id_of(&made) {
        h.step(
            "이름·설명·본문 고침",
            &[
                "save-template",
                &format!("id={id}"),
                "name=고친 이름",
                "desc=고친 설명",
                "body=고친 본문\n",
            ],
            "save_template",
            json!({"id": id, "name": "고친 이름", "desc": "고친 설명", "body": "고친 본문\n"}),
        );
        h.step(
            "유형만 고침",
            &["save-template", &format!("id={id}"), "type=REPORT"],
            "save_template",
            json!({"id": id, "type": "REPORT"}),
        );
        h.step(
            "아무것도 안 줌",
            &["save-template", &format!("id={id}")],
            "save_template",
            json!({"id": id}),
        );
        h.step(
            "새 템플릿 지움",
            &["delete-doc-template", &id],
            "delete_doc_template",
            json!({"id": id}),
        );
        h.step(
            "이미 지웠다",
            &["delete-doc-template", &id],
            "delete_doc_template",
            json!({"id": id}),
        );
    }
    if let Some(id) = &doc_md {
        h.step(
            "기존 문서 템플릿 고침",
            &[
                "save-template",
                &format!("id={id}"),
                "body=바뀐 본문\n  | a | b |\n  |---|---|\n",
            ],
            "save_template",
            json!({"id": id, "body": "바뀐 본문\n  | a | b |\n  |---|---|\n"}),
        );
    }
    if let Some(id) = &doc_html {
        h.step(
            "html 템플릿을 고치면 md로 바뀐다",
            &["save-template", &format!("id={id}"), "desc=html에서"],
            "save_template",
            json!({"id": id, "desc": "html에서"}),
        );
    }
    if let Some(id) = &doc_builtin {
        h.step(
            "빌트인 고침",
            &["save-template", &format!("id={id}"), "desc=고친 빌트인"],
            "save_template",
            json!({"id": id, "desc": "고친 빌트인"}),
        );
        h.step(
            "빌트인 지움",
            &["delete-doc-template", id],
            "delete_doc_template",
            json!({"id": id}),
        );
    }
    if let Some(id) = &ritual {
        h.step(
            "리추얼 고침",
            &[
                "save-ritual-template",
                id,
                "name=회고 이름",
                "desc=회고 설명",
                "body=회고 본문\n",
            ],
            "save_ritual_template",
            json!({"id": id, "name": "회고 이름", "desc": "회고 설명", "body": "회고 본문\n"}),
        );
        h.step(
            "리추얼 아무것도 안 줌",
            &["save-ritual-template", id],
            "save_ritual_template",
            json!({"id": id}),
        );
        h.step(
            "리추얼 공백 이름",
            &["save-ritual-template", id, "name= "],
            "save_ritual_template",
            json!({"id": id, "name": " "}),
        );
        h.step(
            "리추얼 지움",
            &["delete-doc-template", id],
            "delete_doc_template",
            json!({"id": id}),
        );
        h.step(
            "문서 id로 리추얼 고침",
            &["save-template", &format!("id={id}"), "name=x"],
            "save_template",
            json!({"id": id, "name": "x"}),
        );
    }
    if let Some(id) = &doc_md {
        h.step(
            "문서 id로 리추얼 고침 시도",
            &["save-ritual-template", id, "name=x"],
            "save_ritual_template",
            json!({"id": id, "name": "x"}),
        );
    }
    for (label, java_args, args, command) in [
        (
            "리추얼 id 없음",
            vec!["save-ritual-template"],
            json!({}),
            "save_ritual_template",
        ),
        (
            "리추얼 id 모양",
            vec!["save-ritual-template", "a b"],
            json!({"id": "a b"}),
            "save_ritual_template",
        ),
        (
            "리추얼 없는 id",
            vec!["save-ritual-template", "nope-none"],
            json!({"id": "nope-none"}),
            "save_ritual_template",
        ),
        (
            "지움 id 없음",
            vec!["delete-doc-template"],
            json!({}),
            "delete_doc_template",
        ),
        (
            "지움 id 모양",
            vec!["delete-doc-template", "a/b"],
            json!({"id": "a/b"}),
            "delete_doc_template",
        ),
        (
            "없는 템플릿 지움",
            vec!["delete-doc-template", "nope-none"],
            json!({"id": "nope-none"}),
            "delete_doc_template",
        ),
    ] {
        h.step(label, &java_args, command, args);
    }

    // ---- 사실층 ----
    let demo = "agfx0001-0000000a";
    let long = "가".repeat(2001);
    for (label, java_args, args) in [
        (
            "설정",
            vec![
                "update-agenda-facts",
                demo,
                "facts=  지금도 참인 것\n둘째 줄  ",
            ],
            json!({"id": demo, "facts": "  지금도 참인 것\n둘째 줄  "}),
        ),
        (
            "다시 설정",
            vec!["update-agenda-facts", demo, "facts=바뀐 사실"],
            json!({"id": demo, "facts": "바뀐 사실"}),
        ),
        (
            "정확히 상한",
            vec![
                "update-agenda-facts",
                demo,
                &format!("facts={}", "나".repeat(2000)),
            ],
            json!({"id": demo, "facts": "나".repeat(2000)}),
        ),
        (
            "상한 초과",
            vec!["update-agenda-facts", demo, &format!("facts={long}")],
            json!({"id": demo, "facts": long}),
        ),
        (
            "facts 없음",
            vec!["update-agenda-facts", demo],
            json!({"id": demo}),
        ),
        (
            "지움",
            vec!["update-agenda-facts", demo, "facts= "],
            json!({"id": demo, "facts": " "}),
        ),
        (
            "이미 없다",
            vec!["update-agenda-facts", demo, "facts="],
            json!({"id": demo, "facts": ""}),
        ),
        (
            "없는 아젠다",
            vec!["update-agenda-facts", "agzz0001-00000000", "facts=x"],
            json!({"id": "agzz0001-00000000", "facts": "x"}),
        ),
        (
            "id 모양",
            vec!["update-agenda-facts", "nope", "facts=x"],
            json!({"id": "nope", "facts": "x"}),
        ),
        ("id 없음", vec!["update-agenda-facts"], json!({})),
    ] {
        h.step(
            &format!("사실층 {label}"),
            &java_args,
            "update_agenda_facts",
            args,
        );
    }

    // ---- 노트 머리만 고치기: 즐겨찾기, 평가, 연결 ----
    let parent = "ndfx0002-00000002";
    let loose = "ndfx0003-00000003";
    let odd = "ndfx0004-00000004";
    h.note_step(
        "뒤집기",
        "set_favorite",
        &[parent],
        &[],
        json!({"id": parent}),
        Some(parent),
    );
    h.note_step(
        "다시 뒤집기",
        "set_favorite",
        &[parent],
        &[],
        json!({"id": parent}),
        Some(parent),
    );
    h.note_step(
        "켜고 라벨",
        "set_favorite",
        &[parent],
        &[("favorite", "true"), ("category", " 중요: 라벨 ")],
        json!({"id": parent, "favorite": true, "category": " 중요: 라벨 "}),
        Some(parent),
    );
    h.note_step(
        "라벨 지움",
        "set_favorite",
        &[parent],
        &[("category", "")],
        json!({"id": parent, "category": ""}),
        Some(parent),
    );
    h.note_step(
        "바뀐 것 없음(확인 값 필요 없다)",
        "set_favorite",
        &[parent],
        &[("favorite", "true")],
        json!({"id": parent, "favorite": true}),
        None,
    );
    h.note_step(
        "라벨만 줌",
        "set_favorite",
        &[parent],
        &[("category", "보관")],
        json!({"id": parent, "category": "보관"}),
        Some(parent),
    );
    h.note_step(
        "끔",
        "set_favorite",
        &[parent],
        &[("favorite", "false")],
        json!({"id": parent, "favorite": "false"}),
        Some(parent),
    );
    h.note_step(
        "확인 값 없음",
        "set_favorite",
        &[parent],
        &[],
        json!({"id": parent}),
        None,
    );
    let other = h.token(loose);
    h.note_step(
        "남의 확인 값",
        "set_favorite",
        &[parent],
        &[("f1", &other)],
        json!({"id": parent, "f1": other}),
        None,
    );
    h.note_step(
        "모양이 틀린 확인 값",
        "set_favorite",
        &[parent],
        &[("f1", "abc")],
        json!({"id": parent, "f1": "abc"}),
        None,
    );
    for (label, positional, named, args) in [
        (
            "favorite 글",
            vec![parent],
            vec![("favorite", "maybe")],
            json!({"id": parent, "favorite": "maybe"}),
        ),
        ("id 없음", vec![], vec![], json!({})),
        ("id 모양", vec!["nope"], vec![], json!({"id": "nope"})),
        (
            "없는 id",
            vec!["ndzz0001-00000001"],
            vec![],
            json!({"id": "ndzz0001-00000001"}),
        ),
    ] {
        h.note_step(label, "set_favorite", &positional, &named, args, None);
    }
    for (label, id) in [
        ("html 노트", "ndfx0005-00000005"),
        ("확장자 폴더 노트", "ndfx0007-00000007"),
        ("기록 층 노트", "ndfx0008-00000008"),
        ("tier 값이 이상한 노트", "ndfx0009-00000009"),
        ("created 오프셋 노트", "ndfx000a-0000000a"),
        ("제목이 공백인 노트", odd),
    ] {
        h.note_step(
            label,
            "set_favorite",
            &[id],
            &[],
            json!({"id": id}),
            Some(id),
        );
    }
    h.note_step(
        "제목이 공백인 노트 라벨",
        "set_favorite",
        &[odd],
        &[("category", "새 라벨")],
        json!({"id": odd, "category": "새 라벨"}),
        Some(odd),
    );
    h.note_step(
        "json 노트(확인 값 면제)",
        "set_favorite",
        &["ndfx0006-00000006"],
        &[],
        json!({"id": "ndfx0006-00000006"}),
        None,
    );

    let long_note = "가".repeat(1500);
    for (label, named, args) in [
        (
            "반올림",
            vec![("score", "87.5"), ("note", " 좋음 ")],
            json!({"id": parent, "score": 87.5, "note": " 좋음 "}),
        ),
        (
            "0점",
            vec![("score", "0")],
            json!({"id": parent, "score": 0}),
        ),
        (
            "100점",
            vec![("score", "100")],
            json!({"id": parent, "score": "100"}),
        ),
        (
            "100.4",
            vec![("score", "100.4")],
            json!({"id": parent, "score": 100.4}),
        ),
        (
            "공백 점수",
            vec![("score", " 42 ")],
            json!({"id": parent, "score": " 42 "}),
        ),
        (
            "메모를 자름",
            vec![("score", "50"), ("note", &long_note)],
            json!({"id": parent, "score": 50, "note": long_note}),
        ),
        (
            "지움",
            vec![("clear", "true")],
            json!({"id": parent, "clear": true}),
        ),
        (
            "또 지움",
            vec![("clear", "true")],
            json!({"id": parent, "clear": "true"}),
        ),
    ] {
        h.note_step(
            &format!("평가 {label}"),
            "evaluate_doc",
            &[parent],
            &named,
            args,
            Some(parent),
        );
    }
    h.note_step(
        "평가 제목이 공백인 노트",
        "evaluate_doc",
        &[odd],
        &[("score", "55")],
        json!({"id": odd, "score": 55}),
        Some(odd),
    );
    h.note_step(
        "평가 지움(그 노트)",
        "evaluate_doc",
        &[odd],
        &[("clear", "true")],
        json!({"id": odd, "clear": true}),
        Some(odd),
    );
    for (label, positional, named, args, token) in [
        (
            "score 없음",
            vec![parent],
            vec![],
            json!({"id": parent}),
            Some(parent),
        ),
        (
            "빈 score",
            vec![parent],
            vec![("score", " ")],
            json!({"id": parent, "score": " "}),
            Some(parent),
        ),
        (
            "score 글",
            vec![parent],
            vec![("score", "abc")],
            json!({"id": parent, "score": "abc"}),
            Some(parent),
        ),
        (
            "101",
            vec![parent],
            vec![("score", "101")],
            json!({"id": parent, "score": 101}),
            Some(parent),
        ),
        (
            "음수",
            vec![parent],
            vec![("score", "-1")],
            json!({"id": parent, "score": -1}),
            Some(parent),
        ),
        (
            "확인 값 없음",
            vec![parent],
            vec![("score", "10")],
            json!({"id": parent, "score": 10}),
            None,
        ),
        (
            "clear 글",
            vec![parent],
            vec![("clear", "yes")],
            json!({"id": parent, "clear": "yes"}),
            None,
        ),
        (
            "id 모양",
            vec!["nope"],
            vec![("score", "10")],
            json!({"id": "nope", "score": 10}),
            None,
        ),
        (
            "없는 id",
            vec!["ndzz0001-00000001"],
            vec![("score", "10")],
            json!({"id": "ndzz0001-00000001", "score": 10}),
            None,
        ),
    ] {
        h.note_step(
            &format!("평가 오류 {label}"),
            "evaluate_doc",
            &positional,
            &named,
            args,
            token,
        );
    }

    for (label, to, named, args) in [
        (
            "더함(기본 direct)",
            loose,
            vec![],
            json!({"from": parent, "to": loose}),
        ),
        (
            "이미 있다",
            loose,
            vec![],
            json!({"from": parent, "to": loose}),
        ),
        (
            "indirect(대문자)",
            loose,
            vec![("kind", "INDIRECT")],
            json!({"from": parent, "to": loose, "kind": "INDIRECT"}),
        ),
        (
            "attach",
            loose,
            vec![("kind", "attach")],
            json!({"from": parent, "to": loose, "kind": "attach"}),
        ),
        (
            "없는 대상(경고)",
            "ndzz0001-00000001",
            vec![],
            json!({"from": parent, "to": "ndzz0001-00000001"}),
        ),
        (
            "빼기",
            loose,
            vec![("remove", "true")],
            json!({"from": parent, "to": loose, "remove": true}),
        ),
        (
            "없는 연결 빼기",
            loose,
            vec![("remove", "true")],
            json!({"from": parent, "to": loose, "remove": "true"}),
        ),
    ] {
        let no_token = label.contains("이미") || label.contains("없는 연결");
        h.note_step(
            &format!("연결 {label}"),
            "link_docs",
            &[parent, to],
            &named,
            args,
            if no_token { None } else { Some(parent) },
        );
    }
    h.note_step(
        "연결 제목이 공백인 노트에 더함",
        "link_docs",
        &[odd, "ndfx0004-00000004"],
        &[("kind", "indirect")],
        json!({"from": odd, "to": "ndfx0004-00000004", "kind": "indirect"}),
        Some(odd),
    );
    h.note_step(
        "연결 있던 것 빼기",
        "link_docs",
        &[odd, "ndfx0002-00000002"],
        &[("remove", "true")],
        json!({"from": odd, "to": "ndfx0002-00000002", "remove": true}),
        Some(odd),
    );
    for (label, positional, named, args) in [
        (
            "자기 자신",
            vec![parent, parent],
            vec![],
            json!({"from": parent, "to": parent}),
        ),
        (
            "모르는 종류",
            vec![parent, loose],
            vec![("kind", "zzz")],
            json!({"from": parent, "to": loose, "kind": "zzz"}),
        ),
        (
            "remove 글",
            vec![parent, loose],
            vec![("remove", "maybe")],
            json!({"from": parent, "to": loose, "remove": "maybe"}),
        ),
        ("대상 없음", vec![parent], vec![], json!({"from": parent})),
        ("출발 없음", vec![], vec![], json!({})),
        (
            "출발 모양",
            vec!["nope", loose],
            vec![],
            json!({"from": "nope", "to": loose}),
        ),
        (
            "대상 모양",
            vec![parent, "nope"],
            vec![],
            json!({"from": parent, "to": "nope"}),
        ),
        (
            "없는 출발",
            vec!["ndzz0001-00000001", loose],
            vec![],
            json!({"from": "ndzz0001-00000001", "to": loose}),
        ),
        (
            "확인 값 없음",
            vec![parent, loose],
            vec![],
            json!({"from": parent, "to": loose}),
        ),
    ] {
        h.note_step(
            &format!("연결 오류 {label}"),
            "link_docs",
            &positional,
            &named,
            args,
            None,
        );
    }

    compare_doc_updates(&mut h);
    compare_doc_creates(&mut h);

    // ---- init_workspace: 빈 폴더 쌍에서(폴더 이름을 같게 두려고 부모를 다르게 한다) ----
    let pair = |n: u32| -> (PathBuf, PathBuf) {
        let (a, b) = (
            root.join(format!("ja{n}/ws")),
            root.join(format!("ru{n}/ws")),
        );
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        (a, b)
    };
    let doc_md = r##"{"category":"doc","id":"sample-doc","name":"샘플 문서","format":"markdown","desc":"설명: 콜론","body":"# 양식\n본문\n","type":"plan","builtin":true}"##;
    let doc_html = r#"{"category":"doc","id":"page","name":"페이지","format":"html","desc":"","body":"<h1>x</h1>","type":"report","builtin":false}"#;
    let doc_json = r#"{"category":"doc","id":"data","name":"데이터","format":"json","desc":"d","body":"{\"cells\":[]}","type":"build","builtin":true}"#;
    let ritual = r###"{"category":"ritual","id":"morning-ritual","name":"아침","format":"markdown","desc":"d","body":"## 아침\n","rev":"morning","icon":"Sun"}"###;
    let all = format!("[{doc_md},{doc_html},{doc_json},{ritual}]");
    let one_doc = |extra: &str| -> String {
        format!(
            r#"[{{"category":"doc","id":"a","name":"n","format":"markdown","desc":"","body":"","type":"plan"{extra}}}]"#
        )
    };
    let bad_doc = |field: &str, value: &str| -> String {
        let mut base = [
            ("id", r#""a""#),
            ("name", r#""n""#),
            ("format", r#""markdown""#),
            ("desc", r#""""#),
            ("body", r#""""#),
            ("type", r#""plan""#),
            ("builtin", "false"),
        ];
        for entry in base.iter_mut() {
            if entry.0 == field {
                entry.1 = value;
            }
        }
        let fields: Vec<String> = base.iter().map(|(k, v)| format!("\"{k}\":{v}")).collect();
        format!(r#"[{{"category":"doc",{}}}]"#, fields.join(","))
    };
    let bad_ritual = |field: &str, value: &str| -> String {
        let mut base = [
            ("id", r#""r""#),
            ("name", r#""n""#),
            ("format", r#""markdown""#),
            ("desc", r#""""#),
            ("body", r#""""#),
            ("rev", r#""morning""#),
            ("icon", r#""Sun""#),
        ];
        for entry in base.iter_mut() {
            if entry.0 == field {
                entry.1 = value;
            }
        }
        let fields: Vec<String> = base.iter().map(|(k, v)| format!("\"{k}\":{v}")).collect();
        format!(r#"[{{"category":"ritual",{}}}]"#, fields.join(","))
    };
    let (a1, b1) = pair(1);
    let mut init = WriteHarness {
        report: &mut *h.report,
        java,
        a: a1,
        b: b1,
        locks: h.locks.clone(),
        step: 500,
        succeeded: 0,
    };
    init.step(
        "이름 지정",
        &["init-workspace", "name=테스트: 워크스페이스 \"x\""],
        "init_workspace",
        json!({"name": "테스트: 워크스페이스 \"x\""}),
    );
    init.step(
        "다시 부름",
        &["init-workspace"],
        "init_workspace",
        json!({}),
    );
    let mut errors: Vec<(String, String)> = vec![
        ("글이 아님".into(), "abc".into()),
        ("객체".into(), "{}".into()),
        ("문법 오류".into(), "[1,".into()),
        ("항목이 숫자".into(), "[1]".into()),
        ("항목이 글".into(), r#"["x"]"#.into()),
        ("category 없음".into(), r#"[{"a":1}]"#.into()),
        ("category 숫자".into(), r#"[{"category":1}]"#.into()),
        ("category 모름".into(), r#"[{"category":"x"}]"#.into()),
        ("모르는 키".into(), r#"[{"category":"doc","id":"a","zzz":2,"bad":1}]"#.into()),
        ("id 없음".into(), r#"[{"category":"doc"}]"#.into()),
        ("id가 객체(순서 유지)".into(), bad_doc("id", r#"{"b":1,"a":[1.0e3,x5]}"#)),
        ("builtin 글".into(), bad_doc("builtin", r#""yes""#)),
        ("builtin 없음".into(), r#"[{"category":"doc","id":"a","name":"n","format":"markdown","desc":"","body":"","type":"plan"}]"#.into()),
        ("id 모양".into(), bad_doc("id", r#""a b""#)),
        ("이름이 공백".into(), bad_doc("name", r#"" ""#)),
        ("포맷 모름".into(), bad_doc("format", r#""pdf""#)),
        ("유형 모름".into(), bad_doc("type", r#""zzz""#)),
        ("안전하지 않은 id".into(), bad_doc("id", r#""..""#)),
        ("리추얼 종류".into(), bad_ritual("rev", r#""night""#)),
        ("리추얼 아이콘".into(), bad_ritual("icon", r#""1x""#)),
        ("리추얼 키 섞임".into(), r#"[{"category":"ritual","id":"r","name":"n","format":"markdown","desc":"","body":"","rev":"morning","icon":"Sun","type":"plan"}]"#.into()),
        ("65개".into(), format!("[{}]", vec![one_doc("").trim_matches(['[', ']']).to_string(); 65].join(","))),
    ];
    errors.push(("앞뒤 공백".into(), format!("  {}  ", one_doc(""))));
    for (label, templates) in &errors {
        init.step(
            &format!("템플릿 오류 {label}"),
            &["init-workspace", &format!("templates={templates}")],
            "init_workspace",
            json!({"templates": templates}),
        );
    }
    init.step(
        "공백 이름",
        &["init-workspace", "name=  "],
        "init_workspace",
        json!({"name": "  "}),
    );
    let (a2, b2) = pair(2);
    let mut seeding = WriteHarness {
        report: &mut *init.report,
        java,
        a: a2,
        b: b2,
        locks: init.locks.clone(),
        step: 700,
        succeeded: 0,
    };
    seeding.step(
        "폴더 이름을 씀",
        &["init-workspace", &format!("templates={all}")],
        "init_workspace",
        json!({"templates": all}),
    );
    seeding.step(
        "다시 심음(건너뜀)",
        &["init-workspace", &format!("templates={all}")],
        "init_workspace",
        json!({"templates": all}),
    );
    seeding.step(
        "빈 배열",
        &["init-workspace", "templates=[]"],
        "init_workspace",
        json!({"templates": "[]"}),
    );
    println!(
        "  (쓰기 단계 {}개 가운데 Java가 성공으로 답한 것 {}개. 나머지는 일부러 넣은 오류 사례)",
        h.step + (init.step - 500) + (seeding.step - 700),
        h.succeeded + init.succeeded + seeding.succeeded
    );
    let _ = fs::remove_dir_all(&root);
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let verbose = args.iter().any(|a| a == "--verbose");
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let write_fixture: Option<PathBuf> = args
        .iter()
        .find_map(|a| a.strip_prefix("--write-fixture="))
        .map(PathBuf::from);
    let commit_fixture: Option<PathBuf> = args
        .iter()
        .find_map(|a| a.strip_prefix("--commit-fixture="))
        .map(PathBuf::from);
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

    if let Some(fixture) = &write_fixture {
        let mut report = Report::default();
        compare_writes(&mut report, &java, fixture);
        println!(
            "쓰기(합성 vault 사본 둘, 폴더 전체 바이트): {}건 확인, 어긋남 {}건",
            report.checked,
            report.failures.len()
        );
        for failure in &report.failures {
            println!("  - {}", failure.replace('\n', "\n    "));
        }
        total.checked += report.checked;
        total.failures.extend(report.failures);
    }

    if let Some(fixture) = &commit_fixture {
        let mut report = Report::default();
        compare_commit_vault(&mut report, &java, &|vault| Engine::new(vault), fixture);
        println!(
            "commit_vault(합성 vault 사본): {}건 확인, 어긋남 {}건",
            report.checked,
            report.failures.len()
        );
        for failure in &report.failures {
            println!("  - {}", failure.replace('\n', "\n    "));
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
