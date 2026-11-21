//! 경로 조각 검사와 기준 폴더 가두기. 상위 폴더로 빠져나가는 경로, 빈 조각, 경로로 못 쓰는 글을 거절합니다.

use std::ffi::OsString;
use std::io;
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

/// [`check_confinement`]에 더해, 심볼릭 링크를 푼 실제 경로로도 기준 폴더 안인지 본다.
///
/// 글자만 정리하는 [`check_confinement`]는 `vault/notes/hack -> /바깥` 같은 링크를 지나가는 `vault/notes/hack/a.md`를 안으로 본다.
/// 그 위에서 쓰기나 지우기를 하면 vault 밖 파일이 바뀐다. 그래서 `target`과 `base`를 같은 방법으로 풀어 견준다.
/// 풀기는 파일을 만지는 일이라 이 크레이트가 하지 않고, 부른 쪽이 넘긴 함수로 한다(포트: `ports::fs::check_confinement`).
/// - `canonicalize`: 있는 경로의 실제 경로(링크를 모두 푼 경로)
/// - `exists_no_follow`: 링크를 따라가지 않고 그 이름이 있는가(끊어진 링크도 `true`)
///
/// 아직 없는 경로는 있는 가장 가까운 윗 폴더만 풀고 나머지 조각은 그대로 붙인다(새로 만들 자리의 윗 폴더가 링크일 수 있어서).
/// 끊어진 링크나 풀 수 없는 자리를 만나면 안전한지 알 수 없으므로 거절한다.
pub fn check_real_confinement(
    base: &Path,
    target: &Path,
    canonicalize: &dyn Fn(&Path) -> io::Result<PathBuf>,
    exists_no_follow: &dyn Fn(&Path) -> bool,
) -> Result<(), EngineError> {
    check_confinement(base, target)?;
    let outside = || {
        EngineError::invalid_argument(format!(
            "심볼릭 링크를 따라가면 기준 폴더 밖을 가리키는 경로다: {}",
            absolute_normalized(target).display()
        ))
    };
    let Some(real_base) = real_path(base, canonicalize, exists_no_follow) else {
        return Err(outside());
    };
    let Some(real_target) = real_path(target, canonicalize, exists_no_follow) else {
        return Err(outside());
    };
    if !real_target.starts_with(&real_base) {
        return Err(outside());
    }
    Ok(())
}

/// 있는 가장 가까운 윗 경로까지 실제 경로로 풀고 남은 조각을 이어 붙인다. 끊어진 링크를 만나면 `None`이다.
fn real_path(
    path: &Path,
    canonicalize: &dyn Fn(&Path) -> io::Result<PathBuf>,
    exists_no_follow: &dyn Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let absolute = absolute_normalized(path);
    let mut rest: Vec<OsString> = Vec::new();
    let mut current = absolute.as_path();
    loop {
        match canonicalize(current) {
            Ok(mut real) => {
                real.extend(rest.iter().rev());
                return Some(real);
            }
            Err(_) if exists_no_follow(current) => return None,
            Err(_) => {
                let (Some(parent), Some(name)) = (current.parent(), current.file_name()) else {
                    return Some(absolute);
                };
                rest.push(name.to_os_string());
                current = parent;
            }
        }
    }
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

    /// 가짜 파일 세계: `/vault`는 `/real/vault`로 풀리고, `notes/hack`은 바깥 폴더를 가리키는 링크,
    /// `notes/loop`은 끊어진 링크, `notes/ok`는 vault 안 다른 폴더를 가리키는 링크다.
    fn fake_real(path: &Path) -> io::Result<PathBuf> {
        let gone = || Err(io::Error::from(io::ErrorKind::NotFound));
        match path.to_str().unwrap() {
            "/vault" => Ok(PathBuf::from("/real/vault")),
            "/vault/notes" => Ok(PathBuf::from("/real/vault/notes")),
            "/vault/notes/a" => Ok(PathBuf::from("/real/vault/notes/a")),
            "/vault/notes/a/doc.md" => Ok(PathBuf::from("/real/vault/notes/a/doc.md")),
            "/vault/notes/hack" => Ok(PathBuf::from("/outside/dir")),
            "/vault/notes/ok" => Ok(PathBuf::from("/real/vault/notes/a")),
            _ => gone(),
        }
    }

    fn fake_exists(path: &Path) -> bool {
        fake_real(path).is_ok() || path == Path::new("/vault/notes/loop")
    }

    fn real_check(target: &str) -> Result<(), EngineError> {
        check_real_confinement(
            Path::new("/vault"),
            Path::new(target),
            &fake_real,
            &fake_exists,
        )
    }

    #[test]
    fn follows_symlinks_before_judging_confinement() {
        // 평범한 경로와 아직 없는 새 경로는 통과한다
        assert!(real_check("/vault/notes/a/doc.md").is_ok());
        assert!(real_check("/vault/notes/a/new/deep/file.md").is_ok());
        // vault 안 다른 폴더를 가리키는 링크는 통과한다
        assert!(real_check("/vault/notes/ok/doc.md").is_ok());
        // 링크가 바깥을 가리키면, 그 아래 존재하는 파일이든 새로 만들 파일이든 거절한다
        for target in [
            "/vault/notes/hack",
            "/vault/notes/hack/doc.md",
            "/vault/notes/hack/new/deep/file.md",
        ] {
            let error = real_check(target).unwrap_err();
            assert_eq!(
                error.message,
                format!("심볼릭 링크를 따라가면 기준 폴더 밖을 가리키는 경로다: {target}")
            );
        }
        // 끊어진 링크는 어디를 가리키는지 알 수 없으므로 거절한다
        assert!(real_check("/vault/notes/loop/x.md").is_err());
        // 글자만으로도 밖인 경로는 원래 메시지로 거절한다
        assert_eq!(
            real_check("/vault/../etc/passwd").unwrap_err().message,
            "기준 폴더 밖을 가리키는 경로다: /etc/passwd"
        );
    }
}
