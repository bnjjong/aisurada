//! Java 기준값(`YamlGroundTruth`의 JSON 줄)과 Rust 구현의 프론트매터 읽기·쓰기 결과를 맞댑니다.
//!
//! 사용: `cargo run -p ais-core --example yaml_check -- <ground-truth.jsonl> [--verbose] [--limit N]`
//!
//! 사례마다 결과를 넷으로 나눕니다.
//! - 일치: 읽은 값이 같고, 읽은 값이 같으면 다시 쓴 글도 같다.
//! - 알려진 차이: 순환 구조처럼 Rust가 일부러 다르게 다루는 입력.
//! - 경고: Java는 거부하는데 Rust는 읽는다(더 너그럽다). 노트가 사라지는 쪽이 아니라서 실패로 세지 않는다.
//! - 실패: 읽은 값이 다르거나, Java는 읽는데 Rust는 못 읽거나, 다시 쓴 글이 다르다. 하나라도 있으면 종료 코드 1이다.

use std::collections::BTreeMap;
use std::process::ExitCode;
use std::{env, fs};

use ais_core::frontmatter::{self, Value};
use serde::Deserialize;
use serde_json::{Value as Json, json};

/// Rust가 읽은 값을 Java 기준값과 같은 꼬리표 JSON으로 바꿉니다.
fn tagged(value: &Value) -> Json {
    match value {
        Value::Null => Json::Null,
        Value::Bool(b) => Json::Bool(*b),
        Value::Int(text) => json!({"$": "int", "v": text}),
        Value::Float(x) => {
            let bits = if x.is_nan() {
                0x7ff8_0000_0000_0000
            } else {
                x.to_bits()
            };
            json!({"$": "float", "v": format!("{bits:x}")})
        }
        Value::Str(text) => Json::String(text.clone()),
        Value::List(list) => Json::Array(list.iter().map(tagged).collect()),
        Value::Map(map) => {
            json!({"$": "map", "e": map.iter().map(|(k, v)| json!([tagged(k), tagged(v)])).collect::<Vec<_>>()})
        }
        Value::Other => json!({"$": "other"}),
    }
}

/// Java 쪽의 `other`는 클래스 이름이 붙어 있어서, 비교할 때는 `other`라는 사실만 남긴다.
fn normalize(value: &Json) -> Json {
    match value {
        Json::Object(object) if object.get("$") == Some(&json!("other")) => json!({"$": "other"}),
        Json::Object(object) => Json::Object(
            object
                .iter()
                .map(|(k, v)| (k.clone(), normalize(v)))
                .collect(),
        ),
        Json::Array(items) => Json::Array(items.iter().map(normalize).collect()),
        other => other.clone(),
    }
}

fn clip(text: &str) -> String {
    const LIMIT: usize = 500;
    let mut clipped: String = text.chars().take(LIMIT).collect();
    if text.chars().count() > LIMIT {
        clipped.push('…');
    }
    clipped
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|line| format!("      | {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

enum Verdict {
    Match,
    Known(&'static str),
    Lenient(String),
    Fail(String),
}

#[derive(Default)]
struct Stats {
    total: usize,
    matched: usize,
    known: usize,
    lenient: usize,
    failed: usize,
    dump_checked: usize,
    dump_matched: usize,
}

/// 두 값이 정수 칸의 수만 다르고 나머지는 같은지 봅니다.
fn differs_only_in_ints(a: &Json, b: &Json) -> bool {
    match (a, b) {
        (Json::Object(x), Json::Object(y)) => {
            let int = Some(&json!("int"));
            if x.get("$") == int && y.get("$") == int {
                return true;
            }
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| differs_only_in_ints(v, w)))
        }
        (Json::Array(x), Json::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(v, w)| differs_only_in_ints(v, w))
        }
        _ => a == b,
    }
}

/// `1:30`처럼 숫자 사이에 콜론이 있는지 봅니다(60진수 정수 표기).
fn has_sexagesimal_mark(input: &str) -> bool {
    input
        .as_bytes()
        .windows(3)
        .any(|w| w[0].is_ascii_digit() && w[1] == b':' && w[2].is_ascii_digit())
}

/// 읽은 결과를 Java와 견줍니다.
fn judge_parse(row: &Json, input: &str, parsed: &Option<Vec<(Value, Value)>>) -> Verdict {
    let java_kind = row["parse"]["kind"].as_str().unwrap_or("?");
    match (java_kind, parsed) {
        ("map", Some(map)) => {
            let got = tagged(&Value::Map(map.clone()));
            let want = normalize(&row["parse"]["value"]);
            if got == want {
                Verdict::Match
            } else if has_sexagesimal_mark(input) && differs_only_in_ints(&want, &got) {
                // Java(SnakeYAML)는 60진수 정수를 32비트로 계산해서 큰 값이 넘친다. Rust 값이 수학적으로 맞다.
                Verdict::Known(
                    "60진수 정수가 32비트를 넘는 입력: Java는 값이 넘쳐 틀어지고 Rust는 맞는 값을 낸다",
                )
            } else {
                Verdict::Fail(format!(
                    "읽은 값이 다르다\n    Java: {}\n    Rust: {}",
                    clip(&want.to_string()),
                    clip(&got.to_string())
                ))
            }
        }
        ("none", None) => Verdict::Match,
        ("cycle", None) => Verdict::Known(
            "자기 자신을 가리키는 구조: Java는 순환 구조를 만들고 Rust는 머리 없음으로 본다",
        ),
        ("error", None) => {
            Verdict::Known("Java는 런타임 예외로 명령이 실패하는 입력: Rust는 머리 없음으로 본다")
        }
        ("map", None) => Verdict::Fail("Java는 읽었는데 Rust는 머리 없음으로 봤다".to_string()),
        ("none" | "error", Some(map)) => Verdict::Lenient(format!(
            "Java는 거부하는데 Rust는 읽었다: {}",
            clip(&tagged(&Value::Map(map.clone())).to_string())
        )),
        (kind, _) => Verdict::Fail(format!("예상하지 못한 조합: Java `{kind}`")),
    }
}

