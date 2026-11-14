//! 아젠다 사실층(`.aisurada/facts/<slug>.md`) 읽기. 아젠다에 대해 지금도 참인 것을 짧게 적어 두는 파일이다.

use std::fs;
use std::path::Path;

use crate::text::{cut_utf16, trim, utf16_len};

/// 사실층 글자 수 상한(UTF-16 코드 단위)
pub const LIMIT: usize = 2000;
/// 고치려고 읽을 때의 상한
pub const EDIT_LIMIT: usize = LIMIT * 5;

/// 사실층 파일의 vault 기준 상대 경로
pub fn relative_path(agenda_slug: &str) -> String {
    format!(".aisurada/facts/{agenda_slug}.md")
}

/// 읽은 사실층. `facts`가 없고 `too_large`가 `false`면 파일이 없거나 비어 있다.
#[derive(Debug, Default)]
pub struct Facts {
    pub facts: Option<String>,
    /// 상한을 넘어 싣지 않았다
    pub too_large: bool,
}

/// 고칠 때 보여 줄 사실층을 읽는다(상한 [`EDIT_LIMIT`]).
pub fn read_for_edit(vault_root: &Path, agenda_slug: &str) -> Facts {
    read(vault_root, agenda_slug, EDIT_LIMIT)
}

fn read(vault_root: &Path, agenda_slug: &str, limit: usize) -> Facts {
    if agenda_slug.is_empty() {
        return Facts::default();
    }
    let file = vault_root.join(relative_path(agenda_slug));
    if !fs::metadata(&file).map(|m| m.is_file()).unwrap_or(false) {
        return Facts::default();
    }
    let Ok(bytes) = fs::read(&file) else {
        return Facts::default();
    };
    // 상한을 넘는지 알 만큼(상한 + 1 코드 단위)만 본다
    let text = String::from_utf8_lossy(&bytes);
    let head = cut_utf16(&text, limit + 1);
    let trimmed = trim(&head);
    if trimmed.is_empty() {
        return Facts::default();
    }
    if utf16_len(trimmed) > limit {
        return Facts {
            facts: None,
            too_large: true,
        };
    }
    Facts {
        facts: Some(trimmed.to_string()),
        too_large: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_trims_and_limits() {
        let dir = std::env::temp_dir().join(format!("ais-facts-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".aisurada/facts")).unwrap();
        fs::write(dir.join(".aisurada/facts/a.md"), "\n  사실이다  \n\n").unwrap();
        fs::write(dir.join(".aisurada/facts/blank.md"), " \n ").unwrap();
        fs::write(
            dir.join(".aisurada/facts/big.md"),
            "가".repeat(EDIT_LIMIT + 1),
        )
        .unwrap();
        assert_eq!(read_for_edit(&dir, "a").facts.as_deref(), Some("사실이다"));
        assert!(read_for_edit(&dir, "blank").facts.is_none());
        let big = read_for_edit(&dir, "big");
        assert!(big.too_large && big.facts.is_none());
        assert!(read_for_edit(&dir, "none").facts.is_none());
        let _ = fs::remove_dir_all(&dir);
    }
}
