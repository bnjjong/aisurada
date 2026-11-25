//! C4 아키텍처 모델 명령(`get_c4_model`, `apply_c4_changes`)을 엔진 입구로 시험한다.
//! 파일 위치, 읽은 뒤 바뀜 검사, 한 번에 적용하거나 전혀 적용하지 않기, 깨진 파일 보호, 손으로 쓴 칸 보존을 본다.

use std::path::{Path, PathBuf};

use ais_core::engine::Engine;
use ais_core::error::ErrorKind;
use serde_json::{Value, json};

fn isolate_writes() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let locks = std::env::temp_dir().join(format!("ais-c4-locks-{}", std::process::id()));
        // SAFETY: 이 시험 파일의 쓰기 시험만 이 함수를 부르고, 환경 변수는 처음 한 번만 정한다.
        unsafe {
            std::env::set_var("AISURADA_LOCK_DIR", locks);
            std::env::set_var("AISURADA_VAULT_AUTOCOMMIT_ENABLED", "false");
            std::env::set_var(
                "AISURADA_APPROVALS_DIR",
                std::env::temp_dir().join(format!("ais-c4-approvals-{}", std::process::id())),
            );
        }
    });
}

fn vault(name: &str) -> PathBuf {
    isolate_writes();
    let dir = std::env::temp_dir().join(format!("ais-c4-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    write(&dir, ".aisurada/workspace.json", "{}\n");
    write(
        &dir,
        ".aisurada/projects/demo.md",
        "---\nid: pjfixture-00000001\nslug: demo\ntitle: 데모\ntype: development\n---\n",
    );
    dir
}

fn write(vault: &Path, path: &str, text: &str) {
    let file = vault.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

fn get(engine: &Engine, project: &str) -> Value {
    engine
        .call("get_c4_model", json!({"project": project}))
        .unwrap()
}

fn apply(
    engine: &Engine,
    project: &str,
    f1: &str,
    operations: Value,
) -> Result<Value, ais_core::error::EngineError> {
    engine.call(
        "apply_c4_changes",
        json!({"project": project, "f1": f1, "operations": operations}),
    )
}

#[test]
fn a_new_project_has_an_empty_model_and_edits_write_two_files() {
    let vault = vault("write");
    let engine = Engine::new(&vault);
    let empty = get(&engine, "demo");
    assert_eq!(empty["elements"], json!([]));
    assert_eq!(empty["views"], json!({}));
    assert!(
        !vault.join(".aisurada/c4").exists(),
        "읽기만으로는 파일을 만들지 않는다"
    );

    let applied = apply(
        &engine,
        "demo",
        empty["precondition"].as_str().unwrap(),
        json!([
            {"op":"put_element","id":"me","kind":"person","name":"개발자"},
            {"op":"put_element","id":"app","kind":"system","name":"aisurada"},
            {"op":"put_relationship","source":"me","target":"app","label":"지시"},
            {"op":"set_positions","view":"context","positions":{"me":{"x":1,"y":2},"app":{"x":300,"y":2},"r1":{"x":150,"y":50}}}
        ]),
    )
    .unwrap();
    assert_eq!(applied["changed"], true);
    assert_eq!(applied["applied"], 4);
    assert_eq!(applied["createdRelationships"], json!(["r1"]));
    assert_eq!(applied["elements"][1]["level"], 1);

    let model = std::fs::read_to_string(vault.join(".aisurada/c4/demo/model.yaml")).unwrap();
    assert!(model.contains("name: aisurada"), "{model}");
    let views = std::fs::read_to_string(vault.join(".aisurada/c4/demo/views.yaml")).unwrap();
    assert!(views.contains("context:"), "{views}");

    // 쓴 응답의 토큰은 다시 읽은 토큰과 같다
    let again = get(&engine, "demo");
    assert_eq!(again["precondition"], applied["precondition"]);
    assert_eq!(again["elements"], applied["elements"]);
    assert_eq!(again["views"]["context"]["app"], json!({"x": 300, "y": 2}));
    assert_eq!(again["views"]["context"]["r1"], json!({"x": 150, "y": 50}));
}

#[test]
fn a_stale_or_missing_token_writes_nothing() {
    let vault = vault("token");
    let engine = Engine::new(&vault);
    let first = get(&engine, "demo");
    let token = first["precondition"].as_str().unwrap().to_string();
    let put = json!([{"op":"put_element","id":"a","kind":"system","name":"A"}]);

    let missing = engine
        .call(
            "apply_c4_changes",
            json!({"project":"demo","operations":put}),
        )
        .unwrap_err();
    assert_eq!(missing.kind, ErrorKind::PreconditionFailed);
    let bad = apply(&engine, "demo", "abc", put.clone()).unwrap_err();
    assert_eq!(bad.kind, ErrorKind::PreconditionFailed);
    assert!(!vault.join(".aisurada/c4").exists());

    apply(&engine, "demo", &token, put.clone()).unwrap();
    // 같은 토큰을 다시 쓰면 그 사이 바뀐 것이 된다
    let stale = apply(&engine, "demo", &token, put).unwrap_err();
    assert_eq!(stale.kind, ErrorKind::PreconditionFailed);
    assert!(stale.message.contains("먼저 고쳤어요"), "{}", stale.message);

    // 손으로 파일을 고쳐도 토큰이 달라진다
    let now = get(&engine, "demo")["precondition"]
        .as_str()
        .unwrap()
        .to_string();
    let model = vault.join(".aisurada/c4/demo/model.yaml");
    let text = std::fs::read_to_string(&model).unwrap();
    std::fs::write(&model, text.replace("name: A", "name: B")).unwrap();
    let hand = apply(
        &engine,
        "demo",
        &now,
        json!([{"op":"delete_element","id":"a"}]),
    )
    .unwrap_err();
    assert_eq!(hand.kind, ErrorKind::PreconditionFailed);
}

#[test]
fn a_bad_batch_changes_nothing_and_names_the_operation() {
    let vault = vault("atomic");
    let engine = Engine::new(&vault);
    let token = get(&engine, "demo")["precondition"]
        .as_str()
        .unwrap()
        .to_string();
    let error = apply(
        &engine,
        "demo",
        &token,
        json!([
            {"op":"put_element","id":"a","kind":"system","name":"A"},
            {"op":"delete_element","id":"nobody"}
        ]),
    )
    .unwrap_err();
    assert_eq!(error.kind, ErrorKind::NotFound);
    assert!(
        error.message.starts_with("operations[1]:"),
        "{}",
        error.message
    );
    assert!(
        !vault.join(".aisurada/c4").exists(),
        "앞의 변경도 쓰지 않았다"
    );
    assert_eq!(get(&engine, "demo")["precondition"], json!(token));
}

#[test]
fn an_unknown_project_and_unsafe_names_are_refused() {
    let vault = vault("project");
    let engine = Engine::new(&vault);
    for name in ["nope", "../demo", "demo/../demo", ""] {
        let error = engine
            .call("get_c4_model", json!({"project": name}))
            .unwrap_err();
        assert!(
            matches!(error.kind, ErrorKind::NotFound | ErrorKind::InvalidArgument),
            "{name}: {error:?}"
        );
    }
    assert!(!vault.join(".aisurada/c4").exists());
}

#[test]
fn a_broken_file_stops_instead_of_being_overwritten_and_unknown_fields_survive() {
    let vault = vault("hand");
    let engine = Engine::new(&vault);
    write(
        &vault,
        ".aisurada/c4/demo/model.yaml",
        "version: 1\nelements:\n  - id: a\n    kind: system\n    name: A\n    owner: team-x\nrelationships: []\nnote: keep\n",
    );
    let read = get(&engine, "demo");
    let token = read["precondition"].as_str().unwrap().to_string();
    assert_eq!(read["elements"][0]["name"], "A");

    // 바뀐 것이 없는 변경은 파일을 다시 쓰지 않는다
    let same = apply(
        &engine,
        "demo",
        &token,
        json!([{"op":"put_element","id":"a","name":"A"}]),
    )
    .unwrap();
    assert_eq!(same["changed"], false);

    let edited = apply(
        &engine,
        "demo",
        &token,
        json!([{"op":"put_element","id":"a","description":"설명"}]),
    )
    .unwrap();
    assert_eq!(edited["changed"], true);
    let text = std::fs::read_to_string(vault.join(".aisurada/c4/demo/model.yaml")).unwrap();
    assert!(
        text.contains("owner: team-x") && text.contains("note: keep"),
        "{text}"
    );

    write(&vault, ".aisurada/c4/demo/model.yaml", "elements: [1, 2\n");
    let error = engine
        .call("get_c4_model", json!({"project": "demo"}))
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::InvalidArgument);
    assert_eq!(
        std::fs::read_to_string(vault.join(".aisurada/c4/demo/model.yaml")).unwrap(),
        "elements: [1, 2\n"
    );
}

#[test]
fn operations_may_come_from_the_command_line_as_a_json_string() {
    let vault = vault("cli");
    let engine = Engine::new(&vault);
    let token = get(&engine, "demo")["precondition"]
        .as_str()
        .unwrap()
        .to_string();
    let applied = engine
        .call(
            "apply_c4_changes",
            json!({"project":"demo","f1":token,"operations":"[{\"op\":\"put_element\",\"id\":\"a\",\"kind\":\"system\",\"name\":\"A\"}]"}),
        )
        .unwrap();
    assert_eq!(applied["elements"][0]["id"], "a");
}

#[test]
fn a_line_keeps_its_ports_in_the_file_and_in_the_answer() {
    let vault = vault("ports");
    let engine = Engine::new(&vault);
    let first = get(&engine, "demo");
    let applied = apply(
        &engine,
        "demo",
        first["precondition"].as_str().unwrap(),
        json!([
            {"op":"put_element","id":"me","kind":"person","name":"개발자"},
            {"op":"put_element","id":"app","kind":"system","name":"aisurada"},
            {"op":"put_relationship","source":"me","target":"app","label":"지시","sourcePort":"right","targetPort":"top-2"}
        ]),
    )
    .unwrap();
    assert_eq!(applied["relationships"][0]["sourcePort"], "right");
    assert_eq!(applied["relationships"][0]["targetPort"], "top-2");
    let file = std::fs::read_to_string(vault.join(".aisurada/c4/demo/model.yaml")).unwrap();
    assert!(
        file.contains("sourcePort: right") && file.contains("targetPort: top-2"),
        "{file}"
    );

    // 한쪽만 옮기면 다른 쪽은 그대로, 틀린 이름은 아무것도 쓰지 않고 거절한다
    let token = applied["precondition"].as_str().unwrap();
    let moved = apply(
        &engine,
        "demo",
        token,
        json!([{"op":"put_relationship","id":"r1","targetPort":"left"}]),
    )
    .unwrap();
    assert_eq!(moved["relationships"][0]["sourcePort"], "right");
    assert_eq!(moved["relationships"][0]["targetPort"], "left");
    let refused = apply(
        &engine,
        "demo",
        moved["precondition"].as_str().unwrap(),
        json!([{"op":"put_relationship","id":"r1","sourcePort":"middle"}]),
    )
    .unwrap_err();
    assert!(refused.message.contains("middle"), "{}", refused.message);
    assert_eq!(get(&engine, "demo")["precondition"], moved["precondition"]);
}
