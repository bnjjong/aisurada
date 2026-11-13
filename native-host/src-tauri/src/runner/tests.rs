//! 러너 시험. 엔진·claude·GitHub·알림을 가짜로 끼우고, 실제 git 저장소 사본과 실제 파일 저장소로 흐름 전체를 돌린다.
use super::{
    host::{ClaudeCall, ClaudeOutcome, Host},
    job::{self, Ctx, Ending, Mode},
    store::{self, Phase, Settings},
    Runner,
};
use crate::{notifications::NewItem, sidecar::Reply};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

const VAULT: &str = "/test/vault";

/// claude가 할 행동의 대본.
enum Script {
    /// 이 글로 끝난다(결과 JSON으로 감싼다)
    Say(&'static str),
    /// 이 종료 코드와 표준오류로 끝난다
    Exit(i32, &'static str),
    /// claude가 `is_error: true` 결과를 낸다
    IsError(&'static str),
    /// 멈춤 신호가 올 때까지 기다린다
    WaitForCancel,
    /// 상한 시간을 넘긴 것처럼 끝난다
    Timeout,
}

struct FakeTask {
    status: String,
    title: String,
    priority: String,
    blocked: bool,
    comments: Vec<(String, String)>,
}

struct Fake {
    root: PathBuf,
    clone: PathBuf,
    tasks: Mutex<BTreeMap<String, FakeTask>>,
    script: Mutex<VecDeque<Script>>,
    calls: Mutex<Vec<(Vec<String>, String)>>,
    notes: Mutex<Vec<NewItem>>,
    changed: AtomicUsize,
    claude_ok: AtomicBool,
    order: Mutex<Vec<String>>,
}

const RULES: [(&str, &str, &[&str]); 11] = [
    ("ready-for-develop", "in-progress", &["human", "runner"]),
    ("in-progress", "waiting-answer", &["runner"]),
    ("waiting-answer", "in-progress", &["human", "runner"]),
    ("in-progress", "in-review", &["runner"]),
    ("in-review", "done", &["human", "runner"]),
    ("in-progress", "failed", &["runner"]),
    ("waiting-answer", "failed", &["runner"]),
    ("in-progress", "cancelled", &["human"]),
    ("waiting-answer", "cancelled", &["human"]),
    ("ready-for-develop", "cancelled", &["human"]),
    ("failed", "draft", &["human"]),
];

fn fail(code: i32, msg: &str) -> Result<Reply, String> {
    Ok(Reply { status: code, stdout: String::new(), stderr: msg.to_string() })
}

fn ok(v: Value) -> Result<Reply, String> {
    Ok(Reply { status: 0, stdout: v.to_string(), stderr: String::new() })
}

fn opt<'a>(argv: &'a [String], name: &str) -> Option<&'a str> {
    argv.iter().find_map(|a| a.strip_prefix(&format!("{name}=")))
}

impl Host for Fake {
    fn vault(&self) -> Option<String> {
        Some(VAULT.to_string())
    }

    fn engine(&self, _vault: &str, argv: &[String]) -> Result<Reply, String> {
        let mut tasks = self.tasks.lock().unwrap();
        match argv[0].as_str() {
            "claude-mcp" => ok(json!({"mcpServers": {"aisurada": {"type": "stdio", "command": "/j", "args": ["-jar", "/a.jar", "mcp"], "env": {"AISURADA_SIDECAR_JAR": "/s.jar"}}}})),
            "list-tasks" => {
                let want = opt(argv, "status");
                let rows: Vec<Value> = tasks
                    .iter()
                    .filter(|(_, t)| want.is_none_or(|w| w == t.status))
                    .map(|(k, t)| json!({"key": k, "title": t.title, "status": t.status, "priority": t.priority}))
                    .collect();
                ok(json!({ "tasks": rows }))
            }
            "get-task" => {
                let Some(t) = tasks.get(&argv[1]) else { return fail(3, "AIS-404 없는 태스크") };
                let activity: Vec<Value> = t.comments.iter().map(|(by, body)| json!({"kind": "comment", "by": by, "body": body})).collect();
                let blocked = t.blocked && t.status == "ready-for-develop";
                ok(json!({
                    "key": argv[1], "title": t.title, "type": "feature", "priority": t.priority, "status": t.status,
                    "repo": "o/r", "body": "본문", "criteria": [{"text": "기준 하나", "done": false}], "refs": [], "links": [],
                    "activity": activity,
                    "transitions": [{"to": "in-progress", "ok": !blocked, "reason": if blocked { "다른 태스크에 막혀 있어 시작하지 못한다" } else { "" }}]
                }))
            }
            "transition-task" => {
                let (key, to, actor, from) = (&argv[1], opt(argv, "to").unwrap(), opt(argv, "actor").unwrap_or("human"), opt(argv, "from"));
                let Some(t) = tasks.get_mut(key) else { return fail(3, "AIS-404") };
                if from.is_some_and(|f| f != t.status) {
                    return fail(4, &format!("AIS-409 지금 상태는 {} 이다", t.status));
                }
                let Some((_, _, actors)) = RULES.iter().find(|(f, tt, _)| *f == t.status && *tt == to) else {
                    return fail(4, &format!("AIS-409 {} 에서 {} 로는 바꿀 수 없다", t.status, to));
                };
                if !actors.contains(&actor) {
                    return fail(4, &format!("AIS-409 {} → {} 는 {:?} 만 바꾼다", t.status, to, actors));
                }
                if t.blocked && t.status == "ready-for-develop" && to == "in-progress" {
                    return fail(4, "AIS-409 다른 태스크에 막혀 있어 시작하지 못한다");
                }
                t.status = to.to_string();
                ok(json!({"key": key, "status": to}))
            }
            "add-task-comment" => {
                let Some(t) = tasks.get_mut(&argv[1]) else { return fail(3, "AIS-404") };
                t.comments.push((opt(argv, "actor").unwrap_or("human").to_string(), opt(argv, "body").unwrap_or("").to_string()));
                ok(json!({"ok": true}))
            }
            other => fail(2, &format!("알 수 없는 이름: {other}")),
        }
    }

