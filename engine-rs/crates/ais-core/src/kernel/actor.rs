//! 호출자. 앱과 AI는 보이는 것이 다르다(AI에게는 기록 층 노트가 안 보인다).

use serde::Deserialize;

/// 호출자. 앱과 AI는 보이는 것이 다르다(AI에게는 기록 층 노트가 안 보인다).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Caller {
    #[default]
    App,
    Ai,
}
