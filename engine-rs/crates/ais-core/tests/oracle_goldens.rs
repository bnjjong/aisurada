//! 기준(옛 Java 엔진) 없이 도는 YAML 골든 시험: Java가 쓴 기준값을 파일로 고정해 두고 Rust 구현을 견준다(이슈 #67).
//!
//! - `golden/yaml-ground-truth.jsonl`: 노트 머리(프론트매터) 읽기·쓰기를 진짜 SnakeYAML 2.4가 처리한 결과 96건.
//!   사례는 `tools/yaml-spike/cases.txt`와 중첩 깊이 경계 사례이고 vault 내용은 하나도 없다. 견주는 도구는 `examples/yaml_check.rs`다.
//! - `golden/snake-cases.tsv`: 진짜 SnakeYAML 2.4의 `Yaml.dump`가 쓴 방출기 사례 2,232건(손으로 고른 498건 + 시드 1의 퍼징 1,734건).
//!   견주는 도구는 `examples/snake_check.rs`다.
//!
//! 견주는 도구는 예제 실행 파일이라 `cargo run`으로 부른다(`ais-cli`의 골든 시험이 하네스를 부르는 것과 같은 방식).
//! 알고 바꾼 동작이면 기준 Java로 다시 만든다(JDK 21 필요). 방법은 `docs/developer-guide.md` §4 「골든 시험」에 있다.

use std::path::Path;
use std::process::Command;

fn run_example(example: &str, golden: &str) -> String {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let file = manifest.join("../../golden").join(golden);
    assert!(
        file.is_file(),
        "골든 파일 {golden}이 없다. `docs/developer-guide.md` §4 「골든 시험」으로 만든다(JDK 21 필요)."
    );
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let output = Command::new(cargo)
        .current_dir(manifest)
        .args([
            "run",
            "-q",
            "--locked",
            "-p",
            "ais-core",
            "--example",
            example,
            "--",
        ])
        .arg(&file)
        .output()
        .expect("cargo run을 못 돌렸다");
    let text = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success(),
        "{example}가 골든 파일 {golden}과 어긋났다.\n{text}"
    );
    text
}

#[test]
fn frontmatter_matches_the_recorded_snakeyaml_results_without_java() {
    let text = run_example("yaml_check", "yaml-ground-truth.jsonl");
    // 표의 `case` 줄: 출처, 건수, 일치, 알려진 차이, 경고, 실패, 쓰기 일치
    let row: Vec<&str> = text
        .lines()
        .find(|line| line.starts_with("case"))
        .unwrap_or_else(|| panic!("결과 표에 case 줄이 없다.\n{text}"))
        .split_whitespace()
        .collect();
    let number = |i: usize| -> u32 { row[i].parse().unwrap_or_else(|_| panic!("{row:?}")) };
    assert!(number(1) >= 90, "사례가 너무 적다(빈 파일?): {row:?}");
    assert_eq!(number(5), 0, "실패가 있다: {row:?}");
    assert_eq!(
        number(1),
        number(2) + number(3) + number(4),
        "일치, 알려진 차이, 경고의 합이 건수와 다르다: {row:?}"
    );
}

#[test]
fn snake_emitter_matches_the_recorded_snakeyaml_dumps_without_java() {
    let text = run_example("snake_check", "snake-cases.tsv");
    let summary = text
        .lines()
        .find(|line| line.starts_with("사례 "))
        .unwrap_or_else(|| panic!("요약 줄이 없다.\n{text}"));
    assert!(
        summary.contains("어긋남 0건, 오류 0건"),
        "방출기가 어긋났다: {summary}\n{text}"
    );
    let count: u32 = summary
        .trim_start_matches("사례 ")
        .split('건')
        .next()
        .and_then(|n| n.trim().parse().ok())
        .unwrap_or(0);
    assert!(count >= 2000, "사례가 너무 적다(빈 파일?): {summary}");
}
