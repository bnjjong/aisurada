//! Chapter 7: image attachment writes and guarded opening in the default application.
use crate::{ipc, lifecycle::HostState};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};
use tauri::{Runtime, State};
use tauri_plugin_opener::OpenerExt;
const IMAGE_EXTENSIONS: &[&str] = &["svg", "png", "jpg", "jpeg", "gif", "webp", "avif", "bmp"];
const LIMIT: usize = 5_000_000;

#[tauri::command]
pub async fn write_attachment_bytes(
    state: State<'_, HostState>,
    request: tauri::ipc::Request<'_>,
) -> Result<String, String> {
    let note_dir = ipc::header(&request, "note-dir")?;
    let name = ipc::header(&request, "name")?;
    let bytes = ipc::body(&request)?;
    let vault = state
        .vault
        .lock()
        .map_err(|_| "vault 잠금을 얻지 못했다")?
        .clone()
        .ok_or("vault가 아직 열리지 않았어요")?;
    tauri::async_runtime::spawn_blocking(move || write_attachment(&vault, &note_dir, &name, &bytes))
        .await
        .map_err(|e| format!("첨부 쓰기가 끊겼어요: {e}"))?
}
fn canonical_vault(vault: &str) -> Result<PathBuf, String> {
    Path::new(vault)
        .canonicalize()
        .map_err(|e| format!("vault 경로 확인 실패: {e}"))
}
fn write_attachment(
    vault: &str,
    note_dir: &str,
    name: &str,
    bytes: &[u8],
) -> Result<String, String> {
    let vault = canonical_vault(vault)?;
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(format!(
            "첨부 이름은 파일 이름 하나여야 해요(빗금과 `..` 는 못 써요): {name}"
        ));
    }
    if name.starts_with('.') {
        return Err(format!("첨부 이름은 점으로 시작할 수 없어요: {name}"));
    }
    let extension = Path::new(name)
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !IMAGE_EXTENSIONS.contains(&extension.as_str()) {
        return Err(format!("그림 파일이 아니에요(쓸 수 있는 확장자: svg, png, jpg, jpeg, gif, webp, avif, bmp): {name}"));
    }
    let note_dir = Path::new(note_dir)
        .canonicalize()
        .map_err(|e| format!("노트 폴더를 찾을 수 없어요: {note_dir} ({e})"))?;
    if !note_dir.starts_with(vault.join("notes")) {
        return Err(format!(
            "노트 폴더가 vault 의 notes/ 밖이에요: {}",
            note_dir.display()
        ));
    }
    let attach = note_dir.join("attach");
    let target = attach.join(name);
    if !target.starts_with(&attach) {
        return Err(format!(
            "쓰려는 자리가 attach/ 밖이에요: {}",
            target.display()
        ));
    }
    if bytes.len() > LIMIT {
        return Err(format!(
            "그림이 너무 커요: {} 바이트. 상한은 5000000 바이트예요",
            bytes.len()
        ));
    }
    let parent = target.parent().ok_or("첨부를 둘 폴더를 정하지 못했어요")?;
    fs::create_dir_all(parent).map_err(|e| format!("attach 폴더를 못 만들었어요: {e}"))?;
    fs::write(&target, bytes).map_err(|e| format!("첨부를 못 썼어요: {e}"))?;
    let relative = target
        .strip_prefix(&vault)
        .map_err(|_| "저장한 경로가 vault 밖이에요")?;
    Ok(relative.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn open_in_default_app<R: Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, HostState>,
    path: String,
) -> Result<(), String> {
    let vault = state
        .vault
        .lock()
        .map_err(|_| "vault 잠금을 얻지 못했다")?
        .clone()
        .ok_or("vault가 아직 열리지 않았어요")?;
    let target = open_target(&vault, &path)?;
    app.opener()
        .open_path(target.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}
fn open_target(vault: &str, path: &str) -> Result<PathBuf, String> {
    let vault = canonical_vault(vault)?;
    let target = Path::new(path)
        .canonicalize()
        .map_err(|_| "열려는 경로를 찾을 수 없거나 vault 밖이에요")?;
    if !target.starts_with(&vault) {
        return Err("열려는 경로가 현재 vault 밖이라 거부했어요".into());
    }
    if is_executable(&target) {
        return Err(format!("실행 파일은 열지 않아요: {}", target.display()));
    }
    Ok(target)
}
fn is_executable(path: &Path) -> bool {
    if path.is_dir() {
        return path
            .file_name()
            .and_then(|v| v.to_str())
            .is_some_and(|v| v.to_ascii_lowercase().ends_with(".app"));
    }
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0) {
            return true;
        }
    }
    path.extension().and_then(|v| v.to_str()).is_some_and(|v| {
        ["command", "tool", "terminal", "jar", "py"].contains(&v.to_ascii_lowercase().as_str())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (tempfile::TempDir, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("notes/demo/root");
        fs::create_dir_all(&root).unwrap();
        (temp, root)
    }
    #[test]
    fn images_have_exact_limit_empty_bytes_and_overwrite_without_format_sniffing() {
        let (temp, note) = setup();
        let vault = temp.path().to_str().unwrap();
        let note = note.to_str().unwrap();
        for extension in IMAGE_EXTENSIONS {
            let name = format!("한글.{}", extension.to_uppercase());
            let rel = write_attachment(vault, note, &name, &[0, 255, 7]).unwrap();
            assert_eq!(fs::read(temp.path().join(rel)).unwrap(), vec![0, 255, 7]);
        }
        let rel = write_attachment(vault, note, "full.png", &vec![0; LIMIT]).unwrap();
        assert_eq!(
            fs::metadata(temp.path().join(&rel)).unwrap().len(),
            LIMIT as u64
        );
        write_attachment(vault, note, "full.png", &[]).unwrap();
        assert!(fs::read(temp.path().join(rel)).unwrap().is_empty());
    }
    #[test]
    fn path_and_extension_errors_precede_size_and_rejection_does_not_create_attach() {
        let (temp, note) = setup();
        let vault = temp.path().to_str().unwrap();
        let note = note.to_str().unwrap();
        let large = vec![0; LIMIT + 1];
        for name in ["", ".", "..", "./x.png", "../x.png", "a/x.png", "/x.png"] {
            assert!(write_attachment(vault, note, name, &large)
                .unwrap_err()
                .contains("파일 이름 하나"));
        }
        assert!(write_attachment(vault, note, ".x.png", &large)
            .unwrap_err()
            .contains("점으로 시작"));
        assert!(write_attachment(vault, note, "a.command", &large)
            .unwrap_err()
            .contains("그림 파일"));
        assert!(write_attachment(vault, note, "a.png", &large)
            .unwrap_err()
            .contains("5000001 바이트"));
        assert!(!Path::new(note).join("attach").exists());
        assert!(
            write_attachment(vault, "/missing-note-dir", "a.png", &large)
                .unwrap_err()
                .contains("노트 폴더를 찾을")
        );
        assert!(write_attachment("/missing-vault", note, "../x", &large)
            .unwrap_err()
            .contains("vault 경로 확인"));
    }
    #[test]
    fn canonical_notes_boundary_refuses_trash_sibling_and_symlink_escape() {
        let (temp, note) = setup();
        let outside = tempfile::tempdir().unwrap();
        let trash = temp.path().join(".withlog/trash/files/n");
        let sibling = temp.path().join("notes-other");
        fs::create_dir_all(&trash).unwrap();
        fs::create_dir(&sibling).unwrap();
        for dir in [&trash, &sibling, outside.path()] {
            assert!(write_attachment(
                temp.path().to_str().unwrap(),
                dir.to_str().unwrap(),
                "a.png",
                b"x"
            )
            .unwrap_err()
            .contains("notes/ 밖"));
        }
        #[cfg(unix)]
        {
            let alias = note.join("outside");
            std::os::unix::fs::symlink(outside.path(), &alias).unwrap();
            assert!(write_attachment(
                temp.path().to_str().unwrap(),
                alias.to_str().unwrap(),
                "a.png",
                b"x"
            )
            .is_err());
        }
    }
    #[test]
    fn notes_itself_and_agenda_are_allowed_without_note_marker() {
        let (temp, _) = setup();
        for rel in ["notes", "notes/demo"] {
            let result = write_attachment(
                temp.path().to_str().unwrap(),
                temp.path().join(rel).to_str().unwrap(),
                "x.png",
                b"x",
            )
            .unwrap();
            assert_eq!(result, format!("{rel}/attach/x.png"));
        }
    }
    #[test]
    fn open_target_canonicalizes_and_rejects_only_documented_executables() {
        let (temp, note) = setup();
        let vault = temp.path().to_str().unwrap();
        let outside = tempfile::tempdir().unwrap();
        for name in ["x.pdf", "x.sh", "x.pkg", "x.html", "x.svg"] {
            let path = note.join(name);
            fs::write(&path, b"data").unwrap();
            assert_eq!(
                open_target(vault, path.to_str().unwrap()).unwrap(),
                path.canonicalize().unwrap()
            );
        }
        for name in ["x.COMMAND", "x.ToOl", "x.terminal", "x.jar", "x.PY"] {
            let path = note.join(name);
            fs::write(&path, b"data").unwrap();
            assert!(open_target(vault, path.to_str().unwrap())
                .unwrap_err()
                .contains("실행 파일"));
        }
        let app = note.join("program.APP");
        fs::create_dir(&app).unwrap();
        assert!(open_target(vault, app.to_str().unwrap()).is_err());
        assert_eq!(
            open_target(vault, vault).unwrap(),
            temp.path().canonicalize().unwrap()
        );
        assert!(open_target(vault, outside.path().to_str().unwrap())
            .unwrap_err()
            .contains("현재 vault 밖"));
        assert!(open_target(vault, note.join("missing").to_str().unwrap())
            .unwrap_err()
            .contains("찾을 수 없거나"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for bits in [0o100, 0o010, 0o001] {
                let path = note.join("executable");
                fs::write(&path, b"script").unwrap();
                fs::set_permissions(&path, fs::Permissions::from_mode(0o600 | bits)).unwrap();
                assert!(open_target(vault, path.to_str().unwrap()).is_err());
            }
            let alias = note.join("innocent.pdf");
            std::os::unix::fs::symlink(&app, &alias).unwrap();
            assert!(open_target(vault, alias.to_str().unwrap())
                .unwrap_err()
                .contains("실행 파일"));
        }
    }
    #[test]
    fn direct_write_and_directory_creation_errors_are_reported() {
        let (temp, note) = setup();
        fs::write(note.join("attach"), b"file").unwrap();
        assert!(write_attachment(
            temp.path().to_str().unwrap(),
            note.to_str().unwrap(),
            "x.png",
            b"x"
        )
        .unwrap_err()
        .contains("attach 폴더"));
        fs::remove_file(note.join("attach")).unwrap();
        fs::create_dir_all(note.join("attach/x.png")).unwrap();
        assert!(write_attachment(
            temp.path().to_str().unwrap(),
            note.to_str().unwrap(),
            "x.png",
            b"x"
        )
        .unwrap_err()
        .contains("첨부를 못 썼"));
    }
    #[test]
    #[cfg(unix)]
    fn direct_write_keeps_existing_target_symlink_semantics() {
        let (temp, note) = setup();
        let outside = tempfile::tempdir().unwrap();
        let external = outside.path().join("target");
        fs::write(&external, b"old").unwrap();
        fs::create_dir(note.join("attach")).unwrap();
        std::os::unix::fs::symlink(&external, note.join("attach/x.png")).unwrap();
        let relative = write_attachment(
            temp.path().to_str().unwrap(),
            note.to_str().unwrap(),
            "x.png",
            b"new",
        )
        .unwrap();
        assert_eq!(relative, "notes/demo/root/attach/x.png");
        assert_eq!(fs::read(&external).unwrap(), b"new");
        assert!(note.join("attach/x.png").is_symlink());
    }
}