    fn clone_dir(&self, _full_name: &str) -> Result<PathBuf, String> {
        Ok(self.clone.clone())
    }

    fn clone_path(&self, _full_name: &str) -> Option<PathBuf> {
        Some(self.clone.clone())
    }

    fn claude_available(&self) -> bool {
        self.claude_ok.load(Ordering::SeqCst)
    }

    fn run_claude(&self, call: ClaudeCall) -> Result<ClaudeOutcome, String> {
        assert!(call.workdir.join("README.md").exists(), "claude는 읽기 전용 작업 폴더에서 돈다");
        assert!(call.args.contains(&"dontAsk".to_string()), "허용한 도구 밖은 거절한다");
        self.calls.lock().unwrap().push((call.args.clone(), call.prompt.clone()));
        self.order.lock().unwrap().push(call.prompt.lines().find(|l| l.starts_with("## 태스크")).unwrap_or("(이어 가기)").to_string());
        let script = self.script.lock().unwrap().pop_front().expect("대본이 모자라다");
        let done = |text: &str| ClaudeOutcome {
            exit_code: Some(0),
            timed_out: false,
            cancelled: false,
            stdout: json!({"type": "result", "is_error": false, "result": text, "session_id": "s", "total_cost_usd": 0.01, "usage": {"input_tokens": 10, "output_tokens": 5}}).to_string(),
            stderr: String::new(),
        };
        Ok(match script {
            Script::Say(text) => done(text),
            Script::IsError(text) => ClaudeOutcome {
                exit_code: Some(0),
                timed_out: false,
                cancelled: false,
                stdout: json!({"type": "result", "is_error": true, "result": text}).to_string(),
                stderr: String::new(),
            },
            Script::Exit(code, err) => ClaudeOutcome { exit_code: Some(code), timed_out: false, cancelled: false, stdout: String::new(), stderr: err.to_string() },
            Script::Timeout => ClaudeOutcome { exit_code: None, timed_out: true, cancelled: false, stdout: String::new(), stderr: String::new() },
            Script::WaitForCancel => {
                let until = Instant::now() + Duration::from_secs(10);
                while !call.cancel.load(Ordering::SeqCst) && Instant::now() < until {
                    std::thread::sleep(Duration::from_millis(10));
                }
                ClaudeOutcome { exit_code: None, timed_out: false, cancelled: true, stdout: String::new(), stderr: String::new() }
            }
        })
    }

    fn notify(&self, _vault: &str, item: NewItem) {
        self.notes.lock().unwrap().push(item);
    }

    fn changed(&self, _task_key: Option<&str>) {
        self.changed.fetch_add(1, Ordering::SeqCst);
    }

    fn root(&self) -> PathBuf {
        self.root.clone()
    }

