//! `shard_notes`와 `verify_shard`(월별 폴더 옮기기)의 핵심 흐름. 지운 Java `Chapter28Test`(28장)의 시나리오를 Rust로 옮긴 것이다.
//! 골든 테이프에는 합성 vault에 던진 호출이 열세 건뿐이라(계획과 적용, 검증 어긋남, 되돌리기, 충돌로 멈춤이 없다) 따로 둔다.
//! TSV의 유니코드 숫자나 부호 같은 극단 입력은 옮기지 않았다(기준 엔진과의 맞대기가 본다).
//!
//! 이 파일은 시험이 하나뿐이고 환경 변수(월별 폴더 설정)를 시험 안에서 바꾼다.

use std::path::{Path, PathBuf};

use ais_core::engine::Engine;
use serde_json::{Value, json};

fn put(vault: &Path, relative: &str, text: &str) {
    let file = vault.join(relative);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

fn note(vault: &Path, path: &str, id: &str, agenda: &str, created: &str, body: &str) {
    let title = path.rsplit('/').next().unwrap();
    put(
        vault,
        path,
        &format!(
            "---\nid: {id}\nagenda: {agenda}\ntype: build\ntitle: {title}\ncreated: '{created}'\n---\n{body}"
        ),
    );
}

fn root(vault: &Path, name: &str, n: u32, created: &str, body: &str) {
    note(
        vault,
        &format!("notes/demo/{name}/note.md"),
        &format!("ndfixture-{n:08x}"),
        "demo",
        created,
        body,
    );
}

fn new_vault(base: &Path, name: &str) -> PathBuf {
    let vault = base.join(name);
    put(&vault, ".aisurada/workspace.json", "{}\n");
    put(
        &vault,
        ".aisurada/agendas/demo.md",
        "---\nid: agfixture-00000001\nslug: demo\ntitle: 데모\n---\n",
    );
    vault
}

fn sharded(value: bool) {
    // SAFETY: 이 파일의 유일한 시험이고 시험 스레드 하나에서만 바꾼다.
    unsafe {
        std::env::set_var(
            "AISURADA_NOTES_SHARD_BY_MONTH",
            if value { "true" } else { "false" },
        )
    };
}

fn call(vault: &Path, command: &str, args: Value) -> Value {
    Engine::new(vault)
        .call(command, args.clone())
        .unwrap_or_else(|e| panic!("{command} {args}: {e:?}"))
}

#[test]
fn shard_notes_plans_applies_verifies_and_unshards_like_the_java_engine() {
    let base: PathBuf = std::env::temp_dir().join(format!("ais-shard-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    // SAFETY: 아직 다른 스레드가 환경 변수를 읽기 전이다.
    unsafe {
        std::env::set_var("TZ", "Asia/Seoul");
        std::env::set_var("AISURADA_TEST_NOW", "1792559107123");
        std::env::set_var("AISURADA_LOCK_DIR", base.join("locks"));
        std::env::set_var("AISURADA_VAULT_AUTOCOMMIT_ENABLED", "false");
    }
    let v = new_vault(&base, "main");
    sharded(true);
    root(&v, "a", 1, "2026-06-16T10:00:00+09:00", "A");
    root(&v, "group/b", 2, "2026-06-17T23:00:00-10:00", "B");
    put(
        &v,
        "notes/demo/idless/note.md",
        "---\ncreated: '2026-06-18'\n---\nwithout id",
    );
    put(&v, "notes/demo/bad/note.md", "no frontmatter");
    put(&v, "notes/demo/a (conflicted copy).md", "conflict");
    std::os::unix::fs::symlink(base.join("absent"), v.join("notes/demo/link")).unwrap();
    note(
        &v,
        "notes/other/orphan/note.md",
        "ndfixture-00000003",
        "missing",
        "2026-06-19",
        "orphan agenda",
    );
    put(&v, "notes/demo/a/attach/plain.bin", "1234");

    // 계획: 디스크 후보는 일곱, 옮길 것은 넷, 못 옮기는 것은 셋. 계획만 세우고 옮기지 않는다
    let plan = call(&v, "shard_notes", json!({}));
    assert_eq!(
        plan["total"], 7,
        "아이디 없는 노트와 아젠다 없는 노트도 후보: {plan}"
    );
    assert_eq!(
        (plan["planned"].as_u64(), plan["moved"].as_u64()),
        (Some(4), Some(0))
    );
    assert_eq!(plan["unmovable"].as_array().unwrap().len(), 3);
    let mapping = plan["mappingFile"].as_str().unwrap().to_string();
    let tsv = std::fs::read_to_string(&mapping).unwrap();
    assert!(
        tsv.starts_with("noteId\toldRel\tnewRel\tfileCount\tbyteSum\treason\nndfixture-00000003\t"),
        "작은 아젠다가 먼저: {tsv}"
    );
    assert!(
        tsv.contains("\n\tnotes/demo/idless\tnotes/demo/2026-06/idless\t"),
        "아이디 없는 노드: {tsv}"
    );
    assert!(
        v.join("notes/demo/a/note.md").exists(),
        "계획만 세우면 안 옮긴다"
    );

    // 적용: 넷을 옮기고 비게 된 묶음 폴더를 정리한다. 첨부는 노트와 함께 간다
    let applied = call(&v, "shard_notes", json!({"apply": true}));
    assert_eq!(applied["moved"], 4, "{applied}");
    assert_eq!(applied["prunedEmptyDirs"], 1, "옛 묶음 부모 하나 정리");
    assert!(
        v.join("notes/demo/2026-06/a/attach/plain.bin").exists(),
        "첨부가 따라온다"
    );
    assert!(
        v.join("notes/other/2026-06/orphan/note.md").exists(),
        "색인에 없는 아젠다의 노트도 옮긴다"
    );

    // 검증: 기준이 맞고, 파일을 지우면 크기가 갈린 것으로 보이되 명령은 성공이다
    let verified = call(&v, "verify_shard", json!({}));
    assert_eq!(verified["ok"], true, "{verified}");
    assert_eq!(
        (verified["rows"].as_u64(), verified["checked"].as_u64()),
        (Some(7), Some(4))
    );
    std::fs::remove_file(v.join("notes/demo/2026-06/a/attach/plain.bin")).unwrap();
    let changed = call(&v, "verify_shard", json!({}));
    assert_eq!(changed["ok"], false, "크기가 달라지면 거짓: {changed}");

    // 한 번 더 적용하면 기준 파일이 둘이라 검증이 모호하다는 오류(409)다. 파일 이름만 쓰고 경로는 무시한다
    call(&v, "shard_notes", json!({"apply": true}));
    let ambiguous = Engine::new(&v).call("verify_shard", json!({})).unwrap_err();
    assert_eq!(ambiguous.code, "AIS-409", "{ambiguous:?}");
    assert!(
        ambiguous.message.contains("자리 바뀜 0"),
        "{}",
        ambiguous.message
    );
    let basename = Path::new(&mapping)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let by_basename = call(
        &v,
        "verify_shard",
        json!({"file": format!("../outside/{basename}/")}),
    );
    assert_eq!(
        by_basename["ok"], false,
        "경로를 떼고 이름만 쓴다: {by_basename}"
    );

    // 월별 폴더 설정을 끄고 다시 적용하면 평평하게 되돌린다
    sharded(false);
    let flat = call(&v, "shard_notes", json!({"apply": true}));
    assert_eq!(flat["moved"], 4, "넷을 되돌린다: {flat}");
    assert!(!v.join("notes/demo/2026-06").exists(), "빈 월 폴더 정리");
    assert!(
        v.join("notes/demo/a/note.md").exists(),
        "평평한 자리로 돌아온다"
    );
    // 워크스페이스 표식이 없어도 검증은 돈다. 틀린 apply 값은 400이다
    std::fs::remove_file(v.join(".aisurada/workspace.json")).unwrap();
    call(&v, "verify_shard", json!({"file": basename}));
    put(&v, ".aisurada/workspace.json", "{}\n");
    let bad = Engine::new(&v)
        .call("shard_notes", json!({"apply": "bad"}))
        .unwrap_err();
    assert_eq!(bad.code, "AIS-400", "{bad:?}");

    // 옮길 자리에 이미 노트가 있으면 앞의 것은 옮겨진 채 멈추고, 빈 부모 정리는 건너뛴다
    let collision = new_vault(&base, "collision");
    sharded(false);
    root(&collision, "g1/same", 1, "2026-06-16", "first");
    root(&collision, "g2/same", 2, "2026-06-17", "second");
    let failure = Engine::new(&collision)
        .call("shard_notes", json!({"apply": true}))
        .unwrap_err();
    assert_eq!(failure.code, "AIS-500", "{failure:?}");
    assert!(
        failure.message.contains("옮길 자리에"),
        "{}",
        failure.message
    );
    assert!(
        collision.join("notes/demo/same/note.md").exists(),
        "앞 후보는 옮겨진 채로"
    );
    assert!(
        collision.join("notes/demo/g2/same/note.md").exists(),
        "충돌한 후보는 그대로"
    );
    assert!(
        collision.join("notes/demo/g1").is_dir(),
        "오류가 나면 빈 부모 정리를 건너뛴다"
    );

    // 빈 vault: 계획도 머리는 쓰고, 기준 파일이 없는 검증은 404다
    let empty = new_vault(&base, "empty");
    assert_eq!(call(&empty, "shard_notes", json!({}))["total"], 0);
    let missing = Engine::new(&empty)
        .call("verify_shard", json!({}))
        .unwrap_err();
    assert_eq!(missing.code, "AIS-404", "{missing:?}");
    // 깊이 상한과 심볼릭 링크 사유
    std::fs::create_dir_all(empty.join(format!("notes/demo/{}", "deep/".repeat(9)))).unwrap();
    std::os::unix::fs::symlink(
        base.join("absent-reserved"),
        empty.join("notes/demo/attach"),
    )
    .unwrap();
    std::os::unix::fs::symlink(
        base.join("absent-conflict"),
        empty.join("notes/demo/conflicted copy"),
    )
    .unwrap();
    let boundaries = call(&empty, "shard_notes", json!({}));
    assert_eq!(
        boundaries["total"], 2,
        "예약 이름 링크는 뺀다: {boundaries}"
    );
    let reasons: Vec<&str> = boundaries["unmovable"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["reason"].as_str().unwrap())
        .collect();
    assert_eq!(
        reasons,
        ["심링크라 따라가지 않았다", "스캔 깊이 상한을 넘었다"]
    );

    let _ = std::fs::remove_dir_all(&base);
}
