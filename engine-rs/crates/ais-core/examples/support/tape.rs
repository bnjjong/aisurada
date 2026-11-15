//! 맞대기 하네스의 기준(옛 Java 엔진) 응답을 녹화하고 재생하는 테이프.
//!
//! 하네스는 같은 입력을 기준 엔진과 Rust 엔진에 넣어 견준다. 기준 엔진은 태그 `java-engine-final`에서 지어야(JDK 21) 돌아서 무겁다.
//! 어긋남 0을 확인한 그 순간의 기준 응답을 테이프에 적어 두면(`--record=<파일>`) 기준 엔진 없이 같은 하네스를 다시 돌릴 수 있다(`--replay=<파일>`).
//! 테이프는 호출 순서대로 쌓은 줄이다. 하네스는 결정적(시계와 난수를 고정)이라서 순서가 곧 열쇠이고, 재생 때는 호출 인자가 같은지도 확인한다.
//!
//! 임시 폴더 이름(프로세스 번호가 들어 있다)은 실행마다 다르므로, 등록한 폴더(`register_root`)의 경로는 테이프에 자리표시자로 적고 재생 때 지금 경로로 되돌린다.
//! 이 파일은 `ais-core`와 `ais-cli`의 맞대기 예제가 `#[path]`로 함께 쓴다.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

/// 테이프를 쓰는 방식
pub enum Mode {
    /// 기준 엔진을 돌리고 아무것도 적지 않는다(지금까지의 맞대기)
    Live,
    /// 기준 엔진을 돌리고 응답을 적었다가, 어긋남이 0이면 이 파일에 쓴다
    Record(PathBuf),
    /// 기준 엔진 없이 파일에 적힌 응답을 순서대로 돌려준다
    Replay,
}

struct Entry {
    kind: String,
    key: Vec<String>,
    value: Value,
}

pub struct Tape {
    mode: Mode,
    queue: VecDeque<Entry>,
    out: Vec<Entry>,
    /// 자리표시자 번호가 곧 등록 순서다. 등록 순서는 하네스가 결정적이라 녹화와 재생이 같다.
    roots: Vec<String>,
    /// 폴더 해시의 줄기(`lineage`)별 직전 해시. 줄마다 전체를 적으면 테이프가 수십 MB가 되므로 달라진 것만 적는다.
    last_tree: HashMap<String, BTreeMap<String, String>>,
}

impl Tape {
    pub fn live() -> Tape {
        Tape {
            mode: Mode::Live,
            queue: VecDeque::new(),
            out: Vec::new(),
            roots: Vec::new(),
            last_tree: HashMap::new(),
        }
    }

    pub fn record(path: PathBuf) -> Tape {
        Tape {
            mode: Mode::Record(path),
            ..Tape::live()
        }
    }

