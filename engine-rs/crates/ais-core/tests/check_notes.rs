//! `check_notes`(답 없는 코멘트 점검). 지운 Java `Chapter30Test`(30장)의 시나리오를 Rust로 옮긴 것이다.
//! 골든 테이프에는 합성 vault에 던진 호출 5건뿐이라(경계 날짜, 편집 도장, 같은 순간, 못 읽는 코멘트 파일 따위가 없다) 따로 둔다.
//!
//! 이 파일은 시험이 하나뿐이고 환경 변수를 한 번만 정한다(서울 시간대, 시험용 시계, 잠금 폴더, 자동 커밋 끔).

use std::path::{Path, PathBuf};

use ais_core::engine::Engine;
use ais_core::error::ErrorKind;
use serde_json::{Value, json};

fn put(vault: &Path, relative: &str, text: &str) {
    let file = vault.join(relative);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

fn note(vault: &Path, path: &str, id: &str, created: &str, extra: &str, body: &str) {
    let title = path.rsplit('/').next().unwrap();
    let head = format!(
        "id: {id}\nagenda: demo\ntype: build\ntitle: {title}\ncreated: '{created}'\n{extra}"
    );
    let text = if path.ends_with(".html") {
        format!("<!--aisurada\n{head}-->\n{body}")
    } else {
        format!("---\n{head}---\n{body}")
    };
    put(vault, path, &text);
}

fn root(vault: &Path, name: &str, n: u32, created: &str, extra: &str) {
    note(
        vault,
        &format!("notes/demo/{name}/note.md"),
        &format!("ndfixture-{n:08x}"),
        created,
        extra,
        "",
    );
}

/// 코멘트 파일을 쓴다. 코멘트마다 만든 시각만 다르고 고친 시각은 아주 먼 미래다.
fn comments(vault: &Path, path: &str, at: &[&str]) {
    let rows: Vec<Value> = at
        .iter()
        .enumerate()
        .map(|(i, created)| {
            json!({"id": format!("cmfixture-{:08x}", i + 1), "createdAt": created, "body": "말", "editedAt": "2099-01-01T00:00Z"})
        })
        .collect();
    put(
        vault,
        path,
        &(serde_json::to_string_pretty(&rows).unwrap() + "\n"),
    );
}

fn check(vault: &Path, date: &str) -> Value {
    Engine::new(vault)
        .call("check_notes", json!({"date": date}))
        .unwrap_or_else(|e| panic!("check_notes {date}: {e:?}"))
}

fn items(result: &Value) -> Vec<Value> {
    result["items"].as_array().unwrap().clone()
}

fn ids(result: &Value) -> Vec<String> {
    items(result)
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_string())
        .collect()
}

fn item(result: &Value, id: &str) -> Value {
    items(result)
        .into_iter()
        .find(|i| i["id"] == id)
        .unwrap_or_else(|| panic!("{id}가 없다: {result}"))
}

