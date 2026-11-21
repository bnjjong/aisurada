//! Java 엔진(단발 모드)과 Rust 엔진이 같은 vault에서 같은 읽기 결과를 내는지 맞댑니다.
//!
//! 기준(옛 Java 엔진)은 `engine-rs/tools/compat/build-oracle.sh`가 태그 `java-engine-final`에서 지어 `engine-rs/target/oracle/`에 둔다.
//! 사용: `cargo run -p ais-core --example compat_check -- <기준 ais-engine.jar> <vault 폴더>... [--verbose] [--commit-fixture=<git 이력이 있는 합성 vault>] [--write-fixture=<합성 vault>] [--record=<테이프 파일>]`
//! `--record`는 어긋남이 0일 때만 기준(Java) 응답과 폴더 해시를 테이프에 적는다. 같은 입력을 기준 엔진 없이 다시 돌리려면
//! `cargo run -p ais-core --example compat_check -- --replay=<테이프 파일> <vault 폴더>... [같은 --write-fixture, --commit-fixture]`(골든 시험, `tests/golden.rs`).
//! `--commit-fixture`는 그 vault를 사본 둘로 복사해 `commit_vault`(vault를 고친다)를 견준다. 진짜 vault에는 주지 않는다.
//! `--write-fixture`는 그 vault를 사본 둘로 복사해 쓰기 명령을 Java와 Rust로 돌리고 응답과 폴더 전체 바이트를 견준다.
//!
//! Java 응답은 정리안의 새 모양으로 바꿔서 견줍니다: `date`·`time`·`agendaTitle`·`deepLink`를 빼고, 목록은
//! `{items, total, returned, truncated}`로 감싸고, 빈 `tags`를 채우고, `folder`를 더합니다. vault는 읽기만 합니다.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::{env, fs};

use ais_core::engine::Engine;
use serde_json::{Map, Value, json};

#[allow(dead_code)]
#[path = "support/tape.rs"]
mod tape;

/// 기준(옛 Java 엔진). 테이프가 재생 방식이면 엔진을 돌리지 않고 녹화된 응답을 돌려준다.
struct Java {
    jar: String,
    home: PathBuf,
    tape: RefCell<tape::Tape>,
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
        let key: Vec<String> = {
            let tape = self.tape.borrow();
            args.iter().map(|a| tape.norm_str(a)).collect()
        };
        if self.tape.borrow().is_replay() {
            let value = self
                .tape
                .borrow_mut()
                .next("run", &key)
                .unwrap_or_else(|e| panic!("{e}"));
            return match value.get("err") {
                Some(e) => Err(e.as_str().unwrap_or_default().to_string()),
                None => Ok(value["ok"].clone()),
            };
        }
        let result = self.launch(vault, args, props, envs);
        if self.tape.borrow().is_record() {
            let value = match &result {
                Ok(v) => json!({"ok": v}),
                Err(e) => json!({"err": e}),
            };
            self.tape.borrow_mut().push("run", key, &value);
        }
        result
    }

    /// 두 폴더가 같은지 본다(`tree_diff`). 녹화는 기준 쪽 폴더(`a`)의 파일별 해시를 적고,
    /// 재생은 `a`를 만들 기준 엔진이 없으므로 Rust 쪽 폴더(`b`)의 해시를 그 녹화와 견준다.
    fn folders(&self, lineage: &str, a: &Path, b: &Path) -> String {
        if self.tape.borrow().is_replay() {
            let recorded = self
                .tape
                .borrow_mut()
                .next_tree(lineage)
                .unwrap_or_else(|e| panic!("{e}"));
            let local = self.digests(b);
            return tape::digest_diff(&recorded, &local);
        }
        let diff = tree_diff(a, b);
        if self.tape.borrow().is_record() {
            let digests = self.digests(a);
            self.tape.borrow_mut().push_tree(lineage, &digests);
        }
        diff
    }

    /// 기준 쪽에서만 읽을 수 있는 값(예: 기준이 커밋한 git 상태)을 테이프에 싣는다. 재생은 계산하지 않고 녹화값을 쓴다.
    fn taped(&self, kind: &str, compute: impl FnOnce() -> Value) -> Value {
        if self.tape.borrow().is_replay() {
            return self
                .tape
                .borrow_mut()
                .next(kind, &[])
                .unwrap_or_else(|e| panic!("{e}"));
        }
        let value = compute();
        if self.tape.borrow().is_record() {
            self.tape.borrow_mut().push(kind, Vec::new(), &value);
        }
        value
    }

    /// 폴더의 파일별 SHA-256(`Tape::digest_files`)
    fn digests(&self, root: &Path) -> Value {
        self.tape.borrow().digest_files(&tree(root))
    }

    fn launch(
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
/// 오류 종류에서 Java의 AIS 번호를 구한다. 이 맞대기 하네스는 Java 오류 글과 견줘야 해서 코어에만 기대 표를 따로 적었다(운영 쪽 표는 `ais-cli`의 `error` 모듈).
fn java_number(kind: ais_core::error::ErrorKind) -> u16 {
    use ais_core::error::ErrorKind::*;
    match kind {
        InvalidArgument => 400,
        Forbidden => 403,
        NotFound => 404,
        PreconditionFailed => 412,
        Unsupported | Conflict | RecordTier => 409,
        Internal => 500,
    }
}

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

/// TSK-2로 `update-doc` 사용법에 더한 조각(`doc_write.rs`의 USAGE와 글자까지 같아야 한다)
const UPDATE_DOC_AGENDA_USAGE: &str =
    " [agenda=<옮길 아젠다의 slug 또는 이름. 이미 있는 아젠다만>]";

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
            // 오류는 Java의 `AIS-코드 메시지` 글과 Rust의 코드·메시지가 같은지 본다.
            // TSK-2(2026-11-20)로 update-doc 사용법에 더한 agenda 인자는 빼고 견준다
            let rust_text = format!("AIS-{} {}", java_number(r.kind), r.message)
                .replace(UPDATE_DOC_AGENDA_USAGE, "");
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
    java.tape.borrow_mut().register_root(&root);
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
        let (seen_a, seen_b) = (java.taped("observe", || observe(&a)), observe(&b));
        report.record(
            format!("commit_vault {label} 결과물"),
            seen_a == seen_b,
            || first_diff(&seen_a, &seen_b, "$"),
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
                format!(
                    "-Daisurada.approvals.dir={}",
                    self.approvals_base("java").display()
                ),
                format!("-Daisurada.claude.home={}", self.session_home().display()),
                format!("-Daisurada.codex.home={}", self.session_home().display()),
                format!("-Daisurada.gemini.home={}", self.session_home().display()),
            ],
            &[(
                "AISURADA_LOCK_DIR".to_string(),
                self.locks.display().to_string(),
            )],
        );
        // 두 사본의 폴더 경로가 오류 글에 들어가면(단일 파일 노트 따위) Java 쪽 경로를 Rust 사본 경로로 바꿔 견준다
        let (a_path, b_path) = (
            self.a.to_string_lossy().to_string(),
            self.b.to_string_lossy().to_string(),
        );
        let java_out = java_out.map_err(|e| e.replace(&a_path, &b_path));
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
        let (a_dir, b_dir) = (
            self.a.to_string_lossy().to_string(),
            self.b.to_string_lossy().to_string(),
        );
        compare(
            self.report,
            &vault,
            format!("{command} {label} 응답"),
            java_out,
            rust_out,
            move |_, v| {
                let mut v = v.clone();
                // 응답 안의 사본 폴더 경로(`dest`, `mappingFile` 따위)도 Rust 사본 경로로 바꿔 견준다
                fn swap(value: &mut Value, from: &str, to: &str) {
                    match value {
                        Value::String(s) if s.contains(from) => *s = s.replace(from, to),
                        Value::Array(items) => items.iter_mut().for_each(|i| swap(i, from, to)),
                        Value::Object(map) => map.values_mut().for_each(|i| swap(i, from, to)),
                        _ => {}
                    }
                }
                swap(&mut v, &a_dir, &b_dir);
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
        let diff = self.java.folders("vault", &self.a, &self.b);
        self.report
            .record(format!("{command} {label} 폴더"), diff.is_empty(), || {
                diff.clone()
            });
        // 승인 기록(vault 밖의 앱 폴더)은 두 사본이 vault 경로 해시가 다르므로 폴더 이름만 달리 견준다
        let java_dir =
            ais_core::resource::approvals::dir_under(&self.approvals_base("java"), &self.a);
        let rust_dir =
            ais_core::resource::approvals::dir_under(&self.approvals_base("rust"), &self.b);
        if java_dir.exists() || rust_dir.exists() {
            let diff = self.java.folders("approvals", &java_dir, &rust_dir);
            self.report.record(
                format!("{command} {label} 승인 기록"),
                diff.is_empty(),
                || diff.clone(),
            );
        }
        ok_value
    }

    /// 세션 기록을 읽는 합성 홈(두 엔진이 같은 폴더를 읽는다)
    fn session_home(&self) -> PathBuf {
        self.locks.parent().unwrap().join("session-home")
    }

    /// 승인 기록을 두는 바탕 폴더(두 엔진이 따로 쓴다)
    fn approvals_base(&self, who: &str) -> PathBuf {
        self.locks
            .parent()
            .unwrap()
            .join(format!("approvals-{who}"))
    }
}

