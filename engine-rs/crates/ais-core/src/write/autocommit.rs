//! git 자동 커밋: 쓰기가 일어나면 기록해 두었다가, 한동안 조용하거나 쓰는 쪽이 바뀌면 vault를 한 커밋으로 남긴다.
//!
//! 쓴 쪽(앱/도구), 세션, 처음과 마지막 쓴 시각의 **대기 기록은 vault의 `.git/aisurada-pending.json`에 둔다.** 앱, MCP 서버, 단발
//! `ais-cli`가 같은 파일을 읽고 고치므로 프로세스가 달라도 커밋 메시지의 「쓴 쪽」 표시가 어긋나지 않는다. 읽고 고치는 일은 모두
//! vault 쓰기 잠금을 쥔 채(쓰기 앞뒤, 정비 커밋) 하고, 잠금 없이 읽는 것은 「커밋할 때가 됐나」를 가볍게 보는 데만 쓴다.
//!
//! 유휴 타이머는 오래 사는 프로세스(앱, MCP 서버)만 돌린다. 단발 `ais-cli`는 기록만 남기고 끝나므로, 앱이나 MCP 서버가 vault를 열 때
//! [`watch`]를 불러 남은 기록을 이어받아 커밋한다. 그때까지 안 뜨면 커밋이 늦어질 뿐 변경은 빠지지 않는다(커밋은 `git add -A`).
//! `git`이 없는 폴더(저장소가 아닌 vault)는 파일을 만들지 않고 프로세스 메모리에만 둔다.
//!
//! 커밋 메시지 모양(`Aisurada-Writer`, `Aisurada-Reason`, `Aisurada-Session`, `Aisurada-Notes` 꼬리 줄)은 판 목록이 읽으므로
//! 바꾸지 않는다.

use crate::ports::{CurrentLock, VaultLock};
use std::collections::HashMap;
use crate::ports::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::UNIX_EPOCH;

use super::confine::absolute_normalized;
use crate::text::trim;
use crate::vault::git::Git;
use crate::vault::history::{header_id, read_batch};

/// 유휴 커밋의 기본 대기(초)
pub const DEFAULT_IDLE_SECONDS: i64 = 300;
const OVERDUE_MILLIS: i64 = 15 * 60 * 1000;
const STALE_INDEX_LOCK_MILLIS: i64 = 10 * 60 * 1000;
const MAX_NOTE_IDS: usize = 200;

/// 설정: 환경 변수 `AISURADA_VAULT_AUTOCOMMIT_ENABLED`(기본 true), `AISURADA_VAULT_AUTOCOMMIT_IDLE_SECONDS`(기본 300).
#[derive(Debug, Clone, Copy)]
pub struct AutoCommitConfig {
    pub enabled: bool,
    pub idle_seconds: i64,
}

impl AutoCommitConfig {
    /// 환경 변수에서 읽는다(`ports::EnvSource`). 값이 틀리면 기본값을 쓴다.
    pub fn from_env() -> Self {
        let env = crate::ports::env();
        let enabled = !matches!(
            env.var("AISURADA_VAULT_AUTOCOMMIT_ENABLED")
                .as_deref()
                .map(trim),
            Some("false")
        );
        let idle_seconds = env
            .var("AISURADA_VAULT_AUTOCOMMIT_IDLE_SECONDS")
            .and_then(|v| trim(&v).parse::<i64>().ok())
            .filter(|n| *n >= 0)
            .unwrap_or(DEFAULT_IDLE_SECONDS);
        AutoCommitConfig {
            enabled,
            idle_seconds,
        }
    }
}

/// 한 번 시도한 결과. `outcome`은 `committed`, `nothing`, `skipped`, `failed`, `disabled`이고,
/// 건너뛰거나 꺼졌을 때 `blocked`가 이유(`off`, `not-repo`, `index-lock`, `in-progress`, `not-main`)를 말한다.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitResult {
    pub outcome: &'static str,
    pub blocked: Option<&'static str>,
}

impl CommitResult {
    fn new(outcome: &'static str, blocked: Option<&'static str>) -> Self {
        CommitResult { outcome, blocked }
    }
}

/// 시계(유닉스 밀리초). 시험에서 바꿔 끼운다.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

