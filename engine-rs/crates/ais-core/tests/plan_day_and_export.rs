//! `plan_day`(29장)와 `export_markdown`(27장). 지운 Java `Chapter29Test`와 `Chapter27Test`의 시나리오를 Rust로 옮긴 것이다.
//! 골든 테이프에는 합성 vault에 던진 호출 몇 건뿐이라(어제 고르기, 정렬, 날짜 경계, 내보내기 충돌과 심볼릭 링크 따위가 없다) 따로 둔다.
//!
//! 두 시험이 같은 환경 변수를 한 번만 정한다(서울 시간대, 시험용 시계, 자동 커밋 끔). 잠금 폴더는 시험마다 따로다.

use std::path::{Path, PathBuf};
use std::sync::Once;

use ais_core::engine::Engine;
use ais_core::error::ErrorKind;
use serde_json::{Value, json};

fn setup() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // SAFETY: 모든 시험이 이것을 거친 뒤에만 엔진을 부른다.
        unsafe {
            std::env::set_var("TZ", "Asia/Seoul");
            std::env::set_var("AISURADA_TEST_NOW", "1792559107123"); // 2026-10-21T05:05:07.123Z
            std::env::set_var(
                "AISURADA_LOCK_DIR",
                std::env::temp_dir().join(format!("ais-plan-export-locks-{}", std::process::id())),
            );
            std::env::set_var("AISURADA_VAULT_AUTOCOMMIT_ENABLED", "false");
        }
    });
}

