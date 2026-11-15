//! id 모양 모음. `접두어 두 글자 + 36진수 밀리초 + '-' + 16진수 8자리`(예: `ndmwh21ips-0123abcd`)인 id의 정식 모양을 한 곳에 둔다.
//!
//! `tr`(휴지통)과 `ag`(아젠다)의 모양은 한때 영역마다 따로 적혀 있었다(`vault/trash.rs`와 `commands/trash_write.rs`, `vault/agenda.rs`와
//! `vault/trash.rs`). 모양은 새 id를 만드는 `write::apparatus::next_id`와 짝이라 한 곳에서 고쳐야 한다.
//! 노트 id(`nd`)와 후속 노트 id(`sn`)는 글 도구 모듈 `text`에 있어 여기서 다시 내보낸다.

use std::sync::LazyLock;

use regex::Regex;

pub use crate::text::{NOTE_ID, SUB_NOTE_ID};

/// 태스크 id: `tk…-xxxxxxxx`
pub static TASK_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^tk[0-9a-z]+-[0-9a-f]{8}$").unwrap());
/// 자원(스킬 `sk`, 에이전트 `ap`) id
pub static RESOURCE_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(sk|ap)[0-9a-z]+-[0-9a-f]{8}$").unwrap());
/// 승인 대기 변경 id: `pc…-xxxxxxxx`
pub static CHANGE_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^pc[0-9a-z]+-[0-9a-f]{8}$").unwrap());
/// 휴지통 항목 id: `tr…-xxxxxxxx`
pub static TRASH_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^tr[0-9a-z]+-[0-9a-f]{8}$").unwrap());
/// 코멘트 id: `cm…-xxxxxxxx`
pub static COMMENT_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^cm[0-9a-z]+-[0-9a-f]{8}$").unwrap());
/// 아젠다 id: `ag…-xxxxxxxx`
pub static AGENDA_ID: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^ag[0-9a-z]+-[0-9a-f]{8}$").unwrap());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_shapes_accept_what_next_id_makes_and_reject_other_prefixes() {
        // `write::apparatus::next_id`가 만드는 모양(접두어 + 36진수 밀리초 + '-' + 16진수 8자리). 커널은 영역을 쓰지 않으므로 손으로 만든다.
        let make = |prefix: &str| format!("{prefix}mwh21ips-0123abcd");
        for (re, prefix, other) in [
            (&*NOTE_ID, "nd", "sn"),
            (&*SUB_NOTE_ID, "sn", "nd"),
            (&*TASK_ID, "tk", "nd"),
            (&*CHANGE_ID, "pc", "tk"),
            (&*TRASH_ID, "tr", "ag"),
            (&*COMMENT_ID, "cm", "tr"),
            (&*AGENDA_ID, "ag", "tr"),
        ] {
            assert!(re.is_match(&make(prefix)), "{prefix}");
            assert!(!re.is_match(&make(other)), "{prefix} vs {other}");
        }
        assert!(RESOURCE_ID.is_match(&make("sk")) && RESOURCE_ID.is_match(&make("ap")));
        assert!(!RESOURCE_ID.is_match(&make("pc")));
    }
}
