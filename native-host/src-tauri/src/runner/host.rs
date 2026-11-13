//! 러너가 바깥과 만나는 자리. 엔진(`ais-cli`), claude, GitHub 저장소 사본, 알림, 화면 사건을 한 trait 뒤에 둔다.
//! 실제 앱은 [AppHost]를 쓰고, 시험은 가짜를 끼워서 프로세스를 띄우지 않고 흐름만 확인한다.
use super::store::default_root;
use crate::{
    ask, github, ipc,
    notifications::{self, NewItem},
    sidecar::{self, Reply},
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use tauri::{AppHandle, Emitter, Manager, Runtime};

/// claude를 한 번 부르는 데 필요한 것.
pub struct ClaudeCall {
    pub workdir: PathBuf,
    pub vault: String,
    pub args: Vec<String>,
    /// 표준입력으로 주는 프롬프트
    pub prompt: String,
    pub timeout: Duration,
    pub cancel: Arc<AtomicBool>,
}

/// claude 한 번의 결과.
pub struct ClaudeOutcome {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub cancelled: bool,
    pub stdout: String,
    pub stderr: String,
}

pub trait Host: Send + Sync + 'static {
    /// 지금 열린 vault. 없으면 러너는 쉰다.
    fn vault(&self) -> Option<String>;
    /// 엔진 명령 한 번(`ais-cli <argv>`). 화면이 받는 것과 같은 응답을 받는다.
    fn engine(&self, vault: &str, argv: &[String]) -> Result<Reply, String>;
    /// 앱이 가진 저장소 사본의 자리를 최신으로 맞춰서 돌려준다. 없으면 받는다.
    fn clone_dir(&self, full_name: &str) -> Result<PathBuf, String>;
    /// 이미 받아 둔 저장소 사본의 자리(네트워크를 쓰지 않는다). 작업 폴더를 지울 때 쓴다. 없으면 None.
    fn clone_path(&self, full_name: &str) -> Option<PathBuf>;
    /// claude가 있는지(없으면 러너는 일을 집지 않는다).
    fn claude_available(&self) -> bool;
    /// claude를 한 번 띄워 끝나기를 기다린다.
    fn run_claude(&self, call: ClaudeCall) -> Result<ClaudeOutcome, String>;
    /// 알림 센터에 알린다(설정에 따라 배너와 토스트도 뜬다).
    fn notify(&self, vault: &str, item: NewItem);
    /// 화면에 「러너 상태가 바뀌었다」고 알린다. 화면은 태스크와 실행 기록을 다시 읽는다.
    fn changed(&self, task_key: Option<&str>);
    /// 설정과 실행 기록을 두는 폴더
    fn root(&self) -> PathBuf;
    /// 엔진 jar 없이 뜬 개발 실행인지(`claude-mcp config` 결과에 등록 유지 스위치를 더할지 정한다).
    fn is_guest_build(&self) -> bool;
}

/// 실제 앱의 [Host].
pub struct AppHost<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> AppHost<R> {
    pub fn new(app: AppHandle<R>) -> Self {
        AppHost { app }
    }
}

impl<R: Runtime> Host for AppHost<R> {
    fn vault(&self) -> Option<String> {
        let state = self.app.try_state::<crate::lifecycle::HostState>()?;
        let guard = state.vault.lock().ok()?;
        guard.clone()
    }

    fn engine(&self, vault: &str, argv: &[String]) -> Result<Reply, String> {
        sidecar::call_blocking(&self.app, vault, argv)
    }

    fn clone_dir(&self, full_name: &str) -> Result<PathBuf, String> {
        let state = self.app.try_state::<github::GithubState>().ok_or("GitHub 상태를 찾지 못했다")?;
        let service = state.service();
        let root = service.clone_root();
        service.refresh_clone(full_name, &root)
    }

    fn clone_path(&self, full_name: &str) -> Option<PathBuf> {
        if full_name.contains("..") || full_name.matches('/').count() != 1 {
            return None;
        }
        let dest = self.app.try_state::<github::GithubState>()?.service().clone_root().join(full_name);
        dest.join(".git").exists().then_some(dest)
    }

    fn claude_available(&self) -> bool {
        ask::resolve_claude().is_some()
    }

    fn run_claude(&self, call: ClaudeCall) -> Result<ClaudeOutcome, String> {
        let bin = ask::resolve_claude().ok_or("claude 실행 파일을 못 찾았다. AISURADA_CLAUDE_BIN 으로 자리를 알려 줄 수 있다.")?;
        let r = ask::run_cli_process(&bin, &call.workdir, &call.vault, &call.args, &call.prompt, call.timeout.as_secs(), Some(call.cancel))
            .map_err(|e| format!("claude 를 못 띄웠다: {}", e.into_io()))?;
        Ok(ClaudeOutcome { exit_code: r.exit_code, timed_out: r.timed_out, cancelled: r.cancelled, stdout: r.stdout, stderr: r.stderr })
    }

    fn notify(&self, vault: &str, item: NewItem) {
        let _ = notifications::push(&self.app, &notifications::default_dir(), vault, item);
    }

    fn changed(&self, task_key: Option<&str>) {
        let _ = self.app.emit(ipc::RUNNER, serde_json::json!({ "taskKey": task_key }));
    }

    fn root(&self) -> PathBuf {
        default_root()
    }

