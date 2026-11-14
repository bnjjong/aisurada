//! aisurada vault의 규칙을 담은 라이브러리입니다.
//!
//! 호스트(Tauri 앱)와 `ais-cli`가 같은 코드를 쓰도록 UI와 프로세스 경계에 기대지 않는 규칙만 둡니다.
//! 진입점은 [`engine::Engine::call`]입니다.

pub mod commands;
pub mod engine;
pub mod error;
pub mod frontmatter;
pub mod model;
pub mod text;
pub mod vault;
