//! 엔진의 도메인 크레이트. 노트, 태스크, 자원의 순수한 규칙만 둔다.
//!
//! - `note`: 노트 머리(`meta`), 편집 도장 판정(`front`), 자리(`placement`), slug, 새 노트·고치기·후속 노트·코멘트 규칙
//! - `task`: 태스크 모양(`format`), 상태 전이(`transitions`), 집합체와 이벤트(`aggregate`)
//! - `resource`: 스킬·에이전트 정의 모양(`format`), 승인 정책과 믿을 수 있는 상태(`aggregate`), 인자 읽기와 참조 계산(`rules`)
//!
//! 이 크레이트는 `ais-kernel`만 의존한다. 그래서 도메인이 포트(`ports`)나 어댑터, vault 읽기·쓰기를 쓰면 컴파일이 안 된다.
//! vault에서 읽고 쓰는 저장소 부분(`task::store`, `resource::store` 따위)은 `ais-core`에 남아 있다.

// 바닥 크레이트의 모듈을 같은 이름으로 가져와 `crate::error`, `crate::text` 같은 경로가 그대로 통한다.
pub(crate) use ais_kernel::{error, frontmatter, json, kernel, snake_dump, text};

pub mod note;
pub mod resource;
pub mod task;