    fn is_guest_build(&self) -> bool {
        false
    }
}

fn git(dir: &Path, args: &[&str]) {
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

struct Env {
    _temp: tempfile::TempDir,
    fake: Arc<Fake>,
}

fn env_with(tasks: &[(&str, &str, &str)], script: Vec<Script>) -> Env {
    let temp = tempfile::tempdir().unwrap();
    let origin = temp.path().join("origin");
    std::fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "-q", "-b", "main"]);
    std::fs::write(origin.join("README.md"), "hello\n").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "init"]);
    let clone = temp.path().join("clone");
    assert!(Command::new("git").args(["clone", "-q"]).arg(&origin).arg(&clone).status().unwrap().success());
    let tasks = tasks
        .iter()
        .map(|(key, status, priority)| {
            (key.to_string(), FakeTask { status: status.to_string(), title: format!("제목 {key}"), priority: priority.to_string(), blocked: false, comments: vec![] })
        })
        .collect();
    let fake = Arc::new(Fake {
        root: temp.path().join("runs"),
        clone,
        tasks: Mutex::new(tasks),
        script: Mutex::new(script.into()),
        calls: Mutex::new(vec![]),
        notes: Mutex::new(vec![]),
        changed: AtomicUsize::new(0),
        claude_ok: AtomicBool::new(true),
        order: Mutex::new(vec![]),
    });
    Env { _temp: temp, fake }
}

impl Env {
    fn ctx(&self) -> Ctx {
        Ctx { host: self.fake.clone(), vault: VAULT.to_string(), settings: Settings::default() }
    }

    fn run(&self, key: &str, mode: Mode) -> Ending {
        job::execute(&self.ctx(), key, mode, Arc::default(), Arc::default())
    }

    fn status(&self, key: &str) -> String {
        self.fake.tasks.lock().unwrap()[key].status.clone()
    }

    fn comments(&self, key: &str) -> Vec<(String, String)> {
        self.fake.tasks.lock().unwrap()[key].comments.clone()
    }

    fn runs(&self, key: &str) -> Vec<store::RunRecord> {
        store::list_runs(&self.fake.root, VAULT, key)
    }

    fn notes(&self) -> Vec<NewItem> {
        self.fake.notes.lock().unwrap().clone()
    }

    fn worktrees(&self) -> usize {
        let out = Command::new("git").arg("-C").arg(&self.fake.clone).args(["worktree", "list", "--porcelain"]).output().unwrap();
        String::from_utf8_lossy(&out.stdout).matches("worktree ").count()
    }

    fn runner(&self) -> Arc<Runner> {
        Runner::new(self.fake.clone())
    }
}

