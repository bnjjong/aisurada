//! `ais-cli`의 오류. Java `AisException`처럼 AIS 번호와 글을 담고, 종료 코드를 정한다.

use ais_core::error::EngineError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliError {
    /// `AIS-400`의 400
    pub code: u16,
    pub message: String,
}

impl CliError {
    pub fn new(code: u16, message: impl Into<String>) -> Self {
        CliError {
            code,
            message: message.into(),
        }
    }

    /// 표준 오류에 찍는 줄(`AIS-400 …`). 끝 줄바꿈은 붙이지 않는다.
    pub fn line(&self) -> String {
        format!("AIS-{} {}", self.code, self.message)
    }

    /// 프로세스 종료 코드. 고치기 전 확인(412)은 5, 나머지 AIS 오류는 3이다.
    pub fn exit_code(&self) -> i32 {
        if self.code == 412 { 5 } else { 3 }
    }
}

impl From<EngineError> for CliError {
    fn from(error: EngineError) -> Self {
        let code = error
            .code
            .strip_prefix("AIS-")
            .and_then(|digits| digits.parse().ok())
            .unwrap_or(500);
        CliError {
            code,
            message: error.message,
        }
    }
}
