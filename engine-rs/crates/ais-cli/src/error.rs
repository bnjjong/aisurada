//! `ais-cli`의 오류와 Java 호환 번호표.
//!
//! 엔진 코어는 오류의 **종류와 문구**만 안다. 옛 Java 엔진의 `AIS-400` 같은 번호와 종료 코드는 호출 어댑터가 종류에서 붙이는 호환 표시라서
//! 이 모듈이 표를 가진다. 명령줄(`AIS-409 문구` 줄과 종료 코드)과 앱 IPC(`{code, kind, message}`)가 같은 표를 쓴다.

use ais_core::error::{EngineError, ErrorKind};
use serde::Serialize;

/// 종류에서 AIS 번호(400, 403, …)를 정하는 호환 표. `AIS-409`는 지원하지 않는 명령, 부딪힘, 기록 층 거절 셋이 함께 쓴다.
pub fn aisurada_number(kind: ErrorKind) -> u16 {
    match kind {
        ErrorKind::InvalidArgument => 400,
        ErrorKind::Forbidden => 403,
        ErrorKind::NotFound => 404,
        ErrorKind::PreconditionFailed => 412,
        ErrorKind::Unsupported | ErrorKind::Conflict | ErrorKind::RecordTier => 409,
        ErrorKind::Internal => 500,
    }
}

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
        CliError {
            code: aisurada_number(error.kind),
            message: error.message,
        }
    }
}

/// 앱 IPC로 화면에 가는 오류: `{code, kind, message}`. 화면은 `kind`로 가르고 `code`는 옛 호환으로 남는다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IpcError {
    pub code: String,
    pub kind: ErrorKind,
    pub message: String,
}

impl From<EngineError> for IpcError {
    fn from(error: EngineError) -> Self {
        IpcError {
            code: format!("AIS-{}", aisurada_number(error.kind)),
            kind: error.kind,
            message: error.message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 번호표는 Java 시절 생성자가 박던 값과 같다. 응답의 `code`와 종료 코드가 이 값에서 나온다.
    #[test]
    fn the_number_table_matches_the_old_per_constructor_codes() {
        let cases = [
            (EngineError::invalid_argument("x"), 400, "invalid_argument"),
            (EngineError::forbidden("x"), 403, "forbidden"),
            (EngineError::not_found("x"), 404, "not_found"),
            (
                EngineError::precondition_failed("x"),
                412,
                "precondition_failed",
            ),
            (EngineError::conflict("x"), 409, "conflict"),
            (EngineError::unsupported("x"), 409, "unsupported"),
            (EngineError::record_tier("n1"), 409, "forbidden"),
            (EngineError::record_tier_write("n1"), 409, "forbidden"),
            (EngineError::internal("x"), 500, "internal"),
        ];
        for (error, number, kind) in cases {
            let ipc = IpcError::from(error.clone());
            assert_eq!(ipc.code, format!("AIS-{number}"));
            assert_eq!(
                serde_json::to_value(&ipc).unwrap()["kind"],
                kind,
                "{error:?}"
            );
            assert_eq!(CliError::from(error).code, number);
        }
    }

    /// 앱 IPC 오류의 JSON 모양과 칸 차례는 그대로다: `code`, `kind`, `message`.
    #[test]
    fn the_ipc_error_keeps_its_wire_shape() {
        let json = serde_json::to_string(&IpcError::from(EngineError::not_found("없다"))).unwrap();
        assert_eq!(
            json,
            r#"{"code":"AIS-404","kind":"not_found","message":"없다"}"#
        );
    }

    #[test]
    fn the_exit_code_follows_the_precondition_number() {
        assert_eq!(
            CliError::from(EngineError::precondition_failed("x")).exit_code(),
            5
        );
        assert_eq!(CliError::from(EngineError::not_found("x")).exit_code(), 3);
    }
}
