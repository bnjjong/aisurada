//! 손으로 고쳐서 엄격한 JSON이 아니게 된 코멘트 파일([#56](https://github.com/bnjjong/aisurada/issues/56)).
//!
//! Java 엔진은 따옴표 없는 글이나 주석이 든 JSON도 읽었다. Rust는 엄격한 JSON만 읽는다. 대신 안전하게 선다:
//! 읽기는 `state: unreadable`과 이유로 알리고, 쓰기 셋(`create`, `update`, `delete`)은 파일을 그대로 두고 AIS-409로 선다.
//! 그 파일에 있던 코멘트를 말없이 덮어쓰면 안 된다.

use std::path::PathBuf;
use std::process::Command;

struct World {
    base: PathBuf,
}

impl World {
    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let output = Command::new(env!("CARGO_BIN_EXE_ais-cli"))
            .args(args)
            .env_clear()
            .env("HOME", self.base.join("home"))
            .env("AISURADA_VAULT", self.base.join("vault"))
            .env("AISURADA_LOCK_DIR", self.base.join("locks"))
            .output()
            .expect("ais-cli를 못 돌렸다");
        (
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn field(json: &str, key: &str) -> String {
    let value: serde_json::Value = serde_json::from_str(json).expect("응답이 JSON이다");
    value[key].as_str().expect(key).to_owned()
}

#[test]
fn a_comment_file_that_is_not_strict_json_is_reported_and_never_overwritten() {
    let base = std::env::temp_dir().join(format!("ais-hand-comments-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("vault")).unwrap();
    std::fs::create_dir_all(base.join("home")).unwrap();
    let world = World { base };

    assert_eq!(world.run(&["init-workspace", "name=hand"]).0, 0);
    let (code, stdout, stderr) = world.run(&[
        "create-doc",
        "agenda=demo",
        "type=free",
        "title=손으로 고친 코멘트",
        "body=본문",
    ]);
    assert_eq!(code, 0, "{stderr}");
    let (id, path) = (field(&stdout, "id"), field(&stdout, "path"));

    // 따옴표 없는 키와 주석: 사람이 손으로 고치면 나올 모양
    let file = world.base.join("vault").join(&path).join("comments.json");
    let by_hand = "{comments:[{id:\"cmhand-0001\",body:\"손으로 쓴 코멘트\",createdAt:\"2026-11-16T10:00:00+09:00\"}]} // 메모\n";
    std::fs::write(&file, by_hand).unwrap();

    let (code, stdout, _) = world.run(&["list-comments", &id]);
    assert_eq!(code, 0);
    assert_eq!(field(&stdout, "state"), "unreadable");
    assert!(field(&stdout, "reason").contains("JSON"), "{stdout}");

    for args in [
        vec!["create-comment", &id, "body=새로"],
        vec!["update-comment", &id, "comment_id=cmhand-0001", "body=고침"],
        vec!["delete-comment", &id, "comment_id=cmhand-0001"],
    ] {
        let (code, _, stderr) = world.run(&args);
        assert_eq!(code, 3, "{args:?}: {stderr}");
        assert!(
            stderr.starts_with("AIS-409 코멘트 파일을 못 읽어서 아무것도 안 썼다"),
            "{args:?}: {stderr}"
        );
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            by_hand,
            "{args:?}: 파일이 바뀌면 안 된다"
        );
    }
}
