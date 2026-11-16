//! 엔진의 바닥 크레이트. 다른 영역(vault, write, task, …)을 모르는 순수 코드만 둔다.
//!
//! - `text`, `json`, `error`: Java와 같은 글·JSON 규칙과 오류 값(종류와 문구)
//! - `frontmatter`: 노트 머리 YAML 읽기·쓰기
//! - `kernel`: 여러 영역이 같이 쓰는 작은 공통 코드(호출 맥락, id 모양, 시각, 확인 값 따위)
//! - `snake_dump`: SnakeYAML과 같은 YAML 방출기(태스크·자원 파일을 쓴다)
//!
//! 이 크레이트는 파일·시계·환경 변수에 손대지 않는다(`ais-core`의 구조 시험이 이를 못박는다). 컴파일러가 바닥이 위를 못 보게 막는다:
//! 이 크레이트는 `ais-core`를 의존하지 않는다.

pub mod error;
pub mod frontmatter;
pub mod json;
pub mod kernel;
pub mod snake_dump;
pub mod text;
