//! 엔진 명령 하나를 부르고 응답 JSON을 보기 좋게 찍는 개발용 도구입니다.
//!
//! 사용: `cargo run -q -p ais-core --example ais_call -- <vault 폴더> <명령> [인자 JSON]`

use std::env;
use std::process::ExitCode;

use ais_core::engine::Engine;
use serde_json::Value;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("사용법: ais_call <vault 폴더> <명령> [인자 JSON]");
        return ExitCode::from(2);
    }
    let params: Value = match args.get(2) {
        Some(text) => match serde_json::from_str(text) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("인자가 JSON이 아니다: {e}");
                return ExitCode::from(2);
            }
        },
        None => Value::Null,
    };
    match Engine::new(&args[0]).call(&args[1], params) {
        Ok(value) => {
            println!("{}", serde_json::to_string_pretty(&value).unwrap());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{}", serde_json::to_string_pretty(&error).unwrap());
            ExitCode::FAILURE
        }
    }
}
