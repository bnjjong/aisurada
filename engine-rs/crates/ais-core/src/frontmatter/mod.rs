//! 노트 맨 위 머리(YAML)를 읽고 쓰는 규칙입니다.
//!
//! 읽기는 지금 Java 엔진의 SnakeYAML 설정(YAML 1.1 규칙, 암시적 시각 해석 끔, 중첩 깊이와 별칭 상한)을 따르고,
//! 쓰기는 Java 엔진의 `FrontmatterWriter`가 만드는 글과 같은 모양을 냅니다. 기존 노트를 다시 저장해도
//! 건드리지 않은 부분의 글자가 바뀌지 않게 하려는 것입니다.

mod events;
mod read;
mod value;
mod write;

pub use read::{parse, parse_value};
pub use value::Value;
pub use write::{DumpError, dump_general_map};

#[cfg(test)]
mod tests;
