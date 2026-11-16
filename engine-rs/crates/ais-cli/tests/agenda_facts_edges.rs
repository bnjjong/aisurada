//! 사실층 명령의 가장자리. 지운 Java `AgendaFactsTest`를 같은 명령으로 Rust에 돌려 보다가 찾은 차이 둘을 고정한다([#81](https://github.com/bnjjong/aisurada/issues/81)).
//!
//! - 사실층 파일 자리에 빈 폴더가 있을 때 `facts=`(비우기)는 그 폴더를 지운다(Java `Files.deleteIfExists`). 안이 있는 폴더는 지우지 못하고 AIS-500이다.
//! - `facts-brief`가 연도 0 근처에서 음수 연도를 `-0001`처럼 네 자리로 적는다(Java `LocalDate.toString`).
//!   연도 0과 음수·`+10000` 이상의 연도를 입력으로 받는 것은 Rust가 일부러 받지 않는다(`plan-day`의 확장 연도와 같은 판단).

use std::path::PathBuf;
use std::process::Command;

struct World {
    base: PathBuf,
}

impl World {
    fn new(name: &str) -> World {
        let base =
            std::env::temp_dir().join(format!("ais-facts-edges-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("vault")).unwrap();
        std::fs::create_dir_all(base.join("home")).unwrap();
        let world = World { base };
        assert_eq!(world.run(&["init-workspace", "name=facts"]).0, 0);
        world
    }

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

fn json(text: &str) -> serde_json::Value {
    serde_json::from_str(text).unwrap_or_else(|e| panic!("JSON이 아니다: {e}\n{text}"))
}

#[test]
fn clearing_facts_removes_an_empty_directory_but_not_a_full_one() {
    let world = World::new("empty-dir");
    let (code, stdout, stderr) = world.run(&["create-agenda", "title=사실층"]);
    assert_eq!(code, 0, "{stderr}");
    let id = json(&stdout)["id"].as_str().unwrap().to_owned();
    let slug = json(&stdout)["slug"].as_str().unwrap().to_owned();
    let file = world
        .base
        .join("vault/.aisurada/facts")
        .join(format!("{slug}.md"));

    // 빈 폴더가 사실층 파일 자리에 있다: 읽으면 없는 것, 비우면 그 폴더가 사라진다
    std::fs::create_dir_all(&file).unwrap();
    let (code, stdout, _) = world.run(&["get-agenda-facts", &id]);
    assert_eq!(code, 0);
    assert!(
        json(&stdout).get("facts").is_none(),
        "폴더는 없는 것으로 읽는다"
    );
    let (code, _, stderr) = world.run(&["update-agenda-facts", &id, "facts="]);
    assert_eq!(code, 0, "{stderr}");
    assert!(!file.exists(), "빈 폴더가 지워진다");

    // 안이 있는 폴더는 지우지 못하고, 안의 글은 그대로다
    std::fs::create_dir_all(&file).unwrap();
    std::fs::write(file.join("child"), "keep").unwrap();
    let (code, _, stderr) = world.run(&["update-agenda-facts", &id, "facts="]);
    assert_eq!(code, 3);
    assert!(
        stderr.starts_with("AIS-500 파일을 지우지 못했다: "),
        "{stderr}"
    );
    assert_eq!(std::fs::read_to_string(file.join("child")).unwrap(), "keep");
}

#[test]
fn facts_brief_writes_negative_years_with_four_digits() {
    let world = World::new("year-zero");
    let (code, stdout, stderr) = world.run(&["facts-brief", "to=0000-01-03"]);
    assert_eq!(code, 0, "{stderr}");
    let brief = json(&stdout);
    assert_eq!(brief["from"], "-0001-12-28", "연도 -1은 네 자리다");
    assert_eq!(brief["to"], "0000-01-03");

    // 입력으로는 음수·확장 연도를 받지 않는다(일부러 다르다)
    for day in ["-0001-01-07", "+10000-01-07", "10000-01-01"] {
        let (code, _, stderr) = world.run(&["facts-brief", &format!("to={day}")]);
        assert_eq!(code, 3, "{day}");
        assert!(
            stderr.contains("YYYY-MM-DD 모양이어야 한다"),
            "{day}: {stderr}"
        );
    }
}