fn wait_idle(runner: &Runner) {
    let until = Instant::now() + Duration::from_secs(20);
    while !runner.active.lock().unwrap().is_empty() {
        assert!(Instant::now() < until, "러너가 끝나지 않는다");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn report_moves_the_task_to_review_and_cleans_up() {
    let env = env_with(&[("TSK-1", "ready-for-develop", "medium")], vec![Script::Say("[보고] README에 빌드 방법이 있다.\n근거: README.md:1")]);
    assert_eq!(env.run("TSK-1", Mode::Fresh), Ending::Reported);
    assert_eq!(env.status("TSK-1"), "in-review");
    let comments = env.comments("TSK-1");
    assert_eq!(comments.len(), 1);
    assert_eq!(comments[0].0, "runner");
    assert!(comments[0].1.starts_with("**분석 보고**") && comments[0].1.contains("근거: README.md:1"));
    let runs = env.runs("TSK-1");
    assert_eq!(runs.len(), 1);
    let r = &runs[0];
    assert_eq!((r.phase, r.turns, r.questions), (Phase::Done, 1, 0));
    assert!((r.cost_usd - 0.01).abs() < 1e-9 && r.input_tokens == 10 && r.output_tokens == 5);
    assert!(r.summary.as_deref().unwrap().contains("빌드 방법") && r.workdir.is_none() && r.ended_at.is_some());
    assert_eq!(env.worktrees(), 1, "작업 폴더를 지웠다(사본만 남는다)");
    let notes = env.notes();
    assert_eq!(notes.len(), 1);
    assert_eq!((notes[0].kind.as_str(), notes[0].task_key.as_deref()), ("pr", Some("TSK-1")));
    assert_eq!(notes[0].id.as_deref(), Some(format!("run-{}-done", r.run_id).as_str()), "같은 알림을 두 번 쌓지 않으려는 id");
    // 이벤트에 일곱 걸음이 차례로 남는다
    let events = store::read_events(&env.fake.root, VAULT, "TSK-1", &r.run_id);
    let steps: Vec<&str> = events.iter().map(|e| e.step.as_str()).collect();
    for want in ["받기", "집기", "판단", "받아오기", "일하기", "내보내기", "정리"] {
        assert!(steps.contains(&want), "{want} 걸음이 없다: {steps:?}");
    }
    // 프롬프트에 태스크가 실렸고, 읽기 전용 인자로 불렸다
    let calls = env.fake.calls.lock().unwrap();
    assert!(calls[0].1.contains("TSK-1") && calls[0].1.contains("기준 하나"));
    assert!(calls[0].0.contains(&"--session-id".to_string()) && calls[0].0.contains(&r.session_id));
}

#[test]
fn question_waits_for_the_human_then_resumes_the_same_session() {
    let env = env_with(&[("TSK-1", "ready-for-develop", "medium")], vec![Script::Say("[질문] 어느 브랜치를 기준으로 볼까요?"), Script::Say("[보고] main 기준으로 확인했다.")]);
    assert_eq!(env.run("TSK-1", Mode::Fresh), Ending::Asked);
    assert_eq!(env.status("TSK-1"), "waiting-answer");
    let r = env.runs("TSK-1").remove(0);
    assert_eq!((r.phase, r.questions), (Phase::WaitingAnswer, 1));
    assert_eq!(r.question.as_deref(), Some("어느 브랜치를 기준으로 볼까요?"));
    assert!(r.workdir.is_some(), "답을 기다리는 동안 작업 폴더를 남긴다");
    assert_eq!(env.worktrees(), 2);
    assert_eq!(env.notes()[0].kind, "question");
    assert!(env.comments("TSK-1")[0].1.starts_with("**질문**"));

    // 사람이 답한다: 러너가 댓글·전이·기록을 처리하고 이어 간다
    let runner = env.runner();
    runner.answer("TSK-1", "main 으로 봐 주세요").unwrap();
    wait_idle(&runner);
    assert_eq!(env.status("TSK-1"), "in-review");
    let comments = env.comments("TSK-1");
    assert!(comments.iter().any(|(by, b)| by == "human" && b.contains("main 으로 봐 주세요")));
    let r = env.runs("TSK-1").remove(0);
    assert_eq!((r.phase, r.turns, r.questions), (Phase::Done, 2, 1));
    assert!(r.answer.is_none() && r.question.is_none());
    assert_eq!(env.worktrees(), 1);
    let calls = env.fake.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    let resume_at = calls[1].0.iter().position(|a| a == "--resume").expect("같은 세션을 --resume 으로 이어 간다");
    assert_eq!(calls[1].0[resume_at + 1], r.session_id);
    assert!(calls[1].1.contains("main 으로 봐 주세요") && calls[1].1.contains("어느 브랜치를 기준으로 볼까요?"));
}

#[test]
fn answering_requires_a_waiting_run_and_a_real_answer() {
    let env = env_with(&[("TSK-1", "ready-for-develop", "medium")], vec![]);
    let runner = env.runner();
    assert!(runner.answer("TSK-1", "답").unwrap_err().contains("기다리는 실행이 없"));
    assert!(runner.answer("TSK-1", "  ").unwrap_err().contains("비어"));
    assert!(runner.answer("TSK-1", &"가".repeat(4001)).unwrap_err().contains("너무 길"));
    assert!(runner.answer("../x", "답").unwrap_err().contains("이상"));
}

#[test]
fn failures_mark_the_task_failed_and_notify() {
    for (script, needle) in [
        (Script::Exit(1, "boom"), "종료 코드 1"),
        (Script::Timeout, "시간 제한"),
        (Script::IsError("권한 오류"), "오류를 알렸다"),
    ] {
        let env = env_with(&[("TSK-1", "ready-for-develop", "medium")], vec![script]);
        assert_eq!(env.run("TSK-1", Mode::Fresh), Ending::Failed, "{needle}");
        assert_eq!(env.status("TSK-1"), "failed");
        let r = env.runs("TSK-1").remove(0);
        assert_eq!(r.phase, Phase::Failed);
        assert!(r.error.as_deref().unwrap().contains(needle), "{needle}: {:?}", r.error);
        assert!(env.comments("TSK-1")[0].1.contains("실행이 실패했어요"));
        assert_eq!(env.notes()[0].kind, "failed");
        assert_eq!(env.worktrees(), 1, "실패해도 작업 폴더를 지운다");
    }
}

#[test]
fn a_task_that_is_not_ready_or_is_blocked_is_not_picked() {
    let env = env_with(&[("TSK-1", "draft", "medium"), ("TSK-2", "ready-for-develop", "medium")], vec![]);
    env.fake.tasks.lock().unwrap().get_mut("TSK-2").unwrap().blocked = true;
    assert!(matches!(env.run("TSK-1", Mode::Fresh), Ending::NotPicked(m) if m.contains("Ready")));
    assert!(matches!(env.run("TSK-2", Mode::Fresh), Ending::NotPicked(m) if m.contains("막혀")));
    assert_eq!((env.status("TSK-1").as_str(), env.status("TSK-2").as_str()), ("draft", "ready-for-develop"));
    assert!(env.runs("TSK-2").is_empty(), "집지 못했으면 실행 기록을 만들지 않는다");
    assert!(env.notes().is_empty());
}

#[test]
fn a_human_stop_cancels_the_task() {
    let env = env_with(&[("TSK-1", "ready-for-develop", "medium")], vec![Script::WaitForCancel]);
    let runner = env.runner();
    runner.run_now("TSK-1").unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    while env.fake.calls.lock().unwrap().is_empty() {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(10));
    }
    runner.stop("TSK-1").unwrap();
    wait_idle(&runner);
    assert_eq!(env.status("TSK-1"), "cancelled");
    let r = env.runs("TSK-1").remove(0);
    assert_eq!(r.phase, Phase::Cancelled);
    assert_eq!(env.worktrees(), 1);
    assert!(env.notes().is_empty(), "사람이 직접 멈춘 것은 알리지 않는다");
    assert!(runner.stop("TSK-1").unwrap_err().contains("멈출 실행이 없"));
}

#[test]
fn stopping_a_run_that_waits_for_an_answer_cancels_it_directly() {
    let env = env_with(&[("TSK-1", "ready-for-develop", "medium")], vec![Script::Say("[질문] 무엇을?")]);
    assert_eq!(env.run("TSK-1", Mode::Fresh), Ending::Asked);
    let runner = env.runner();
    runner.stop("TSK-1").unwrap();
    assert_eq!(env.status("TSK-1"), "cancelled");
    assert_eq!(env.runs("TSK-1")[0].phase, Phase::Cancelled);
    assert_eq!(env.worktrees(), 1);
}

#[test]
fn closing_the_app_keeps_the_run_and_the_next_launch_resumes_it() {
    let env = env_with(&[("TSK-1", "ready-for-develop", "medium")], vec![Script::WaitForCancel, Script::Say("[보고] 이어서 마쳤다.")]);
    let runner = env.runner();
    runner.run_now("TSK-1").unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    while env.fake.calls.lock().unwrap().is_empty() {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(10));
    }
    runner.shutdown(Duration::from_secs(5));
    // 앱을 닫았다: 태스크는 In Progress, 기록은 running, 작업 폴더는 남는다
    assert_eq!(env.status("TSK-1"), "in-progress");
    let r = env.runs("TSK-1").remove(0);
    assert_eq!(r.phase, Phase::Running);
    assert!(r.workdir.is_some() && env.worktrees() == 2);
    assert!(env.notes().is_empty() && env.comments("TSK-1").is_empty(), "닫아서 끊긴 것은 실패가 아니다");
    // 다시 켠다: 첫 훑기가 끊긴 실행을 같은 세션으로 이어 간다
    let again = env.runner();
    again.tick();
    wait_idle(&again);
    assert_eq!(env.status("TSK-1"), "in-review");
    let r2 = env.runs("TSK-1").remove(0);
    assert_eq!((r2.run_id.as_str(), r2.phase, r2.turns), (r.run_id.as_str(), Phase::Done, 2));
    let calls = env.fake.calls.lock().unwrap();
    assert!(calls[1].0.contains(&"--resume".to_string()) && calls[1].1.contains("끊겼다"));
}

