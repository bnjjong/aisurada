//! 합성 vault(`tools/compat/make-fixture.sh`)로 읽기 명령의 핵심 동작을 확인합니다. Java 엔진 없이 돕니다.
//! 같은 vault를 Java 엔진과 맞대는 시험은 `examples/compat_check.rs`입니다.

use std::path::{Path, PathBuf};
use std::process::Command;

use ais_core::engine::Engine;
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
        (unknown.code, serde_json::to_value(unknown.kind).unwrap()),
        ("AIS-404", json!("not_found"))
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
        serde_json::to_value(engine.call("update_doc", json!({})).unwrap_err().kind).unwrap(),
        json!("unsupported")
    );
    assert!(
        Engine::supports("list_docs")
            && Engine::supports("get_doc")
            && !Engine::supports("update_doc")
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
        (refused.code, serde_json::to_value(refused.kind).unwrap()),
        ("AIS-409", json!("forbidden"))
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
    assert_eq!(wrong.code, "AIS-404");
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
            (err.code, err.message.as_str()),
            (
                "AIS-400",
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
        (err.code, err.message.as_str()),
        ("AIS-400", "태그 개수 상한은 1 이상이다: 0")
    );

    // 템플릿 하나: 본문까지. 모양이 틀리면 400, 없으면 404
    let template = engine
        .call("get_template", json!({"id": "plan-basic"}))
        .unwrap();
    assert_eq!(
        (template["name"].as_str(), template["body"].as_str()),
        (Some("기본 계획"), Some("# 계획\n\n내용\n"))
    );
    for (id, code) in [
        ("a b", "AIS-400"),
        ("../x", "AIS-400"),
        ("zzz-none", "AIS-404"),
        ("", "AIS-400"),
    ] {
        let err = engine.call("get_template", json!({"id": id})).unwrap_err();
        assert_eq!(err.code, code, "{id:?}: {}", err.message);
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
        (out_of_range.code, out_of_range.message.as_str()),
        ("AIS-409", "판 번호가 범위 밖이다(1 부터 6 까지 있다): 7")
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
    assert_eq!(error.code, "AIS-500");
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
                error.code,
                error
                    .message
                    .starts_with("선결 조건 토큰이 안 왔어요. 아무것도 쓰지 않았습니다: notes/")
            ),
            ("AIS-412", true)
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
    assert_eq!(gone.code, "AIS-500");
    assert!(
        gone.message
            .starts_with("노트 본문 파일을 못 읽어 메타를 저장하지 않았다: notes/"),
        "{}",
        gone.message
    );

    // 워크스페이스 표식: 합성 vault에는 없고, 만들면 통과한다
    let error = marker::check_workspace_marker(&vault).unwrap_err();
    assert_eq!(error.code, "AIS-400");
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
    assert_eq!(no_marker.code, "AIS-400");
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
    assert_eq!(error("set_favorite", json!({"id": parent})).code, "AIS-412");
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
        error("evaluate_doc", json!({"id": parent, "f1": token(parent)})).code,
        "AIS-400"
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
