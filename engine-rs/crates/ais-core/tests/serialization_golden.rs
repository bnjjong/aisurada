//! 직렬화 골든 시험: Rust 엔진이 쓰는 파일의 **바이트**를 그대로 못박는다.
//!
//! 맞대기(`compat_check`)는 승인 대기 변경 파일을 Rust에서만 만들어 Java 쪽에 복사해 넣으므로(`examples/compat_check.rs`) 칸 차례가 틀려도
//! 못 잡는다. 이 시험은 태스크(`TSK-n.md`, `activity.jsonl`), 자원 기록장(`ledger.json`), 승인 대기 변경 파일, 휴지통 목록(`trash.json`)을
//! 시계와 난수를 고정한 한 시나리오로 만들어, `tests/golden/serialization/`에 둔 정답과 바이트까지 견준다.
//! 정답을 새로 쓸 때는 `UPDATE_GOLDEN=1 cargo test -p ais-core --test serialization_golden`을 돌리고 `git diff`로 리뷰한다.
//!
//! 이 파일은 시험이 하나뿐이라 환경 변수(시계, 난수, 시간대)를 바꿔도 다른 시험에 새지 않는다.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ais_core::engine::Engine;
use serde_json::{Value, json};

const NOW: &str = "1794700000000";

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/serialization")
}

fn make_fixture() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ais-serial-golden-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/compat/make-fixture.sh");
    let out = Command::new("bash")
        .arg(script)
        .arg(&dir)
        .output()
        .expect("bash를 못 돌렸다");
    assert!(
        out.status.success(),
        "합성 vault를 못 만들었다: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // 쓰기 명령은 워크스페이스 표식이 있어야 일한다
    fs::write(dir.join(".aisurada/workspace.json"), "{}\n").unwrap();
    dir
}

/// 폴더 안 파일을 (상대 경로 → 글)로 모은다. `prefix` 아래만 본다.
fn collect(root: &Path, rel_dir: &str, out: &mut BTreeMap<String, String>, tag: &str) {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>, tag: &str) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out, tag);
            } else {
                let rel = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .to_string();
                out.insert(
                    format!("{tag}/{rel}"),
                    String::from_utf8_lossy(&fs::read(&path).unwrap()).into_owned(),
                );
            }
        }
    }
    walk(root, &root.join(rel_dir), out, tag);
}

