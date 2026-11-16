//! `ais-cli install`과 `ais-cli sweep`.
//!
//! Java 시절의 `install`은 jar를 `Application Support/aisurada/bin`에 복사하고 사이드카를 등록했다. Rust `ais-cli`는 실행 파일 하나라서
//! 설치할 것이 없다. 그래서 `install`은 안내만 하고, 옛 jar와 사이드카를 걷어 내는 일은 `sweep`이 한다.

use crate::Outcome;
use crate::env::Env;
use crate::old_sidecar;

pub fn install(env: &Env) -> Outcome {
    let inventory = old_sidecar::inventory(std::path::Path::new(&env.home));
    let mut text = String::from(
        "설치할 파일이 없다. ais-cli는 실행 파일 하나이고, 사이드카도 jar도 쓰지 않는다.\n",
    );
    if !inventory.is_empty() {
        text.push_str(&format!(
            "옛 사이드카 {}개와 jar {}개가 남아 있다. `ais-cli sweep` 으로 걷어 낸다.\n",
            inventory.traces.len(),
            inventory.jars.len()
        ));
    }
    Outcome {
        stdout: text,
        stderr: String::new(),
        code: 0,
    }
}

/// `ais-cli sweep [--dry-run]`: 옛 사이드카를 모두 내리고 plist·상태 파일·설치해 둔 jar·로그를 지운다.
pub fn sweep(args: &[String], env: &Env) -> Outcome {
    let dry_run = args.iter().any(|a| a == "--dry-run");
    let report = old_sidecar::sweep(env, std::path::Path::new(&env.home), dry_run);
    Outcome {
        stdout: report.lines.join("\n") + "\n",
        stderr: report.left.iter().map(|l| format!("{l}\n")).collect(),
        code: if report.left.is_empty() { 0 } else { 1 },
    }
}