/// 읽은 값이 같은 사례의 다시 쓴 글을 Java와 견줍니다.
fn judge_dump(row: &Json, map: &[(Value, Value)]) -> Result<(), String> {
    let java = &row["dump"];
    match (java["kind"].as_str(), frontmatter::dump_general_map(map)) {
        (Some("text"), Ok(got)) => {
            let want = java["text"].as_str().unwrap_or_default();
            if got == want {
                Ok(())
            } else {
                Err(format!(
                    "다시 쓴 글이 다르다\n    Java:\n{}\n    Rust:\n{}",
                    indent(&clip(want)),
                    indent(&clip(&got))
                ))
            }
        }
        (Some("error"), Err(_)) => Ok(()),
        (Some("text"), Err(e)) => Err(format!("Java는 썼는데 Rust는 오류: {e}")),
        (Some("error"), Ok(got)) => Err(format!(
            "Java는 오류인데 Rust는 썼다:\n{}",
            indent(&clip(&got))
        )),
        (other, _) => Err(format!("알 수 없는 쓰기 결과: {other:?}")),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(path) = args
        .iter()
        .find(|a| !a.starts_with("--") && a.parse::<usize>().is_err())
    else {
        eprintln!("사용법: yaml_check <ground-truth.jsonl> [--verbose] [--limit N]");
        return ExitCode::from(2);
    };
    let verbose = args.iter().any(|a| a == "--verbose");
    let limit = args
        .iter()
        .position(|a| a == "--limit")
        .and_then(|i| args.get(i + 1))
        .and_then(|n| n.parse::<usize>().ok())
        .unwrap_or(usize::MAX);
    let text = fs::read_to_string(path).expect("기준값 파일을 읽지 못했다");

    let mut stats: BTreeMap<String, Stats> = BTreeMap::new();
    // (이름, 입력, 이유)
    let mut fails: Vec<(String, String, String)> = Vec::new();
    let mut lenients: Vec<(String, String, String)> = Vec::new();
    let mut knowns: BTreeMap<&'static str, usize> = BTreeMap::new();

    for line in text.lines() {
        // 깊이 시험 사례는 JSON도 깊어서 재귀 한도를 푼다
        let mut deserializer = serde_json::Deserializer::from_str(line);
        deserializer.disable_recursion_limit();
        let row = Json::deserialize(&mut deserializer).expect("기준값 줄이 JSON이 아니다");
        let name = row["name"].as_str().unwrap_or("?").to_string();
        let input = row["input"].as_str().unwrap_or_default();
        let source = name.split(':').next().unwrap_or("?").to_string();
        let entry = stats.entry(source).or_default();
        entry.total += 1;

        // 죽는 입력을 찾을 때 쓴다: 처리하기 전에 이름을 stderr에 남긴다
        if env::var_os("YAML_CHECK_TRACE").is_some() {
            eprintln!("{name}");
        }
        let parsed = frontmatter::parse(input);

        match judge_parse(&row, input, &parsed) {
            Verdict::Match => {
                entry.matched += 1;
                // 읽은 값이 같을 때만 다시 쓴 글을 맞댄다(Java가 값을 읽은 경우)
                if let (Some(map), "map") = (&parsed, row["parse"]["kind"].as_str().unwrap_or("")) {
                    entry.dump_checked += 1;
                    match judge_dump(&row, map) {
                        Ok(()) => entry.dump_matched += 1,
                        Err(why) => {
                            entry.failed += 1;
                            entry.matched -= 1;
                            fails.push((name, input.to_string(), why));
                        }
                    }
                }
            }
            Verdict::Known(why) => {
                entry.known += 1;
                *knowns.entry(why).or_default() += 1;
            }
            Verdict::Lenient(why) => {
                entry.lenient += 1;
                lenients.push((name, input.to_string(), why));
            }
            Verdict::Fail(why) => {
                entry.failed += 1;
                fails.push((name, input.to_string(), why));
            }
        }
    }

    println!(
        "{:<8} {:>6} {:>6} {:>10} {:>6} {:>6} {:>14}",
        "출처", "건수", "일치", "알려진 차이", "경고", "실패", "쓰기 일치"
    );
    for (source, s) in &stats {
        println!(
            "{:<8} {:>6} {:>6} {:>10} {:>6} {:>6} {:>14}",
            source,
            s.total,
            s.matched,
            s.known,
            s.lenient,
            s.failed,
            format!("{}/{}", s.dump_matched, s.dump_checked)
        );
    }

    if !knowns.is_empty() {
        println!("\n알려진 차이");
        for (why, count) in &knowns {
            println!("  - {count}건: {why}");
        }
    }

    print_list("경고(Rust가 더 너그럽게 읽음)", &lenients, verbose, limit);
    print_list("실패", &fails, verbose, limit);

    if fails.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn print_list(title: &str, items: &[(String, String, String)], verbose: bool, limit: usize) {
    if items.is_empty() {
        return;
    }
    println!("\n{title} {}건", items.len());
    for (name, input, why) in items.iter().take(limit) {
        if verbose {
            println!("  - {name}: {why}\n    입력: {:?}", clip(input));
        } else {
            println!("  - {name}: {}", why.lines().next().unwrap_or(""));
        }
    }
}