#[test]
fn check_notes_finds_unanswered_comments_like_the_java_engine() {
    let base: PathBuf =
        std::env::temp_dir().join(format!("ais-check-notes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    // SAFETY: 이 파일의 유일한 시험이고, 다른 스레드가 환경 변수를 읽기 전에 정한다.
    unsafe {
        std::env::set_var("TZ", "Asia/Seoul");
        std::env::set_var("AISURADA_TEST_NOW", "1792559107123"); // 2026-10-21T05:05:07.123Z
        std::env::set_var("AISURADA_LOCK_DIR", base.join("locks"));
        std::env::set_var("AISURADA_VAULT_AUTOCOMMIT_ENABLED", "false");
    }
    let v = base.join("vault");
    put(&v, ".aisurada/workspace.json", "{}\n");
    put(
        &v,
        ".aisurada/agendas/demo.md",
        "---\nid: agfixture-00000001\nslug: demo\ntitle: 데모\n---\n",
    );
    root(
        &v,
        "root",
        1,
        "2026-06-16T10:00:00+09:00",
        "title: 픽스처 노트\n",
    );

    // 코멘트 파일이 없으면 조용하다
    assert_eq!(check(&v, "2026-06-20")["count"], 0, "코멘트 파일 없음");

    comments(
        &v,
        "notes/demo/root/comments.json",
        &[
            "2026-06-16T10:30:00Z",
            "2026-06-16T10:31:00Z",
            "2026-06-16T10:32:00.000Z",
        ],
    );
    let response = check(&v, "2026-06-20");
    let keys: Vec<&str> = response
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, ["count", "date", "items", "section", "windowDays"]);
    assert_eq!(response["count"], 1, "답 없는 노트 하나");
    let row = &items(&response)[0];
    assert_eq!(
        row["lastCommentAt"], "2026-06-16T10:32Z",
        "가장 짧은 오프셋 ISO"
    );
    assert_eq!(
        row["lastChangeAt"], "2026-06-16T10:00+09:00",
        "만든 시각은 자기 오프셋을 지킨다"
    );
    assert_eq!(
        (row["comments"].as_u64(), row["stamped"].as_bool()),
        (Some(3), Some(false))
    );
    assert_eq!(
        response["section"],
        "- 답 없는 코멘트: \"픽스처 노트\" (데모), 2026-06-16. 코멘트 3개, 마지막 2026-06-16 10:32. 그 뒤로 노트가 바뀐 기록이 없다(편집 도장 없음, 작성 2026-06-16 10:00).",
        "설명서 모양 그대로의 절"
    );
    // 14일 창: 아래쪽 경계는 포함하고 위쪽은 막지 않는다
    assert_eq!(check(&v, "2026-06-30")["count"], 1, "14일 아래 경계는 포함");
    assert_eq!(check(&v, "2026-07-01")["count"], 0, "창 밖");
    assert_eq!(check(&v, "2026-05-01")["count"], 1, "위쪽 경계 없음");

    // 같은 순간이면 벽시계가 큰 쪽, 읽을 수 없는 시각도 개수에는 든다
    comments(
        &v,
        "notes/demo/root/comments.json",
        &[
            "2026-06-16T10:32Z",
            "2026-06-16T19:32+09:00",
            "2026-06-16T20:00",
            " 2026-06-16T20:00Z",
        ],
    );
    let row = items(&check(&v, "2026-06-20"))[0].clone();
    assert_eq!(
        row["lastCommentAt"], "2026-06-16T19:32+09:00",
        "같은 순간이면 큰 벽시계"
    );
    assert_eq!(row["comments"], 4, "못 읽는 시각도 센다");

    // 편집 도장이 있으면 그것이 마지막 변경이고, 같은 분은 답한 것으로 본다
    root(
        &v,
        "root",
        1,
        "2026-06-16T10:00:00+09:00",
        "title: 픽스처 노트\neditedBy: app\neditedAt: '2026-06-16T19:32:00+09:00'\n",
    );
    comments(
        &v,
        "notes/demo/root/comments.json",
        &["2026-06-16T19:32:59.999+09:00"],
    );
    assert_eq!(check(&v, "2026-06-20")["count"], 0, "같은 분은 답한 것");
    comments(
        &v,
        "notes/demo/root/comments.json",
        &["2026-06-16T19:33:00+09:00"],
    );
    assert_eq!(
        items(&check(&v, "2026-06-20"))[0]["stamped"],
        true,
        "도장을 쓴다"
    );

    // 도장이 없는 노트는 가장 이른 도장보다 먼저 만든 것을 뺀다. 같은 순간이면 넣는다
    root(&v, "old", 2, "2026-06-15T10:00:00+09:00", "");
    comments(&v, "notes/demo/old/comments.json", &["2026-06-25T10:00Z"]);
    assert!(
        !ids(&check(&v, "2026-06-20")).contains(&"ndfixture-00000002".to_string()),
        "가장 이른 도장 전에 만든 노트는 뺀다"
    );
    root(&v, "equal", 3, "2026-06-16T19:32:00+09:00", "");
    comments(&v, "notes/demo/equal/comments.json", &["2026-06-17T10:00Z"]);
    assert!(
        ids(&check(&v, "2026-06-20")).contains(&"ndfixture-00000003".to_string()),
        "가장 이른 순간과 같으면 넣는다"
    );

    // 기록 층 노트의 도장은 가장 이른 도장으로 안 쓴다
    root(
        &v,
        "record",
        4,
        "2026-01-01",
        "tier: record\neditedBy: app\neditedAt: '2026-01-02T10:00Z'\n",
    );
    comments(
        &v,
        "notes/demo/record/comments.json",
        &["2026-06-20T10:00Z"],
    );
    assert!(
        !ids(&check(&v, "2026-06-20")).contains(&"ndfixture-00000002".to_string()),
        "기록 층 도장은 안 쓴다"
    );

    // JSON 노트의 도장은 무시하고 만든 시각을 쓴다. 코멘트 파일을 못 읽으면 건너뛴다
    note(
        &v,
        "notes/demo/json/note.json",
        "ndfixture-00000005",
        "2026-06-18",
        "editedBy: app\neditedAt: '2026-06-25T10:00Z'\n",
        "{}",
    );
    comments(&v, "notes/demo/json/comments.json", &["2026-06-20T10:00Z"]);
    let response = check(&v, "2026-06-20");
    assert!(
        ids(&response).contains(&"ndfixture-00000005".to_string()),
        "JSON 도장은 무시"
    );
    assert_eq!(
        item(&response, "ndfixture-00000005")["stamped"],
        false,
        "JSON은 만든 시각"
    );
    put(&v, "notes/demo/json/comments.json", "bad JSON");
    assert!(
        !ids(&check(&v, "2026-06-20")).contains(&"ndfixture-00000005".to_string()),
        "못 읽는 코멘트는 건너뛴다"
    );

    // 단일 HTML 노트의 도장도 가장 이른 도장에 든다
    note(
        &v,
        "notes/demo/earlier.html",
        "ndfixture-00000006",
        "2026-01-01",
        "editedBy: robot\neditedAt: '2026-06-14T10:00Z'\n",
        "<p>old</p>",
    );
    assert!(
        ids(&check(&v, "2026-06-20")).contains(&"ndfixture-00000002".to_string()),
        "단일 HTML 노트도 가장 이른 도장에 든다"
    );

    // 고칠 수 없는 도장은 없는 것이고, 나노초와 초 오프셋은 보존한다
    root(
        &v,
        "invalid",
        7,
        "2026-06-18",
        "editedBy: app\neditedAt: 'bad'\n",
    );
    comments(
        &v,
        "notes/demo/invalid/comments.json",
        &["2026-06-19T10:00:00.123456789+09:00:30"],
    );
    let row = item(&check(&v, "2026-06-20"), "ndfixture-00000007");
    assert_eq!(row["stamped"], false, "잘못된 도장은 없는 것");
    assert_eq!(
        row["lastCommentAt"], "2026-06-19T10:00:00.123456789+09:00:30",
        "나노초와 초 오프셋 보존"
    );

    // 날짜 인자 검사
    for bad in ["2026-02-30", "bad"] {
        let error = Engine::new(&v)
            .call("check_notes", json!({"date": bad}))
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidArgument, "{bad}: {error:?}");
    }
    // 워크스페이스 표식이 날짜 검사보다 앞선다
    std::fs::remove_file(v.join(".aisurada/workspace.json")).unwrap();
    let error = Engine::new(&v)
        .call("check_notes", json!({"date": "bad"}))
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::InvalidArgument);
    assert!(error.message.contains("워크스페이스"), "{}", error.message);

    let _ = std::fs::remove_dir_all(&base);
}