/// 세션 홈의 파일마다 수정 시각을 고정한다. 지금 수정 시각의 차례(같으면 경로 차례)를 지켜서 `WRITE_NOW`보다 10분 앞에서 1초씩 늘린다.
/// 두 엔진이 같은 값을 읽고, 실행 시각과 상관없이 응답이 같아지게 한다(골든 재생).
fn pin_session_mtimes(home: &Path) {
    fn walk(dir: &Path, out: &mut Vec<(std::time::SystemTime, PathBuf)>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if let Ok(modified) = fs::metadata(&path).and_then(|m| m.modified()) {
                out.push((modified, path));
            }
        }
    }
    let mut files = Vec::new();
    walk(home, &mut files);
    files.sort();
    let base = std::time::UNIX_EPOCH
        + std::time::Duration::from_millis(WRITE_NOW.parse::<u64>().unwrap() - 600_000);
    for (rank, (_, path)) in files.iter().enumerate() {
        let when = base + std::time::Duration::from_secs(rank as u64);
        let file = fs::OpenOptions::new()
            .write(true)
            .open(path)
            .expect("세션 파일");
        file.set_modified(when).expect("수정 시각 고정");
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
    ais_core::kernel::token::compute_token(&fs::read(h.b.join(rel)).unwrap())
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

/// 후속 노트 쓰기 한 단계. `java`는 `ais-cli`의 인자 목록(자리 인자 포함)이고, `rust`는 같은 뜻의 호출 객체다.
/// `token_of`가 `(부모, id)`면 그 후속 노트의 지금 확인 값을 `f1`로 더한다.
fn sub_step(
    h: &mut WriteHarness,
    label: &str,
    command: &str,
    java: &[&str],
    rust: Value,
    token_of: Option<(&str, &str)>,
) {
    let mut java_args: Vec<String> = java.iter().map(|a| a.to_string()).collect();
    let mut args = rust;
    if let Some((parent, id)) = token_of {
        let token = Engine::new(&h.b)
            .call("get_sub_note", json!({"parent": parent, "id": id}))
            .ok()
            .and_then(|v| v["precondition"].as_str().map(str::to_string))
            .unwrap_or_default();
        java_args.push(format!("f1={token}"));
        args["f1"] = json!(token);
    }
    let refs: Vec<&str> = java_args.iter().map(String::as_str).collect();
    h.step(&format!("후속 {label}"), &refs, command, args);
}

/// `create_sub_note`, `update_sub_note`, `delete_sub_note` 맞대기.
fn compare_sub_note_writes(h: &mut WriteHarness) {
    let parent = "ndfx0002-00000002";
    let child1 = "snfx0001-00000011"; // 단일 파일 후속 노트
    let child2 = "snfx0002-00000012"; // 폴더 후속 노트(아래에 후속 노트 여러 개)
    let grand = "snfx0003-00000013";
    let deep = "snfx0004-00000014";
    let under_child2 = format!("{parent}/{child2}");
    // SAFETY: 단일 스레드이고, Java 단발 프로세스는 부모의 환경을 물려받는다.
    unsafe {
        env::set_var("TZ", "Asia/Seoul");
    }

    // ---- 만들기 ----
    sub_step(
        h,
        "만들기 최소",
        "create_sub_note",
        &[
            "create-sub-note",
            parent,
            "title=새 후속",
            "body=# 새 후속\n\n본문\n",
        ],
        json!({"parent": parent, "title": "새 후속", "body": "# 새 후속\n\n본문\n"}),
        None,
    );
    sub_step(
        h,
        "만들기 유형",
        "create_sub_note",
        &[
            "create-sub-note",
            parent,
            "title=계획",
            "body=# 계획\n",
            "type=PLAN",
        ],
        json!({"parent": parent, "title": "계획", "body": "# 계획\n", "type": "PLAN"}),
        None,
    );
    sub_step(
        h,
        "만들기 경고",
        "create_sub_note",
        &[
            "create-sub-note",
            parent,
            "title=경고 모음",
            "body=# 다른 제목\n  ## 들여쓴\n깨진\u{FFFD}글자\n",
        ],
        json!({"parent": parent, "title": "경고 모음", "body": "# 다른 제목\n  ## 들여쓴\n깨진\u{FFFD}글자\n"}),
        None,
    );
    sub_step(
        h,
        "만들기 같은 제목",
        "create_sub_note",
        &[
            "create-sub-note",
            parent,
            "title=새 후속",
            "body=# 새 후속\n",
        ],
        json!({"parent": parent, "title": "새 후속", "body": "# 새 후속\n"}),
        None,
    );
    sub_step(
        h,
        "만들기 빈 본문",
        "create_sub_note",
        &["create-sub-note", parent, "title=빈 본문", "body="],
        json!({"parent": parent, "title": "빈 본문", "body": ""}),
        None,
    );
    sub_step(
        h,
        "만들기 특수 글자 제목",
        "create_sub_note",
        &[
            "create-sub-note",
            parent,
            "title=# 해시: 'a' \"b\" 😀 / \\",
            "body=x",
        ],
        json!({"parent": parent, "title": "# 해시: 'a' \"b\" 😀 / \\", "body": "x"}),
        None,
    );
    sub_step(
        h,
        "만들기 후속 노트 아래",
        "create_sub_note",
        &[
            "create-sub-note",
            &under_child2,
            "title=손자",
            "body=# 손자\n",
        ],
        json!({"parent": under_child2, "title": "손자", "body": "# 손자\n"}),
        None,
    );
    sub_step(
        h,
        "만들기 사슬 공백과 빈 조각",
        "create_sub_note",
        &[
            "create-sub-note",
            &format!(" {parent} // {child2} "),
            "title=공백 사슬",
            "body=x",
        ],
        json!({"parent": format!(" {parent} // {child2} "), "title": "공백 사슬", "body": "x"}),
        None,
    );
    unsafe {
        env::set_var("TZ", "America/Los_Angeles");
    }
    sub_step(
        h,
        "만들기 로스앤젤레스 시간대",
        "create_sub_note",
        &["create-sub-note", parent, "title=엘에이", "body=x"],
        json!({"parent": parent, "title": "엘에이", "body": "x"}),
        None,
    );
    unsafe {
        env::set_var("TZ", "Asia/Seoul");
    }
    let chain_wrong = format!("{parent}/{grand}");
    let chain_single_file = format!("{parent}/{child1}");
    put_both(
        h,
        "notes/work/ext2.md/note.md",
        "---\nid: ndfx00c0-000000c0\nagenda: work\ntype: free\ntitle: 확장자 폴더\ncreated: '2026-11-10T09:00:00+09:00'\n---\n본문\n",
    );
    let huge = "a".repeat(1_000_000);
    let huge_arg = format!("body={huge}");
    for (label, java, rust) in [
        ("부모 없음", vec!["create-sub-note"], json!({})),
        (
            "부모 공백",
            vec!["create-sub-note", " ", "title=t", "body=b"],
            json!({"parent": " ", "title": "t", "body": "b"}),
        ),
        (
            "부모 모양",
            vec!["create-sub-note", "nope", "title=t", "body=b"],
            json!({"parent": "nope", "title": "t", "body": "b"}),
        ),
        (
            "부모에 후속 노트 id만",
            vec!["create-sub-note", child2, "title=t", "body=b"],
            json!({"parent": child2, "title": "t", "body": "b"}),
        ),
        (
            "제목 없음",
            vec!["create-sub-note", parent, "body=b"],
            json!({"parent": parent, "body": "b"}),
        ),
        (
            "본문 없음",
            vec!["create-sub-note", parent, "title=t"],
            json!({"parent": parent, "title": "t"}),
        ),
        (
            "유형 모름",
            vec!["create-sub-note", parent, "title=t", "body=b", "type=zzz"],
            json!({"parent": parent, "title": "t", "body": "b", "type": "zzz"}),
        ),
        (
            "제목 공백",
            vec!["create-sub-note", parent, "title=  ", "body=b"],
            json!({"parent": parent, "title": "  ", "body": "b"}),
        ),
        (
            "없는 메인 노트",
            vec!["create-sub-note", "ndzz0001-00000001", "title=t", "body=b"],
            json!({"parent": "ndzz0001-00000001", "title": "t", "body": "b"}),
        ),
        (
            "없는 메인 노트와 제목 없음",
            vec!["create-sub-note", "ndzz0001-00000001", "body=b"],
            json!({"parent": "ndzz0001-00000001", "body": "b"}),
        ),
        (
            "사슬에 없는 후속 노트",
            vec![
                "create-sub-note",
                &format!("{parent}/snzz0001-00000001"),
                "title=t",
                "body=b",
            ],
            json!({"parent": format!("{parent}/snzz0001-00000001"), "title": "t", "body": "b"}),
        ),
        (
            "사슬 후속 노트 모양",
            vec![
                "create-sub-note",
                &format!("{parent}/nope"),
                "title=t",
                "body=b",
            ],
            json!({"parent": format!("{parent}/nope"), "title": "t", "body": "b"}),
        ),
        (
            "사슬 부모가 아님",
            vec!["create-sub-note", &chain_wrong, "title=t", "body=b"],
            json!({"parent": chain_wrong, "title": "t", "body": "b"}),
        ),
        (
            "단일 파일 후속 노트 아래",
            vec!["create-sub-note", &chain_single_file, "title=t", "body=b"],
            json!({"parent": chain_single_file, "title": "t", "body": "b"}),
        ),
        (
            "단일 파일 메인 노트 아래",
            vec!["create-sub-note", "ndfx0005-00000005", "title=t", "body=b"],
            json!({"parent": "ndfx0005-00000005", "title": "t", "body": "b"}),
        ),
        (
            "확장자 폴더 메인 노트 아래",
            vec!["create-sub-note", "ndfx00c0-000000c0", "title=t", "body=b"],
            json!({"parent": "ndfx00c0-000000c0", "title": "t", "body": "b"}),
        ),
        (
            "저장 상한",
            vec!["create-sub-note", parent, "title=큼", &huge_arg],
            json!({"parent": parent, "title": "큼", "body": huge}),
        ),
    ] {
        sub_step(
            h,
            &format!("만들기 오류 {label}"),
            "create_sub_note",
            &java,
            rust,
            None,
        );
    }

    // ---- 고치기 ----
    sub_step(
        h,
        "고치기 제목만",
        "update_sub_note",
        &["update-sub-note", parent, child2, "title=자식 새 제목"],
        json!({"parent": parent, "id": child2, "title": "자식 새 제목"}),
        Some((parent, child2)),
    );
    sub_step(
        h,
        "고치기 본문만(제목은 첫 헤딩을 따름)",
        "update_sub_note",
        &[
            "update-sub-note",
            parent,
            child2,
            "body=# 헤딩이 제목이 됨\n\n본문\n",
        ],
        json!({"parent": parent, "id": child2, "body": "# 헤딩이 제목이 됨\n\n본문\n"}),
        Some((parent, child2)),
    );
    sub_step(
        h,
        "고치기 본문만(헤딩 없음이면 제목 그대로)",
        "update_sub_note",
        &["update-sub-note", parent, child2, "body=헤딩 없는 글\n"],
        json!({"parent": parent, "id": child2, "body": "헤딩 없는 글\n"}),
        Some((parent, child2)),
    );
    sub_step(
        h,
        "고치기 제목과 본문",
        "update_sub_note",
        &[
            "update-sub-note",
            parent,
            child2,
            "title=명시한 제목",
            "body=# 다른 헤딩\n  ## 들여쓴\n깨진\u{FFFD}글자\n",
        ],
        json!({"parent": parent, "id": child2, "title": "명시한 제목", "body": "# 다른 헤딩\n  ## 들여쓴\n깨진\u{FFFD}글자\n"}),
        Some((parent, child2)),
    );
    sub_step(
        h,
        "고치기 바뀐 것 없음",
        "update_sub_note",
        &["update-sub-note", parent, child2],
        json!({"parent": parent, "id": child2}),
        Some((parent, child2)),
    );
    sub_step(
        h,
        "고치기 태그",
        "update_sub_note",
        &[
            "update-sub-note",
            parent,
            child2,
            r#"tags=["b"," a ","a",""]"#,
        ],
        json!({"parent": parent, "id": child2, "tags": r#"["b"," a ","a",""]"#}),
        Some((parent, child2)),
    );
    sub_step(
        h,
        "고치기 태그 배열",
        "update_sub_note",
        &["update-sub-note", parent, child2, r#"tags=["x","y"]"#],
        json!({"parent": parent, "id": child2, "tags": ["x", "y"]}),
        Some((parent, child2)),
    );
    sub_step(
        h,
        "고치기 태그 비움",
        "update_sub_note",
        &["update-sub-note", parent, child2, "tags=[]"],
        json!({"parent": parent, "id": child2, "tags": "[]"}),
        Some((parent, child2)),
    );
    sub_step(
        h,
        "고치기 깊은 후속 노트",
        "update_sub_note",
        &[
            "update-sub-note",
            &under_child2,
            grand,
            "title=손자 새 제목",
        ],
        json!({"parent": under_child2, "id": grand, "title": "손자 새 제목"}),
        Some((&under_child2, grand)),
    );
    sub_step(
        h,
        "고치기 사슬 모든 조각",
        "update_sub_note",
        &[
            "update-sub-note",
            &format!("{under_child2}/{grand}"),
            deep,
            "title=틀린 부모",
        ],
        json!({"parent": format!("{under_child2}/{grand}"), "id": deep, "title": "틀린 부모"}),
        None,
    );
    // 정식 폴더 자리가 이미 차 있으면 -2
    let taken = "notes/demo/2026-11";
    let _ = taken;
    for (label, java, rust, token) in [
        ("부모 없음", vec!["update-sub-note"], json!({}), None),
        (
            "후속 노트 id 없음",
            vec!["update-sub-note", parent],
            json!({"parent": parent}),
            None,
        ),
        (
            "후속 노트 id 공백",
            vec!["update-sub-note", parent, " "],
            json!({"parent": parent, "id": " "}),
            None,
        ),
        (
            "후속 노트 id 모양",
            vec!["update-sub-note", parent, "nope"],
            json!({"parent": parent, "id": "nope"}),
            None,
        ),
        (
            "없는 후속 노트",
            vec!["update-sub-note", parent, "snzz0001-00000001"],
            json!({"parent": parent, "id": "snzz0001-00000001"}),
            None,
        ),
        (
            "바로 아래가 아님",
            vec!["update-sub-note", parent, grand],
            json!({"parent": parent, "id": grand}),
            None,
        ),
        (
            "제목 공백",
            vec!["update-sub-note", parent, child2, "title= "],
            json!({"parent": parent, "id": child2, "title": " "}),
            Some((parent, child2)),
        ),
        (
            "태그 문법",
            vec!["update-sub-note", parent, child2, "tags=[1,"],
            json!({"parent": parent, "id": child2, "tags": "[1,"}),
            Some((parent, child2)),
        ),
        (
            "태그 줄바꿈",
            vec!["update-sub-note", parent, child2, r#"tags=["a\nb"]"#],
            json!({"parent": parent, "id": child2, "tags": r#"["a\nb"]"#}),
            Some((parent, child2)),
        ),
        (
            "확인 값 없음",
            vec!["update-sub-note", parent, child2, "title=x"],
            json!({"parent": parent, "id": child2, "title": "x"}),
            None,
        ),
        (
            "확인 값 틀림",
            vec![
                "update-sub-note",
                parent,
                child2,
                "title=x",
                "f1=f1:deadbeef",
            ],
            json!({"parent": parent, "id": child2, "title": "x", "f1": "f1:deadbeef"}),
            None,
        ),
        (
            "단일 파일 후속 노트(고칠 수 없음)",
            vec!["update-sub-note", parent, child1, "title=x"],
            json!({"parent": parent, "id": child1, "title": "x"}),
            Some((parent, child1)),
        ),
    ] {
        sub_step(
            h,
            &format!("고치기 오류 {label}"),
            "update_sub_note",
            &java,
            rust,
            token,
        );
    }

    // ---- 지우기 ----
    // 부모 본문이 카드 줄을 가진 경우: 후속 노트(child2)가 부모인 때(자리도 정식 이름으로 옮겨진다)와 메인 노트가 부모인 때
    sub_step(
        h,
        "고치기 카드 줄을 넣음",
        "update_sub_note",
        &[
            "update-sub-note",
            parent,
            child2,
            &format!("body=# 자식\n[손자 카드](#sub:{grand}) \n[다른](#sub:{deep})\n끝\n"),
        ],
        json!({"parent": parent, "id": child2, "body": format!("# 자식\n[손자 카드](#sub:{grand}) \n[다른](#sub:{deep})\n끝\n")}),
        Some((parent, child2)),
    );
    sub_step(
        h,
        "지우기 후속 노트의 카드와 함께",
        "delete_sub_note",
        &["delete-sub-note", &under_child2, grand],
        json!({"parent": under_child2, "id": grand}),
        None,
    );
    sub_step(
        h,
        "지우기 카드 없이(아래 후속 노트까지)",
        "delete_sub_note",
        &["delete-sub-note", &under_child2, deep.to_string().as_str()],
        json!({"parent": under_child2, "id": deep}),
        None,
    );
    h.note_step(
        "본문 메인 노트에 카드 줄을 넣음",
        "update_doc",
        &[parent],
        &[("body", &format!("# 부모\n[첫째](#sub:{child1})\n"))],
        json!({"id": parent, "body": format!("# 부모\n[첫째](#sub:{child1})\n")}),
        Some(parent),
    );
    sub_step(
        h,
        "지우기 메인 노트의 카드와 함께(단일 파일)",
        "delete_sub_note",
        &["delete-sub-note", parent, child1],
        json!({"parent": parent, "id": child1}),
        None,
    );
    for (label, java, rust) in [
        ("부모 없음", vec!["delete-sub-note"], json!({})),
        (
            "후속 노트 id 없음",
            vec!["delete-sub-note", parent],
            json!({"parent": parent}),
        ),
        (
            "없는 후속 노트",
            vec!["delete-sub-note", parent, "snzz0001-00000001"],
            json!({"parent": parent, "id": "snzz0001-00000001"}),
        ),
        (
            "이미 지운 노트",
            vec!["delete-sub-note", parent, child1],
            json!({"parent": parent, "id": child1}),
        ),
        (
            "바로 아래가 아님",
            vec!["delete-sub-note", parent, "snfx0005-00000015"],
            json!({"parent": parent, "id": "snfx0005-00000015"}),
        ),
    ] {
        sub_step(
            h,
            &format!("지우기 오류 {label}"),
            "delete_sub_note",
            &java,
            rust,
            None,
        );
    }
    unsafe {
        env::remove_var("TZ");
    }
}

/// 코멘트 쓰기 한 단계. `java`는 `ais-cli` 인자 목록(자리 인자 포함)이고 `rust`는 같은 뜻의 호출 객체다.
fn comment_step(h: &mut WriteHarness, label: &str, command: &str, java: &[&str], rust: Value) {
    h.step(&format!("코멘트 {label}"), java, command, rust);
}

/// `create_comment`, `update_comment`, `delete_comment` 맞대기.
fn compare_comment_writes(h: &mut WriteHarness) {
    let parent = "ndfx0002-00000002";
    let child2 = "snfx0002-00000012";
    let grand = "snfx0003-00000013";
    let work_note = "ndfx0004-00000004"; // 코멘트 파일이 없는 노트(앞 단계에서 폴더가 됐을 수 있다)
    let json_note = "ndfx0006-00000006"; // JSON 본문 노트
    let big = "ndfx0020-00000020"; // 코멘트 파일이 너무 큼
    let rec = "ndfx0008-00000008"; // 코멘트 파일이 `[]`
    // SAFETY: 단일 스레드이고, Java 단발 프로세스는 부모의 환경을 물려받는다.
    unsafe {
        env::set_var("TZ", "Asia/Seoul");
    }

    // ---- 만들기 ----
    comment_step(
        h,
        "만들기 메인 노트(번호 발급)",
        "create_comment",
        &["create-comment", parent, "body=새 코멘트"],
        json!({"id": parent, "body": "새 코멘트"}),
    );
    comment_step(
        h,
        "만들기 식별자와 앵커",
        "create_comment",
        &[
            "create-comment",
            parent,
            "body=앵커 있는 코멘트",
            "comment_id=cmnew-00000001",
            "anchor_quote=가리킨 글",
        ],
        json!({"id": parent, "body": "앵커 있는 코멘트", "comment_id": "cmnew-00000001", "anchor_quote": "가리킨 글"}),
    );
    comment_step(
        h,
        "만들기 앵커가 공백(저장 안 함)",
        "create_comment",
        &[
            "create-comment",
            parent,
            "body=공백 앵커",
            "comment_id=cmnew-00000002",
            "anchor_quote=   ",
        ],
        json!({"id": parent, "body": "공백 앵커", "comment_id": "cmnew-00000002", "anchor_quote": "   "}),
    );
    comment_step(
        h,
        "만들기 식별자 공백을 다듬음",
        "create_comment",
        &[
            "create-comment",
            parent,
            "body=다듬음",
            "comment_id=  cmnew-00000003  ",
        ],
        json!({"id": parent, "body": "다듬음", "comment_id": "  cmnew-00000003  "}),
    );
    comment_step(
        h,
        "만들기 식별자 모양이 아니어도 받는다",
        "create_comment",
        &[
            "create-comment",
            parent,
            "body=이상한 식별자",
            "comment_id=weird id!",
        ],
        json!({"id": parent, "body": "이상한 식별자", "comment_id": "weird id!"}),
    );
    comment_step(
        h,
        "만들기 특수 글자 본문",
        "create_comment",
        &[
            "create-comment",
            parent,
            "body=줄\n\"따옴표\"\t\\ 😀 \u{7f}",
            "comment_id=cmnew-00000004",
        ],
        json!({"id": parent, "body": "줄\n\"따옴표\"\t\\ 😀 \u{7f}", "comment_id": "cmnew-00000004"}),
    );
    comment_step(
        h,
        "만들기 후속 노트",
        "create_comment",
        &[
            "create-comment",
            parent,
            child2,
            "body=후속 코멘트",
            "comment_id=cmnew-00000005",
        ],
        json!({"id": parent, "subId": child2, "body": "후속 코멘트", "comment_id": "cmnew-00000005"}),
    );
    comment_step(
        h,
        "만들기 후속 노트(파일이 아직 없음)",
        "create_comment",
        &["create-comment", parent, child2, "body=둘째 후속 코멘트"],
        json!({"id": parent, "subId": child2, "body": "둘째 후속 코멘트"}),
    );
    comment_step(
        h,
        "만들기 코멘트 파일이 없는 노트",
        "create_comment",
        &[
            "create-comment",
            work_note,
            "body=새 파일",
            "comment_id=cmzz-00000001",
        ],
        json!({"id": work_note, "body": "새 파일", "comment_id": "cmzz-00000001"}),
    );
    comment_step(
        h,
        "만들기 JSON 노트",
        "create_comment",
        &[
            "create-comment",
            json_note,
            "body=JSON 노트",
            "comment_id=cmjson-00000001",
        ],
        json!({"id": json_note, "body": "JSON 노트", "comment_id": "cmjson-00000001"}),
    );
    comment_step(
        h,
        "만들기 `[]` 파일에",
        "create_comment",
        &["create-comment", rec, "body=기록 노트"],
        json!({"id": rec, "body": "기록 노트"}),
    );
    let long_quote = "가".repeat(2_000);
    let too_long_quote = "가".repeat(2_001);
    let limit_body = "나".repeat(20_000);
    let over_body = "나".repeat(20_001);
    for (label, java, rust) in [
        (
            "앵커 상한(2000자)",
            vec![
                "create-comment".to_string(),
                parent.to_string(),
                "body=상한".to_string(),
                format!("anchor_quote={long_quote}"),
                "comment_id=cmlim-00000001".to_string(),
            ],
            json!({"id": parent, "body": "상한", "anchor_quote": long_quote, "comment_id": "cmlim-00000001"}),
        ),
        (
            "본문 상한(20000자)",
            vec![
                "create-comment".to_string(),
                parent.to_string(),
                format!("body={limit_body}"),
                "comment_id=cmlim-00000002".to_string(),
            ],
            json!({"id": parent, "body": limit_body, "comment_id": "cmlim-00000002"}),
        ),
        (
            "오류 앵커가 너무 김",
            vec![
                "create-comment".to_string(),
                parent.to_string(),
                "body=x".to_string(),
                format!("anchor_quote={too_long_quote}"),
            ],
            json!({"id": parent, "body": "x", "anchor_quote": too_long_quote}),
        ),
        (
            "오류 본문이 너무 김",
            vec![
                "create-comment".to_string(),
                parent.to_string(),
                format!("body={over_body}"),
            ],
            json!({"id": parent, "body": over_body}),
        ),
        (
            "오류 노트 없음",
            vec!["create-comment".to_string()],
            json!({}),
        ),
        (
            "오류 노트 공백",
            vec![
                "create-comment".to_string(),
                " ".to_string(),
                "body=x".to_string(),
            ],
            json!({"id": " ", "body": "x"}),
        ),
        (
            "오류 노트 모양",
            vec![
                "create-comment".to_string(),
                "nope".to_string(),
                "body=x".to_string(),
            ],
            json!({"id": "nope", "body": "x"}),
        ),
        (
            "오류 후속 노트 모양",
            vec![
                "create-comment".to_string(),
                parent.to_string(),
                "nope".to_string(),
                "body=x".to_string(),
            ],
            json!({"id": parent, "subId": "nope", "body": "x"}),
        ),
        (
            "오류 본문 없음",
            vec!["create-comment".to_string(), parent.to_string()],
            json!({"id": parent}),
        ),
        (
            "오류 본문 공백",
            vec![
                "create-comment".to_string(),
                parent.to_string(),
                "body=  ".to_string(),
            ],
            json!({"id": parent, "body": "  "}),
        ),
        (
            "오류 없는 노트",
            vec![
                "create-comment".to_string(),
                "ndzz0001-00000001".to_string(),
                "body=x".to_string(),
            ],
            json!({"id": "ndzz0001-00000001", "body": "x"}),
        ),
        (
            "오류 없는 후속 노트",
            vec![
                "create-comment".to_string(),
                parent.to_string(),
                "snzz0001-00000001".to_string(),
                "body=x".to_string(),
            ],
            json!({"id": parent, "subId": "snzz0001-00000001", "body": "x"}),
        ),
        (
            "오류 이미 있는 식별자",
            vec![
                "create-comment".to_string(),
                parent.to_string(),
                "body=x".to_string(),
                "comment_id=cmnew-00000001".to_string(),
            ],
            json!({"id": parent, "body": "x", "comment_id": "cmnew-00000001"}),
        ),
        (
            "오류 이미 있는 식별자(대소문자만 다름은 다른 것)",
            vec![
                "create-comment".to_string(),
                parent.to_string(),
                "body=x".to_string(),
                "comment_id=CMNEW-00000001".to_string(),
            ],
            json!({"id": parent, "body": "x", "comment_id": "CMNEW-00000001"}),
        ),
        (
            "오류 코멘트 파일이 너무 큼",
            vec![
                "create-comment".to_string(),
                big.to_string(),
                "body=x".to_string(),
            ],
            json!({"id": big, "body": "x"}),
        ),
        (
            "오류 코멘트 파일이 JSON이 아님",
            vec![
                "create-comment".to_string(),
                parent.to_string(),
                grand.to_string(),
                "body=x".to_string(),
            ],
            json!({"id": parent, "subId": grand, "body": "x"}),
        ),
    ] {
        let refs: Vec<&str> = java.iter().map(String::as_str).collect();
        comment_step(h, label, "create_comment", &refs, rust);
    }

    // ---- 고치기 ----
    comment_step(
        h,
        "고치기 본문",
        "update_comment",
        &[
            "update-comment",
            parent,
            "comment_id=cmabc-0a0b0c0d",
            "body=고친 첫 코멘트",
        ],
        json!({"id": parent, "comment_id": "cmabc-0a0b0c0d", "body": "고친 첫 코멘트"}),
    );
    comment_step(
        h,
        "고치기 이미 고친 코멘트를 또",
        "update_comment",
        &[
            "update-comment",
            parent,
            "comment_id=cmabc-1a2b3c4d",
            "body=또 고침",
        ],
        json!({"id": parent, "comment_id": "cmabc-1a2b3c4d", "body": "또 고침"}),
    );
    comment_step(
        h,
        "고치기 식별자 공백을 다듬음",
        "update_comment",
        &[
            "update-comment",
            parent,
            "comment_id=  cmnew-00000001  ",
            "body=공백 식별자",
        ],
        json!({"id": parent, "comment_id": "  cmnew-00000001  ", "body": "공백 식별자"}),
    );
    comment_step(
        h,
        "고치기 후속 노트",
        "update_comment",
        &[
            "update-comment",
            parent,
            child2,
            "comment_id=cmnew-00000005",
            "body=후속 고침",
        ],
        json!({"id": parent, "subId": child2, "comment_id": "cmnew-00000005", "body": "후속 고침"}),
    );
    for (label, java, rust) in [
        (
            "오류 식별자 없음",
            vec!["update-comment", parent, "body=x"],
            json!({"id": parent, "body": "x"}),
        ),
        (
            "오류 본문 없음",
            vec!["update-comment", parent, "comment_id=cmabc-0a0b0c0d"],
            json!({"id": parent, "comment_id": "cmabc-0a0b0c0d"}),
        ),
        (
            "오류 본문 공백",
            vec![
                "update-comment",
                parent,
                "comment_id=cmabc-0a0b0c0d",
                "body= ",
            ],
            json!({"id": parent, "comment_id": "cmabc-0a0b0c0d", "body": " "}),
        ),
        (
            "오류 그 코멘트가 없다",
            vec![
                "update-comment",
                parent,
                "comment_id=cmzz-00000000",
                "body=x",
            ],
            json!({"id": parent, "comment_id": "cmzz-00000000", "body": "x"}),
        ),
        (
            "오류 필터로 빠진 코멘트",
            vec![
                "update-comment",
                parent,
                "comment_id=cmabc-nocreated",
                "body=x",
            ],
            json!({"id": parent, "comment_id": "cmabc-nocreated", "body": "x"}),
        ),
        (
            "오류 다른 노트의 코멘트",
            vec![
                "update-comment",
                work_note,
                "comment_id=cmabc-0a0b0c0d",
                "body=x",
            ],
            json!({"id": work_note, "comment_id": "cmabc-0a0b0c0d", "body": "x"}),
        ),
    ] {
        comment_step(h, label, "update_comment", &java, rust);
    }

    // ---- 지우기: 본문에 앵커 표시를 넣어 두고 지운다 ----
    h.note_step(
        "본문 메인 노트에 앵커 표시를 넣음",
        "update_doc",
        &[parent],
        &[("body", "# 부모\n<!--aisc:cmabc-0a0b0c0d-->첫<!--/aisc:cmabc-0a0b0c0d--> 그리고 <!--aisc:cmnew-00000001-->둘<!--/aisc:cmnew-00000001-->\n")],
        json!({"id": parent, "body": "# 부모\n<!--aisc:cmabc-0a0b0c0d-->첫<!--/aisc:cmabc-0a0b0c0d--> 그리고 <!--aisc:cmnew-00000001-->둘<!--/aisc:cmnew-00000001-->\n"}),
        Some(parent),
    );
    comment_step(
        h,
        "지우기 앵커 표시와 함께",
        "delete_comment",
        &["delete-comment", parent, "comment_id=cmabc-0a0b0c0d"],
        json!({"id": parent, "comment_id": "cmabc-0a0b0c0d"}),
    );
    comment_step(
        h,
        "지우기 앵커 표시 없이",
        "delete_comment",
        &["delete-comment", parent, "comment_id=cmnew-00000002"],
        json!({"id": parent, "comment_id": "cmnew-00000002"}),
    );
    comment_step(
        h,
        "지우기 후속 노트(표시 없음)",
        "delete_comment",
        &[
            "delete-comment",
            parent,
            child2,
            "comment_id=cmnew-00000005",
        ],
        json!({"id": parent, "subId": child2, "comment_id": "cmnew-00000005"}),
    );
    // 후속 노트 본문에 앵커 표시를 넣고 지운다(그 후속 노트의 폴더가 정식 이름으로 옮겨질 수 있다)
    comment_step(
        h,
        "만들기 후속 노트(표시를 넣을 것)",
        "create_comment",
        &[
            "create-comment",
            parent,
            child2,
            "body=표시 코멘트",
            "comment_id=cmmark-00000001",
        ],
        json!({"id": parent, "subId": child2, "body": "표시 코멘트", "comment_id": "cmmark-00000001"}),
    );
    let marked = "# 자식\n<!--aisc:cmmark-00000001-->표시<!--/aisc:cmmark-00000001-->\n";
    let token = Engine::new(&h.b)
        .call("get_sub_note", json!({"parent": parent, "id": child2}))
        .ok()
        .and_then(|v| v["precondition"].as_str().map(str::to_string))
        .unwrap_or_default();
    h.step(
        "코멘트 후속 노트 본문에 앵커 표시를 넣음",
        &[
            "update-sub-note",
            parent,
            child2,
            &format!("body={marked}"),
            &format!("f1={token}"),
        ],
        "update_sub_note",
        json!({"parent": parent, "id": child2, "body": marked, "f1": token}),
    );
    comment_step(
        h,
        "지우기 후속 노트 앵커 표시와 함께",
        "delete_comment",
        &[
            "delete-comment",
            parent,
            child2,
            "comment_id=cmmark-00000001",
        ],
        json!({"id": parent, "subId": child2, "comment_id": "cmmark-00000001"}),
    );
    for (label, java, rust) in [
        (
            "오류 식별자 없음",
            vec!["delete-comment", parent],
            json!({"id": parent}),
        ),
        (
            "오류 그 코멘트가 없다",
            vec!["delete-comment", parent, "comment_id=cmzz-00000000"],
            json!({"id": parent, "comment_id": "cmzz-00000000"}),
        ),
        (
            "오류 이미 지운 코멘트",
            vec!["delete-comment", parent, "comment_id=cmabc-0a0b0c0d"],
            json!({"id": parent, "comment_id": "cmabc-0a0b0c0d"}),
        ),
        (
            "오류 JSON 노트(앵커를 못 걷어냄)",
            vec!["delete-comment", json_note, "comment_id=cmjson-00000001"],
            json!({"id": json_note, "comment_id": "cmjson-00000001"}),
        ),
    ] {
        comment_step(h, label, "delete_comment", &java, rust);
    }
    // 마지막 코멘트를 지우면 파일이 사라진다
    comment_step(
        h,
        "지우기 마지막 코멘트(파일이 사라짐)",
        "delete_comment",
        &["delete-comment", work_note, "comment_id=cmzz-00000001"],
        json!({"id": work_note, "comment_id": "cmzz-00000001"}),
    );
    // 코멘트 파일이 JSON이 아닌 후속 노트에는 쓰지 않는다(파일은 그대로 남는다)
    let root_path = Engine::new(&h.b)
        .call("get_doc", json!({"id": parent}))
        .ok()
        .and_then(|v| v["path"].as_str().map(str::to_string))
        .unwrap_or_default();
    put_both(
        h,
        &format!("{root_path}/sub-comments/{child2}.json"),
        "이건 JSON이 아니다",
    );
    comment_step(
        h,
        "오류 코멘트 파일이 JSON이 아님(후속 노트)",
        "create_comment",
        &["create-comment", parent, child2, "body=x"],
        json!({"id": parent, "subId": child2, "body": "x"}),
    );
    comment_step(
        h,
        "오류 코멘트 파일이 JSON이 아님(지우기)",
        "delete_comment",
        &[
            "delete-comment",
            parent,
            child2,
            "comment_id=cmnew-00000005",
        ],
        json!({"id": parent, "subId": child2, "comment_id": "cmnew-00000005"}),
    );
    unsafe {
        env::remove_var("TZ");
    }
}

/// 휴지통 쓰기 한 단계
fn trash_step(h: &mut WriteHarness, label: &str, command: &str, java: &[&str], rust: Value) {
    h.step(&format!("휴지통 {label}"), java, command, rust);
}

/// 문서 id로 지금 휴지통 항목 id를 찾는다(양쪽 사본이 같으므로 Rust 사본에서 읽는다).
fn trash_id_of(h: &WriteHarness, doc_id: &str) -> String {
    Engine::new(&h.b)
        .call("list_trash", Value::Null)
        .ok()
        .and_then(|v| {
            v["items"]
                .as_array()?
                .iter()
                .find(|i| i["docId"] == doc_id)
                .and_then(|i| i["trashId"].as_str().map(str::to_string))
        })
        .unwrap_or_else(|| "trzz0000-00000000".to_string())
}

/// `delete_doc`, `restore_doc`, `purge_doc`, `empty_trash` 맞대기.
fn compare_trash_writes(h: &mut WriteHarness) {
    let parent = "ndfx0002-00000002"; // 후속 노트가 달린 폴더 노트
    let loose = "ndfx0003-00000003"; // 즐겨찾기(라벨 있음)
    let html = "ndfx0005-00000005"; // 단일 .html 파일
    let json_note = "ndfx0006-00000006"; // JSON 본문 폴더 노트
    let record = "ndfx0008-00000008";
    let ext = "ndfx00c0-000000c0"; // 확장자 폴더 노트(`ext2.md/`)
    // SAFETY: 단일 스레드이고, Java 단발 프로세스는 부모의 환경을 물려받는다.
    unsafe {
        env::set_var("TZ", "Asia/Seoul");
    }

    // ---- 휴지통으로 보내기 ----
    trash_step(
        h,
        "보내기 즐겨찾기 노트(경고)",
        "delete_doc",
        &["delete-doc", loose],
        json!({"id": loose}),
    );
    trash_step(
        h,
        "보내기 후속 노트가 달린 노트",
        "delete_doc",
        &["delete-doc", parent],
        json!({"id": parent}),
    );
    trash_step(
        h,
        "보내기 단일 .html",
        "delete_doc",
        &["delete-doc", html],
        json!({"id": html}),
    );
    trash_step(
        h,
        "보내기 JSON 노트",
        "delete_doc",
        &["delete-doc", json_note],
        json!({"id": json_note}),
    );
    trash_step(
        h,
        "보내기 확장자 폴더 노트",
        "delete_doc",
        &["delete-doc", ext],
        json!({"id": ext}),
    );
    trash_step(
        h,
        "보내기 기록 층 노트",
        "delete_doc",
        &["delete-doc", record],
        json!({"id": record}),
    );
    for (label, java, rust) in [
        ("보내기 오류 노트 없음", vec!["delete-doc"], json!({})),
        (
            "보내기 오류 노트 공백",
            vec!["delete-doc", " "],
            json!({"id": " "}),
        ),
        (
            "보내기 오류 노트 모양",
            vec!["delete-doc", "nope"],
            json!({"id": "nope"}),
        ),
        (
            "보내기 오류 없는 노트",
            vec!["delete-doc", "ndzz0001-00000001"],
            json!({"id": "ndzz0001-00000001"}),
        ),
        (
            "보내기 오류 이미 보낸 노트",
            vec!["delete-doc", loose],
            json!({"id": loose}),
        ),
    ] {
        trash_step(h, label, "delete_doc", &java, rust);
    }

    // ---- 되살리기 ----
    let loose_trash = trash_id_of(h, loose);
    let parent_trash = trash_id_of(h, parent);
    let html_trash = trash_id_of(h, html);
    trash_step(
        h,
        "되살리기 즐겨찾기 노트",
        "restore_doc",
        &["restore-doc", &loose_trash],
        json!({"trashId": loose_trash}),
    );
    // 되살릴 자리에 이미 무엇이 있으면 `-restored-<꼬리>`가 붙는다
    let (restore_dir, restore_name) = {
        let item = Engine::new(&h.b)
            .call("list_trash", Value::Null)
            .ok()
            .and_then(|v| {
                v["items"]
                    .as_array()?
                    .iter()
                    .find(|i| i["trashId"] == parent_trash.as_str())
                    .cloned()
            })
            .unwrap();
        let original = item["originalPath"].as_str().unwrap().to_string();
        let name = original.rsplit('/').next().unwrap().to_string();
        let created = item["created"].as_str().unwrap();
        (format!("notes/demo/{}", &created[..7]), name)
    };
    put_both(h, &format!("{restore_dir}/{restore_name}/keep.txt"), "x\n");
    trash_step(
        h,
        "되살리기 같은 이름이 있음",
        "restore_doc",
        &["restore-doc", &parent_trash],
        json!({"trashId": parent_trash}),
    );
    trash_step(
        h,
        "되살리기 단일 .html",
        "restore_doc",
        &["restore-doc", &html_trash],
        json!({"trashId": html_trash}),
    );
    for (label, java, rust) in [
        ("되살리기 오류 항목 없음", vec!["restore-doc"], json!({})),
        (
            "되살리기 오류 항목 공백",
            vec!["restore-doc", " "],
            json!({"trashId": " "}),
        ),
        (
            "되살리기 오류 항목 모양",
            vec!["restore-doc", "nope"],
            json!({"trashId": "nope"}),
        ),
        (
            "되살리기 오류 없는 항목",
            vec!["restore-doc", "trzz0000-00000000"],
            json!({"trashId": "trzz0000-00000000"}),
        ),
        (
            "되살리기 오류 이미 되살린 항목",
            vec!["restore-doc", &loose_trash],
            json!({"trashId": loose_trash}),
        ),
    ] {
        trash_step(h, label, "restore_doc", &java, rust);
    }

    // 원래 아젠다가 사라진 노트: 새 아젠다에 노트를 만들어 보내고, 아젠다를 지운 뒤 되살리면 `recovered`로 간다
    let created = h.step(
        "휴지통 임시 아젠다에 노트를 만듦",
        &["create-doc", "agenda=임시 아젠다", "type=free", "title=임시", "body=# 임시\n", "date=2026-11-03", "time=10:00"],
        "create_doc",
        json!({"agenda": "임시 아젠다", "type": "free", "title": "임시", "body": "# 임시\n", "date": "2026-11-03", "time": "10:00"}),
    );
    let temp_note = created
        .as_ref()
        .and_then(|v| v["id"].as_str())
        .unwrap_or("ndzz0000-00000000")
        .to_string();
    trash_step(
        h,
        "보내기 임시 노트",
        "delete_doc",
        &["delete-doc", &temp_note],
        json!({"id": temp_note}),
    );
    let agenda_id = Engine::new(&h.b)
        .call("list_agendas", Value::Null)
        .ok()
        .and_then(|v| {
            v["items"]
                .as_array()?
                .iter()
                .find(|a| a["slug"] == "임시-아젠다")
                .and_then(|a| a["id"].as_str().map(str::to_string))
        })
        .unwrap_or_default();
    h.step(
        "휴지통 임시 아젠다를 지움",
        &["delete-agenda", &agenda_id],
        "delete_agenda",
        json!({"id": agenda_id}),
    );
    let temp_trash = trash_id_of(h, &temp_note);
    trash_step(
        h,
        "되살리기 아젠다가 사라진 노트(recovered로)",
        "restore_doc",
        &["restore-doc", &temp_trash],
        json!({"trashId": temp_trash}),
    );

    // ---- 보관 파일이 사라진 항목, 영구 삭제, 비우기 ----
    // 픽스처의 항목들은 보관 파일이 없다. 첫 항목에는 보관 파일을 만들어 영구 삭제하고, 다른 항목은 되살리다 막힌다
    put_both(
        h,
        ".aisurada/trash/files/trfx0001-000000b1/note.html",
        "<!--aisurada\nid: ndfx0100-00000100\n-->\n<p>x</p>",
    );
    trash_step(
        h,
        "영구 삭제 보관물과 함께",
        "purge_doc",
        &["purge-doc", "trfx0001-000000b1"],
        json!({"trashId": "trfx0001-000000b1"}),
    );
    for (label, java, rust) in [
        ("영구 삭제 오류 항목 없음", vec!["purge-doc"], json!({})),
        (
            "영구 삭제 오류 항목 모양",
            vec!["purge-doc", "nope"],
            json!({"trashId": "nope"}),
        ),
        (
            "영구 삭제 오류 없는 항목",
            vec!["purge-doc", "trzz0000-00000000"],
            json!({"trashId": "trzz0000-00000000"}),
        ),
        (
            "되살리기 오류 보관 파일이 사라짐(목록이 정리됨)",
            vec!["restore-doc", "trfx0002-000000b2"],
            json!({"trashId": "trfx0002-000000b2"}),
        ),
    ] {
        let command = if label.starts_with("영구") {
            "purge_doc"
        } else {
            "restore_doc"
        };
        trash_step(h, label, command, &java, rust);
    }
    trash_step(
        h,
        "영구 삭제 보관 파일이 없는 항목",
        "purge_doc",
        &["purge-doc", "trfx0003-000000b3"],
        json!({"trashId": "trfx0003-000000b3"}),
    );
    put_both(h, ".aisurada/trash/files/orphan.txt", "고아\n");
    trash_step(h, "비우기", "empty_trash", &["empty-trash"], json!({}));
    trash_step(
        h,
        "비우기 이미 빔",
        "empty_trash",
        &["empty-trash"],
        json!({}),
    );

    // ---- 목록이 1,000,000바이트를 넘으면 가장 오래된 항목부터 영구 삭제 ----
    let mut manifest = String::from("[\n");
    for n in 0..340u32 {
        if n > 0 {
            manifest.push_str(",\n");
        }
        manifest.push_str(&format!(
            "  {{\"trashId\": \"trbg{n:04x}-0000{n:04x}\", \"file\": \"trbg{n:04x}-0000{n:04x}.md\", \"originalPath\": \"notes/demo/2026-11/big{n}\", \"docId\": \"ndbg{n:04x}-0000{n:04x}\", \"agendaId\": \"agfx0001-0000000a\", \"agendaSlug\": \"demo\", \"title\": \"{}\", \"type\": \"free\", \"format\": \"markdown\", \"created\": \"2026-11-01T09:00:00+09:00\", \"trashedAt\": \"2026-11-02T01:00:00.000Z\"}}",
            "가".repeat(1000)
        ));
    }
    manifest.push_str("\n]\n");
    put_both(h, ".aisurada/trash/trash.json", &manifest);
    put_both(h, ".aisurada/trash/files/trbg0153-00000153.md", "보관물\n");
    trash_step(
        h,
        "보내기 목록이 1MB를 넘음(가장 오래된 항목이 영구 삭제됨)",
        "delete_doc",
        &["delete-doc", "ndfx0009-00000009"],
        json!({"id": "ndfx0009-00000009"}),
    );
    unsafe {
        env::remove_var("TZ");
    }
}

/// 태스크 명령 한 단계. `java`는 `ais-cli` 인자 목록(자리 인자 포함)이고 `rust`는 같은 뜻의 호출 객체다.
/// `token_of`가 태스크 키면 그 태스크의 지금 확인 값을 `f1`로 더한다.
fn task_step(
    h: &mut WriteHarness,
    label: &str,
    command: &str,
    java: &[&str],
    rust: Value,
    token_of: Option<&str>,
) -> Option<Value> {
    let mut java_args: Vec<String> = java.iter().map(|a| a.to_string()).collect();
    let mut args = rust;
    if let Some(key) = token_of {
        let token = Engine::new(&h.b)
            .call("get_task", json!({"id": key, "activity": 0}))
            .ok()
            .and_then(|v| v["precondition"].as_str().map(str::to_string))
            .unwrap_or_default();
        java_args.push(format!("f1={token}"));
        args["f1"] = json!(token);
    }
    let refs: Vec<&str> = java_args.iter().map(String::as_str).collect();
    h.step(&format!("태스크 {label}"), &refs, command, args)
}

/// 태스크 명령 맞대기: 만들기·읽기·고치기·상태·연결·댓글·휴지통. 태스크 파일은 진짜 SnakeYAML이 쓴 바이트와 견준다.
fn compare_task_commands(h: &mut WriteHarness) {
    // SAFETY: 단일 스레드이고, Java 단발 프로세스는 부모의 환경을 물려받는다.
    unsafe {
        env::set_var("TZ", "Asia/Seoul");
    }
    let crit = r#"["첫째 기준","둘째 기준"]"#;

    // ---- 만들기 ----
    task_step(
        h,
        "만들기 최소",
        "create_task",
        &[
            "create-task",
            "title=기본 작업",
            "repo=owner/name",
            &format!("criteria={crit}"),
        ],
        json!({"title": "기본 작업", "repo": "owner/name", "criteria": crit}),
        None,
    );
    task_step(
        h,
        "만들기 모든 값",
        "create_task",
        &[
            "create-task",
            "title=  모든 값  ",
            "repo=my-org/my.repo_1",
            "criteria=[\"하나\"]",
            "type=BUG",
            "priority=High",
            "project=alpha",
            "base=release/1.0",
            "body=# 설명\n\n본문 두 줄\n끝",
            r#"labels=["백엔드"," api ","백엔드","긴급"]"#,
            r#"refs=[{"kind":"note","target":"ndfx0002-00000002"},{"kind":"github","target":"owner/name#53","title":"  이슈 제목 "},{"kind":"github","target":"https://github.com/owner/name/pull/7"}]"#,
            "actor=RUNNER",
        ],
        json!({"title": "  모든 값  ", "repo": "my-org/my.repo_1", "criteria": "[\"하나\"]", "type": "BUG", "priority": "High", "project": "alpha", "base": "release/1.0",
               "body": "# 설명\n\n본문 두 줄\n끝", "labels": r#"["백엔드"," api ","백엔드","긴급"]"#,
               "refs": r#"[{"kind":"note","target":"ndfx0002-00000002"},{"kind":"github","target":"owner/name#53","title":"  이슈 제목 "},{"kind":"github","target":"https://github.com/owner/name/pull/7"}]"#, "actor": "RUNNER"}),
        None,
    );
    let special_title = "특수: 'a' \"b\" # 해시 - 대시 [대괄호] {중괄호} & 앰퍼 😀";
    let special_crit = r#"[{"text":"체크됨: 예","done":true,"checkedAt":"2026-11-01T09:00:00+09:00","checkedBy":"human"},{"text":"안 됨","done":false,"checkedAt":"무시됨"},"- 대시로 시작","? 물음표","true","007","2026-11-05","@at","%pct","  앞뒤 공백  ","탭\t포함","yes","null","~","a: b","a #b"]"#;
    task_step(
        h,
        "만들기 특수 글자",
        "create_task",
        &[
            "create-task",
            &format!("title={special_title}"),
            "repo=owner/name",
            &format!("criteria={special_crit}"),
            "body=줄1\r\n줄2\n\n끝  ",
        ],
        json!({"title": special_title, "repo": "owner/name", "criteria": special_crit, "body": "줄1\r\n줄2\n\n끝  "}),
        None,
    );
    task_step(
        h,
        "만들기 배열로",
        "create_task",
        &[
            "create-task",
            "title=배열 인자",
            "repo=owner/name",
            r#"criteria=["a",{"text":"b","done":true}]"#,
            r#"labels=["x","y"]"#,
            r#"refs=[{"kind":"note","target":"snfx0001-00000011"}]"#,
        ],
        json!({"title": "배열 인자", "repo": "owner/name", "criteria": ["a", {"text": "b", "done": true}], "labels": ["x", "y"], "refs": [{"kind": "note", "target": "snfx0001-00000011"}]}),
        None,
    );
    let weird_labels = r###"["yes","null","123","a: b","#x","~","true","2026-11-05","- d","? q","!t","&a","*a","|b",">c","%p","@a","`b"]"###;
    task_step(
        h,
        "만들기 라벨이 YAML 예약어",
        "create_task",
        &[
            "create-task",
            "title=라벨 시험",
            "repo=owner/name",
            "criteria=[\"x\"]",
            &format!("labels={weird_labels}"),
        ],
        json!({"title": "라벨 시험", "repo": "owner/name", "criteria": "[\"x\"]", "labels": weird_labels}),
        None,
    );
    task_step(
        h,
        "만들기 설명이 비었고 제목이 한 글자",
        "create_task",
        &[
            "create-task",
            "title=가",
            "repo=a/b",
            "criteria=[\"x\"]",
            "body=",
        ],
        json!({"title": "가", "repo": "a/b", "criteria": "[\"x\"]", "body": ""}),
        None,
    );
    let long_title = "가".repeat(200);
    task_step(
        h,
        "만들기 제목 200자",
        "create_task",
        &[
            "create-task",
            &format!("title={long_title}"),
            "repo=a/b",
            "criteria=[\"x\"]",
        ],
        json!({"title": long_title, "repo": "a/b", "criteria": "[\"x\"]"}),
        None,
    );
    let many_crit = format!(
        "[{}]",
        (0..50)
            .map(|n| format!("\"기준 {n}\""))
            .collect::<Vec<_>>()
            .join(",")
    );
    task_step(
        h,
        "만들기 완료 기준 50개",
        "create_task",
        &[
            "create-task",
            "title=기준 많음",
            "repo=a/b",
            &format!("criteria={many_crit}"),
        ],
        json!({"title": "기준 많음", "repo": "a/b", "criteria": many_crit}),
        None,
    );
    let over_crit = format!(
        "[{}]",
        (0..51)
            .map(|n| format!("\"기준 {n}\""))
            .collect::<Vec<_>>()
            .join(",")
    );
    let over_title = "가".repeat(201);
    let long_crit = format!("[\"{}\"]", "나".repeat(501));
    let long_label = format!("[\"{}\"]", "다".repeat(31));
    let many_labels = format!(
        "[{}]",
        (0..21)
            .map(|n| format!("\"l{n}\""))
            .collect::<Vec<_>>()
            .join(",")
    );
    let many_refs = format!(
        "[{}]",
        (0..51)
            .map(|n| format!("{{\"kind\":\"note\",\"target\":\"nd{n:04x}-0000{n:04x}\"}}"))
            .collect::<Vec<_>>()
            .join(",")
    );
    for (label, java, rust) in [
        (
            "제목 없음",
            vec!["create-task", "repo=a/b", "criteria=[\"x\"]"],
            json!({"repo": "a/b", "criteria": "[\"x\"]"}),
        ),
        (
            "제목 공백",
            vec!["create-task", "title=  ", "repo=a/b", "criteria=[\"x\"]"],
            json!({"title": "  ", "repo": "a/b", "criteria": "[\"x\"]"}),
        ),
        (
            "제목 201자",
            vec![
                "create-task",
                &format!("title={over_title}"),
                "repo=a/b",
                "criteria=[\"x\"]",
            ],
            json!({"title": over_title, "repo": "a/b", "criteria": "[\"x\"]"}),
        ),
        (
            "제목에 제어 문자",
            vec![
                "create-task",
                "title=a\u{7}b",
                "repo=a/b",
                "criteria=[\"x\"]",
            ],
            json!({"title": "a\u{7}b", "repo": "a/b", "criteria": "[\"x\"]"}),
        ),
        (
            "저장소 없음",
            vec!["create-task", "title=t", "criteria=[\"x\"]"],
            json!({"title": "t", "criteria": "[\"x\"]"}),
        ),
        (
            "저장소 빈 글",
            vec!["create-task", "title=t", "repo=", "criteria=[\"x\"]"],
            json!({"title": "t", "repo": "", "criteria": "[\"x\"]"}),
        ),
        (
            "저장소 모양",
            vec!["create-task", "title=t", "repo=noslash", "criteria=[\"x\"]"],
            json!({"title": "t", "repo": "noslash", "criteria": "[\"x\"]"}),
        ),
        (
            "저장소 점점",
            vec!["create-task", "title=t", "repo=a../b", "criteria=[\"x\"]"],
            json!({"title": "t", "repo": "a../b", "criteria": "[\"x\"]"}),
        ),
        (
            "기준 없음",
            vec!["create-task", "title=t", "repo=a/b"],
            json!({"title": "t", "repo": "a/b"}),
        ),
        (
            "기준 빈 목록",
            vec!["create-task", "title=t", "repo=a/b", "criteria=[]"],
            json!({"title": "t", "repo": "a/b", "criteria": "[]"}),
        ),
        (
            "기준 빈 글",
            vec!["create-task", "title=t", "repo=a/b", "criteria="],
            json!({"title": "t", "repo": "a/b", "criteria": ""}),
        ),
        (
            "기준 JSON 문법",
            vec!["create-task", "title=t", "repo=a/b", "criteria=[1,"],
            json!({"title": "t", "repo": "a/b", "criteria": "[1,"}),
        ),
        (
            "기준이 배열 아님",
            vec!["create-task", "title=t", "repo=a/b", "criteria={}"],
            json!({"title": "t", "repo": "a/b", "criteria": "{}"}),
        ),
        (
            "기준 항목 숫자",
            vec!["create-task", "title=t", "repo=a/b", "criteria=[1]"],
            json!({"title": "t", "repo": "a/b", "criteria": "[1]"}),
        ),
        (
            "기준 항목 모르는 키",
            vec![
                "create-task",
                "title=t",
                "repo=a/b",
                r#"criteria=[{"text":"a","zz":1}]"#,
            ],
            json!({"title": "t", "repo": "a/b", "criteria": r#"[{"text":"a","zz":1}]"#}),
        ),
        (
            "기준 done 모양",
            vec![
                "create-task",
                "title=t",
                "repo=a/b",
                r#"criteria=[{"text":"a","done":"yes"}]"#,
            ],
            json!({"title": "t", "repo": "a/b", "criteria": r#"[{"text":"a","done":"yes"}]"#}),
        ),
        (
            "기준 글 없음",
            vec![
                "create-task",
                "title=t",
                "repo=a/b",
                r#"criteria=[{"done":true}]"#,
            ],
            json!({"title": "t", "repo": "a/b", "criteria": r#"[{"done":true}]"#}),
        ),
        (
            "기준 51개",
            vec![
                "create-task",
                "title=t",
                "repo=a/b",
                &format!("criteria={over_crit}"),
            ],
            json!({"title": "t", "repo": "a/b", "criteria": over_crit}),
        ),
        (
            "기준 501자",
            vec![
                "create-task",
                "title=t",
                "repo=a/b",
                &format!("criteria={long_crit}"),
            ],
            json!({"title": "t", "repo": "a/b", "criteria": long_crit}),
        ),
        (
            "type 모름",
            vec![
                "create-task",
                "title=t",
                "repo=a/b",
                "criteria=[\"x\"]",
                "type=epic",
            ],
            json!({"title": "t", "repo": "a/b", "criteria": "[\"x\"]", "type": "epic"}),
        ),
        (
            "priority 모름",
            vec![
                "create-task",
                "title=t",
                "repo=a/b",
                "criteria=[\"x\"]",
                "priority=urgent",
            ],
            json!({"title": "t", "repo": "a/b", "criteria": "[\"x\"]", "priority": "urgent"}),
        ),
        (
            "base 모양",
            vec![
                "create-task",
                "title=t",
                "repo=a/b",
                "criteria=[\"x\"]",
                "base=-bad",
            ],
            json!({"title": "t", "repo": "a/b", "criteria": "[\"x\"]", "base": "-bad"}),
        ),
        (
            "base .lock",
            vec![
                "create-task",
                "title=t",
                "repo=a/b",
                "criteria=[\"x\"]",
                "base=x.lock",
            ],
            json!({"title": "t", "repo": "a/b", "criteria": "[\"x\"]", "base": "x.lock"}),
        ),
        (
            "프로젝트 없음",
            vec![
                "create-task",
                "title=t",
                "repo=a/b",
                "criteria=[\"x\"]",
                "project=nope",
            ],
            json!({"title": "t", "repo": "a/b", "criteria": "[\"x\"]", "project": "nope"}),
        ),
        (
            "라벨 항목 숫자",
            vec![
                "create-task",
                "title=t",
                "repo=a/b",
                "criteria=[\"x\"]",
                "labels=[1]",
            ],
            json!({"title": "t", "repo": "a/b", "criteria": "[\"x\"]", "labels": "[1]"}),
        ),
        (
            "라벨 31자",
            vec![
                "create-task",
                "title=t",
                "repo=a/b",
                "criteria=[\"x\"]",
                &format!("labels={long_label}"),
            ],
            json!({"title": "t", "repo": "a/b", "criteria": "[\"x\"]", "labels": long_label}),
        ),
        (
            "라벨 21개",
            vec![
                "create-task",
                "title=t",
                "repo=a/b",
                "criteria=[\"x\"]",
                &format!("labels={many_labels}"),
            ],
            json!({"title": "t", "repo": "a/b", "criteria": "[\"x\"]", "labels": many_labels}),
        ),
        (
            "자원 항목이 객체 아님",
            vec![
                "create-task",
                "title=t",
                "repo=a/b",
                "criteria=[\"x\"]",
                r#"refs=["x"]"#,
            ],
            json!({"title": "t", "repo": "a/b", "criteria": "[\"x\"]", "refs": r#"["x"]"#}),
        ),
        (
            "자원 모르는 키",
            vec![
                "create-task",
                "title=t",
                "repo=a/b",
                "criteria=[\"x\"]",
                r#"refs=[{"kind":"note","target":"ndfx0002-00000002","zz":1}]"#,
            ],
            json!({"title": "t", "repo": "a/b", "criteria": "[\"x\"]", "refs": r#"[{"kind":"note","target":"ndfx0002-00000002","zz":1}]"#}),
        ),
        (
            "자원 종류 모름",
            vec![
                "create-task",
                "title=t",
                "repo=a/b",
                "criteria=[\"x\"]",
                r#"refs=[{"kind":"web","target":"x"}]"#,
            ],
            json!({"title": "t", "repo": "a/b", "criteria": "[\"x\"]", "refs": r#"[{"kind":"web","target":"x"}]"#}),
        ),
        (
            "자원 대상 모양",
            vec![
                "create-task",
                "title=t",
                "repo=a/b",
                "criteria=[\"x\"]",
                r#"refs=[{"kind":"note","target":"nope"}]"#,
            ],
            json!({"title": "t", "repo": "a/b", "criteria": "[\"x\"]", "refs": r#"[{"kind":"note","target":"nope"}]"#}),
        ),
        (
            "자원 겹침",
            vec![
                "create-task",
                "title=t",
                "repo=a/b",
                "criteria=[\"x\"]",
                r#"refs=[{"kind":"note","target":"ndfx0002-00000002"},{"kind":"note","target":"ndfx0002-00000002","title":"x"}]"#,
            ],
            json!({"title": "t", "repo": "a/b", "criteria": "[\"x\"]", "refs": r#"[{"kind":"note","target":"ndfx0002-00000002"},{"kind":"note","target":"ndfx0002-00000002","title":"x"}]"#}),
        ),
        (
            "자원 51개",
            vec![
                "create-task",
                "title=t",
                "repo=a/b",
                "criteria=[\"x\"]",
                &format!("refs={many_refs}"),
            ],
            json!({"title": "t", "repo": "a/b", "criteria": "[\"x\"]", "refs": many_refs}),
        ),
        (
            "설명 10만자 넘음",
            vec![
                "create-task",
                "title=t",
                "repo=a/b",
                "criteria=[\"x\"]",
                &format!("body={}", "a".repeat(100_001)),
            ],
            json!({"title": "t", "repo": "a/b", "criteria": "[\"x\"]", "body": "a".repeat(100_001)}),
        ),
        (
            "actor 모름",
            vec![
                "create-task",
                "title=t",
                "repo=a/b",
                "criteria=[\"x\"]",
                "actor=bot",
            ],
            json!({"title": "t", "repo": "a/b", "criteria": "[\"x\"]", "actor": "bot"}),
        ),
    ] {
        task_step(
            h,
            &format!("만들기 오류 {label}"),
            "create_task",
            &java,
            rust,
            None,
        );
    }

    // ---- 읽기 ----
    for (label, java, rust) in [
        ("보기 키", vec!["get-task", "TSK-1"], json!({"id": "TSK-1"})),
        (
            "보기 활동 0",
            vec!["get-task", "TSK-2", "activity=0"],
            json!({"id": "TSK-2", "activity": "0"}),
        ),
        (
            "보기 활동 숫자로",
            vec!["get-task", "TSK-2", "activity=1"],
            json!({"id": "TSK-2", "activity": 1}),
        ),
        (
            "보기 활동 음수",
            vec!["get-task", "TSK-1", "activity=-1"],
            json!({"id": "TSK-1", "activity": "-1"}),
        ),
        (
            "보기 활동 글",
            vec!["get-task", "TSK-1", "activity=abc"],
            json!({"id": "TSK-1", "activity": "abc"}),
        ),
        (
            "보기 키 모양",
            vec!["get-task", "TSK-0"],
            json!({"id": "TSK-0"}),
        ),
        (
            "보기 없는 키",
            vec!["get-task", "TSK-999"],
            json!({"id": "TSK-999"}),
        ),
        (
            "보기 없는 id",
            vec!["get-task", "tkzz0000-00000000"],
            json!({"id": "tkzz0000-00000000"}),
        ),
        ("보기 id 없음", vec!["get-task"], json!({})),
        ("보기 공백", vec!["get-task", " "], json!({"id": " "})),
        ("목록 전체", vec!["list-tasks"], json!({})),
        (
            "목록 본문 포함",
            vec!["list-tasks", "bodies=true"],
            json!({"bodies": true}),
        ),
        (
            "목록 상태",
            vec!["list-tasks", "status=Draft"],
            json!({"status": "Draft"}),
        ),
        (
            "목록 상태 모름",
            vec!["list-tasks", "status=zzz"],
            json!({"status": "zzz"}),
        ),
        (
            "목록 유형",
            vec!["list-tasks", "type=bug"],
            json!({"type": "bug"}),
        ),
        (
            "목록 유형 모름",
            vec!["list-tasks", "type=zzz"],
            json!({"type": "zzz"}),
        ),
        (
            "목록 저장소",
            vec!["list-tasks", "repo=owner/name"],
            json!({"repo": "owner/name"}),
        ),
        (
            "목록 저장소 모양",
            vec!["list-tasks", "repo=zzz"],
            json!({"repo": "zzz"}),
        ),
        (
            "목록 라벨",
            vec!["list-tasks", "label=api"],
            json!({"label": "api"}),
        ),
        (
            "목록 글",
            vec!["list-tasks", "q=  특수"],
            json!({"q": "  특수"}),
        ),
        (
            "목록 글이 키",
            vec!["list-tasks", "q=tsk-3"],
            json!({"q": "tsk-3"}),
        ),
        (
            "목록 막힌 것만",
            vec!["list-tasks", "blocked=true"],
            json!({"blocked": "true"}),
        ),
        (
            "목록 휴지통",
            vec!["list-tasks", "trash=true"],
            json!({"trash": true}),
        ),
    ] {
        task_step(
            h,
            label,
            if label.starts_with("보기") {
                "get_task"
            } else {
                "list_tasks"
            },
            &java,
            rust,
            None,
        );
    }

    // ---- 고치기 ----
    task_step(
        h,
        "고치기 제목",
        "update_task",
        &["update-task", "TSK-1", "title=새 제목: 콜론"],
        json!({"id": "TSK-1", "title": "새 제목: 콜론"}),
        Some("TSK-1"),
    );
    task_step(
        h,
        "고치기 바뀐 것 없음",
        "update_task",
        &["update-task", "TSK-1", "title=새 제목: 콜론"],
        json!({"id": "TSK-1", "title": "새 제목: 콜론"}),
        Some("TSK-1"),
    );
    task_step(
        h,
        "고치기 모든 칸",
        "update_task",
        &[
            "update-task",
            "TSK-1",
            "title=T",
            "type=chore",
            "priority=low",
            "repo=x/y",
            "project=beta",
            "base=main",
            "body=새 본문\n두 줄",
            r#"labels=["a","b"]"#,
            r#"criteria=[{"text":"첫째 기준","done":true},"둘째 기준","셋째"]"#,
            r#"refs=[{"kind":"github","target":"x/y#1"}]"#,
            "actor=runner",
        ],
        json!({"id": "TSK-1", "title": "T", "type": "chore", "priority": "low", "repo": "x/y", "project": "beta", "base": "main", "body": "새 본문\n두 줄", "labels": r#"["a","b"]"#,
               "criteria": r#"[{"text":"첫째 기준","done":true},"둘째 기준","셋째"]"#, "refs": r#"[{"kind":"github","target":"x/y#1"}]"#, "actor": "runner"}),
        Some("TSK-1"),
    );
    task_step(
        h,
        "고치기 기준 확인을 풀고 다시 확인",
        "update_task",
        &[
            "update-task",
            "TSK-1",
            r#"criteria=[{"text":"첫째 기준","done":false},{"text":"둘째 기준","done":true,"checkedAt":"2026-11-01T00:00:00Z","checkedBy":"me"},"셋째"]"#,
        ],
        json!({"id": "TSK-1", "criteria": r#"[{"text":"첫째 기준","done":false},{"text":"둘째 기준","done":true,"checkedAt":"2026-11-01T00:00:00Z","checkedBy":"me"},"셋째"]"#}),
        Some("TSK-1"),
    );
    task_step(
        h,
        "고치기 지움(프로젝트·base·저장소·라벨·자원)",
        "update_task",
        &[
            "update-task",
            "TSK-1",
            "project=",
            "base=",
            "repo=",
            "labels=[]",
            "refs=[]",
        ],
        json!({"id": "TSK-1", "project": "", "base": "", "repo": "", "labels": "[]", "refs": "[]"}),
        Some("TSK-1"),
    );
    task_step(
        h,
        "고치기 배열 인자",
        "update_task",
        &[
            "update-task",
            "TSK-1",
            "repo=owner/name",
            r#"labels=["z"]"#,
            r#"criteria=["새 기준"]"#,
        ],
        json!({"id": "TSK-1", "repo": "owner/name", "labels": ["z"], "criteria": ["새 기준"]}),
        Some("TSK-1"),
    );
    for (label, java, rust, token) in [
        (
            "확인 값 없음",
            vec!["update-task", "TSK-1", "title=x"],
            json!({"id": "TSK-1", "title": "x"}),
            None,
        ),
        (
            "확인 값 틀림",
            vec!["update-task", "TSK-1", "title=x", "f1=f1:deadbeef"],
            json!({"id": "TSK-1", "title": "x", "f1": "f1:deadbeef"}),
            None,
        ),
        (
            "제목 공백",
            vec!["update-task", "TSK-1", "title= "],
            json!({"id": "TSK-1", "title": " "}),
            Some("TSK-1"),
        ),
        ("키 없음", vec!["update-task"], json!({}), None),
        (
            "없는 키",
            vec!["update-task", "TSK-999", "title=x"],
            json!({"id": "TSK-999", "title": "x"}),
            None,
        ),
        (
            "프로젝트 없음",
            vec!["update-task", "TSK-1", "project=nope"],
            json!({"id": "TSK-1", "project": "nope"}),
            Some("TSK-1"),
        ),
        (
            "type 모름",
            vec!["update-task", "TSK-1", "type=zzz"],
            json!({"id": "TSK-1", "type": "zzz"}),
            Some("TSK-1"),
        ),
        (
            "actor 모름",
            vec!["update-task", "TSK-1", "title=x", "actor=bot"],
            json!({"id": "TSK-1", "title": "x", "actor": "bot"}),
            Some("TSK-1"),
        ),
    ] {
        task_step(
            h,
            &format!("고치기 오류 {label}"),
            "update_task",
            &java,
            rust,
            token,
        );
    }
    // 모르는 칸이 있는 손으로 쓴 파일: 모르는 칸은 그대로 남고, 읽을 수 없는 파일은 목록에서 건너뛴다
    put_both(
        h,
        ".aisurada/tasks/TSK-20.md",
        "---\nkey: TSK-20\nid: tkex0001-00000001\ntitle: 손으로 쓴 태스크\nstatus: Draft\nrepo: a/b\nlabels:\n- 하나\n- 하나\n- ' '\ncriteria:\n- 문자열 기준\n- text: 맵 기준\n  done: 'TRUE'\n  checkedAt: 2026-11-02T10:00:00+09:00\n- {text: ' ', done: true}\nrefs:\n- {kind: note, target: ndfx0002-00000002}\n- {kind: web, target: x}\nlinks:\n- {type: blocks, to: TSK-1}\n- {type: nope, to: TSK-1}\ncustom: 5\nscore: 1.5\nflag: yes\nnested:\n  a: [1, 2]\n  b:\n    c: d\nempty:\nlist:\n- x: 1\n  y: [a, b]\n---\n\n손으로 쓴 본문\n",
    );
    put_both(h, ".aisurada/tasks/TSK-21.md", "머리말이 없다\n");
    put_both(
        h,
        ".aisurada/tasks/TSK-22.md",
        "---\nkey: TSK-23\nid: tkex0002-00000002\ntitle: 이름이 다르다\nstatus: draft\n---\n",
    );
    put_both(
        h,
        ".aisurada/tasks/TSK-24.md",
        "---\nkey: TSK-24\nid: tkex0001-00000001\ntitle: id가 겹친다\nstatus: draft\n---\n",
    );
    put_both(
        h,
        ".aisurada/tasks/TSK-25.md",
        "---\nkey: TSK-25\nid: tkex0003-00000003\ntitle: 상태 모름\nstatus: zzz\n---\n",
    );
    put_both(
        h,
        ".aisurada/tasks/TSK-26.md",
        "---\nkey: TSK-26\nid: nope\ntitle: id 모양\nstatus: draft\n---\n",
    );
    put_both(
        h,
        ".aisurada/tasks/TSK-27.md",
        &format!(
            "---\nkey: TSK-27\nid: tkex0004-00000004\ntitle: 크다\nstatus: draft\n---\n{}",
            "a".repeat(400_001)
        ),
    );
    task_step(
        h,
        "목록에 건너뛴 파일",
        "list_tasks",
        &["list-tasks"],
        json!({}),
        None,
    );
    task_step(
        h,
        "보기 손으로 쓴 태스크",
        "get_task",
        &["get-task", "TSK-20"],
        json!({"id": "TSK-20"}),
        None,
    );
    task_step(
        h,
        "고치기 손으로 쓴 태스크(모르는 칸이 남는다)",
        "update_task",
        &["update-task", "TSK-20", "title=다시 쓴 제목"],
        json!({"id": "TSK-20", "title": "다시 쓴 제목"}),
        Some("TSK-20"),
    );

    // ---- 상태 ----
    task_step(
        h,
        "상태 Draft → Ready",
        "transition_task",
        &["transition-task", "TSK-2", "to=ready-for-develop"],
        json!({"id": "TSK-2", "to": "ready-for-develop"}),
        None,
    );
    for (label, java, rust) in [
        (
            "사람이 할 수 없는 전이",
            vec!["transition-task", "TSK-2", "to=in-review"],
            json!({"id": "TSK-2", "to": "in-review"}),
        ),
        (
            "러너만 하는 전이를 사람이",
            vec![
                "transition-task",
                "TSK-3",
                "to=ready-for-develop",
                "actor=runner",
            ],
            json!({"id": "TSK-3", "to": "ready-for-develop", "actor": "runner"}),
        ),
        (
            "기대한 상태와 다름",
            vec!["transition-task", "TSK-2", "to=in-progress", "from=draft"],
            json!({"id": "TSK-2", "to": "in-progress", "from": "draft"}),
        ),
        (
            "기대한 상태 모름",
            vec!["transition-task", "TSK-2", "to=in-progress", "from=zzz"],
            json!({"id": "TSK-2", "to": "in-progress", "from": "zzz"}),
        ),
        (
            "to 없음",
            vec!["transition-task", "TSK-2"],
            json!({"id": "TSK-2"}),
        ),
        (
            "to 모름",
            vec!["transition-task", "TSK-2", "to=zzz"],
            json!({"id": "TSK-2", "to": "zzz"}),
        ),
        (
            "actor 모름",
            vec!["transition-task", "TSK-2", "to=in-progress", "actor=bot"],
            json!({"id": "TSK-2", "to": "in-progress", "actor": "bot"}),
        ),
        (
            "같은 상태로",
            vec!["transition-task", "TSK-2", "to=ready-for-develop"],
            json!({"id": "TSK-2", "to": "ready-for-develop"}),
        ),
    ] {
        task_step(
            h,
            &format!("상태 오류 {label}"),
            "transition_task",
            &java,
            rust,
            None,
        );
    }
    task_step(
        h,
        "상태 Ready → In Progress(기대한 상태 맞음)",
        "transition_task",
        &[
            "transition-task",
            "TSK-2",
            "to=In_Progress",
            "from=Ready For Develop",
        ],
        json!({"id": "TSK-2", "to": "In_Progress", "from": "Ready For Develop"}),
        None,
    );
    task_step(
        h,
        "상태 InProgress → Waiting(러너)",
        "transition_task",
        &[
            "transition-task",
            "TSK-2",
            "to=waiting-answer",
            "actor=runner",
        ],
        json!({"id": "TSK-2", "to": "waiting-answer", "actor": "runner"}),
        None,
    );
    task_step(
        h,
        "상태 Waiting → InProgress",
        "transition_task",
        &["transition-task", "TSK-2", "to=in-progress"],
        json!({"id": "TSK-2", "to": "in-progress"}),
        None,
    );
    task_step(
        h,
        "상태 InProgress → InReview(러너)",
        "transition_task",
        &["transition-task", "TSK-2", "to=in-review", "actor=runner"],
        json!({"id": "TSK-2", "to": "in-review", "actor": "runner"}),
        None,
    );
    task_step(
        h,
        "상태 InReview → Done",
        "transition_task",
        &["transition-task", "TSK-2", "to=done"],
        json!({"id": "TSK-2", "to": "done"}),
        None,
    );
    task_step(
        h,
        "상태 Draft → Cancelled",
        "transition_task",
        &["transition-task", "TSK-4", "to=cancelled"],
        json!({"id": "TSK-4", "to": "cancelled"}),
        None,
    );
    task_step(
        h,
        "상태 Cancelled → Draft",
        "transition_task",
        &["transition-task", "TSK-4", "to=draft"],
        json!({"id": "TSK-4", "to": "draft"}),
        None,
    );
    task_step(
        h,
        "상태 Ready 조건이 안 찬 것(저장소를 지움)",
        "update_task",
        &["update-task", "TSK-5", "repo="],
        json!({"id": "TSK-5", "repo": ""}),
        Some("TSK-5"),
    );
    task_step(
        h,
        "상태 오류 Ready 조건",
        "transition_task",
        &["transition-task", "TSK-5", "to=ready-for-develop"],
        json!({"id": "TSK-5", "to": "ready-for-develop"}),
        None,
    );

    // ---- 연결 ----
    task_step(
        h,
        "연결 막고 있음",
        "link_tasks",
        &["link-tasks", "TSK-3", "TSK-4", "type=blocks"],
        json!({"from": "TSK-3", "to": "TSK-4", "type": "blocks"}),
        None,
    );
    task_step(
        h,
        "연결 같은 연결을 또",
        "link_tasks",
        &["link-tasks", "TSK-3", "TSK-4", "type=blocks"],
        json!({"from": "TSK-3", "to": "TSK-4", "type": "blocks"}),
        None,
    );
    task_step(
        h,
        "연결 관련",
        "link_tasks",
        &["link-tasks", "TSK-3", "TSK-6", "type=Relates"],
        json!({"from": "TSK-3", "to": "TSK-6", "type": "Relates"}),
        None,
    );
    task_step(
        h,
        "연결 관련을 반대쪽에서(바뀐 것 없음)",
        "link_tasks",
        &["link-tasks", "TSK-6", "TSK-3", "type=relates"],
        json!({"from": "TSK-6", "to": "TSK-3", "type": "relates"}),
        None,
    );
    task_step(
        h,
        "연결 중복",
        "link_tasks",
        &["link-tasks", "tkzz", "TSK-7", "type=duplicates"],
        json!({"from": "tkzz", "to": "TSK-7", "type": "duplicates"}),
        None,
    );
    task_step(
        h,
        "새 중복 연결 등록",
        "link_tasks",
        &["link-tasks", "TSK-7", "TSK-8", "type=duplicates"],
        json!({"from": "TSK-7", "to": "TSK-8", "type": "duplicates"}),
        None,
    );
    task_step(
        h,
        "연결 막힌 태스크를 시작하려 함",
        "update_task",
        &["update-task", "TSK-4", "repo=owner/name"],
        json!({"id": "TSK-4", "repo": "owner/name"}),
        Some("TSK-4"),
    );
    task_step(
        h,
        "연결 Ready로",
        "transition_task",
        &["transition-task", "TSK-4", "to=ready-for-develop"],
        json!({"id": "TSK-4", "to": "ready-for-develop"}),
        None,
    );
    task_step(
        h,
        "연결 오류 막힌 것을 시작",
        "transition_task",
        &["transition-task", "TSK-4", "to=in-progress"],
        json!({"id": "TSK-4", "to": "in-progress"}),
        None,
    );
    task_step(
        h,
        "연결 보기(받은 쪽 이름)",
        "get_task",
        &["get-task", "TSK-4"],
        json!({"id": "TSK-4"}),
        None,
    );
    task_step(
        h,
        "연결 풀기(반대쪽에서도 풀린다)",
        "link_tasks",
        &["link-tasks", "TSK-4", "TSK-3", "type=blocks", "remove=true"],
        json!({"from": "TSK-4", "to": "TSK-3", "type": "blocks", "remove": true}),
        None,
    );
    task_step(
        h,
        "연결 풀 것이 없음",
        "link_tasks",
        &["link-tasks", "TSK-4", "TSK-3", "type=blocks", "remove=true"],
        json!({"from": "TSK-4", "to": "TSK-3", "type": "blocks", "remove": "true"}),
        None,
    );
    task_step(
        h,
        "연결 다시 걸고",
        "link_tasks",
        &["link-tasks", "TSK-3", "TSK-4", "type=blocks"],
        json!({"from": "TSK-3", "to": "TSK-4", "type": "blocks"}),
        None,
    );
    for (label, java, rust) in [
        (
            "서로 막음",
            vec!["link-tasks", "TSK-4", "TSK-3", "type=blocks"],
            json!({"from": "TSK-4", "to": "TSK-3", "type": "blocks"}),
        ),
        (
            "자기 자신",
            vec!["link-tasks", "TSK-3", "TSK-3", "type=blocks"],
            json!({"from": "TSK-3", "to": "TSK-3", "type": "blocks"}),
        ),
        (
            "종류 모름",
            vec!["link-tasks", "TSK-3", "TSK-4", "type=zzz"],
            json!({"from": "TSK-3", "to": "TSK-4", "type": "zzz"}),
        ),
        (
            "종류 없음",
            vec!["link-tasks", "TSK-3", "TSK-4"],
            json!({"from": "TSK-3", "to": "TSK-4"}),
        ),
        (
            "인자 하나",
            vec!["link-tasks", "TSK-3", "type=blocks"],
            json!({"from": "TSK-3", "type": "blocks"}),
        ),
        (
            "없는 대상",
            vec!["link-tasks", "TSK-3", "TSK-999", "type=blocks"],
            json!({"from": "TSK-3", "to": "TSK-999", "type": "blocks"}),
        ),
        (
            "키 모양",
            vec!["link-tasks", "TSK-3", "x", "type=blocks"],
            json!({"from": "TSK-3", "to": "x", "type": "blocks"}),
        ),
    ] {
        task_step(
            h,
            &format!("연결 오류 {label}"),
            "link_tasks",
            &java,
            rust,
            None,
        );
    }
    task_step(
        h,
        "연결 대상이 Done 이 되면 막힘이 풀림",
        "transition_task",
        &["transition-task", "TSK-3", "to=cancelled"],
        json!({"id": "TSK-3", "to": "cancelled"}),
        None,
    );

    // ---- 댓글 ----
    task_step(
        h,
        "댓글",
        "add_task_comment",
        &["add-task-comment", "TSK-1", "body=  댓글: 'a' \"b\" 😀  "],
        json!({"id": "TSK-1", "body": "  댓글: 'a' \"b\" 😀  "}),
        None,
    );
    task_step(
        h,
        "댓글 러너",
        "add_task_comment",
        &["add-task-comment", "tkzz", "body=x"],
        json!({"id": "tkzz", "body": "x"}),
        None,
    );
    task_step(
        h,
        "댓글 러너(유효)",
        "add_task_comment",
        &[
            "add-task-comment",
            "TSK-1",
            "body=[보고] 끝",
            "actor=runner",
        ],
        json!({"id": "TSK-1", "body": "[보고] 끝", "actor": "runner"}),
        None,
    );
    for (label, java, rust) in [
        (
            "본문 없음",
            vec!["add-task-comment", "TSK-1"],
            json!({"id": "TSK-1"}),
        ),
        (
            "본문 공백",
            vec!["add-task-comment", "TSK-1", "body=  "],
            json!({"id": "TSK-1", "body": "  "}),
        ),
        (
            "본문 2만자 넘음",
            vec![
                "add-task-comment",
                "TSK-1",
                &format!("body={}", "a".repeat(20_001)),
            ],
            json!({"id": "TSK-1", "body": "a".repeat(20_001)}),
        ),
        (
            "키 없음",
            vec!["add-task-comment", "body=x"],
            json!({"body": "x"}),
        ),
        (
            "actor 모름",
            vec!["add-task-comment", "TSK-1", "body=x", "actor=bot"],
            json!({"id": "TSK-1", "body": "x", "actor": "bot"}),
        ),
    ] {
        task_step(
            h,
            &format!("댓글 오류 {label}"),
            "add_task_comment",
            &java,
            rust,
            None,
        );
    }
    task_step(
        h,
        "보기 활동이 쌓인 태스크",
        "get_task",
        &["get-task", "TSK-1", "activity=3"],
        json!({"id": "TSK-1", "activity": 3}),
        None,
    );

    // ---- 휴지통 ----
    task_step(
        h,
        "지우기 막고 있던 태스크",
        "delete_task",
        &["delete-task", "TSK-3"],
        json!({"id": "TSK-3"}),
        None,
    );
    task_step(
        h,
        "지우기 막혀 있던 태스크(키로)",
        "delete_task",
        &["delete-task", "TSK-4"],
        json!({"id": "TSK-4"}),
        None,
    );
    task_step(
        h,
        "지우기 오류 이미 지움",
        "delete_task",
        &["delete-task", "TSK-3"],
        json!({"id": "TSK-3"}),
        None,
    );
    task_step(
        h,
        "지우기 오류 키 없음",
        "delete_task",
        &["delete-task"],
        json!({}),
        None,
    );
    task_step(
        h,
        "휴지통 목록",
        "list_tasks",
        &["list-tasks", "trash=true"],
        json!({"trash": true}),
        None,
    );
    task_step(
        h,
        "만들기 지운 번호는 다시 안 씀",
        "create_task",
        &[
            "create-task",
            "title=번호 확인",
            "repo=a/b",
            "criteria=[\"x\"]",
        ],
        json!({"title": "번호 확인", "repo": "a/b", "criteria": "[\"x\"]"}),
        None,
    );
    task_step(
        h,
        "되살리기 키로",
        "restore_task",
        &["restore-task", "TSK-3"],
        json!({"id": "TSK-3"}),
        None,
    );
    task_step(
        h,
        "되살리기 오류 휴지통에 없음",
        "restore_task",
        &["restore-task", "TSK-3"],
        json!({"id": "TSK-3"}),
        None,
    );
    task_step(
        h,
        "되살리기 오류 키 없음",
        "restore_task",
        &["restore-task"],
        json!({}),
        None,
    );
    // 제자리에 같은 키가 이미 있으면 되살리지 못한다
    let trashed = Engine::new(&h.b)
        .call("list_tasks", json!({"trash": true}))
        .ok()
        .and_then(|v| v["trash"][0]["id"].as_str().map(str::to_string))
        .unwrap_or_default();
    put_both(
        h,
        ".aisurada/tasks/TSK-4.md",
        "---\nkey: TSK-4\nid: tkcf0001-00000001\ntitle: 자리를 차지한 태스크\nstatus: draft\n---\n",
    );
    task_step(
        h,
        "되살리기 오류 제자리가 참",
        "restore_task",
        &["restore-task", &trashed],
        json!({"id": trashed}),
        None,
    );
    unsafe {
        env::remove_var("TZ");
    }
}

/// 자원 명령 한 단계. `java`는 `ais-cli` 인자 목록이고 `rust`는 같은 뜻의 호출 객체다.
/// `token_of`가 자원 id면 그 자원 파일의 지금 확인 값(`f1`)을 더한다.
fn res_step(
    h: &mut WriteHarness,
    label: &str,
    command: &str,
    java: &[&str],
    rust: Value,
    token_of: Option<&str>,
) -> Option<Value> {
    let mut java_args: Vec<String> = java.iter().map(|a| a.to_string()).collect();
    let mut args = rust;
    if let Some(id) = token_of {
        let token = res_token(h, id);
        java_args.push(format!("f1={token}"));
        args["f1"] = json!(token);
    }
    let refs: Vec<&str> = java_args.iter().map(String::as_str).collect();
    h.step(&format!("자원 {label}"), &refs, command, args)
}

/// 자원 파일의 지금 확인 값. 앞 단계에서 두 사본이 같아졌으므로 Rust 사본에서 읽는다.
fn res_token(h: &WriteHarness, id: &str) -> String {
    Engine::new(&h.b)
        .call("get_resource", json!({"id": id}))
        .ok()
        .and_then(|v| v["precondition"].as_str().map(str::to_string))
        .unwrap_or_default()
}

/// AI 통로로 Rust 사본에만 돌린다(Java 단발 모드는 AI 표식을 못 보낸다). 대기에 올라가는 요청만 쓴다:
/// 그러면 vault에 생기는 것은 대기 변경 파일뿐이라, 그 파일을 Java 사본에 그대로 옮겨 두고 승인·거절을 양쪽에서 견줄 수 있다.
fn res_ai_queue(h: &mut WriteHarness, label: &str, command: &str, mut args: Value) -> Value {
    h.step += 1;
    let n = h.step;
    let random = format!("{n:04x}0001,{n:04x}0002,{n:04x}0003,{n:04x}0004");
    // SAFETY: 단일 스레드이고, 환경 변수는 Rust 명령을 부르기 직전에만 바꾼다.
    unsafe {
        env::set_var("AISURADA_TEST_RANDOM", &random);
    }
    ais_core::write::apparatus::reset_random_queue();
    args["caller"] = json!("ai");
    args["session"] = json!("ai-session-1");
    args["sessionAgent"] = json!("codex");
    let out = Engine::new(&h.b).call(command, args);
    let queued = out.as_ref().is_ok_and(|v| v["queued"] == json!(true));
    h.report.record(
        format!("{command} 자원 AI {label} 대기"),
        queued,
        || format!("{out:?}"),
    );
    let pending = h.b.join(".aisurada/pending");
    if let Ok(entries) = fs::read_dir(&pending) {
        for entry in entries.flatten() {
            let target = h.a.join(".aisurada/pending").join(entry.file_name());
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::copy(entry.path(), target).unwrap();
        }
    }
    out.unwrap_or(Value::Null)
}

/// 자원 명령 맞대기: 만들기·고치기·참조·휴지통·승인 기록·직접 고친 파일·대기 변경의 승인/거절. 자원 파일은 진짜 SnakeYAML이 쓴 바이트와 견준다.
fn compare_resource_commands(h: &mut WriteHarness) -> String {
    // SAFETY: 단일 스레드이고, Java 단발 프로세스는 부모의 환경을 물려받는다.
    unsafe {
        env::set_var("TZ", "Asia/Seoul");
    }
    let id_of = |v: &Option<Value>| {
        v.as_ref()
            .and_then(|r| r["id"].as_str().map(str::to_string))
            .unwrap_or_default()
    };
    let path_of = |v: &Option<Value>| {
        v.as_ref()
            .and_then(|r| r["path"].as_str().map(str::to_string))
            .unwrap_or_default()
    };

    // ---- 처음 읽기: 승인 기록장 기준선이 두 사본에 같은 때에 만들어진다 ----
    res_step(
        h,
        "빈 목록",
        "list_resources",
        &["list-resources"],
        json!({}),
        None,
    );
    res_step(
        h,
        "정책 기본",
        "get_resource_policy",
        &["get-resource-policy"],
        json!({}),
        None,
    );
    res_step(
        h,
        "변경 빈 목록",
        "list_resource_changes",
        &["list-resource-changes"],
        json!({}),
        None,
    );

    // ---- 만들기 ----
    let skill_body = "# 절차\n1. 읽는다\n2. 쓴다\n";
    let s1 = id_of(&res_step(
        h,
        "글로벌 스킬",
        "save_resource",
        &[
            "save-resource",
            "kind=skill",
            "scope=global",
            "name=코드 리뷰",
            "description=PR을 검토한다",
            &format!("body={skill_body}"),
        ],
        json!({"kind": "skill", "scope": "global", "name": "코드 리뷰", "description": "PR을 검토한다", "body": skill_body}),
        None,
    ));
    let special_name = "Deploy: staging #1";
    let special_desc = "설명: 콜론 # 해시 'a' \"b\" - 대시";
    let special_body = "줄1\r\n줄2\n\n끝  \n";
    let s2 = id_of(&res_step(
        h,
        "특수 글자 스킬",
        "save_resource",
        &[
            "save-resource",
            "kind=SKILL",
            "scope=global",
            &format!("name={special_name}"),
            &format!("description={special_desc}"),
            &format!("body={special_body}"),
        ],
        json!({"kind": "SKILL", "scope": "global", "name": special_name, "description": special_desc, "body": special_body}),
        None,
    ));
    let tools = r#"{"claude":{"model":"opus","turns":5,"ratio":2.5},"codex":{"list":[1,2.5,true,null,"x: y"],"when":"2026-01-02","note":"multi\nline"}}"#;
    let skills_s1 = format!("[\"{s1}\"]");
    let a1 = id_of(&res_step(
        h,
        "글로벌 에이전트",
        "save_resource",
        &[
            "save-resource",
            "kind=agent",
            "scope=global",
            "name=리뷰어",
            "description=코드를 본다",
            "permission=Edit",
            &format!("skills={skills_s1}"),
            &format!("tools={tools}"),
            "body=지침 본문",
        ],
        json!({"kind": "agent", "scope": "global", "name": "리뷰어", "description": "코드를 본다", "permission": "Edit",
               "skills": skills_s1, "tools": tools, "body": "지침 본문"}),
        None,
    ));
    let ps1 = id_of(&res_step(
        h,
        "프로젝트 스킬",
        "save_resource",
        &[
            "save-resource",
            "kind=skill",
            "project=alpha",
            "name=알파 스킬",
            "description=알파 전용",
        ],
        json!({"kind": "skill", "project": "alpha", "name": "알파 스킬", "description": "알파 전용"}),
        None,
    ));
    let pskills = format!("[\"{s1}\",\"{ps1}\"]");
    let pa1 = id_of(&res_step(
        h,
        "프로젝트 에이전트",
        "save_resource",
        &[
            "save-resource",
            "kind=agent",
            "project=alpha",
            "name=알파 에이전트",
            "description=알파를 맡는다",
            "permission=publish",
            &format!("skills={pskills}"),
            "repo=web",
        ],
        json!({"kind": "agent", "project": "alpha", "name": "알파 에이전트", "description": "알파를 맡는다",
               "permission": "publish", "skills": pskills, "repo": "web"}),
        None,
    ));

    // ---- 만들기 오류 ----
    let long_name = "가".repeat(61);
    let bad_skills_dup = format!("[\"{s1}\",\"{s1}\"]");
    let unknown_skill = "[\"sk999-0123abcd\"]".to_string();
    let proj_skill_for_global = format!("[\"{ps1}\"]");
    for (label, java, rust) in [
        (
            "종류 없음",
            vec!["scope=global", "name=x", "description=y"],
            json!({"scope": "global", "name": "x", "description": "y"}),
        ),
        (
            "종류 모름",
            vec!["kind=zzz", "scope=global", "name=x", "description=y"],
            json!({"kind": "zzz", "scope": "global", "name": "x", "description": "y"}),
        ),
        (
            "범위 없음",
            vec!["kind=skill", "name=x", "description=y"],
            json!({"kind": "skill", "name": "x", "description": "y"}),
        ),
        (
            "범위 둘",
            vec![
                "kind=skill",
                "scope=global",
                "project=alpha",
                "name=x",
                "description=y",
            ],
            json!({"kind": "skill", "scope": "global", "project": "alpha", "name": "x", "description": "y"}),
        ),
        (
            "범위 값",
            vec!["kind=skill", "scope=weird", "name=x", "description=y"],
            json!({"kind": "skill", "scope": "weird", "name": "x", "description": "y"}),
        ),
        (
            "없는 프로젝트",
            vec!["kind=skill", "project=nope", "name=x", "description=y"],
            json!({"kind": "skill", "project": "nope", "name": "x", "description": "y"}),
        ),
        (
            "이름 없음",
            vec!["kind=skill", "scope=global", "description=y"],
            json!({"kind": "skill", "scope": "global", "description": "y"}),
        ),
        (
            "설명 없음",
            vec!["kind=skill", "scope=global", "name=x"],
            json!({"kind": "skill", "scope": "global", "name": "x"}),
        ),
        (
            "이름 빈 값",
            vec!["kind=skill", "scope=global", "name=  ", "description=y"],
            json!({"kind": "skill", "scope": "global", "name": "  ", "description": "y"}),
        ),
        (
            "이름 길이",
            vec![
                "kind=skill",
                "scope=global",
                &format!("name={long_name}"),
                "description=y",
            ],
            json!({"kind": "skill", "scope": "global", "name": long_name, "description": "y"}),
        ),
        (
            "이름 슬래시",
            vec!["kind=skill", "scope=global", "name=a/b", "description=y"],
            json!({"kind": "skill", "scope": "global", "name": "a/b", "description": "y"}),
        ),
        (
            "이름 점만",
            vec!["kind=skill", "scope=global", "name=...", "description=y"],
            json!({"kind": "skill", "scope": "global", "name": "...", "description": "y"}),
        ),
        (
            "설명 빈 값",
            vec!["kind=skill", "scope=global", "name=x", "description= "],
            json!({"kind": "skill", "scope": "global", "name": "x", "description": " "}),
        ),
        (
            "같은 이름",
            vec![
                "kind=skill",
                "scope=global",
                "name=코드 리뷰",
                "description=y",
            ],
            json!({"kind": "skill", "scope": "global", "name": "코드 리뷰", "description": "y"}),
        ),
        (
            "스킬에 권한",
            vec![
                "kind=skill",
                "scope=global",
                "name=x",
                "description=y",
                "permission=read",
            ],
            json!({"kind": "skill", "scope": "global", "name": "x", "description": "y", "permission": "read"}),
        ),
        (
            "스킬에 저장소",
            vec![
                "kind=skill",
                "scope=global",
                "name=x",
                "description=y",
                "repo=r",
            ],
            json!({"kind": "skill", "scope": "global", "name": "x", "description": "y", "repo": "r"}),
        ),
        (
            "권한 모름",
            vec![
                "kind=agent",
                "scope=global",
                "name=x",
                "description=y",
                "permission=root",
            ],
            json!({"kind": "agent", "scope": "global", "name": "x", "description": "y", "permission": "root"}),
        ),
        (
            "skills 배열 아님",
            vec![
                "kind=agent",
                "scope=global",
                "name=x",
                "description=y",
                "skills={}",
            ],
            json!({"kind": "agent", "scope": "global", "name": "x", "description": "y", "skills": "{}"}),
        ),
        (
            "skills 깨진 JSON",
            vec![
                "kind=agent",
                "scope=global",
                "name=x",
                "description=y",
                "skills=[\"a\"",
            ],
            json!({"kind": "agent", "scope": "global", "name": "x", "description": "y", "skills": "[\"a\""}),
        ),
        (
            "skills 항목 모양",
            vec![
                "kind=agent",
                "scope=global",
                "name=x",
                "description=y",
                "skills=[1]",
            ],
            json!({"kind": "agent", "scope": "global", "name": "x", "description": "y", "skills": "[1]"}),
        ),
        (
            "skills 겹침",
            vec![
                "kind=agent",
                "scope=global",
                "name=x",
                "description=y",
                &format!("skills={bad_skills_dup}"),
            ],
            json!({"kind": "agent", "scope": "global", "name": "x", "description": "y", "skills": bad_skills_dup}),
        ),
        (
            "skills 없는 스킬",
            vec![
                "kind=agent",
                "scope=global",
                "name=x",
                "description=y",
                &format!("skills={unknown_skill}"),
            ],
            json!({"kind": "agent", "scope": "global", "name": "x", "description": "y", "skills": unknown_skill}),
        ),
        (
            "글로벌 에이전트가 프로젝트 스킬",
            vec![
                "kind=agent",
                "scope=global",
                "name=x",
                "description=y",
                &format!("skills={proj_skill_for_global}"),
            ],
            json!({"kind": "agent", "scope": "global", "name": "x", "description": "y", "skills": proj_skill_for_global}),
        ),
        (
            "글로벌 에이전트에 저장소",
            vec![
                "kind=agent",
                "scope=global",
                "name=x",
                "description=y",
                "repo=web",
            ],
            json!({"kind": "agent", "scope": "global", "name": "x", "description": "y", "repo": "web"}),
        ),
        (
            "저장소 이름 틀림",
            vec![
                "kind=agent",
                "project=alpha",
                "name=x",
                "description=y",
                "repo=nope",
            ],
            json!({"kind": "agent", "project": "alpha", "name": "x", "description": "y", "repo": "nope"}),
        ),
        (
            "tools 객체 아님",
            vec![
                "kind=skill",
                "scope=global",
                "name=x",
                "description=y",
                "tools=[]",
            ],
            json!({"kind": "skill", "scope": "global", "name": "x", "description": "y", "tools": "[]"}),
        ),
        (
            "tools 칸 이름",
            vec![
                "kind=skill",
                "scope=global",
                "name=x",
                "description=y",
                "tools={\"Claude\":{}}",
            ],
            json!({"kind": "skill", "scope": "global", "name": "x", "description": "y", "tools": "{\"Claude\":{}}"}),
        ),
        (
            "tools 깨진 JSON",
            vec![
                "kind=skill",
                "scope=global",
                "name=x",
                "description=y",
                "tools={",
            ],
            json!({"kind": "skill", "scope": "global", "name": "x", "description": "y", "tools": "{"}),
        ),
    ] {
        let mut argv = vec!["save-resource"];
        argv.extend(java.iter().copied());
        res_step(
            h,
            &format!("만들기 오류 {label}"),
            "save_resource",
            &argv,
            rust,
            None,
        );
    }

    // ---- 읽기 ----
    res_step(
        h,
        "목록",
        "list_resources",
        &["list-resources"],
        json!({}),
        None,
    );
    res_step(
        h,
        "목록 지침",
        "list_resources",
        &["list-resources", "bodies=true"],
        json!({"bodies": "true"}),
        None,
    );
    res_step(
        h,
        "목록 글로벌만",
        "list_resources",
        &["list-resources", "scope=global"],
        json!({"scope": "global"}),
        None,
    );
    res_step(
        h,
        "목록 프로젝트",
        "list_resources",
        &["list-resources", "project=alpha", "kind=agent"],
        json!({"project": "alpha", "kind": "agent"}),
        None,
    );
    res_step(
        h,
        "목록 종류 오류",
        "list_resources",
        &["list-resources", "kind=zzz"],
        json!({"kind": "zzz"}),
        None,
    );
    res_step(
        h,
        "목록 없는 프로젝트",
        "list_resources",
        &["list-resources", "project=nope"],
        json!({"project": "nope"}),
        None,
    );
    res_step(
        h,
        "목록 범위 오류",
        "list_resources",
        &["list-resources", "scope=x"],
        json!({"scope": "x"}),
        None,
    );
    res_step(
        h,
        "조회 스킬",
        "get_resource",
        &["get-resource", &s1],
        json!({"id": s1}),
        None,
    );
    res_step(
        h,
        "조회 에이전트",
        "get_resource",
        &["get-resource", &a1],
        json!({"id": a1}),
        None,
    );
    res_step(
        h,
        "조회 id 모양",
        "get_resource",
        &["get-resource", "nope"],
        json!({"id": "nope"}),
        None,
    );
    res_step(
        h,
        "조회 없는 id",
        "get_resource",
        &["get-resource", "sk999-0123abcd"],
        json!({"id": "sk999-0123abcd"}),
        None,
    );
    res_step(
        h,
        "조회 인자 없음",
        "get_resource",
        &["get-resource"],
        json!({}),
        None,
    );
    res_step(
        h,
        "조회 판 모양",
        "get_resource",
        &["get-resource", &s1, "version=A!"],
        json!({"id": s1, "version": "A!"}),
        None,
    );
    res_step(
        h,
        "조회 없는 판",
        "get_resource",
        &["get-resource", &s1, "version=zzzz"],
        json!({"id": s1, "version": "zzzz"}),
        None,
    );
    res_step(
        h,
        "판 목록",
        "list_resource_versions",
        &["list-resource-versions", &s1],
        json!({"id": s1}),
        None,
    );
    res_step(
        h,
        "판 목록 인자 없음",
        "list_resource_versions",
        &["list-resource-versions"],
        json!({}),
        None,
    );

    // ---- 고치기 ----
    res_step(
        h,
        "고치기 f1 없음",
        "save_resource",
        &["save-resource", &format!("id={s1}"), "description=새 설명"],
        json!({"id": s1, "description": "새 설명"}),
        None,
    );
    res_step(
        h,
        "고치기 f1 낡음",
        "save_resource",
        &[
            "save-resource",
            &format!("id={s1}"),
            "description=새 설명",
            "f1=f1:낡음",
        ],
        json!({"id": s1, "description": "새 설명", "f1": "f1:낡음"}),
        None,
    );
    res_step(
        h,
        "고치기 설명·지침",
        "save_resource",
        &[
            "save-resource",
            &format!("id={s1}"),
            "description=새 설명",
            "body=고친 지침\n",
        ],
        json!({"id": s1, "description": "새 설명", "body": "고친 지침\n"}),
        Some(&s1),
    );
    res_step(
        h,
        "고치기 변화 없음",
        "save_resource",
        &["save-resource", &format!("id={s1}"), "description=새 설명"],
        json!({"id": s1, "description": "새 설명"}),
        Some(&s1),
    );
    res_step(
        h,
        "고치기 이름 바꾸기(폴더 이동)",
        "save_resource",
        &["save-resource", &format!("id={s1}"), "name=코드 리뷰 v2"],
        json!({"id": s1, "name": "코드 리뷰 v2"}),
        Some(&s1),
    );
    res_step(
        h,
        "고치기 이름 같은 이름",
        "save_resource",
        &["save-resource", &format!("id={s2}"), "name=코드 리뷰 v2"],
        json!({"id": s2, "name": "코드 리뷰 v2"}),
        Some(&s2),
    );
    res_step(
        h,
        "고치기 에이전트 권한·도구 비움",
        "save_resource",
        &[
            "save-resource",
            &format!("id={a1}"),
            "permission=publish",
            "tools=",
        ],
        json!({"id": a1, "permission": "publish", "tools": ""}),
        Some(&a1),
    );
    res_step(
        h,
        "고치기 에이전트 skills 비움",
        "save_resource",
        &["save-resource", &format!("id={a1}"), "skills="],
        json!({"id": a1, "skills": ""}),
        Some(&a1),
    );
    res_step(
        h,
        "고치기 에이전트 skills 다시",
        "save_resource",
        &[
            "save-resource",
            &format!("id={a1}"),
            &format!("skills={skills_s1}"),
        ],
        json!({"id": a1, "skills": skills_s1}),
        Some(&a1),
    );
    res_step(
        h,
        "고치기 저장소 지움",
        "save_resource",
        &["save-resource", &format!("id={pa1}"), "repo="],
        json!({"id": pa1, "repo": ""}),
        Some(&pa1),
    );
    res_step(
        h,
        "고치기 저장소 다시",
        "save_resource",
        &["save-resource", &format!("id={pa1}"), "repo=api"],
        json!({"id": pa1, "repo": "api"}),
        Some(&pa1),
    );
    res_step(
        h,
        "고치기 종류 바꿈",
        "save_resource",
        &["save-resource", &format!("id={s1}"), "kind=agent"],
        json!({"id": s1, "kind": "agent"}),
        Some(&s1),
    );
    res_step(
        h,
        "고치기 범위 바꿈",
        "save_resource",
        &["save-resource", &format!("id={s1}"), "project=alpha"],
        json!({"id": s1, "project": "alpha"}),
        Some(&s1),
    );
    res_step(
        h,
        "고치기 범위 같음",
        "save_resource",
        &[
            "save-resource",
            &format!("id={s1}"),
            "scope=global",
            "description=또 설명",
        ],
        json!({"id": s1, "scope": "global", "description": "또 설명"}),
        Some(&s1),
    );
    res_step(
        h,
        "고치기 스킬에 권한",
        "save_resource",
        &["save-resource", &format!("id={s1}"), "permission=edit"],
        json!({"id": s1, "permission": "edit"}),
        Some(&s1),
    );
    res_step(
        h,
        "고치기 id 모양",
        "save_resource",
        &["save-resource", "id=nope", "description=x"],
        json!({"id": "nope", "description": "x"}),
        None,
    );
    res_step(
        h,
        "고치기 없는 id",
        "save_resource",
        &["save-resource", "id=sk999-0123abcd", "description=x"],
        json!({"id": "sk999-0123abcd", "description": "x"}),
        None,
    );
    res_step(
        h,
        "고치기 다른 프로젝트 스킬 참조",
        "save_resource",
        &[
            "save-resource",
            &format!("id={a1}"),
            &format!("skills={proj_skill_for_global}"),
        ],
        json!({"id": a1, "skills": proj_skill_for_global}),
        Some(&a1),
    );
    res_step(
        h,
        "판 목록(고친 뒤)",
        "list_resource_versions",
        &["list-resource-versions", &s1],
        json!({"id": s1}),
        None,
    );
    res_step(
        h,
        "조회 옛 판",
        "get_resource",
        &["get-resource", &s1, "version=a"],
        json!({"id": s1, "version": "a"}),
        None,
    );
    let versions = Engine::new(&h.b)
        .call("list_resource_versions", json!({"id": s1}))
        .ok()
        .and_then(|v| v["versions"].as_array().cloned())
        .unwrap_or_default();
    for v in versions
        .iter()
        .take(3)
        .filter_map(|v| v["version"].as_str().map(str::to_string))
    {
        res_step(
            h,
            &format!("조회 판 {v}"),
            "get_resource",
            &["get-resource", &s1, &format!("version={v}")],
            json!({"id": s1, "version": v}),
            None,
        );
    }

    // ---- 참조 ----
    res_step(
        h,
        "참조 켜기",
        "set_resource_ref",
        &["set-resource-ref", "alpha", &s1],
        json!({"project": "alpha", "id": s1}),
        None,
    );
    res_step(
        h,
        "참조 또 켜기",
        "set_resource_ref",
        &["set-resource-ref", "alpha", &s1],
        json!({"project": "alpha", "id": s1}),
        None,
    );
    res_step(
        h,
        "참조 에이전트와 스킬",
        "set_resource_ref",
        &["set-resource-ref", "alpha", &a1, "with-skills=true"],
        json!({"project": "alpha", "id": a1, "with-skills": "true"}),
        None,
    );
    res_step(
        h,
        "참조 프로젝트 자원",
        "set_resource_ref",
        &["set-resource-ref", "alpha", &ps1],
        json!({"project": "alpha", "id": ps1}),
        None,
    );
    res_step(
        h,
        "참조 없는 자원",
        "set_resource_ref",
        &["set-resource-ref", "alpha", "sk999-0123abcd"],
        json!({"project": "alpha", "id": "sk999-0123abcd"}),
        None,
    );
    res_step(
        h,
        "참조 id 모양",
        "set_resource_ref",
        &["set-resource-ref", "alpha", "nope"],
        json!({"project": "alpha", "id": "nope"}),
        None,
    );
    res_step(
        h,
        "참조 없는 프로젝트",
        "set_resource_ref",
        &["set-resource-ref", "nope", &s1],
        json!({"project": "nope", "id": s1}),
        None,
    );
    res_step(
        h,
        "참조 인자 모자람",
        "set_resource_ref",
        &["set-resource-ref", "alpha"],
        json!({"project": "alpha"}),
        None,
    );
    res_step(
        h,
        "참조 끄기",
        "set_resource_ref",
        &["set-resource-ref", "alpha", &s1, "remove=true"],
        json!({"project": "alpha", "id": s1, "remove": "true"}),
        None,
    );
    res_step(
        h,
        "참조 끄기 없는 것",
        "set_resource_ref",
        &["set-resource-ref", "alpha", &s1, "remove=true"],
        json!({"project": "alpha", "id": s1, "remove": true}),
        None,
    );
    res_step(
        h,
        "참조 다시 켜기",
        "set_resource_ref",
        &["set-resource-ref", "alpha", &s1],
        json!({"project": "alpha", "id": s1}),
        None,
    );
    res_step(
        h,
        "쓸 수 있는 자원",
        "usable_resources",
        &["usable-resources", "alpha"],
        json!({"project": "alpha"}),
        None,
    );
    res_step(
        h,
        "쓸 수 있는 자원 지침",
        "usable_resources",
        &["usable-resources", "alpha", "bodies=true"],
        json!({"project": "alpha", "bodies": true}),
        None,
    );
    res_step(
        h,
        "쓸 수 있는 자원 없는 프로젝트",
        "usable_resources",
        &["usable-resources", "nope"],
        json!({"project": "nope"}),
        None,
    );
    res_step(
        h,
        "쓸 수 있는 자원 인자 없음",
        "usable_resources",
        &["usable-resources"],
        json!({}),
        None,
    );
    res_step(
        h,
        "쓸 수 있는 자원 참조 없는 프로젝트",
        "usable_resources",
        &["usable-resources", "beta"],
        json!({"project": "beta"}),
        None,
    );
    res_step(
        h,
        "조회 참조한 프로젝트",
        "get_resource",
        &["get-resource", &s1],
        json!({"id": s1}),
        None,
    );

    // ---- 지우기·되살리기 ----
    res_step(
        h,
        "지우기 참조된 글로벌 스킬",
        "delete_resource",
        &["delete-resource", &s2],
        json!({"id": s2}),
        None,
    );
    res_step(
        h,
        "지우기 프로젝트 에이전트",
        "delete_resource",
        &["delete-resource", &pa1],
        json!({"id": pa1}),
        None,
    );
    res_step(
        h,
        "지우기 없는 id",
        "delete_resource",
        &["delete-resource", "ap999-0123abcd"],
        json!({"id": "ap999-0123abcd"}),
        None,
    );
    res_step(
        h,
        "지우기 인자 없음",
        "delete_resource",
        &["delete-resource"],
        json!({}),
        None,
    );
    res_step(
        h,
        "휴지통 목록",
        "list_resources",
        &["list-resources", "trash=true"],
        json!({"trash": "true"}),
        None,
    );
    res_step(
        h,
        "휴지통 목록 필터",
        "list_resources",
        &[
            "list-resources",
            "trash=true",
            "project=alpha",
            "kind=agent",
        ],
        json!({"trash": true, "project": "alpha", "kind": "agent"}),
        None,
    );
    res_step(
        h,
        "휴지통 목록 글로벌",
        "list_resources",
        &["list-resources", "trash=true", "scope=global"],
        json!({"trash": "true", "scope": "global"}),
        None,
    );
    res_step(
        h,
        "되살리기",
        "restore_resource",
        &["restore-resource", &pa1],
        json!({"id": pa1}),
        None,
    );
    res_step(
        h,
        "되살리기 없음",
        "restore_resource",
        &["restore-resource", &pa1],
        json!({"id": pa1}),
        None,
    );
    res_step(
        h,
        "되살리기 인자 없음",
        "restore_resource",
        &["restore-resource"],
        json!({}),
        None,
    );
    res_step(
        h,
        "지우고 같은 이름 만들고 되살리기",
        "delete_resource",
        &["delete-resource", &ps1],
        json!({"id": ps1}),
        None,
    );
    let again = id_of(&res_step(
        h,
        "같은 이름 새로",
        "save_resource",
        &[
            "save-resource",
            "kind=skill",
            "project=alpha",
            "name=알파 스킬",
            "description=새로",
        ],
        json!({"kind": "skill", "project": "alpha", "name": "알파 스킬", "description": "새로"}),
        None,
    ));
    res_step(
        h,
        "되살리기 제자리 있음",
        "restore_resource",
        &["restore-resource", &ps1],
        json!({"id": ps1}),
        None,
    );
    res_step(
        h,
        "지우기 새 것",
        "delete_resource",
        &["delete-resource", &again],
        json!({"id": again}),
        None,
    );
    res_step(
        h,
        "되살리기 옛 것",
        "restore_resource",
        &["restore-resource", &ps1],
        json!({"id": ps1}),
        None,
    );
    res_step(
        h,
        "쓸 수 있는 자원(지운 뒤)",
        "usable_resources",
        &["usable-resources", "alpha"],
        json!({"project": "alpha"}),
        None,
    );

    // ---- 승인 정책 ----
    res_step(
        h,
        "정책 바꾸기",
        "set_resource_policy",
        &["set-resource-policy", "project=approve", "refs=auto"],
        json!({"project": "approve", "refs": "auto"}),
        None,
    );
    res_step(
        h,
        "정책 값 오류",
        "set_resource_policy",
        &["set-resource-policy", "global=maybe"],
        json!({"global": "maybe"}),
        None,
    );
    res_step(
        h,
        "정책 칸 없음",
        "set_resource_policy",
        &["set-resource-policy"],
        json!({}),
        None,
    );
    res_step(
        h,
        "정책 읽기",
        "get_resource_policy",
        &["get-resource-policy"],
        json!({}),
        None,
    );
    res_step(
        h,
        "정책 되돌리기",
        "set_resource_policy",
        &[
            "set-resource-policy",
            "project=auto",
            "refs=approve",
            "permission=approve",
            "global=approve",
        ],
        json!({"project": "auto", "refs": "approve", "permission": "approve", "global": "approve"}),
        None,
    );

    // ---- 직접 고친 파일: dirty(승인한 뒤 바뀜), new(승인 기록 없음), 참조 목록 ----
    let s1_path = path_of(&res_step(
        h,
        "경로 읽기",
        "get_resource",
        &["get-resource", &s1],
        json!({"id": s1}),
        None,
    ));
    let edited = format!(
        "{}\n손으로 덧붙인 줄\n",
        String::from_utf8_lossy(&fs::read(h.b.join(&s1_path)).unwrap())
    );
    put_both(h, &s1_path, &edited);
    res_step(
        h,
        "직접 고침 목록",
        "list_resources",
        &["list-resources", "bodies=true"],
        json!({"bodies": true}),
        None,
    );
    res_step(
        h,
        "직접 고침 조회",
        "get_resource",
        &["get-resource", &s1],
        json!({"id": s1}),
        None,
    );
    res_step(
        h,
        "직접 고침 변경 목록",
        "list_resource_changes",
        &["list-resource-changes"],
        json!({}),
        None,
    );
    res_step(
        h,
        "직접 고침 쓸 수 있는 자원",
        "usable_resources",
        &["usable-resources", "alpha", "bodies=true"],
        json!({"project": "alpha", "bodies": "true"}),
        None,
    );
    res_step(
        h,
        "직접 고침 위에 고치기",
        "save_resource",
        &[
            "save-resource",
            &format!("id={s1}"),
            "description=승인한 판 위에",
        ],
        json!({"id": s1, "description": "승인한 판 위에"}),
        Some(&s1),
    );
    put_both(h, &s1_path, &edited);
    res_step(
        h,
        "직접 고침 되돌리기",
        "resolve_resource_edit",
        &["resolve-resource-edit", &s1, "action=revert"],
        json!({"id": s1, "action": "revert"}),
        None,
    );
    res_step(
        h,
        "직접 고침 또 처리",
        "resolve_resource_edit",
        &["resolve-resource-edit", &s1, "action=approve"],
        json!({"id": s1, "action": "approve"}),
        None,
    );
    put_both(h, &s1_path, &edited);
    res_step(
        h,
        "직접 고침 받아들이기",
        "resolve_resource_edit",
        &["resolve-resource-edit", &s1, "action=approve"],
        json!({"id": s1, "action": "approve"}),
        None,
    );
    res_step(
        h,
        "받아들인 뒤 조회",
        "get_resource",
        &["get-resource", &s1],
        json!({"id": s1}),
        None,
    );

    let manual_id = "apmanual1-0123abcd";
    let manual_rel = ".aisurada/resources/global/agents/손으로-만든.md";
    let manual = format!(
        "---\nid: {manual_id}\nname: 손으로 만든\ndescription: 승인 기록 없음\npermission: edit\n---\n본문\n"
    );
    put_both(h, manual_rel, &manual);
    res_step(
        h,
        "새 파일 목록",
        "list_resources",
        &["list-resources"],
        json!({}),
        None,
    );
    res_step(
        h,
        "새 파일 조회",
        "get_resource",
        &["get-resource", manual_id],
        json!({"id": manual_id}),
        None,
    );
    res_step(
        h,
        "새 파일 고치기",
        "save_resource",
        &["save-resource", &format!("id={manual_id}"), "description=x"],
        json!({"id": manual_id, "description": "x"}),
        None,
    );
    res_step(
        h,
        "새 파일 변경 목록",
        "list_resource_changes",
        &["list-resource-changes", "decided=0"],
        json!({"decided": 0}),
        None,
    );
    res_step(
        h,
        "새 파일 되돌리기(휴지통)",
        "resolve_resource_edit",
        &["resolve-resource-edit", manual_id, "action=revert"],
        json!({"id": manual_id, "action": "revert"}),
        None,
    );
    put_both(h, manual_rel, &manual);
    res_step(
        h,
        "새 파일 받아들이기",
        "resolve_resource_edit",
        &["resolve-resource-edit", manual_id, "action=approve"],
        json!({"id": manual_id, "action": "approve"}),
        None,
    );
    res_step(
        h,
        "새 파일 받아들인 뒤 조회",
        "get_resource",
        &["get-resource", manual_id],
        json!({"id": manual_id}),
        None,
    );
    put_both(
        h,
        ".aisurada/resources/global/skills/깨진/SKILL.md",
        "머리말 없음\n",
    );
    put_both(
        h,
        ".aisurada/resources/global/agents/큰-id.md",
        &format!("---\nid: {manual_id}\nname: 같은 id\ndescription: 겹침\n---\n"),
    );
    res_step(
        h,
        "깨진 파일 목록",
        "list_resources",
        &["list-resources"],
        json!({}),
        None,
    );

    let refs_rel = ".aisurada/resources/projects/beta/refs.md";
    put_both(
        h,
        refs_rel,
        &format!("# 손으로 쓴 목록\n{s1}  # 코드 리뷰\nnot-an-id\n{a1}\n"),
    );
    res_step(
        h,
        "참조 새 파일 변경 목록",
        "list_resource_changes",
        &["list-resource-changes", "decided=0"],
        json!({"decided": "0"}),
        None,
    );
    res_step(
        h,
        "참조 새 파일 쓸 수 있는 자원",
        "usable_resources",
        &["usable-resources", "beta"],
        json!({"project": "beta"}),
        None,
    );
    res_step(
        h,
        "참조 새 파일 되돌리기",
        "resolve_resource_edit",
        &["resolve-resource-edit", "refs:beta", "action=revert"],
        json!({"id": "refs:beta", "action": "revert"}),
        None,
    );
    res_step(
        h,
        "참조 되돌린 뒤 변경 목록",
        "list_resource_changes",
        &["list-resource-changes", "decided=0"],
        json!({"decided": 0}),
        None,
    );
    put_both(h, refs_rel, &format!("{s1}  # 코드 리뷰\n"));
    res_step(
        h,
        "참조 새 파일 받아들이기",
        "resolve_resource_edit",
        &["resolve-resource-edit", "refs:beta", "action=approve"],
        json!({"id": "refs:beta", "action": "approve"}),
        None,
    );
    put_both(h, refs_rel, &format!("{s1}  # 코드 리뷰\n{a1}  # 리뷰어\n"));
    res_step(
        h,
        "참조 직접 고침 목록",
        "list_resource_changes",
        &["list-resource-changes", "decided=0"],
        json!({"decided": 0}),
        None,
    );
    res_step(
        h,
        "참조 직접 고침 쓸 수 있는 자원",
        "usable_resources",
        &["usable-resources", "beta"],
        json!({"project": "beta"}),
        None,
    );
    res_step(
        h,
        "참조 직접 고침 위에 끄기",
        "set_resource_ref",
        &["set-resource-ref", "beta", &s1, "remove=true"],
        json!({"project": "beta", "id": s1, "remove": "true"}),
        None,
    );
    put_both(h, refs_rel, &format!("{s1}  # 코드 리뷰\n{a1}  # 리뷰어\n"));
    res_step(
        h,
        "참조 직접 고침 되돌리기",
        "resolve_resource_edit",
        &["resolve-resource-edit", "refs:beta", "action=revert"],
        json!({"id": "refs:beta", "action": "revert"}),
        None,
    );
    res_step(
        h,
        "참조 처리할 것 없음",
        "resolve_resource_edit",
        &["resolve-resource-edit", "refs:beta", "action=approve"],
        json!({"id": "refs:beta", "action": "approve"}),
        None,
    );
    res_step(
        h,
        "처리 action 오류",
        "resolve_resource_edit",
        &["resolve-resource-edit", &s1, "action=maybe"],
        json!({"id": s1, "action": "maybe"}),
        None,
    );
    res_step(
        h,
        "처리 action 없음",
        "resolve_resource_edit",
        &["resolve-resource-edit", &s1],
        json!({"id": s1}),
        None,
    );
    res_step(
        h,
        "처리 인자 없음",
        "resolve_resource_edit",
        &["resolve-resource-edit", "action=approve"],
        json!({"action": "approve"}),
        None,
    );

    let doomed = id_of(&res_step(
        h,
        "지울 글로벌 스킬",
        "save_resource",
        &[
            "save-resource",
            "kind=skill",
            "scope=global",
            "name=지울 스킬",
            "description=곧 지운다",
        ],
        json!({"kind": "skill", "scope": "global", "name": "지울 스킬", "description": "곧 지운다"}),
        None,
    ));
    // ---- AI가 올린 대기 변경: Rust 사본에서 올리고 파일을 Java 사본에 옮긴 뒤, 승인과 거절을 두 엔진에 돌린다 ----
    let token = res_token(h, &a1);
    let queued = res_ai_queue(
        h,
        "권한 올림",
        "save_resource",
        json!({"id": a1, "permission": "read", "f1": token}),
    );
    let raise_id = queued["changeId"].as_str().unwrap_or_default().to_string();
    let token = res_token(h, &a1);
    let queued = res_ai_queue(
        h,
        "권한 올림 같은 요청",
        "save_resource",
        json!({"id": a1, "permission": "read", "f1": token}),
    );
    h.report.record(
        "자원 AI 같은 요청은 한 건".to_string(),
        queued["duplicate"] == json!(true) && queued["changeId"] == json!(raise_id),
        || format!("{queued}"),
    );
    let queued = res_ai_queue(
        h,
        "글로벌 스킬 만들기",
        "save_resource",
        json!({"kind": "skill", "scope": "global", "name": "AI가 제안한 스킬", "description": "제안", "body": "본문"}),
    );
    let create_change = queued["changeId"].as_str().unwrap_or_default().to_string();
    let queued = res_ai_queue(
        h,
        "글로벌 스킬 지우기",
        "delete_resource",
        json!({"id": doomed}),
    );
    let delete_change = queued["changeId"].as_str().unwrap_or_default().to_string();
    let queued = res_ai_queue(
        h,
        "참조 끄기",
        "set_resource_ref",
        json!({"project": "alpha", "id": s1, "remove": true}),
    );
    let ref_change = queued["changeId"].as_str().unwrap_or_default().to_string();
    let queued = res_ai_queue(
        h,
        "참조 켜기",
        "set_resource_ref",
        json!({"project": "beta", "id": a1, "with-skills": true}),
    );
    let ref_add_change = queued["changeId"].as_str().unwrap_or_default().to_string();
    let token = res_token(h, &pa1);
    let queued = res_ai_queue(
        h,
        "도구 칸 변경",
        "save_resource",
        json!({"id": pa1, "tools": r#"{"claude":{"a":1}}"#, "description": "AI가 고친 설명", "f1": token}),
    );
    let tools_change = queued["changeId"].as_str().unwrap_or_default().to_string();
    let token = res_token(h, &pa1);
    let queued = res_ai_queue(
        h,
        "곧 낡을 변경",
        "save_resource",
        json!({"id": pa1, "body": "낡을 지침", "permission": "read", "f1": token}),
    );
    let stale_change = queued["changeId"].as_str().unwrap_or_default().to_string();
    let token = res_token(h, &a1);
    let queued = res_ai_queue(
        h,
        "거절할 변경",
        "save_resource",
        json!({"id": a1, "name": "AI 이름", "f1": token}),
    );
    let reject_change = queued["changeId"].as_str().unwrap_or_default().to_string();

    res_step(
        h,
        "대기 목록",
        "list_resource_changes",
        &["list-resource-changes"],
        json!({}),
        None,
    );
    res_step(
        h,
        "대기 하나(권한 올림)",
        "get_resource_change",
        &["get-resource-change", &raise_id],
        json!({"id": raise_id}),
        None,
    );
    res_step(
        h,
        "대기 하나(만들기)",
        "get_resource_change",
        &["get-resource-change", &create_change],
        json!({"id": create_change}),
        None,
    );
    res_step(
        h,
        "대기 하나(참조)",
        "get_resource_change",
        &["get-resource-change", &ref_change],
        json!({"id": ref_change}),
        None,
    );
    res_step(
        h,
        "대기 하나(도구 칸)",
        "get_resource_change",
        &["get-resource-change", &tools_change],
        json!({"id": tools_change}),
        None,
    );
    res_step(
        h,
        "대기 id 모양",
        "get_resource_change",
        &["get-resource-change", "nope"],
        json!({"id": "nope"}),
        None,
    );
    res_step(
        h,
        "대기 없는 id",
        "get_resource_change",
        &["get-resource-change", "pc999-0123abcd"],
        json!({"id": "pc999-0123abcd"}),
        None,
    );
    res_step(
        h,
        "대기 인자 없음",
        "get_resource_change",
        &["get-resource-change"],
        json!({}),
        None,
    );
    res_step(
        h,
        "대기 목록 decided 오류",
        "list_resource_changes",
        &["list-resource-changes", "decided=abc"],
        json!({"decided": "abc"}),
        None,
    );
    res_step(
        h,
        "대기 목록 decided 음수",
        "list_resource_changes",
        &["list-resource-changes", "decided=-1"],
        json!({"decided": "-1"}),
        None,
    );
    res_step(
        h,
        "조회는 이전 판",
        "get_resource",
        &["get-resource", &a1],
        json!({"id": a1}),
        None,
    );

    res_step(
        h,
        "승인 도구 칸·설명",
        "approve_resource_change",
        &["approve-resource-change", &tools_change],
        json!({"id": tools_change}),
        None,
    );
    // 낡을 변경: 승인 전에 사람이 같은 자원을 고친다
    res_step(
        h,
        "낡게 만들기",
        "save_resource",
        &[
            "save-resource",
            &format!("id={pa1}"),
            "description=사람이 먼저 고침",
        ],
        json!({"id": pa1, "description": "사람이 먼저 고침"}),
        Some(&pa1),
    );
    res_step(
        h,
        "승인 낡은 변경",
        "approve_resource_change",
        &["approve-resource-change", &stale_change],
        json!({"id": stale_change}),
        None,
    );
    res_step(
        h,
        "승인 이미 처리됨",
        "approve_resource_change",
        &["approve-resource-change", &stale_change],
        json!({"id": stale_change}),
        None,
    );
    res_step(
        h,
        "거절 이미 처리됨",
        "reject_resource_change",
        &["reject-resource-change", &stale_change],
        json!({"id": stale_change}),
        None,
    );
    res_step(
        h,
        "승인 권한 올림",
        "approve_resource_change",
        &["approve-resource-change", &raise_id],
        json!({"id": raise_id}),
        None,
    );
    res_step(
        h,
        "조회 승인 뒤",
        "get_resource",
        &["get-resource", &a1],
        json!({"id": a1}),
        None,
    );
    res_step(
        h,
        "승인 만들기",
        "approve_resource_change",
        &["approve-resource-change", &create_change],
        json!({"id": create_change}),
        None,
    );
    res_step(
        h,
        "승인 참조 끄기",
        "approve_resource_change",
        &["approve-resource-change", &ref_change],
        json!({"id": ref_change}),
        None,
    );
    res_step(
        h,
        "승인 참조 켜기",
        "approve_resource_change",
        &["approve-resource-change", &ref_add_change],
        json!({"id": ref_add_change}),
        None,
    );
    res_step(
        h,
        "거절 이유 없이",
        "reject_resource_change",
        &["reject-resource-change", &reject_change],
        json!({"id": reject_change}),
        None,
    );
    res_step(
        h,
        "승인 지우기(이미 지워진 뒤 아님)",
        "approve_resource_change",
        &["approve-resource-change", &delete_change],
        json!({"id": delete_change}),
        None,
    );
    res_step(
        h,
        "승인 인자 없음",
        "approve_resource_change",
        &["approve-resource-change"],
        json!({}),
        None,
    );
    res_step(
        h,
        "승인 id 모양",
        "approve_resource_change",
        &["approve-resource-change", "nope"],
        json!({"id": "nope"}),
        None,
    );
    res_step(
        h,
        "거절 없는 id",
        "reject_resource_change",
        &["reject-resource-change", "pc999-0123abcd"],
        json!({"id": "pc999-0123abcd"}),
        None,
    );
    res_step(
        h,
        "처리 뒤 변경 목록",
        "list_resource_changes",
        &["list-resource-changes", "decided=50"],
        json!({"decided": 50}),
        None,
    );
    res_step(
        h,
        "처리 뒤 변경 하나(낡음)",
        "get_resource_change",
        &["get-resource-change", &stale_change],
        json!({"id": stale_change}),
        None,
    );
    res_step(
        h,
        "처리 뒤 변경 하나(승인)",
        "get_resource_change",
        &["get-resource-change", &raise_id],
        json!({"id": raise_id}),
        None,
    );
    res_step(
        h,
        "처리 뒤 목록",
        "list_resources",
        &["list-resources", "bodies=true"],
        json!({"bodies": true}),
        None,
    );
    let queued = res_ai_queue(h, "거절 이유 있게", "delete_resource", json!({"id": a1}));
    let reason_change = queued["changeId"].as_str().unwrap_or_default().to_string();
    res_step(
        h,
        "거절 이유 있게",
        "reject_resource_change",
        &[
            "reject-resource-change",
            &reason_change,
            "reason=  필요 없다  ",
        ],
        json!({"id": reason_change, "reason": "  필요 없다  "}),
        None,
    );
    res_step(
        h,
        "처리 뒤 변경 하나(거절)",
        "get_resource_change",
        &["get-resource-change", &reason_change],
        json!({"id": reason_change}),
        None,
    );
    s1
}

/// 자원 고치기에서 종류와 범위가 **함께** 틀린 입력의 오류 차례(종류 → 범위, 이슈 #83). 한 번에 하나만 틀린 사례로는 순서를 못 본다.
/// 하네스의 id와 시각은 단계 번호에서 나오므로, 앞 사례의 기록이 밀리지 않게 쓰기 단계의 맨 끝에서 부른다.
fn compare_resource_edit_error_order(h: &mut WriteHarness, skill: &str) {
    for (label, kind, scope_key, scope_val) in [
        ("종류 바꿈과 범위 문법 틀림", "agent", "scope", "weird"),
        ("종류 모름과 범위 문법 틀림", "bogus", "scope", "weird"),
        ("종류 바꿈과 범위 바꿈", "agent", "project", "alpha"),
        ("종류 같고 범위 문법 틀림", "skill", "scope", "weird"),
    ] {
        res_step(
            h,
            &format!("고치기 {label}"),
            "save_resource",
            &[
                "save-resource",
                &format!("id={skill}"),
                &format!("kind={kind}"),
                &format!("{scope_key}={scope_val}"),
            ],
            json!({"id": skill, "kind": kind, scope_key: scope_val}),
            Some(skill),
        );
    }
}

/// 문답·레일 대화 명령 한 단계. `java`는 `ais-cli` 인자 목록(자리 인자 포함)이고 `rust`는 같은 뜻의 호출 객체다.
fn ask_step(
    h: &mut WriteHarness,
    label: &str,
    command: &str,
    java: &[&str],
    rust: Value,
) -> Option<Value> {
    h.step(&format!("문답 {label}"), java, command, rust)
}

/// 문답·레일 대화 명령 맞대기: 읽기(`get_ask`, `list_chats`)와 붙이기(`append_ask_turn`, `append_chat_turn`).
/// 손으로 쓴 파일(모르는 칸, 틀린 모양, 너무 큰 파일, 깨진 JSON)도 두 엔진이 같은 글로 읽는지 본다.
fn compare_ask_chat_commands(h: &mut WriteHarness) {
    let parent = "ndfx0002-00000002";
    let child = "snfx0002-00000012";
    let grand = "snfx0003-00000013";
    let other = "ndfx0004-00000004";
    let at = "2026-11-14T10:00:00+09:00";

    // ---- 문답 읽기: 파일이 없으면 null, 인자 오류 ----
    ask_step(
        h,
        "읽기 파일 없음",
        "get_ask",
        &["get-ask", parent],
        json!({"id": parent}),
    );
    ask_step(
        h,
        "읽기 후속 노트 파일 없음",
        "get_ask",
        &["get-ask", parent, child],
        json!({"id": parent, "subId": child}),
    );
    ask_step(h, "읽기 인자 없음", "get_ask", &["get-ask"], json!({}));
    ask_step(
        h,
        "읽기 노트 id 모양",
        "get_ask",
        &["get-ask", "nope"],
        json!({"id": "nope"}),
    );
    ask_step(
        h,
        "읽기 하위 id 모양",
        "get_ask",
        &["get-ask", parent, "nope"],
        json!({"id": parent, "subId": "nope"}),
    );
    ask_step(
        h,
        "읽기 하위 id 비어 있음",
        "get_ask",
        &["get-ask", parent, " "],
        json!({"id": parent, "subId": " "}),
    );
    ask_step(
        h,
        "읽기 없는 노트",
        "get_ask",
        &["get-ask", "ndzz0001-00000001"],
        json!({"id": "ndzz0001-00000001"}),
    );
    ask_step(
        h,
        "읽기 없는 후속 노트",
        "get_ask",
        &["get-ask", parent, "snzz0001-00000001"],
        json!({"id": parent, "subId": "snzz0001-00000001"}),
    );

    // ---- 문답 붙이기 ----
    let base = |extra: &[(&str, &str)]| -> (Vec<String>, Value) {
        let mut named: Vec<(String, String)> = vec![
            ("session-id".into(), "sess-1".into()),
            ("turn-id".into(), "t1".into()),
            ("mark-id".into(), "m1".into()),
            ("asked-at".into(), at.into()),
            ("question".into(), "무엇인가?".into()),
            ("answer".into(), "이것이다.".into()),
            ("model".into(), "opus".into()),
            ("effort".into(), "high".into()),
            ("elapsed-ms".into(), "1200".into()),
        ];
        for (k, v) in extra {
            match named.iter_mut().find(|(name, _)| name == k) {
                Some(slot) => slot.1 = v.to_string(),
                None => named.push((k.to_string(), v.to_string())),
            }
        }
        let java: Vec<String> = named.iter().map(|(k, v)| format!("{k}={v}")).collect();
        let rust: Map<String, Value> = named.into_iter().map(|(k, v)| (k, json!(v))).collect();
        (java, Value::Object(rust))
    };
    let append_ask = |h: &mut WriteHarness,
                      label: &str,
                      id: &[&str],
                      extra: &[(&str, &str)],
                      drop_key: Option<&str>| {
        let (mut java, mut rust) = base(extra);
        if let Some(key) = drop_key {
            java.retain(|a| !a.starts_with(&format!("{key}=")));
            rust.as_object_mut().unwrap().remove(key);
        }
        let mut argv: Vec<String> = vec!["append-ask-turn".into()];
        argv.extend(id.iter().map(|s| s.to_string()));
        argv.extend(java);
        rust["id"] = json!(id.first().copied().unwrap_or(""));
        if let Some(sub) = id.get(1) {
            rust["subId"] = json!(sub);
        }
        let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        ask_step(h, label, "append_ask_turn", &refs, rust);
    };
    append_ask(h, "붙이기 새 파일", &[parent], &[], None);
    append_ask(
        h,
        "붙이기 둘째 회차",
        &[parent],
        &[
            ("turn-id", "t2"),
            ("mark-id", "m2"),
            ("session-id", "sess-ignored"),
        ],
        None,
    );
    append_ask(
        h,
        "붙이기 모든 칸",
        &[parent],
        &[
            ("turn-id", "t3"),
            ("quote", "인용한 \"대목\"\n둘째 줄"),
            ("error", "실패: 시간 초과"),
            ("session-lines", " 42 "),
            ("images", "attach/a.png, attach/한글.JPG ,, attach/c.svg"),
            ("question", "줄1\r\n줄2\t탭 😀"),
            ("answer", ""),
        ],
        None,
    );
    append_ask(
        h,
        "붙이기 후속 노트",
        &[parent, child],
        &[("turn-id", "s1")],
        None,
    );
    append_ask(
        h,
        "붙이기 손자 노트",
        &[parent, grand],
        &[("turn-id", "g1")],
        None,
    );
    append_ask(
        h,
        "붙이기 빈 인용과 빈 오류",
        &[parent, child],
        &[("turn-id", "s2"), ("quote", ""), ("error", "")],
        None,
    );
    append_ask(h, "붙이기 단일 파일 노트", &[other], &[], None);
    ask_step(
        h,
        "읽기 메인",
        "get_ask",
        &["get-ask", parent],
        json!({"id": parent}),
    );
    ask_step(
        h,
        "읽기 후속",
        "get_ask",
        &["get-ask", parent, child],
        json!({"id": parent, "subId": child}),
    );
    ask_step(
        h,
        "읽기 단일 파일 노트",
        "get_ask",
        &["get-ask", other],
        json!({"id": other}),
    );
    ask_step(
        h,
        "읽기 후속 노트(빈 값 칸)",
        "get_ask",
        &["get-ask", parent, child],
        json!({"id": parent, "subId": child}),
    );
    let long_text = "가".repeat(20_001);
    let long_quote = "나".repeat(2_001);
    let edge_text = "가".repeat(20_000);
    for (label, extra, drop_key) in [
        ("인자 없음", vec![], Some("session-id")),
        ("turn-id 없음", vec![], Some("turn-id")),
        ("mark-id 없음", vec![], Some("mark-id")),
        ("asked-at 없음", vec![], Some("asked-at")),
        ("question 없음", vec![], Some("question")),
        ("answer 없음", vec![], Some("answer")),
        ("model 없음", vec![], Some("model")),
        ("effort 없음", vec![], Some("effort")),
        ("elapsed-ms 없음", vec![], Some("elapsed-ms")),
        ("model 공백", vec![("model", "  ")], None),
        ("elapsed-ms 글", vec![("elapsed-ms", "abc")], None),
        ("elapsed-ms 음수", vec![("elapsed-ms", "-1")], None),
        (
            "elapsed-ms 32비트 넘침",
            vec![("elapsed-ms", "2147483648")],
            None,
        ),
        ("elapsed-ms 소수", vec![("elapsed-ms", "1.5")], None),
        ("질문 길이", vec![("question", long_text.as_str())], None),
        ("답변 길이", vec![("answer", long_text.as_str())], None),
        ("인용 길이", vec![("quote", long_quote.as_str())], None),
        ("session-lines 0", vec![("session-lines", "0")], None),
        ("session-lines 글", vec![("session-lines", "x")], None),
        ("session-lines 공백", vec![("session-lines", "  ")], None),
        (
            "그림 9장",
            vec![(
                "images",
                "attach/1.png,attach/2.png,attach/3.png,attach/4.png,attach/5.png,attach/6.png,attach/7.png,attach/8.png,attach/9.png",
            )],
            None,
        ),
        ("그림 자리", vec![("images", "pic.png")], None),
        ("그림 이름", vec![("images", "attach/a.txt")], None),
        ("그림 숨김 파일", vec![("images", "attach/.a.png")], None),
        ("그림 빈 이름", vec![("images", "attach/")], None),
    ] {
        append_ask(
            h,
            &format!("붙이기 오류 {label}"),
            &[parent],
            &extra,
            drop_key,
        );
    }
    append_ask(
        h,
        "붙이기 경계 길이",
        &[parent],
        &[("turn-id", "edge"), ("question", edge_text.as_str())],
        None,
    );
    append_ask(h, "붙이기 없는 노트", &["ndzz0001-00000001"], &[], None);
    append_ask(
        h,
        "붙이기 없는 후속 노트",
        &[parent, "snzz0001-00000001"],
        &[],
        None,
    );
    append_ask(h, "붙이기 노트 id 모양", &["nope"], &[], None);
    append_ask(h, "붙이기 하위 id 모양", &[parent, "nope"], &[], None);
    append_ask(h, "붙이기 하위 id 비어 있음", &[parent, " "], &[], None);
    append_ask(h, "붙이기 자리 인자 없음", &[], &[], None);

    // ---- 손으로 쓴 문답 파일 ----
    let ask_rel = |dir: &str| format!("{dir}/ask.json");
    let rust_root = h.b.clone();
    // 앞 시나리오가 노트를 지우고 옮겨서, 지금 남아 있는 폴더 노트 하나를 손으로 쓴 파일의 자리로 고른다
    let loose_id = {
        let index = ais_core::vault::index::NoteIndex::fresh(&rust_root);
        index
            .root_notes()
            .find(|n| !n.is_single_file && !n.is_extension_folder && n.id != parent)
            .expect("폴더 노트가 있다")
            .id
            .clone()
    };
    // 색인이 보는 노트 폴더(엔진이 문답 파일을 두는 곳)를 vault 기준 상대 경로로
    let note_dir = move |id: &str| -> String {
        let index = ais_core::vault::index::NoteIndex::fresh(&rust_root);
        let note = index.note(id).expect("노트가 있다");
        note.content_file
            .parent()
            .unwrap()
            .strip_prefix(&rust_root)
            .unwrap()
            .to_string_lossy()
            .to_string()
    };
    let loose_dir = note_dir(&loose_id);
    let weird = r#"{"v":9,"noteId":"","sessionId":5,"extra":1,"turns":[
      {"id":"a","markId":"","askedAt":"2026-11-14T00:00:00Z","quote":"","question":"q","answer":"a","error":"","model":"m","effort":"e","elapsedMs":"123","sessionLines":"4","images":["attach/ok.png","attach/bad.txt","x.png",5]},
      "문자열",
      {"id":7,"askedAt":null,"question":[],"elapsedMs":7600.5,"sessionLines":-3,"images":[]},
      {"id":"b","elapsedMs":1e3,"sessionLines":2147483648,"images":"attach/a.png","unknown":true},
      {"id":"c","elapsedMs":9007199254740993,"sessionLines":5,"error":"실패","markId":"mm"}
    ]}"#;
    put_both(h, &ask_rel(&loose_dir), weird);
    ask_step(
        h,
        "손으로 쓴 파일 읽기",
        "get_ask",
        &["get-ask", &loose_id],
        json!({"id": &loose_id}),
    );
    append_ask(
        h,
        "손으로 쓴 파일에 붙이기",
        &[&loose_id],
        &[("turn-id", "n1")],
        None,
    );
    ask_step(
        h,
        "손으로 쓴 파일 다시 읽기",
        "get_ask",
        &["get-ask", &loose_id],
        json!({"id": &loose_id}),
    );
    for (label, content) in [
        ("깨진 JSON", "{\"turns\": [".to_string()),
        ("최상위가 배열", "[]".to_string()),
        ("빈 파일", String::new()),
        (
            "너무 큼",
            format!("{{\"turns\":[],\"pad\":\"{}\"}}", "x".repeat(1_000_000)),
        ),
    ] {
        put_both(h, &ask_rel(&loose_dir), &content);
        ask_step(
            h,
            &format!("{label} 읽기"),
            "get_ask",
            &["get-ask", &loose_id],
            json!({"id": &loose_id}),
        );
        append_ask(h, &format!("{label}에 붙이기"), &[&loose_id], &[], None);
    }
    // 붙이면 100만 바이트를 넘게 되는 파일
    let fat_turn = format!(
        r#"{{"id":"fat","askedAt":"x","question":"{}","answer":"","model":"m","effort":"e","elapsedMs":1}}"#,
        "가".repeat(160_000)
    );
    let fat = format!(
        r#"{{"v":1,"noteId":"{loose_id}","sessionId":"s","turns":[{fat_turn},{fat_turn}]}}"#
    );
    put_both(h, &ask_rel(&loose_dir), &fat);
    append_ask(
        h,
        "붙이면 상한 넘음",
        &[&loose_id],
        &[("question", &"나".repeat(20_000))],
        None,
    );

    // ---- 레일 대화 읽기와 붙이기 ----
    ask_step(
        h,
        "대화 목록 없음",
        "list_chats",
        &["list-chats", parent],
        json!({"id": parent}),
    );
    ask_step(
        h,
        "대화 목록 후속 노트 없음",
        "list_chats",
        &["list-chats", parent, child],
        json!({"id": parent, "subId": child}),
    );
    ask_step(
        h,
        "대화 목록 인자 없음",
        "list_chats",
        &["list-chats"],
        json!({}),
    );
    ask_step(
        h,
        "대화 목록 노트 id 모양",
        "list_chats",
        &["list-chats", "nope"],
        json!({"id": "nope"}),
    );
    ask_step(
        h,
        "대화 목록 하위 id 모양",
        "list_chats",
        &["list-chats", parent, "nope"],
        json!({"id": parent, "subId": "nope"}),
    );
    ask_step(
        h,
        "대화 목록 없는 노트",
        "list_chats",
        &["list-chats", "ndzz0001-00000001"],
        json!({"id": "ndzz0001-00000001"}),
    );
    ask_step(
        h,
        "대화 목록 없는 후속 노트",
        "list_chats",
        &["list-chats", parent, "snzz0001-00000001"],
        json!({"id": parent, "subId": "snzz0001-00000001"}),
    );

    let chat_base = |extra: &[(&str, &str)]| -> (Vec<String>, Value) {
        let mut named: Vec<(String, String)> = vec![
            ("chat-id".into(), "chat-1".into()),
            ("source-session".into(), "sess-x".into()),
            ("turn-id".into(), "t1".into()),
            ("asked-at".into(), at.into()),
            ("question".into(), "무엇인가?".into()),
            ("answer".into(), "이것이다.".into()),
            ("model".into(), "opus".into()),
            ("effort".into(), "high".into()),
            ("elapsed-ms".into(), "900".into()),
        ];
        for (k, v) in extra {
            match named.iter_mut().find(|(name, _)| name == k) {
                Some(slot) => slot.1 = v.to_string(),
                None => named.push((k.to_string(), v.to_string())),
            }
        }
        let java: Vec<String> = named.iter().map(|(k, v)| format!("{k}={v}")).collect();
        let rust: Map<String, Value> = named.into_iter().map(|(k, v)| (k, json!(v))).collect();
        (java, Value::Object(rust))
    };
    let append_chat = |h: &mut WriteHarness,
                       label: &str,
                       id: &[&str],
                       extra: &[(&str, &str)],
                       drop_key: Option<&str>| {
        let (mut java, mut rust) = chat_base(extra);
        if let Some(key) = drop_key {
            java.retain(|a| !a.starts_with(&format!("{key}=")));
            rust.as_object_mut().unwrap().remove(key);
        }
        let mut argv: Vec<String> = vec!["append-chat-turn".into()];
        argv.extend(id.iter().map(|s| s.to_string()));
        argv.extend(java);
        rust["id"] = json!(id.first().copied().unwrap_or(""));
        if let Some(sub) = id.get(1) {
            rust["subId"] = json!(sub);
        }
        let refs: Vec<&str> = argv.iter().map(String::as_str).collect();
        ask_step(h, label, "append_chat_turn", &refs, rust);
    };
    append_chat(h, "대화 붙이기 새 대화", &[parent], &[], None);
    append_chat(
        h,
        "대화 붙이기 같은 대화 둘째",
        &[parent],
        &[
            ("turn-id", "t2"),
            ("asked-at", "2026-11-14T11:00:00+09:00"),
            ("source-session", "ignored"),
        ],
        None,
    );
    append_chat(
        h,
        "대화 붙이기 더 이른 대화",
        &[parent],
        &[("chat-id", "chat-0"), ("asked-at", "2026-11-13T00:00:00Z")],
        None,
    );
    append_chat(
        h,
        "대화 붙이기 모든 칸",
        &[parent],
        &[
            ("chat-id", "Chat_A-2"),
            ("turn-id", "t9"),
            ("error", ""),
            ("images", "attach/a.png, attach/b.webp"),
            ("question", "줄1\r\n줄2 😀"),
            ("answer", ""),
            ("asked-at", "2026-11-14T10:00:00Z"),
        ],
        None,
    );
    append_chat(
        h,
        "대화 붙이기 후속 노트",
        &[parent, child],
        &[("chat-id", "sub-chat")],
        None,
    );
    append_chat(
        h,
        "대화 붙이기 손자 노트",
        &[parent, grand],
        &[("chat-id", "grand-chat")],
        None,
    );
    ask_step(
        h,
        "대화 목록 메인",
        "list_chats",
        &["list-chats", parent],
        json!({"id": parent}),
    );
    ask_step(
        h,
        "대화 목록 후속",
        "list_chats",
        &["list-chats", parent, child],
        json!({"id": parent, "subId": child}),
    );
    let long_id = "a".repeat(65);
    let long_session = "s".repeat(129);
    let edge_session = "s".repeat(128);
    for (label, extra, drop_key) in [
        ("chat-id 없음", vec![], Some("chat-id")),
        ("chat-id 모양", vec![("chat-id", "a b")], None),
        ("chat-id 길이", vec![("chat-id", long_id.as_str())], None),
        ("chat-id 점", vec![("chat-id", "a.b")], None),
        ("source-session 없음", vec![], Some("source-session")),
        (
            "source-session 길이",
            vec![("source-session", long_session.as_str())],
            None,
        ),
        ("turn-id 없음", vec![], Some("turn-id")),
        ("asked-at 없음", vec![], Some("asked-at")),
        ("question 없음", vec![], Some("question")),
        ("answer 없음", vec![], Some("answer")),
        ("model 없음", vec![], Some("model")),
        ("effort 없음", vec![], Some("effort")),
        ("elapsed-ms 없음", vec![], Some("elapsed-ms")),
        ("elapsed-ms 글", vec![("elapsed-ms", "abc")], None),
        ("elapsed-ms 음수", vec![("elapsed-ms", "-5")], None),
        ("elapsed-ms 넘침", vec![("elapsed-ms", "2147483648")], None),
        ("질문 길이", vec![("question", long_text.as_str())], None),
        ("답 길이", vec![("answer", long_text.as_str())], None),
        (
            "그림 9장",
            vec![(
                "images",
                "attach/1.png,attach/2.png,attach/3.png,attach/4.png,attach/5.png,attach/6.png,attach/7.png,attach/8.png,attach/9.png",
            )],
            None,
        ),
        ("그림 자리", vec![("images", "pic.png")], None),
        ("그림 이름", vec![("images", "attach/a.exe")], None),
    ] {
        append_chat(
            h,
            &format!("대화 붙이기 오류 {label}"),
            &[parent],
            &extra,
            drop_key,
        );
    }
    append_chat(
        h,
        "대화 붙이기 경계 길이",
        &[parent],
        &[
            ("chat-id", "edge"),
            ("source-session", edge_session.as_str()),
            ("question", edge_text.as_str()),
            ("images", "  "),
        ],
        None,
    );
    append_chat(
        h,
        "대화 붙이기 없는 노트",
        &["ndzz0001-00000001"],
        &[],
        None,
    );
    append_chat(
        h,
        "대화 붙이기 없는 후속 노트",
        &[parent, "snzz0001-00000001"],
        &[],
        None,
    );
    append_chat(h, "대화 붙이기 노트 id 모양", &["nope"], &[], None);
    append_chat(
        h,
        "대화 붙이기 하위 id 비어 있음",
        &[parent, " "],
        &[],
        None,
    );
    append_chat(h, "대화 붙이기 자리 인자 없음", &[], &[], None);

    // ---- 손으로 쓴 대화 파일 ----
    let chats_dir = format!("{}/chats", note_dir(&loose_id));
    let chat_files: Vec<(&str, String)> = vec![
        ("a-no-created.json", r#"{"v":1,"noteId":" ","sourceSessionId":7,"turns":[{"id":"x","askedAt":"2026-11-14T03:00:00Z","question":"q"},{"id":"y","askedAt":"2026-11-14T04:00:00Z"}]}"#.to_string()),
        ("b-blank-created.json", r#"{"createdAt":"  ","turns":[{"askedAt":""}],"sourceSessionId":"s"}"#.to_string()),
        ("c-bad-created.json", r#"{"createdAt":"어제","sourceSessionId":"s","turns":[]}"#.to_string()),
        ("d-offset.json", r#"{"createdAt":"2026-11-14T12:00:00+09:00","sourceSessionId":"s","turns":[{"id":"1","elapsedMs":"77","images":["attach/a.png","bad"],"error":"","unknown":1},3,{"elapsedMs":1.5}]}"#.to_string()),
        ("e-same-time.json", r#"{"createdAt":"2026-11-14T03:00:00Z","sourceSessionId":"s","turns":[]}"#.to_string()),
        ("f-same-time.json", r#"{"createdAt":"2026-11-14T03:00:00.000Z","sourceSessionId":"s","turns":[]}"#.to_string()),
        ("g-no-seconds.json", r#"{"createdAt":"2026-11-14T03:00+00:00","sourceSessionId":"s","turns":[]}"#.to_string()),
        ("h-broken.json", "{\"turns\": [".to_string()),
        ("i-array.json", "[]".to_string()),
        ("j-big.json", format!("{{\"turns\":[],\"pad\":\"{}\"}}", "x".repeat(1_000_000))),
        ("k-fraction.json", r#"{"createdAt":"2026-11-14T03:00:00.5Z","sourceSessionId":"s","turns":[]}"#.to_string()),
        ("bad name.json", "{}".to_string()),
        ("note.txt", "{}".to_string()),
        (".json", "{}".to_string()),
        ("z-after-deleted-field.json", r#"{"noteId":"ndother-00000000","chatId":"ignored","createdAt":"2026-11-14T03:00:00Z","turns":[]}"#.to_string()),
    ];
    for (name, content) in &chat_files {
        put_both(h, &format!("{chats_dir}/{name}"), content);
    }
    ask_step(
        h,
        "손으로 쓴 대화 목록",
        "list_chats",
        &["list-chats", &loose_id],
        json!({"id": &loose_id}),
    );
    for (label, chat_id) in [
        ("빠진 칸 대화", "a-no-created"),
        ("깨진 대화", "h-broken"),
        ("배열 대화", "i-array"),
        ("큰 대화", "j-big"),
    ] {
        append_chat(
            h,
            &format!("대화 붙이기 {label}"),
            &[&loose_id],
            &[("chat-id", chat_id)],
            None,
        );
    }
    ask_step(
        h,
        "붙인 뒤 대화 목록",
        "list_chats",
        &["list-chats", &loose_id],
        json!({"id": &loose_id}),
    );
    let fat_chat = format!(
        r#"{{"v":1,"noteId":"{loose_id}","chatId":"fat","sourceSessionId":"s","createdAt":"x","turns":[{fat_turn},{fat_turn}]}}"#
    );
    put_both(h, &format!("{chats_dir}/fat.json"), &fat_chat);
    append_chat(
        h,
        "대화 붙이면 상한 넘음",
        &[&loose_id],
        &[("chat-id", "fat"), ("question", &"나".repeat(20_000))],
        None,
    );
}

// ---------------------------------------------------------------- 세션 기록 읽기(4d)

fn pb_varint(mut value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        if value == 0 {
            out.push(byte);
            return out;
        }
        out.push(byte | 0x80);
    }
}

fn pb_num(field: u32, value: u64) -> Vec<u8> {
    let mut out = pb_varint(u64::from(field) << 3);
    out.extend(pb_varint(value));
    out
}

fn pb_len(field: u32, bytes: &[u8]) -> Vec<u8> {
    let mut out = pb_varint((u64::from(field) << 3) | 2);
    out.extend(pb_varint(bytes.len() as u64));
    out.extend_from_slice(bytes);
    out
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

/// 합성 Antigravity 대화 기록(SQLite)을 시스템의 `sqlite3`로 만든다.
fn make_antigravity_db(path: &Path, cwd: &str) {
    let time = |secs: u64, nanos: u64| {
        let mut t = pb_num(1, secs);
        t.extend(pb_num(2, nanos));
        pb_len(1, &t)
    };
    let user_meta = time(1_794_700_000, 123_000_000);
    let model_meta = time(1_794_700_005, 0);
    let user_payload = pb_len(
        19,
        &pb_len(
            2,
            "안티그래비티에게 묻는다 https://example.com/agy 끝".as_bytes(),
        ),
    );
    let model_payload = pb_len(
        20,
        &[
            pb_len(1, "답한다".as_bytes()),
            pb_len(3, "생각은 뺀다".as_bytes()),
        ]
        .concat(),
    );
    let empty_payload = pb_len(19, &pb_len(2, b"   "));
    let tool = |name: &str, args: &str| {
        let call = [pb_len(2, name.as_bytes()), pb_len(3, args.as_bytes())].concat();
        [time(1_794_700_010, 1_000), pb_len(4, &call)].concat()
    };
    let mcp_args = r#"{"x":1,"ServerName":"srv","ToolName":"search"}"#;
    let rows: Vec<String> = vec![
        format!(
            "(0,14,0,NULL,X'{}',X'{}')",
            hex(&user_meta),
            hex(&user_payload)
        ),
        format!(
            "(1,15,0,NULL,X'{}',X'{}')",
            hex(&model_meta),
            hex(&model_payload)
        ),
        format!(
            "(2,14,5,NULL,X'{}',X'{}')",
            hex(&user_meta),
            hex(&empty_payload)
        ),
        format!(
            "(3,132,0,NULL,X'{}',NULL)",
            hex(&tool(
                "run_command",
                r#"{"CommandLine":"ls -la","description":"목록을 본다"}"#
            ))
        ),
        format!(
            "(4,132,7,NULL,X'{}',NULL)",
            hex(&tool("view_file", r#"{"AbsolutePath":"/tmp/a.txt"}"#))
        ),
        format!(
            "(5,132,0,X'01',X'{}',NULL)",
            hex(&tool("call_mcp_tool", mcp_args))
        ),
        format!("(6,101,0,NULL,X'{}',NULL)", hex(&user_meta)),
        "(7,14,0,NULL,X'',NULL)".to_string(),
    ];
    let cwd_url = format!("file://{cwd}");
    let trajectory = pb_len(7, cwd_url.as_bytes());
    let executor = pb_len(
        10,
        &pb_len(1, &pb_len(28, "gemini-3.8-flash-high".as_bytes())),
    );
    let generated = pb_len(1, &pb_len(21, "Gemini 화면 이름".as_bytes()));
    let sql = format!(
        "create table steps(idx integer, step_type integer, status integer, error_details blob, metadata blob, step_payload blob);\
         create table trajectory_metadata_blob(data blob); create table executor_metadata(idx integer, data blob); create table gen_metadata(idx integer, data blob);\
         insert into steps values {};\
         insert into trajectory_metadata_blob values (X'{}');\
         insert into executor_metadata values (0, X'{}');\
         insert into gen_metadata values (0, X'{}');",
        rows.join(","),
        hex(&trajectory),
        hex(&executor),
        hex(&generated)
    );
    let _ = fs::remove_file(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let status = Command::new("/usr/bin/sqlite3")
        .arg(path)
        .arg(sql)
        .status()
        .expect("sqlite3");
    assert!(status.success(), "합성 Antigravity 기록을 못 만들었다");
}

/// 세션 읽기 맞대기: 합성 홈(`session-home`)에 네 가지 형식의 기록을 만들어 두 엔진이 같은 글로 읽는지 본다.
fn compare_session_commands(h: &mut WriteHarness) {
    let home = h.locks.parent().unwrap().join("session-home");
    let _ = fs::remove_dir_all(&home);
    let write = |rel: &str, content: &str| {
        let path = home.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    };
    let set_age = |rel: &str, seconds_ago: u64| {
        let file = fs::OpenOptions::new()
            .write(true)
            .open(home.join(rel))
            .unwrap();
        let when = std::time::SystemTime::now() - std::time::Duration::from_secs(seconds_ago);
        file.set_modified(when).unwrap();
    };

    // ---- Claude ----
    let a = "aaaaaaaa-1111-2222-3333-444444444444";
    let long_text = format!("긴 글 😀 {}", "가".repeat(2100));
    let note = "ndclaude01-0001";
    let claude_lines = vec![
        r#"{"type":"custom-title","customTitle":"꼬리 제목 아님","sessionId":"x"}"#.to_string(),
        r#"{"type":"user","timestamp":"2026-11-14T01:00:00.000Z","cwd":"/work/proj","gitBranch":"feat/x","entrypoint":"cli","message":{"role":"user","content":"처음 질문 https://github.com/a/b/pull/1. 그리고 http://localhost:3000/x 와 https://example.com/page, 끝"}}"#.to_string(),
        String::new(),
        r#"   "#.to_string(),
        "{깨진 줄".to_string(),
        "[1,2]".to_string(),
        r#"{"timestamp":"2026-11-14T01:00:01Z","message":{"content":"type 없음"}}"#.to_string(),
        format!(r#"{{"type":"assistant","timestamp":"2026-11-14T01:00:02.500Z","effort":"high","message":{{"model":"claude-opus","usage":{{"input_tokens":10,"cache_creation_input_tokens":5,"cache_read_input_tokens":100}},"content":[{{"type":"text","text":"{long_text}"}},{{"type":"tool_use","id":"t1","name":"Bash","input":{{"command":"ls -la\n둘째 줄","description":""}}}},{{"type":"tool_use","id":"t2","name":"TaskCreate","input":{{"subject":"할 일"}}}},{{"type":"tool_use","id":"t3","name":"Agent","input":{{"description":"하위 에이전트 일"}}}},{{"type":"tool_use","id":"t4","name":"Read","input":{{"file_path":"/very/long/{}/file.rs"}}}},{{"type":"tool_use","id":"t5","name":"TaskUpdate","input":{{"status":"completed","subject":"고친 제목"}}}},{{"type":"tool_use","id":"t6","name":"Edit","input":{{"file_path":"{}"}}}}]}}}}"#, "경".repeat(130), "x".repeat(130)),
        format!(r#"{{"type":"user","timestamp":"2026-11-14T01:00:05.250Z","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"t1","content":"출력"}},{{"type":"tool_result","tool_use_id":"t2","content":"Task #1 created successfully: 할 일 제목 "}},{{"type":"tool_result","tool_use_id":"t5","content":"Updated task #1 status","is_error":true}},{{"type":"tool_result","tool_use_id":"t4","content":["배열"]}},{{"type":"tool_result","tool_use_id":"nope","content":"x"}},{{"type":"text","text":"사용자가 {note} 노트를 말한다"}},{{"type":"text","text":"   "}}]}}}}"#),
        r#"{"type":"assistant","timestamp":"2026-11-14T01:00:06Z","message":{"model":"<synthetic>","content":"합성 답"},"quotaLimits":{"resetsAt":1794700000}}"#.to_string(),
        r#"{"type":"assistant","timestamp":"2026-11-14T01:00:07Z","message":{"model":"claude-sonnet","usage":{"output_tokens":3},"content":[{"type":"text","text":"문자열 입력 없음"}]}}"#.to_string(),
        r#"{"type":"assistant","timestamp":"2026-11-14T01:00:08Z","quotaLimits":{"resetsAt":1794709999},"message":{"model":"claude-sonnet","content":"quota 뒤"}}"#.to_string(),
        r#"{"type":"user","timestamp":"2026-11-14T01:00:09Z","message":{"content":[{"type":"tool_result","tool_use_id":"t6","content":"Task #x created successfully"}]}}"#.to_string(),
        r#"{"type":"user","timestamp":"2026-11-14T01:00:10Z","message":{"content":[{"type":"tool_result","tool_use_id":"t3","content":"Task #99999999999 created successfully: 큰 번호"}]}}"#.to_string(),
        format!(r#"{{"type":"user","timestamp":"2026-11-14T01:00:11Z","message":{{"content":"{note} 다시"}}}}"#),
        r#"{"type":"ai-title","aiTitle":"AI가 붙인 제목","sessionId":"x"}"#.to_string(),
    ];
    write(
        &format!(".claude/projects/-work-proj/{a}.jsonl"),
        &(claude_lines.join("\r\n") + "\n"),
    );
    set_age(&format!(".claude/projects/-work-proj/{a}.jsonl"), 3600);
    // 제목 규칙 4가지와 폴더 안의 custom-title.json
    let t_custom = "bbbbbbbb-1111-2222-3333-444444444444";
    write(
        &format!(".claude/projects/-work-proj/{t_custom}.jsonl"),
        "{\"type\":\"custom-title\",\"customTitle\":\"직접 붙인 제목\"}\n{\"type\":\"ai-title\",\"aiTitle\":\"AI 제목\"}\n{\"type\":\"user\",\"entrypoint\":\"cli\",\"timestamp\":\"2026-11-14T02:00:00Z\",\"gitBranch\":\"main\",\"cwd\":\"/w/b\",\"message\":{\"content\":\"x\"}}\n",
    );
    let t_file = "cccccccc-1111-2222-3333-444444444444";
    write(
        &format!(".claude/projects/-work-proj/{t_file}.jsonl"),
        "{\"type\":\"user\",\"entrypoint\":\"cli\",\"timestamp\":\"2026-11-14T03:00:00Z\",\"cwd\":\"/w/c\",\"message\":{\"content\":\"x\"}}\n{\"type\":\"ai-title\",\"aiTitle\":\"AI 제목\"}\n",
    );
    write(
        &format!(".claude/projects/-work-proj/{t_file}/custom-title.json"),
        r#"{"customTitle":"폴더의 제목"}"#,
    );
    set_age(&format!(".claude/projects/-work-proj/{t_file}.jsonl"), 7200);
    let t_none = "dddddddd-1111-2222-3333-444444444444";
    write(
        &format!(".claude/projects/-other/{t_none}.jsonl"),
        "{\"type\":\"user\",\"entrypoint\":\"sdk-cli\",\"message\":{\"content\":\"x\"}}\n",
    );
    // 64KB를 넘는 파일: 머리와 꼬리 창만 본다(꼬리에서 branch, 머리에서 cwd)
    let big = "eeeeeeee-1111-2222-3333-444444444444";
    let mut big_text = String::from(
        "{\"type\":\"user\",\"entrypoint\":\"cli\",\"timestamp\":\"2026-11-14T04:00:00Z\",\"cwd\":\"/w/big\",\"gitBranch\":\"head-branch\",\"message\":{\"content\":\"머리\"}}\n",
    );
    for i in 0..1400 {
        big_text.push_str(&format!(
            "{{\"type\":\"assistant\",\"message\":{{\"content\":\"채움 {i} 가나다라마바사\"}}}}\n"
        ));
    }
    big_text.push_str("{\"type\":\"user\",\"gitBranch\":\"tail\\u0020branch\",\"message\":{\"content\":\"꼬리\"}}\n");
    big_text.push_str("{\"type\":\"custom-title\",\"customTitle\":\"꼬리의 직접 제목\"}\n");
    write(
        &format!(".claude/projects/-work-proj/{big}.jsonl"),
        &big_text,
    );
    set_age(&format!(".claude/projects/-work-proj/{big}.jsonl"), 100);
    // 오래된 세션(7일 밖)과 깨진 UTF-8 바이트
    let old = "ffffffff-1111-2222-3333-444444444444";
    write(
        &format!(".claude/projects/-work-proj/{old}.jsonl"),
        "{\"type\":\"user\",\"entrypoint\":\"cli\",\"message\":{\"content\":\"옛\"}}\n",
    );
    set_age(
        &format!(".claude/projects/-work-proj/{old}.jsonl"),
        40 * 86_400,
    );
    let broken = "99999999-1111-2222-3333-444444444444";
    let mut bytes = b"{\"type\":\"user\",\"entrypoint\":\"cli\",\"message\":{\"content\":\"\xff\xfe\xe2\x82 \xEB\x81\x9D\"}}\n".to_vec();
    bytes.extend_from_slice("{\"type\":\"assistant\",\"message\":{\"content\":\"답\"}}".as_bytes());
    let path = home.join(format!(".claude/projects/-work-proj/{broken}.jsonl"));
    fs::write(&path, bytes).unwrap();

    // ---- Codex ----
    let cx_new = "11111111-aaaa-bbbb-cccc-111111111111";
    let codex_new = [
        r#"{"timestamp":"2026-11-14T05:00:00Z","type":"session_meta","payload":{"cwd":"/work/codex","id":"x"}}"#,
        r#"{"timestamp":"2026-11-14T05:00:01Z","type":"turn_context","payload":{"model":"gpt-5","cwd":"/ignored"}}"#,
        r#"{"timestamp":"2026-11-14T05:00:02Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"UserMessage","content":[{"type":"input_text","text":"코덱스에게"},{"type":"text","text":"둘째 조각"}]}}}"#,
        r#"{"timestamp":"2026-11-14T05:00:03Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"AgentMessage","content":"그냥 글"}}}"#,
        r#"{"timestamp":"2026-11-14T05:00:04Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","command":["/bin/zsh","-lc","cargo test\n--all"],"exit_code":1,"duration":{"secs":2,"nanos":340000000}}}}"#,
        r#"{"timestamp":"2026-11-14T05:00:05Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","command":["ls","-la"],"exit_code":0}}}"#,
        r#"{"timestamp":"2026-11-14T05:00:06Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"McpToolCall","server":"srv","tool":"find","status":"failed","duration":{"secs":0,"nanos":50000000}}}}"#,
        r#"{"timestamp":"2026-11-14T05:00:07Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"McpToolCall","status":"completed"}}}"#,
        r#"{"timestamp":"2026-11-14T05:00:08Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"FileChange","status":"completed","changes":{"/a/b/c.rs":{},"d.md":{}}}}}"#,
        r#"{"timestamp":"2026-11-14T05:00:09Z","type":"event_msg","payload":{"type":"item_completed","item":{"type":"Reasoning"}}}"#,
        r#"{"timestamp":"2026-11-14T05:00:10Z","type":"event_msg","payload":{"type":"user_message","message":"새 형식이 있으면 무시"}}"#,
        "\"item_completed\" 깨진 줄",
        r#"{"type":"unrelated"}"#,
    ];
    write(
        &format!(".codex/sessions/2026/11/14/rollout-2026-11-14T05-00-00-{cx_new}.jsonl"),
        &(codex_new.join("\n") + "\n"),
    );
    let cx_old = "22222222-aaaa-bbbb-cccc-222222222222";
    let codex_old = [
        r#"{"timestamp":"2026-11-14T06:00:00Z","type":"session_meta","payload":{"cwd":"/work/codex-old"}}"#,
        r#"{"timestamp":"2026-11-14T06:00:01Z","type":"event_msg","payload":{"type":"user_message","message":"예전 질문 https://example.org/old"}}"#,
        r#"{"timestamp":"2026-11-14T06:00:02Z","type":"event_msg","payload":{"type":"agent_message","message":"예전 답"}}"#,
        r#"{"timestamp":"2026-11-14T06:00:03Z","type":"response_item","payload":{"type":"function_call","name":"shell","arguments":"{\"command\":[\"bash\",\"-lc\",\"git status\"]}"}}"#,
        r#"{"timestamp":"2026-11-14T06:00:04Z","type":"response_item","payload":{"type":"function_call","name":"raw","arguments":"JSON 아님"}}"#,
        r#"{"timestamp":"2026-11-14T06:00:05Z","type":"response_item","payload":{"type":"function_call","name":"nocommand","arguments":"{\"x\":1}"}}"#,
    ];
    write(
        &format!(".codex/sessions/2026/11/14/rollout-2026-11-14T06-00-00-{cx_old}.jsonl"),
        &(codex_old.join("\n") + "\n"),
    );

    // ---- Gemini CLI ----
    let gm = "33333333-aaaa-bbbb-cccc-333333333333";
    let gemini = [
        format!(r#"{{"sessionId":"{gm}","projectHash":"h","startTime":"2026-11-14T07:00:00Z","kind":"main"}}"#),
        r#"{"id":"m1","type":"user","timestamp":"2026-11-14T07:00:01Z","content":[{"text":"<session_context>환경</session_context>"}]}"#.to_string(),
        r#"{"id":"m2","type":"user","timestamp":"2026-11-14T07:00:02Z","content":[{"text":"제미나이 질문"}]}"#.to_string(),
        r#"{"id":"m3","type":"gemini","timestamp":"2026-11-14T07:00:03Z","model":"gemini-3","content":"답","toolCalls":[{"name":"read_file","args":{"file_path":"/a/b.rs"},"status":"success"},{"name":"run","args":{"command":"ls"},"status":"error"},{"name":"nostatus","args":{}},{"name":"nullstatus","status":null,"args":{"dir_path":"/d"}},{"args":{}}]}"#.to_string(),
        r#"{"$set":{"lastUpdated":"x"}}"#.to_string(),
        r#"{"id":"m4","type":"user","timestamp":"2026-11-14T07:00:04Z","content":"되감을 질문"}"#.to_string(),
        r#"{"id":"m5","type":"gemini","timestamp":"2026-11-14T07:00:05Z","content":[{"text":"되감길 답"}]}"#.to_string(),
        r#"{"$rewindTo":"m4"}"#.to_string(),
        r#"{"$rewindTo":"unknown"}"#.to_string(),
        r#"{"id":"m6","type":"user","timestamp":"2026-11-14T07:00:06Z","content":"되감은 뒤 질문 https://gemini.example.com/doc"}"#.to_string(),
        "깨진 줄".to_string(),
    ];
    write(
        &format!(
            ".gemini/tmp/projx/chats/session-2026-11-14T07-00-{}.jsonl",
            &gm[..8]
        ),
        &(gemini.join("\n") + "\n"),
    );
    write(
        ".gemini/projects.json",
        r#"{"projects":{"/work/gemini":"projx","/other":"other"}}"#,
    );

    // ---- Antigravity ----
    let agy = "44444444-aaaa-bbbb-cccc-444444444444";
    make_antigravity_db(
        &home.join(format!(".gemini/antigravity-cli/conversations/{agy}.db")),
        "/work/agy dir%20x",
    );

    let ids = [
        a, t_custom, t_file, t_none, big, old, broken, cx_new, cx_old, gm, agy,
    ];
    let step = |h: &mut WriteHarness, label: &str, command: &str, java: &[&str], rust: Value| {
        h.step(&format!("세션 {label}"), java, command, rust);
    };
    for id in ids {
        step(
            h,
            &format!("읽기 {id}"),
            "read_session",
            &["read-session", id],
            json!({"id": id}),
        );
        step(
            h,
            &format!("읽기 주소 {id}"),
            "read_session",
            &["read-session", id, "urls=true", "limit=2"],
            json!({"id": id, "urls": "true", "limit": "2"}),
        );
    }
    step(
        h,
        "읽기 before",
        "read_session",
        &["read-session", a, "before=3", "limit=1"],
        json!({"id": a, "before": "3", "limit": "1"}),
    );
    step(
        h,
        "읽기 before 범위 밖",
        "read_session",
        &["read-session", a, "before=999", "limit=0"],
        json!({"id": a, "before": "999", "limit": "0"}),
    );
    step(
        h,
        "읽기 limit 큰 값",
        "read_session",
        &["read-session", a, "limit=100000", "before=-5"],
        json!({"id": a, "limit": "100000", "before": "-5"}),
    );
    step(
        h,
        "읽기 노트 언급",
        "read_session",
        &["read-session", a, &format!("note={note}")],
        json!({"id": a, "note": note}),
    );
    step(
        h,
        "읽기 글쓴이",
        "read_session",
        &[
            "read-session",
            a,
            &format!("author=ndnone0000-0000,{note}"),
            "limit=1",
        ],
        json!({"id": a, "author": format!("ndnone0000-0000,{note}"), "limit": "1"}),
    );
    step(
        h,
        "읽기 글쓴이 없음",
        "read_session",
        &["read-session", a, "author=ndnone0000-0000"],
        json!({"id": a, "author": "ndnone0000-0000"}),
    );
    step(
        h,
        "읽기 노트 언급 Codex",
        "read_session",
        &["read-session", cx_new, "note=ndclaude01-0001"],
        json!({"id": cx_new, "note": "ndclaude01-0001"}),
    );
    step(
        h,
        "읽기 없는 세션",
        "read_session",
        &[
            "read-session",
            "12345678-0000-0000-0000-000000000000",
            "note=ndclaude01-0001",
        ],
        json!({"id": "12345678-0000-0000-0000-000000000000", "note": "ndclaude01-0001"}),
    );
    for (label, java, rust) in [
        ("인자 없음", vec!["read-session"], json!({})),
        (
            "세션 id 모양",
            vec!["read-session", "nope"],
            json!({"id": "nope"}),
        ),
        (
            "세션 id 공백",
            vec!["read-session", " aaaaaaaa-1111"],
            json!({"id": " aaaaaaaa-1111"}),
        ),
        (
            "limit 글",
            vec!["read-session", a, "limit=x"],
            json!({"id": a, "limit": "x"}),
        ),
        (
            "before 글",
            vec!["read-session", a, "before=1.5"],
            json!({"id": a, "before": "1.5"}),
        ),
        (
            "urls 글",
            vec!["read-session", a, "urls=yes"],
            json!({"id": a, "urls": "yes"}),
        ),
        (
            "note 모양",
            vec!["read-session", a, "note=BAD"],
            json!({"id": a, "note": "BAD"}),
        ),
        (
            "author 모양",
            vec!["read-session", a, "author=ndnone0000-0000, ndclaude01-0001"],
            json!({"id": a, "author": "ndnone0000-0000, ndclaude01-0001"}),
        ),
        (
            "author 9개",
            vec![
                "read-session",
                a,
                "author=a000000000,a000000001,a000000002,a000000003,a000000004,a000000005,a000000006,a000000007,a000000008",
            ],
            json!({"id": a, "author": "a000000000,a000000001,a000000002,a000000003,a000000004,a000000005,a000000006,a000000007,a000000008"}),
        ),
    ] {
        step(
            h,
            &format!("읽기 오류 {label}"),
            "read_session",
            &java,
            rust,
        );
    }
    // 제목: 한 번에 여럿(Claude, Codex, Gemini, Antigravity, 없는 것 섞어서)
    let mut titles_java: Vec<&str> = vec!["session-titles"];
    titles_java.extend(ids.iter().copied());
    titles_java.push("12345678-0000-0000-0000-000000000000");
    let mut titles_ids: Vec<Value> = ids.iter().map(|i| json!(i)).collect();
    titles_ids.push(json!("12345678-0000-0000-0000-000000000000"));
    step(
        h,
        "제목 여럿",
        "session_titles",
        &titles_java,
        json!({"ids": titles_ids}),
    );
    step(
        h,
        "제목 하나",
        "session_titles",
        &["session-titles", gm],
        json!({"ids": [gm]}),
    );
    step(
        h,
        "제목 없음",
        "session_titles",
        &["session-titles"],
        json!({"ids": []}),
    );
    step(
        h,
        "제목 모양 오류",
        "session_titles",
        &["session-titles", a, "nope"],
        json!({"ids": [a, "nope"]}),
    );
    let many: Vec<String> = (0..201).map(|i| format!("{i:08x}-0000")).collect();
    let mut many_java: Vec<&str> = vec!["session-titles"];
    many_java.extend(many.iter().map(String::as_str));
    step(
        h,
        "제목 201개",
        "session_titles",
        &many_java,
        json!({"ids": many}),
    );
    // 최근 세션: `lastActiveAt`은 세션 파일의 수정 시각이라 실행 때마다 달라진다. 골든 재생이 녹화본과 같은 값을 보도록 고정한다.
    pin_session_mtimes(&h.session_home());
    step(
        h,
        "최근 기본",
        "recent_sessions",
        &["recent-sessions"],
        json!({}),
    );
    step(
        h,
        "최근 1일",
        "recent_sessions",
        &["recent-sessions", "days=1"],
        json!({"days": "1"}),
    );
    step(
        h,
        "최근 30일",
        "recent_sessions",
        &["recent-sessions", "days=30"],
        json!({"days": 30}),
    );
    step(
        h,
        "최근 days 0",
        "recent_sessions",
        &["recent-sessions", "days=0"],
        json!({"days": "0"}),
    );
    step(
        h,
        "최근 days 31",
        "recent_sessions",
        &["recent-sessions", "days=31"],
        json!({"days": "31"}),
    );
    step(
        h,
        "최근 days 글",
        "recent_sessions",
        &["recent-sessions", "days=x"],
        json!({"days": "x"}),
    );
    // 합성 기록이 비어 있어서 양쪽이 똑같이 빈 응답을 내는 일이 아닌지 가려낸다
    for (id, least) in [(a, 8), (cx_new, 4), (cx_old, 3), (gm, 3), (agy, 3)] {
        let messages = Engine::new(&h.b)
            .call("read_session", json!({"id": id}))
            .ok()
            .and_then(|v| v["source"]["messages"].as_u64())
            .unwrap_or(0);
        h.report.record(
            format!("세션 합성 기록이 비어 있지 않다 {id}"),
            messages >= least,
            || format!("회차 {messages}개"),
        );
    }
    let _ = fs::remove_dir_all(&home);
}

// ---------------------------------------------------------------- 노트 잔여 명령(4e)

/// 노트 도구 명령 맞대기: 첨부 쓰기, 마크다운 내보내기, 사실층 요약, 데일리 플랜, 답 없는 코멘트 점검, 샤드 이동과 검증.
fn compare_note_tools(h: &mut WriteHarness) {
    let step = |h: &mut WriteHarness, label: &str, command: &str, java: &[&str], rust: Value| {
        h.step(&format!("노트도구 {label}"), java, command, rust)
    };
    // 앞 시나리오가 노트를 지우고 옮겨서, 지금 남아 있는 폴더 노트와 후속 노트를 색인에서 고른다
    let (parent, sub) = {
        let index = ais_core::vault::index::NoteIndex::fresh(&h.b);
        let root = index
            .root_notes()
            .find(|n| {
                !n.is_single_file
                    && !n.is_extension_folder
                    && !n.sub_notes.is_empty()
                    && n.meta.tier.is_reading()
            })
            .or_else(|| {
                index
                    .root_notes()
                    .find(|n| !n.is_single_file && !n.is_extension_folder)
            })
            .expect("폴더 노트가 있다");
        let sub = root
            .sub_notes
            .iter()
            .find(|s| !s.is_single_file)
            .map(|s| s.id.clone());
        (root.id.clone(), sub)
    };
    let single = {
        let index = ais_core::vault::index::NoteIndex::fresh(&h.b);
        index
            .root_notes()
            .find(|n| n.is_single_file)
            .map(|n| n.id.clone())
    };

    // ---- write_attachment ----
    step(
        h,
        "첨부 새 파일",
        "write_attachment",
        &[
            "write-attachment",
            &parent,
            "name=메모.md",
            "content=# 제목\n본문 😀\n",
        ],
        json!({"id": parent, "name": "메모.md", "content": "# 제목\n본문 😀\n"}),
    );
    step(
        h,
        "첨부 같은 이름 덮기",
        "write_attachment",
        &[
            "write-attachment",
            &parent,
            "name=메모.md",
            "content=덮었다",
        ],
        json!({"id": parent, "name": "메모.md", "content": "덮었다"}),
    );
    step(
        h,
        "첨부 빈 내용",
        "write_attachment",
        &["write-attachment", &parent, "name=  빈.TXT  ", "content="],
        json!({"id": parent, "name": "  빈.TXT  ", "content": ""}),
    );
    step(
        h,
        "첨부 SVG",
        "write_attachment",
        &[
            "write-attachment",
            &parent,
            "name=그림.svg",
            "content=<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
        ],
        json!({"id": parent, "name": "그림.svg", "content": "<svg xmlns=\"http://www.w3.org/2000/svg\"/>"}),
    );
    step(
        h,
        "첨부 HTML",
        "write_attachment",
        &[
            "write-attachment",
            &parent,
            "name=페이지.html",
            "content=<p>안녕 &amp; 반가워</p>",
        ],
        json!({"id": parent, "name": "페이지.html", "content": "<p>안녕 &amp; 반가워</p>"}),
    );
    if let Some(sub) = &sub {
        step(
            h,
            "첨부 후속 노트",
            "write_attachment",
            &[
                "write-attachment",
                &parent,
                sub,
                "name=sub.json",
                "content={\"a\":1}",
            ],
            json!({"id": parent, "subId": sub, "name": "sub.json", "content": "{\"a\":1}"}),
        );
        step(
            h,
            "첨부 후속 노트 빈 칸",
            "write_attachment",
            &[
                "write-attachment",
                &parent,
                " ",
                "name=blank-sub.csv",
                "content=a,b",
            ],
            json!({"id": parent, "subId": " ", "name": "blank-sub.csv", "content": "a,b"}),
        );
    }
    if let Some(single) = &single {
        step(
            h,
            "첨부 단일 파일 노트",
            "write_attachment",
            &["write-attachment", single, "name=a.txt", "content=x"],
            json!({"id": single, "name": "a.txt", "content": "x"}),
        );
    }
    let long_name = format!("{}.txt", "가".repeat(118));
    let too_long = format!("{}.txt", "가".repeat(117));
    for (label, java, rust) in [
        ("인자 없음", vec!["write-attachment"], json!({})),
        (
            "노트 id 모양",
            vec!["write-attachment", "nope", "name=a.txt", "content=x"],
            json!({"id": "nope", "name": "a.txt", "content": "x"}),
        ),
        (
            "후속 id 모양",
            vec![
                "write-attachment",
                &parent,
                "nope",
                "name=a.txt",
                "content=x",
            ],
            json!({"id": parent, "subId": "nope", "name": "a.txt", "content": "x"}),
        ),
        (
            "name 없음",
            vec!["write-attachment", &parent, "content=x"],
            json!({"id": parent, "content": "x"}),
        ),
        (
            "content 없음",
            vec!["write-attachment", &parent, "name=a.txt"],
            json!({"id": parent, "name": "a.txt"}),
        ),
        (
            "이름 빈 값",
            vec!["write-attachment", &parent, "name=  ", "content=x"],
            json!({"id": parent, "name": "  ", "content": "x"}),
        ),
        (
            "이름 길이",
            vec![
                "write-attachment",
                &parent,
                &format!("name={too_long}"),
                "content=x",
            ],
            json!({"id": parent, "name": too_long, "content": "x"}),
        ),
        (
            "이름 경계 길이",
            vec![
                "write-attachment",
                &parent,
                &format!("name={long_name}"),
                "content=x",
            ],
            json!({"id": parent, "name": long_name, "content": "x"}),
        ),
        (
            "이름 슬래시",
            vec!["write-attachment", &parent, "name=a/b.txt", "content=x"],
            json!({"id": parent, "name": "a/b.txt", "content": "x"}),
        ),
        (
            "이름 점점",
            vec!["write-attachment", &parent, "name=a..txt", "content=x"],
            json!({"id": parent, "name": "a..txt", "content": "x"}),
        ),
        (
            "이름 점 시작",
            vec!["write-attachment", &parent, "name=.a.txt", "content=x"],
            json!({"id": parent, "name": ".a.txt", "content": "x"}),
        ),
        (
            "이름 제어문자",
            vec!["write-attachment", &parent, "name=a\u{1}b.txt", "content=x"],
            json!({"id": parent, "name": "a\u{1}b.txt", "content": "x"}),
        ),
        (
            "이름 확장자 없음",
            vec!["write-attachment", &parent, "name=noext", "content=x"],
            json!({"id": parent, "name": "noext", "content": "x"}),
        ),
        (
            "이름 이미지 확장자",
            vec!["write-attachment", &parent, "name=a.png", "content=x"],
            json!({"id": parent, "name": "a.png", "content": "x"}),
        ),
        (
            "이름 끝 점",
            vec!["write-attachment", &parent, "name=a.", "content=x"],
            json!({"id": parent, "name": "a.", "content": "x"}),
        ),
        (
            "없는 노트",
            vec![
                "write-attachment",
                "ndzz0001-00000001",
                "name=a.txt",
                "content=x",
            ],
            json!({"id": "ndzz0001-00000001", "name": "a.txt", "content": "x"}),
        ),
        (
            "없는 후속 노트",
            vec![
                "write-attachment",
                &parent,
                "snzz0001-00000001",
                "name=a.txt",
                "content=x",
            ],
            json!({"id": parent, "subId": "snzz0001-00000001", "name": "a.txt", "content": "x"}),
        ),
    ] {
        step(
            h,
            &format!("첨부 오류 {label}"),
            "write_attachment",
            &java,
            rust,
        );
    }

    // ---- facts_brief ----
    step(h, "사실층 기본", "facts_brief", &["facts-brief"], json!({}));
    step(
        h,
        "사실층 구간",
        "facts_brief",
        &[
            "facts-brief",
            "from=2026-11-01",
            "to=2026-11-30",
            "min-new-file=1",
        ],
        json!({"from": "2026-11-01", "to": "2026-11-30", "min-new-file": "1"}),
    );
    step(
        h,
        "사실층 하루",
        "facts_brief",
        &[
            "facts-brief",
            "from=2026-11-07",
            "to=2026-11-07",
            "min-new-file=1",
        ],
        json!({"from": "2026-11-07", "to": "2026-11-07", "min-new-file": 1}),
    );
    step(
        h,
        "사실층 큰 최소",
        "facts_brief",
        &[
            "facts-brief",
            "from=2026-01-01",
            "to=2026-12-31",
            "min-new-file=1000",
        ],
        json!({"from": "2026-01-01", "to": "2026-12-31", "min-new-file": "1000"}),
    );
    for (label, java, rust) in [
        (
            "from 모양",
            vec!["facts-brief", "from=2026-1-1"],
            json!({"from": "2026-1-1"}),
        ),
        (
            "to 달력 밖",
            vec!["facts-brief", "to=2026-02-30"],
            json!({"to": "2026-02-30"}),
        ),
        (
            "뒤집힘",
            vec!["facts-brief", "from=2026-12-01", "to=2026-11-01"],
            json!({"from": "2026-12-01", "to": "2026-11-01"}),
        ),
        (
            "min 글",
            vec!["facts-brief", "min-new-file=x"],
            json!({"min-new-file": "x"}),
        ),
        (
            "min 0",
            vec!["facts-brief", "min-new-file=0"],
            json!({"min-new-file": "0"}),
        ),
    ] {
        step(
            h,
            &format!("사실층 오류 {label}"),
            "facts_brief",
            &java,
            rust,
        );
    }
    put_both(
        h,
        ".aisurada/facts/demo.md",
        "# 데모 사실층\n\n\n첫 줄\r\n둘째 줄  \n\n",
    );
    step(
        h,
        "사실층 제목 줄 떼기",
        "facts_brief",
        &[
            "facts-brief",
            "from=2026-11-01",
            "to=2026-11-30",
            "min-new-file=1",
        ],
        json!({"from": "2026-11-01", "to": "2026-11-30", "min-new-file": "1"}),
    );

    // ---- plan_day, check_notes ----
    step(
        h,
        "플랜 미리 보기",
        "plan_day",
        &["plan-day", "date=2026-11-08"],
        json!({"date": "2026-11-08"}),
    );
    step(h, "플랜 오늘", "plan_day", &["plan-day"], json!({}));
    step(
        h,
        "플랜 dryRun",
        "plan_day",
        &["plan-day", "date=2026-11-10", "agenda=demo", "dryRun=true"],
        json!({"date": "2026-11-10", "agenda": "demo", "dryRun": "true"}),
    );
    step(
        h,
        "플랜 저장",
        "plan_day",
        &["plan-day", "date=2026-11-09", "agenda=demo"],
        json!({"date": "2026-11-09", "agenda": "demo"}),
    );
    step(
        h,
        "플랜 새 아젠다에 저장",
        "plan_day",
        &["plan-day", "date=2026-11-12", "agenda=새 플랜 아젠다"],
        json!({"date": "2026-11-12", "agenda": "새 플랜 아젠다"}),
    );
    step(
        h,
        "플랜 오류 날짜",
        "plan_day",
        &["plan-day", "date=내일"],
        json!({"date": "내일"}),
    );
    step(
        h,
        "플랜 오류 dryRun",
        "plan_day",
        &["plan-day", "dryRun=maybe"],
        json!({"dryRun": "maybe"}),
    );
    step(
        h,
        "점검 코멘트 없음",
        "check_notes",
        &["check-notes", "date=2026-11-14"],
        json!({"date": "2026-11-14"}),
    );
    // 편집 도장이 옛날인 노트와 도장이 없는 새 노트에 코멘트를 달면 「답 없는 코멘트」로 잡힌다
    put_both(
        h,
        "notes/work/check-note/note.md",
        "---\nid: ndfx9990-00009990\nagenda: work\ntype: plan\ntitle: 점검 노트\ncreated: '2026-11-10T08:00:00+09:00'\neditedBy: app\neditedAt: '2026-11-10T09:00:00+09:00'\n---\n본문\n",
    );
    for id in ["ndfx9990-00009990", parent.as_str()] {
        step(
            h,
            &format!("코멘트 만들기 {id}"),
            "create_comment",
            &["create-comment", id, "body=답 없는 코멘트"],
            json!({"id": id, "body": "답 없는 코멘트"}),
        );
    }
    step(
        h,
        "점검 코멘트 뒤",
        "check_notes",
        &["check-notes", "date=2026-11-15"],
        json!({"date": "2026-11-15"}),
    );
    step(
        h,
        "점검 날짜 밖",
        "check_notes",
        &["check-notes", "date=2027-06-01"],
        json!({"date": "2027-06-01"}),
    );
    step(
        h,
        "점검 기본 날짜",
        "check_notes",
        &["check-notes"],
        json!({}),
    );
    step(
        h,
        "점검 오류 날짜",
        "check_notes",
        &["check-notes", "date=2026-13-01"],
        json!({"date": "2026-13-01"}),
    );

    // ---- export_markdown ----
    step(
        h,
        "내보내기",
        "export_markdown",
        &["export-markdown"],
        json!({}),
    );
    put_both(h, ".aisurada/md-export/남은-파일.md", "지워져야 한다");
    step(
        h,
        "내보내기 다시(옛 파일 지움)",
        "export_markdown",
        &["export-markdown"],
        json!({}),
    );

    // ---- verify_shard 오류, shard_notes 계획과 이동, 다시 검증 ----
    step(
        h,
        "검증 매핑 없음",
        "verify_shard",
        &["verify-shard"],
        json!({}),
    );
    step(
        h,
        "검증 없는 파일",
        "verify_shard",
        &["verify-shard", "file=nope.tsv"],
        json!({"file": "nope.tsv"}),
    );
    // SAFETY: 단일 스레드이고, Java 단발 프로세스는 부모의 환경을 물려받는다.
    unsafe {
        env::set_var("AISURADA_NOTES_SHARD_BY_MONTH", "true");
    }
    step(h, "샤드 계획", "shard_notes", &["shard-notes"], json!({}));
    step(
        h,
        "샤드 apply 오류",
        "shard_notes",
        &["shard-notes", "apply=yes"],
        json!({"apply": "yes"}),
    );
    put_both(h, "notes/work/깨진-노트/note.md", "머리말 없음\n");
    put_both(
        h,
        "notes/work/날짜-밖/note.md",
        "---\nid: ndfx9999-00009999\ntype: plan\ntitle: 날짜 밖\ncreated: 2026-02-30T09:00:00+09:00\n---\n본문\n",
    );
    put_both(
        h,
        "notes/work/충돌 (conflicted copy)/note.md",
        "---\nid: ndfx9998-00009998\ntype: plan\ntitle: 충돌\ncreated: '2026-11-01T09:00:00+09:00'\n---\n",
    );
    put_both(
        h,
        "notes/work/loose-note/note.md",
        "---\nid: ndfx9997-00009997\nagenda: work\ntype: plan\ntitle: 느슨한 노트\ncreated: '2026-11-03T09:00:00+09:00'\n---\n본문\n",
    );
    put_both(h, "notes/work/loose-note/attach/a.txt", "첨부");
    put_both(
        h,
        "notes/work/loose-file.md",
        "---\nid: ndfx9996-00009996\nagenda: work\ntype: plan\ntitle: 단일\ncreated: '2026-10-03T09:00:00+09:00'\n---\n본문\n",
    );
    step(
        h,
        "샤드 계획 두 번째",
        "shard_notes",
        &["shard-notes"],
        json!({}),
    );
    step(
        h,
        "샤드 이동",
        "shard_notes",
        &["shard-notes", "apply=true"],
        json!({"apply": "true"}),
    );
    step(
        h,
        "샤드 이동 다시(이미 자리)",
        "shard_notes",
        &["shard-notes", "apply=true"],
        json!({"apply": true}),
    );
    step(
        h,
        "검증 기준 파일 둘",
        "verify_shard",
        &["verify-shard"],
        json!({}),
    );
    let maps: Vec<String> = fs::read_dir(h.b.join(".aisurada/migration"))
        .map(|d| {
            d.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with("shard-map-"))
                .collect()
        })
        .unwrap_or_default();
    let mut maps = maps;
    maps.sort();
    if let Some(first) = maps.first() {
        step(
            h,
            "검증 첫 회차",
            "verify_shard",
            &["verify-shard", &format!("file={first}")],
            json!({"file": first}),
        );
    }
    if let Some(second) = maps.get(1) {
        step(
            h,
            "검증 둘째 회차",
            "verify_shard",
            &["verify-shard", &format!("file={second}")],
            json!({"file": second}),
        );
    }
    // 이동한 노트를 고치고 지워 어긋남을 만든다
    put_both(
        h,
        "notes/work/2026-11/loose-note/attach/a.txt",
        "첨부가 바뀌었다",
    );
    put_both(h, "notes/work/2026-11/loose-note/attach/extra.txt", "더");
    if let Some(first) = maps.first() {
        step(
            h,
            "검증 크기 어긋남",
            "verify_shard",
            &["verify-shard", &format!("file={first}")],
            json!({"file": first}),
        );
        for root in [&h.a, &h.b] {
            let _ = fs::remove_dir_all(root.join("notes/work/2026-10"));
        }
        step(
            h,
            "검증 자리 없음",
            "verify_shard",
            &["verify-shard", &format!("file={first}")],
            json!({"file": first}),
        );
        put_both(h, "notes/work/loose-file.md", "옛 자리에도 남았다");
        step(
            h,
            "검증 옛 자리에 남음",
            "verify_shard",
            &["verify-shard", &format!("file={first}")],
            json!({"file": first}),
        );
    }
    // 양쪽이 똑같이 아무 일도 안 해서 맞아 보이는 일이 아닌지 가려낸다
    let engine = Engine::new(&h.b);
    let checks: Vec<(&str, bool)> = vec![
        (
            "내보낸 마크다운이 있다",
            fs::read_dir(h.b.join(".aisurada/md-export")).is_ok_and(|d| d.count() > 0),
        ),
        (
            "샤드 이동이 일어났다",
            h.b.join("notes/work/2026-11/loose-note/note.md").exists()
                || h.b.join("notes/work/2026-10/loose-file.md").exists(),
        ),
        (
            "답 없는 코멘트가 잡힌다",
            engine
                .call("check_notes", json!({"date": "2026-11-15"}))
                .is_ok_and(|v| v["count"].as_u64().unwrap_or(0) >= 1),
        ),
        (
            "사실층 요약에 아젠다가 든다",
            engine
                .call(
                    "facts_brief",
                    json!({"from": "2026-11-01", "to": "2026-11-30", "min-new-file": 1}),
                )
                .is_ok_and(|v| v["count"].as_u64().unwrap_or(0) >= 1),
        ),
        (
            "첨부가 쓰였다",
            fs::read_dir(
                h.b.join(
                    ais_core::vault::index::NoteIndex::fresh(&h.b)
                        .note(&parent)
                        .map(|n| n.rel_path.clone())
                        .unwrap_or_default(),
                )
                .join("attach"),
            )
            .is_ok_and(|d| d.count() >= 3),
        ),
    ];
    for (label, ok) in checks {
        h.report.record(
            format!("노트도구 비어 있지 않다: {label}"),
            ok,
            || "일이 안 일어났다".to_string(),
        );
    }
    // SAFETY: 단일 스레드이다.
    unsafe {
        env::remove_var("AISURADA_NOTES_SHARD_BY_MONTH");
    }
}

fn compare_writes(report: &mut Report, java: &Java, fixture: &Path) {
    let root = env::temp_dir().join(format!("ais-write-compat-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("임시 폴더");
    java.tape.borrow_mut().register_root(&root);
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
        env::set_var("AISURADA_APPROVALS_DIR", root.join("approvals-rust"));
        env::set_var("AISURADA_CLAUDE_HOME", root.join("session-home"));
        env::set_var("AISURADA_CODEX_HOME", root.join("session-home"));
        env::set_var("AISURADA_GEMINI_HOME", root.join("session-home"));
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
    // 자원 명령만 빠르게 돌려 볼 때(`HARNESS_ONLY=resources`): 앞 시나리오를 건너뛴다
    if env::var("HARNESS_ONLY").as_deref() == Ok("tools") {
        compare_note_tools(&mut h);
        println!(
            "  (노트 도구 단계만: 쓰기 단계 {}개 가운데 Java가 성공으로 답한 것 {}개)",
            h.step, h.succeeded
        );
        let _ = fs::remove_dir_all(&root);
        return;
    }
    if env::var("HARNESS_ONLY").as_deref() == Ok("session") {
        compare_session_commands(&mut h);
        println!(
            "  (세션 단계만: 쓰기 단계 {}개 가운데 Java가 성공으로 답한 것 {}개)",
            h.step, h.succeeded
        );
        let _ = fs::remove_dir_all(&root);
        return;
    }
    if env::var("HARNESS_ONLY").as_deref() == Ok("ask") {
        compare_ask_chat_commands(&mut h);
        println!(
            "  (문답·대화 단계만: 쓰기 단계 {}개 가운데 Java가 성공으로 답한 것 {}개)",
            h.step, h.succeeded
        );
        let _ = fs::remove_dir_all(&root);
        return;
    }
    if env::var("HARNESS_ONLY").as_deref() == Ok("resources") {
        let skill = compare_resource_commands(&mut h);
        compare_resource_edit_error_order(&mut h, &skill);
        println!(
            "  (자원 단계만: 쓰기 단계 {}개 가운데 Java가 성공으로 답한 것 {}개)",
            h.step, h.succeeded
        );
        let _ = fs::remove_dir_all(&root);
        return;
    }
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
    compare_sub_note_writes(&mut h);
    compare_comment_writes(&mut h);
    compare_trash_writes(&mut h);
    compare_task_commands(&mut h);
    let skill = compare_resource_commands(&mut h);
    compare_ask_chat_commands(&mut h);
    compare_session_commands(&mut h);
    compare_note_tools(&mut h);
    compare_resource_edit_error_order(&mut h, &skill);

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
    let record: Option<PathBuf> = args
        .iter()
        .find_map(|a| a.strip_prefix("--record="))
        .map(PathBuf::from);
    let replay: Option<PathBuf> = args
        .iter()
        .find_map(|a| a.strip_prefix("--replay="))
        .map(PathBuf::from);
    let write_fixture: Option<PathBuf> = args
        .iter()
        .find_map(|a| a.strip_prefix("--write-fixture="))
        .map(PathBuf::from);
    let commit_fixture: Option<PathBuf> = args
        .iter()
        .find_map(|a| a.strip_prefix("--commit-fixture="))
        .map(PathBuf::from);
    // 재생은 기준 엔진(jar)이 없으므로 위치 인자가 모두 vault다
    let (jar, vaults): (String, &[&String]) = if replay.is_some() {
        (String::new(), &positional[..])
    } else {
        match positional.split_first() {
            Some((jar, vaults)) if !vaults.is_empty() => ((*jar).clone(), vaults),
            _ => {
                eprintln!(
                    "사용법: compat_check <기준 ais-engine.jar(build-oracle.sh)> <vault 폴더>... [--verbose] [--record=<테이프>]\n        compat_check --replay=<테이프> <vault 폴더>..."
                );
                return ExitCode::from(2);
            }
        }
    };
    if replay.is_some() && vaults.is_empty() {
        eprintln!("사용법: compat_check --replay=<테이프> <vault 폴더>...");
        return ExitCode::from(2);
    }
    let home = env::temp_dir().join("ais-compat-home");
    fs::create_dir_all(&home).ok();
    let tape = match (&replay, &record) {
        (Some(path), _) => match tape::Tape::replay(path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::from(2);
            }
        },
        (None, Some(path)) => tape::Tape::record(path.clone()),
        (None, None) => tape::Tape::live(),
    };
    let java = Java {
        jar,
        home,
        tape: RefCell::new(tape),
    };

    let mut total = Report::default();
    for vault in vaults {
        let vault = Path::new(vault);
        java.tape.borrow_mut().register_root(vault);
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
    // 어긋남이 있으면 녹화를 버린다(어긋난 응답을 정답으로 박제하지 않는다). 재생은 테이프가 남지 않았는지 본다.
    if total.failures.is_empty() {
        match java.tape.borrow().finish() {
            Ok(()) if java.tape.borrow().is_record() => println!(
                "테이프 {}줄을 적었다: {}",
                java.tape.borrow().recorded_len(),
                record
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
            ),
            Ok(()) => {}
            Err(e) => {
                println!("테이프 오류: {e}");
                return ExitCode::FAILURE;
            }
        }
    }
    if total.failures.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
