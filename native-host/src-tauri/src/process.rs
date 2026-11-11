//! Chapter 6: concurrent pipe draining and bounded direct-child termination.
use std::{
    io::{Read, Write},
    process::{Child, Command, ExitStatus, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

pub struct Output {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}
pub enum Outcome {
    Finished(Output),
    TimedOut,
    Cancelled,
}

/// 띄우지 못한 실패와, 띄운 뒤 기다리다 난 입출력 실패를 가른다. 4부 묻기가 두 실패에 다른 글과
/// 다른 elapsed_ms 를 답해서(5장 5.8) 둘을 구분해야 한다.
#[derive(Debug)]
pub enum RunError {
    Spawn(std::io::Error),
    Io(std::io::Error),
}
impl RunError {
    pub fn into_io(self) -> std::io::Error {
        match self {
            RunError::Spawn(error) | RunError::Io(error) => error,
        }
    }
}
impl From<std::io::Error> for RunError {
    fn from(error: std::io::Error) -> Self {
        RunError::Io(error)
    }
}

pub fn run(
    command: &mut Command,
    input: Option<Vec<u8>>,
    timeout: Duration,
) -> std::io::Result<Outcome> {
    run_typed(command, input, timeout).map_err(RunError::into_io)
}

pub fn run_typed(
    command: &mut Command,
    input: Option<Vec<u8>>,
    timeout: Duration,
) -> Result<Outcome, RunError> {
    run_with_grace(command, input, timeout, Duration::from_secs(3), None)
}

pub fn run_typed_cancellable(
    command: &mut Command,
    input: Option<Vec<u8>>,
    timeout: Duration,
    cancel_flag: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> Result<Outcome, RunError> {
    run_with_grace(command, input, timeout, Duration::from_secs(3), cancel_flag)
}

fn run_with_grace(
    command: &mut Command,
    input: Option<Vec<u8>>,
    timeout: Duration,
    grace: Duration,
    cancel_flag: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
) -> Result<Outcome, RunError> {
    let has_input = input.as_ref().is_some_and(|bytes| !bytes.is_empty());
    command
        .stdin(if has_input {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(RunError::Spawn)?;
    let deadline = Instant::now() + timeout;
    if let Some(mut stdin) = child.stdin.take() {
        thread::spawn(move || {
            let _ = stdin.write_all(input.as_deref().unwrap_or_default());
        });
    }
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let copy = tx.clone();
    thread::spawn(move || {
        let _ = copy.send((0, read(stdout)));
    });
    thread::spawn(move || {
        let _ = tx.send((1, read(stderr)));
    });
    let mut streams: [Option<std::io::Result<Vec<u8>>>; 2] = [None, None];
    loop {
        if cancel_flag.as_ref().map_or(false, |f| f.load(std::sync::atomic::Ordering::Relaxed)) {
            terminate(&mut child, grace);
            return Ok(Outcome::Cancelled);
        }
        // Prefer completion when EOF and the deadline become ready together.
        while let Ok((index, bytes)) = rx.try_recv() {
            streams[index] = Some(bytes);
        }
        if streams.iter().all(Option::is_some) {
            match child.try_wait() {
                Ok(Some(status)) => {
                    return Ok(Outcome::Finished(Output {
                        status,
                        stdout: streams[0].take().unwrap()?,
                        stderr: streams[1].take().unwrap()?,
                    }));
                }
                Ok(None) => {}
                Err(error) => {
                    terminate(&mut child, grace);
                    return Err(RunError::Io(error));
                }
            }
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            terminate(&mut child, grace);
            return Ok(Outcome::TimedOut);
        }
        if streams.iter().all(Option::is_some) {
            thread::sleep(remaining.min(Duration::from_millis(10)));
        } else {
            match rx.recv_timeout(remaining.min(Duration::from_millis(50))) {
                Ok((index, bytes)) => streams[index] = Some(bytes),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    terminate(&mut child, grace);
                    return Err(RunError::Io(std::io::Error::other(
                        "child output reader disconnected",
                    )));
                }
            }
        }
    }
}
fn read(mut stream: impl Read) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes)?;
    Ok(bytes)
}
fn terminate(child: &mut Child, grace: Duration) {
    // Do not reap before signalling: the unreaped child still owns its PID.
    let _ = Command::new("/bin/kill")
        .args(["-TERM", &child.id().to_string()])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let deadline = Instant::now() + grace;
    loop {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            let _ = child.kill();
            let _ = child.wait();
            return;
        }
        thread::sleep(remaining.min(Duration::from_millis(50)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn command(script: &str) -> Command {
        let mut c = Command::new("/bin/sh");
        c.args(["-c", script]);
        c
    }
    #[test]
    fn drains_large_independent_streams_and_writes_stdin() {
        let mut c = command("/bin/cat; /usr/bin/head -c 200000 /dev/zero >&2");
        let Outcome::Finished(output) =
            run(&mut c, Some(vec![0xFF; 200000]), Duration::from_secs(5)).unwrap()
        else {
            panic!("timeout")
        };
        assert!(output.status.success());
        assert_eq!(output.stdout, vec![0xFF; 200000]);
        assert_eq!(output.stderr.len(), 200000);
    }
    #[test]
    fn eof_stdin_and_nonzero_exit_are_preserved() {
        let Outcome::Finished(output) = run(
            &mut command("/bin/cat; printf err >&2; exit 7"),
            None,
            Duration::from_secs(3),
        )
        .unwrap() else {
            panic!("timeout")
        };
        assert_eq!(output.status.code(), Some(7));
        assert!(output.stdout.is_empty());
        assert_eq!(output.stderr, b"err");
    }
    #[test]
    fn term_allows_cleanup_and_kill_reaps_uncooperative_child() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("term");
        let mut c = command("trap 'printf cleanup > \"$MARKER\"; exit' TERM; while :; do :; done");
        c.env("MARKER", &marker);
        assert!(matches!(
            run_with_grace(
                &mut c,
                None,
                Duration::from_millis(150),
                Duration::from_secs(1),
                None
            )
            .unwrap(),
            Outcome::TimedOut
        ));
        assert_eq!(std::fs::read(marker).unwrap(), b"cleanup");
        let start = Instant::now();
        assert!(matches!(
            run_with_grace(
                &mut command("trap '' TERM; while :; do :; done"),
                None,
                Duration::from_millis(100),
                Duration::from_millis(150),
                None
            )
            .unwrap(),
            Outcome::TimedOut
        ));
        assert!(start.elapsed() < Duration::from_secs(2));
    }
    #[test]
    fn closed_streams_do_not_disable_timeout() {
        assert!(matches!(
            run_with_grace(
                &mut command("exec 1>&- 2>&-; while :; do :; done"),
                None,
                Duration::from_millis(100),
                Duration::from_millis(150),
                None
            )
            .unwrap(),
            Outcome::TimedOut
        ));
    }
    #[test]
    fn inherited_grandchild_pipe_reaches_deadline_and_grandchild_survives() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("grandchild");
        let mut c = command("(/bin/sleep 0.5; printf alive > \"$MARKER\") & exit 0");
        c.env("MARKER", &marker);
        assert!(matches!(
            run_with_grace(
                &mut c,
                None,
                Duration::from_millis(100),
                Duration::from_millis(100),
                None
            )
            .unwrap(),
            Outcome::TimedOut
        ));
        let until = Instant::now() + Duration::from_secs(3);
        while !marker.exists() && Instant::now() < until {
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(std::fs::read(marker).unwrap(), b"alive");
    }
}