/// 대기 기록 파일(`.git/aisurada-pending.json`)의 내용. 쓴 쪽과 세션, 처음과 마지막 쓴 시각(유닉스 밀리초)이다.
#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct PendingFile {
    writers: Vec<String>,
    sessions: Vec<String>,
    pairs: Vec<String>,
    first_write: Option<i64>,
    last_write: Option<i64>,
}

/// 이 프로세스가 아는 대기 기록. 파일에 올리는 앞부분과, 이 프로세스만 아는 재시도 정보(`retry_after`, `failures`)로 이뤄진다.
#[derive(Default)]
struct Pending {
    writers: Vec<String>,
    sessions: Vec<String>,
    pairs: Vec<String>,
    first_write: Option<i64>,
    last_write: Option<i64>,
    retry_after: i64,
    failures: u32,
}

/// 대기 기록 파일 이름(`.git/` 안)
const PENDING_FILE: &str = "aisurada-pending.json";

/// 넣은 차례를 지키는 집합에 넣는다.
fn add(set: &mut Vec<String>, value: String) {
    if !set.contains(&value) {
        set.push(value);
    }
}

pub struct AutoCommit {
    vault: PathBuf,
    config: AutoCommitConfig,
    clock: Clock,
    git: Git,
    lock: Arc<dyn VaultLock>,
    state: Mutex<Pending>,
}

static SERVING: LazyLock<Mutex<HashMap<String, Arc<AutoCommit>>>> = LazyLock::new(Default::default);
static TICKERS: LazyLock<Mutex<std::collections::HashSet<String>>> =
    LazyLock::new(Default::default);

/// 유휴 타이머를 도는 간격
const TICK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

/// 이 프로세스가 `vault`에 쓰기 시작하면 유휴 타이머를 한 번만 띄운다. 5초마다 `tick()`을 불러서, 쓰기가 끊긴 지 유휴 시간이
/// 지났거나 처음 쓴 지 15분이 지난 대기 기록을 커밋한다. 프로세스가 끝나면 함께 끝난다. 새로 띄웠으면 `true`다.
pub fn ensure_ticker(vault: &Path) -> bool {
    let key = absolute_normalized(vault).to_string_lossy().into_owned();
    if !TICKERS.lock().unwrap().insert(key) {
        return false;
    }
    let auto = AutoCommit::serving(vault);
    let _ = std::thread::Builder::new()
        .name("ais-autocommit-tick".into())
        .spawn(move || {
            loop {
                std::thread::sleep(TICK_INTERVAL);
                auto.tick();
            }
        });
    true
}

/// 오래 사는 프로세스(앱, MCP 서버)가 vault를 열 때 부른다. 유휴 타이머를 띄우고, 앞서 단발 `ais-cli`나 닫힌 앱이 남긴 대기 기록이
/// 유휴 시간이 지났으면 곧바로 커밋한다. 커밋은 느릴 수 있어 별도 스레드에서 한다.
pub fn watch(vault: &Path) {
    // 이 프로세스에서 이 vault를 처음 지켜볼 때만 남은 기록을 이어받는다(여러 번 불러도 스레드가 늘지 않는다)
    if ensure_ticker(vault) {
        let auto = AutoCommit::serving(vault);
        let _ = std::thread::Builder::new()
            .name("ais-autocommit-recover".into())
            .spawn(move || auto.tick());
    }
}

impl AutoCommit {
    pub fn new(
        vault: &Path,
        config: AutoCommitConfig,
        clock: Clock,
        lock: Arc<dyn VaultLock>,
    ) -> Self {
        let vault = absolute_normalized(vault);
        AutoCommit {
            git: Git::strict(&vault),
            vault,
            config,
            clock,
            lock,
            state: Mutex::new(Pending::default()),
        }
    }

    /// 이 프로세스가 vault 하나에 하나만 두고 나눠 쓰는 대기 기록(앱 안에서 쓴다)
    pub fn serving(vault: &Path) -> Arc<AutoCommit> {
        let key = absolute_normalized(vault).to_string_lossy().into_owned();
        SERVING
            .lock()
            .unwrap()
            .entry(key)
            .or_insert_with(|| {
                Arc::new(AutoCommit::new(
                    vault,
                    AutoCommitConfig::from_env(),
                    Arc::new(super::apparatus::now_millis),
                    Arc::new(CurrentLock),
                ))
            })
            .clone()
    }

    fn now(&self) -> i64 {
        (self.clock)()
    }

