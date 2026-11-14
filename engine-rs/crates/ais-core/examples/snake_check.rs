//! SnakeYAML 방출기 맞대기: `<사례 파일>`의 각 줄 `입력 JSON \t 기대 YAML(JSON 글)`을 `snake_dump::dump_map`으로 쓴 결과와 견준다.
//! 사례 파일은 진짜 SnakeYAML 2.4가 쓴 것이다(만드는 법은 `docs/developer-guide.md` §4).
//! 사용: `cargo run -p ais-core --example snake_check -- <사례 파일> [--show=<건수>]`

use std::{env, fs, process::ExitCode};

use ais_core::frontmatter::Value;
use ais_core::json::{self, JsonValue};
use ais_core::write::snake_dump::dump_map;

fn to_value(value: &JsonValue) -> Value {
    match value {
        JsonValue::Str(text) => Value::Str(text.clone()),
        JsonValue::Array(items) => Value::List(items.iter().map(to_value).collect()),
        JsonValue::Object(entries) => Value::Map(
            entries
                .iter()
                .map(|(key, item)| (Value::Str(key.clone()), to_value(item)))
                .collect(),
        ),
        JsonValue::Literal(raw) => match raw.as_str() {
            "null" => Value::Null,
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            "NaN" => Value::Float(f64::NAN),
            "Infinity" => Value::Float(f64::INFINITY),
            "-Infinity" => Value::Float(f64::NEG_INFINITY),
            text if text.contains(['.', 'E', 'e']) => {
                Value::Float(text.parse().unwrap_or(f64::NAN))
            }
            text => Value::Int(text.to_string()),
        },
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(path) = args.iter().find(|a| !a.starts_with("--")) else {
        eprintln!("사용: snake_check <사례 파일> [--show=<건수>]");
        return ExitCode::from(2);
    };
    let show: usize = args
        .iter()
        .find_map(|a| a.strip_prefix("--show="))
        .and_then(|n| n.parse().ok())
        .unwrap_or(10);
    let text = fs::read_to_string(path).expect("사례 파일을 못 읽었다");
    let (mut total, mut diffs, mut errors) = (0usize, 0usize, 0usize);
    for line in text.lines() {
        let Some((input, expected)) = line.split_once('\t') else {
            continue;
        };
        let (Ok(JsonValue::Object(entries)), Ok(JsonValue::Str(expected))) =
            (json::parse(input), json::parse(expected))
        else {
            continue;
        };
        total += 1;
        let map: Vec<(Value, Value)> = entries
            .iter()
            .map(|(key, item)| (Value::Str(key.clone()), to_value(item)))
            .collect();
        match dump_map(&map) {
            Ok(actual) if actual == expected => {}
            Ok(actual) => {
                diffs += 1;
                if diffs <= show {
                    println!("어긋남 {input}\n--SnakeYAML--\n{expected}--Rust--\n{actual}");
                }
            }
            Err(e) => {
                errors += 1;
                if errors <= show {
                    println!("오류 {input}: {e}");
                }
            }
        }
    }
    println!("사례 {total}건, 어긋남 {diffs}건, 오류 {errors}건");
    if diffs + errors == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
