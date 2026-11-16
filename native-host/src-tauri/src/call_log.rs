//! 점검용 호출 기록. 환경변수 `AISURADA_CALL_LOG`에 파일 경로를 주면 앱이 엔진을 부른 호출(문자열 목록 `sidecar_call`, 호출 객체
//! `engine_call`)을 한 줄씩 JSON으로 적는다. 실제 창 점검(`scripts/workspace-acceptance.mjs`)이 어느 길로 갔는지와 상관없이 호출 수와
//! 차례를 세는 데 쓴다. 환경변수가 없으면 아무것도 하지 않는다.

use serde_json::{json, Value};
use std::{io::Write, time::UNIX_EPOCH};

fn append(line: Value) {
    let Some(path) = std::env::var_os("AISURADA_CALL_LOG").filter(|p| !p.is_empty()) else {
        return;
    };
    let start = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let mut line = line;
    line["start"] = json!(start);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        // 한 번의 쓰기로 한 줄을 통째로 적어야 여러 스레드의 호출이 줄 중간에서 섞이지 않는다
        let _ = file.write_all(format!("{line}\n").as_bytes());
    }
}

/// `sidecar_call`(명령줄 길) 한 번
pub fn sidecar(argv: &[String]) {
    append(json!({ "kind": "sidecar", "argv": argv }));
}

/// `engine_call`(호출 객체 길) 한 번
pub fn engine(command: &str, args: &Option<Value>) {
    append(json!({ "kind": "engine", "command": command, "args": args }));
}