#[test]
fn resuming_recreates_a_missing_worktree() {
    let env = env_with(&[("TSK-1", "ready-for-develop", "medium")], vec![Script::Say("[질문] 하나?"), Script::Say("[보고] 됐다.")]);
    assert_eq!(env.run("TSK-1", Mode::Fresh), Ending::Asked);
    let r = env.runs("TSK-1").remove(0);
    // 며칠 사이에 작업 폴더가 사라졌다
    std::fs::remove_dir_all(r.workdir.as_deref().unwrap()).unwrap();
    let runner = env.runner();
    runner.answer("TSK-1", "응").unwrap();
    wait_idle(&runner);
    assert_eq!(env.status("TSK-1"), "in-review", "읽기 전용이라 작업 폴더를 다시 만들어 이어 간다");
}

#[test]
fn a_lost_claude_session_falls_back_to_a_fresh_one_with_the_comments() {
    let env = env_with(
        &[("TSK-1", "ready-for-develop", "medium")],
        vec![Script::Say("[질문] 하나?"), Script::Exit(1, "No conversation found with session ID"), Script::Say("[보고] 새 세션으로 마쳤다.")],
    );
    assert_eq!(env.run("TSK-1", Mode::Fresh), Ending::Asked);
    let first = env.runs("TSK-1").remove(0);
    let runner = env.runner();
    runner.answer("TSK-1", "둘째 안").unwrap();
    wait_idle(&runner);
    assert_eq!(env.status("TSK-1"), "in-review");
    let calls = env.fake.calls.lock().unwrap();
    assert_eq!(calls.len(), 3);
    assert!(calls[2].0.contains(&"--session-id".to_string()), "새 세션");
    assert!(!calls[2].0.contains(&first.session_id), "옛 세션 id를 다시 쓰지 않는다");
    assert!(calls[2].1.contains("## 태스크 TSK-1") && calls[2].1.contains("둘째 안"), "댓글에 질문과 답이 들어 있어 맥락이 이어진다");
}

