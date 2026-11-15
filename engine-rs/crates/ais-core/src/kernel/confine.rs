//! 경로 조각 검사와 기준 폴더 가두기. 상위 폴더로 빠져나가는 경로, 빈 조각, 경로로 못 쓰는 글을 거절합니다.

use std::path::{Component, Path, PathBuf};

use crate::error::EngineError;
use crate::text::is_blank;

/// 절대 경로로 만들고 `.`과 `..`를 글자 그대로 정리한다(심볼릭 링크는 따라가지 않는다. Java `toAbsolutePath().normalize()`).
pub fn absolute_normalized(path: &Path) -> PathBuf {
    normalize(&std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()))
}

/// `.`과 `..`를 글자 그대로 정리한다. 맨 앞 뿌리를 넘어서는 `..`는 버린다.
pub fn normalize(path: &Path) -> PathBuf {
    let mut parts: Vec<Component> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match parts.last() {
                Some(Component::Normal(_)) => {
                    parts.pop();
                }
                Some(Component::RootDir) | Some(Component::Prefix(_)) => {}
                _ => parts.push(component),
            },
            other => parts.push(other),
        }
    }
    parts.iter().collect()
}

/// 경로 조각이 쓸 만한지 본다: 빈 글이거나 공백뿐이거나, 경로로 못 쓰는 글자가 들었으면 거절한다.
pub fn check_segment(base: &Path, segment: &str) -> Result<(), EngineError> {
    if is_blank(segment) {
        return Err(EngineError::invalid_argument(format!(
            "빈 경로 조각은 결합할 수 없다: 기준={}",
            absolute_normalized(base).display()
        )));
    }
    if has_unusable_path_characters(segment) {
        return Err(EngineError::invalid_argument(format!(
            "경로로 쓸 수 없는 값이다: {segment}"
        )));
    }
    Ok(())
}

/// 이어 붙인 경로가 기준 폴더 안에 있는지 본다.
pub fn check_confinement(base: &Path, target: &Path) -> Result<(), EngineError> {
    let base = absolute_normalized(base);
    let target = absolute_normalized(target);
    if !target.starts_with(&base) {
        return Err(EngineError::invalid_argument(format!(
            "기준 폴더 밖을 가리키는 경로다: {}",
            target.display()
        )));
    }
    Ok(())
}

/// NUL 글자가 들었는가. Java는 짝 없는 서로게이트도 거절했지만 Rust 글에는 그런 것이 있을 수 없다.
pub fn has_unusable_path_characters(text: &str) -> bool {
    text.contains('\0')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_like_java() {
        assert_eq!(normalize(Path::new("/a/b/../c/./d")), Path::new("/a/c/d"));
        assert_eq!(normalize(Path::new("/..")), Path::new("/"));
        assert_eq!(normalize(Path::new("a/../../b")), Path::new("../b"));
    }

    #[test]
    fn rejects_escapes_blank_and_nul() {
        let base = Path::new("/vault");
        assert!(check_confinement(base, Path::new("/vault/notes/../notes/a.md")).is_ok());
        let escape = check_confinement(base, Path::new("/vault/../etc/passwd")).unwrap_err();
        assert_eq!(
            escape.message,
            "기준 폴더 밖을 가리키는 경로다: /etc/passwd"
        );
        // 이름만 같은 이웃 폴더는 안이 아니다
        assert!(check_confinement(base, Path::new("/vault-other/a")).is_err());
        assert_eq!(
            check_segment(base, "  ").unwrap_err().message,
            "빈 경로 조각은 결합할 수 없다: 기준=/vault"
        );
        assert_eq!(
            check_segment(base, "a\0b").unwrap_err().message,
            "경로로 쓸 수 없는 값이다: a\0b"
        );
        assert!(check_segment(base, "정상 조각").is_ok());
    }
}
