//! 10장 10.8: 세션 입력창의 받아쓰기 단추. AppKit 의 표준 동작 `startDictation:` 을 응답 사슬로 보낸다.
//! 사슬 끝의 NSApplication 이 받아, 그때 포커스가 든 칸에 macOS 받아쓰기를 켠다.

/// 받아쓰기를 켠다. 사슬에서 그 동작을 받은 쪽이 있었는지를 돌려준다.
/// AppKit 은 main 스레드에서만 부른다. main 스레드에서 불렸으면 그 자리에서 보내고,
/// 아니면 main 스레드로 넘겨 5초까지 답을 기다린다. macOS 가 아닌 판에서는 거절한다.
#[tauri::command]
pub fn start_dictation(app: tauri::AppHandle) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        if objc2::MainThreadMarker::new().is_some() {
            return Ok(send_start_dictation());
        }
        let (tx, rx) = std::sync::mpsc::channel();
        app.run_on_main_thread(move || {
            let _ = tx.send(send_start_dictation());
        })
        .map_err(|e| format!("받아쓰기를 메인 스레드로 못 넘겼다: {e}"))?;
        rx.recv_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| format!("받아쓰기 답을 못 받았다: {e}"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err("받아쓰기 단추는 macOS 에서만 돈다".to_string())
    }
}

/// 대상과 보낸 쪽을 둘 다 비운 채 `startDictation:` 을 응답 사슬로 보낸다. main 스레드에서만 부른다.
#[cfg(target_os = "macos")]
fn send_start_dictation() -> bool {
    use objc2::{class, msg_send, runtime::AnyObject, sel};
    unsafe {
        let application: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
        if application.is_null() {
            return false;
        }
        msg_send![
            application,
            sendAction: sel!(startDictation:),
            to: std::ptr::null::<AnyObject>(),
            from: std::ptr::null::<AnyObject>()
        ]
    }
}
