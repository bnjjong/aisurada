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

fn history(engine: &Engine, diagram: &str) -> Value {
    call(
        engine,
        "list_c4_history",
        json!({"project": "demo", "diagram": diagram}),
    )
    .unwrap()
}

fn put(engine: &Engine, diagram: &str, id: &str, name: &str) -> Value {
    let read = call(
        engine,
        "get_c4_model",
        json!({"project": "demo", "diagram": diagram}),
    )
    .unwrap();
    engine
        .call(
            "apply_c4_changes",
            json!({
                "project": "demo", "diagram": diagram, "f1": read["precondition"],
                "operations": [{"op":"put_element","id":id,"kind":"system","name":name}],
            }),
        )
        .unwrap()
}

fn seqs_of(list: &Value) -> Vec<u64> {
    list["snapshots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["seq"].as_u64().unwrap())
        .collect()
}

#[test]
fn every_model_change_adds_a_point_and_position_only_changes_merge_into_the_last_one() {
    let vault = vault("history-points");
    let engine = Engine::new(&vault);
    assert_eq!(history(&engine, "main")["snapshots"], json!([]));
    assert_eq!(
        history(&engine, "main")["latestIsCurrent"],
        true,
        "빈 모델은 점이 없어도 현재와 같다"
    );

    put(&engine, "main", "a", "에이");
    put(&engine, "main", "b", "비");
    let list = history(&engine, "main");
    assert_eq!(seqs_of(&list), [1, 2]);
    assert_eq!(list["snapshots"][0]["summary"], "요소 추가: 에이");
    assert_eq!(list["snapshots"][1]["summary"], "요소 추가: 비");
    assert_eq!(list["snapshots"][1]["by"], "app");
    assert_eq!(list["snapshots"][1]["elements"], 2);
    assert_eq!(list["latestIsCurrent"], true);
    assert!(
        vault
            .join(".aisurada/c4/demo/history/000002.json")
            .is_file()
    );

    // 위치만 바꾸면 새 점이 없고 마지막 점의 위치만 갱신된다
    let read = get(&engine, "demo");
    apply(
        &engine,
        "demo",
        read["precondition"].as_str().unwrap(),
        json!([{"op":"set_positions","view":"context","positions":{"a":{"x":5,"y":6}}}]),
    )
    .unwrap();
    let list = history(&engine, "main");
    assert_eq!(
        seqs_of(&list),
        [1, 2],
        "위치만 바뀐 쓰기는 점을 만들지 않는다"
    );
    assert_eq!(list["latestIsCurrent"], true);
    let last = call(&engine, "get_c4_model", json!({"project": "demo", "at": 2})).unwrap();
    assert_eq!(
        last["views"]["context"]["a"],
        json!({"x": 5, "y": 6}),
        "마지막 점에 합쳐졌다"
    );
    let first = call(
        &engine,
        "get_c4_model",
        json!({"project": "demo", "at": "1"}),
    )
    .unwrap();
    assert_eq!(
        first["elements"].as_array().unwrap().len(),
        1,
        "첫 점은 그때의 모양이다(명령줄 글 번호도 된다)"
    );
    assert_eq!(first["readOnly"], true);
    assert!(
        first.get("precondition").is_none(),
        "과거에는 쓸 수 없으니 확인 값이 없다"
    );
    assert_eq!(first["snapshot"]["seq"], 1);

    // 바뀐 것이 없는 쓰기는 점을 만들지 않는다
    let read = get(&engine, "demo");
    apply(
        &engine,
        "demo",
        read["precondition"].as_str().unwrap(),
        json!([{"op":"put_element","id":"a","name":"에이"}]),
    )
    .unwrap();
    assert_eq!(seqs_of(&history(&engine, "main")), [1, 2]);
}

#[test]
fn a_hand_edit_and_a_pre_existing_model_get_their_own_points() {
    let vault = vault("history-external");
    // 이력이 생기기 전부터 있던 모델
    write(
        &vault,
        ".aisurada/c4/demo/model.yaml",
        "version: 1\nelements:\n- id: old\n  kind: system\n  name: 옛것\nrelationships: []\n",
    );
    let engine = Engine::new(&vault);
    assert_eq!(
        history(&engine, "main")["latestIsCurrent"],
        false,
        "점이 없는데 모델이 있으면 현재와 다르다"
    );
    put(&engine, "main", "b", "비");
    let list = history(&engine, "main");
    assert_eq!(seqs_of(&list), [1, 2]);
    assert_eq!(list["snapshots"][0]["kind"], "baseline");
    assert_eq!(list["snapshots"][0]["summary"], "시작 상태");
    assert_eq!(
        list["snapshots"][0]["elements"], 1,
        "시작 상태는 쓰기 전의 모양이다"
    );
    assert_eq!(list["snapshots"][1]["kind"], "change");

    // 엔진을 거치지 않은 손 편집은 다음 쓰기 때 먼저 「외부 변경」 점이 된다
    write(
        &vault,
        ".aisurada/c4/demo/model.yaml",
        "version: 1\nelements:\n- id: old\n  kind: system\n  name: 옛것\n- id: b\n  kind: system\n  name: 비\n- id: hand\n  kind: system\n  name: 손으로\nrelationships: []\n",
    );
    assert_eq!(history(&engine, "main")["latestIsCurrent"], false);
    put(&engine, "main", "c", "씨");
    let list = history(&engine, "main");
    assert_eq!(seqs_of(&list), [1, 2, 3, 4]);
    assert_eq!(list["snapshots"][2]["kind"], "external");
    assert_eq!(
        list["snapshots"][2]["summary"],
        "외부 변경 · 요소 추가: 손으로"
    );
    assert_eq!(list["snapshots"][3]["summary"], "요소 추가: 씨");
    assert_eq!(list["latestIsCurrent"], true);
}

#[test]
fn the_caller_is_recorded_and_the_oldest_points_go_past_the_cap() {
    use ais_core::kernel::actor::CallContext;
    let vault = vault("history-cap");
    let engine = Engine::new(&vault);
    let read = get(&engine, "demo");
    engine
        .call_as(
            &CallContext::ai(Some("s1".into()), None),
            "apply_c4_changes",
            json!({"project": "demo", "f1": read["precondition"],
                   "operations": [{"op":"put_element","id":"a","kind":"system","name":"AI가 만듦"}]}),
        )
        .unwrap();
    assert_eq!(history(&engine, "main")["snapshots"][0]["by"], "ai");

    for n in 0..204 {
        put(&engine, "main", "a", &format!("이름 {n}"));
    }
    let list = history(&engine, "main");
    let seqs = seqs_of(&list);
    assert_eq!(seqs.len(), 200, "상한을 넘으면 오래된 것부터 지운다");
    assert_eq!(
        (seqs[0], *seqs.last().unwrap()),
        (6, 205),
        "번호는 다시 쓰지 않는다"
    );
    assert!(!vault.join(".aisurada/c4/demo/history/000005.json").exists());
    let gone = call(&engine, "get_c4_model", json!({"project": "demo", "at": 5}));
    assert_eq!(gone.unwrap_err().kind, ErrorKind::NotFound);
}

#[test]
fn each_diagram_keeps_its_own_history_and_deleting_takes_it_along() {
    let vault = vault("history-diagrams");
    let engine = Engine::new(&vault);
    call(
        &engine,
        "create_c4_diagram",
        json!({"project": "demo", "name": "둘째"}),
    )
    .unwrap();
    put(&engine, "main", "a", "메인");
    put(&engine, "arch-2", "x", "둘째 것");
    put(&engine, "arch-2", "y", "둘째 것 2");
    assert_eq!(seqs_of(&history(&engine, "main")), [1]);
    assert_eq!(seqs_of(&history(&engine, "arch-2")), [1, 2]);
    let second = call(
        &engine,
        "get_c4_model",
        json!({"project": "demo", "diagram": "arch-2", "at": 1}),
    )
    .unwrap();
    assert_eq!(second["elements"][0]["name"], "둘째 것");
    assert_eq!(second["diagram"]["id"], "arch-2");

    // 이름을 바꿔도 이력은 그대로
    call(
        &engine,
        "rename_c4_diagram",
        json!({"project": "demo", "diagram": "arch-2", "name": "바뀜"}),
    )
    .unwrap();
    assert_eq!(seqs_of(&history(&engine, "arch-2")), [1, 2]);

    call(
        &engine,
        "delete_c4_diagram",
        json!({"project": "demo", "diagram": "arch-2"}),
    )
    .unwrap();
    assert!(!vault.join(".aisurada/c4/demo/diagrams/arch-2").exists());
    call(
        &engine,
        "delete_c4_diagram",
        json!({"project": "demo", "diagram": "main"}),
    )
    .unwrap();
    assert!(
        !vault.join(".aisurada/c4/demo/history").exists(),
        "main을 지우면 이력도 지워진다"
    );
    assert_eq!(history(&engine, "main")["snapshots"], json!([]));
}

#[test]
fn bad_point_numbers_and_unknown_diagrams_are_refused() {
    let vault = vault("history-refuse");
    let engine = Engine::new(&vault);
    put(&engine, "main", "a", "에이");
    for bad in [
        json!(0),
        json!(-1),
        json!("x"),
        json!(1.5),
        json!(null),
        json!([1]),
    ] {
        let got = call(
            &engine,
            "get_c4_model",
            json!({"project": "demo", "at": bad}),
        );
        // null 은 "안 줌"과 같아 현재 모델을 낸다
        if bad.is_null() {
            assert!(got.is_ok());
        } else {
            assert_eq!(got.unwrap_err().kind, ErrorKind::InvalidArgument, "{bad}");
        }
    }
    assert_eq!(
        call(&engine, "get_c4_model", json!({"project": "demo", "at": 9}))
            .unwrap_err()
            .kind,
        ErrorKind::NotFound
    );
    assert!(
        call(
            &engine,
            "list_c4_history",
            json!({"project": "demo", "diagram": "arch-9"})
        )
        .is_err()
    );
    assert!(call(&engine, "list_c4_history", json!({"project": "nope"})).is_err());
    assert!(call(&engine, "list_c4_history", json!({})).is_err());
}
