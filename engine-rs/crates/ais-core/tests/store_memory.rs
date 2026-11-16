//! 메모리 저장소 시험: 디스크에 없는 vault를 **저장소 포트만 바꿔서** 읽는다.
//!
//! 5단계(저장소 포트)의 목적을 보인다. 도메인 코드(아젠다·노트 훑기, 목록)는 `std::fs`를 직접 부르지 않고 `ports::fs`를 거치므로,
//! 메모리에 파일을 놓은 저장소를 끼우면 같은 규칙이 디스크 없이 돈다. 훗날의 클라우드 저장소도 같은 자리에 들어간다.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ais_core::adapters::default_ports;
use ais_core::engine::Engine;
use ais_core::ports::VaultStore;
use ais_core::ports::store::{DirEntry, FileType, Metadata};
use serde_json::json;

/// 파일 몇 개를 메모리에 둔 저장소. 폴더는 파일 경로의 윗부분으로만 있다.
#[derive(Default)]
struct MemStore {
    files: Mutex<BTreeMap<PathBuf, Vec<u8>>>,
}

impl MemStore {
    fn put(&self, path: &str, text: &str) {
        self.files
            .lock()
            .unwrap()
            .insert(PathBuf::from(path), text.as_bytes().to_vec());
    }

    fn is_dir(&self, path: &Path) -> bool {
        self.files
            .lock()
            .unwrap()
            .keys()
            .any(|f| f != path && f.starts_with(path))
    }
}

fn not_found() -> io::Error {
    io::Error::from(io::ErrorKind::NotFound)
}

impl VaultStore for MemStore {
    fn metadata(&self, path: &Path) -> io::Result<Metadata> {
        self.symlink_metadata(path)
    }

    fn symlink_metadata(&self, path: &Path) -> io::Result<Metadata> {
        if let Some(bytes) = self.files.lock().unwrap().get(path) {
            return Ok(Metadata::new(FileType::File, bytes.len() as u64, None));
        }
        if self.is_dir(path) {
            return Ok(Metadata::new(FileType::Dir, 0, None));
        }
        Err(not_found())
    }

    fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.files.lock().unwrap().get(path).cloned().ok_or_else(not_found)
    }

    fn open_read(&self, path: &Path) -> io::Result<Box<dyn Read + Send>> {
        Ok(Box::new(Cursor::new(self.read(path)?)))
    }

    fn read_range(&self, path: &Path, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        let all = self.read(path)?;
        let start = (offset as usize).min(all.len());
        Ok(all[start..(start + length).min(all.len())].to_vec())
    }

    fn read_dir(&self, path: &Path) -> io::Result<Vec<io::Result<DirEntry>>> {
        if !self.is_dir(path) {
            return Err(not_found());
        }
        let mut names = BTreeSet::new();
        for file in self.files.lock().unwrap().keys() {
            if let Ok(rest) = file.strip_prefix(path)
                && let Some(first) = rest.components().next()
            {
                let child = path.join(first);
                names.insert((child.clone(), &child != file));
            }
        }
        Ok(names
            .into_iter()
            .map(|(child, is_dir)| {
                Ok(DirEntry::new(
                    child,
                    Some(if is_dir { FileType::Dir } else { FileType::File }),
                ))
            })
            .collect())
    }

    fn create_dir_all(&self, _: &Path) -> io::Result<()> {
        Ok(())
    }

    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let mut files = self.files.lock().unwrap();
        if files.contains_key(path) {
            return Err(io::Error::from(io::ErrorKind::AlreadyExists));
        }
        files.insert(path.to_path_buf(), bytes.to_vec());
        Ok(())
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        self.files.lock().unwrap().remove(path).map(|_| ()).ok_or_else(not_found)
    }

    fn remove_dir(&self, _: &Path) -> io::Result<()> {
        Ok(())
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        self.files.lock().unwrap().retain(|f, _| !f.starts_with(path));
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let mut files = self.files.lock().unwrap();
        let moved: Vec<PathBuf> = files.keys().filter(|f| f.starts_with(from)).cloned().collect();
        if moved.is_empty() {
            return Err(not_found());
        }
        for old in moved {
            let bytes = files.remove(&old).unwrap();
            files.insert(to.join(old.strip_prefix(from).unwrap()), bytes);
        }
        Ok(())
    }

    fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        self.symlink_metadata(path).map(|_| path.to_path_buf())
    }
}

#[test]
fn a_vault_that_only_exists_in_memory_is_read_through_the_store_port() {
    // 이 경로는 실제 디스크에 없다
    let vault = Path::new("/ais-memory-vault-that-does-not-exist");
    assert!(!vault.exists());

    let store = Arc::new(MemStore::default());
    store.put(
        "/ais-memory-vault-that-does-not-exist/.aisurada/agendas/demo.md",
        "---\nid: agfx0001-0000000a\nslug: demo\ntitle: 메모리 아젠다\nhue: teal\n---\n소개\n",
    );
    store.put(
        "/ais-memory-vault-that-does-not-exist/notes/demo/2026-11/first/note.md",
        "---\nid: ndzz0001-0000aaaa\nagenda: demo\ntype: free\ntitle: 메모리 노트\ncreated: '2026-11-12T09:00:00+09:00'\n---\n\n본문\n",
    );

    let mut ports = default_ports();
    ports.store = store;
    let engine = Engine::with_ports(vault, ports);

    let agendas = engine.call("list_agendas", json!({})).unwrap();
    let rows = agendas["items"].as_array().expect("목록");
    assert_eq!(rows.len(), 1, "{agendas}");
    assert_eq!(rows[0]["slug"], "demo");
    assert_eq!(rows[0]["title"], "메모리 아젠다");

    let docs = engine.call("list_docs", json!({"limit": 0})).unwrap();
    let notes = docs["items"].as_array().expect("목록");
    assert_eq!(notes.len(), 1, "{docs}");
    assert_eq!(notes[0]["id"], "ndzz0001-0000aaaa");
    assert_eq!(notes[0]["title"], "메모리 노트");

    // 디스크에는 아무것도 생기지 않았다
    assert!(!vault.exists());
}
