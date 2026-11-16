//! 스킬과 에이전트 정의(자원)의 vault 쪽: 승인 기록(`approvals`), 대기 변경(`changes`), 참조 목록 파일(`refs`),
//! 쓸 수 있는 자원 풀기(`resolver`), 읽고 쓰는 곳(`store`). 모양(`format`), 승인 규칙(`aggregate`), 인자·참조 규칙(`rules`)과
//! 타입(`Resource`, `ResourceKind`, `Entry`, `Fields` …)은 도메인 크레이트(`ais-domain`)에 있고 같은 이름으로 다시 내놓는다.

pub use ais_domain::resource::*;

pub mod approvals;
pub mod changes;
pub mod refs;
pub mod resolver;
pub mod store;
