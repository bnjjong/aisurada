//! 엔진이 돌려주는 오류입니다. 앱은 `kind`로 가르고, 옛 `AIS-…` 코드는 `code`에 그대로 남깁니다.

use std::fmt;

use serde::Serialize;

/// 오류의 종류입니다. 앱은 글이나 숫자 대신 이 값으로 분기합니다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// 인자가 모자라거나 모양이 틀림(`AIS-400`)
    InvalidArgument,
    /// 호출자에게 허락되지 않은 일(`AIS-403`)
    Forbidden,
    /// 찾는 것이 없음(`AIS-404`)
    NotFound,
    /// 고치기 전에 읽은 내용이 그 사이 바뀜(`AIS-412`)
    PreconditionFailed,
    /// 이 판에 없는 명령(`AIS-409` 가운데 일부)
    Unsupported,
    /// 지금 상태와 부딪힘(`AIS-409` 나머지)
    Conflict,
    /// 예상 밖 실패(`AIS-500`)
    Internal,
}

/// 엔진 오류. JSON으로는 `{code, kind, message}`가 됩니다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EngineError {
    pub code: &'static str,
    pub kind: ErrorKind,
    pub message: String,
}

impl EngineError {
    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self {
            code: "AIS-400",
            kind: ErrorKind::InvalidArgument,
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            code: "AIS-404",
            kind: ErrorKind::NotFound,
            message: message.into(),
        }
    }

    /// 이 엔진에 없는 명령(`AIS-409`)
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self {
            code: "AIS-409",
            kind: ErrorKind::Unsupported,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "AIS-500",
            kind: ErrorKind::Internal,
            message: message.into(),
        }
    }
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.code, self.message)
    }
}

impl std::error::Error for EngineError {}