#[test]
fn the_question_limit_forces_a_report() {
    let mut script = vec![];
    for _ in 0..5 {
        script.push(Script::Say("[질문] 또 질문?"));
    }
    script.push(Script::Say("[질문] 한 번만 더?"));
    script.push(Script::Say("[보고] 모른다고 보고한다."));
    let env = env_with(&[("TSK-1", "ready-for-develop", "medium")], script);
    let runner = env.runner();
    assert_eq!(env.run("TSK-1", Mode::Fresh), Ending::Asked);
    for i in 0..4 {
        runner.answer("TSK-1", &format!("답 {i}")).unwrap();
        wait_idle(&runner);
        assert_eq!(env.status("TSK-1"), "waiting-answer", "{i}번째 답 뒤에도 다시 묻는다");
    }
    assert_eq!(env.runs("TSK-1")[0].questions, 5);
    // 다섯 번째 답 뒤의 질문은 한도를 넘으므로 보고를 쓰게 한다
    runner.answer("TSK-1", "마지막 답").unwrap();
    wait_idle(&runner);
    assert_eq!(env.status("TSK-1"), "in-review");
    let calls = env.fake.calls.lock().unwrap();
    assert!(calls.last().unwrap().1.contains("더는 묻지 말고"));
}

#[test]
fn recovery_closes_runs_whose_task_was_moved_by_a_human() {
    let env = env_with(&[("TSK-1", "ready-for-develop", "medium"), ("TSK-2", "ready-for-develop", "medium")], vec![Script::Say("[질문] 하나?"), Script::Say("[질문] 둘?")]);
    assert_eq!(env.run("TSK-1", Mode::Fresh), Ending::Asked);
    assert_eq!(env.run("TSK-2", Mode::Fresh), Ending::Asked);
    // 사람이 두 태스크를 손으로 취소했다(앱이 꺼진 동안)
    for key in ["TSK-1", "TSK-2"] {
        env.fake.tasks.lock().unwrap().get_mut(key).unwrap().status = "cancelled".into();
    }
    env.runner().tick();
    for key in ["TSK-1", "TSK-2"] {
        let r = env.runs(key).remove(0);
        assert_eq!(r.phase, Phase::Abandoned, "{key}");
        assert!(r.workdir.is_none() && r.error.is_some());
    }
    assert_eq!(env.worktrees(), 1, "남은 작업 폴더를 지웠다");
}

#[test]
fn pickup_takes_ready_tasks_by_priority_within_the_concurrency_limit() {
    let env = env_with(
        &[("TSK-1", "ready-for-develop", "low"), ("TSK-2", "ready-for-develop", "high"), ("TSK-3", "ready-for-develop", "medium"), ("TSK-4", "draft", "high")],
        vec![Script::Say("[보고] 둘."), Script::Say("[보고] 셋."), Script::Say("[보고] 하나.")],
    );
    let runner = env.runner();
    // 자동 집기가 꺼져 있으면(기본) 아무것도 집지 않는다
    runner.tick();
    wait_idle(&runner);
    assert!(env.fake.calls.lock().unwrap().is_empty());
    runner.set_settings(Settings { auto_pickup: true, concurrency: 2, model: "sonnet".into() }).unwrap();
    runner.tick();
    wait_idle(&runner);
    let order: Vec<String> = env.fake.order.lock().unwrap().clone();
    assert_eq!(order.len(), 2, "한 번에 둘까지");
    assert!(order[0].contains("TSK-2") || order[1].contains("TSK-2"));
    assert_eq!((env.status("TSK-2").as_str(), env.status("TSK-3").as_str(), env.status("TSK-1").as_str()), ("in-review", "in-review", "ready-for-develop"));
    assert_eq!(env.status("TSK-4"), "draft");
    runner.tick();
    wait_idle(&runner);
    assert_eq!(env.status("TSK-1"), "in-review");
}

