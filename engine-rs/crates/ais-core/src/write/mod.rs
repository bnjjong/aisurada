//! 쓰기 명령이 함께 쓰는 공통 기반입니다.
//!
//! - [`apparatus`]: 시험용 시계와 난수, id 발급
//! - [`confine`]: 경로 조각 검사와 기준 폴더 가두기
//! - [`atomic`]: 임시 파일에 쓰고 이름을 바꿔 한 번에 교체하는 쓰기
//! - [`lock`]: vault별 쓰기 잠금(프로세스 안 줄 세우기와 프로세스 간 파일 잠금)
//! - [`precondition`]: 고치기 전 확인 값(`f1`) 검사
//! - [`marker`]: 워크스페이스 표식 검사
//! - [`autocommit`]: git 자동 커밋

pub mod apparatus;
pub mod atomic;
pub mod autocommit;
pub mod confine;
pub mod lock;
pub mod marker;
pub mod precondition;
