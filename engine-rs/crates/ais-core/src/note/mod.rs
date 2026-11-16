//! 노트 도메인: 노트의 규칙을 파일도 vault도 모르는 순수한 코드로 모은 곳.
//!
//! - `meta`: 메타데이터 타입(`MetaData`, `Link`, `Evaluation`), 본문 형식(`Format`), 머리 맵 도우미
//! - `front`: 노트 머리를 글로 짓는 일(`NoteFront`)과 편집 도장 판정(`judge`). 지금 시각과 편집자는 인자로 받는다
//! - `placement`: 노트를 놓을 자리 정책(날짜·시각 인자, 폴더 이름, 월별 배치 여부는 입력으로)
//! - `edit`: `update_doc`의 규칙(`ParsedEdit`): 인자 검사, 고칠 수 있는지 확인, 새 글과 새 자리 계산
//!
//! 파일을 읽고 쓰는 일, 폴더를 옮기는 일, 색인은 이 모듈 밖(`commands`, `write`, `vault`)이 한다. `tests/architecture.rs`가 이 모듈이
//! 포트·쓰기·vault·어댑터·`std::fs`를 쓰지 않는지 막는다.

pub mod edit;
pub mod front;
pub mod meta;
pub mod placement;
pub mod slug;