#[test]
fn pickup_does_nothing_without_claude() {
    let env = env_with(&[("TSK-1", "ready-for-develop", "high")], vec![]);
    env.fake.claude_ok.store(false, Ordering::SeqCst);
    let runner = env.runner();
    runner.set_settings(Settings { auto_pickup: true, ..Settings::default() }).unwrap();
    runner.tick();
    wait_idle(&runner);
    assert_eq!(env.status("TSK-1"), "ready-for-develop");
    assert!(runner.run_now("TSK-1").unwrap_err().contains("claude"));
}

#[test]
fn run_now_explains_why_it_cannot_run() {
    let env = env_with(&[("TSK-1", "draft", "medium"), ("TSK-2", "ready-for-develop", "medium"), ("TSK-3", "ready-for-develop", "medium")], vec![]);
    env.fake.tasks.lock().unwrap().get_mut("TSK-2").unwrap().blocked = true;
    let runner = env.runner();
    assert!(runner.run_now("TSK-1").unwrap_err().contains("Ready For Develop"));
    assert!(runner.run_now("TSK-2").unwrap_err().contains("막혀"));
    assert!(runner.run_now("TSK-99").is_err());
    assert!(runner.run_now("../x").unwrap_err().contains("이상"));
    // 동시 실행 한도
    runner.set_settings(Settings { concurrency: 1, ..Settings::default() }).unwrap();
    runner.active.lock().unwrap().insert("TSK-8".into(), super::Active { cancel: Arc::default() });
    assert!(runner.run_now("TSK-3").unwrap_err().contains("동시에"));
}

#[test]
fn status_lists_active_and_waiting_runs() {
    let env = env_with(&[("TSK-1", "ready-for-develop", "medium")], vec![Script::Say("[질문] 하나?")]);
    let runner = env.runner();
    assert_eq!(runner.status().unwrap().waiting.len(), 0);
    env.run("TSK-1", Mode::Fresh);
    let s = runner.status().unwrap();
    assert_eq!((s.running, s.active.len(), s.waiting.len()), (0, 0, 1));
    assert_eq!(s.waiting[0].task_key, "TSK-1");
    assert!(s.claude_available && !s.settings.auto_pickup);
    let runs = runner.runs("TSK-1").unwrap();
    assert_eq!(runs.runs.len(), 1);
    assert!(runs.events.iter().any(|e| e.step == "내보내기"));
    assert!(runner.runs("nope").is_err());
}

// ---------------------------------------------------------------------------
// 실제 ais-cli·격리 엔진·claude로 끝까지 도는 시험(손으로만 돌린다)
// ---------------------------------------------------------------------------

/// 가짜 대신 진짜 `ais-cli`(java -jar)와 진짜 claude를 쓰는 호스트. 저장소 사본만 임시 git 저장소다.
struct RealHost {
    root: PathBuf,
    clone: PathBuf,
    vault: String,
    env: Vec<(String, String)>,
    cli_jar: String,
}

impl Host for RealHost {
    fn vault(&self) -> Option<String> {
        Some(self.vault.clone())
    }