#[test]
fn serialized_files_keep_their_exact_bytes() {
    let locks = std::env::temp_dir().join(format!("ais-serial-locks-{}", std::process::id()));
    let approvals =
        std::env::temp_dir().join(format!("ais-serial-approvals-{}", std::process::id()));
    // SAFETY: 이 파일의 유일한 시험이고, 엔진을 부르기 전에 한 번만 바꾼다.
    unsafe {
        std::env::set_var("TZ", "Asia/Seoul");
        std::env::set_var("AISURADA_TEST_NOW", NOW);
        std::env::set_var("AISURADA_LOCK_DIR", &locks);
        std::env::set_var("AISURADA_VAULT_AUTOCOMMIT_ENABLED", "false");
        std::env::set_var("AISURADA_APPROVALS_DIR", &approvals);
    }
    let vault = make_fixture();
    let engine = Engine::new(&vault);

    // 명령마다 난수 목록을 다시 먹인다(맞대기 하네스와 같은 방식). 순서가 곧 id다.
    let mut step = 0u32;
    let mut call = |command: &str, args: Value| -> Value {
        step += 1;
        let random = format!("{step:04x}0001,{step:04x}0002,{step:04x}0003,{step:04x}0004");
        // SAFETY: 단일 스레드에서 호출 직전에만 바꾼다.
        unsafe {
            std::env::set_var("AISURADA_TEST_RANDOM", random);
        }
        ais_core::write::apparatus::reset_random_queue();
        engine
            .call(command, args)
            .unwrap_or_else(|e| panic!("{command}: {} {}", e.code, e.message))
    };

    // 태스크: 만들기, 고치기(확인 값), 댓글, 상태 바뀜, 둘째 만들고 연결
    let first = call(
        "create_task",
        json!({"title": "첫 태스크", "repo": "owner/name", "criteria": ["하나", {"text": "둘", "done": true}], "labels": ["b", "a"], "type": "BUG"}),
    );
    let key = first["key"].as_str().unwrap().to_string();
    let token = first["precondition"].as_str().unwrap().to_string();
    call(
        "update_task",
        json!({"id": key, "title": "첫 태스크 고침", "priority": "high", "f1": token}),
    );
    call("add_task_comment", json!({"id": key, "body": "댓글 하나"}));
    let second = call(
        "create_task",
        json!({"title": "둘째", "repo": "a/b", "criteria": ["x"]}),
    );
    let key2 = second["key"].as_str().unwrap().to_string();
    call(
        "transition_task",
        json!({"id": key2, "to": "ready-for-develop"}),
    );
    call(
        "link_tasks",
        json!({"from": key, "to": key2, "type": "blocks"}),
    );

    // 자원: 사람이 만든 스킬·에이전트(기록장에 by=human), AI의 권한 올림(승인 대기 변경 파일)
    call(
        "save_resource",
        json!({"kind": "skill", "scope": "global", "name": "리뷰 스킬", "description": "d", "body": "본문"}),
    );
    let agent = call(
        "save_resource",
        json!({"kind": "agent", "scope": "global", "name": "리뷰어", "description": "d", "permission": "read"}),
    );
    let agent_id = agent["id"].as_str().unwrap().to_string();
    let agent_token = agent["precondition"].as_str().unwrap().to_string();
    call(
        "save_resource",
        json!({"id": agent_id, "permission": "edit", "f1": agent_token,
               "caller": "ai", "session": "ai-1", "sessionAgent": "codex"}),
    );

    // 휴지통: 노트를 지워 목록 파일을 만든다
    call("delete_doc", json!({"id": "ndfx0009-00000009"}));

    let mut now: BTreeMap<String, String> = BTreeMap::new();
    collect(&vault, ".aisurada/tasks", &mut now, "vault");
    collect(&vault, ".aisurada/pending", &mut now, "vault");
    collect(&vault, ".aisurada/trash", &mut now, "vault");
    let approvals_dir = ais_core::resource::approvals::dir(&vault).unwrap();
    collect(&approvals_dir, "", &mut now, "approvals");
    // 폴더 경로가 글에 들어갔으면(실행마다 다르다) 자리표시자로 바꾼다
    let vault_text = vault.to_string_lossy().to_string();
    let approvals_text = approvals.to_string_lossy().to_string();
    let now: BTreeMap<String, String> = now
        .into_iter()
        .map(|(path, text)| {
            let path = path.replace(&approvals_dir.to_string_lossy().to_string(), "<approvals>");
            let text = text
                .replace(&vault_text, "@@VAULT@@")
                .replace(&approvals_text, "@@APPROVALS@@");
            (path, text)
        })
        .collect();
    assert!(
        now.keys().any(|k| k.ends_with("activity.jsonl"))
            && now.keys().any(|k| k.ends_with("ledger.json"))
            && now.keys().any(|k| k.contains("pending"))
            && now.keys().any(|k| k.ends_with("trash.json")),
        "시나리오가 네 가지 파일을 모두 만들지 못했다: {:?}",
        now.keys().collect::<Vec<_>>()
    );

    // 정답 파일 이름은 `/`를 `__`로 바꾼 것이다
    let file_name = |key: &str| key.replace('/', "__");
    let dir = golden_dir();
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for (key, text) in &now {
            fs::write(dir.join(file_name(key)), text).unwrap();
        }
    } else {
        let mut want: BTreeMap<String, String> = BTreeMap::new();
        for entry in fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("정답 폴더를 못 읽었다({e}). UPDATE_GOLDEN=1로 먼저 만든다"))
            .flatten()
        {
            want.insert(
                entry.file_name().to_string_lossy().to_string(),
                fs::read_to_string(entry.path()).unwrap(),
            );
        }
        let got: BTreeMap<String, String> =
            now.iter().map(|(k, v)| (file_name(k), v.clone())).collect();
        assert_eq!(
            got.keys().collect::<Vec<_>>(),
            want.keys().collect::<Vec<_>>(),
            "만든 파일 목록이 정답과 다르다"
        );
        for (name, text) in &got {
            assert_eq!(text, &want[name], "바이트가 정답과 다르다: {name}");
        }
    }
    let _ = fs::remove_dir_all(&vault);
    let _ = fs::remove_dir_all(&locks);
    let _ = fs::remove_dir_all(&approvals);
}
