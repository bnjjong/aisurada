//! Chapter 8: inform every window of a self-write; caller identity is host supplied.
use crate::{
    ipc::{SelfWrite, SELF_WRITE},
    lifecycle::HostState,
};
use tauri::{Emitter, Manager, Runtime, State, WebviewWindow};

fn validate_relative(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.len() > 2048
        || path.starts_with('/')
        || path.split('/').any(|part| matches!(part, "" | "." | ".."))
    {
        return Err(format!("vault 안의 상대 경로가 아니에요: {path}"));
    }
    Ok(())
}
#[tauri::command]
pub async fn announce_self_write<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, HostState>,
    root: String,
    rel_path: String,
) -> Result<(), String> {
    validate_relative(&rel_path)?;
    let current = state
        .vault
        .lock()
        .map_err(|_| "vault 잠금을 얻지 못했다")?
        .clone();
    if current.as_deref() != Some(root.as_str()) {
        return Ok(());
    }
    window
        .app_handle()
        .emit(
            SELF_WRITE,
            SelfWrite {
                root,
                rel_path,
                from: window.label().into(),
            },
        )
        .map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validation_is_utf8_byte_based_and_only_checks_documented_components() {
        for path in ["", "/a", "a/", "a//b", "a/./b", "a/../b", ".", ".."] {
            assert!(validate_relative(path).is_err());
        }
        assert!(validate_relative(&"a".repeat(2048)).is_ok());
        assert!(validate_relative(&"a".repeat(2049)).is_err());
        assert!(validate_relative(&"한".repeat(682)).is_ok());
        assert!(validate_relative(&"한".repeat(683)).is_err());
        for path in [
            ".withlog/facts/a.md",
            "uncreated/place",
            "a\\b",
            "a\0b",
            "a\nb",
            "　",
        ] {
            assert!(validate_relative(path).is_ok());
        }
    }
}
