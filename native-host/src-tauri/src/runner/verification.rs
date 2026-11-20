//! QA가 요청한 명령은 인덱스에 담긴 소스의 임시 사본에서 실행한다.
use super::host::git;
use crate::process::{self, Outcome};
use std::{
    fs::File,
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

const PROFILE: &str = r#"(version 1)
(deny default)
(allow process-fork process-exec sysctl-read)
(allow file-read-metadata)
(allow file-read* (literal "/") (subpath "/bin") (subpath "/usr") (subpath "/System") (subpath "/Library/Developer")
 (subpath "/Applications/Xcode.app") (subpath "/opt/homebrew") (literal "/dev/null")
 (literal "/dev/random") (literal "/dev/urandom") (subpath (param "ROOT")))
(allow file-write* (subpath (param "ROOT")) (literal "/dev/null"))"#;

pub(super) fn check(
    work: &Path,
    command: &str,
    budget: Duration,
    cancel: Arc<AtomicBool>,
) -> Result<(bool, String), String> {
    let temp = tempfile::tempdir().map_err(|e| e.to_string())?;
    let root = temp.path().canonicalize().map_err(|e| e.to_string())?;
    let src = root.join("src");
    std::fs::create_dir(&src).map_err(|e| e.to_string())?;
    git(
        work,
        &[
            "checkout-index",
            "--all",
            &format!("--prefix={}/", src.display()),
        ],
    )?;
    let log_path = root.join("output");
    let out = File::create(&log_path).map_err(|e| e.to_string())?;
    let err = out.try_clone().map_err(|e| e.to_string())?;
    let mut cmd = Command::new("/usr/bin/sandbox-exec");
    cmd.arg("-D")
        .arg(format!("ROOT={}", root.display()))
        .args(["-p", PROFILE, "/bin/sh", "-c", command])
        .current_dir(&src)
        .env_clear()
        .env("HOME", &root)
        .env("TMPDIR", &root)
        .env(
            "PATH",
            "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin",
        )
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .process_group(0);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("격리한 검사 명령을 시작할 수 없다: {e}"))?;
    let pid = child.id() as i32;
    let start = Instant::now();
    let outcome = loop {
        if cancel.load(Ordering::SeqCst) {
            break Err("검사가 중지됐다".into());
        }
        if start.elapsed() >= budget {
            break Err("검사 시간이 초과됐다".into());
        }
        if std::fs::metadata(&log_path).is_ok_and(|m| m.len() > 4_000_000) {
            break Err("검사 출력이 4MB를 넘었다".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => break Err(e.to_string()),
        }
    };
    // 셸이 끝나거나 취소돼도 그 셸이 시작한 검사 프로세스가 남지 않게 한다.
    unsafe { libc::kill(-pid, libc::SIGKILL) };
    let _ = child.wait();
    let status = outcome?;
    let bytes = std::fs::read(log_path).map_err(|e| e.to_string())?;
    let tail = String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(10000)..]);
    Ok((status.success(), format!("{status}\n{tail}")))
}

pub(super) fn apply(work: &Path, patch: &str, cancel: Arc<AtomicBool>) -> Result<(), String> {
    let mut cmd = Command::new(crate::github::git_binary());
    cmd.arg("-C").arg(work).args([
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.fsmonitor=false",
        "apply",
        "--index",
        "--recount",
        "--whitespace=nowarn",
        "-",
    ]);
    let input = format!("{}\n", patch.trim_end_matches('\n'));
    match process::run_typed_cancellable(
        &mut cmd,
        Some(input.into_bytes()),
        Duration::from_secs(20),
        Some(cancel),
    )
    .map_err(|e| e.into_io().to_string())?
    {
        Outcome::Finished(out) if out.status.success() => Ok(()),
        Outcome::Finished(out) => Err(String::from_utf8_lossy(&out.stderr).into_owned()),
        _ => Err("패치 적용이 중단됐다".into()),
    }
}