    fn engine(&self, _vault: &str, argv: &[String]) -> Result<Reply, String> {
        let out = Command::new("java")
            .arg(format!("-Duser.home={}", self.env.iter().find(|(k, _)| k == "E2E_HOME").unwrap().1))
            .arg("-jar")
            .arg(&self.cli_jar)
            .args(argv)
            .envs(self.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .output()
            .map_err(|e| e.to_string())?;
        let mut stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        if argv[0] == "claude-mcp" {
            // 격리 엔진으로 이어지도록 MCP 서버 환경에 소켓과 홈을 더한다(실제 앱은 필요 없는 시험용 배선)
            let mut v: Value = serde_json::from_str(&stdout).map_err(|e| e.to_string())?;
            for key in ["AISURADA_SIDECAR_SOCKET", "AISURADA_SIDECAR_JAR"] {
                if let Some((_, val)) = self.env.iter().find(|(k, _)| k == key) {
                    v["mcpServers"]["aisurada"]["env"][key] = json!(val);
                }
            }
            let home = &self.env.iter().find(|(k, _)| k == "E2E_HOME").unwrap().1;
            v["mcpServers"]["aisurada"]["args"] = json!([format!("-Duser.home={home}"), "-jar", self.cli_jar, "mcp"]);
            stdout = v.to_string();
        }
        Ok(Reply { status: out.status.code().unwrap_or(-1), stdout, stderr: String::from_utf8_lossy(&out.stderr).into_owned() })
    }

    fn clone_dir(&self, _full_name: &str) -> Result<PathBuf, String> {
        Ok(self.clone.clone())
    }

    fn clone_path(&self, _full_name: &str) -> Option<PathBuf> {
        Some(self.clone.clone())
    }

    fn claude_available(&self) -> bool {
        crate::ask::resolve_claude().is_some()
    }

    fn run_claude(&self, call: ClaudeCall) -> Result<ClaudeOutcome, String> {
        let bin = crate::ask::resolve_claude().ok_or("claude 없음")?;
        let r = crate::ask::run_cli_process(&bin, &call.workdir, &call.vault, &call.args, &call.prompt, call.timeout.as_secs(), Some(call.cancel))
            .map_err(|e| e.into_io().to_string())?;
        Ok(ClaudeOutcome { exit_code: r.exit_code, timed_out: r.timed_out, cancelled: r.cancelled, stdout: r.stdout, stderr: r.stderr })
    }

    fn notify(&self, _vault: &str, item: NewItem) {
        eprintln!("[알림] {} / {} / {}", item.kind, item.title, item.body);
    }

    fn changed(&self, _task_key: Option<&str>) {}

    fn root(&self) -> PathBuf {
        self.root.clone()
    }

    fn is_guest_build(&self) -> bool {
        false
    }
}

/// 격리 엔진(`AISURADA_VAULT`, `AISURADA_SIDECAR_SOCKET`, `E2E_HOME`, `E2E_CLI_JAR`)이 떠 있어야 한다.
/// 태스크 하나를 만들어 Ready로 보낸 뒤 진짜 claude가 저장소 사본을 읽고 보고하는지 본다. 토큰을 조금 쓴다.
#[test]
#[ignore = "진짜 claude를 부른다. 격리 엔진을 띄우고 환경변수를 준 뒤 --ignored 로 돌린다"]
fn real_cli_and_claude_report_end_to_end() {
    let var = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k} 가 필요하다"));
    let temp = tempfile::tempdir().unwrap();
    let origin = temp.path().join("origin");
    std::fs::create_dir_all(&origin).unwrap();
    git(&origin, &["init", "-q", "-b", "main"]);
    std::fs::write(origin.join("README.md"), "# 데모\n\n빌드: `make build`\n시험: `make test`\n").unwrap();
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "init"]);
    let clone = temp.path().join("clone");
    assert!(Command::new("git").args(["clone", "-q"]).arg(&origin).arg(&clone).status().unwrap().success());
    let env: Vec<(String, String)> = ["AISURADA_VAULT", "AISURADA_SIDECAR_SOCKET", "AISURADA_SIDECAR_JAR", "E2E_HOME"]
        .iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| (k.to_string(), v)))
        .collect();
    let host = Arc::new(RealHost { root: temp.path().join("runs"), clone, vault: var("AISURADA_VAULT"), env, cli_jar: var("E2E_CLI_JAR") });
    let ctx = Ctx { host: host.clone(), vault: host.vault.clone(), settings: Settings { model: "haiku".into(), ..Settings::default() } };
    let created = ctx
        .call(&["create-task", "title=README에서 빌드와 시험 명령 찾기", "repo=o/r", "criteria=[\"빌드 명령을 알려 준다\",\"시험 명령을 알려 준다\"]", "body=저장소의 README를 읽고 빌드와 시험 명령을 알려 줘. 질문 없이 바로 보고해."])
        .unwrap();
    let key = created["key"].as_str().unwrap().to_string();
    ctx.transition(&key, "ready-for-develop", "human", "draft").unwrap();
    let ending = job::execute(&ctx, &key, Mode::Fresh, Arc::default(), Arc::default());
    let task = ctx.call(&["get-task", &key]).unwrap();
    let comments: Vec<String> = task["activity"].as_array().unwrap().iter().filter(|a| a["kind"] == "comment").map(|a| a["body"].as_str().unwrap().to_string()).collect();
    eprintln!("끝: {ending:?} / 상태 {} / 댓글 {comments:#?}", task["status"]);
    let runs = store::list_runs(&host.root, &host.vault, &key);
    eprintln!("기록: {:#?}", runs[0]);
    assert!(matches!(ending, Ending::Reported | Ending::Asked), "{ending:?}");
    assert!(runs[0].turns >= 1 && runs[0].cost_usd > 0.0);
    if ending == Ending::Reported {
        assert_eq!(task["status"], "in-review");
        assert!(comments.iter().any(|c| c.contains("make build")), "{comments:?}");
    }
}
