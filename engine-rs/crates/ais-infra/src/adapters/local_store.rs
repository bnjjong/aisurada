//! 로컬 디스크 저장소 어댑터: `std::fs`를 그대로 부른다. 저장소 포트(`ports::VaultStore`)의 기본 구현이다.

use std::fs::{self, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::ports::store::{DirEntry, FileType, Metadata, VaultStore};

pub struct LocalStore;

fn kind(file_type: fs::FileType) -> FileType {
    if file_type.is_symlink() {
        FileType::Symlink
    } else if file_type.is_dir() {
        FileType::Dir
    } else if file_type.is_file() {
        FileType::File
    } else {
        FileType::Other
    }
}

fn meta(m: fs::Metadata) -> Metadata {
    Metadata::new(kind(m.file_type()), m.len(), m.modified().ok())
}

impl VaultStore for LocalStore {
    fn metadata(&self, path: &Path) -> io::Result<Metadata> {
        fs::metadata(path).map(meta)
    }

    fn symlink_metadata(&self, path: &Path) -> io::Result<Metadata> {
        fs::symlink_metadata(path).map(meta)
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    fn open_read(&self, path: &Path) -> io::Result<Box<dyn Read + Send>> {
        Ok(Box::new(fs::File::open(path)?))
    }

    fn read_range(&self, path: &Path, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        let mut file = fs::File::open(path)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut buffer = vec![0u8; length];
        let mut filled = 0;
        while filled < length {
            let n = file.read(&mut buffer[filled..])?;
            if n == 0 {
                break;
            }
            filled += n;
        }
        buffer.truncate(filled);
        Ok(buffer)
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<io::Result<DirEntry>>> {
        Ok(fs::read_dir(path)?
            .map(|entry| entry.map(|e| DirEntry::new(e.path(), e.file_type().ok().map(kind))))
            .collect())
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path)
    }

    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        file.write_all(bytes)?;
        file.sync_all()
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }

    fn remove_dir(&self, path: &Path) -> io::Result<()> {
        fs::remove_dir(path)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        fs::canonicalize(path)
    }
}
