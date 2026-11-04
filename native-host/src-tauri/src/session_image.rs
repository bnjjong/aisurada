use tauri::Manager;

const MAX_IMAGE_BYTES: usize = 5_000_000;
const ALLOWED_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "avif", "svg", "bmp"];

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

fn is_valid_image_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 120 || name.contains('/') || name.contains('\\') || name.contains("..") {
        return false;
    }
    if let Some(pos) = name.rfind('.') {
        let ext = &name[pos + 1..].to_ascii_lowercase();
        ALLOWED_EXTS.contains(&ext.as_str())
    } else {
        false
    }
}

#[tauri::command]
pub fn write_session_image(
    app: tauri::AppHandle,
    session_id: String,
    name: String,
    bytes: Vec<u8>,
) -> Result<String, String> {
    // 1. 세션 id UUID 검사
    if !is_valid_uuid(&session_id) {
        return Err(format!("세션 id 가 UUID 꼴이 아니에요: {session_id}"));
    }

    // 2. 그림 파일 이름 검사
    if !is_valid_image_name(&name) {
        return Err(format!("그림 파일 이름이 올바르지 않아요: {name}"));
    }

    // 3. 크기 상한 검사 (5,000,000바이트)
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(format!(
            "그림이 너무 커요: {} 바이트. 상한은 5000000 바이트예요",
            bytes.len()
        ));
    }

    // 앱 데이터 디렉토리 결정
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("앱 데이터 폴더를 못 찾았어요: {e}"))?;

    let session_dir = app_data_dir.join("session-images").join(&session_id);
    let target_file = session_dir.join(&name);

    // 4. 경로가 세션 폴더 안인지 검사 (Path traversal 방지)
    if !target_file.starts_with(&session_dir) {
        return Err(format!(
            "쓰려는 자리가 세션 그림 폴더 밖이에요: {}",
            target_file.to_string_lossy()
        ));
    }

    // 5. 폴더 생성 및 쓰기
    if !session_dir.exists() {
        std::fs::create_dir_all(&session_dir)
            .map_err(|e| format!("세션 그림 폴더를 못 만들었어요: {e}"))?;
    }

    std::fs::write(&target_file, &bytes)
        .map_err(|e| format!("그림을 못 썼어요: {e}"))?;

    Ok(target_file.to_string_lossy().to_string())
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
    fn test_image_name_validation() {
        assert!(is_valid_image_name("ask-123.png"));
        assert!(is_valid_image_name("photo.JPEG"));
        assert!(!is_valid_image_name("../escape.png"));
        assert!(!is_valid_image_name("script.sh"));
        assert!(!is_valid_image_name("noext"));
    }
}
