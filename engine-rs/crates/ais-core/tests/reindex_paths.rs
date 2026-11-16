//! `reindex_paths`의 선택 범위와 진단. 지운 Java `ReindexPathsTest`(8.5/8.6)의 시나리오를 Rust로 옮긴 것이다.
//!
//! 골든 테이프에는 합성 vault에 던진 경로 16건뿐이라(심볼릭 링크, 치명적 YAML, NUL 경로, 월별 폴더, 경로 200개 경계가 없다) 따로 둔다.
//! 이 파일은 시험이 하나뿐이고 환경 변수를 한 번만 정한다(월별 폴더 배치 설정).

use std::path::{Path, PathBuf};

use ais_core::engine::Engine;
use serde_json::{Value, json};

fn root() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ais-reindex-paths-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(vault: &Path, path: &str, text: &str) {
    let file = vault.join(path);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

fn note(vault: &Path, path: &str, id: &str) {
    write(
        vault,
        path,
        &format!(
            "---\nid: {id}\nagenda: demo\ntitle: 검사\ntype: free\ncreated: '2026-06-16T10:00:00+09:00'\n---\n본문\n"
        ),
    );
}

/// 아젠다 하나와 노트 셋(`a`, `folder/b`, `folder/c`)이 든 vault
fn vault(base: &Path, name: &str) -> PathBuf {
    let vault = base.join(name);
    write(
        &vault,
        ".aisurada/agendas/demo.md",
        "---\nid: agfixture-00000001\nslug: demo\ntitle: 데모\n---\n",
    );
    note(&vault, "notes/demo/a/note.md", "ndfixture-00000001");
    note(&vault, "notes/demo/folder/b/note.md", "ndfixture-00000002");
    note(&vault, "notes/demo/folder/c/note.md", "ndfixture-00000003");
    vault
}

/// `reindex_paths`를 부르고 응답의 모양(상태가 없는 훑기)을 확인해 돌려준다.
fn index(vault: &Path, paths: &[&str]) -> Value {
    let result = Engine::new(vault)
        .call("reindex_paths", json!({"paths": paths}))
        .unwrap_or_else(|e| panic!("{paths:?}: {e:?}"));
    let keys: Vec<&str> = result
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        [
            "agendas",
            "conflicts",
            "elapsedMs",
            "indexed",
            "mode",
            "removed",
            "scanned",
            "skipped",
            "unchanged",
            "unsharded"
        ],
        "응답 키"
    );
    assert_eq!(
        result["scanned"], result["indexed"],
        "상태가 없으니 훑은 수와 색인한 수가 같다"
    );
    assert_eq!(
        (result["unchanged"].as_u64(), result["removed"].as_u64()),
        (Some(0), Some(0))
    );
    assert!(result["elapsedMs"].as_i64().unwrap() >= 0);
    result
}

fn indexed(vault: &Path, paths: &[&str]) -> u64 {
    index(vault, paths)["indexed"].as_u64().unwrap()
}

fn error_of(vault: &Path, paths: &[&str]) -> ais_core::error::EngineError {
    Engine::new(vault)
        .call("reindex_paths", json!({"paths": paths}))
        .unwrap_err()
}