    pub fn replay(path: &Path) -> Result<Tape, String> {
        let text = fs::read_to_string(path)
            .map_err(|e| format!("테이프를 못 읽었다 {}: {e}", path.display()))?;
        let mut queue = VecDeque::new();
        for (n, line) in text.lines().enumerate() {
            let v: Value = serde_json::from_str(line)
                .map_err(|e| format!("테이프 {}번째 줄이 JSON이 아니다: {e}", n + 1))?;
            queue.push_back(Entry {
                kind: v["kind"].as_str().unwrap_or_default().to_string(),
                key: v["key"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|s| s.as_str().unwrap_or_default().to_string())
                            .collect()
                    })
                    .unwrap_or_default(),
                value: v["value"].clone(),
            });
        }
        Ok(Tape {
            mode: Mode::Replay,
            queue,
            ..Tape::live()
        })
    }

    pub fn is_replay(&self) -> bool {
        matches!(self.mode, Mode::Replay)
    }

    pub fn is_record(&self) -> bool {
        matches!(self.mode, Mode::Record(_))
    }

    /// 실행마다 달라지는 폴더를 등록한다. 이미 등록한 폴더 안의 경로는 따로 등록하지 않아도 된다.
    pub fn register_root(&mut self, root: &Path) {
        self.roots.push(root.to_string_lossy().to_string());
    }

    /// 등록한 폴더 경로(`/` 꼴과 URI 부품 꼴 둘)를 자리표시자로 바꾼다.
    pub fn norm_str(&self, text: &str) -> String {
        let mut out = text.to_string();
        for (i, root) in self.sorted_roots() {
            out = out.replace(&root, &format!("@@R{i}@@"));
            out = out.replace(&uri(&root), &format!("@@U{i}@@"));
        }
        out
    }

    /// `norm_str`의 반대로, 자리표시자를 지금 등록된 경로로 되돌린다.
    pub fn denorm_str(&self, text: &str) -> String {
        let mut out = text.to_string();
        for (i, root) in self.roots.iter().enumerate() {
            out = out.replace(&format!("@@R{i}@@"), root);
            out = out.replace(&format!("@@U{i}@@"), &uri(root));
        }
        out
    }

    pub fn norm_value(&self, value: &Value) -> Value {
        map_strings(value, &|s| self.norm_str(s))
    }

    pub fn denorm_value(&self, value: &Value) -> Value {
        map_strings(value, &|s| self.denorm_str(s))
    }

    /// 재생: 다음 줄을 꺼낸다. 종류나 열쇠(호출 인자)가 다르면 하네스 순서가 녹화 때와 어긋난 것이다.
    pub fn next(&mut self, kind: &str, key: &[String]) -> Result<Value, String> {
        let entry = self
            .queue
            .pop_front()
            .ok_or_else(|| format!("테이프가 끝났다. 더 부른 것: {kind} {key:?}"))?;
        if entry.kind != kind || entry.key != key {
            return Err(format!(
                "하네스 순서가 테이프와 다르다.\n  테이프: {} {:?}\n  지금:   {kind} {key:?}",
                entry.kind, entry.key
            ));
        }
        Ok(self.denorm_value(&entry.value))
    }

    /// 녹화: 한 줄을 쌓는다. `key`는 이미 `norm_str`을 거친 것이어야 한다.
    pub fn push(&mut self, kind: &str, key: Vec<String>, value: &Value) {
        let value = self.norm_value(value);
        self.out.push(Entry {
            kind: kind.to_string(),
            key,
            value,
        });
    }

    /// 녹화: 폴더 해시(`digest_files`의 결과)를 적는다. `lineage`는 같은 폴더가 시간에 따라 바뀌는 줄기의 이름(예: `vault`, `approvals`)이고,
    /// 같은 줄기의 직전 해시와 달라진 것(`set`)과 없어진 것(`del`)만 적는다.
    pub fn push_tree(&mut self, lineage: &str, full: &Value) {
        let now: BTreeMap<String, String> = full
            .as_object()
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let before = self.last_tree.remove(lineage).unwrap_or_default();
        let set: Map<String, Value> = now
            .iter()
            .filter(|(k, v)| before.get(*k) != Some(*v))
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect();
        let del: Vec<&String> = before.keys().filter(|k| !now.contains_key(*k)).collect();
        self.out.push(Entry {
            kind: "tree".to_string(),
            key: vec![lineage.to_string()],
            value: json!({"set": set, "del": del}),
        });
        self.last_tree.insert(lineage.to_string(), now);
    }

    /// 재생: `push_tree`로 적은 폴더 해시 전체를 되살린다.
    pub fn next_tree(&mut self, lineage: &str) -> Result<Value, String> {
        let delta = self.next("tree", &[lineage.to_string()])?;
        let mut state = self.last_tree.remove(lineage).unwrap_or_default();
        for path in delta["del"].as_array().cloned().unwrap_or_default() {
            state.remove(path.as_str().unwrap_or_default());
        }
        for (path, hash) in delta["set"].as_object().cloned().unwrap_or_default() {
            state.insert(path, hash.as_str().unwrap_or_default().to_string());
        }
        let full: Map<String, Value> = state
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect();
        self.last_tree.insert(lineage.to_string(), state);
        Ok(Value::Object(full))
    }

    /// 녹화는 파일에 쓰고, 재생은 테이프가 남지 않았는지 본다.
    pub fn finish(&self) -> Result<(), String> {
        match &self.mode {
            Mode::Live => Ok(()),
            Mode::Record(path) => {
                let mut text = String::new();
                for e in &self.out {
                    text.push_str(
                        &json!({"kind": e.kind, "key": e.key, "value": e.value}).to_string(),
                    );
                    text.push('\n');
                }
                if let Some(dir) = path.parent() {
                    fs::create_dir_all(dir).map_err(|e| format!("폴더를 못 만들었다: {e}"))?;
                }
                fs::write(path, text).map_err(|e| format!("테이프를 못 썼다: {e}"))
            }
            Mode::Replay => {
                if self.queue.is_empty() {
                    Ok(())
                } else {
                    Err(format!("테이프가 {}줄 남았다", self.queue.len()))
                }
            }
        }
    }

    pub fn recorded_len(&self) -> usize {
        self.out.len()
    }

    /// 폴더 내용(상대 경로 → 바이트)의 파일별 SHA-256. 내용에 든 등록 폴더 경로는 자리표시자로 바꾼 뒤 센다.
    pub fn digest_files(&self, files: &BTreeMap<String, Vec<u8>>) -> Value {
        let mut map = Map::new();
        for (path, bytes) in files {
            // 승인 대기 파일(`.aisurada/pending/*.json`)은 키 차례가 Java(넣은 차례)와 Rust(이름 차례)에서 달라 맞대기도 같은 것으로 본다.
            // 그래서 읽은 값을 다시 적은 글(키 정렬)로 센다. Rust가 쓰는 바이트 자체는 직렬화 골든 시험이 따로 못박는다.
            let canonical = (path.starts_with(".aisurada/pending/") && path.ends_with(".json"))
                .then(|| serde_json::from_slice::<Value>(bytes).ok())
                .flatten()
                .map(|v| v.to_string());
            let hash = match (canonical, String::from_utf8(bytes.clone())) {
                (Some(text), _) => Sha256::digest(self.norm_str(&text).as_bytes()),
                (None, Ok(text)) => Sha256::digest(self.norm_str(&text).as_bytes()),
                (None, Err(_)) => Sha256::digest(bytes),
            };
            map.insert(
                path.clone(),
                Value::String(hash.iter().map(|b| format!("{b:02x}")).collect()),
            );
        }
        Value::Object(map)
    }

    fn sorted_roots(&self) -> Vec<(usize, String)> {
        let mut roots: Vec<(usize, String)> = self.roots.iter().cloned().enumerate().collect();
        // 폴더가 서로 안에 들어 있어도 긴 경로부터 바꾼다
        roots.sort_by_key(|(_, r)| std::cmp::Reverse(r.len()));
        roots
    }
}

fn uri(text: &str) -> String {
    ais_core::text::encode_uri_component(text)
}

fn map_strings(value: &Value, f: &dyn Fn(&str) -> String) -> Value {
    match value {
        Value::String(s) => Value::String(f(s)),
        Value::Array(items) => Value::Array(items.iter().map(|i| map_strings(i, f)).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), map_strings(v, f)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// 녹화한 기준 폴더의 해시와 지금 Rust 폴더의 해시를 견준다. 다르면 앞의 몇 곳을 글로 돌려준다.
pub fn digest_diff(recorded: &Value, local: &Value) -> String {
    let (left, right) = (
        recorded.as_object().cloned().unwrap_or_default(),
        local.as_object().cloned().unwrap_or_default(),
    );
    let mut diffs = Vec::new();
    for (path, hash) in &left {
        match right.get(path) {
            None => diffs.push(format!("녹화에만 있다: {path}")),
            Some(other) if other != hash => diffs.push(format!("내용 해시가 다르다: {path}")),
            _ => {}
        }
    }
    for path in right.keys().filter(|p| !left.contains_key(*p)) {
        diffs.push(format!("Rust에만 있다: {path}"));
    }
    diffs.truncate(5);
    diffs.join("\n    ")
}
