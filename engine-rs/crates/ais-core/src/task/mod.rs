//! 태스크(티켓)의 vault 저장소. 모양(`format`), 상태 전이(`transitions`), 집합체(`aggregate`)와 타입(`Task`, `TaskStatus` …)은
//! 도메인 크레이트(`ais-domain`)에 있고 같은 이름으로 다시 내놓는다. 여기에는 vault에서 읽고 쓰는 `store`가 있다.

pub use ais_domain::task::*;

pub mod store;
