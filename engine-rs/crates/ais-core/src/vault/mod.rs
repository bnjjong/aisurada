//! vault(노트 폴더) 읽기: 아젠다, 노트, 후속 노트를 훑어 색인을 만들고 조건으로 거릅니다.

pub mod agenda;
pub mod attach_text;
pub mod filter;
pub mod index;
pub mod note_file;
pub mod note_meta;
pub mod project;
pub mod scanner;
pub mod template;
pub mod trash;

use serde::Serialize;

/// 훑다가 건너뛴 파일 하나. `kind`는 `note`, `agenda`, `docTemplate`, `ritualTemplate`, `trash` 가운데 하나다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Skipped {
    pub kind: &'static str,
    pub path: String,
    pub reason: String,
}

/// 동기화 도구가 만든 충돌 사본으로 보고 노트로 읽지 않은 항목
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Conflict {
    pub kind: &'static str,
    pub path: String,
}