#[test]
fn reindex_paths_selects_notes_and_reports_diagnostics_like_the_java_engine() {
    let base = root();

    // ---- 경로 고르기 ----
    let v = vault(&base, "paths");
    assert_eq!(indexed(&v, &[]), 0, "경로가 없으면");
    assert_eq!(
        indexed(&v, &["notes", "notes/", "NOTES/demo", "elsewhere", " "]),
        0,
        "노트 폴더 밖이나 아젠다 폴더 위는 무시"
    );
    assert_eq!(
        indexed(&v, &["\u{a0}//notes/demo/a/note.md///\u{a0}"]),
        1,
        "앞뒤 공백과 바깥 슬래시를 뗀다"
    );
    assert_eq!(
        indexed(
            &v,
            &[
                "notes/demo/a/sub/missing/note.md",
                "notes/demo/a/attach/absent.png",
                "notes/demo/a"
            ]
        ),
        1,
        "한 노트 아래 경로는 그 노트 하나로 접고 겹치면 한 번만"
    );
    assert_eq!(
        indexed(&v, &["notes/demo/folder"]),
        2,
        "일반 폴더 아래 노트"
    );
    assert_eq!(
        indexed(&v, &["notes/demo/folder/missing/note.md"]),
        0,
        "중간이 없으면 위로 안 올라간다"
    );
    assert_eq!(
        indexed(&v, &["notes/demo/a/missing/.."]),
        1,
        "나머지 조각 전에 노트 뿌리에서 멈춘다"
    );
    assert_eq!(
        indexed(&v, &["notes/demo/ /folder//b/note.md"]),
        1,
        "빈 조각은 버린다"
    );
    assert_eq!(indexed(&v, &["notes/demo"]), 3, "아젠다 폴더");
    assert_eq!(
        index(&v, &["notes/.."])["mode"],
        "paths",
        "둘째 조각이 부모면 받아들인다"
    );

    // 아젠다 폴더로 가는 링크는 따라가고, 노트 폴더 안의 링크는 건너뛴다
    std::os::unix::fs::symlink(v.join("notes/demo"), v.join("notes/alias")).unwrap();
    assert_eq!(
        indexed(&v, &["notes/alias/a"]),
        1,
        "아젠다 심볼릭 링크는 따라간다"
    );
    std::os::unix::fs::symlink(v.join("notes/demo/a"), v.join("notes/demo/link")).unwrap();
    let link = index(&v, &["notes/demo/link/note.md"]);
    assert_eq!(link["indexed"], 0, "중첩 링크는 건너뛴다");
    assert!(
        link["skipped"].to_string().contains("심링크"),
        "심링크 진단: {}",
        link["skipped"]
    );

    let empty = base.join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    assert_eq!(
        index(&empty, &["notes/demo"])["agendas"],
        0,
        "빈 vault는 워크스페이스 표식이 필요 없다"
    );

    // ---- 진단 ----
    let v = vault(&base, "diagnostics");
    write(&v, "notes/demo/bad.md", "no header");
    std::os::unix::fs::symlink(v.join("notes/demo/a"), v.join("notes/demo/z-link")).unwrap();
    note(
        &v,
        "notes/demo/conflicted copy/note.md",
        "ndfixture-00000004",
    );
    let result = index(&v, &["notes/demo"]);
    let skipped = result["skipped"].to_string();
    assert!(
        skipped.find("z-link").unwrap() < skipped.find("bad.md").unwrap(),
        "수집 진단이 열기 진단보다 앞: {skipped}"
    );
    assert!(
        result["conflicts"].to_string().contains("conflicted copy"),
        "충돌 사본"
    );

    // 읽을 수 없는 머리: 고르지 않은 노트는 열지 않는다. 고른 노트는 진단에 올리고 명령은 계속한다.
    // 다른 점: Java는 `mins: !!int abc` 같은 머리에서 처리되지 않은 NumberFormatException으로 명령 전체가 죽었다(기준 엔진으로 확인했다,
    // 종료 코드 1). Rust는 그 노트만 건너뛰고 나머지를 훑는다. JVM의 우연한 예외라서 옮기지 않았다.
    write(
        &v,
        "notes/demo/poison/note.md",
        "---\nmins: !!int abc\n---\n",
    );
    assert_eq!(
        indexed(&v, &["notes/demo/a"]),
        1,
        "고르지 않은 노트는 안 연다"
    );
    let poisoned = index(&v, &["notes/demo/poison"]);
    assert_eq!(poisoned["indexed"], 0);
    assert!(
        poisoned["skipped"]
            .to_string()
            .contains("notes/demo/poison"),
        "고른 노트는 진단에 오른다: {}",
        poisoned["skipped"]
    );
    // 경로 검사가 노트 열기보다 앞선다
    let escape = error_of(&v, &["notes/demo/poison", "notes/demo/../escape"]);
    assert_eq!(escape.code, "AIS-400", "{escape:?}");
    assert!(
        escape.message.contains("기준 폴더 밖"),
        "{}",
        escape.message
    );
    assert_eq!(
        error_of(&v, &["notes/demo/\u{0}"]).code,
        "AIS-400",
        "NUL 경로"
    );
    std::fs::remove_file(v.join("notes/demo/poison/note.md")).unwrap();

    // 월별 폴더 배치를 켜면 월 폴더 밖 후보는 `unsharded`에 오른다
    // SAFETY: 이 파일의 유일한 시험이고, 다른 스레드가 환경 변수를 읽기 전에 정한다.
    unsafe { std::env::set_var("AISURADA_NOTES_SHARD_BY_MONTH", "true") };
    let result = index(&v, &["notes/demo/a", "notes/demo/bad.md"]);
    assert_eq!(
        result["unsharded"],
        json!(["notes/demo/a", "notes/demo/bad.md"]),
        "{result}"
    );
    note(&v, "notes/demo/2026-10/month/note.md", "ndfixture-00000005");
    assert_eq!(
        index(&v, &["notes/demo/2026-10/month"])["unsharded"],
        json!([]),
        "월 폴더 경로는 받아들인다"
    );
    unsafe { std::env::remove_var("AISURADA_NOTES_SHARD_BY_MONTH") };

    // ---- 경계: 경로 200개까지는 경로 방식, 201개부터는 전체를 훑는다 ----
    let v = vault(&base, "boundaries");
    let blanks = vec![""; 200];
    assert_eq!(index(&v, &blanks)["mode"], "paths", "원문 200개");
    let mut over = vec![""; 201];
    over[0] = "notes/demo/../bad";
    let full = index(&v, &over);
    assert_eq!(
        full["mode"], "full",
        "201개부터는 경로 검사를 건너뛰고 전체를 훑는다"
    );
    assert_eq!(full["indexed"], 3, "전체를 훑으면 노트 셋");
    write(&v, ".aisurada/templates/doc/broken.md", "no header");
    assert_eq!(
        index(&v, &over)["skipped"],
        json!([]),
        "경로 방식과 그 대체(전체)는 템플릿 진단을 싣지 않는다"
    );
    let reindexed = Engine::new(&v).call("reindex", Value::Null).unwrap();
    assert_ne!(
        reindexed["skipped"],
        json!([]),
        "전체 reindex는 템플릿 진단을 싣는다"
    );

    let _ = std::fs::remove_dir_all(&base);
}
