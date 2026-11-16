//! aisurada vault의 규칙을 담은 라이브러리입니다.
//!
//! 호스트(Tauri 앱)와 `ais-cli`가 같은 코드를 쓰도록 UI와 프로세스 경계에 기대지 않는 규칙만 둡니다.
//! 진입점은 [`engine::Engine::call_as`]입니다.
//!
//! **바깥에 내놓는 면은 좁다.** 앱과 명령줄이 쓰는 것만 `pub`이다.
//! - `engine`: 진입점(`Engine`, `catalog`)과 명령 표 정보
//! - `error`: 오류 값(`EngineError`, `ErrorKind`)
//! - `kernel::actor`: 호출 맥락(`CallContext`, `Caller`)
//! - `json`, `text`: Java와 같은 JSON·글 규칙
//! - `vault::{facts, git, index, template, trash}`: 명령줄이 옛 응답 모양을 만들려고 읽는 vault 읽기
//! - `write::autocommit`: 앱과 MCP 서버가 도는 자동 커밋 감시
//!
//! 나머지 안쪽 모듈은 이 크레이트 안에서만 보인다. 시험과 예제가 안쪽을 직접 만져야 할 때만 `test-support` feature가 이들을 `pub`으로 연다
//! (`[dev-dependencies]`가 켠다. 앱과 명령줄의 운영 빌드에는 켜지지 않는다).

// 안쪽 모듈을 닫으면 시험과 예제만 쓰던 재수출과 항목이 「안 쓰임」으로 보인다. 운영 빌드에서만 그 경고를 끈다.
#![cfg_attr(not(feature = "test-support"), allow(unused_imports))]

/// 안쪽 모듈 선언. `test-support`일 때만 바깥에 `pub`으로 열리고, 아니면 이 크레이트 안에서만 보인다.
macro_rules! internal_mods {
    ($($name:ident),* $(,)?) => {
        $(
            #[cfg(feature = "test-support")]
            pub mod $name;
            #[cfg(not(feature = "test-support"))]
            pub(crate) mod $name;
        )*
    };
}

pub mod engine;
pub mod error;
pub mod json;
pub mod kernel;
pub mod text;
pub mod vault;
pub mod write;

internal_mods!(
    adapters,
    commands,
    frontmatter,
    model,
    note,
    ports,
    resource,
    session,
    task,
);
