//! 폴더나 파일의 파일 수와 바이트 합(28장). 폴더 링크는 내려가지 않고 파일 링크는 대상 크기로 센다.

use crate::ports::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FileSize {
    pub files: usize,
    pub bytes: u64,
}

pub fn measure(root: &Path) -> FileSize {
    if fs::metadata(root).map(|m| m.is_file()).unwrap_or(false) {
        return FileSize {
            files: 1,
            bytes: size(root),
        };
    }
    if !fs::symlink_metadata(root)
        .map(|m| m.is_dir())
        .unwrap_or(false)
    {
        return FileSize::default();
    }
    let mut total = FileSize::default();
    walk(root, &mut total);
    total
}

fn walk(dir: &Path, total: &mut FileSize) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if fs::metadata(&path).map(|m| m.is_file()).unwrap_or(false) {
            total.files += 1;
            total.bytes += size(&path);
        }
        // 폴더 링크는 따라 내려가지 않는다
        if fs::symlink_metadata(&path)
            .map(|m| m.is_dir())
            .unwrap_or(false)
        {
            walk(&path, total);
        }
    }
}

fn size(path: &Path) -> u64 {
    fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}