fn put(vault: &Path, relative: &str, text: &str) {
    let file = vault.join(relative);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

fn note(vault: &Path, path: &str, id: &str, agenda: &str, created: &str, extra: &str, body: &str) {
    let title = path.rsplit('/').next().unwrap();
    let head = format!(
        "id: {id}\nagenda: {agenda}\ntype: build\ntitle: {title}\ncreated: '{created}'\n{extra}"
    );
    let text = if path.ends_with(".html") {
        format!("<!--aisurada\n{head}-->\n{body}")
    } else {
        format!("---\n{head}---\n{body}")
    };
    put(vault, path, &text);
}

fn root(vault: &Path, name: &str, n: u32, created: &str, extra: &str, body: &str) {
    note(
        vault,
        &format!("notes/demo/{name}/note.md"),
        &format!("ndfixture-{n:08x}"),
        "demo",
        created,
        extra,
        body,
    );
}

fn agenda(vault: &Path, slug: &str, id: &str, title: &str) {
    put(
        vault,
        &format!(".aisurada/agendas/{slug}.md"),
        &format!("---\nid: {id}\nslug: {slug}\ntitle: {title}\n---\n"),
    );
}

fn new_vault(name: &str) -> PathBuf {
    let base = std::env::temp_dir().join(format!("ais-plan-export-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    put(&base, ".aisurada/workspace.json", "{}\n");
    agenda(&base, "demo", "agfixture-00000001", "데모");
    base
}

fn ok(vault: &Path, command: &str, args: Value) -> Value {
    Engine::new(vault)
        .call(command, args.clone())
        .unwrap_or_else(|e| panic!("{command} {args}: {e:?}"))
}

fn error_400(vault: &Path, command: &str, args: Value) {
    let error = Engine::new(vault).call(command, args.clone()).unwrap_err();
    assert_eq!(
        error.kind,
        ErrorKind::InvalidArgument,
        "{command} {args}: {error:?}"
    );
}

#[test]
fn plan_day_drafts_and_saves_the_daily_plan_like_the_java_engine() {
    setup();
    let v = new_vault("plan");
    let empty = ok(&v, "plan_day", json!({"date": "2026-06-22"}));
    assert_eq!(empty["saved"], false);
    assert_eq!(
        empty["body"],
        "# 2026-06-22 데일리 플랜\n\n## 어제 한 일\n- (기록 없음)\n\n## 오늘\n- (아직 예정 없음)\n"
    );

    agenda(&v, "alpha", "agfixture-00000002", "알파");
    root(
        &v,
        "friday",
        1,
        "2026-06-19T09:00:00+09:00",
        "title: 데모 어제\n",
        "",
    );
    note(
        &v,
        "notes/alpha/friday/note.md",
        "ndfixture-00000002",
        "alpha",
        "2026-06-19T16:00:00Z",
        "title: 알파 어제\n",
        "",
    );
    root(
        &v,
        "today",
        3,
        "2026-06-22T09:00:00+09:00",
        "title: 데모 오늘\n",
        "",
    );
    note(
        &v,
        "notes/alpha/today/note.html",
        "ndfixture-00000004",
        "alpha",
        "2026-06-22T10:00:00Z",
        "title: 알파 오늘\n",
        "<p>html</p>",
    );
    root(
        &v,
        "record",
        5,
        "2026-06-21",
        "tier: record\ntitle: 기록 제외\n",
        "",
    );
    note(
        &v,
        "notes/demo/friday/sub/child/note.md",
        "snfixture-00000001",
        "demo",
        "2026-06-20",
        "title: 하위 제외\n",
        "",
    );
    let expected = "# 2026-06-22 데일리 플랜\n\n## 어제 한 일 (2026-06-19)\n- 알파 어제 · 알파 (build)\n- 데모 어제 · 데모 (build)\n\n## 오늘\n- 데모 오늘 · 데모 (build)\n- 알파 오늘 · 알파 (build)\n";

    // 주말 건너뛰기(월요일의 어제는 금요일), 아젠다별 정렬. 시험 삼아 돌리면(dryRun) 아젠다를 만들지 않는다
    let draft = ok(
        &v,
        "plan_day",
        json!({"date": "2026-06-22", "agenda": "존재 안 함", "dryRun": true}),
    );
    assert_eq!(
        draft["body"], expected,
        "기록 층, 후속 노트, 단일 HTML이 아닌 읽기 층 노트만"
    );
    assert_eq!(draft["saved"], false);
    assert!(
        !v.join(".aisurada/agendas/존재-안-함.md").exists(),
        "dryRun은 아젠다를 안 만든다"
    );

    // 저장: 초안을 먼저 만든 다음 자기 자신을 넣는다. AI 도구의 세션은 계획 노트에 안 남긴다
    let saved = ok(
        &v,
        "plan_day",
        json!({"date": "2026-06-22", "agenda": "데모", "caller": "ai", "session": "plan-session"}),
    );
    let keys: std::collections::BTreeSet<&str> = saved
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        ["agenda", "agendaCreated", "body", "id", "path", "saved"]
            .into_iter()
            .collect(),
        "저장 응답은 날짜를 안 싣는다"
    );
    assert_eq!(
        saved["body"], expected,
        "저장 본문은 자기 자신을 넣기 전의 초안"
    );
    assert_eq!(saved["agenda"], "demo", "제목으로 찾은 아젠다");
    let file =
        std::fs::read_to_string(v.join(saved["path"].as_str().unwrap()).join("note.md")).unwrap();
    assert!(
        file.contains("type: plan\n")
            && file.contains("created: '2026-06-22T09:00:00+09:00'\n")
            && !file.contains("mins:"),
        "고정된 계획 머리: {file}"
    );
    assert!(
        !file.contains("sessionId:"),
        "계획 도구의 세션은 일부러 안 남긴다: {file}"
    );
    assert!(file.ends_with(expected), "파일 본문이 응답과 같다");

    // 같은 날 다시 돌리면 앞서 저장한 계획이 다음 초안에 들어가고, 같은 제목 경고가 응답 밖(경고 칸)으로 나온다
    let second = ok(
        &v,
        "plan_day",
        json!({"date": "2026-06-22", "agenda": "demo", "dryRun": false}),
    );
    assert!(
        second["body"]
            .as_str()
            .unwrap()
            .contains("데일리 플랜 · 데모 (plan)"),
        "{second}"
    );
    // 없는 아젠다는 같은 길로 만든다
    let created = ok(
        &v,
        "plan_day",
        json!({"date": "2026-06-22", "agenda": "새 작업"}),
    );
    assert_eq!(created["agendaCreated"], true);
    // 연도 경계: 0000년(윤년)은 받는다. 다른 점: Java는 java.time의 확장 연도(`-0001-12-31`, `+10000-01-01`)도 받았지만 Rust는 네 자리 연도만 받는다.
    // 마이너스 연도나 다섯 자리 연도의 하루 계획은 쓸 곳이 없어서 옮기지 않았다.
    ok(&v, "plan_day", json!({"date": "0000-02-29"}));
    for date in ["-0001-12-31", "+10000-01-01"] {
        error_400(&v, "plan_day", json!({"date": date}));
    }
    // 날짜 모양과 dryRun 값 검사(전각 숫자, 부호, 자리수, 대문자 TRUE)
    for date in [
        "2026-02-30",
        "2026-6-22",
        "２０２６-０６-２２",
        "+2026-06-22",
    ] {
        error_400(&v, "plan_day", json!({"date": date, "dryRun": "bad"}));
    }
    error_400(
        &v,
        "plan_day",
        json!({"date": "2026-06-22", "dryRun": "TRUE"}),
    );
    assert_eq!(
        ok(
            &v,
            "plan_day",
            json!({"date": "2026-06-22", "agenda": "demo", "dryRun": ""})
        )["saved"],
        true,
        "빈 dryRun은 거짓"
    );

    // 노트가 백 건을 넘어도 잘라 내지 않는다
    let many = new_vault("plan-many");
    for i in 1..=101 {
        root(
            &many,
            &format!("n{i}"),
            i,
            "2026-06-22T10:00:00+09:00",
            &format!("title: item{i}\n"),
            "",
        );
    }
    let body = ok(&many, "plan_day", json!({"date": "2026-06-22"}))["body"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(
        body.lines().filter(|l| l.starts_with("- item")).count(),
        101,
        "100건에서 안 잘린다"
    );

    // 워크스페이스 표식이 날짜 검사보다 앞선다
    std::fs::remove_file(v.join(".aisurada/workspace.json")).unwrap();
    let error = Engine::new(&v)
        .call("plan_day", json!({"date": "bad"}))
        .unwrap_err();
    assert_eq!(error.kind, ErrorKind::InvalidArgument);
    assert!(error.message.contains("워크스페이스"), "{}", error.message);
    let _ = std::fs::remove_dir_all(&v);
    let _ = std::fs::remove_dir_all(&many);
}

#[test]
fn export_markdown_writes_reading_notes_and_replaces_the_destination_like_the_java_engine() {
    setup();
    let v = new_vault("export");
    root(
        &v,
        "hand-renamed",
        1,
        "2026-06-16",
        "",
        "# exact\r\n\r\nattach/x #sub:id <!--aisc:c-->\n",
    );
    note(
        &v,
        "notes/demo/old.md",
        "ndfixture-00000002",
        "demo",
        "2026-06-17",
        "",
        "no final newline",
    );
    note(
        &v,
        "notes/demo/html/note.html",
        "ndfixture-00000003",
        "demo",
        "2026-06-18",
        "",
        "<p>html</p>",
    );
    note(
        &v,
        "notes/demo/json/note.json",
        "ndfixture-00000004",
        "demo",
        "2026-06-19",
        "",
        "{}",
    );
    root(&v, "record", 5, "2026-06-20", "tier: record\n", "secret");
    root(&v, "a..b", 6, "2026-06-21", "", "unsafe");
    root(&v, "folder.md", 7, "2026-06-22", "", "hidden by suffix bug");
    note(
        &v,
        "notes/demo/hand-renamed/sub/child/note.md",
        "snfixture-00000001",
        "demo",
        "2026-06-16",
        "",
        "child",
    );
    put(&v, ".aisurada/md-export/stale.md", "stale");

    let response = ok(&v, "export_markdown", json!({}));
    assert_eq!(response["totalNotes"], 6, "읽기 층 메인 노트만");
    assert_eq!(response["exported"], 3);
    assert_eq!(
        (
            response["skippedUnsafe"].as_u64(),
            response["skippedHtml"].as_u64(),
            response["skippedJson"].as_u64()
        ),
        (Some(1), Some(1), Some(1))
    );
    let dest = v.join(".aisurada/md-export");
    assert_eq!(
        std::fs::read_to_string(dest.join("demo/hand-renamed.md")).unwrap(),
        "# exact\r\n\r\nattach/x #sub:id <!--aisc:c-->\n",
        "본문 그대로(엔진 참조 표식 포함)"
    );
    assert_eq!(
        std::fs::read_to_string(dest.join("demo/old.md")).unwrap(),
        "no final newline",
        "줄바꿈을 더하지 않는다"
    );
    assert_eq!(
        std::fs::metadata(dest.join("demo/folder.md"))
            .unwrap()
            .len(),
        0,
        "확장자 폴더 호환: 빈 파일이 된다"
    );
    assert!(!dest.join("stale.md").exists(), "앞의 내보내기는 지운다");

    // 같은 자리로 가는 둘은 둘 다 세고 나중에 만든 것이 이긴다
    root(&v, "x", 8, "2026-06-23", "", "first");
    note(
        &v,
        "notes/demo/x.md",
        "ndfixture-00000009",
        "demo",
        "2026-06-24",
        "",
        "second",
    );
    let again = ok(&v, "export_markdown", json!({}));
    assert_eq!(again["exported"], 5, "충돌한 쓰기도 둘 다 센다");
    assert_eq!(
        std::fs::read_to_string(dest.join("demo/x.md")).unwrap(),
        "second",
        "만든 시각이 늦은 쪽이 이긴다"
    );

    // 내보내기 폴더가 심볼릭 링크면 링크를 폴더로 바꾸고 가리키던 곳은 건드리지 않는다
    let outside = v
        .parent()
        .unwrap()
        .join(format!("ais-plan-export-{}-outside", std::process::id()));
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("keep.md"), "keep").unwrap();
    std::fs::remove_dir_all(&dest).unwrap();
    std::os::unix::fs::symlink(&outside, &dest).unwrap();
    ok(&v, "export_markdown", json!({}));
    assert!(
        !std::fs::symlink_metadata(&dest)
            .unwrap()
            .file_type()
            .is_symlink(),
        "링크를 폴더로 바꾼다"
    );
    assert_eq!(
        std::fs::read_to_string(outside.join("keep.md")).unwrap(),
        "keep",
        "가리키던 곳은 그대로"
    );
    let _ = std::fs::remove_dir_all(&v);
    let _ = std::fs::remove_dir_all(&outside);
}
