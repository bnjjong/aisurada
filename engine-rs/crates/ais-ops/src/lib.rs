//! 운영 도구 크레이트. 엔진을 부르는 명령줄(`ais-cli`)과 갈라서, 에이전트 등록·진단·이주처럼 설치 환경을 만지는 일만 둔다.
//!
//! - `register`, `agent_cli`: `claude-mcp`, `codex-mcp`, `gemini-mcp`(MCP 서버를 각 에이전트에 등록·해제)
//! - `doctor`: vault, git, MCP 등록, 남은 옛 사이드카의 상태 진단
//! - `migrate`, `old_sidecar`, `install`: 첫 실행 이주, 옛 Java 사이드카 걷어 내기
//! - `env`: 프로세스 환경(환경 변수, 홈, 현재 폴더)을 값으로 묶고 vault와 세션 표식을 정하는 규칙
//!
//! 이 크레이트는 엔진의 명령 해석(`parse`, `map`, `tool`, `legacy`)과 MCP 서버를 모른다. `ais-cli`가 이 크레이트를 의존하고 모듈을 같은 이름으로 다시 내놓는다.

pub mod agent_cli;
pub mod doctor;
pub mod env;
pub mod install;
pub mod migrate;
pub mod old_sidecar;
pub mod register;

/// 한 번의 호출 결과: 표준 출력·표준 오류에 쓸 글과 종료 코드
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

impl Outcome {
    /// 성공: 표준 출력에 글을 쓰고 종료 코드 0
    pub fn out(stdout: String) -> Outcome {
        Outcome {
            stdout,
            stderr: String::new(),
            code: 0,
        }
    }

    /// 실패: 표준 오류에 글을 쓰고 주어진 종료 코드
    pub fn err(stderr: String, code: i32) -> Outcome {
        Outcome {
            stdout: String::new(),
            stderr,
            code,
        }
    }
}
