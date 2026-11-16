//! `std::fs`와 같은 모양의 함수 모음. 부르면 현재 스레드의 저장소 포트([`crate::ports::VaultStore`])로 넘어간다.
//!
//! 도메인 코드는 `use std::fs;` 대신 `use crate::ports::fs;`로 가져와 `fs::read(..)`, `fs::metadata(..)`처럼 쓴다. 호출 모양은 그대로이고
//! 디스크가 아니라 약속을 거친다. 어댑터와 시험 코드는 `std::fs`를 그대로 쓸 수 있다.

use std::io::{self, Read};
use std::path::{Path, PathBuf};

pub use super::store::{DirEntry, FileType, Metadata};

/// `read_dir`의 결과. 항목을 모두 읽은 뒤의 반복자다.
pub type ReadDir = std::vec::IntoIter<io::Result<DirEntry>>;

pub fn metadata(path: impl AsRef<Path>) -> io::Result<Metadata> {
    super::store().metadata(path.as_ref())
}

pub fn symlink_metadata(path: impl AsRef<Path>) -> io::Result<Metadata> {
    super::store().symlink_metadata(path.as_ref())
}

pub fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    super::store().read(path.as_ref())
}

/// 처음부터 흘려 읽는 입구(`std::fs::File::open`의 읽기 쪽)
pub fn open(path: impl AsRef<Path>) -> io::Result<Box<dyn Read + Send>> {
    super::store().open_read(path.as_ref())
}

/// 파일의 `offset`부터 최대 `length`바이트(파일 끝이 먼저 오면 읽은 만큼)
pub fn read_range(path: impl AsRef<Path>, offset: u64, length: usize) -> io::Result<Vec<u8>> {
    super::store().read_range(path.as_ref(), offset, length)
}

pub fn read_dir(path: impl AsRef<Path>) -> io::Result<ReadDir> {
    Ok(super::store().read_dir(path.as_ref())?.into_iter())
}

pub fn create_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    super::store().create_dir_all(path.as_ref())
}

pub fn write_new(path: impl AsRef<Path>, bytes: &[u8]) -> io::Result<()> {
    super::store().write_new(path.as_ref(), bytes)
}

pub fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
    super::store().remove_file(path.as_ref())
}

pub fn remove_dir(path: impl AsRef<Path>) -> io::Result<()> {
    super::store().remove_dir(path.as_ref())
}

pub fn remove_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    super::store().remove_dir_all(path.as_ref())
}

pub fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    super::store().rename(from.as_ref(), to.as_ref())
}

pub fn canonicalize(path: impl AsRef<Path>) -> io::Result<PathBuf> {
    super::store().canonicalize(path.as_ref())
}
