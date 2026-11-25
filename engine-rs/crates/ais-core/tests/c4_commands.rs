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
        json!([{"op":"put_relationship","id":"r1","sourcePort":"left-1","targetPort":"right-3"}]),
    )
    .unwrap();
    assert_eq!(moved["relationships"][0]["sourcePort"], "left-1");
    assert_eq!(moved["relationships"][0]["targetPort"], "right-3");
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

fn call(engine: &Engine, name: &str, args: Value) -> Result<Value, ais_core::error::EngineError> {
    engine.call(name, args)
}

fn ids(list: &Value) -> Vec<String> {
    list["diagrams"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_project_always_lists_main_and_new_diagrams_get_numbered_ids() {
    let vault = vault("diagrams-list");
    let engine = Engine::new(&vault);
    let list = call(&engine, "list_c4_diagrams", json!({"project": "demo"})).unwrap();
    assert_eq!(ids(&list), ["main"]);
    assert_eq!(list["diagrams"][0]["name"], "기본");

    let second = call(
        &engine,
        "create_c4_diagram",
        json!({"project": "demo", "name": "  결제 흐름 "}),
    )
    .unwrap();
    assert_eq!(second["diagram"]["id"], "arch-2");
    assert_eq!(second["diagram"]["name"], "결제 흐름");
    let third = call(
        &engine,
        "create_c4_diagram",
        json!({"project": "demo", "name": "배포"}),
    )
    .unwrap();
    assert_eq!(third["diagram"]["id"], "arch-3");

    let list = call(&engine, "list_c4_diagrams", json!({"project": "demo"})).unwrap();
    assert_eq!(ids(&list), ["main", "arch-2", "arch-3"]);
    assert_eq!(list["diagrams"][1]["elements"], 0);
    assert!(
        vault
            .join(".aisurada/c4/demo/diagrams/arch-2/diagram.yaml")
            .is_file()
    );
    assert!(
        !vault.join(".aisurada/c4/demo/model.yaml").exists(),
        "main의 파일은 만들지 않는다"
    );
}

#[test]
fn each_diagram_keeps_its_own_model_and_token() {
    let vault = vault("diagrams-own");
    let engine = Engine::new(&vault);
    let main = get(&engine, "demo");
    apply(
        &engine,
        "demo",
        main["precondition"].as_str().unwrap(),
        json!([{"op":"put_element","id":"app","kind":"system","name":"메인 앱"}]),
    )
    .unwrap();
    call(
        &engine,
        "create_c4_diagram",
        json!({"project": "demo", "name": "다른 것"}),
    )
    .unwrap();

    let other = call(
        &engine,
        "get_c4_model",
        json!({"project": "demo", "diagram": "arch-2"}),
    )
    .unwrap();
    assert_eq!(other["elements"], json!([]), "다른 아키텍처는 비어 있다");
    assert_eq!(other["diagram"]["id"], "arch-2");
    assert_eq!(other["diagram"]["name"], "다른 것");

    let written = engine
        .call(
            "apply_c4_changes",
            json!({
                "project": "demo", "diagram": "arch-2",
                "f1": other["precondition"],
                "operations": [{"op":"put_element","id":"app","kind":"system","name":"둘째 앱"}],
            }),
        )
        .unwrap();
    assert_eq!(written["diagram"]["id"], "arch-2");
    assert_eq!(written["elements"][0]["name"], "둘째 앱");
    assert_eq!(get(&engine, "demo")["elements"][0]["name"], "메인 앱");
    let file = vault.join(".aisurada/c4/demo/diagrams/arch-2/model.yaml");
    assert!(std::fs::read_to_string(file).unwrap().contains("둘째 앱"));

    // 다른 아키텍처의 토큰은 이쪽에서 통하지 않는다
    let stale = engine.call(
        "apply_c4_changes",
        json!({
            "project": "demo", "diagram": "arch-2",
            "f1": main["precondition"],
            "operations": [{"op":"delete_element","id":"app"}],
        }),
    );
    assert_eq!(stale.unwrap_err().kind, ErrorKind::PreconditionFailed);
}

#[test]
fn renaming_changes_only_the_name_and_not_the_token() {
    let vault = vault("diagrams-rename");
    let engine = Engine::new(&vault);
    call(
        &engine,
        "create_c4_diagram",
        json!({"project": "demo", "name": "첫째"}),
    )
    .unwrap();
    call(
        &engine,
        "create_c4_diagram",
        json!({"project": "demo", "name": "둘째"}),
    )
    .unwrap();
    let before = call(
        &engine,
        "get_c4_model",
        json!({"project": "demo", "diagram": "arch-2"}),
    )
    .unwrap();

    let renamed = call(
        &engine,
        "rename_c4_diagram",
        json!({"project": "demo", "diagram": "arch-2", "name": "셋째"}),
    )
    .unwrap();
    assert_eq!(renamed["diagram"]["id"], "arch-2");
    assert_eq!(renamed["diagram"]["name"], "셋째");
    let after = call(
        &engine,
        "get_c4_model",
        json!({"project": "demo", "diagram": "arch-2"}),
    )
    .unwrap();
    assert_eq!(after["precondition"], before["precondition"]);
    assert_eq!(after["diagram"]["name"], "셋째");

    // main의 이름도 바꿀 수 있고, 같은 이름은 거절한다
    call(
        &engine,
        "rename_c4_diagram",
        json!({"project": "demo", "diagram": "main", "name": "개요"}),
    )
    .unwrap();
    let list = call(&engine, "list_c4_diagrams", json!({"project": "demo"})).unwrap();
    assert_eq!(list["diagrams"][0]["name"], "개요");
    let taken = call(
        &engine,
        "rename_c4_diagram",
        json!({"project": "demo", "diagram": "arch-3", "name": "개요"}),
    );
    assert_eq!(taken.unwrap_err().kind, ErrorKind::Conflict);
    let same = call(
        &engine,
        "create_c4_diagram",
        json!({"project": "demo", "name": "셋째"}),
    );
    assert_eq!(same.unwrap_err().kind, ErrorKind::Conflict);
    // 자기 이름 그대로는 된다
    call(
        &engine,
        "rename_c4_diagram",
        json!({"project": "demo", "diagram": "arch-3", "name": "둘째"}),
    )
    .unwrap();
}

#[test]
fn deleting_removes_the_folder_and_main_goes_back_to_empty() {
    let vault = vault("diagrams-delete");
    let engine = Engine::new(&vault);
    let main = get(&engine, "demo");
    apply(
        &engine,
        "demo",
        main["precondition"].as_str().unwrap(),
        json!([{"op":"put_element","id":"app","kind":"system","name":"앱"}]),
    )
    .unwrap();
    call(
        &engine,
        "create_c4_diagram",
        json!({"project": "demo", "name": "둘째"}),
    )
    .unwrap();

    let gone = call(
        &engine,
        "delete_c4_diagram",
        json!({"project": "demo", "diagram": "arch-2"}),
    )
    .unwrap();
    assert_eq!(gone["deleted"], "arch-2");
    assert!(!vault.join(".aisurada/c4/demo/diagrams/arch-2").exists());
    let list = call(&engine, "list_c4_diagrams", json!({"project": "demo"})).unwrap();
    assert_eq!(ids(&list), ["main"]);
    // 지운 뒤 번호는 가장 큰 번호 다음부터 이어진다(비어 있으면 다시 arch-2)
    let again = call(
        &engine,
        "create_c4_diagram",
        json!({"project": "demo", "name": "새것"}),
    )
    .unwrap();
    assert_eq!(again["diagram"]["id"], "arch-2");

    call(
        &engine,
        "delete_c4_diagram",
        json!({"project": "demo", "diagram": "main"}),
    )
    .unwrap();
    assert_eq!(get(&engine, "demo")["elements"], json!([]));
    assert!(!vault.join(".aisurada/c4/demo/model.yaml").exists());
    let list = call(&engine, "list_c4_diagrams", json!({"project": "demo"})).unwrap();
    assert_eq!(
        ids(&list),
        ["main", "arch-2"],
        "main을 지워도 다른 아키텍처는 남는다"
    );
    assert_eq!(list["diagrams"][0]["name"], "기본");
}

#[test]
fn unknown_unsafe_and_missing_diagram_arguments_are_refused() {
    let vault = vault("diagrams-refuse");
    let engine = Engine::new(&vault);
    for bad in ["arch-9", "../x", "a/b", "A"] {
        let got = call(
            &engine,
            "get_c4_model",
            json!({"project": "demo", "diagram": bad}),
        );
        assert!(got.is_err(), "{bad}");
    }
    assert_eq!(
        call(
            &engine,
            "get_c4_model",
            json!({"project": "demo", "diagram": "arch-9"})
        )
        .unwrap_err()
        .kind,
        ErrorKind::NotFound
    );
    // 지우기와 이름 바꾸기는 diagram을 꼭 적어야 한다(없다고 main으로 보지 않는다)
    assert!(call(&engine, "delete_c4_diagram", json!({"project": "demo"})).is_err());
    assert!(
        call(
            &engine,
            "rename_c4_diagram",
            json!({"project": "demo", "name": "x"})
        )
        .is_err()
    );
    assert!(
        call(
            &engine,
            "create_c4_diagram",
            json!({"project": "demo", "name": "  "})
        )
        .is_err()
    );
    assert!(
        call(
            &engine,
            "create_c4_diagram",
            json!({"project": "nope", "name": "x"})
        )
        .is_err()
    );
    assert!(
        !vault.join(".aisurada/c4").exists(),
        "거절된 호출은 아무것도 만들지 않는다"
    );
}
