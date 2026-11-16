//! 합성 vault(`tools/compat/make-fixture.sh`)로 읽기 명령의 핵심 동작을 확인합니다. Java 엔진 없이 돕니다.
//! 같은 vault를 Java 엔진과 맞대는 시험은 `examples/compat_check.rs`입니다.

use std::path::{Path, PathBuf};
use std::process::Command;

use ais_core::engine::Engine;
use ais_core::error::ErrorKind;
use serde_json::{Value, json};

fn fixture() -> PathBuf {
    make_fixture(false)
}

/// 검색 시험용 노트(`add-search-notes.sh`)까지 더한 합성 vault
fn fixture_with_search_notes() -> PathBuf {
    make_fixture(true)
}

/// git 저장소와 노트의 판 이력(`add-git-history.sh`)까지 만든 합성 vault
fn fixture_with_git_history() -> PathBuf {
    let dir = make_fixture(false);
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/compat/add-git-history.sh");
    let output = Command::new("bash")
        .arg(script)
        .arg(&dir)
        .output()
        .expect("bash를 못 돌렸다");
    assert!(
        output.status.success(),
        "git 이력을 못 만들었다: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    dir
}

fn make_fixture(with_search_notes: bool) -> PathBuf {
    // 시험이 병렬로 돌아도 폴더가 겹치지 않게 번호를 붙인다
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("ais-core-fixture-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/compat/make-fixture.sh");
    let status = Command::new("bash")
        .arg(script)
        .arg(&dir)
        .output()
        .expect("bash를 못 돌렸다");
    assert!(
        status.status.success(),
        "합성 vault를 못 만들었다: {}",
        String::from_utf8_lossy(&status.stderr)
    );
    if with_search_notes {
        let script =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/compat/add-search-notes.sh");
        let status = Command::new("bash")
            .arg(script)
            .arg(&dir)
            .output()
            .expect("bash를 못 돌렸다");
        assert!(
            status.status.success(),
            "검색용 노트를 못 더했다: {}",
            String::from_utf8_lossy(&status.stderr)
        );
    }
    dir
}

fn ids(page: &Value) -> Vec<String> {
    page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect()
}

/// 새 입구(`call_as`)는 호출 맥락을 인자 밖에서만 받는다. 인자의 `caller` 칸으로 호출자를 바꿀 수 없다.
#[test]
fn the_context_decides_who_is_calling_and_an_argument_field_cannot() {
    use ais_core::kernel::actor::CallContext;
    let vault = fixture();
    let engine = Engine::new(&vault);
    let record = "ndfx0008-00000008"; // 기록 층 노트: 앱은 읽고 AI는 못 읽는다
    // 맥락이 AI이면 인자가 앱이라 해도 거절된다
    let refused = engine
        .call_as(
            &CallContext::ai(None, None),
            "get_doc",
            json!({"id": record, "caller": "app"}),
        )
        .unwrap_err();
    assert_eq!(refused.kind, ErrorKind::RecordTier);
    // 맥락이 앱이면 인자가 AI라 해도 읽힌다
    let read = engine
        .call_as(
            &CallContext::app(),
            "get_doc",
            json!({"id": record, "caller": "ai"}),
        )
        .unwrap();
    assert_eq!(read["id"], record);
    // 과도기 입구는 인자의 칸을 맥락으로 읽는다(옛 호출과 같다)
    let legacy = engine
        .call("get_doc", json!({"id": record, "caller": "ai"}))
        .unwrap_err();
    assert_eq!(legacy.kind, ErrorKind::RecordTier);
}

#[test]
fn reads_the_synthetic_vault() {
    let vault = fixture();
    let engine = Engine::new(&vault);

    // 목록: 21개 노트를 만든 시각 차례로. 건수 제한이 없으면 잘리지 않는다
    let docs = engine.call("list_docs", json!({"limit": 0})).unwrap();
    assert_eq!(
        (
            docs["total"].as_u64(),
            docs["returned"].as_u64(),
            docs["truncated"].as_bool()
        ),
        (Some(21), Some(21), Some(false))
    );
    let created: Vec<&str> = docs["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["created"].as_str().unwrap())
        .collect();
    assert_eq!(
        created
            .iter()
            .filter(|c| c.starts_with("0001-01-01"))
            .count(),
        1,
        "연도 0000은 0001로 바뀐다"
    );

    // 건수를 안 정하면 앱은 100건, 정하면 그만큼이고 잘렸음을 알린다
    let limited = engine.call("list_docs", json!({"limit": 5})).unwrap();
    assert_eq!(
        (
            limited["returned"].as_u64(),
            limited["total"].as_u64(),
            limited["truncated"].as_bool()
        ),
        (Some(5), Some(21), Some(true))
    );
    assert_eq!(engine.call("list_docs", json!({})).unwrap()["returned"], 21);

    // 호출자: AI에게는 기록 층 노트(2건)가 안 보이고 tier 표시도 없다
    let ai = engine.call("list_docs", json!({"caller": "ai"})).unwrap();
    assert_eq!(ai["total"], 19);
    assert!(
        ai["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r.get("tier").is_none())
    );
    let app = engine.call("list_docs", json!({})).unwrap();
    assert_eq!(
        app["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["tier"] == "record")
            .count(),
        2
    );

    // JSON 노트와 확장자 폴더 노트는 본문·확인 값을 숨긴다. 평가 점수는 0~100으로 맞춘다
    let with_bodies = engine
        .call("list_docs", json!({"limit": 0, "bodies": true}))
        .unwrap();
    let by_id = |id: &str| {
        with_bodies["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id)
            .unwrap()
            .clone()
    };
    let json_note = by_id("ndfx0006-00000006");
    assert_eq!(json_note["format"], "json");
    assert!(
        json_note.get("precondition").is_none()
            && json_note["body"] == ""
            && json_note.get("sessionId").is_none()
    );
    assert!(by_id("ndfx0007-00000007").get("precondition").is_none());
    assert_eq!(by_id("ndfx000d-0000000d")["evaluation"]["score"], 100);
    assert_eq!(
        by_id("ndfx000d-0000000d")["tags"].as_array().unwrap().len(),
        50
    );
    assert_eq!(by_id("ndfx0001-00000001")["sessionAgent"], "gemini");
    // HTML 첨부의 글은 스크립트와 스타일을 빼고 이름 차례로 잇는다
    assert_eq!(
        by_id("ndfx0002-00000002")["attachText"],
        "첫째 문서 끝 둘째 & 문서"
    );
    // 응답에는 date·time·agendaTitle이 없다
    assert!(
        with_bodies["items"][0].get("date").is_none()
            && with_bodies["items"][0].get("agendaTitle").is_none()
    );

    // 조건: 아젠다 이름(대소문자 무시)으로도 찾고, 모르는 값은 오류 종류로 가른다
    let demo = engine
        .call("list_docs", json!({"agenda": "데모 아젠다", "limit": 0}))
        .unwrap();
    assert_eq!(ids(&demo).len(), 3);
    let unknown = engine
        .call("list_docs", json!({"agenda": "없는-아젠다"}))
        .unwrap_err();
    assert_eq!(
        (unknown.kind, serde_json::to_value(unknown.kind).unwrap()),
        (ErrorKind::NotFound, json!("not_found"))
    );
    let bad_type = engine
        .call("list_docs", json!({"type": "zzz"}))
        .unwrap_err();
    assert_eq!(
        serde_json::to_value(bad_type.kind).unwrap(),
        json!("invalid_argument")
    );
    assert!(
        engine
            .call(
                "list_docs",
                json!({"from": "2026-12-01", "to": "2026-01-01"})
            )
            .is_err(),
        "뒤집힌 날짜 구간"
    );

    // 아젠다: slug 차례, 안전하지 않은 것은 빠진다
    let agendas = engine.call("list_agendas", Value::Null).unwrap();
    let slugs: Vec<&str> = agendas["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["slug"].as_str().unwrap())
        .collect();
    assert_eq!(slugs, ["demo", "empty-title", "work", "기록"]);
    assert_eq!(agendas["items"][0]["projectRepo"], "web");

    // 프로젝트: 같은 slug를 다른 id가 쓰면 뒤의 것을 뺀다. 저장소는 규칙에 맞는 것만 남는다
    let projects = engine.call("list_projects", Value::Null).unwrap();
    assert_eq!(projects["total"], 2);
    let repo_names: Vec<&str> = projects["items"][0]["repos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["name"].as_str().unwrap())
        .collect();
    assert_eq!(repo_names, ["web", "api"]);

    // 템플릿: 문서 먼저, id 차례. 모르는 갈래는 오류
    let templates = engine.call("list_templates", json!({})).unwrap();
    let template_ids: Vec<&str> = templates["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        template_ids,
        [
            "data-json",
            "page",
            "plan-basic",
            "morning-1",
            "plain",
            "weekly"
        ]
    );
    assert_eq!(templates["items"][2]["builtin"], false);
    assert!(
        engine
            .call("list_templates", json!({"category": "zzz"}))
            .is_err()
    );

    // 휴지통: 버린 시각이 늦은 것부터, 같으면 목록에 적힌 차례. 못 세우는 항목은 빠진다
    let trash = engine.call("list_trash", Value::Null).unwrap();
    let trash_ids: Vec<&str> = trash["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["trashId"].as_str().unwrap())
        .collect();
    assert_eq!(
        trash_ids,
        [
            "trfx0002-000000b2",
            "trfx0004-000000b4",
            "trfx0003-000000b3",
            "trfx0001-000000b1"
        ]
    );

    // reindex: 건너뜀은 아젠다, 노트, 템플릿, 휴지통 차례로 이유와 함께 나온다
    let summary = engine.call("reindex", Value::Null).unwrap();
    assert_eq!(
        (summary["indexed"].as_u64(), summary["agendas"].as_u64()),
        (Some(21), Some(4))
    );
    let kinds: Vec<&str> = summary["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["kind"].as_str().unwrap())
        .collect();
    let first_of = |k: &str| kinds.iter().position(|x| *x == k).unwrap();
    assert!(
        first_of("agenda") < first_of("note")
            && first_of("note") < first_of("docTemplate")
            && first_of("docTemplate") < first_of("trash")
    );
    assert_eq!(summary["conflicts"].as_array().unwrap().len(), 2);

    // 이 엔진에 없는 명령은 unsupported로 알린다
    assert_eq!(
        serde_json::to_value(engine.call("no_such_command", json!({})).unwrap_err().kind).unwrap(),
        json!("unsupported")
    );
    assert!(
        Engine::supports("list_docs")
            && Engine::supports("get_doc")
            && !Engine::supports("no_such_command")
    );

    let _ = std::fs::remove_dir_all(&vault);
}

#[test]
fn opens_notes_sub_notes_comments_attachments_and_facts() {
    let vault = fixture();
    let engine = Engine::new(&vault);
    let parent = "ndfx0002-00000002";

    // get_doc: 본문과 코멘트가 붙고, 응답에 date·time·agendaTitle·deepLink가 없다
    let doc = engine.call("get_doc", json!({"id": parent})).unwrap();
    assert_eq!(
        (doc["comments"]["state"].as_str(), doc["folder"].as_bool()),
        (Some("loaded"), Some(true))
    );
    assert!(doc["body"].as_str().unwrap().contains("본문 ndfx0002"));
    assert!(doc.get("deepLink").is_none() && doc.get("date").is_none());
    // 파일 하나로 된 노트는 코멘트가 없다. 없는 노트와 모양이 틀린 id는 오류 종류로 가른다
    assert_eq!(
        engine
            .call("get_doc", json!({"id": "ndfx0004-00000004"}))
            .unwrap()["comments"]["state"],
        "absent"
    );
    let kinds = |id: &str| {
        serde_json::to_value(engine.call("get_doc", json!({"id": id})).unwrap_err().kind).unwrap()
    };
    assert_eq!(
        (kinds("ndzzzz-00000000"), kinds("없는-id")),
        (json!("not_found"), json!("invalid_argument"))
    );
    // AI에게는 기록 층 노트가 열리지 않는다
    let refused = engine
        .call(
            "get_doc",
            json!({"id": "ndfx0008-00000008", "caller": "ai"}),
        )
        .unwrap_err();
    assert_eq!(
        (refused.kind, serde_json::to_value(refused.kind).unwrap()),
        (ErrorKind::RecordTier, json!("forbidden"))
    );

    // 후속 노트: 부모 사슬로도, 후속 노트 id 하나로도 같은 결과. 바로 아래 노트만, 만든 시각 차례
    let by_chain = engine
        .call(
            "list_sub_notes",
            json!({"parent": format!("{parent}/snfx0002-00000012"), "bodies": true}),
        )
        .unwrap();
    let by_id = engine
        .call(
            "list_sub_notes",
            json!({"parent": "snfx0002-00000012", "bodies": true}),
        )
        .unwrap();
    assert_eq!(by_chain, by_id);
    let ids: Vec<&str> = by_id["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["snfx0003-00000013", "snfx0004-00000014"]);
    assert_eq!(
        (
            by_id["items"][0]["rootId"].as_str(),
            by_id["items"][0]["parentId"].as_str()
        ),
        (Some(parent), Some("snfx0002-00000012"))
    );
    let top = engine
        .call("list_sub_notes", json!({"parent": parent}))
        .unwrap();
    assert_eq!(top["total"], 2);
    assert!(
        top["items"][0].get("body").is_none(),
        "본문은 bodies일 때만"
    );
    // 잘못된 사슬: 부모가 아닌 노트 아래로 부르면 없는 후속 노트(404)다
    let wrong = engine
        .call(
            "list_sub_notes",
            json!({"parent": format!("{parent}/snfx0003-00000013")}),
        )
        .unwrap_err();
    assert_eq!(wrong.kind, ErrorKind::NotFound);
    assert!(
        engine
            .call(
                "get_sub_note",
                json!({"parent": parent, "id": "snfx0003-00000013"})
            )
            .is_err(),
        "손자는 메인 노트 바로 아래가 아니다"
    );
    let sub = engine
        .call("get_sub_note", json!({"id": "snfx0001-00000011"}))
        .unwrap();
    assert_eq!(
        (
            sub["comments"]["state"].as_str(),
            sub["comments"]["comments"][0]["id"].as_str()
        ),
        (Some("loaded"), Some("cmsub-00000001"))
    );

    // 코멘트: 규칙에 안 맞는 항목과 같은 id는 빠지고 시각·id 차례. JSON이 아닌 파일은 unreadable
    let comments = engine.call("list_comments", json!({"id": parent})).unwrap();
    let comment_ids: Vec<&str> = comments["comments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        comment_ids,
        ["cmabc-0a0b0c0d", "CMABC-FFFF", "cmabc-1a2b3c4d"]
    );
    assert_eq!(comments["comments"][2]["anchor"]["quote"], "가리킨 글");
    assert!(
        comments["comments"][0].get("anchor").is_none(),
        "빈 인용문은 앵커가 아니다"
    );
    assert_eq!(
        engine
            .call(
                "list_comments",
                json!({"id": parent, "subId": "snfx0003-00000013"})
            )
            .unwrap()["state"],
        "unreadable"
    );
    assert_eq!(
        engine
            .call("list_comments", json!({"id": "ndfx0020-00000020"}))
            .unwrap()["state"],
        "unreadable"
    );

    // 첨부: 규칙에 맞는 이름만, 이름 차례. 그림은 ![…], 그 밖은 […] 참조
    let attachments = engine
        .call("list_attachments", json!({"id": parent}))
        .unwrap();
    let names: Vec<&str> = attachments["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "a-first.htm",
            "a]b.pdf",
            "b-second.html",
            "it's!~*.svg",
            "readme.txt",
            "지도 (1).PNG"
        ]
    );
    assert_eq!(attachments["items"][1]["ref"], "[ab.pdf](attach/a%5Db.pdf)");
    assert_eq!(
        attachments["items"][5]["ref"],
        "![지도 (1).PNG](attach/%EC%A7%80%EB%8F%84%20%281%29.PNG)"
    );
    assert_eq!(
        engine
            .call(
                "list_attachments",
                json!({"id": parent, "subId": "snfx0002-00000012"})
            )
            .unwrap()["total"],
        1
    );

    // 사실층: 읽은 글, 상한을 넘은 글, 빈 파일
    let facts = engine
        .call("get_agenda_facts", json!({"id": "agfx0001-0000000a"}))
        .unwrap();
    assert_eq!(
        (
            facts["facts"].as_str(),
            facts["limit"].as_u64(),
            facts["editLimit"].as_u64()
        ),
        (
            Some("지금도 참인 것: 합성 vault다."),
            Some(2000),
            Some(10000)
        )
    );
    assert_eq!(
        engine
            .call("get_agenda_facts", json!({"id": "agfx0002-0000000b"}))
            .unwrap()["tooLargeToEdit"],
        true
    );
    assert!(
        engine
            .call("get_agenda_facts", json!({"id": "agfx0003-0000000c"}))
            .unwrap()
            .get("facts")
            .is_none()
    );
    assert!(
        engine
            .call("get_agenda_facts", json!({"id": "agzzzz-00000000"}))
            .is_err()
    );

    let _ = std::fs::remove_dir_all(&vault);
}

fn short_ids(page: &Value) -> Vec<String> {
    ids(page)
        .iter()
        .map(|id| id.rsplit('-').next().unwrap().to_string())
        .collect()
}

#[test]
fn searches_tags_and_templates() {
    let vault = fixture_with_search_notes();
    let engine = Engine::new(&vault);
    let search = |args: Value| engine.call("search_docs", args).unwrap();

    // 순위: 제목(만든 시각이 늦은 것 먼저) → 메타(태그) → 본문. AI에게는 기록 층 노트가 안 보인다
    let alpha = search(json!({"query": "alpha"}));
    assert_eq!(
        short_ids(&alpha),
        ["000000a4", "000000a1", "000000a2", "000000a3"]
    );
    let matched: Vec<&str> = alpha["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["matchedIn"].as_str().unwrap())
        .collect();
    assert_eq!(matched, ["title", "title", "meta", "body"]);
    assert!(
        alpha["items"][0].get("body").is_none(),
        "검색 줄에는 본문이 없다"
    );
    let ai = search(json!({"query": "alpha", "caller": "ai"}));
    assert_eq!(short_ids(&ai), ["000000a1", "000000a2", "000000a3"]);
    assert_eq!(ai["total"], 3);

    // 아이디로 찾기: `#doc:` 머리와 `/` 뒤는 떼고 대소문자를 가리지 않는다
    let by_id = search(json!({"query": " #DOC:ndsr0003-000000a3/아무거나 "}));
    assert_eq!(short_ids(&by_id), ["000000a3"]);
    assert_eq!(by_id["items"][0]["matchedIn"], "id");
    assert!(by_id["items"][0].get("snippet").is_none());

    // 영문 대소문자, 서식 제거, 맥락 글
    assert_eq!(search(json!({"query": "MIXED case"}))["total"], 1);
    assert_eq!(
        search(json!({"query": "scriptword"}))["total"],
        0,
        "스크립트는 본문이 아니다"
    );
    assert_eq!(
        search(json!({"query": "jsonword"}))["total"],
        0,
        "JSON 노트는 본문을 찾지 않는다"
    );
    let markup = search(json!({"query": "mkB"}));
    assert!(markup["items"][0]["snippet"].as_str().unwrap().starts_with(
        "mkB Heading Text quoted words item one item two numbered one col a col b cell1"
    ));
    let middle = search(json!({"query": "needleword"}));
    let snippet = middle["items"][0]["snippet"].as_str().unwrap();
    assert!(
        snippet.starts_with('…') && snippet.ends_with('…'),
        "{snippet}"
    );
    let head = search(json!({"query": "needlehead"}));
    assert!(
        !head["items"][0]["snippet"]
            .as_str()
            .unwrap()
            .starts_with('…')
    );
    // 앞뒤 공백과 BOM은 검색어에서 떼고, 본문에 낀 BOM은 맥락 글에서 떼지 않는다
    assert_eq!(
        short_ids(&search(json!({"query": "\u{FEFF} 회의록 \u{FEFF}"}))),
        ["000000ae"]
    );

    // 같은 만든 시각이면 표기가 늦은 것, 표기도 같으면 id가 큰 것이 먼저
    assert_eq!(
        short_ids(&search(json!({"query": "tiedword"}))),
        ["000000b2", "000000b1", "000000ab", "000000ac", "000000ad"]
    );

    // 건수와 오류
    let limited = search(json!({"query": "tiedword", "limit": 2}));
    assert_eq!(
        (
            limited["total"].as_u64(),
            limited["returned"].as_u64(),
            limited["truncated"].as_bool()
        ),
        (Some(5), Some(2), Some(true))
    );
    for bad in [0, 201, -1] {
        let err = engine
            .call("search_docs", json!({"query": "e", "limit": bad}))
            .unwrap_err();
        assert_eq!(
            (err.kind, err.message.as_str()),
            (
                ErrorKind::InvalidArgument,
                format!("받을 건수가 범위를 벗어났다: {bad}").as_str()
            )
        );
    }
    for blank in ["", "  ", "\t"] {
        let err = engine
            .call("search_docs", json!({"query": blank}))
            .unwrap_err();
        assert!(
            err.message.starts_with("검색어가 필요하다: search-docs"),
            "{}",
            err.message
        );
    }
    let by_agenda = search(json!({"query": "tiedword", "agenda": "work"}));
    assert_eq!(by_agenda["total"], 0);

    // 태그: 많이 쓰인 것부터, 같으면 바이트 차례. 기록 층 노트의 태그는 세지 않는다. Beta와 beta는 다른 태그
    let tags = engine.call("list_tags", json!({})).unwrap();
    assert_eq!(tags["items"][0], json!({"tag": "공통", "count": 3}));
    assert_eq!(tags["items"][1], json!({"tag": "Beta", "count": 2}));
    let names: Vec<&str> = tags["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["tag"].as_str().unwrap())
        .collect();
    assert!(
        !names.contains(&"기록") && !names.contains(&"alpha"),
        "{names:?}"
    );
    let beta = engine
        .call("list_tags", json!({"query": " BETA "}))
        .unwrap();
    assert_eq!(
        beta["items"],
        json!([{"tag": "Beta", "count": 2}, {"tag": "beta", "count": 1}])
    );
    let one = engine
        .call("list_tags", json!({"query": "beta", "limit": 1}))
        .unwrap();
    assert_eq!(
        (
            one["total"].as_u64(),
            one["returned"].as_u64(),
            one["truncated"].as_bool()
        ),
        (Some(2), Some(1), Some(true))
    );
    let err = engine.call("list_tags", json!({"limit": 0})).unwrap_err();
    assert_eq!(
        (err.kind, err.message.as_str()),
        (ErrorKind::InvalidArgument, "태그 개수 상한은 1 이상이다: 0")
    );

    // 템플릿 하나: 본문까지. 모양이 틀리면 400, 없으면 404
    let template = engine
        .call("get_template", json!({"id": "plan-basic"}))
        .unwrap();
    assert_eq!(
        (template["name"].as_str(), template["body"].as_str()),
        (Some("기본 계획"), Some("# 계획\n\n내용\n"))
    );
    for (id, kind) in [
        ("a b", ErrorKind::InvalidArgument),
        ("../x", ErrorKind::InvalidArgument),
        ("zzz-none", ErrorKind::NotFound),
        ("", ErrorKind::InvalidArgument),
    ] {
        let err = engine.call("get_template", json!({"id": id})).unwrap_err();
        assert_eq!(err.kind, kind, "{id:?}: {}", err.message);
    }
}

#[test]
fn lists_note_versions_from_git() {
    let vault = fixture_with_git_history();
    let engine = Engine::new(&vault);
    let history = |args: Value| engine.call("history", args);
    let parent = "ndfx0002-00000002";

    // 판: 시각 내림차순. 11-02의 고침은 같은 내용의 스냅숏에 접히고, main이 아닌 가지의 커밋은 빠진다
    let all = history(json!({"id": parent})).unwrap();
    assert_eq!(all["count"], 6);
    let rows = all["versions"].as_array().unwrap();
    let sources: Vec<&str> = rows.iter().map(|r| r["source"].as_str().unwrap()).collect();
    assert_eq!(
        sources,
        ["auto", "auto", "snapshot", "manual", "snapshot", "auto"]
    );
    assert_eq!(rows[0]["takenAt"], "2026-11-04T09:00:00+09:00");
    assert_eq!(rows[0]["index"], 1);
    assert_eq!(rows[2]["path"], "note.md", "스냅숏은 파일 이름만 가진다");
    assert_eq!(rows[0]["path"], "notes/demo/2026-11/parent/note.md");
    assert!(rows.iter().all(|r| r["bytes"].as_i64().unwrap() > 0));
    // 편집 정보는 그 판의 머리에서: 사람이 고친 판, 공백뿐인 편집자, 세션이 딸린 판(에이전트 이름은 소문자로)
    assert_eq!(rows[3]["editedBy"], "user");
    assert!(rows[3].get("editedSessionId").is_none());
    assert!(
        rows[1].get("editedBy").is_none(),
        "공백뿐인 editedBy는 없는 것"
    );
    assert_eq!(
        (
            rows[4]["editedBy"].as_str(),
            rows[4]["editedSessionId"].as_str(),
            rows[4]["editedSessionAgent"].as_str()
        ),
        (Some("tool"), Some("sess-1111"), Some("codex"))
    );
    assert!(all.get("body").is_none() && all.get("matches").is_none());

    // 번호로 고른 판: 파일 전체 글과 머리를 뗀 본문
    let second_from_last = history(json!({"id": parent, "number": 4})).unwrap();
    assert!(
        second_from_last["body"]
            .as_str()
            .unwrap()
            .starts_with("---\nid: ndfx0002-00000002")
    );
    assert!(
        second_from_last["noteBody"]
            .as_str()
            .unwrap()
            .contains("수정 둘 한글 검색어")
    );
    assert!(
        !second_from_last["noteBody"]
            .as_str()
            .unwrap()
            .contains("editedBy")
    );
    // 같은 판을 커밋으로도 고른다
    let commit = rows[3]["commit"].as_str().unwrap();
    let by_commit = history(json!({"id": parent, "commit": commit})).unwrap();
    assert_eq!(by_commit["noteBody"], second_from_last["noteBody"]);

    // 글자 찾기: 본문에서(머리는 빼고) 영문 대소문자를 가리지 않고
    let keyword = history(json!({"id": parent, "query": " KEYWORD "})).unwrap();
    assert_eq!(keyword["matches"], json!([rows[4]["commit"]]));
    assert_eq!(
        history(json!({"id": parent, "query": "한글 검색어"})).unwrap()["matches"],
        json!([rows[3]["commit"]])
    );
    assert_eq!(
        history(json!({"id": parent, "query": "editedBy"})).unwrap()["matches"],
        json!([]),
        "머리는 찾지 않는다"
    );

    // 모든 필드 노트: 수정 한 번과 되돌림(되돌린 판은 처음 판과 내용이 같아도 사이에 다른 판이 있어 접히지 않는다)
    assert_eq!(
        history(json!({"id": "ndfx0001-00000001"})).unwrap()["count"],
        3
    );
    // 폴더 이름이 바뀌었다 돌아온 노트: 내용이 같아 하나로 접힌다
    let moved = history(json!({"id": "ndfx0003-00000003"})).unwrap();
    assert_eq!(
        (
            moved["count"].as_u64(),
            moved["versions"][0]["path"].as_str()
        ),
        (Some(1), Some("notes/demo/loose/note.md"))
    );
    // 지금은 없는 노트는 커밋 메시지 꼬리 줄로 찾는다. 한 커밋이 두 노트를 더했어도 id가 든 파일을 고른다
    let ghost = history(json!({"id": "ndfx0040-00000040"})).unwrap();
    assert_eq!(
        (
            ghost["count"].as_u64(),
            ghost["versions"][0]["path"].as_str()
        ),
        (Some(1), Some("notes/demo/ghost/note.md"))
    );
    let ghost2 = history(json!({"id": "ndfx0041-00000041"})).unwrap();
    assert_eq!(ghost2["versions"][0]["path"], "notes/demo/ghost2/note.md");
    assert_eq!(
        ghost["versions"][0]["commit"],
        ghost2["versions"][0]["commit"]
    );
    // 없거나 안전하지 않은 id는 판이 없다
    for id in ["ndzz9999-99999999", "a b", "../x"] {
        assert_eq!(history(json!({"id": id})).unwrap()["count"], 0, "{id}");
    }

    // 오류
    let err = |args: Value| history(args).unwrap_err();
    let out_of_range = err(json!({"id": parent, "number": 7}));
    assert_eq!(
        (out_of_range.kind, out_of_range.message.as_str()),
        (
            ErrorKind::Conflict,
            "판 번호가 범위 밖이다(1 부터 6 까지 있다): 7"
        )
    );
    assert_eq!(
        err(json!({"id": parent, "number": 0})).message,
        "판 번호는 1 이상의 정수여야 한다: 0"
    );
    assert_eq!(
        err(json!({"id": parent, "commit": "ABC"})).message,
        "커밋은 40 자리 16 진수여야 한다: ABC"
    );
    assert!(
        err(json!({"id": parent, "commit": "0".repeat(40)}))
            .message
            .starts_with("그 커밋은 이 노트의 판 목록에 없다")
    );
    assert_eq!(
        err(json!({"id": parent, "number": 1, "commit": commit})).message,
        "판 번호와 커밋을 함께 줄 수 없다. 하나만 준다"
    );
    assert!(
        err(json!({"id": "  "}))
            .message
            .starts_with("인자가 하나 필요하다: history")
    );
    assert_eq!(
        err(json!({"id": "ndzz9999-99999999", "number": 1})).message,
        "그 노트에는 판이 아직 없다: ndzz9999-99999999"
    );
}

#[test]
fn a_vault_without_git_has_no_versions() {
    let vault = fixture();
    let engine = Engine::new(&vault);
    let none = engine
        .call("history", json!({"id": "ndfx0002-00000002"}))
        .unwrap();
    assert_eq!(
        (
            none["count"].as_u64(),
            none["versions"].as_array().map(Vec::len)
        ),
        (Some(0), Some(0))
    );
}

#[test]
fn reconciled_index_refuses_writes_when_comment_ids_clash() {
    use ais_core::vault::index::NoteIndex;
    let vault = fixture();
    // 겹침이 없으면 읽기용과 쓰기용이 같은 노트를 본다
    let clean = NoteIndex::reconciled(&vault).expect("겹침이 없다");
    assert_eq!(clean.root_notes().count(), 21);

    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/compat/add-comment-clash.sh");
    let output = Command::new("bash")
        .arg(script)
        .arg(&vault)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let error = NoteIndex::reconciled(&vault)
        .err()
        .expect("코멘트 id가 겹친다");
    assert_eq!(error.kind, ErrorKind::Internal);
    assert!(
        error
            .message
            .starts_with("UNIQUE constraint failed: comments.id (cmabc-"),
        "{}",
        error.message
    );
    assert!(
        error.message.ends_with("zcopy/comments.json)"),
        "{}",
        error.message
    );

    // 읽기는 처음 겹친 노트부터 뒤의 노트를 뺀 목록을 읽는다
    let docs = Engine::new(&vault)
        .call("list_docs", json!({"limit": 0}))
        .unwrap();
    let total = docs["total"].as_u64().unwrap();
    assert!(total < 21, "겹친 노트와 그 뒤가 빠진다: {total}");
    assert!(!ids(&docs).contains(&"ndcc0001-0000cc01".to_string()));
}

#[test]
fn write_foundation_checks_follow_the_java_rules() {
    use ais_core::vault::index::NoteIndex;
    use ais_core::write::{marker, precondition};

    let vault = fixture();
    let index = NoteIndex::fresh(&vault);
    let note = index.note("ndfx0002-00000002").unwrap();
    let token = note.token.clone().unwrap();
    let action = "고치지";

    // 읽은 값 그대로면 통과하고 파일 바이트를 돌려준다
    let bytes = precondition::check(&index.vault_root, note, Some(&token), action).unwrap();
    assert_eq!(bytes, std::fs::read(&note.content_file).unwrap());
    // 없거나 비면 412, 모양이 틀려도 412
    for missing in [None, Some("")] {
        let error = precondition::check(&index.vault_root, note, missing, action).unwrap_err();
        assert_eq!(
            (
                error.kind,
                error
                    .message
                    .starts_with("선결 조건 토큰이 안 왔어요. 아무것도 쓰지 않았습니다: notes/")
            ),
            (ErrorKind::PreconditionFailed, true)
        );
    }
    let malformed = precondition::check(&index.vault_root, note, Some("abc"), action).unwrap_err();
    assert!(
        malformed
            .message
            .starts_with("선결 조건 토큰의 모양이 아니에요."),
        "{}",
        malformed.message
    );
    // JSON 노트는 확인 값이 없어도 된다
    let json_note = index.note("ndfx0006-00000006").unwrap();
    assert!(precondition::check(&index.vault_root, json_note, None, action).is_ok());
    // 읽은 뒤 파일이 바뀌면 낡은 값이다
    std::fs::write(
        &note.content_file,
        "---\nid: ndfx0002-00000002\n---\n\n바뀜\n",
    )
    .unwrap();
    let stale = precondition::check(&index.vault_root, note, Some(&token), action).unwrap_err();
    assert!(
        stale
            .message
            .starts_with("다른 쪽이 이 노트를 먼저 고쳤어요."),
        "{}",
        stale.message
    );
    // 파일이 없으면 500
    std::fs::remove_file(&note.content_file).unwrap();
    let gone =
        precondition::check(&index.vault_root, note, Some(&token), "메타를 저장하지").unwrap_err();
    assert_eq!(gone.kind, ErrorKind::Internal);
    assert!(
        gone.message
            .starts_with("노트 본문 파일을 못 읽어 메타를 저장하지 않았다: notes/"),
        "{}",
        gone.message
    );

    // 워크스페이스 표식: 합성 vault에는 없고, 만들면 통과한다
    let error = marker::check_workspace_marker(&vault).unwrap_err();
    assert_eq!(error.kind, ErrorKind::InvalidArgument);
    assert!(
        error
            .message
            .starts_with("이 폴더는 아직 워크스페이스가 아니다: "),
        "{}",
        error.message
    );
    assert!(error.message.ends_with(". .aisurada/workspace.json 이 없다. 경로가 맞는지 보고, 새로 만들 자리면 init-workspace 를 먼저 부른다."));
    std::fs::write(vault.join(".aisurada/workspace.json"), "{}").unwrap();
    assert!(marker::check_workspace_marker(&vault).is_ok());
}

/// 쓰기 시험이 실제 홈에 잠금 파일을 쌓거나 커밋 대기 기록을 만들지 않게 한 번만 환경을 정한다.
fn isolate_writes() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let locks = std::env::temp_dir().join(format!("ais-test-locks-{}", std::process::id()));
        // SAFETY: 쓰기 시험만 이 함수를 부르고, 다른 스레드가 이 환경 변수를 읽는 것은 그 뒤다.
        unsafe {
            std::env::set_var("AISURADA_LOCK_DIR", locks);
            std::env::set_var("AISURADA_VAULT_AUTOCOMMIT_ENABLED", "false");
            std::env::set_var(
                "AISURADA_APPROVALS_DIR",
                std::env::temp_dir().join(format!("ais-test-approvals-{}", std::process::id())),
            );
        }
    });
}

#[test]
fn writes_agendas_through_the_write_foundation() {
    isolate_writes();
    let vault = fixture();
    let engine = Engine::new(&vault);

    // 표식이 없는 폴더에서는 일하지 않는다
    let no_marker = engine
        .call("create_agenda", json!({"title": "새"}))
        .unwrap_err();
    assert_eq!(no_marker.kind, ErrorKind::InvalidArgument);
    assert!(
        no_marker
            .message
            .starts_with("이 폴더는 아직 워크스페이스가 아니다: "),
        "{}",
        no_marker.message
    );
    assert!(!vault.join(".aisurada/agendas/새.md").exists());
    std::fs::write(vault.join(".aisurada/workspace.json"), "{}\n").unwrap();

    // 만들기: 파일 내용은 머리 + 요약이고 응답에는 딥링크가 붙는다
    let created = engine
        .call(
            "create_agenda",
            json!({"title": "새 아젠다", "hue": "teal", "summary": "요약"}),
        )
        .unwrap();
    let id = created["id"].as_str().unwrap().to_string();
    assert!(id.starts_with("ag") && created["slug"] == "새-아젠다" && created["hue"] == "teal");
    assert!(
        created["deepLink"]
            .as_str()
            .unwrap()
            .starts_with("aisurada://agenda/")
    );
    let file = vault.join(".aisurada/agendas/새-아젠다.md");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        format!("---\nid: {id}\nslug: 새-아젠다\ntitle: 새 아젠다\nhue: teal\n---\n요약")
    );
    // 같은 이름이면 번호가 붙고, 새 아젠다는 목록에 보인다
    let second = engine
        .call("create_agenda", json!({"title": "새 아젠다"}))
        .unwrap();
    assert_eq!(second["slug"], "새-아젠다-2");
    assert_eq!(engine.call("list_agendas", json!({})).unwrap()["total"], 6);

    // 고치기: 준 칸만 바뀌고 빈 값은 지운다
    let updated = engine
        .call(
            "update_agenda",
            json!({"id": id, "title": "바뀜", "repo": "o/r", "summary": ""}),
        )
        .unwrap();
    assert_eq!(
        (
            updated["title"].as_str(),
            updated["repo"].as_str(),
            updated["slug"].as_str()
        ),
        (Some("바뀜"), Some("o/r"), Some("새-아젠다"))
    );
    let cleared = engine
        .call("update_agenda", json!({"id": id, "repo": ""}))
        .unwrap();
    assert!(cleared.get("repo").is_none());
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("title: 바뀜\nhue: teal\n---\n") && !text.contains("repo:"),
        "{text}"
    );

    // 오류: 제목 없음, 없는 색, 모양이 틀린 id, 없는 id
    let message = |command: &str, args: Value| engine.call(command, args).unwrap_err().message;
    assert_eq!(message("create_agenda", json!({})), "title 이 필요하다");
    assert_eq!(
        message("create_agenda", json!({"title": "x", "hue": "zzz"})),
        "hue 값이 아니다: zzz"
    );
    assert_eq!(
        message("update_agenda", json!({"id": "nope"})),
        "아젠다 id 모양이 아니다: nope"
    );
    assert_eq!(
        message("update_agenda", json!({"id": "agzz0001-00000000"})),
        "Agenda not found: agzz0001-00000000"
    );

    // 지우기: 노트가 있으면 거절하고, 비어 있으면 휴지통 폴더로 옮긴다
    let has_notes = message("delete_agenda", json!({"id": "agfx0001-0000000a"}));
    assert!(
        has_notes.starts_with("노트가 ") && has_notes.contains("건 있는 아젠다는 지울 수 없다"),
        "{has_notes}"
    );
    let deleted = engine.call("delete_agenda", json!({"id": id})).unwrap();
    assert_eq!(
        deleted["archivedPath"],
        format!(".aisurada/trash/agendas/새-아젠다-{id}.md")
    );
    assert!(
        !file.exists()
            && vault
                .join(deleted["archivedPath"].as_str().unwrap())
                .is_file()
    );
    assert_eq!(deleted["notesDir"], "notes/새-아젠다");
}

#[test]
fn write_commands_refuse_to_run_on_an_unreconcilable_vault() {
    isolate_writes();
    let vault = fixture();
    std::fs::write(vault.join(".aisurada/workspace.json"), "{}\n").unwrap();
    let script =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/compat/add-comment-clash.sh");
    assert!(
        Command::new("bash")
            .arg(script)
            .arg(&vault)
            .output()
            .unwrap()
            .status
            .success()
    );
    let error = Engine::new(&vault)
        .call("create_agenda", json!({"title": "새"}))
        .unwrap_err();
    assert!(
        error
            .message
            .starts_with("UNIQUE constraint failed: comments.id ("),
        "{}",
        error.message
    );
    assert!(
        !vault.join(".aisurada/agendas/새.md").exists(),
        "일을 하지 않는다"
    );
}

#[test]
fn commit_vault_answers_with_outcome_and_blocked() {
    isolate_writes();
    let vault = fixture();
    let result = Engine::new(&vault).call("commit_vault", json!({})).unwrap();
    assert_eq!(
        (result["outcome"].as_str(), result["blocked"].as_str()),
        (Some("disabled"), Some("off"))
    );
}

/// 폴더 안의 모든 파일(경로 → 바이트). `.git`은 뺀다.
fn snapshot(root: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut std::collections::BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.file_name().is_some_and(|n| n == ".git") {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let rel = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                // 합성 vault에는 가리키는 곳이 없는 링크가 있어서 못 읽으면 빈 바이트로 본다
                out.insert(rel, std::fs::read(&path).unwrap_or_default());
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(root, root, &mut out);
    out
}

#[test]
fn commit_vault_waits_for_the_vault_write_lock() {
    isolate_writes();
    let vault = fixture();
    // 다른 쪽이 쓰는 중인 것처럼 vault 쓰기 잠금을 쥔다
    let held = ais_core::ports::lock().acquire(&vault).unwrap();
    let (sender, receiver) = std::sync::mpsc::channel();
    let path = vault.clone();
    let worker = std::thread::spawn(move || {
        let _ = sender.send(Engine::new(&path).call("commit_vault", json!({})));
    });
    // 잠금을 쥐고 있는 동안은 커밋이 끝나지 않는다(쓰다 만 파일을 커밋하지 않는다)
    assert!(
        receiver
            .recv_timeout(std::time::Duration::from_millis(400))
            .is_err(),
        "잠금을 쥔 채로 commit_vault가 끝났다"
    );
    drop(held);
    // 놓으면 이어서 커밋하고 답한다
    let result = receiver
        .recv_timeout(std::time::Duration::from_secs(20))
        .expect("잠금을 놓았는데도 commit_vault가 끝나지 않았다")
        .unwrap();
    assert_eq!(result["outcome"], "disabled");
    worker.join().unwrap();
}

#[test]
fn ai_cannot_write_to_record_tier_notes() {
    isolate_writes();
    let vault = fixture();
    std::fs::write(vault.join(".aisurada/workspace.json"), "{}\n").unwrap();
    let engine = Engine::new(&vault);
    let reading = "ndfx0002-00000002";

    // 앱은 기록 층 노트와 그 아래 후속 노트를 만들 수 있다
    let record = engine
        .call(
            "create_doc",
            json!({"agenda": "demo", "type": "free", "title": "기록", "body": "# 기록\n", "date": "2026-12-27", "time": "00:00", "tier": "record"}),
        )
        .unwrap();
    let id = record["id"].as_str().unwrap().to_string();
    let sub = engine
        .call(
            "create_sub_note",
            json!({"parent": id, "title": "후속", "body": "내용\n"}),
        )
        .unwrap();
    let sub_id = sub["id"].as_str().unwrap().to_string();
    let token = engine.call("get_doc", json!({"id": id})).unwrap()["precondition"]
        .as_str()
        .unwrap()
        .to_string();

    let ai = |command: &str, mut args: Value| {
        args["caller"] = json!("ai");
        engine.call(command, args).unwrap_err()
    };
    let before = snapshot(&vault);
    let sub_ref = format!("{id}/{sub_id}");
    let refused: Vec<(&str, Value)> = vec![
        // 확인 값이 없어도 412가 아니라 층 때문에 먼저 거절된다
        ("set_favorite", json!({"id": id})),
        ("set_favorite", json!({"id": id, "f1": token})),
        ("evaluate_doc", json!({"id": id, "score": 50, "f1": token})),
        ("link_docs", json!({"from": id, "to": reading, "f1": token})),
        (
            "update_doc",
            json!({"id": id, "title": "바뀜", "f1": token}),
        ),
        ("delete_doc", json!({"id": id})),
        (
            "create_sub_note",
            json!({"parent": id, "title": "t", "body": "b"}),
        ),
        (
            "create_sub_note",
            json!({"parent": sub_ref, "title": "t", "body": "b"}),
        ),
        (
            "update_sub_note",
            json!({"parent": id, "id": sub_id, "title": "바뀜"}),
        ),
        ("delete_sub_note", json!({"parent": id, "id": sub_id})),
        (
            "write_attachment",
            json!({"id": id, "name": "a.txt", "content": "c"}),
        ),
        (
            "write_attachment",
            json!({"id": id, "subId": sub_id, "name": "a.txt", "content": "c"}),
        ),
        ("create_comment", json!({"id": id, "body": "c"})),
        (
            "create_comment",
            json!({"id": id, "subId": sub_id, "body": "c"}),
        ),
    ];
    for (command, args) in refused {
        let error = ai(command, args.clone());
        assert_eq!(
            error.kind,
            ErrorKind::RecordTier,
            "{command} {args}: {}",
            error.message
        );
        assert!(
            error.message.starts_with(&format!(
                "도구 표면이 고칠 수 없는 노트다(층=record): {id}."
            )),
            "{command}: {}",
            error.message
        );
    }
    assert_eq!(snapshot(&vault), before, "거절된 쓰기가 vault를 바꿨다");

    // 읽기 층 노트는 AI도 그대로 고친다
    let reading_token = engine.call("get_doc", json!({"id": reading})).unwrap()["precondition"]
        .as_str()
        .unwrap()
        .to_string();
    let by_ai = engine
        .call(
            "set_favorite",
            json!({"id": reading, "f1": reading_token, "caller": "ai"}),
        )
        .unwrap();
    assert_eq!(by_ai["changed"], true);

    // 앱은 기록 층 노트도 그대로 고치고 휴지통에 보낸다
    let by_app = engine
        .call("set_favorite", json!({"id": id, "f1": token}))
        .unwrap();
    assert_eq!(by_app["changed"], true);
    let trashed = engine.call("delete_doc", json!({"id": id})).unwrap();
    assert_eq!(trashed["id"], id.as_str());
}

#[test]
fn ai_cannot_read_or_restore_record_tier_leftovers() {
    isolate_writes();
    let vault = fixture();
    std::fs::write(vault.join(".aisurada/workspace.json"), "{}\n").unwrap();
    let engine = Engine::new(&vault);
    let reading = "ndfx0002-00000002"; // 읽기 층 폴더 노트(대조군)
    let loose = "ndfx0003-00000003"; // 읽기 층(휴지통 대조군)
    let single_record = "ndfx0009-00000009"; // 기록 층, 단일 파일 노트
    let other_record = "ndfx0008-00000008"; // 기록 층

    // 앱이 기록 층 폴더 노트에 후속 노트, 코멘트, 첨부를 달고, 읽기 층 노트에도 코멘트와 첨부를 단다
    let secret = "비밀본문";
    let record = engine
        .call(
            "create_doc",
            json!({"agenda": "demo", "type": "free", "title": "기록", "body": format!("# 기록\n{secret}\n"), "date": "2026-12-27", "time": "00:00", "tier": "record"}),
        )
        .unwrap();
    let id = record["id"].as_str().unwrap().to_string();
    let sub = engine
        .call(
            "create_sub_note",
            json!({"parent": id, "title": "후속", "body": "내용\n"}),
        )
        .unwrap();
    let sub_id = sub["id"].as_str().unwrap().to_string();
    for owner in [
        json!({"id": id}),
        json!({"id": id, "subId": sub_id}),
        json!({"id": reading}),
    ] {
        let mut comment = owner.clone();
        comment["body"] = json!("코멘트");
        engine.call("create_comment", comment).unwrap();
        let mut attachment = owner.clone();
        attachment["name"] = json!("a.txt");
        attachment["content"] = json!("첨부");
        engine.call("write_attachment", attachment).unwrap();
    }

    // 코멘트와 첨부 목록: AI에게는 기록 층 노트가 안 열린다. 앱과 읽기 층 노트는 그대로다
    let ai = |command: &str, mut args: Value| {
        args["caller"] = json!("ai");
        engine.call(command, args)
    };
    for command in ["list_comments", "list_attachments"] {
        for args in [json!({"id": id}), json!({"id": id, "subId": sub_id})] {
            let error = ai(command, args.clone()).unwrap_err();
            assert_eq!(
                error.kind,
                ErrorKind::RecordTier,
                "{command} {args}: {}",
                error.message
            );
            assert!(
                error.message.starts_with(&format!(
                    "도구 표면이 읽을 수 없는 노트다(층=record): {id}."
                )),
                "{command}: {}",
                error.message
            );
            assert!(engine.call(command, args).is_ok(), "{command}: 앱은 읽는다");
        }
        assert!(
            ai(command, json!({"id": reading})).is_ok(),
            "{command}: 읽기 층 노트는 AI도 읽는다"
        );
    }

    // 마크다운 내보내기는 AI가 불러도 기록 층 본문을 안 내보낸다
    ai("export_markdown", json!({})).unwrap();
    let exported: String = snapshot(&vault.join(".aisurada/md-export"))
        .values()
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .collect();
    assert!(exported.contains("내용 한 줄."), "읽기 층 노트는 내보낸다");
    assert!(!exported.contains(secret), "기록 층 본문이 내보내졌다");

    // 휴지통: 앱이 기록 층 노트 셋(폴더 노트 둘, 단일 파일 노트 하나)과 읽기 층 노트 하나를 보낸다
    let mut trash_of = std::collections::BTreeMap::new();
    for note in [id.as_str(), single_record, other_record, loose] {
        let sent = engine.call("delete_doc", json!({"id": note})).unwrap();
        trash_of.insert(
            note.to_string(),
            sent["trashId"].as_str().unwrap().to_string(),
        );
    }
    let doc_ids = |page: &Value| -> Vec<String> {
        let mut found: Vec<String> = page["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["docId"].as_str().unwrap().to_string())
            .collect();
        found.sort();
        found
    };
    // 합성 vault에 원래 있던 휴지통 항목 넷도 읽기 층이라 AI에게 그대로 보인다
    let records = [id.as_str(), single_record, other_record];
    let app_ids = doc_ids(&engine.call("list_trash", Value::Null).unwrap());
    for note in records.iter().copied().chain([loose]) {
        assert!(
            app_ids.iter().any(|d| d == note),
            "앱에는 모두 보인다: {note}"
        );
    }
    let by_ai = ai("list_trash", json!({})).unwrap();
    let expected: Vec<String> = app_ids
        .iter()
        .filter(|d| !records.contains(&d.as_str()))
        .cloned()
        .collect();
    assert_eq!(doc_ids(&by_ai), expected, "{by_ai}");
    assert!(expected.iter().any(|d| d == loose));
    assert_eq!(by_ai["total"].as_u64(), Some(expected.len() as u64));

    // AI는 기록 층 노트를 되살리지 못하고, 거절은 휴지통 목록과 보관물을 건드리지 않는다
    let before = snapshot(&vault);
    for note in [id.as_str(), single_record, other_record] {
        let error = ai("restore_doc", json!({"trashId": trash_of[note]})).unwrap_err();
        assert_eq!(
            error.kind,
            ErrorKind::RecordTier,
            "{note}: {}",
            error.message
        );
        assert!(
            error.message.starts_with(&format!(
                "도구 표면이 고칠 수 없는 노트다(층=record): {note}."
            )),
            "{note}: {}",
            error.message
        );
    }
    assert_eq!(snapshot(&vault), before, "거절된 되살리기가 vault를 바꿨다");

    // 읽기 층 노트는 AI도 되살리고, 앱은 기록 층 노트도 되살린다
    let restored = ai("restore_doc", json!({"trashId": trash_of[loose]})).unwrap();
    assert_eq!(restored["restored"], true);
    for note in [id.as_str(), single_record] {
        let restored = engine
            .call("restore_doc", json!({"trashId": trash_of[note]}))
            .unwrap();
        assert_eq!(restored["restored"], true, "{note}");
    }
}

#[test]
fn writes_projects_through_the_write_foundation() {
    isolate_writes();
    let vault = fixture();
    std::fs::write(vault.join(".aisurada/workspace.json"), "{}\n").unwrap();
    let engine = Engine::new(&vault);
    let repos = r#"[{"name":"web","path":"/tmp/a b","github":"owner/name","githubId":9},{"name":"api","github":"o/n"}]"#;

    let created = engine
        .call(
            "create_project",
            json!({"title": "새 프로젝트", "hue": "teal", "summary": "요약", "repos": repos}),
        )
        .unwrap();
    let id = created["id"].as_str().unwrap().to_string();
    assert!(id.starts_with("pj") && created["slug"] == "새-프로젝트");
    assert_eq!(created["repos"][0]["githubId"], 9);
    let file = vault.join(".aisurada/projects/새-프로젝트.md");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        format!(
            "---\nid: {id}\nslug: 새-프로젝트\ntitle: 새 프로젝트\nhue: teal\nrepos: {repos}\n---\n요약"
        )
    );

    // 저장소를 통째로 바꾸고 지운다(빈 글)
    let swapped = engine
        .call(
            "update_project",
            json!({"id": id, "repos": r#"[{"name":"cli","path":"/x"}]"#}),
        )
        .unwrap();
    assert_eq!(swapped["repos"].as_array().unwrap().len(), 1);
    let cleared = engine
        .call("update_project", json!({"id": id, "repos": ""}))
        .unwrap();
    assert!(cleared.get("repos").is_none());
    assert!(!std::fs::read_to_string(&file).unwrap().contains("repos:"));

    // 오류는 Java와 같은 문구
    let message = |command: &str, args: Value| engine.call(command, args).unwrap_err().message;
    assert_eq!(
        message("create_project", json!({"title": "x", "repos": "[1]"})),
        "repos[1] 는 객체여야 한다"
    );
    assert_eq!(
        message("update_project", json!({"id": "nope"})),
        "프로젝트 id 모양이 아니다: nope"
    );
    assert_eq!(
        message("update_project", json!({"id": "pjzz0001-00000000"})),
        "Project not found: pjzz0001-00000000"
    );

    // 아젠다가 쓰는 프로젝트는 못 지우고, 아젠다를 옮기면 휴지통 폴더로 옮긴다
    let agenda = engine
        .call(
            "create_agenda",
            json!({"title": "소속", "project": "새-프로젝트"}),
        )
        .unwrap();
    assert!(
        message("delete_project", json!({"id": id}))
            .starts_with("아젠다가 1건 있는 프로젝트는 지울 수 없다.")
    );
    engine
        .call("update_agenda", json!({"id": agenda["id"], "project": ""}))
        .unwrap();
    let deleted = engine.call("delete_project", json!({"id": id})).unwrap();
    assert_eq!(
        deleted["archivedPath"],
        format!(".aisurada/trash/projects/새-프로젝트-{id}.md")
    );
    assert!(
        !file.exists()
            && vault
                .join(deleted["archivedPath"].as_str().unwrap())
                .is_file()
    );
}

#[test]
fn writes_templates_and_facts_through_the_write_foundation() {
    isolate_writes();
    let vault = fixture();
    std::fs::write(vault.join(".aisurada/workspace.json"), "{}\n").unwrap();
    let engine = Engine::new(&vault);
    let message = |command: &str, args: Value| engine.call(command, args).unwrap_err().message;

    // 문서 템플릿: 새로 만들면 기본 유형 plan이고, 들여쓴 블록이 있으면 경고가 붙는다
    let created = engine
        .call(
            "save_template",
            json!({"name": "새 양식", "body": "# 양식\n  # 들여쓴 제목\n"}),
        )
        .unwrap();
    let id = created["id"].as_str().unwrap().to_string();
    assert!(
        id.starts_with("tpl-") && created["action"] == "created" && created["builtin"] == false
    );
    assert!(
        created["warning"]
            .as_str()
            .unwrap()
            .starts_with("본문에 들여쓴 블록이 1곳 있어요(2번째 줄 제목)")
    );
    let file = vault.join(format!(".aisurada/templates/doc/{id}.md"));
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.starts_with(&format!(
            "---\nid: {id}\nname: 새 양식\ntype: plan\nformat: markdown\n"
        )),
        "{text}"
    );
    assert!(
        text.ends_with("builtin: false\n---\n# 양식\n  # 들여쓴 제목\n"),
        "{text}"
    );
    let updated = engine
        .call(
            "save_template",
            json!({"id": id, "type": "BUILD", "desc": "설명"}),
        )
        .unwrap();
    assert_eq!(updated["action"], "updated");
    assert!(
        std::fs::read_to_string(&file)
            .unwrap()
            .contains("type: build")
    );
    assert_eq!(
        message("save_template", json!({"name": "x"})),
        format!("새 템플릿은 body 이 필요하다: {}", SAVE_TEMPLATE_USAGE)
    );
    assert_eq!(
        message("save_template", json!({"id": "a b"})),
        "템플릿 id 모양이 아니다: a b"
    );
    assert_eq!(
        message("save_template", json!({"id": "nope-none"})),
        "Template not found: nope-none"
    );
    let removed = engine
        .call("delete_doc_template", json!({"id": id}))
        .unwrap();
    assert_eq!(
        removed["removed"],
        json!([format!(".aisurada/templates/doc/{id}.md")])
    );
    assert!(!file.exists());
    assert_eq!(
        engine
            .call("delete_doc_template", json!({"id": id}))
            .unwrap()["removed"],
        json!([])
    );

    // 사실층: 앞뒤 공백을 떼어 한 줄 끝 줄바꿈과 함께 쓰고, 빈 값이면 지운다
    let demo = "agfx0001-0000000a";
    let set = engine
        .call(
            "update_agenda_facts",
            json!({"id": demo, "facts": "  지금도 참인 것  "}),
        )
        .unwrap();
    assert_eq!(
        (
            set["facts"].as_str(),
            set["limit"].as_u64(),
            set["editLimit"].as_u64()
        ),
        (Some("지금도 참인 것"), Some(2000), Some(10000))
    );
    let facts_file = vault.join(set["path"].as_str().unwrap());
    assert_eq!(
        std::fs::read_to_string(&facts_file).unwrap(),
        "지금도 참인 것\n"
    );
    assert_eq!(
        message("update_agenda_facts", json!({"id": demo})),
        "facts 가 필요하다. 사실층을 지우려면 빈 값을 준다."
    );
    assert_eq!(
        message(
            "update_agenda_facts",
            json!({"id": demo, "facts": "가".repeat(2001)})
        ),
        "사실층이 상한을 넘었다: 2001 자. 상한은 2000 자다."
    );
    let cleared = engine
        .call("update_agenda_facts", json!({"id": demo, "facts": " "}))
        .unwrap();
    assert!(cleared.get("facts").is_none() && !facts_file.exists());
}

const SAVE_TEMPLATE_USAGE: &str = "save-template [id=<고칠 템플릿 id. 없으면 새로 만든다>] [name=<이름. 새로 만들 때 필수>] [type=<plan|build|report|research|meeting|free. 새로 만들 때 기본 plan>] [desc=<언제 쓰는 템플릿인지>] [body=<본문. 새로 만들 때 필수>]";

#[test]
fn initializes_a_workspace_and_seeds_templates_once() {
    isolate_writes();
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "ais-init-{}-{}/내 작업공간",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let engine = Engine::new(&dir);

    // 표식이 없는 빈 폴더에서도 일하고, 이름을 안 주면 폴더 이름을 쓴다
    let seeds = r###"[{"category":"doc","id":"plan-x","name":"계획","format":"markdown","desc":"","body":"# 계획\n","type":"plan","builtin":true},{"category":"ritual","id":"morning","name":"아침","format":"markdown","desc":"","body":"## 아침\n","rev":"morning","icon":"Sun"}]"###;
    let first = engine
        .call("init_workspace", json!({"templates": seeds}))
        .unwrap();
    assert_eq!(first["created"], true);
    assert_eq!(
        first["seeded"],
        json!([
            ".aisurada/templates/doc/plan-x.md",
            ".aisurada/templates/ritual/morning.md"
        ])
    );
    let marker: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join(".aisurada/workspace.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        (marker["schemaVersion"].as_u64(), marker["name"].as_str()),
        (Some(1), Some("내 작업공간"))
    );
    assert!(marker["createdAt"].as_str().unwrap().ends_with('Z'));
    for folder in [".aisurada/agendas", "notes"] {
        assert!(dir.join(folder).is_dir(), "{folder}");
    }

    // 다시 부르면 아무것도 새로 만들지 않고, 이미 있는 템플릿은 건너뛴다
    let second = engine
        .call("init_workspace", json!({"templates": seeds}))
        .unwrap();
    assert_eq!(
        (
            second["created"].as_bool(),
            second["seeded"].as_array().map(Vec::len),
            second["skipped"].as_array().map(Vec::len)
        ),
        (Some(false), Some(0), Some(2))
    );

    // 오류: 글이 아닌 인자, 공백 이름. 뼈대 폴더는 검사보다 먼저 만들어 둔다
    let message = |args: Value| engine.call("init_workspace", args).unwrap_err().message;
    assert_eq!(
        message(json!({"templates": "abc"})),
        "templates 은 JSON 배열이어야 한다: abc"
    );
    assert_eq!(
        message(json!({"templates": "[1]"})),
        "templates 항목이 객체가 아니다: 1"
    );
    assert_eq!(
        message(json!({"name": "  "})),
        "워크스페이스 이름이 비었다. 이름을 주거나 아예 빼서 폴더 이름을 쓰게 한다."
    );
    let _ = std::fs::remove_dir_all(dir.parent().unwrap());
}

#[test]
fn writes_note_meta_with_stamps_and_preconditions() {
    isolate_writes();
    let vault = fixture();
    std::fs::write(vault.join(".aisurada/workspace.json"), "{}\n").unwrap();
    let engine = Engine::new(&vault);
    let parent = "ndfx0002-00000002";
    let token = |id: &str| {
        engine.call("get_doc", json!({"id": id})).unwrap()["precondition"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let file = vault.join("notes/demo/2026-11/parent/note.md");
    let text = || std::fs::read_to_string(&file).unwrap();
    let error = |command: &str, args: Value| engine.call(command, args).unwrap_err();

    // 확인 값이 없으면 아무것도 안 쓰고 412다
    let before = text();
    assert_eq!(
        error("set_favorite", json!({"id": parent})).kind,
        ErrorKind::PreconditionFailed
    );
    assert_eq!(text(), before);

    // 앱이 켜면 본문은 그대로이고 머리에 즐겨찾기와 편집 도장이 붙는다
    let first = token(parent);
    let on = engine
        .call("set_favorite", json!({"id": parent, "f1": first}))
        .unwrap();
    assert_eq!(
        (on["favorite"].as_bool(), on["changed"].as_bool()),
        (Some(true), Some(true))
    );
    assert!(on["path"].as_str().unwrap().ends_with("/parent"));
    assert_ne!(on["precondition"].as_str().unwrap(), first);
    let written = text();
    assert!(
        written.contains("favorite: true\n")
            && written.contains("editedBy: app\n")
            && written.contains("editedAt: "),
        "{written}"
    );
    assert!(
        written.ends_with("# 본문 ndfx0002-00000002\n내용 한 줄.\n"),
        "본문은 그대로: {written}"
    );
    // 같은 확인 값을 다시 쓰면 낡은 값이다
    assert!(
        error("set_favorite", json!({"id": parent, "f1": first}))
            .message
            .starts_with("다른 쪽이 이 노트를 먼저 고쳤어요.")
    );
    // 바뀐 것이 없으면 확인 값 없이도 답하고 파일을 안 건드린다
    let same = engine
        .call("set_favorite", json!({"id": parent, "favorite": true}))
        .unwrap();
    assert_eq!(same["changed"], false);
    assert_eq!(text(), written);

    // AI 도구가 고치면 편집자가 tool이고 세션과 에이전트(소문자)가 적힌다
    let off = engine
        .call("set_favorite", json!({"id": parent, "favorite": false, "f1": token(parent), "caller": "ai", "session": "sess-1", "sessionAgent": "Codex"}))
        .unwrap();
    assert_eq!(off["favorite"], false);
    let by_tool = text();
    assert!(
        by_tool.contains("editedBy: tool\n")
            && by_tool.contains("editedSessionId: sess-1\n")
            && by_tool.contains("editedSessionAgent: codex\n"),
        "{by_tool}"
    );
    assert!(!by_tool.contains("favorite: true"));

    // 평가: 반올림해서 0~100으로, 메모는 다듬는다. 범위를 벗어나면 400, 지우면 머리에서 빠진다
    let rated = engine
        .call(
            "evaluate_doc",
            json!({"id": parent, "score": 87.5, "note": " 메모 ", "f1": token(parent)}),
        )
        .unwrap();
    assert_eq!(
        (
            rated["evaluation"]["score"].as_u64(),
            rated["evaluation"]["note"].as_str()
        ),
        (Some(88), Some("메모"))
    );
    assert!(text().contains("evaluation:\n  score: 88\n  note: 메모\n  at: "));
    assert_eq!(
        error(
            "evaluate_doc",
            json!({"id": parent, "score": 100.4, "f1": token(parent)})
        )
        .message,
        "평가 점수는 0 에서 100 사이여야 한다: 100.4"
    );
    assert_eq!(
        error("evaluate_doc", json!({"id": parent, "f1": token(parent)})).kind,
        ErrorKind::InvalidArgument
    );
    let cleared = engine
        .call(
            "evaluate_doc",
            json!({"id": parent, "clear": true, "f1": token(parent)}),
        )
        .unwrap();
    assert!(cleared["evaluation"].is_null() && !text().contains("evaluation:"));

    // 연결: 더하고, 이미 있으면 그대로, 빼고, 없는 대상은 경고만 붙는다
    let loose = "ndfx0003-00000003";
    let linked = engine
        .call(
            "link_docs",
            json!({"from": parent, "to": loose, "f1": token(parent)}),
        )
        .unwrap();
    assert_eq!(
        (linked["kind"].as_str(), linked["changed"].as_bool()),
        (Some("direct"), Some(true))
    );
    assert!(text().contains("links:\n  - to: ndfx0003-00000003\n    kind: direct\n"));
    let again = engine
        .call("link_docs", json!({"from": parent, "to": loose}))
        .unwrap();
    assert_eq!(
        (again["changed"].as_bool(), again["note"].as_str()),
        (Some(false), Some("이미 있는 연결이에요."))
    );
    let ghost = engine.call("link_docs", json!({"from": parent, "to": "ndzz0001-00000001", "kind": "ATTACH", "f1": token(parent)})).unwrap();
    assert!(
        ghost["warning"]
            .as_str()
            .unwrap()
            .starts_with("대상 문서 id를 vault에서 찾지 못했어요(ndzz0001-00000001)")
    );
    let removed = engine
        .call(
            "link_docs",
            json!({"from": parent, "to": loose, "remove": true, "f1": token(parent)}),
        )
        .unwrap();
    assert_eq!(
        (removed["removed"].as_bool(), removed["changed"].as_bool()),
        (Some(true), Some(true))
    );
    assert!(!text().contains(loose));
    assert_eq!(
        error("link_docs", json!({"from": parent, "to": parent})).message,
        format!("자기 자신과는 연결할 수 없다: {parent}")
    );

    // JSON 노트는 확인 값이 없어도 된다
    let json_note = engine
        .call("set_favorite", json!({"id": "ndfx0006-00000006"}))
        .unwrap();
    assert_eq!(json_note["changed"], true);
}

#[test]
fn updates_note_text_and_moves_the_note_to_its_canonical_place() {
    isolate_writes();
    let vault = fixture();
    std::fs::write(vault.join(".aisurada/workspace.json"), "{}\n").unwrap();
    let engine = Engine::new(&vault);
    let loose = "ndfx0003-00000003";
    let parent = "ndfx0002-00000002";
    let token = |id: &str| {
        engine.call("get_doc", json!({"id": id})).unwrap()["precondition"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let error = |args: Value| engine.call("update_doc", args).unwrap_err();
    let read = |rel: &str| std::fs::read_to_string(vault.join(rel)).unwrap();

    // 확인 값이 없으면 아무것도 안 쓴다
    let before = read("notes/demo/loose/note.md");
    assert_eq!(
        error(json!({"id": loose, "title": "x"})).kind,
        ErrorKind::PreconditionFailed
    );
    assert_eq!(read("notes/demo/loose/note.md"), before);
    // 입력 검사 순서: id → 태그 → 날짜 → 시각 → 제목
    assert_eq!(error(json!({})).kind, ErrorKind::InvalidArgument);
    assert_eq!(
        error(json!({"id": "nope"})).message,
        "노트 id 모양이 아니다: nope"
    );
    assert_eq!(
        error(json!({"id": loose, "title": " ", "tags": "[1]"})).message,
        "tags 항목이 문자열이 아니다: 1"
    );
    assert_eq!(
        error(json!({"id": loose, "title": " "})).message,
        format!("노트 제목이 비었다: {loose}")
    );
    assert_eq!(
        error(json!({"id": "ndzz0001-00000001"})).message,
        "Note not found: ndzz0001-00000001"
    );
    assert_eq!(
        error(json!({"id": loose, "tags": ""})).message,
        "tags 은 JSON 배열이어야 한다: "
    );
    assert_eq!(
        error(json!({"id": loose, "date": "2026-01-01\n"})).message,
        "date 은 YYYY-MM-DD 모양이어야 한다: 2026-01-01\n"
    );

    // 제목과 본문을 고치면 월별 폴더의 정식 이름으로 옮겨지고, 머리에 앱 도장이 붙는다
    let first = engine
        .call(
            "update_doc",
            json!({"id": loose, "title": "새 제목", "body": "# 다른 헤딩\n본문\n", "tags": ["b", " a ", "", "b"], "f1": token(loose)}),
        )
        .unwrap();
    assert_eq!(first["renamed"], true);
    let path = first["path"].as_str().unwrap().to_string();
    assert!(
        path.starts_with("notes/demo/2026-11/2026-11-06-0800-"),
        "{path}"
    );
    assert!(path.ends_with("-0003"), "{path}");
    assert!(!vault.join("notes/demo/loose").exists());
    let text = read(&format!("{path}/note.md"));
    assert!(
        text.contains("title: 새 제목\n") && text.contains("editedBy: app\n"),
        "{text}"
    );
    assert!(text.contains("tags:\n  - b\n  - a\n"), "{text}");
    assert!(text.ends_with("---\n# 다른 헤딩\n본문\n"), "{text}");
    assert!(
        first["warning"]
            .as_str()
            .unwrap()
            .starts_with("본문 첫 헤딩(\"다른 헤딩\")이 제목(\"새 제목\")과 달라요"),
        "{first}"
    );
    assert_eq!(token(loose), first["precondition"].as_str().unwrap());

    // 같은 값을 다시 쓰면 도장을 건드리지 않고 자리도 그대로다
    let again = engine
        .call(
            "update_doc",
            json!({"id": loose, "title": "새 제목", "body": "# 다른 헤딩\n본문\n", "f1": token(loose)}),
        )
        .unwrap();
    assert_eq!(
        (again["renamed"].as_bool(), again["path"].as_str()),
        (Some(false), Some(path.as_str()))
    );
    assert_eq!(read(&format!("{path}/note.md")), text);

    // 시각만 바꾸면 날짜와 오프셋(Z)은 그대로 지킨다. 명령줄 꼴의 tags 글도 받는다
    let timed = engine
        .call(
            "update_doc",
            json!({"id": loose, "time": "23:59", "tags": "[\"x\"]", "f1": token(loose)}),
        )
        .unwrap();
    assert!(
        timed["path"].as_str().unwrap().contains("2026-11-06-2359-"),
        "{timed}"
    );
    assert!(
        read(&format!("{}/note.md", timed["path"].as_str().unwrap()))
            .contains("2026-11-06T23:59:00Z")
    );

    // AI 호출은 날짜와 시각을 받지 않고, 도장은 tool과 세션이 적힌다
    let by_tool = engine
        .call(
            "update_doc",
            json!({"id": loose, "date": "2030-01-01", "time": "01:01", "title": "AI 제목", "f1": token(loose), "caller": "ai", "session": "sess-9", "sessionAgent": "Codex"}),
        )
        .unwrap();
    let tool_path = by_tool["path"].as_str().unwrap();
    assert!(
        tool_path.contains("2026-11-06-2359-"),
        "날짜와 시각은 무시: {tool_path}"
    );
    let tool_text = read(&format!("{tool_path}/note.md"));
    assert!(
        tool_text.contains("editedBy: tool\n")
            && tool_text.contains("editedSessionId: sess-9\n")
            && tool_text.contains("editedSessionAgent: codex\n"),
        "{tool_text}"
    );

    // 후속 노트가 있는 폴더 노트는 폴더째 옮겨진다
    let moved = engine
        .call(
            "update_doc",
            json!({"id": parent, "date": "2026-12-24", "f1": token(parent)}),
        )
        .unwrap();
    let moved_path = moved["path"].as_str().unwrap();
    assert!(
        moved_path.starts_with("notes/demo/2026-12/2026-12-24-0900-"),
        "{moved_path}"
    );
    assert!(vault.join(moved_path).join("sub/child1.md").is_file());

    // JSON 노트는 본문을 못 고치고, 단일 .html 노트는 이름만 바뀌며 옛 파일이 사라진다
    assert_eq!(
        error(json!({"id": "ndfx0006-00000006", "body": "x"})).kind,
        ErrorKind::Internal
    );
    let page = engine
        .call(
            "update_doc",
            json!({"id": "ndfx0005-00000005", "title": "새 페이지", "f1": token("ndfx0005-00000005")}),
        )
        .unwrap();
    assert!(page["path"].as_str().unwrap().ends_with(".html"), "{page}");
    assert!(!vault.join("notes/work/page.html").exists());
    assert!(vault.join(page["path"].as_str().unwrap()).is_file());
}

#[test]
fn creates_notes_and_agendas_with_warnings() {
    isolate_writes();
    let vault = fixture();
    std::fs::write(vault.join(".aisurada/workspace.json"), "{}\n").unwrap();
    let engine = Engine::new(&vault);
    let error = |args: Value| engine.call("create_doc", args).unwrap_err();
    let read = |rel: &str| std::fs::read_to_string(vault.join(rel)).unwrap();

    // 필수 인자와 값 검사
    assert!(
        error(json!({"type": "free", "title": "t", "body": "b"}))
            .message
            .starts_with("agenda 이 필요하다: create-doc ")
    );
    assert_eq!(
        error(json!({"agenda": "demo", "type": "zzz", "title": "t", "body": "b"})).message,
        "모르는 유형 이다: zzz. 쓸 수 있는 것은 [plan, build, report, research, meeting, free] 뿐이다."
    );
    assert_eq!(
        error(json!({"agenda": "demo", "type": "free", "title": " ", "body": "b"})).message,
        "노트 제목이 비었다"
    );
    assert_eq!(
        error(json!({"agenda": "demo", "type": "free", "title": "t", "body": "b", "links": [{"to": "nope", "kind": "direct"}]})).message,
        "노트 id 모양이 아니다: nope"
    );

    // 날짜와 시각을 주면 월별 폴더의 정식 이름에 쓰고, 태그와 연결은 배열로도 받는다
    let first = engine
        .call(
            "create_doc",
            json!({
                "agenda": "demo", "type": "PLAN", "title": "새 노트", "body": "# 다른 헤딩\n  ## 들여씀\n",
                "date": "2026-12-24", "time": "10:20", "tags": [" b ", "b", ""],
                "links": [{"to": "ndfx0002-00000002", "kind": "DIRECT"}, {"to": "ndfx0002-00000002", "kind": "direct"}],
                "template": " my-tpl "
            }),
        )
        .unwrap();
    let id = first["id"].as_str().unwrap();
    let path = first["path"].as_str().unwrap();
    assert!(
        path.starts_with("notes/demo/2026-12/2026-12-24-1020-"),
        "{path}"
    );
    assert_eq!(first["date"], "2026-12-24");
    assert_eq!(first["agendaCreated"], false);
    assert!(
        first["deepLink"]
            .as_str()
            .unwrap()
            .starts_with(&format!("aisurada://doc/{id}?ws="))
    );
    let text = read(&format!("{path}/note.md"));
    assert!(
        text.contains("type: plan\n")
            && text.contains("created: '2026-12-24T10:20:00+09:00'\n")
            && text.contains("tags:\n  - b\n")
            && text.contains("template: my-tpl\n")
            && text.contains("links:\n  - to: ndfx0002-00000002\n    kind: direct\n"),
        "{text}"
    );
    assert!(
        !text.contains("editedBy"),
        "새 노트에는 편집 도장이 없다: {text}"
    );
    let warning = first["warning"].as_str().unwrap();
    assert!(
        warning.contains("들여쓴 블록이 1곳")
            && warning.contains("본문 첫 헤딩(\"다른 헤딩\")이 제목(\"새 노트\")과 달라요"),
        "{warning}"
    );
    assert_eq!(
        engine.call("get_doc", json!({"id": id})).unwrap()["precondition"],
        first["precondition"]
    );

    // 같은 아젠다에 같은 제목이 있으면 그 노트를 알린다
    let again = engine
        .call(
            "create_doc",
            json!({"agenda": "demo", "type": "free", "title": " 새  노트 ", "body": "# 새 노트\n", "date": "2026-12-24", "time": "10:21"}),
        )
        .unwrap();
    assert!(
        again["warning"].as_str().unwrap().starts_with(&format!(
            "같은 아젠다에 제목이 같은 노트가 이미 있어요: {id}(2026-12-24)"
        )),
        "{again}"
    );

    // 없는 아젠다 이름은 새로 만들고(저장소·프로젝트 없음), 다음에는 slug로 찾는다
    let created = engine
        .call(
            "create_doc",
            json!({"agenda": "신규 아젠다", "type": "free", "title": "t", "body": "b", "date": "2026-12-25", "time": "00:00", "tier": "record"}),
        )
        .unwrap();
    assert_eq!(
        (
            created["agenda"].as_str(),
            created["agendaCreated"].as_bool()
        ),
        (Some("신규-아젠다"), Some(true))
    );
    let agenda_file = read(".aisurada/agendas/신규-아젠다.md");
    assert!(
        agenda_file.contains("title: 신규 아젠다\n") && !agenda_file.contains("project"),
        "{agenda_file}"
    );
    assert!(
        read(&format!("{}/note.md", created["path"].as_str().unwrap())).contains("tier: record\n")
    );
    let found = engine
        .call(
            "create_doc",
            json!({"agenda": "신규-아젠다", "type": "free", "title": "t2", "body": "b", "date": "2026-12-25", "time": "00:01"}),
        )
        .unwrap();
    assert_eq!(found["agendaCreated"], false);

    // AI 도구 호출은 층을 받지 않고, 세션과 에이전트가 머리에 적히며, 원본 요청 절이 없으면 경고가 붙는다
    let by_tool = engine
        .call(
            "create_doc",
            json!({"agenda": "demo", "type": "free", "title": "AI 노트", "body": "# AI 노트\n\n요약\n", "date": "2026-12-26", "time": "00:00",
                   "tier": "record", "caller": "ai", "session": "sess-1", "sessionAgent": "Codex"}),
        )
        .unwrap();
    let tool_text = read(&format!("{}/note.md", by_tool["path"].as_str().unwrap()));
    assert!(
        tool_text.contains("sessionId: sess-1\n")
            && tool_text.contains("sessionAgent: codex\n")
            && !tool_text.contains("tier:"),
        "{tool_text}"
    );
    assert!(
        by_tool["warning"]
            .as_str()
            .unwrap()
            .contains("\"## 원본 요청\" 절이 없어요"),
        "{by_tool}"
    );
}

#[test]
fn writes_sub_notes_and_strips_their_cards() {
    isolate_writes();
    let vault = fixture();
    std::fs::write(vault.join(".aisurada/workspace.json"), "{}\n").unwrap();
    let engine = Engine::new(&vault);
    let parent = "ndfx0002-00000002";
    let child2 = "snfx0002-00000012";
    let grand = "snfx0003-00000013";
    let sub_token = |parent: &str, id: &str| {
        engine
            .call("get_sub_note", json!({"parent": parent, "id": id}))
            .unwrap()["precondition"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let read = |rel: &str| std::fs::read_to_string(vault.join(rel)).unwrap();

    // 만들기: 메인 노트 아래와 후속 노트 아래. 후속 노트 id 하나만으로는 부모를 못 가리킨다
    let created = engine
        .call(
            "create_sub_note",
            json!({"parent": parent, "title": "새 후속", "body": "# 다른 헤딩\n"}),
        )
        .unwrap();
    let id = created["id"].as_str().unwrap();
    assert_eq!(created["ref"], format!("{parent}/{id}"));
    assert!(
        created["path"].as_str().unwrap().contains("/parent/sub/"),
        "{created}"
    );
    assert!(
        created["warning"]
            .as_str()
            .unwrap()
            .starts_with("본문 첫 헤딩(\"다른 헤딩\")이 제목(\"새 후속\")과 달라요")
    );
    let text = read(&format!("{}/note.md", created["path"].as_str().unwrap()));
    assert!(
        text.starts_with("---\nid: sn")
            && text.contains("type: free\n")
            && !text.contains("editedBy"),
        "{text}"
    );
    assert_eq!(
        sub_token(parent, id),
        created["precondition"].as_str().unwrap()
    );
    assert_eq!(
        engine
            .call(
                "create_sub_note",
                json!({"parent": child2, "title": "t", "body": "b"})
            )
            .unwrap_err()
            .message,
        format!("노트 id 모양이 아니다: {child2}")
    );
    let nested = engine
        .call(
            "create_sub_note",
            json!({"parent": format!(" {parent} / {child2} "), "title": "손자", "body": "x"}),
        )
        .unwrap();
    assert_eq!(nested["parent"], format!("{parent}/{child2}"));
    assert!(
        nested["path"].as_str().unwrap().contains("/child2/sub/"),
        "{nested}"
    );
    assert_eq!(
        engine
            .call(
                "create_sub_note",
                json!({"parent": format!("{parent}/snfx0001-00000011"), "title": "t", "body": "b"})
            )
            .unwrap_err()
            .kind,
        ErrorKind::Internal
    );

    // 고치기: 본문만 주면 제목은 첫 헤딩을 따르고, 폴더가 정식 이름으로 옮겨진다. AI 호출은 tool 도장을 남긴다
    let updated = engine
        .call(
            "update_sub_note",
            json!({"parent": parent, "id": child2, "body": "# 헤딩 제목\n\n본문\n", "f1": sub_token(parent, child2)}),
        )
        .unwrap();
    assert_eq!(updated["title"], "헤딩 제목");
    let updated_path = updated["path"].as_str().unwrap();
    assert!(
        updated_path.contains("/sub/2026-11-05-1100-헤딩-제목-0012"),
        "{updated_path}"
    );
    assert!(read(&format!("{updated_path}/note.md")).contains("editedBy: app\n"));
    assert!(
        vault.join(updated_path).join("sub/grand").is_dir(),
        "아래 후속 노트가 따라옴"
    );
    let by_tool = engine
        .call(
            "update_sub_note",
            json!({"parent": parent, "id": child2, "tags": ["a", " a ", "b"], "f1": sub_token(parent, child2), "caller": "ai", "session": "sess-3", "sessionAgent": "Codex"}),
        )
        .unwrap();
    let tool_text = read(&format!("{}/note.md", by_tool["path"].as_str().unwrap()));
    assert!(
        tool_text.contains("editedBy: tool\n")
            && tool_text.contains("editedSessionId: sess-3\n")
            && tool_text.contains("tags:\n  - a\n  - b\n"),
        "{tool_text}"
    );
    assert_eq!(
        engine
            .call(
                "update_sub_note",
                json!({"parent": parent, "id": child2, "title": "x"})
            )
            .unwrap_err()
            .kind,
        ErrorKind::PreconditionFailed
    );

    // 지우기: 부모 본문에서 그 노트의 카드 줄만 걷어내고, 그 노트와 아래를 지운다
    let under = format!("{parent}/{child2}");
    engine
        .call(
            "update_sub_note",
            json!({"parent": parent, "id": child2, "body": format!("# 자식\n[카드](#sub:{grand})\n끝\n"), "f1": sub_token(parent, child2)}),
        )
        .unwrap();
    let deleted = engine
        .call("delete_sub_note", json!({"parent": under, "id": grand}))
        .unwrap();
    assert_eq!(
        (
            deleted["deleted"].as_bool(),
            deleted["cardRemoved"].as_bool()
        ),
        (Some(true), Some(true))
    );
    assert_ne!(deleted["containerBefore"], deleted["containerAfter"]);
    assert!(!vault.join(deleted["path"].as_str().unwrap()).exists());
    let container = engine
        .call("get_sub_note", json!({"parent": parent, "id": child2}))
        .unwrap();
    assert!(
        !container["body"].as_str().unwrap().contains("#sub:"),
        "{container}"
    );
    assert_eq!(
        engine
            .call("delete_sub_note", json!({"parent": under, "id": grand}))
            .unwrap_err()
            .message,
        format!("SubNote not found: {parent}/{grand}")
    );
    let plain = engine
        .call("delete_sub_note", json!({"parent": parent, "id": id}))
        .unwrap();
    assert_eq!(plain["cardRemoved"], false);
    assert!(plain.get("containerBefore").is_none());
}

#[test]
fn writes_comments_and_strips_their_anchors() {
    isolate_writes();
    let vault = fixture();
    std::fs::write(vault.join(".aisurada/workspace.json"), "{}\n").unwrap();
    let engine = Engine::new(&vault);
    let parent = "ndfx0002-00000002";
    let loose = "ndfx0003-00000003";
    let read = |rel: &str| std::fs::read_to_string(vault.join(rel)).unwrap();
    let error = |command: &str, args: Value| engine.call(command, args).unwrap_err();

    // 만들기: 번호를 발급하고 앵커는 공백뿐이면 저장하지 않는다. 파일은 `{"v": 1, "comments": [...]}`로 다시 쓰인다
    let created = engine
        .call(
            "create_comment",
            json!({"id": parent, "body": "새 코멘트", "anchor_quote": "인용"}),
        )
        .unwrap();
    let new_id = created["id"].as_str().unwrap().to_string();
    assert!(
        new_id.starts_with("cm") && created["total"] == 4,
        "{created}"
    );
    assert_eq!(created["path"], "notes/demo/2026-11/parent/comments.json");
    let file = read("notes/demo/2026-11/parent/comments.json");
    assert!(
        file.starts_with(
            "{\n  \"v\": 1,\n  \"comments\": [\n    {\n      \"id\": \"cmabc-0a0b0c0d\","
        ),
        "{file}"
    );
    assert!(
        file.contains(
            "\"anchor\": {\n        \"quote\": \"인용\"\n      },\n      \"body\": \"새 코멘트\""
        ),
        "{file}"
    );
    assert!(file.ends_with("\n  ]\n}\n"));
    let listed = engine.call("list_comments", json!({"id": parent})).unwrap();
    assert_eq!(listed["comments"].as_array().unwrap().len(), 4);
    let blank_quote = engine
        .call("create_comment", json!({"id": parent, "body": "공백 앵커", "comment_id": " cmblank-00000001 ", "anchor_quote": "  "}))
        .unwrap();
    assert_eq!(blank_quote["id"], "cmblank-00000001");
    let written: Value =
        serde_json::from_str(&read("notes/demo/2026-11/parent/comments.json")).unwrap();
    let row = written["comments"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "cmblank-00000001")
        .unwrap();
    assert!(row.get("anchor").is_none(), "{row}");

    // 고치기: 만든 시각 뒤로 editedAt이 붙는다. 없는 코멘트와 중복 식별자는 409다
    let edited = engine
        .call(
            "update_comment",
            json!({"id": parent, "comment_id": &new_id, "body": "고친 코멘트"}),
        )
        .unwrap();
    assert_eq!(edited["total"], 5);
    assert!(read("notes/demo/2026-11/parent/comments.json").contains("\"editedAt\": \""));
    assert_eq!(
        error(
            "update_comment",
            json!({"id": parent, "comment_id": "cmzz-00000000", "body": "x"})
        )
        .message,
        "그 코멘트가 없다: cmzz-00000000"
    );
    assert_eq!(
        error(
            "create_comment",
            json!({"id": parent, "body": "x", "comment_id": &new_id})
        )
        .kind,
        ErrorKind::Conflict
    );
    assert_eq!(
        error("create_comment", json!({"id": parent, "body": "  "})).message,
        "코멘트 본문이 비었다."
    );
    assert!(
        error("update_comment", json!({"id": parent, "body": "x"}))
            .message
            .starts_with("comment_id 이 필요하다")
    );

    // 후속 노트의 코멘트는 바로 위 노트 폴더의 sub-comments/ 에 놓인다
    let child2 = "snfx0002-00000012";
    let sub = engine
        .call(
            "create_comment",
            json!({"id": parent, "subId": child2, "body": "후속", "comment_id": "cmsub-00000009"}),
        )
        .unwrap();
    assert_eq!(
        sub["path"],
        format!("notes/demo/2026-11/parent/sub-comments/{child2}.json")
    );

    // 지우기: 본문에서 그 코멘트의 앵커 표시만 걷어내고, 마지막 코멘트를 지우면 파일이 사라진다
    let token = engine.call("get_doc", json!({"id": parent})).unwrap()["precondition"]
        .as_str()
        .unwrap()
        .to_string();
    engine
        .call(
            "update_doc",
            json!({"id": parent, "body": format!("# 부모\n<!--aisc:{new_id}-->첫<!--/aisc:{new_id}--> <!--aisc:cmabc-1a2b3c4d-->둘<!--/aisc:cmabc-1a2b3c4d-->\n"), "f1": token}),
        )
        .unwrap();
    let deleted = engine
        .call(
            "delete_comment",
            json!({"id": parent, "comment_id": &new_id}),
        )
        .unwrap();
    assert_eq!(
        (
            deleted["markerStripped"].as_bool(),
            deleted["remaining"].as_u64()
        ),
        (Some(true), Some(4))
    );
    assert_ne!(deleted["bodyBefore"], deleted["bodyAfter"]);
    let body = engine.call("get_doc", json!({"id": parent})).unwrap()["body"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        body.contains("첫 <!--aisc:cmabc-1a2b3c4d-->둘<!--/aisc:cmabc-1a2b3c4d-->")
            && !body.contains(&new_id),
        "{body}"
    );
    let plain = engine
        .call(
            "delete_comment",
            json!({"id": parent, "comment_id": "cmblank-00000001"}),
        )
        .unwrap();
    assert_eq!(plain["markerStripped"], false);
    assert!(plain.get("bodyBefore").is_none());
    let only = engine
        .call(
            "create_comment",
            json!({"id": loose, "body": "하나뿐", "comment_id": "cmone-00000001"}),
        )
        .unwrap();
    let only_file = vault.join(only["path"].as_str().unwrap());
    assert!(only_file.is_file());
    engine
        .call(
            "delete_comment",
            json!({"id": loose, "comment_id": "cmone-00000001"}),
        )
        .unwrap();
    assert!(!only_file.exists());
}

#[test]
fn moves_notes_through_the_trash() {
    isolate_writes();
    let vault = fixture();
    std::fs::write(vault.join(".aisurada/workspace.json"), "{}\n").unwrap();
    let engine = Engine::new(&vault);
    let loose = "ndfx0003-00000003"; // 즐겨찾기, 라벨 `보관`
    let html = "ndfx0005-00000005"; // 단일 .html 파일
    let error = |command: &str, args: Value| engine.call(command, args).unwrap_err();

    // 보내기: 노트 폴더가 보관 폴더로 옮겨지고 목록 맨 앞에 항목이 생기며, 즐겨찾기였다는 경고가 붙는다
    let sent = engine.call("delete_doc", json!({"id": loose})).unwrap();
    let trash_id = sent["trashId"].as_str().unwrap().to_string();
    assert!(
        trash_id.starts_with("tr") && sent["file"] == trash_id.as_str(),
        "{sent}"
    );
    assert_eq!(sent["originalPath"], "notes/demo/loose");
    assert_eq!(
        (
            sent["wasFavorite"].as_bool(),
            sent["favoriteCategory"].as_str()
        ),
        (Some(true), Some("보관"))
    );
    assert!(
        sent["warning"]
            .as_str()
            .unwrap()
            .starts_with("이 노트는 즐겨찾기였어요(카테고리 '보관') — "),
        "{sent}"
    );
    assert!(
        vault
            .join(".aisurada/trash/files")
            .join(&trash_id)
            .join("note.md")
            .is_file()
    );
    assert!(!vault.join("notes/demo/loose").exists());
    assert_eq!(
        error("get_doc", json!({"id": loose})).kind,
        ErrorKind::NotFound
    );
    let listed = engine.call("list_trash", Value::Null).unwrap();
    assert_eq!(listed["items"][0]["trashId"], trash_id.as_str());
    let manifest = std::fs::read_to_string(vault.join(".aisurada/trash/trash.json")).unwrap();
    assert!(
        manifest.starts_with(&format!(
            "[\n  {{\n    \"trashId\": \"{trash_id}\",\n    \"file\": \"{trash_id}\","
        )),
        "{manifest}"
    );
    assert_eq!(
        error("delete_doc", json!({"id": loose})).message,
        format!("Note not found: {loose}")
    );

    // 단일 파일 노트는 확장자가 붙은 이름으로 보관된다
    let page = engine.call("delete_doc", json!({"id": html})).unwrap();
    let page_trash = page["trashId"].as_str().unwrap().to_string();
    assert_eq!(page["file"], format!("{page_trash}.html"));
    assert!(
        vault
            .join(".aisurada/trash/files")
            .join(format!("{page_trash}.html"))
            .is_file()
    );

    // 되살리기: 원래 아젠다의 월별 폴더로 돌아오고 같은 id로 다시 읽힌다. 같은 이름이 있으면 `-restored-`가 붙는다
    let restored = engine
        .call("restore_doc", json!({"trashId": &trash_id}))
        .unwrap();
    assert_eq!(restored["restored"], true);
    let restored_path = restored["path"].as_str().unwrap();
    assert_eq!(restored_path, "notes/demo/2026-11/loose");
    assert_eq!(
        engine.call("get_doc", json!({"id": loose})).unwrap()["path"],
        restored_path
    );
    assert!(
        engine.call("list_trash", Value::Null).unwrap()["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["trashId"] != trash_id.as_str())
    );
    std::fs::create_dir_all(vault.join("notes/work/2026-11/page.html")).unwrap();
    let again = engine
        .call("restore_doc", json!({"trashId": &page_trash}))
        .unwrap();
    assert!(
        again["path"].as_str().unwrap().contains("-restored-"),
        "{again}"
    );
    assert_eq!(
        error("restore_doc", json!({"trashId": &trash_id})).message,
        format!("TrashEntry not found: {trash_id}")
    );
    assert_eq!(
        error("restore_doc", json!({"trashId": "nope"})).kind,
        ErrorKind::InvalidArgument
    );
    // 보관 파일이 사라진 항목은 되살릴 수 없고 목록에서도 정리된다
    assert!(
        error("restore_doc", json!({"trashId": "trfx0003-000000b3"}))
            .message
            .starts_with("보관 파일이 사라져 복구할 수 없어요")
    );
    assert!(
        engine.call("list_trash", Value::Null).unwrap()["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["trashId"] != "trfx0003-000000b3")
    );

    // 영구 삭제와 비우기(목록에 없는 보관물은 건드리지 않고 세기만 한다)
    let gone = engine
        .call("delete_doc", json!({"id": "ndfx0008-00000008"}))
        .unwrap();
    let gone_file = vault
        .join(".aisurada/trash/files")
        .join(gone["file"].as_str().unwrap());
    assert!(gone_file.exists());
    assert_eq!(
        engine
            .call("purge_doc", json!({"trashId": gone["trashId"]}))
            .unwrap()["purged"],
        true
    );
    assert!(!gone_file.exists());
    std::fs::write(vault.join(".aisurada/trash/files/orphan.txt"), "고아").unwrap();
    let emptied = engine.call("empty_trash", Value::Null).unwrap();
    assert_eq!(
        (emptied["skipped"].as_u64(), emptied["orphans"].as_u64()),
        (Some(0), Some(1)),
        "{emptied}"
    );
    assert_eq!(
        std::fs::read_to_string(vault.join(".aisurada/trash/trash.json")).unwrap(),
        "[]\n"
    );
    assert!(vault.join(".aisurada/trash/files/orphan.txt").exists());

    // 목록이 1,000,000바이트를 넘으면 가장 오래된 항목부터 영구 삭제한다
    let rows: Vec<String> = (0..340)
        .map(|n| format!("  {{\"trashId\": \"trbg{n:04x}-0000{n:04x}\", \"file\": \"trbg{n:04x}.md\", \"originalPath\": \"notes/demo/2026-11/b{n}\", \"docId\": \"ndbg{n:04x}-0000{n:04x}\", \"agendaId\": \"agfx0001-0000000a\", \"agendaSlug\": \"demo\", \"title\": \"{}\", \"created\": \"2026-11-01T09:00:00+09:00\", \"trashedAt\": \"2026-11-02T01:00:00.000Z\"}}", "가".repeat(1000)))
        .collect();
    std::fs::write(
        vault.join(".aisurada/trash/trash.json"),
        format!("[\n{}\n]\n", rows.join(",\n")),
    )
    .unwrap();
    let overflow = engine
        .call("delete_doc", json!({"id": "ndfx0009-00000009"}))
        .unwrap();
    assert!(
        overflow["autoPurged"]
            .as_array()
            .is_some_and(|a| !a.is_empty()),
        "{overflow}"
    );
    assert!(
        std::fs::metadata(vault.join(".aisurada/trash/trash.json"))
            .unwrap()
            .len()
            <= 1_000_000
    );
}

#[test]
fn runs_the_task_lifecycle() {
    isolate_writes();
    let vault = fixture();
    std::fs::write(vault.join(".aisurada/workspace.json"), "{}\n").unwrap();
    let engine = Engine::new(&vault);
    let error = |command: &str, args: Value| engine.call(command, args).unwrap_err();
    let read = |rel: &str| std::fs::read_to_string(vault.join(rel)).unwrap();

    // 만들기: 완료 기준은 필수다. 파일은 SnakeYAML 모양이고 활동에 `created`가 남는다
    assert_eq!(
        error("create_task", json!({"title": "t", "repo": "a/b"})).kind,
        ErrorKind::InvalidArgument
    );
    let created = engine
        .call(
            "create_task",
            json!({"title": "첫 태스크", "repo": "owner/name", "criteria": ["하나", {"text": "둘", "done": true}], "labels": ["b", "a", "b"], "type": "BUG"}),
        )
        .unwrap();
    assert_eq!(
        (created["key"].as_str(), created["created"].as_bool()),
        (Some("TSK-1"), Some(true))
    );
    assert_eq!(created["status"], "draft");
    assert_eq!(created["criteriaDone"], 1);
    let text = read(".aisurada/tasks/TSK-1.md");
    assert!(
        text.starts_with("---\nkey: TSK-1\nid: tk")
            && text.contains("\ntype: bug\npriority: medium\nstatus: draft\nrepo: owner/name\nlabels:\n  - b\n  - a\ncriteria:\n  - text: 하나\n    done: false\n  - text: 둘\n    done: true\n    checkedAt: ")
            && text.ends_with("---\n\n"),
        "{text}"
    );
    let activity = read(".aisurada/tasks/TSK-1/activity.jsonl");
    assert!(
        activity.starts_with("{\"at\":\"")
            && activity.contains("\"kind\":\"created\",\"by\":\"human\"}\n"),
        "{activity}"
    );

    // 고치기: 확인 값이 필요하고, 바뀐 것이 없으면 파일도 활동도 그대로다
    let token = created["precondition"].as_str().unwrap().to_string();
    assert_eq!(
        error("update_task", json!({"id": "TSK-1", "title": "x"})).kind,
        ErrorKind::PreconditionFailed
    );
    let before = read(".aisurada/tasks/TSK-1.md");
    let same = engine
        .call(
            "update_task",
            json!({"id": "TSK-1", "title": "첫 태스크", "f1": &token}),
        )
        .unwrap();
    assert_eq!(same["changed"], false);
    assert_eq!(read(".aisurada/tasks/TSK-1.md"), before);
    let updated = engine
        .call(
            "update_task",
            json!({"id": "TSK-1", "title": "새 제목", "f1": &token, "project": "alpha"}),
        )
        .unwrap();
    assert_eq!(updated["changed"], true);
    assert!(
        read(".aisurada/tasks/TSK-1/activity.jsonl")
            .contains("\"kind\":\"edited\",\"by\":\"human\",\"fields\":[\"title\",\"project\"]}")
    );
    assert_eq!(
        error(
            "update_task",
            json!({"id": "TSK-1", "title": "x", "f1": &token})
        )
        .kind,
        ErrorKind::PreconditionFailed
    );

    // 상태: 사람과 러너가 할 수 있는 전이가 다르고, 사람이 갈 수 있는 곳은 응답에 있다
    let ready = engine
        .call(
            "transition_task",
            json!({"id": "TSK-1", "to": "ready-for-develop"}),
        )
        .unwrap();
    assert_eq!(ready["from"], "draft");
    assert!(
        ready["transitions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["to"] == "in-progress" && t["ok"] == true)
    );
    assert!(
        error("transition_task", json!({"id": "TSK-1", "to": "in-review"}))
            .message
            .contains("로는 바꿀 수 없다")
    );
    assert_eq!(
        error(
            "transition_task",
            json!({"id": "TSK-1", "to": "in-progress", "from": "draft"})
        )
        .message,
        "상태가 이미 바뀌었다: 지금 ready-for-develop, 기대한 것 draft"
    );

    // AI 세션이 고치면 세션과(claude가 아니면) 에이전트 이름이 파일과 활동에 남는다
    let by_ai = engine
        .call(
            "transition_task",
            json!({"id": "TSK-1", "to": "in-progress", "actor": "runner", "caller": "ai", "session": "sess-5", "sessionAgent": "codex"}),
        )
        .unwrap();
    assert_eq!(by_ai["editedSession"], "sess-5");
    assert_eq!(by_ai["editedAgent"], "codex");
    let claude = engine
        .call(
            "add_task_comment",
            json!({"id": "TSK-1", "body": " 댓글 ", "actor": "runner", "session": "sess-6", "sessionAgent": "claude"}),
        )
        .unwrap();
    assert_eq!(claude["comment"]["session"], "sess-6");
    assert!(claude["comment"].get("agent").is_none());

    // 연결과 막힘: 막은 태스크가 Done 이 되기 전에는 시작하지 못한다
    let second = engine
        .call(
            "create_task",
            json!({"title": "둘째", "repo": "a/b", "criteria": ["x"]}),
        )
        .unwrap();
    let key2 = second["key"].as_str().unwrap();
    engine
        .call(
            "transition_task",
            json!({"id": key2, "to": "ready-for-develop"}),
        )
        .unwrap();
    let linked = engine
        .call(
            "link_tasks",
            json!({"from": "TSK-1", "to": key2, "type": "blocks"}),
        )
        .unwrap();
    assert_eq!(linked["changed"], true);
    assert!(
        linked["links"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["type"] == "blocks" && l["key"] == key2)
    );
    assert!(
        error("transition_task", json!({"id": key2, "to": "in-progress"}))
            .message
            .starts_with("다른 태스크에 막혀 있어 시작하지 못한다")
    );
    assert_eq!(
        engine.call("get_task", json!({"id": key2})).unwrap()["links"][0]["type"],
        "blocked-by"
    );
    assert!(
        error(
            "link_tasks",
            json!({"from": key2, "to": "TSK-1", "type": "blocks"})
        )
        .message
        .contains("서로 막게 할 수 없다")
    );
    let listed = engine.call("list_tasks", json!({"blocked": true})).unwrap();
    assert_eq!(listed["tasks"].as_array().unwrap().len(), 1);

    // 휴지통: 지우면 활동까지 옮겨지고 번호를 다시 쓰지 않는다. 되살리면 같은 키로 돌아온다
    let deleted = engine.call("delete_task", json!({"id": key2})).unwrap();
    assert_eq!(
        (
            deleted["trashed"].as_bool(),
            deleted["wasBlockedBy"][0].as_str()
        ),
        (Some(true), Some("TSK-1"))
    );
    assert!(!vault.join(format!(".aisurada/tasks/{key2}.md")).exists());
    assert_eq!(
        engine.call("list_tasks", json!({"trash": true})).unwrap()["trash"][0]["key"],
        key2
    );
    let third = engine
        .call(
            "create_task",
            json!({"title": "셋째", "repo": "a/b", "criteria": ["x"]}),
        )
        .unwrap();
    assert_eq!(third["key"], "TSK-3");
    let restored = engine.call("restore_task", json!({"id": key2})).unwrap();
    assert_eq!(
        (restored["restored"].as_bool(), restored["key"].as_str()),
        (Some(true), Some(key2))
    );
    assert!(
        vault
            .join(format!(".aisurada/tasks/{key2}/activity.jsonl"))
            .is_file()
    );

    // 손으로 쓴 파일: 모르는 칸이 고친 뒤에도 남고, 못 읽는 파일은 건너뛴다고 알린다
    std::fs::write(
        vault.join(".aisurada/tasks/TSK-20.md"),
        "---\nkey: TSK-20\nid: tkex0001-00000001\ntitle: 손\nstatus: draft\ncustom: 5\nscore: 1.5\nnested:\n  a: [1, 2]\n---\n본문\n",
    )
    .unwrap();
    std::fs::write(vault.join(".aisurada/tasks/TSK-21.md"), "머리말이 없다\n").unwrap();
    let hand = engine.call("get_task", json!({"id": "TSK-20"})).unwrap();
    engine
        .call(
            "update_task",
            json!({"id": "TSK-20", "title": "고침", "f1": hand["precondition"]}),
        )
        .unwrap();
    let rewritten = read(".aisurada/tasks/TSK-20.md");
    assert!(
        rewritten.contains("\ncustom: 5\nscore: 1.5\nnested:\n  a:\n    - 1\n    - 2\n---\n본문\n"),
        "{rewritten}"
    );
    let all = engine.call("list_tasks", json!({})).unwrap();
    assert!(
        all["skipped"][0]
            .as_str()
            .unwrap()
            .starts_with(".aisurada/tasks/TSK-21.md: 머리말(---)로 시작하지 않는다"),
        "{all}"
    );
    assert_eq!(
        error("get_task", json!({"id": "TSK-0"})).kind,
        ErrorKind::InvalidArgument
    );
    assert_eq!(
        error("get_task", json!({"id": "TSK-99"})).message,
        "태스크를 못 찾았다: TSK-99"
    );
}

/// 승인 기록장(앱 폴더의 `ledger.json`) 글. vault 경로 해시 폴더를 스스로 찾는다.
fn ledger_text(vault: &Path) -> String {
    let dir = ais_core::resource::approvals::dir(vault).unwrap();
    std::fs::read_to_string(dir.join("ledger.json")).unwrap_or_default()
}

#[test]
fn queues_ai_resource_changes_until_a_human_approves() {
    isolate_writes();
    let vault = fixture();
    std::fs::write(vault.join(".aisurada/workspace.json"), "{}\n").unwrap();
    let engine = Engine::new(&vault);
    let error = |command: &str, args: Value| engine.call(command, args).unwrap_err();
    let read = |rel: &str| std::fs::read_to_string(vault.join(rel)).unwrap();
    let ai = |mut args: Value| {
        args["caller"] = json!("ai");
        args["session"] = json!("ai-1");
        args["sessionAgent"] = json!("codex");
        args
    };

    // 사람이 만든 것은 바로 적용되고 승인한 판으로 기록된다(by=human)
    let skill = engine
        .call(
            "save_resource",
            json!({"kind": "skill", "scope": "global", "name": "리뷰 스킬", "description": "d", "body": "본문"}),
        )
        .unwrap();
    let skill_id = skill["id"].as_str().unwrap().to_string();
    let agent = engine
        .call(
            "save_resource",
            json!({"kind": "agent", "scope": "global", "name": "리뷰어", "description": "d", "permission": "read"}),
        )
        .unwrap();
    let agent_id = agent["id"].as_str().unwrap().to_string();
    let agent_path = agent["path"].as_str().unwrap().to_string();
    assert!(ledger_text(&vault).contains("\"by\": \"human\""));

    // AI가 프로젝트 스킬을 만들면 정책(project=auto)에 따라 바로 적용되고, 기록은 by=policy다. 고친 기록에는 세션과 도구가 남는다
    let own = engine
        .call(
            "save_resource",
            ai(json!({"kind": "skill", "project": "alpha", "name": "알파 스킬", "description": "d"})),
        )
        .unwrap();
    assert_eq!(own["created"], true);
    let own_text = read(own["path"].as_str().unwrap());
    assert!(
        own_text.contains("editedSession: ai-1\n") && own_text.contains("editedAgent: codex\n"),
        "{own_text}"
    );
    assert!(ledger_text(&vault).contains("\"by\": \"policy\""));

    // 권한 올림은 정책과 상관없이 대기에 오른다. 파일과 목록은 그대로다
    let before = read(&agent_path);
    let token = agent["precondition"].as_str().unwrap().to_string();
    let request = ai(json!({"id": agent_id, "permission": "edit", "f1": token}));
    let queued = engine.call("save_resource", request.clone()).unwrap();
    assert_eq!(queued["queued"], true);
    // 이유는 정책이 승인을 요구하는 갈래들이다(글로벌 변경과 권한 올림)
    let reasons = queued["reasons"].as_array().unwrap();
    assert_eq!(
        reasons
            .iter()
            .map(|r| r["category"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["global-change", "permission-raise"]
    );
    assert_eq!(reasons[1]["fixed"], true);
    let change_id = queued["changeId"].as_str().unwrap().to_string();
    assert!(change_id.starts_with("pc"));
    assert_eq!(read(&agent_path), before);
    let listed = engine
        .call("get_resource", json!({"id": agent_id}))
        .unwrap();
    assert_eq!(listed["permission"], "read");

    // 같은 요청을 되풀이해도 쌓이지 않는다
    let again = engine.call("save_resource", request).unwrap();
    assert_eq!(
        (again["duplicate"].as_bool(), again["changeId"].as_str()),
        (Some(true), Some(change_id.as_str()))
    );
    let overview = engine.call("list_resource_changes", json!({})).unwrap();
    assert_eq!(overview["counts"]["pending"], 1);
    let detail = engine
        .call("get_resource_change", json!({"id": change_id}))
        .unwrap();
    assert_eq!(detail["changedFields"], json!(["permission"]));
    assert_eq!(detail["proposer"]["session"], "ai-1");
    assert_eq!(detail["proposer"]["channel"], "mcp");
    assert_eq!(detail["args"]["permission"], "edit");

    // 승인, 거절, 직접 처리, 정책 변경은 사람만 한다. 세션 표식만 달려도 AI다
    for (command, args) in [
        ("approve_resource_change", json!({"id": change_id})),
        ("reject_resource_change", json!({"id": change_id})),
        (
            "resolve_resource_edit",
            json!({"id": agent_id, "action": "approve"}),
        ),
        ("set_resource_policy", json!({"project": "approve"})),
    ] {
        assert_eq!(
            error(command, ai(args.clone())).kind,
            ErrorKind::Forbidden,
            "{command}"
        );
        assert_eq!(
            error(command, {
                let mut by_session = args.clone();
                by_session["session"] = json!("ai-2");
                by_session
            })
            .kind,
            ErrorKind::Forbidden,
            "{command} 세션"
        );
    }

    // 사람이 승인하면 올린 사람의 기록(세션, 도구, 올린 시각)으로 적용되고 승인 기록은 by=approved다
    let approved = engine
        .call("approve_resource_change", json!({"id": change_id}))
        .unwrap();
    assert_eq!(approved["approved"], true);
    assert_eq!(approved["result"]["changed"], true);
    let after = read(&agent_path);
    assert!(
        after.contains("permission: edit\n")
            && after.contains("editedSession: ai-1\n")
            && after.contains("editedAgent: codex\n"),
        "{after}"
    );
    assert!(ledger_text(&vault).contains("\"by\": \"approved\""));
    let done = engine
        .call("get_resource_change", json!({"id": change_id}))
        .unwrap();
    assert_eq!(
        (done["status"].as_str(), done["decidedBy"].as_str()),
        (Some("approved"), Some("human"))
    );
    // 시각처럼 보이는 글은 SnakeYAML이 따옴표로 감싼다
    assert!(
        after.contains(&format!(
            "editedAt: '{}'\n",
            done["createdAt"].as_str().unwrap()
        )),
        "{after}"
    );
    assert_eq!(
        error("approve_resource_change", json!({"id": change_id})).kind,
        ErrorKind::Conflict
    );

    // 글로벌 자원 만들기는 대기에 올릴 때 보여 준 ID 그대로 만들어진다
    let create = engine
        .call(
            "save_resource",
            ai(json!({"kind": "skill", "scope": "global", "name": "제안 스킬", "description": "d"})),
        )
        .unwrap();
    assert_eq!(create["reasons"][0]["category"], "global-change");
    let create_change = create["changeId"].as_str().unwrap().to_string();
    let planned = engine
        .call("get_resource_change", json!({"id": create_change}))
        .unwrap()["forcedId"]
        .as_str()
        .unwrap()
        .to_string();
    let made = engine
        .call("approve_resource_change", json!({"id": create_change}))
        .unwrap();
    assert_eq!(made["result"]["id"], planned.as_str());

    // 거절은 이유를 다듬어 남기고 자원은 그대로다
    let doomed = engine
        .call("delete_resource", ai(json!({"id": skill_id})))
        .unwrap();
    assert_eq!(doomed["queued"], true);
    let rejected = engine
        .call(
            "reject_resource_change",
            json!({"id": doomed["changeId"], "reason": "  아직 쓴다  "}),
        )
        .unwrap();
    assert_eq!(rejected["rejected"], true);
    let row = engine
        .call("get_resource_change", json!({"id": doomed["changeId"]}))
        .unwrap();
    assert_eq!(
        (row["status"].as_str(), row["reason"].as_str()),
        (Some("rejected"), Some("아직 쓴다"))
    );
    assert!(engine.call("get_resource", json!({"id": skill_id})).is_ok());

    // 기준이 바뀌어 적용하지 못하면 낡은 변경(stale)으로 남고 오류를 돌려준다
    let token = engine
        .call("get_resource", json!({"id": agent_id}))
        .unwrap()["precondition"]
        .as_str()
        .unwrap()
        .to_string();
    let late = engine
        .call(
            "save_resource",
            ai(json!({"id": agent_id, "permission": "publish", "f1": token})),
        )
        .unwrap();
    let late_id = late["changeId"].as_str().unwrap().to_string();
    engine
        .call(
            "save_resource",
            json!({"id": agent_id, "description": "사람이 먼저", "f1": token}),
        )
        .unwrap();
    assert_eq!(
        error("approve_resource_change", json!({"id": late_id})).kind,
        ErrorKind::PreconditionFailed
    );
    let stale = engine
        .call("get_resource_change", json!({"id": late_id}))
        .unwrap();
    assert_eq!(stale["status"], "stale");
    assert!(
        stale["reason"]
            .as_str()
            .unwrap()
            .starts_with("적용하지 못했다: ")
    );

    // 정책: 글로벌 변경을 auto로 풀면 AI도 바로 적용한다. 권한 올림은 고정이라 여전히 대기한다
    engine
        .call("set_resource_policy", json!({"global": "auto"}))
        .unwrap();
    assert_eq!(
        engine.call("get_resource_policy", json!({})).unwrap()["policy"]["global"],
        "auto"
    );
    let token = engine
        .call("get_resource", json!({"id": agent_id}))
        .unwrap()["precondition"]
        .as_str()
        .unwrap()
        .to_string();
    let direct = engine
        .call(
            "save_resource",
            ai(json!({"id": agent_id, "description": "AI가 바로", "f1": token})),
        )
        .unwrap();
    assert_eq!(direct["changed"], true);
    assert!(direct.get("queued").is_none());
    let token = direct["precondition"].as_str().unwrap().to_string();
    let raise = engine
        .call(
            "save_resource",
            ai(json!({"id": agent_id, "permission": "publish", "f1": token})),
        )
        .unwrap();
    assert_eq!(raise["queued"], true);

    // 참조 켜기는 고정 종류다. 참조 끄기는 정책(refs=approve)을 따른다
    let ref_add = engine
        .call(
            "set_resource_ref",
            ai(json!({"project": "alpha", "id": skill_id})),
        )
        .unwrap();
    assert_eq!(ref_add["reasons"][0]["category"], "ref-add");
    engine
        .call(
            "approve_resource_change",
            json!({"id": ref_add["changeId"]}),
        )
        .unwrap();
    let usable = engine
        .call("usable_resources", json!({"project": "alpha"}))
        .unwrap();
    assert_eq!(usable["skills"].as_array().unwrap().len(), 2);
    let ref_off = engine
        .call(
            "set_resource_ref",
            ai(json!({"project": "alpha", "id": skill_id, "remove": true})),
        )
        .unwrap();
    assert_eq!(ref_off["reasons"][0]["category"], "ref-remove");
    assert!(ref_off["summary"].as_str().unwrap().contains("1개 끄기"));

    // 승인한 뒤 손으로 고친 파일은 승인한 판으로 보이고, 목록에 dirty가 붙는다
    let live = read(&agent_path);
    std::fs::write(vault.join(&agent_path), format!("{live}\n손으로\n")).unwrap();
    let rows = engine
        .call("list_resources", json!({"kind": "agent"}))
        .unwrap();
    assert_eq!(rows["resources"][0]["approval"], "dirty");
    assert_eq!(rows["approvals"]["unapproved"], 1);
    let overview = engine.call("list_resource_changes", json!({})).unwrap();
    assert_eq!(overview["unapproved"][0]["state"], "dirty");
    assert_eq!(overview["unapproved"][0]["id"], agent_id.as_str());
    engine
        .call(
            "resolve_resource_edit",
            json!({"id": agent_id, "action": "revert"}),
        )
        .unwrap();
    assert_eq!(read(&agent_path), live);
}

#[test]
fn reads_resource_arguments_like_the_command_line() {
    isolate_writes();
    let vault = fixture();
    std::fs::write(vault.join(".aisurada/workspace.json"), "{}\n").unwrap();
    let engine = Engine::new(&vault);
    let error = |command: &str, args: Value| engine.call(command, args).unwrap_err();

    // 참거짓과 숫자도 명령줄 글처럼 받고, 모르는 칸은 무시한다
    let skill = engine
        .call(
            "save_resource",
            json!({"kind": "skill", "scope": "global", "name": "글 인자", "description": "d", "bodies": 1}),
        )
        .unwrap();
    assert_eq!(skill["created"], true);
    let listed = engine
        .call("list_resources", json!({"bodies": true, "scope": "global"}))
        .unwrap();
    assert_eq!(listed["resources"][0]["body"], "");
    assert_eq!(
        error(
            "save_resource",
            json!({"kind": "skill", "scope": "global", "name": "x"})
        )
        .message,
        "description 이 필요하다"
    );
    assert_eq!(
        error("save_resource", json!({"scope": "global"})).message,
        "kind 는 skill 이나 agent 여야 한다: null"
    );
    // tools는 객체로 줘도 글로 줘도 같다
    let a = engine
        .call(
            "save_resource",
            json!({"kind": "skill", "scope": "global", "name": "도구 객체", "description": "d", "tools": {"claude": {"n": 1}}}),
        )
        .unwrap();
    let b = engine
        .call(
            "save_resource",
            json!({"kind": "skill", "scope": "global", "name": "도구 글", "description": "d", "tools": "{\"claude\":{\"n\":1}}"}),
        )
        .unwrap();
    assert_eq!(a["tools"], b["tools"]);
    // 읽기만 하는 명령도 표식이 없는 폴더에서는 일하지 않는다
    let bare = std::env::temp_dir().join(format!("ais-res-bare-{}", std::process::id()));
    std::fs::create_dir_all(&bare).unwrap();
    assert_eq!(
        Engine::new(&bare)
            .call("list_resources", json!({}))
            .unwrap_err()
            .kind,
        ErrorKind::InvalidArgument
    );
}

#[test]
fn appends_ask_and_chat_turns_next_to_the_note() {
    isolate_writes();
    let vault = fixture();
    std::fs::write(vault.join(".aisurada/workspace.json"), "{}\n").unwrap();
    let engine = Engine::new(&vault);
    let error = |command: &str, args: Value| engine.call(command, args).unwrap_err();
    let (parent, child, single) = (
        "ndfx0002-00000002",
        "snfx0002-00000012",
        "ndfx0004-00000004",
    );
    let turn = |extra: Value| {
        let mut base = json!({
            "session-id": "sess-1", "turn-id": "t1", "mark-id": "m1", "asked-at": "2026-11-14T10:00:00+09:00",
            "question": "무엇인가?", "answer": "이것이다.", "model": "opus", "effort": "high", "elapsed-ms": 1200
        });
        for (key, value) in extra.as_object().unwrap() {
            base[key] = value.clone();
        }
        base
    };
    let with_id = |mut args: Value, id: &str, sub: Option<&str>| {
        args["id"] = json!(id);
        if let Some(sub) = sub {
            args["subId"] = json!(sub);
        }
        args
    };

    // 문답: 파일이 없으면 null, 붙이면 만들어지고 둘째 회차는 첫 세션 id를 지킨다
    assert_eq!(
        engine.call("get_ask", json!({"id": parent})).unwrap(),
        Value::Null
    );
    let first = engine
        .call(
            "append_ask_turn",
            with_id(turn(json!({"session-lines": " 3 ", "images": ["attach/a.png", "attach/b.jpg"], "quote": "인용"})), parent, None),
        )
        .unwrap();
    let ask_path = first["path"].as_str().unwrap().to_string();
    assert!(ask_path.ends_with("/ask.json"), "{ask_path}");
    assert_eq!(first["turns"][0]["sessionLines"], 3);
    assert_eq!(
        first["turns"][0]["images"],
        json!(["attach/a.png", "attach/b.jpg"])
    );
    let text = std::fs::read_to_string(vault.join(&ask_path)).unwrap();
    assert!(
        text.starts_with("{\n  \"v\": 1,\n  \"noteId\": \"ndfx0002-00000002\",\n  \"sessionId\": \"sess-1\",\n  \"turns\": [\n    {\n      \"id\": \"t1\",\n      \"markId\": \"m1\",\n      \"askedAt\": \"2026-11-14T10:00:00+09:00\",\n      \"quote\": \"인용\",")
            && text.ends_with("}\n"),
        "{text}"
    );
    let second = engine
        .call(
            "append_ask_turn",
            with_id(
                turn(json!({"turn-id": "t2", "session-id": "ignored", "error": ""})),
                parent,
                None,
            ),
        )
        .unwrap();
    assert_eq!(
        (
            second["sessionId"].as_str(),
            second["turns"].as_array().map(Vec::len)
        ),
        (Some("sess-1"), Some(2))
    );
    assert_eq!(second["turns"][1]["error"], "");
    let read = engine.call("get_ask", json!({"id": parent})).unwrap();
    assert!(
        read["turns"][1].get("error").is_none(),
        "읽을 때 빈 error는 없는 칸이다: {read}"
    );

    // 후속 노트는 자기 폴더에 따로 둔다
    let sub = engine
        .call(
            "append_ask_turn",
            with_id(turn(json!({})), parent, Some(child)),
        )
        .unwrap();
    assert_ne!(sub["path"], first["path"]);
    assert_eq!(sub["noteId"], child);

    // 인자 오류는 노트를 찾기 전에 낸다. 단일 파일 노트에는 둘 수 없다(AIS-500)
    for (args, message) in [
        (
            with_id(turn(json!({"model": "  "})), parent, None),
            "model 이 필요하다",
        ),
        (
            with_id(turn(json!({"elapsed-ms": -1})), parent, None),
            "elapsed-ms 은 숫자여야 한다: -1",
        ),
        (
            with_id(turn(json!({"images": "pic.png"})), parent, None),
            "그림 자리가 attach/<그림 파일 이름> 꼴이 아니다: pic.png",
        ),
        (
            with_id(turn(json!({"session-lines": 0})), parent, None),
            "session-lines 은 1 이상이어야 한다: 0",
        ),
    ] {
        assert_eq!(error("append_ask_turn", args).message, message);
    }
    let blocked = error("append_ask_turn", with_id(turn(json!({})), single, None));
    assert_eq!(blocked.kind, ErrorKind::Internal);
    assert!(
        blocked
            .message
            .starts_with("폴더로 안 올라간 단일 파일 노트에는 문답을 둘 수 없다: "),
        "{}",
        blocked.message
    );
    assert_eq!(
        error("get_ask", json!({"id": parent, "subId": " "})).kind,
        ErrorKind::InvalidArgument
    );
    assert_eq!(
        error("get_ask", json!({"id": "ndzz0001-00000001"})).kind,
        ErrorKind::NotFound
    );

    // 레일 대화: 만든 시각 순(읽을 수 없는 시각은 뒤), 같으면 대화 id 순. 못 읽는 파일은 이름만 알린다
    let chat = |extra: Value| {
        let mut base = json!({
            "chat-id": "chat-1", "source-session": "sess-x", "turn-id": "t1", "asked-at": "2026-11-14T10:00:00+09:00",
            "question": "q", "answer": "a", "model": "opus", "effort": "high", "elapsed-ms": 900
        });
        for (key, value) in extra.as_object().unwrap() {
            base[key] = value.clone();
        }
        base
    };
    let none = engine.call("list_chats", json!({"id": parent})).unwrap();
    assert_eq!(none, json!({"chats": [], "unreadable": []}));
    engine
        .call("append_chat_turn", with_id(chat(json!({})), parent, None))
        .unwrap();
    engine
        .call(
            "append_chat_turn",
            with_id(
                chat(json!({"turn-id": "t2", "source-session": "ignored"})),
                parent,
                None,
            ),
        )
        .unwrap();
    engine
        .call(
            "append_chat_turn",
            with_id(
                chat(json!({"chat-id": "chat-0", "asked-at": "2026-11-13T00:00:00Z"})),
                parent,
                None,
            ),
        )
        .unwrap();
    let chats_dir = vault
        .join(ask_path.trim_end_matches("ask.json"))
        .join("chats");
    std::fs::write(
        chats_dir.join("zz-bad-time.json"),
        r#"{"createdAt":"어제","turns":[]}"#,
    )
    .unwrap();
    std::fs::write(chats_dir.join("broken.json"), "{").unwrap();
    std::fs::write(chats_dir.join("bad name.json"), "{}").unwrap();
    std::fs::write(chats_dir.join("note.txt"), "{}").unwrap();
    let listed = engine.call("list_chats", json!({"id": parent})).unwrap();
    let ids: Vec<&str> = listed["chats"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["chatId"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["chat-0", "chat-1", "zz-bad-time"]);
    assert_eq!(listed["unreadable"], json!(["broken.json"]));
    assert_eq!(listed["chats"][1]["sourceSessionId"], "sess-x");
    assert_eq!(listed["chats"][1]["turns"].as_array().unwrap().len(), 2);
    assert_eq!(listed["chats"][2]["createdAt"], "어제");
    let appended = engine
        .call("append_chat_turn", with_id(chat(json!({"chat-id": "fresh", "images": "attach/a.png,, attach/b.gif ", "error": ""})), parent, Some(child)))
        .unwrap();
    assert_eq!(appended["chat"]["noteId"], child);
    assert_eq!(
        appended["chat"]["turns"][0]["images"],
        json!(["attach/a.png", "attach/b.gif"])
    );
    assert!(
        appended["path"]
            .as_str()
            .unwrap()
            .ends_with("/chats/fresh.json")
    );
    assert_eq!(
        error(
            "append_chat_turn",
            with_id(chat(json!({"chat-id": "broken"})), parent, None)
        )
        .message,
        "레일 대화 파일을 못 읽어 붙일 수 없다: Unterminated object"
    );
    assert_eq!(
        error(
            "append_chat_turn",
            with_id(chat(json!({"chat-id": "a b"})), parent, None)
        )
        .message,
        "대화 id 모양이 아니다: a b"
    );
    assert_eq!(
        error("list_chats", json!({"id": single})).kind,
        ErrorKind::Internal
    );
}

/// 세션 기록 시험이 함께 쓰는 홈. 세션 홈은 환경 변수로 정해서 프로세스 전체에 걸리므로, 세션 명령을 부르는 시험들이 한 홈을 나눠 쓴다.
fn session_home() -> &'static PathBuf {
    static HOME: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    HOME.get_or_init(|| {
        let home = std::env::temp_dir().join(format!("ais-session-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        // SAFETY: 이 환경 변수를 읽는 것은 세션 읽기 명령뿐이고, 세션 명령을 부르는 시험은 모두 이 홈을 쓴다.
        unsafe {
            std::env::set_var("AISURADA_CLAUDE_HOME", &home);
            std::env::set_var("AISURADA_CODEX_HOME", &home);
            std::env::set_var("AISURADA_GEMINI_HOME", &home);
        }
        home
    })
}

#[test]
fn reads_session_transcripts_of_every_agent() {
    let home = session_home();
    let write = |rel: &str, text: &str| {
        let path = home.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    let vault = fixture();
    let engine = Engine::new(&vault);
    let call = |command: &str, args: Value| engine.call(command, args);

    // Claude: 회차, 도구 짝짓기(초와 실패), 할 일, 돌고 있는 에이전트, 주소, 실행 상태
    let claude = "aaaaaaaa-1111-2222-3333-444444444444";
    let lines = [
        r#"{"type":"user","timestamp":"2026-11-14T01:00:00Z","cwd":"/w/p","gitBranch":"main","entrypoint":"cli","message":{"content":"질문 https://example.com/doc. 끝"}}"#,
        "깨진 줄",
        r#"{"type":"assistant","timestamp":"2026-11-14T01:00:01Z","effort":"high","message":{"model":"claude-opus","usage":{"input_tokens":1,"cache_read_input_tokens":2},"content":[{"type":"text","text":"답"},{"type":"tool_use","id":"t1","name":"TaskCreate","input":{}},{"type":"tool_use","id":"t2","name":"Agent","input":{"description":"하위 일"}},{"type":"tool_use","id":"t3","name":"Bash","input":{"command":"ls\n-la"}}]}}"#,
        r#"{"type":"user","timestamp":"2026-11-14T01:00:03.500Z","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"Task #1 created successfully: 할 일"},{"type":"tool_result","tool_use_id":"t3","content":"x","is_error":true}]}}"#,
    ];
    write(
        &format!(".claude/projects/-w-p/{claude}.jsonl"),
        &(lines.join("\n") + "\n"),
    );
    let read = call("read_session", json!({"id": claude, "urls": "true"})).unwrap();
    assert_eq!(
        (
            read["source"]["lines"].as_u64(),
            read["source"]["skipped"].as_u64(),
            read["source"]["messages"].as_u64()
        ),
        (Some(4), Some(1), Some(5))
    );
    assert_eq!(
        read["urls"],
        json!([{"url": "https://example.com/doc", "count": 1}])
    );
    assert_eq!(
        read["todos"],
        json!([{"number": 1, "title": "할 일", "status": "pending"}])
    );
    assert_eq!(
        read["runningAgents"],
        json!([{"what": "하위 일", "at": "2026-11-14T01:00:01Z"}])
    );
    assert_eq!(
        read["runState"],
        json!({"model": "claude-opus", "effort": "high", "contextTokens": 3, "quotaBlocked": false})
    );
    let bash = &read["turns"][4];
    assert_eq!(
        (
            bash["tool"]["name"].as_str(),
            bash["tool"]["arg"].as_str(),
            bash["tool"]["failed"].as_bool(),
            bash["tool"]["seconds"].as_f64()
        ),
        (Some("Bash"), Some("ls"), Some(true), Some(2.5))
    );
    assert_eq!(read["hasMore"], false);
    let paged = call("read_session", json!({"id": claude, "limit": 1})).unwrap();
    assert_eq!(
        paged["turns"].as_array().unwrap().len(),
        4,
        "끝에서 거꾸로 글 회차 1개와 그 뒤의 도구 회차들"
    );
    assert_eq!(paged["hasMore"], true);

    // 제목과 최근 세션(CLI 입구만, 파일 수정 시각이 7일 안)
    write(
        &format!(".claude/projects/-w-p/{claude}.jsonl"),
        &(lines.join("\n") + "\n{\"type\":\"ai-title\",\"aiTitle\":\"붙은 제목\"}\n"),
    );
    let titles = call(
        "session_titles",
        json!({"ids": [claude, "12345678-0000-0000-0000-000000000000"]}),
    )
    .unwrap();
    assert_eq!(
        titles["sessions"][0],
        json!({"sessionId": claude, "found": true, "title": "붙은 제목", "branch": "main", "cwd": "/w/p"})
    );
    assert_eq!(titles["sessions"][1]["found"], false);
    let recent = call("recent_sessions", json!({})).unwrap();
    assert_eq!(recent["sessions"][0]["sessionId"], claude);
    assert_eq!(recent["dropped"], 0);

    // Codex 새 형식, Gemini(되감기와 주입된 줄 제외), 없는 세션은 빈 응답
    let codex = "11111111-aaaa-bbbb-cccc-111111111111";
    let codex_lines = [
        r#"{"timestamp":"t","type":"session_meta","payload":{"cwd":"/w/codex"}}"#,
        r#"{"timestamp":"t1","type":"event_msg","payload":{"type":"item_completed","item":{"type":"UserMessage","content":[{"text":"코덱스"}]}}}"#,
        r#"{"timestamp":"t2","type":"event_msg","payload":{"type":"item_completed","item":{"type":"CommandExecution","command":["/bin/zsh","-lc","cargo test"],"exit_code":1,"duration":{"secs":2,"nanos":340000000}}}}"#,
    ];
    write(
        &format!(".codex/sessions/2026/11/14/rollout-2026-11-14T05-00-00-{codex}.jsonl"),
        &(codex_lines.join("\n") + "\n"),
    );
    let found = call("read_session", json!({"id": codex})).unwrap();
    assert_eq!(found["agent"], "codex");
    assert_eq!(
        found["turns"][1]["tool"],
        json!({"name": "shell", "arg": "cargo test", "seconds": 2.3, "failed": true})
    );
    let gemini = "33333333-aaaa-bbbb-cccc-333333333333";
    let gemini_lines = [
        format!(r#"{{"sessionId":"{gemini}"}}"#),
        r#"{"id":"a","type":"user","content":"<session_context>x</session_context>"}"#.to_string(),
        r#"{"id":"b","type":"user","content":"되감길 질문"}"#.to_string(),
        r#"{"$rewindTo":"b"}"#.to_string(),
        r#"{"id":"c","type":"user","content":"남는 질문"}"#.to_string(),
    ];
    write(
        &format!(
            ".gemini/tmp/projx/chats/session-2026-11-14T07-00-{}.jsonl",
            &gemini[..8]
        ),
        &(gemini_lines.join("\n") + "\n"),
    );
    write(
        ".gemini/projects.json",
        r#"{"projects":{"/w/gemini":"projx"}}"#,
    );
    let found = call("read_session", json!({"id": gemini})).unwrap();
    assert_eq!(
        (
            found["agent"].as_str(),
            found["source"]["messages"].as_u64(),
            found["turns"][0]["text"].as_str()
        ),
        (Some("gemini"), Some(1), Some("남는 질문"))
    );
    let titles = call("session_titles", json!({"ids": [gemini]})).unwrap();
    assert_eq!(
        titles["sessions"][0],
        json!({"sessionId": gemini, "found": true, "title": "남는 질문", "branch": null, "cwd": "/w/gemini", "agent": "gemini"})
    );
    let none = call(
        "read_session",
        json!({"id": "12345678-0000-0000-0000-000000000000", "note": "ndnone0000-0000"}),
    )
    .unwrap();
    assert_eq!(
        (
            none["file"].clone(),
            none["noteMentions"].clone(),
            none["turns"].clone()
        ),
        (Value::Null, json!([]), json!([]))
    );

    // 인자 오류
    for (args, message) in [
        (
            json!({}),
            "인자가 하나 필요하다: read-session <세션 id> [limit=<개수>] [before=<순번>] [urls=true] [note=<노트 id>] [author=<노트 id>,...]",
        ),
        (json!({"id": "nope"}), "세션 식별자 모양이 아니에요: nope"),
        (
            json!({"id": claude, "limit": "x"}),
            "limit 은 숫자여야 한다: x",
        ),
        (
            json!({"id": claude, "urls": "yes"}),
            "urls 은 true 나 false 여야 한다: yes",
        ),
        (
            json!({"id": claude, "note": "BAD"}),
            "노트 id 모양이 아니에요: BAD",
        ),
    ] {
        assert_eq!(call("read_session", args).unwrap_err().message, message);
    }
    assert_eq!(
        call("recent_sessions", json!({"days": 31}))
            .unwrap_err()
            .message,
        "days 는 1 부터 30 사이여야 해요: 31"
    );
}

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

/// 엔진이 Antigravity 기록을 읽을 때 쓰는 `sqlite3`의 자리(엔진과 같은 차례: 환경 변수, `/usr/bin/sqlite3`, `PATH`). 없으면 `None`이다.
fn sqlite3_binary() -> Option<String> {
    let custom = std::env::var("AISURADA_SQLITE_BIN").unwrap_or_default();
    if !custom.is_empty() {
        return Some(custom);
    }
    if Path::new("/usr/bin/sqlite3").is_file() {
        return Some("/usr/bin/sqlite3".to_string());
    }
    Command::new("sqlite3")
        .arg("--version")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|_| "sqlite3".to_string())
}

/// 합성 Antigravity 대화 기록(SQLite)을 만든다. 단계 8개(사용자 입력, 모델 답, 빈 입력, 도구 호출 셋, 시스템 알림, 빈 단계)와
/// 대화 메타(작업 폴더, 모델 이름, 화면 이름)를 넣는다. 같은 기록을 기준 Java 엔진도 읽어(`compat_check`) 응답이 같음을 확인했다.
fn make_antigravity_db(sqlite: &str, path: &Path, cwd: &str) {
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
    let rows = [
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
    let trajectory = pb_len(7, format!("file://{cwd}").as_bytes());
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
    let _ = std::fs::remove_file(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let status = Command::new(sqlite)
        .arg(path)
        .arg(sql)
        .status()
        .expect("sqlite3를 못 돌렸다");
    assert!(status.success(), "합성 Antigravity 기록을 못 만들었다");
}

/// Antigravity 대화는 SQLite 파일이라 시스템 `sqlite3`로 읽는다. 사용자 입력(14), 모델 답(15), 도구 호출(132)만 회차가 되고
/// 시스템 알림(101)과 빈 단계는 건너뛴다. 기대값은 같은 기록을 기준 Java 엔진이 읽은 응답(`golden/compat.ndjson`에 녹화)과 같다.
/// `sqlite3`가 없는 기기에서는 건너뛴다.
#[test]
fn reads_antigravity_conversations_through_sqlite3() {
    let Some(sqlite) = sqlite3_binary() else {
        eprintln!("sqlite3가 없어 Antigravity 시험을 건너뛴다");
        return;
    };
    let home = session_home();
    let agy = "44444444-aaaa-bbbb-cccc-444444444444";
    make_antigravity_db(
        &sqlite,
        &home.join(format!(".gemini/antigravity-cli/conversations/{agy}.db")),
        "/work/agy dir%20x",
    );
    let vault = fixture();
    let engine = Engine::new(&vault);

    let read = engine.call("read_session", json!({"id": agy})).unwrap();
    assert_eq!(
        (
            read["agent"].as_str(),
            read["client"].as_str(),
            read["sessionId"].as_str()
        ),
        (Some("gemini"), Some("antigravity"), Some(agy))
    );
    assert_eq!(read["hasMore"], false);
    assert_eq!(
        read["runState"],
        json!({"model": "gemini-3.8-flash-high", "quotaBlocked": false})
    );
    assert_eq!(read["source"]["lines"], 8);
    assert_eq!(read["source"]["messages"], 5);
    assert_eq!(read["source"]["skipped"], 0);
    assert_eq!(read["todos"], json!([]));
    assert_eq!(read["runningAgents"], json!([]));
    assert_eq!(read["urls"], json!([]));
    assert_eq!(
        read["turns"],
        json!([
            {"index": 0, "who": "user", "at": "2026-11-14T23:46:40.123Z", "chars": 38, "truncated": false,
             "text": "안티그래비티에게 묻는다 https://example.com/agy 끝"},
            {"index": 1, "who": "assistant", "at": "2026-11-14T23:46:45Z", "chars": 3, "truncated": false,
             "text": "답한다"},
            {"index": 2, "who": "tool", "at": "2026-11-14T23:46:50.000001Z", "chars": 0, "truncated": false, "text": "",
             "tool": {"name": "run_command", "arg": "목록을 본다"}},
            {"index": 3, "who": "tool", "at": "2026-11-14T23:46:50.000001Z", "chars": 0, "truncated": false, "text": "",
             "tool": {"name": "view_file", "arg": "/tmp/a.txt", "failed": true}},
            {"index": 4, "who": "tool", "at": "2026-11-14T23:46:50.000001Z", "chars": 0, "truncated": false, "text": "",
             "tool": {"name": "srv/search", "failed": true}},
        ])
    );

    // 주소를 모아 달라고 하면 사용자 입력의 주소가 센다
    let with_urls = engine
        .call("read_session", json!({"id": agy, "urls": "true"}))
        .unwrap();
    assert_eq!(
        with_urls["urls"],
        json!([{"count": 1, "url": "https://example.com/agy"}])
    );

    // 제목은 첫 사용자 입력이고, 에이전트는 gemini지만 `client`로 Gemini CLI와 가린다. 없는 대화는 찾지 못했다고 답한다
    let titles = engine
        .call(
            "session_titles",
            json!({"ids": [agy, "12345678-0000-0000-0000-000000000000"]}),
        )
        .unwrap();
    assert_eq!(
        titles["sessions"][0],
        json!({"sessionId": agy, "found": true, "agent": "gemini", "client": "antigravity",
               "title": "안티그래비티에게 묻는다 https://example.com/agy 끝", "branch": null, "cwd": null})
    );
    assert_eq!(titles["sessions"][1]["found"], false);
}

#[test]
fn runs_the_note_tool_commands() {
    isolate_writes();
    let vault = fixture();
    std::fs::write(vault.join(".aisurada/workspace.json"), "{}\n").unwrap();
    let engine = Engine::new(&vault);
    let error = |command: &str, args: Value| engine.call(command, args).unwrap_err();
    let parent = "ndfx0002-00000002";

    // 첨부: 글 확장자만, 같은 이름은 덮고, 이름 규칙은 처음 걸린 하나만 알린다
    let first = engine
        .call(
            "write_attachment",
            json!({"id": parent, "name": " 메모.MD ", "content": "본문"}),
        )
        .unwrap();
    assert_eq!(
        (first["name"].as_str(), first["parent"].as_str()),
        (Some("메모.MD"), Some(parent))
    );
    let path = first["path"].as_str().unwrap().to_string();
    assert_eq!(std::fs::read_to_string(vault.join(&path)).unwrap(), "본문");
    engine
        .call(
            "write_attachment",
            json!({"id": parent, "name": "메모.MD", "content": ""}),
        )
        .unwrap();
    assert_eq!(std::fs::read_to_string(vault.join(&path)).unwrap(), "");
    for (name, message) in [
        ("a/b.txt", "첨부 파일명에 경로 구분자가 있다: a/b.txt"),
        ("a..txt", "첨부 파일명에 상위 디렉터리 참조가 있다: a..txt"),
        (".a.txt", "첨부 파일명이 점으로 시작한다: .a.txt"),
        ("noext", "첨부 파일명에 확장자가 없다: noext"),
        (
            "a.png",
            "텍스트로 쓸 수 없는 확장자다(이미지 같은 바이너리는 attach/ 에 직접 둔다): a.png",
        ),
    ] {
        assert_eq!(
            error(
                "write_attachment",
                json!({"id": parent, "name": name, "content": "x"})
            )
            .message,
            message
        );
    }
    assert_eq!(
        error("write_attachment", json!({"id": parent, "name": "a.txt"}))
            .message
            .split(':')
            .next(),
        Some("content 이 필요하다")
    );

    // 사실층 요약과 내보내기와 데일리 플랜
    let brief = engine
        .call(
            "facts_brief",
            json!({"from": "2026-11-01", "to": "2026-11-30", "min-new-file": 1}),
        )
        .unwrap();
    assert!(brief["count"].as_u64().unwrap() >= 1, "{brief}");
    assert!(brief["section"].as_str().unwrap().starts_with("### "));
    assert_eq!(
        error(
            "facts_brief",
            json!({"from": "2026-12-01", "to": "2026-11-01"})
        )
        .message,
        "날짜 구간이 뒤집혔다: 2026-12-01 ~ 2026-11-01"
    );
    let exported = engine.call("export_markdown", json!({})).unwrap();
    assert!(exported["exported"].as_u64().unwrap() >= 1);
    assert!(vault.join(".aisurada/md-export").is_dir());
    let dry = engine
        .call(
            "plan_day",
            json!({"date": "2026-11-09", "agenda": "demo", "dryRun": true}),
        )
        .unwrap();
    assert_eq!(
        (dry["saved"].as_bool(), dry["date"].as_str()),
        (Some(false), Some("2026-11-09"))
    );
    assert!(
        dry["body"]
            .as_str()
            .unwrap()
            .starts_with("# 2026-11-09 데일리 플랜\n\n## 어제 한 일")
    );
    let saved = engine
        .call("plan_day", json!({"date": "2026-11-09", "agenda": "demo"}))
        .unwrap();
    assert_eq!(saved["saved"], true);
    assert!(vault.join(saved["path"].as_str().unwrap()).exists());

    // 샤드: 계획은 매핑 파일만 쓰고, 이동은 폴더를 옮기고, 검증은 어긋남을 알린다
    let plan = engine.call("shard_notes", json!({})).unwrap();
    assert_eq!(plan["applied"], false);
    assert!(vault.join(".aisurada/migration/shard-plan.tsv").exists());
    assert_eq!(error("verify_shard", json!({})).kind, ErrorKind::NotFound);
    assert_eq!(
        error("shard_notes", json!({"apply": "yes"})).message,
        "apply 은 true 나 false 여야 한다: yes"
    );
}
