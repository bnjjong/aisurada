#[tauri::command]
pub fn start_dictation() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        // macOS 시스템 받아쓰기 키보드 시그널 전송 시도
        let script = r#"
            tell application "System Events"
                key code 63
                delay 0.1
                key code 63
            end tell
        "#;
        let _ = std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output();
    }
    Ok(())
}
