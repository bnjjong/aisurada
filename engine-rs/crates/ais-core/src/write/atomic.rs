//! 원자적 쓰기: 부모 폴더를 만들고, 임시 파일에 쓰고 디스크에 내린 뒤, 이름을 바꿔 한 번에 교체합니다.
//! 중간에 죽어도 대상 파일은 옛 내용이거나 새 내용이고 반쯤 쓴 파일이 보이지 않습니다.

use crate::ports::fs;
use std::path::Path;

use super::apparatus::{next_random_u64, to_base36_u64};
use super::confine::absolute_normalized;
pub(crate) use crate::error::EngineError;

/// `bytes`를 `target`에 원자적으로 쓴다. 실패하면 임시 파일을 지우고 `AIS-500`이다.
pub fn write_atomic(target: &Path, bytes: &[u8]) -> Result<(), EngineError> {
    let target = absolute_normalized(target);
    let parent = target.parent();
    // 부모 자리에 폴더가 아닌 것이 있어도 만들기를 시도해서 같은 문구로 선다
    if let Some(parent) = parent
        && !parent.is_dir()
        && fs::create_dir_all(parent).is_err()
    {
        return Err(EngineError::internal(format!(
            "폴더를 만들지 못했다: {}",
            parent.display()
        )));
    }
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    let mut temp = None;
    for _ in 0..3 {
        let candidate = format!("{name}.{}.tmp", to_base36_u64(next_random_u64()));
        let path = parent.map_or_else(
            || Path::new(&candidate).to_path_buf(),
            |p| p.join(&candidate),
        );
        // 심볼릭 링크도 있는 것으로 센다
        if fs::symlink_metadata(&path).is_err() {
            temp = Some(path);
            break;
        }
    }
    let Some(temp) = temp else {
        return Err(EngineError::internal(format!(
            "임시 파일 이름을 3 번 골랐는데 다 이미 있었다: {}",
            parent.map(|p| p.display().to_string()).unwrap_or_default()
        )));
    };

    if fs::write_new(&temp, bytes).is_err() {
        let _ = fs::remove_file(&temp);
        return Err(failed(&target));
    }
    // 원자적 이름 바꾸기만 쓴다. 대상 자리에 빈 폴더가 있어도 파일로 바꿔 치우지 않고 실패한다.
    if fs::rename(&temp, &target).is_err() {
        let _ = fs::remove_file(&temp);
        return Err(failed(&target));
    }
    Ok(())
}

fn failed(target: &Path) -> EngineError {
    EngineError::internal(format!("파일을 쓰지 못했다: {}", target.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn scratch() -> std::path::PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ais-atomic-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn creates_parents_replaces_and_leaves_no_temp_files() {
        let dir = scratch();
        let target = dir.join("a/b/note.md");
        write_atomic(&target, b"one").unwrap();
        write_atomic(&target, b"two").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"two");
        let names: Vec<_> = fs::read_dir(target.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names, ["note.md"]);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn fails_when_target_is_a_directory_or_parent_is_a_file() {
        let dir = scratch();
        fs::create_dir_all(dir.join("occupied")).unwrap();
        let err = write_atomic(&dir.join("occupied"), b"x").unwrap_err();
        assert_eq!(err.kind, ErrorKind::Internal);
        assert!(
            err.message.starts_with("파일을 쓰지 못했다: "),
            "{}",
            err.message
        );
        assert!(dir.join("occupied").is_dir(), "빈 폴더를 지우지 않는다");
        assert_eq!(
            fs::read_dir(&dir).unwrap().count(),
            1,
            "임시 파일이 남지 않는다"
        );

        fs::write(dir.join("plain"), b"x").unwrap();
        let err = write_atomic(&dir.join("plain/child.md"), b"x").unwrap_err();
        assert!(
            err.message.starts_with("폴더를 만들지 못했다: "),
            "{}",
            err.message
        );
        fs::remove_dir_all(&dir).unwrap();
    }
}
