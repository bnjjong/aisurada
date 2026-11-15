//! 공용 커널: 여러 영역이 같이 쓰는 아주 작은 공통 코드.
//!
//! 영역(`vault`, `write`, `task`, `resource`, `session`, `commands`)은 서로를 거치지 않고 여기를 거친다. 그래서 이 모듈은 다른 영역을 쓰지 않는다
//! (쓰는 것은 `text`, `json`, `frontmatter`, `error` 같은 바탕 모듈뿐이다). 규칙은 `tests/architecture.rs`가 막는다.
//!
//! - `actor`: 호출자(앱, AI)
//! - `doc_type`: 노트 유형 여섯 가지
//! - `hue`: 아젠다 색 열두 가지
//! - `ids`: id 모양(접두어별 정규식)
//! - `java`: Java가 글·수를 다루던 규칙(`trim`, `isBlank`, `String.valueOf`, `Double.toString`)과 JSON 값 바꾸기
//! - `link_kind`, `task_link`: 노트 연결 종류, 태스크 연결 종류
//! - `token`: 내용 확인 값(`f1`, `precondition`, `token`)
//! - `tier`: 노트의 층(읽기·기록)과 가시성 정책
//! - `time`: 날짜 계산(그레고리력)과 ISO 시각 읽기·쓰기
//! - `confine`: 경로 조각 검사와 기준 폴더 가두기

pub mod actor;
pub mod confine;
pub mod doc_type;
pub mod hue;
pub mod ids;
pub mod java;
pub mod link_kind;
pub mod task_link;
pub mod tier;
pub mod token;
pub mod time;
