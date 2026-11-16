//! 저장소 포트(`VaultStore`): vault 폴더와 파일을 읽고 쓰는 가장 낮은 층의 약속.
//!
//! 도메인 코드는 `std::fs`를 직접 부르지 않고 이 약속을 거친다. 기본 어댑터(`adapters/local_store.rs`)는 로컬 디스크를 그대로 읽고 쓴다.
//! 다른 구현(메모리 저장소, 훗날의 클라우드 저장소)을 끼우면 같은 규칙이 다른 곳에서 돈다.
//! 반환 타입은 `std::fs`의 것과 같은 메서드 이름을 가진 이 모듈의 [`Metadata`], [`FileType`], [`DirEntry`]다(`std::fs::Metadata`는 로컬 디스크에 묶여 있어서다).
//!
//! 도메인 코드는 약속을 직접 꺼내지 않고 `std::fs`와 같은 모양의 함수 모음 [`crate::ports::fs`]를 쓴다.

use std::ffi::OsString;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// 파일 종류. `std::fs::FileType`과 같은 메서드를 가진다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    File,
    Dir,
    Symlink,
    Other,
}

impl FileType {
    pub fn is_file(&self) -> bool {
        matches!(self, FileType::File)
    }

    pub fn is_dir(&self) -> bool {
        matches!(self, FileType::Dir)
    }

    pub fn is_symlink(&self) -> bool {
        matches!(self, FileType::Symlink)
    }
}

/// 파일 정보. `std::fs::Metadata`와 같은 이름의 메서드를 가진다(`modified`는 못 읽는 저장소에서 오류다).
#[derive(Debug, Clone, Copy)]
pub struct Metadata {
    file_type: FileType,
    len: u64,
    modified: Option<SystemTime>,
}

impl Metadata {
    pub fn new(file_type: FileType, len: u64, modified: Option<SystemTime>) -> Self {
        Metadata {
            file_type,
            len,
            modified,
        }
    }

    pub fn file_type(&self) -> FileType {
        self.file_type
    }

    pub fn is_file(&self) -> bool {
        self.file_type.is_file()
    }

    pub fn is_dir(&self) -> bool {
        self.file_type.is_dir()
    }

    pub fn is_symlink(&self) -> bool {
        self.file_type.is_symlink()
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn modified(&self) -> io::Result<SystemTime> {
        self.modified
            .ok_or_else(|| io::Error::new(io::ErrorKind::Unsupported, "수정 시각을 모르는 저장소"))
    }
}

/// 폴더 항목 하나. `std::fs::DirEntry`처럼 이름, 경로, 종류를 주고 정보는 필요할 때 묻는다(심볼릭 링크를 따라가지 않는 정보다).
#[derive(Debug, Clone)]
pub struct DirEntry {
    path: PathBuf,
    file_type: Option<FileType>,
}

impl DirEntry {
    pub fn new(path: PathBuf, file_type: Option<FileType>) -> Self {
        DirEntry { path, file_type }
    }

    pub fn path(&self) -> PathBuf {
        self.path.clone()
    }

    pub fn file_name(&self) -> OsString {
        self.path.file_name().map(OsString::from).unwrap_or_default()
    }

    pub fn file_type(&self) -> io::Result<FileType> {
        self.file_type
            .ok_or_else(|| io::Error::other("항목 종류를 못 읽었다"))
    }

    pub fn metadata(&self) -> io::Result<Metadata> {
        super::store().symlink_metadata(&self.path)
    }
}

/// vault 저장소
pub trait VaultStore: Send + Sync {
    /// 심볼릭 링크를 따라간 정보
    fn metadata(&self, path: &Path) -> io::Result<Metadata>;
    /// 심볼릭 링크를 따라가지 않은 정보
    fn symlink_metadata(&self, path: &Path) -> io::Result<Metadata>;
    fn read(&self, path: &Path) -> io::Result<Vec<u8>>;
    /// 처음부터 흘려 읽는다(큰 파일의 앞부분만 읽을 때)
    fn open_read(&self, path: &Path) -> io::Result<Box<dyn Read + Send>>;
    /// 파일의 `offset`부터 최대 `length`바이트를 읽는다. 파일 끝이 먼저 오면 읽은 만큼만 돌려준다(큰 파일의 머리와 꼬리를 읽을 때).
    fn read_range(&self, path: &Path, offset: u64, length: usize) -> io::Result<Vec<u8>>;
    /// 폴더 항목을 운영체제가 주는 차례로 돌려준다. 항목 하나를 못 읽으면 그 자리가 오류다.
    fn read_dir(&self, path: &Path) -> io::Result<Vec<io::Result<DirEntry>>>;
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    /// 이미 있으면 실패하는 새 파일을 만들어 바이트를 쓰고 디스크에 내린다(원자적 쓰기의 임시 파일)
    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    fn remove_dir(&self, path: &Path) -> io::Result<()>;
    fn remove_dir_all(&self, path: &Path) -> io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// 심볼릭 링크를 푼 실제 경로
    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf>;
}
