//! 앱의 첫 실행에서 Java 시절의 흔적을 정리한다: 옛 사이드카(launchd에 등록된 상주 JVM)를 내리고, 낡은 MCP 등록을 새 `ais-cli`로 다시 등록한다.
//!
//! 일은 `ais_cli::migrate::run_once`가 하고(표시 파일이 있으면 아무것도 안 한다), 여기서는 정식 앱일 때만 백그라운드 스레드로 부른다.
//! 우리 레이블(`co.kcd.aisurada.sidecar.*`)만 건드린다. 개발 실행과 빌드 폴더의 번들은 기기 전체에 영향을 주면 안 되므로 하지 않는다.

use crate::sidecar;
use tauri::{AppHandle, Runtime};

pub fn install<R: Runtime>(app: &AppHandle<R>) {
    let Some(env) = sidecar::official_cli_env(app) else {
        return;
    };
    let _ = std::thread::Builder::new()
        .name("ais-migrate".into())
        .spawn(move || {
            if let Some(report) = ais_cli::migrate::run_once(&env) {
                for line in &report.lines {
                    eprintln!("[이주] {line}");
                }
                for left in &report.left {
                    eprintln!("[이주] 못 한 것: {left}");
                }
            }
        });
}
