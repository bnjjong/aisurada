//! 엔진이 돌려주는 오류입니다. 앱은 `kind`로 가르고, 옛 Java의 `AIS-…` 번호는 코어가 모릅니다.
//!
//! 번호(`AIS-400` 따위)는 호출 어댑터가 종류에서 붙이는 호환 표시입니다(`ais-cli`의 `error` 모듈이 표를 가집니다: 명령줄 줄 머리,
//! 종료 코드, 앱 IPC의 `{code, kind, message}`). 코어는 종류와 문구만 정하고, 문구는 바이트까지 그대로입니다.
//!
//! **도메인 오류와 인프라 오류.** [`ErrorKind::Internal`]은 인프라가 실패한 것(파일 읽기·쓰기, 파싱, 직렬화)이고 나머지는 모두
//! 호출자가 고칠 수 있는 도메인 쪽 거절입니다([`EngineError::is_infra`]). 두 쪽을 타입으로 가르는 일은 처리 함수의 반환 타입을
//! 모두 바꿔야 해서 아직 하지 않았습니다.

use std::fmt;

use serde::Serialize;

/// 오류의 종류입니다. 앱은 글이나 숫자 대신 이 값으로 분기합니다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// 인자가 모자라거나 모양이 틀림
    InvalidArgument,
    /// 호출자에게 허락되지 않은 일
    Forbidden,
    /// AI 호출자에게는 기록 층 노트를 보여 주거나 고치게 하지 않음. 응답의 종류 글자는 `forbidden`이다.
    #[serde(rename = "forbidden")]
    RecordTier,
    /// 찾는 것이 없음
    NotFound,
    /// 고치기 전에 읽은 내용이 그 사이 바뀜
    PreconditionFailed,
    /// 이 판에 없는 명령
    Unsupported,
    /// 지금 상태와 부딪힘
    Conflict,
    /// 예상 밖 실패(인프라)
    Internal,
}

impl ErrorKind {
    /// 응답 JSON의 `kind` 글자
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorKind::InvalidArgument => "invalid_argument",
            ErrorKind::Forbidden | ErrorKind::RecordTier => "forbidden",
            ErrorKind::NotFound => "not_found",
            ErrorKind::PreconditionFailed => "precondition_failed",
            ErrorKind::Unsupported => "unsupported",
            ErrorKind::Conflict => "conflict",
            ErrorKind::Internal => "internal",
        }
    }
}

/// 엔진 오류: 종류와 문구.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineError {
    pub kind: ErrorKind,
    pub message: String,
}

impl EngineError {
    fn of(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self::of(ErrorKind::InvalidArgument, message)
    }

    /// 호출자에게 허락되지 않은 일
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::of(ErrorKind::Forbidden, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::of(ErrorKind::NotFound, message)
    }

    /// 고치기 전에 읽은 내용이 그 사이 바뀌었거나 확인 값이 없음
    pub fn precondition_failed(message: impl Into<String>) -> Self {
        Self::of(ErrorKind::PreconditionFailed, message)
    }

    /// 지금 상태와 부딪힘
    pub fn conflict(message: impl Into<String>) -> Self {
        Self::of(ErrorKind::Conflict, message)
    }

    /// 이 엔진에 없는 명령
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::of(ErrorKind::Unsupported, message)
    }

    /// AI 호출자에게는 기록 층 노트를 보여 주지 않는다
    pub fn record_tier(note_id: &str) -> Self {
        Self::of(
            ErrorKind::RecordTier,
            format!(
                "도구 표면이 읽을 수 없는 노트다(층=record): {note_id}. 기계가 남긴 기록 노트는 앱에서만 열립니다."
            ),
        )
    }

    /// AI 호출자가 기록 층 노트를 고치려 할 때. 종류는 읽기 쪽 [`EngineError::record_tier`]와 같고 문구만 쓰기에 맞춘다.
    pub fn record_tier_write(note_id: &str) -> Self {
        Self::of(
            ErrorKind::RecordTier,
            format!(
                "도구 표면이 고칠 수 없는 노트다(층=record): {note_id}. 기계가 남긴 기록 노트는 앱에서만 고칩니다."
            ),
        )
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::of(ErrorKind::Internal, message)
    }

    /// 인프라가 실패한 오류인가(파일, 파싱, 직렬화). 아니면 호출자가 고칠 수 있는 도메인 쪽 거절이다.
    pub fn is_infra(&self) -> bool {
        self.kind == ErrorKind::Internal
    }
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind.as_str(), self.message)
    }
}

impl std::error::Error for EngineError {}
