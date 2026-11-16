//! 아젠다의 색(hue) 열두 가지. 목록은 `vault/agenda.rs`에 있었고 쓰기 쪽(`write/agenda_file.rs`, `commands/agenda_write.rs`)이
//! 그 안쪽 상수를 가져다 썼다.

/// 쓸 수 있는 색(차례가 곧 「가장 적게 쓰인 색」을 고를 때의 차례다)
pub const HUES: [&str; 12] = [
    "blue", "violet", "teal", "orange", "pink", "green", "red", "amber", "lime", "cyan", "fuchsia",
    "brown",
];

/// 모르는 색이나 색이 없을 때의 기본 색
pub const DEFAULT_HUE: &str = "blue";

/// 글자가 정확히 목록의 색일 때만 참
pub fn is_hue(text: &str) -> bool {
    HUES.contains(&text)
}

/// 목록에서 색의 자리. 모르는 색은 기본 색의 자리로 센다.
pub fn position_or_default(text: &str) -> usize {
    HUES.iter()
        .position(|h| *h == text)
        .unwrap_or_else(|| HUES.iter().position(|h| *h == DEFAULT_HUE).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_the_first_hue_and_unknown_counts_as_default() {
        assert_eq!(HUES[0], DEFAULT_HUE);
        assert!(is_hue("teal"));
        assert!(!is_hue("Teal"));
        assert_eq!(position_or_default("nope"), 0);
        assert_eq!(position_or_default("red"), 6);
    }
}
