//! 엔진의 인프라 크레이트. 바깥 세상에 닿는 약속(포트)과 그 기본 구현(어댑터)을 둔다.
//!
//! - `ports`: 시계, 난수, 환경 변수, 잠금, git, sqlite3, 저장소(`VaultStore`)의 약속과, 현재 스레드에 거는 약속 묶음(`Ports`)
//! - `adapters`: 위 약속의 기본 구현(시스템 시계, 프로세스 환경 변수, flock 잠금, git·sqlite3 명령줄, 로컬 디스크 저장소)
//!
//! 포트와 어댑터를 한 크레이트에 둔 까닭: 약속 묶음의 기본값(`ports`)이 기본 어댑터(`adapters`)를 가리키고 어댑터는 약속을 구현해서, 둘이
//! 서로를 가리킨다. 한 크레이트 안의 모듈이라 이 순환이 허용되고, 나눠 놓으면 기본값을 밖에서 꽂아 줘야 한다(그러면 아무도 꽂지 않은 스레드가 죽는다).
//! 이 크레이트는 `ais-kernel`만 의존하므로 도메인(`ais-domain`)과 엔진(`ais-core`)을 보지 못한다.

// 바닥 크레이트의 모듈을 같은 이름으로 가져와 `crate::error`, `crate::kernel` 같은 경로가 그대로 통한다.
pub(crate) use ais_kernel::{error, kernel, text};

pub mod adapters;
pub mod ports;
