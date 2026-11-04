//! 10장 10.6: 세션 입력창에 붙인 그림을 앱 데이터 폴더의 `session-images/<세션 id>/` 에 쓴다.
//! 바이트는 IPC 본문 그대로, 세션 id와 이름은 헤더 `x-wl-session-id`, `x-wl-name` 으로 받는다(2부 설명서 3.4).
use crate::ipc;
use std::path::{Path, PathBuf};
use tauri::Manager;

const MAX_IMAGE_BYTES: usize = 5_000_000;

fn is_valid_uuid(id: &str) -> bool {
    let parts: Vec<&str> = id.split('-').collect();
    if parts.len() != 5 {
        return false;
    }
    let lens = [8, 4, 4, 4, 12];
    for (i, part) in parts.iter().enumerate() {
        if part.len() != lens[i] || !part.chars().all(|c| c.is_ascii_hexdigit()) {
            return false;
        }
    }
    true
}

/// 폴더를 만들기 전에 보는 앞 3가지. 거절된 회차가 빈 폴더를 남기지 않게 한다.
fn validate(session_id: &str, name: &str, length: usize) -> Result<(), String> {
    // 1. 세션 id UUID 검사
    if !is_valid_uuid(session_id) {
        return Err(format!("세션 id 가 UUID 꼴이 아니에요: {session_id}"));
    }
    // 2. 그림 파일 이름 검사. 2부 설명서 7.1의 그림 이름 검사 그대로다
    crate::files::check_attachment_name(name)?;
    // 3. 크기 상한 검사 (5,000,000바이트)
    if length > MAX_IMAGE_BYTES {
        return Err(format!(
            "그림이 너무 커요: {length} 바이트. 상한은 5000000 바이트예요"
        ));
    }
    Ok(())
}

fn write_image(app_data_dir: &Path, session_id: &str, name: &str, bytes: &[u8]) -> Result<String, String> {
    let session_dir: PathBuf = app_data_dir.join("session-images").join(session_id);
    let target_file = session_dir.join(name);

    // 4. 이어 붙인 자리가 그 세션 폴더 안인지 검사
    if !target_file.starts_with(&session_dir) {
        return Err(format!(
            "쓰려는 자리가 세션 그림 폴더 밖이에요: {}",
            target_file.to_string_lossy()
        ));
    }

    if !session_dir.exists() {
        std::fs::create_dir_all(&session_dir)
            .map_err(|e| format!("세션 그림 폴더를 못 만들었어요: {e}"))?;
    }
    std::fs::write(&target_file, bytes).map_err(|e| format!("그림을 못 썼어요: {e}"))?;
    Ok(target_file.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn write_session_image(
    app: tauri::AppHandle,
    request: tauri::ipc::Request<'_>,
) -> Result<String, String> {
    let session_id = ipc::header(&request, "session-id")?;
    let name = ipc::header(&request, "name")?;
    let bytes = ipc::body(&request)?;
    validate(&session_id, &name, bytes.len())?;
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("앱 데이터 폴더를 못 찾았어요: {e}"))?;
    tauri::async_runtime::spawn_blocking(move || write_image(&app_data_dir, &session_id, &name, &bytes))
        .await
        .map_err(|e| format!("그림 쓰기가 끊겼어요: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uuid_validation() {
        assert!(is_valid_uuid("3f2a9c4e-1b2d-4c3e-8f40-00000000a001"));
        assert!(is_valid_uuid("F1A50000-0000-4000-8000-000000000001"));
        assert!(!is_valid_uuid("invalid-uuid"));
        assert!(!is_valid_uuid("../3f2a9c4e-1b2d-4c3e-8f40-00000000a001"));
    }

    #[test]
    fn image_name_follows_the_part_two_attachment_rule() {
        let id = "3f2a9c4e-1b2d-4c3e-8f40-00000000a001";
        assert!(validate(id, "ask-123.png", 1).is_ok());
        assert!(validate(id, "photo.JPEG", 1).is_ok());
        // 점 둘이 이름 한가운데 들어도 파일 이름 하나라서 받는다. 120바이트 상한도 없다
        assert!(validate(id, "a..b.png", 1).is_ok());
        assert!(validate(id, &format!("{}.png", "a".repeat(200)), 1).is_ok());
        // 점으로 시작하는 이름과 빗금과 .. 와 그림 아닌 확장자는 거절한다
        assert!(validate(id, ".x.png", 1).is_err());
        assert!(validate(id, "../escape.png", 1).is_err());
        assert!(validate(id, "a/b.png", 1).is_err());
        assert!(validate(id, "script.sh", 1).is_err());
        assert!(validate(id, "noext", 1).is_err());
    }

    #[test]
    fn rejects_before_creating_the_folder_and_keeps_the_limit_boundary() {
        let id = "3f2a9c4e-1b2d-4c3e-8f40-00000000a001";
        assert!(validate(id, "a.png", 5_000_000).is_ok());
        assert!(validate(id, "a.png", 5_000_001).unwrap_err().contains("그림이 너무 커요"));
        assert!(validate("../x", "a.png", 1).unwrap_err().contains("UUID 꼴이 아니에요"));
        let temp = tempfile::tempdir().unwrap();
        let path = write_image(temp.path(), id, "a..b.png", b"png").unwrap();
        assert!(path.ends_with("session-images/3f2a9c4e-1b2d-4c3e-8f40-00000000a001/a..b.png"));
        assert_eq!(std::fs::read(path).unwrap(), b"png");
    }
}