    /// 대기 기록을 파일에 올릴 수 있는가: 자동 커밋이 켜져 있고 vault가 git 저장소여야 한다.
    fn persistent(&self) -> bool {
        self.config.enabled && self.vault.join(".git").is_dir()
    }

    fn pending_path(&self) -> PathBuf {
        self.vault.join(".git").join(PENDING_FILE)
    }

    /// 파일의 대기 기록을 메모리 기록에 덮어쓴다. 다른 프로세스가 고치거나 커밋해서 지웠을 수 있어서, 읽기 전에 늘 이것을 부른다.
    /// 파일이 없으면 대기 기록이 없는 것이다. 읽을 수 없는 파일은 무시하고 메모리 기록을 그대로 둔다(다음 쓰기가 새로 쓴다).
    fn load(&self, state: &mut Pending) {
        if !self.persistent() {
            return;
        }
        let file = match fs::read(self.pending_path()) {
            Ok(bytes) => match serde_json::from_slice::<PendingFile>(&bytes) {
                Ok(file) => file,
                Err(_) => return,
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => PendingFile::default(),
            Err(_) => return,
        };
        state.writers = file.writers;
        state.sessions = file.sessions;
        state.pairs = file.pairs;
        state.first_write = file.first_write;
        state.last_write = file.last_write;
    }

    /// 메모리의 대기 기록을 파일에 올린다. 비었으면 파일을 지운다. 못 써도 쓰기의 실패가 아니다(메모리 기록은 남는다).
    fn save(&self, state: &Pending) {
        if !self.persistent() {
            return;
        }
        if state.pairs.is_empty() {
            let _ = fs::remove_file(self.pending_path());
            return;
        }
        let file = PendingFile {
            writers: state.writers.clone(),
            sessions: state.sessions.clone(),
            pairs: state.pairs.clone(),
            first_write: state.first_write,
            last_write: state.last_write,
        };
        if let Ok(bytes) = serde_json::to_vec(&file) {
            let _ = super::atomic::write_atomic(&self.pending_path(), &bytes);
        }
    }

    /// 쓰기 앞에서 부른다. 앞서 기록된 쓴 쪽이 이번 쓴 쪽과 다르면 먼저 커밋해서 둘이 한 커밋에 섞이지 않게 한다.
    pub fn before_write(&self, tool: bool, session: Option<&str>) {
        let mut state = self.state.lock().unwrap();
        self.load(&mut state);
        let pair = pair_of(tool, session);
        if !state.pairs.is_empty()
            && !(state.pairs.len() == 1 && state.pairs.contains(&pair))
            && self.now() >= state.retry_after
        {
            self.attempt_locked(&mut state, "writer");
            self.save(&state);
        }
    }

    /// 쓰기가 끝난 뒤에 부른다. 쓴 쪽과 시각을 기록한다.
    pub fn after_write(&self, tool: bool, session: Option<&str>) {
        if !self.config.enabled {
            return;
        }
        let mut state = self.state.lock().unwrap();
        self.load(&mut state);
        add(
            &mut state.writers,
            if tool { "tool" } else { "app" }.to_string(),
        );
        if tool && let Some(session) = session {
            add(&mut state.sessions, session.to_string());
        }
        add(&mut state.pairs, pair_of(tool, session));
        let now = self.now();
        state.first_write.get_or_insert(now);
        state.last_write = Some(now);
        self.save(&state);
    }

    /// 주기적으로 부른다. 마지막 쓰기 뒤 유휴 시간이 지났거나, 처음 쓴 지 15분이 넘었으면 정비 커밋을 시도한다.
    pub fn tick(&self) {
        let mut state = self.state.lock().unwrap();
        self.load(&mut state);
        let now = self.now();
        if state.pairs.is_empty() || now < state.retry_after {
            return;
        }
        let (Some(first), Some(last)) = (state.first_write, state.last_write) else {
            return;
        };
        if (now - last) / 1000 >= self.config.idle_seconds {
            self.attempt_maintenance(&mut state, "idle", now);
        } else if now - first >= OVERDUE_MILLIS {
            self.attempt_maintenance(&mut state, "overdue", now);
        }
    }

    /// 대기 중인 기록이 있는가
    pub fn has_pending(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        self.load(&mut state);
        !state.pairs.is_empty()
    }

    /// 재시도 시각을 기다리지 않고 한 번 시도한다(시작과 종료 때 쓴다). 커밋 실패는 쓰기의 실패가 아니다.
    pub fn commit(&self, reason: &str) -> CommitResult {
        let mut state = self.state.lock().unwrap();
        self.load(&mut state);
        let result = self.attempt_locked(&mut state, reason);
        self.save(&state);
        result
    }

    fn attempt_maintenance(&self, state: &mut Pending, reason: &str, now: i64) {
        match self.lock.try_acquire(&self.vault) {
            Ok(Some(_guard)) => {
                // 잠금을 기다리는 사이 다른 프로세스가 커밋했을 수 있어서 다시 읽는다
                self.load(state);
                if !state.pairs.is_empty() {
                    self.attempt_locked(state, reason);
                    self.save(state);
                }
            }
            // 쓰기 중이면 정비 커밋은 건너뛰고 5초 뒤 다시 시도한다(불완전한 커밋을 막는다)
            _ => state.retry_after = now + 5_000,
        }
    }

    fn attempt_locked(&self, state: &mut Pending, reason: &str) -> CommitResult {
        let result = self.attempt(state, reason);
        match result.outcome {
            "committed" | "nothing" => {
                *state = Pending {
                    retry_after: state.retry_after,
                    ..Pending::default()
                };
            }
            "disabled" => {
                *state = Pending {
                    retry_after: state.retry_after,
                    failures: state.failures,
                    ..Pending::default()
                };
            }
            "skipped" => state.retry_after = self.now() + 30_000,
            _ => {
                state.failures = (state.failures + 1).min(5);
                let seconds = (30i64 << (state.failures - 1)).min(480);
                state.retry_after = self.now() + seconds * 1000;
                eprintln!(
                    "경고: vault git 커밋을 못 했다. 다음 정비에서 다시 시도한다: {}",
                    self.vault.display()
                );
            }
        }
        result
    }

    fn attempt(&self, state: &Pending, reason: &str) -> CommitResult {
        if !self.config.enabled {
            return CommitResult::new("disabled", Some("off"));
        }
        let git_dir = self.vault.join(".git");
        if !git_dir.is_dir() {
            return CommitResult::new("disabled", Some("not-repo"));
        }
        let index_lock = git_dir.join("index.lock");
        match fs::metadata(&index_lock) {
            Ok(meta) => {
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                if self.now() - modified <= STALE_INDEX_LOCK_MILLIS
                    || fs::remove_file(&index_lock).is_err()
                {
                    return CommitResult::new("skipped", Some("index-lock"));
                }
                eprintln!(
                    "경고: 10분 넘은 vault git 잠금을 지웠다: {}",
                    index_lock.display()
                );
            }
            // 깨진 링크도 따라가면 없으므로 잠금이 아니다
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return CommitResult::new("skipped", Some("index-lock")),
        }
        for marker in [
            "rebase-merge",
            "rebase-apply",
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
        ] {
            if git_dir.join(marker).exists() {
                return CommitResult::new("skipped", Some("in-progress"));
            }
        }
        if trim(&self.git.run(&["symbolic-ref", "-q", "HEAD"], &[]).text()) != "refs/heads/main" {
            return CommitResult::new("skipped", Some("not-main"));
        }
        if self
            .git
            .run(&["add", "-A", "--", ".", ":(exclude)*.tmp"], &[])
            .code
            != 0
        {
            return CommitResult::new("failed", None);
        }
        let diff = self.git.run(
            &["diff", "--cached", "--no-renames", "--name-status", "-z"],
            &[],
        );
        if diff.code != 0 {
            return CommitResult::new("failed", None);
        }
        let text = diff.text();
        let fields: Vec<&str> = text.split('\0').collect();
        let mut changes = Vec::new();
        let mut i = 0;
        while i + 1 < fields.len() {
            if let (Some(status), false) = (fields[i].chars().next(), fields[i + 1].is_empty()) {
                changes.push(Change {
                    status,
                    path: fields[i + 1].to_string(),
                });
            }
            i += 2;
        }
        if changes.is_empty() {
            return CommitResult::new("nothing", None);
        }
        let ids = self.note_ids(&changes);
        let actors = if state.writers.is_empty() {
            "outside".to_string()
        } else {
            state.writers.join(",")
        };
        let actor_values = if state.writers.is_empty() {
            "outside".to_string()
        } else {
            state.writers.join(" ")
        };
        let mut message = format!(
            "ais({actors}): 노트 {}개, 파일 {}개\n\nAisurada-Writer: {actor_values}\nAisurada-Reason: {reason}\n",
            ids.len(),
            changes.len()
        );
        if !state.sessions.is_empty() {
            message.push_str(&format!("Aisurada-Session: {}\n", state.sessions.join(" ")));
        }
        if !ids.is_empty() {
            let listed: Vec<&str> = ids.iter().take(MAX_NOTE_IDS).map(String::as_str).collect();
            message.push_str(&format!("Aisurada-Notes: {}\n", listed.join(" ")));
        }
        let commit = self.git.run(
            &["commit", "-q", "--no-verify", "-F", "-"],
            message.as_bytes(),
        );
        if commit.code == 0 {
            CommitResult::new("committed", None)
        } else {
            CommitResult::new("failed", None)
        }
    }

    /// 바뀐 노트 파일(`note.md`)의 id들. 지워진 파일은 `HEAD`의 옛 내용에서 읽는다.
    fn note_ids(&self, changes: &[Change]) -> Vec<String> {
        let deleted: Vec<&Change> = changes
            .iter()
            .filter(|c| c.status == 'D' && is_note_path(&c.path))
            .collect();
        let mut previous: HashMap<&str, Vec<u8>> = HashMap::new();
        if !deleted.is_empty() {
            let specs: Vec<String> = deleted.iter().map(|c| format!("HEAD:{}", c.path)).collect();
            let batch = self.git.run(
                &["cat-file", "--batch"],
                format!("{}\n", specs.join("\n")).as_bytes(),
            );
            if batch.code == 0 {
                for (change, content) in
                    deleted.iter().zip(read_batch(deleted.len(), &batch.output))
                {
                    if let Some(content) = content {
                        previous.insert(change.path.as_str(), content);
                    }
                }
            }
        }
        let mut ids: Vec<String> = Vec::new();
        for change in changes.iter().filter(|c| is_note_path(&c.path)) {
            let bytes = if change.status == 'D' {
                previous.get(change.path.as_str()).cloned()
            } else {
                match read_head(&self.vault.join(&change.path)) {
                    Some(bytes) => Some(bytes),
                    None => continue,
                }
            };
            if let Some(id) = bytes.as_deref().and_then(header_id) {
                add(&mut ids, id);
            }
        }
        ids
    }
}

struct Change {
    status: char,
    path: String,
}

fn pair_of(tool: bool, session: Option<&str>) -> String {
    format!(
        "{}{}",
        if tool { "tool:" } else { "app:" },
        if tool { session.unwrap_or("") } else { "" }
    )
}

fn is_note_path(path: &str) -> bool {
    path.rsplit('/').next() == Some("note.md")
}

/// 파일 앞 4096바이트
fn read_head(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut head = Vec::new();
    fs::open(path)
        .ok()?
        .take(4096)
        .read_to_end(&mut head)
        .ok()?;
    Some(head)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use crate::adapters::flock_lock::FlockLock;
    use std::process::Command;
    use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
    use std::time::{Duration, SystemTime};

    fn scratch() -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ais-autocommit-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("vault")).unwrap();
        dir
    }

    fn git(vault: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(vault)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "tester")
            .env("GIT_COMMITTER_NAME", "tester")
            .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
            .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn note(vault: &Path, rel: &str, id: &str, body: &str) {
        let path = vault.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(
            path,
            format!("---\nid: {id}\ntype: free\ntitle: t\n---\n\n{body}\n"),
        )
        .unwrap();
    }

    /// 노트 둘이 첫 커밋에 든 `main` 브랜치 저장소
    fn repo() -> (PathBuf, PathBuf) {
        let dir = scratch();
        let vault = dir.join("vault");
        git(&vault, &["init", "-q", "-b", "main"]);
        note(&vault, "notes/a/note.md", "ndaa-00000001", "첫째");
        note(&vault, "notes/b/note.md", "ndbb-00000002", "둘째");
        git(&vault, &["add", "-A"]);
        git(&vault, &["commit", "-q", "-m", "init"]);
        (dir.clone(), vault)
    }

    fn auto(dir: &Path, vault: &Path, clock: Clock) -> AutoCommit {
        AutoCommit::new(
            vault,
            AutoCommitConfig {
                enabled: true,
                idle_seconds: 300,
            },
            clock,
            Arc::new(FlockLock::in_dir(dir.join("locks"))),
        )
    }

    fn real_clock() -> Clock {
        Arc::new(super::super::apparatus::now_millis)
    }

    fn count(vault: &Path) -> usize {
        git(vault, &["rev-list", "--count", "HEAD"])
            .trim()
            .parse()
            .unwrap()
    }

    #[test]
    fn commits_with_trailers_and_note_ids() {
        let (dir, vault) = repo();
        let ac = auto(&dir, &vault, real_clock());
        note(&vault, "notes/a/note.md", "ndaa-00000001", "고침");
        note(&vault, "notes/c/note.md", "ndcc-00000003", "새 노트");
        fs::remove_dir_all(vault.join("notes/b")).unwrap();
        fs::write(vault.join("notes/readme.txt"), "노트가 아님").unwrap();
        fs::write(vault.join("notes/skip.md.tmp"), "임시").unwrap();
        ac.after_write(true, Some("s1"));
        ac.after_write(false, None);
        ac.after_write(true, Some("s1"));
        assert!(ac.has_pending());

        assert_eq!(ac.commit("restore"), CommitResult::new("committed", None));
        assert!(!ac.has_pending());
        assert_eq!(
            git(&vault, &["log", "-1", "--format=%B"]).trim_end(),
            "ais(tool,app): 노트 3개, 파일 4개\n\nAisurada-Writer: tool app\nAisurada-Reason: restore\nAisurada-Session: s1\nAisurada-Notes: ndaa-00000001 ndbb-00000002 ndcc-00000003"
        );
        assert_eq!(
            git(&vault, &["log", "-1", "--format=%an <%ae>"]).trim(),
            "aisurada-auto <noreply@aisurada.local>"
        );
        assert!(
            git(&vault, &["ls-files"])
                .lines()
                .all(|f| !f.ends_with(".tmp")),
            ".tmp는 담지 않는다"
        );
        // 바뀐 것이 없으면 커밋하지 않는다
        assert_eq!(ac.commit("restore"), CommitResult::new("nothing", None));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn outside_changes_have_no_writer() {
        let (dir, vault) = repo();
        let ac = auto(&dir, &vault, real_clock());
        fs::write(vault.join("notes/readme.txt"), "x").unwrap();
        assert_eq!(ac.commit("startup").outcome, "committed");
        assert_eq!(
            git(&vault, &["log", "-1", "--format=%B"]).trim_end(),
            "ais(outside): 노트 0개, 파일 1개\n\nAisurada-Writer: outside\nAisurada-Reason: startup"
        );
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reports_why_it_did_not_commit() {
        let (dir, vault) = repo();
        let ac = auto(&dir, &vault, real_clock());
        fs::write(vault.join("notes/readme.txt"), "x").unwrap();

        let off = AutoCommit::new(
            &vault,
            AutoCommitConfig {
                enabled: false,
                idle_seconds: 300,
            },
            real_clock(),
            Arc::new(FlockLock::in_dir(dir.join("locks"))),
        );
        assert_eq!(
            off.commit("restore"),
            CommitResult::new("disabled", Some("off"))
        );

        fs::write(vault.join(".git/index.lock"), "").unwrap();
        assert_eq!(
            ac.commit("restore"),
            CommitResult::new("skipped", Some("index-lock"))
        );
        fs::remove_file(vault.join(".git/index.lock")).unwrap();

        fs::write(vault.join(".git/MERGE_HEAD"), "").unwrap();
        assert_eq!(
            ac.commit("restore"),
            CommitResult::new("skipped", Some("in-progress"))
        );
        fs::remove_file(vault.join(".git/MERGE_HEAD")).unwrap();

        git(&vault, &["switch", "-q", "-c", "other"]);
        assert_eq!(
            ac.commit("restore"),
            CommitResult::new("skipped", Some("not-main"))
        );
        git(&vault, &["switch", "-q", "main"]);

        let plain = dir.join("plain");
        fs::create_dir_all(&plain).unwrap();
        let none = auto(&dir, &plain, real_clock());
        assert_eq!(
            none.commit("restore"),
            CommitResult::new("disabled", Some("not-repo"))
        );
        assert_eq!(ac.commit("restore").outcome, "committed");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn removes_an_index_lock_older_than_ten_minutes() {
        let (dir, vault) = repo();
        let ac = auto(&dir, &vault, real_clock());
        fs::write(vault.join("notes/readme.txt"), "x").unwrap();
        let lock = fs::File::create(vault.join(".git/index.lock")).unwrap();
        lock.set_modified(SystemTime::now() - Duration::from_secs(11 * 60))
            .unwrap();
        drop(lock);
        assert_eq!(ac.commit("restore").outcome, "committed");
        assert!(!vault.join(".git/index.lock").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_different_writer_commits_the_earlier_ones_first() {
        let (dir, vault) = repo();
        let ac = auto(&dir, &vault, real_clock());
        note(&vault, "notes/a/note.md", "ndaa-00000001", "앱이 고침");
        ac.before_write(false, None);
        ac.after_write(false, None);
        let before = count(&vault);
        // 같은 쓴 쪽이면 커밋하지 않는다
        ac.before_write(false, None);
        assert_eq!(count(&vault), before);
        // 도구가 쓰려 하면 앱의 변경을 먼저 커밋한다
        ac.before_write(true, Some("s9"));
        assert_eq!(count(&vault), before + 1);
        assert!(
            git(&vault, &["log", "-1", "--format=%B"])
                .contains("Aisurada-Writer: app\nAisurada-Reason: writer")
        );
        assert!(!ac.has_pending());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn tick_commits_after_idle_and_after_fifteen_minutes() {
        let (dir, vault) = repo();
        let now = Arc::new(AtomicI64::new(1_000_000_000_000));
        let clock: Clock = {
            let now = now.clone();
            Arc::new(move || now.load(Ordering::SeqCst))
        };
        let ac = auto(&dir, &vault, clock);
        let advance = |seconds: i64| now.fetch_add(seconds * 1000, Ordering::SeqCst);

        note(&vault, "notes/a/note.md", "ndaa-00000001", "하나");
        ac.after_write(false, None);
        advance(299);
        ac.tick();
        assert!(ac.has_pending(), "유휴 시간이 아직 안 됐다");
        advance(1);
        ac.tick();
        assert!(!ac.has_pending());
        assert!(git(&vault, &["log", "-1", "--format=%B"]).contains("Aisurada-Reason: idle"));

        // 계속 쓰면 유휴가 오지 않아도 처음 쓴 지 15분이 지나면 커밋한다
        for step in 0..8 {
            note(
                &vault,
                "notes/a/note.md",
                "ndaa-00000001",
                &format!("계속 {step}"),
            );
            ac.after_write(false, None);
            advance(100);
            ac.tick();
        }
        assert!(ac.has_pending(), "처음 쓴 지 14분이 안 됐다");
        ac.after_write(false, None);
        advance(100);
        ac.tick();
        assert!(!ac.has_pending(), "처음 쓴 지 정확히 15분이다");
        assert!(git(&vault, &["log", "-1", "--format=%B"]).contains("Aisurada-Reason: overdue"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn maintenance_waits_while_a_write_holds_the_lock() {
        let (dir, vault) = repo();
        let now = Arc::new(AtomicI64::new(1_000_000_000_000));
        let clock: Clock = {
            let now = now.clone();
            Arc::new(move || now.load(Ordering::SeqCst))
        };
        let ac = auto(&dir, &vault, clock);
        note(&vault, "notes/a/note.md", "ndaa-00000001", "쓰는 중");
        ac.after_write(false, None);
        now.fetch_add(301_000, Ordering::SeqCst);

        let held = crate::adapters::flock_lock::acquire(&vault, &dir.join("locks")).unwrap();
        ac.tick();
        assert!(ac.has_pending(), "쓰기 중에는 정비 커밋을 건너뛴다");
        drop(held);
        ac.tick();
        assert!(ac.has_pending(), "5초는 기다린다");
        now.fetch_add(6_000, Ordering::SeqCst);
        ac.tick();
        assert!(!ac.has_pending());
        fs::remove_dir_all(&dir).unwrap();
    }

    /// 같은 vault를 두 프로세스가 연 것처럼 인스턴스 둘을 만든다(메모리는 따로이고 `.git/` 안 파일만 같다).
    fn second_process(dir: &Path, vault: &Path, clock: Clock) -> AutoCommit {
        auto(dir, vault, clock)
    }

    #[test]
    fn the_pending_record_is_shared_through_a_file_in_git_dir() {
        let (dir, vault) = repo();
        let writer = auto(&dir, &vault, real_clock());
        let other = second_process(&dir, &vault, real_clock());
        let file = vault.join(".git/aisurada-pending.json");

        note(
            &vault,
            "notes/a/note.md",
            "ndaa-00000001",
            "단발 호출이 고침",
        );
        writer.after_write(true, Some("s1"));
        assert!(file.is_file(), "대기 기록이 파일로 올라간다");
        assert!(other.has_pending(), "다른 프로세스가 그 기록을 본다");

        // 다른 프로세스가 커밋하면 쓴 쪽과 세션이 그대로 커밋 메시지에 실린다
        assert_eq!(other.commit("startup").outcome, "committed");
        let message = git(&vault, &["log", "-1", "--format=%B"]);
        assert!(
            message
                .contains("Aisurada-Writer: tool\nAisurada-Reason: startup\nAisurada-Session: s1"),
            "{message}"
        );
        assert!(!file.exists(), "커밋하면 파일이 지워진다");
        assert!(!writer.has_pending(), "처음 프로세스도 커밋된 것을 안다");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn another_process_commits_an_idle_record_on_tick_and_a_writer_change_commits_first() {
        let (dir, vault) = repo();
        let now = Arc::new(AtomicI64::new(1_000_000_000_000));
        let clock: Clock = {
            let now = now.clone();
            Arc::new(move || now.load(Ordering::SeqCst))
        };
        let single_shot = auto(&dir, &vault, clock.clone());
        let resident = second_process(&dir, &vault, clock);

        note(
            &vault,
            "notes/a/note.md",
            "ndaa-00000001",
            "앱을 닫은 동안 고침",
        );
        single_shot.after_write(true, Some("s2"));
        now.fetch_add(299_000, Ordering::SeqCst);
        resident.tick();
        assert!(resident.has_pending(), "유휴 시간이 아직 안 됐다");
        now.fetch_add(1_000, Ordering::SeqCst);
        resident.tick();
        assert!(
            !single_shot.has_pending(),
            "상주 프로세스가 단발 호출의 기록을 이어받아 커밋했다"
        );
        let message = git(&vault, &["log", "-1", "--format=%B"]);
        assert!(
            message.contains("Aisurada-Reason: idle") && message.contains("Aisurada-Session: s2"),
            "{message}"
        );

        // 다른 프로세스가 앱의 기록을 남겨 두면, 도구가 쓰려 할 때 앱의 변경을 먼저 커밋한다
        note(&vault, "notes/a/note.md", "ndaa-00000001", "앱이 고침");
        resident.after_write(false, None);
        let before = count(&vault);
        single_shot.before_write(true, Some("s3"));
        assert_eq!(count(&vault), before + 1);
        assert!(git(&vault, &["log", "-1", "--format=%B"]).contains("Aisurada-Writer: app"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_vault_without_git_or_with_autocommit_off_leaves_no_file() {
        let (dir, vault) = repo();
        let off = AutoCommit::new(
            &vault,
            AutoCommitConfig {
                enabled: false,
                idle_seconds: 300,
            },
            real_clock(),
            Arc::new(FlockLock::in_dir(dir.join("locks"))),
        );
        off.after_write(false, None);
        assert!(!vault.join(".git/aisurada-pending.json").exists());

        let plain = dir.join("plain");
        fs::create_dir_all(&plain).unwrap();
        let memory_only = auto(&dir, &plain, real_clock());
        memory_only.after_write(false, None);
        assert!(
            memory_only.has_pending(),
            "저장소가 아니어도 이 프로세스 안에서는 기록한다"
        );
        assert!(!plain.join(".git").exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_unreadable_record_file_does_not_stop_writes() {
        let (dir, vault) = repo();
        let ac = auto(&dir, &vault, real_clock());
        fs::write(vault.join(".git/aisurada-pending.json"), "{깨진").unwrap();
        ac.after_write(false, None);
        assert!(ac.has_pending(), "다음 쓰기가 새 기록을 쓴다");
        assert_eq!(ac.commit("restore").outcome, "nothing");
        assert!(!vault.join(".git/aisurada-pending.json").exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
