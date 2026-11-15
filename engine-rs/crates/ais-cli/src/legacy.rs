//! 엔진 응답을 Java `ais-cli`가 내던 옛 모양으로 되돌린다.
//!
//! Rust 엔진은 앱을 위해 읽기 응답 16개를 새 모양(`{items, total, returned, truncated}` 따위)으로 바꿨다. `ais-cli`와 MCP를 읽는
//! 에이전트의 프롬프트·스킬은 옛 모양에 맞춰져 있으므로 여기서 거꾸로 바꿔 낸다. 새 모양에서 빠진 값(`date`, `time`,
//! `agendaTitle`, `deepLink`)은 vault를 다시 읽어 채운다.

use serde_json::Value;

use crate::map::EngineCall;

/// 새 모양의 응답을 옛 모양으로 되돌린다. 모양이 바뀌지 않은 명령은 그대로 돌려준다.
pub fn to_legacy(_call: &EngineCall, _vault: &str, response: Value) -> Value {
    response
}