    fn is_guest_build(&self) -> bool {
        tauri::is_dev()
    }
}

// ---------------------------------------------------------------------------
// 읽기 전용 작업 폴더
// ---------------------------------------------------------------------------

fn git(repo: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new(github::git_binary())
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| format!("git을 실행하지 못했다: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(format!("git {} 실패: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()))
    }
}

/// 작업 폴더의 기준이 될 커밋 이름. 태스크의 기준 브랜치가 있으면 그것, 없으면 원격의 기본 브랜치다.
pub fn base_ref(clone: &Path, base: Option<&str>) -> Result<String, String> {
    let mut candidates: Vec<String> = Vec::new();
    if let Some(b) = base {
        candidates.push(format!("origin/{b}"));
        candidates.push(b.to_string());
    } else if let Ok(head) = git(clone, &["symbolic-ref", "--short", "refs/remotes/origin/HEAD"]) {
        candidates.push(head);
    }
    if base.is_none() {
        candidates.extend(["origin/main".to_string(), "origin/master".to_string(), "HEAD".to_string()]);
    }
    for c in candidates {
        if git(clone, &["rev-parse", "--verify", "--quiet", &format!("{c}^{{commit}}")]).is_ok() {
            return Ok(c);
        }
    }
    Err(match base {
        Some(b) => format!("기준 브랜치 {b} 를 저장소에서 찾지 못했다"),
        None => "저장소의 기본 브랜치를 찾지 못했다".to_string(),
    })
}

/// [clone]에서 [work]로 읽기 전용(분리된 HEAD) 작업 폴더를 만든다. 이미 있으면 지우고 새로 만든다.
/// 저장소 사본의 브랜치와 작업 트리는 건드리지 않는다.
pub fn create_worktree(clone: &Path, work: &Path, base: Option<&str>) -> Result<(), String> {
    let reference = base_ref(clone, base)?;
    remove_worktree(clone, work);
    if let Some(parent) = work.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("작업 폴더의 부모를 못 만들었다: {e}"))?;
    }
    git(clone, &["worktree", "add", "--detach", "--force", &work.to_string_lossy(), &reference])?;
    Ok(())
}

/// 작업 폴더를 지운다. git이 아는 것은 `worktree remove`로, 남은 것은 폴더째 지우고 `prune`한다. 없어도 오류가 아니다.
pub fn remove_worktree(clone: &Path, work: &Path) {
    if work.exists() {
        let _ = git(clone, &["worktree", "remove", "--force", &work.to_string_lossy()]);
        if work.exists() {
            let _ = std::fs::remove_dir_all(work);
        }
    }
    let _ = git(clone, &["worktree", "prune"]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.email=t@t", "-c", "user.name=t", "-c", "commit.gpgsign=false"])
            .args(args)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    /// 원격과 그 사본(clone)을 만든다. 원격에는 main과 release 브랜치가 있다.
    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let temp = tempfile::tempdir().unwrap();
        let origin = temp.path().join("origin");
        std::fs::create_dir_all(&origin).unwrap();
        run(&origin, &["init", "-q", "-b", "main"]);
        std::fs::write(origin.join("README.md"), "main\n").unwrap();
        run(&origin, &["add", "."]);
        run(&origin, &["commit", "-q", "-m", "init"]);
        run(&origin, &["branch", "release"]);
        run(&origin, &["checkout", "-q", "release"]);
        std::fs::write(origin.join("README.md"), "release\n").unwrap();
        run(&origin, &["commit", "-q", "-am", "release"]);
        run(&origin, &["checkout", "-q", "main"]);
        let clone = temp.path().join("clone");
        let ok = Command::new("git").args(["clone", "-q"]).arg(&origin).arg(&clone).status().unwrap().success();
        assert!(ok);
        (temp, origin, clone)
    }

    #[test]
    fn worktree_is_detached_at_the_base_and_leaves_the_clone_alone() {
        let (temp, _origin, clone) = fixture();
        let work = temp.path().join("work/TSK-1");
        create_worktree(&clone, &work, None).unwrap();
        assert_eq!(std::fs::read_to_string(work.join("README.md")).unwrap(), "main\n");
        assert_eq!(git(&work, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap(), "HEAD", "분리된 HEAD");
        assert_eq!(git(&clone, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap(), "main", "사본의 브랜치는 그대로");
        // 다시 만들면 지우고 새로 만든다
        std::fs::write(work.join("junk"), "x").unwrap();
        create_worktree(&clone, &work, None).unwrap();
        assert!(!work.join("junk").exists());
        // 기준 브랜치를 주면 그 브랜치다
        let other = temp.path().join("work/TSK-2");
        create_worktree(&clone, &other, Some("release")).unwrap();
        assert_eq!(std::fs::read_to_string(other.join("README.md")).unwrap(), "release\n");
        assert!(create_worktree(&clone, &temp.path().join("work/TSK-3"), Some("없는-브랜치")).unwrap_err().contains("없는-브랜치"));
        // 지우면 폴더와 git의 기록이 모두 사라진다
        remove_worktree(&clone, &work);
        remove_worktree(&clone, &other);
        assert!(!work.exists() && !other.exists());
        assert_eq!(git(&clone, &["worktree", "list", "--porcelain"]).unwrap().matches("worktree ").count(), 1);
        // 이미 없는 것을 지워도 오류가 아니다
        remove_worktree(&clone, &work);
    }
}
