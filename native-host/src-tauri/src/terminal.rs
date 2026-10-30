//! Chapter 14. One owned PTY per key; stale readers and grace timers cannot delete replacements.
use crate::lifecycle::HostState;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::{
    collections::HashMap,
    io::{Read, Write},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    time::Duration,
};
use tauri::{ipc::Channel, State};
const REPLAY: usize = 2_097_152;
const FNV_START: u64 = 0xcbf29ce484222325;
pub(crate) type Sink = Arc<dyn Fn(&[u8]) -> Result<(), String> + Send + Sync>;
struct Buffer {
    bytes: Vec<u8>,
    sink: Option<Sink>,
    bytes_out: u64,
    chunks_out: u64,
    hash: u64,
}
impl Default for Buffer {
    fn default() -> Self {
        Self {
            bytes: vec![],
            sink: None,
            bytes_out: 0,
            chunks_out: 0,
            hash: FNV_START,
        }
    }
}
impl Buffer {
    fn push(&mut self, bytes: &[u8]) {
        self.bytes_out += bytes.len() as u64;
        self.chunks_out += 1;
        for b in bytes {
            self.hash = (self.hash ^ *b as u64).wrapping_mul(0x100000001b3);
        }
        self.bytes.extend_from_slice(bytes);
        if self.bytes.len() > REPLAY {
            self.bytes.drain(..self.bytes.len() - REPLAY);
        }
        if let Some(s) = &self.sink {
            let _ = s(bytes);
        }
    }
}
pub struct Entry {
    fell_back: bool,
    master: Mutex<Option<Box<dyn MasterPty + Send>>>,
    child: Mutex<Option<Box<dyn Child + Send + Sync>>>,
    input: Mutex<Option<mpsc::Sender<Vec<u8>>>>,
    buffer: Mutex<Buffer>,
    generation: AtomicU64,
}
impl Entry {
    fn terminate(&self) {
        let mut child = self.child.lock().unwrap();
        if let Some(c) = child.as_mut() {
            if !matches!(c.try_wait(), Ok(Some(_))) {
                let _ = c.kill();
            }
            let _ = c.wait();
        }
        *child = None;
        self.input.lock().unwrap().take();
        self.master.lock().unwrap().take();
        self.buffer.lock().unwrap().sink = None;
    }
}
#[derive(Default, Clone)]
pub struct Registry(Arc<Mutex<HashMap<String, Arc<Entry>>>>);
impl Registry {
    fn remove_if(&self, key: &str, entry: &Arc<Entry>, generation: Option<u64>) -> bool {
        let mut entries = self.0.lock().unwrap();
        if entries.get(key).is_some_and(|e| {
            Arc::ptr_eq(e, entry)
                && generation.is_none_or(|n| e.generation.load(Ordering::SeqCst) == n)
        }) {
            entries.remove(key);
            true
        } else {
            false
        }
    }
    pub fn reap(&self) {
        let entries = std::mem::take(&mut *self.0.lock().unwrap());
        for e in entries.into_values() {
            e.terminate();
        }
    }
    fn entry(&self, key: &str) -> Result<Arc<Entry>, String> {
        self.0
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or_else(|| "pty 가 아직 안 떴다".into())
    }
    #[cfg(test)]
    fn start(
        &self,
        key: String,
        size: PtySize,
        sink: Sink,
        command: CommandBuilder,
    ) -> Result<(), String> {
        self.start_shell(key, size, sink, command, false)
            .map(|_| ())
    }
    pub(crate) fn start_shell(
        &self,
        key: String,
        size: PtySize,
        sink: Sink,
        command: CommandBuilder,
        fell_back: bool,
    ) -> Result<bool, String> {
        let entries = self.0.lock().unwrap();
        if let Some(e) = entries.get(&key) {
            let mut b = e.buffer.lock().unwrap();
            e.generation.fetch_add(1, Ordering::SeqCst);
            if !b.bytes.is_empty() {
                sink(&b.bytes)?;
            }
            b.sink = Some(sink);
            return Ok(e.fell_back);
        }
        drop(entries);
        let pair = native_pty_system()
            .openpty(size)
            .map_err(|e| e.to_string())?;
        let mut child = pair
            .slave
            .spawn_command(command)
            .map_err(|e| e.to_string())?;
        drop(pair.slave);
        let (mut reader, mut writer) = match pair
            .master
            .try_clone_reader()
            .and_then(|reader| pair.master.take_writer().map(|writer| (reader, writer)))
        {
            Ok(handles) => handles,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.to_string());
            }
        };
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        std::thread::spawn(move || {
            while let Ok(bytes) = rx.recv() {
                if writer
                    .write_all(&bytes)
                    .and_then(|_| writer.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
        let entry = Arc::new(Entry {
            fell_back,
            master: Mutex::new(Some(pair.master)),
            child: Mutex::new(Some(child)),
            input: Mutex::new(Some(tx)),
            buffer: Mutex::new(Buffer {
                sink: Some(sink),
                ..Buffer::default()
            }),
            generation: AtomicU64::new(0),
        });
        let registry = self.clone();
        let e = entry.clone();
        let k = key.clone();
        std::thread::spawn(move || {
            let mut bytes = [0; 8192];
            loop {
                match reader.read(&mut bytes) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => e.buffer.lock().unwrap().push(&bytes[..n]),
                }
            }
            for _ in 0..50 {
                let done = e
                    .child
                    .lock()
                    .unwrap()
                    .as_mut()
                    .is_none_or(|c| c.try_wait().ok().flatten().is_some());
                if done {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            registry.remove_if(&k, &e, None);
            e.terminate();
        });
        // Preserve the documented reader-before-registration ordering (C42).
        self.0.lock().unwrap().insert(key, entry);
        Ok(fell_back)
    }
    fn write(&self, key: &str, data: Vec<u8>) -> Result<(), String> {
        let e = self.entry(key)?;
        let input = e.input.lock().unwrap();
        input
            .as_ref()
            .ok_or("pty 가 아직 안 떴다")?
            .send(data)
            .map_err(|_| "셸이 이미 끝났다".into())
    }
    fn detach(&self, key: &str) {
        let Ok(e) = self.entry(key) else { return };
        let generation = {
            let mut buffer = e.buffer.lock().unwrap();
            buffer.sink = None;
            e.generation.fetch_add(1, Ordering::SeqCst) + 1
        };
        let registry = self.clone();
        let key = key.to_owned();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(300));
            if registry.remove_if(&key, &e, Some(generation)) {
                e.terminate();
            }
        });
    }
    fn stats(&self, key: &str) -> Stats {
        self.entry(key)
            .ok()
            .map(|e| {
                let b = e.buffer.lock().unwrap();
                Stats {
                    bytes_out: b.bytes_out,
                    chunks_out: b.chunks_out,
                    hash_out: format!("{:016x}", b.hash),
                }
            })
            .unwrap_or_default()
    }
}
pub fn login_shell(cwd: Option<&std::path::Path>) -> (CommandBuilder, bool) {
    let mut c = CommandBuilder::new("/bin/zsh");
    c.args(["-l", "-i"]);
    c.env("TERM", "xterm-256color");
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let fell_back = cwd.is_some_and(|p| !p.is_dir());
    if let Some(path) = cwd.filter(|p| p.is_dir()).or(home.as_deref()) {
        c.cwd(path);
    }
    (c, fell_back)
}
fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows,
        cols,
        pixel_width: 0,
        pixel_height: 0,
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Started {
    fell_back: bool,
}
#[tauri::command]
pub fn pty_start(
    state: State<'_, HostState>,
    key: String,
    cols: u16,
    rows: u16,
    on_data: Channel<String>,
) -> Result<Started, String> {
    let (c, fell_back) = login_shell(None);
    let fell_back = state.terminals.start_shell(
        key,
        size(cols, rows),
        Arc::new(move |b| crate::ipc::send_bytes(&on_data, b).map_err(|e| e.to_string())),
        c,
        fell_back,
    )?;
    Ok(Started { fell_back })
}
#[tauri::command]
pub fn pty_write(state: State<'_, HostState>, key: String, data: String) -> Result<(), String> {
    state.terminals.write(&key, data.into_bytes())
}
#[tauri::command]
pub fn pty_resize(
    state: State<'_, HostState>,
    key: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let e = state.terminals.entry(&key)?;
    let master = e.master.lock().unwrap();
    master
        .as_ref()
        .ok_or("pty 가 아직 안 떴다")?
        .resize(size(cols, rows))
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn pty_detach(state: State<'_, HostState>, key: String) {
    state.terminals.detach(&key)
}
#[tauri::command]
pub fn pty_alive(state: State<'_, HostState>, key: String) -> bool {
    state.terminals.entry(&key).ok().is_some_and(|e| {
        e.child
            .lock()
            .unwrap()
            .as_mut()
            .is_some_and(|c| c.try_wait().ok().is_some_and(|s| s.is_none()))
    })
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    bytes_out: u64,
    chunks_out: u64,
    hash_out: String,
}
impl Default for Stats {
    fn default() -> Self {
        Self {
            bytes_out: 0,
            chunks_out: 0,
            hash_out: format!("{FNV_START:016x}"),
        }
    }
}
#[tauri::command]
pub fn pty_stats(state: State<'_, HostState>, key: String) -> Stats {
    state.terminals.stats(&key)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    fn shell() -> CommandBuilder {
        let mut c = CommandBuilder::new("/bin/sh");
        c.args([
            "-c",
            "printf ready; read value; printf 'result:%s' \"$value\"; sleep 1",
        ]);
        c
    }
    #[test]
    fn actual_pty_round_trip_replay_and_stats() {
        let r = Registry::default();
        let (tx, rx) = mpsc::channel();
        r.start(
            "test".into(),
            size(80, 24),
            Arc::new(move |b| tx.send(b.to_vec()).map_err(|e| e.to_string())),
            shell(),
        )
        .unwrap();
        let mut received = vec![];
        while !String::from_utf8_lossy(&received).contains("ready") {
            received.extend(rx.recv_timeout(Duration::from_secs(3)).unwrap());
        }
        let stats = r.stats("test");
        assert!(stats.bytes_out > 0 && stats.chunks_out > 0);
        assert_eq!(stats.hash_out.len(), 16);
        r.detach("test");
        let replay = Arc::new(Mutex::new(vec![]));
        let out = replay.clone();
        let entry = r.entry("test").unwrap();
        let old = entry.generation.load(Ordering::SeqCst);
        r.start(
            "test".into(),
            size(1, 1),
            Arc::new(move |b| {
                out.lock().unwrap().extend_from_slice(b);
                Ok(())
            }),
            shell(),
        )
        .unwrap();
        assert!(!r.remove_if("test", &entry, Some(old)));
        assert!(String::from_utf8_lossy(&replay.lock().unwrap()).contains("ready"));
        r.write("test", b"part2-42\n".to_vec()).unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        while r.entry("test").is_ok() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(r.entry("test").is_err());
        assert!(String::from_utf8_lossy(&replay.lock().unwrap()).contains("result:part2-42"));
        assert_eq!(r.stats("missing").hash_out, "cbf29ce484222325");
    }
    #[test]
    fn stale_reader_cannot_remove_replacement() {
        let r = Registry::default();
        let sink: Sink = Arc::new(|_| Ok(()));
        r.start("same".into(), size(80, 24), sink.clone(), shell())
            .unwrap();
        let old = r.entry("same").unwrap();
        r.reap();
        r.start("same".into(), size(80, 24), sink, shell()).unwrap();
        assert!(!r.remove_if("same", &old, None));
        assert!(!r.remove_if("same", &old, Some(0)));
        r.reap();
        r.reap();
        assert!(r.0.lock().unwrap().is_empty());
    }
    #[test]
    fn replay_cap_and_fnv_cover_all_output() {
        let mut b = Buffer::default();
        b.push(b"hello");
        assert_eq!(b.hash, 0xa430d84680aabd0b);
        b.push(&vec![b'x'; REPLAY + 100]);
        assert_eq!(b.bytes.len(), REPLAY);
        assert_eq!(b.bytes_out, REPLAY as u64 + 105);
        assert_eq!(b.chunks_out, 2);
    }
}

#[cfg(test)]
mod fallback_test {
    use super::*;
    #[test]
    fn reattachment_keeps_original_start_fallback() {
        let r = Registry::default();
        let command = || {
            let mut c = CommandBuilder::new("/bin/sh");
            c.args(["-c", "sleep 10"]);
            c
        };
        let sink: Sink = Arc::new(|_| Ok(()));
        assert!(r
            .start_shell(
                "fallback".into(),
                size(80, 24),
                sink.clone(),
                command(),
                true
            )
            .unwrap());
        assert!(r
            .start_shell("fallback".into(), size(1, 1), sink, command(), false)
            .unwrap());
        r.reap();
    }
}
