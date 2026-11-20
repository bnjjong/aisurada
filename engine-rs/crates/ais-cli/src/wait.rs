//! `wait-decisions`: 사람이 노트의 결정 블록에 답을 끝낼 때까지 기다린다.
//!
//! AI가 「정할 것」을 결정 블록으로 쓴 뒤 셸에서 건다. 몇 초마다 `list-decisions`를 다시 불러, 사람이 「답 다 했음」을 체크해
//! 저장하면(`finished`) 답을 JSON으로 찍고 0으로 끝난다. 한도가 지나면 `result: "timeout"`을 찍고 [`TIMEOUT_CODE`]로 끝난다.
//!
//! 기다리기를 MCP 도구가 아니라 명령으로 둔 까닭: MCP 도구 호출에는 클라이언트의 시간 제한이 있어서 몇십 분을 붙잡지 못한다.
//! Claude Code는 백그라운드로 건 이 명령이 끝나면 세션을 다시 부르고, Gemini CLI는 설정 두 개를 켜면 같으며, Codex는 턴을 연 채
//! 이 명령을 기다린다(MCP 서버 안내문 참고).

use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::Outcome;
use crate::env::Env;
use crate::output;

/// 한도가 지나 「아직 답 없음」으로 끝났을 때의 종료 코드. 다른 실패(1, 2, 3, 5)와 가르려고 따로 둔다.
pub const TIMEOUT_CODE: i32 = 6;
/// 기본 한도(초): 2시간. 잊힌 대기 프로세스가 남지 않게 한다(2026-11-20 사용자 결정).
const DEFAULT_TIMEOUT: u64 = 7_200;
const MAX_TIMEOUT: u64 = 86_400;
/// 기본 확인 간격(초)
const DEFAULT_INTERVAL: u64 = 5;
const MAX_INTERVAL: u64 = 300;

const USAGE: &str = "wait-decisions <노트 id>[/<하위 노트 id>] [하위 노트 id] [timeout=<초. 기본 7200>] [interval=<초. 기본 5>]";

/// 명령줄을 풀고 기다린다. `args`는 `wait-decisions` 뒤의 칸들이다.
pub fn run(args: &[String], env: &Env) -> Outcome {
    let request = match Request::parse(args) {
        Ok(request) => request,
        Err(message) => return Outcome::err(format!("AIS-400 {message}: {USAGE}\n"), 3),
    };
    wait(&request, env, std::thread::sleep)
}

#[derive(Debug, PartialEq, Eq)]
struct Request {
    note: String,
    sub: Option<String>,
    timeout: Duration,
    interval: Duration,
}

impl Request {
    fn parse(args: &[String]) -> Result<Request, String> {
        let mut positional = Vec::new();
        let (mut timeout, mut interval) = (DEFAULT_TIMEOUT, DEFAULT_INTERVAL);
        for arg in args {
            if let Some(value) = arg.strip_prefix("timeout=") {
                timeout = seconds("timeout", value, MAX_TIMEOUT)?;
            } else if let Some(value) = arg.strip_prefix("interval=") {
                interval = seconds("interval", value, MAX_INTERVAL)?;
            } else if arg.contains('=') {
                return Err(format!("모르는 옵션이다: {arg}"));
            } else {
                positional.push(arg.trim().to_string());
            }
        }
        let (note, sub) = match positional.as_slice() {
            [one] => match one.split_once('/') {
                Some((note, sub)) => (note.to_string(), Some(sub.to_string())),
                None => (one.clone(), None),
            },
            [note, sub] => (note.clone(), Some(sub.clone())),
            _ => return Err("노트 id가 하나 필요하다".to_string()),
        };
        if note.is_empty() || sub.as_deref().is_some_and(str::is_empty) {
            return Err("노트 id가 비었다".to_string());
        }
        Ok(Request {
            note,
            sub,
            timeout: Duration::from_secs(timeout),
            interval: Duration::from_secs(interval),
        })
    }
}

fn seconds(name: &str, value: &str, max: u64) -> Result<u64, String> {
    match value.trim().parse::<u64>() {
        Ok(n) if (1..=max).contains(&n) => Ok(n),
        _ => Err(format!("{name}은 1~{max} 사이의 초여야 한다: {value}")),
    }
}

/// 기다리는 몸통. 시험이 시계를 빨리 돌리게 잠자기를 인자로 받는다.
fn wait(request: &Request, env: &Env, sleep: impl Fn(Duration)) -> Outcome {
    let started = Instant::now();
    let mut slept = Duration::ZERO;
    let mut call = vec!["list-decisions".to_string(), request.note.clone()];
    call.extend(request.sub.clone());
    loop {
        let read = crate::run(&call, env);
        if read.code != 0 {
            // 노트가 없어졌거나 vault를 못 찾은 것 따위는 그대로 알린다
            return read;
        }
        let Ok(mut value) = serde_json::from_str::<Value>(&read.stdout) else {
            return Outcome::err(
                format!("list-decisions 응답을 못 읽었다: {}\n", read.stdout.trim()),
                1,
            );
        };
        if value["total"].as_u64() == Some(0) {
            return Outcome::err(
                format!(
                    "AIS-400 기다릴 결정 블록이 없다(`> [!decision]`): {}\n",
                    call[1..].join(" ")
                ),
                3,
            );
        }
        let waited = started.elapsed().max(slept);
        if value["finished"] == json!(true) {
            return finish(&mut value, "done", waited, 0);
        }
        if waited >= request.timeout {
            return finish(&mut value, "timeout", waited, TIMEOUT_CODE);
        }
        let nap = request.interval.min(request.timeout - waited);
        sleep(nap);
        slept += nap;
    }
}

fn finish(value: &mut Value, result: &str, waited: Duration, code: i32) -> Outcome {
    if let Value::Object(map) = value {
        map.insert("result".into(), json!(result));
        map.insert("waitedSeconds".into(), json!(waited.as_secs()));
    }
    Outcome {
        stdout: output::stringify(value),
        stderr: String::new(),
        code,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_ids_and_limits() {
        let one = Request::parse(&args(&["nd1-abc"])).unwrap();
        assert_eq!((one.note.as_str(), one.sub.as_deref()), ("nd1-abc", None));
        assert_eq!(
            (one.timeout.as_secs(), one.interval.as_secs()),
            (DEFAULT_TIMEOUT, DEFAULT_INTERVAL)
        );
        let slash =
            Request::parse(&args(&["nd1-abc/sn2-def", "timeout=60", "interval=2"])).unwrap();
        assert_eq!(
            (
                slash.sub.as_deref(),
                slash.timeout.as_secs(),
                slash.interval.as_secs()
            ),
            (Some("sn2-def"), 60, 2)
        );
        let two = Request::parse(&args(&["nd1-abc", "sn2-def"])).unwrap();
        assert_eq!(two.sub.as_deref(), Some("sn2-def"));
    }

    #[test]
    fn rejects_bad_arguments() {
        assert!(Request::parse(&args(&[])).is_err());
        assert!(Request::parse(&args(&["a", "b", "c"])).is_err());
        assert!(Request::parse(&args(&["a", "timeout=0"])).is_err());
        assert!(Request::parse(&args(&["a", "interval=301"])).is_err());
        assert!(Request::parse(&args(&["a", "limit=3"])).is_err());
        assert!(Request::parse(&args(&["a/"])).is_err());
    }
}
