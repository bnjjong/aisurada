//! 쓰기 잠금을 잡고 지정한 시간 동안 쥐고 있는다. Java 엔진의 쓰기가 이 잠금을 기다리는지 보는 상호 운용 시험용이다.
//!
//! 사용: `cargo run -q -p ais-core --example hold_lock -- <vault 폴더> <잠금 폴더> <쥐는 시간(ms)>`
//! 잠금을 잡으면 표준 출력에 `locked`를 찍는다.

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use ais_core::adapters::flock_lock as lock;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [vault, lock_dir, millis] = args.as_slice() else {
        eprintln!("사용법: hold_lock <vault 폴더> <잠금 폴더> <쥐는 시간(ms)>");
        return ExitCode::from(2);
    };
    let Ok(millis) = millis.parse::<u64>() else {
        eprintln!("시간은 숫자여야 한다: {millis}");
        return ExitCode::from(2);
    };
    match lock::acquire(Path::new(vault), Path::new(lock_dir)) {
        Ok(_guard) => {
            println!("locked");
            let _ = std::io::stdout().flush();
            std::thread::sleep(Duration::from_millis(millis));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
