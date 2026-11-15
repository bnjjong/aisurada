use std::io::Write;

use ais_cli::env::Env;

fn main() {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let outcome = ais_cli::run(&args, &Env::from_process());

    // 파이프가 먼저 닫혀도(`| head`) 조용히 끝낸다
    let _ = std::io::stdout().write_all(outcome.stdout.as_bytes());
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().write_all(outcome.stderr.as_bytes());
    std::process::exit(outcome.code);
}
