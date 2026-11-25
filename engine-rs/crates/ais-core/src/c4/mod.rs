//! C4 아키텍처 모델의 vault 저장소. 모양, 검사, 변경 적용, 파일 글은 도메인 크레이트(`ais-domain`)에 있고 같은 이름으로
//! 다시 내놓는다. 여기에는 vault에서 읽고 쓰는 `store`가 있다.

pub use ais_domain::c4::*;

pub mod store;
