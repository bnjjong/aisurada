//! 태스크 하나를 처음부터 끝까지 돌리는 실행 한 번. 일곱 걸음(받기 집기 판단 받아오기 일하기 내보내기 정리)을 밟는다.
//!
//! 읽기 전용 단계의 약속:
//! - claude는 저장소의 읽기 전용 작업 폴더와 aisurada 읽기 도구만 쓴다(`protocol::claude_args`).
//! - 끝은 둘 중 하나다. `[보고]`면 보고를 댓글로 남기고 In Review로 보내고, `[질문]`이면 질문을 댓글로 남기고
//!   Waiting Answer로 보낸 채 사람의 답을 기다린다. 답이 오면 같은 세션을 `--resume`으로 이어 간다.
//! - 태스크의 상태는 모두 엔진의 `transition-task`로만 바꾼다(규칙 검사를 엔진 한 곳에 둔다).
//! - 앱을 닫아 끊기면 기록을 `running`으로 남겨 두고, 다시 켜면 이어 간다(결정 7-나).
use super::{
    host::{self, ClaudeCall, Host},
    protocol::{self, Reply, TaskInfo},
    store::{self, Phase, RunRecord, Settings, MAX_QUESTIONS, MAX_WORK_MINUTES},
};
use crate::notifications::NewItem;
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

/// 실행을 어떻게 시작하는지.
#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    /// Ready For Develop인 태스크를 새로 집는다.
    Fresh,
    /// 사람의 답이 왔다. 기록에 답이 들어 있다.
    Answer { run_id: String },
    /// 앱을 껐다 켜서 끊긴 실행을 이어 간다.
    Recover { run_id: String },
}

/// 실행이 어떻게 끝났는지.
#[derive(Debug, Clone, PartialEq)]
pub enum Ending {
    /// 집지 못했다(태스크가 Ready가 아니거나, 막혀 있거나, 그 사이 바뀌었다). 까닭을 담는다.
    NotPicked(String),
    /// 보고를 올리고 In Review로 보냈다.
    Reported,
    /// 질문을 올리고 사람의 답을 기다린다.
    Asked,
    /// 실패로 끝냈다.
    Failed,
    /// 사람이 중지해서 Cancelled로 끝냈다.
    Stopped,
    /// 앱을 닫아 끊겼다. 기록은 running으로 남아 다음에 이어 간다.
    Interrupted,
    /// 사람이 태스크를 손으로 옮겨서 더 이어 갈 이유가 없다.
    Abandoned,
}

pub struct Ctx {
    pub host: Arc<dyn Host>,
    pub vault: String,
    pub settings: Settings,
}

impl Ctx {
    fn root(&self) -> PathBuf {
        self.host.root()
    }

    /// 엔진 명령을 부르고 JSON을 돌려준다. 종료 코드가 0이 아니면 표준오류를 오류로 삼는다.
    pub fn call(&self, argv: &[&str]) -> Result<Value, String> {
        let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
        let reply = self.host.engine(&self.vault, &argv)?;
        if reply.status != 0 {
            let message = if reply.stderr.trim().is_empty() { reply.stdout.trim() } else { reply.stderr.trim() };
            return Err(protocol::clip(message, 400));
        }
        let out = reply.stdout.trim();
        if out.is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(out).map_err(|e| format!("엔진 응답을 읽지 못했다: {e}"))
    }

    /// 읽기 전용 작업 폴더를 지운다. 저장소 사본을 찾으면 git이 아는 방식으로, 못 찾으면 폴더만 지운다.
    pub fn discard_workspace(&self, repo: &str, workdir: &str) {
        match self.host.clone_path(repo) {
            Some(clone) => host::remove_worktree(&clone, std::path::Path::new(workdir)),
            None => {
                let _ = std::fs::remove_dir_all(workdir);
            }
        }
    }

    pub fn get_task(&self, key: &str) -> Result<TaskInfo, String> {
        TaskInfo::from_json(&self.call(&["get-task", key])?)
    }

    pub fn transition(&self, key: &str, to: &str, actor: &str, from: &str) -> Result<(), String> {
        self.call(&["transition-task", key, &format!("to={to}"), &format!("actor={actor}"), &format!("from={from}")]).map(|_| ())
    }

    pub fn comment(&self, key: &str, actor: &str, body: &str) -> Result<(), String> {
        self.call(&["add-task-comment", key, &format!("body={}", protocol::clip(body, protocol::COMMENT_CHARS)), &format!("actor={actor}")]).map(|_| ())
    }
}

/// 다음에 claude에 줄 것.
enum Input {
    /// 새 세션. 태스크 전체를 프롬프트로 준다.
    First,
    /// 답을 반영해 같은 세션을 이어 간다.
    Answer { question: String, answer: String },
    /// 끊긴 세션을 이어 간다.
    Recover,
    /// 질문 한도에 닿아서 보고를 쓰게 한다.
    ForceReport { question: String },
}

struct Run<'a> {
    ctx: &'a Ctx,
    rec: RunRecord,
    task: TaskInfo,
    cancel: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
}

/// 실행 하나를 끝까지 돌린다. 일꾼 스레드에서 부른다.
pub fn execute(ctx: &Ctx, key: &str, mode: Mode, cancel: Arc<AtomicBool>, shutdown: Arc<AtomicBool>) -> Ending {
    let (rec, task, input) = match begin(ctx, key, &mode) {
        Ok(started) => started,
        Err(ending) => return ending,
    };
    let mut run = Run { ctx, rec, task, cancel, shutdown };
    let ending = run.work(input);
    ctx.host.changed(Some(key));
    ending
}

/// 받기와 집기. 실행 기록과 처음 입력을 만든다.
fn begin(ctx: &Ctx, key: &str, mode: &Mode) -> Result<(RunRecord, TaskInfo, Input), Ending> {
    let task = ctx.get_task(key).map_err(|e| Ending::NotPicked(format!("태스크를 읽지 못했다: {e}")))?;
    match mode {
        Mode::Fresh => {
            if task.status != "ready-for-develop" {
                return Err(Ending::NotPicked(format!("Ready For Develop가 아니다({})", task.status)));
            }
            ctx.transition(key, "in-progress", "runner", "ready-for-develop").map_err(Ending::NotPicked)?;
            let rec = RunRecord {
                run_id: store::new_uuid(),
                task_key: key.to_string(),
                repo: task.repo.clone(),
                session_id: store::new_uuid(),
                started_at: store::now_ms(),
                step: "집기".to_string(),
                ..Default::default()
            };
            let run = Run { ctx, rec: rec.clone(), task: task.clone(), cancel: Arc::default(), shutdown: Arc::default() };
            run.save();
            run.event("받기", &format!("{} \"{}\"을 읽었다", key, task.title));
            run.event("집기", "Ready For Develop → In Progress");
            Ok((rec, task, Input::First))
        }
        Mode::Answer { run_id } | Mode::Recover { run_id } => {
            let mut rec = store::load_run(&ctx.root(), &ctx.vault, key, run_id).ok_or_else(|| Ending::NotPicked("실행 기록을 찾지 못했다".into()))?;
            if rec.phase != Phase::Running {
                return Err(Ending::NotPicked(format!("이어 갈 수 없는 단계다({:?})", rec.phase)));
            }
            if task.status != "in-progress" {
                // 사람이 그 사이 태스크를 옮겼다
                if let Some(dir) = rec.workdir.take() {
                    ctx.discard_workspace(&rec.repo, &dir);
                }
                rec.phase = Phase::Abandoned;
                rec.ended_at = Some(store::now_ms());
                rec.error = Some(format!("태스크가 {} 로 옮겨져 이어 가지 않았다", task.status));
                let _ = store::save_run(&ctx.root(), &ctx.vault, &rec);
                return Err(Ending::Abandoned);
            }
            let input = match rec.answer.take() {
                Some(answer) => Input::Answer { question: rec.question.take().unwrap_or_default(), answer },
                None if matches!(mode, Mode::Answer { .. }) => return Err(Ending::NotPicked("도착한 답이 없다".into())),
                None => Input::Recover,
            };
            Ok((rec, task, input))
        }
    }
}

impl Run<'_> {
    fn root(&self) -> PathBuf {
        self.ctx.root()
    }

    fn save(&self) {
        let _ = store::save_run(&self.root(), &self.ctx.vault, &self.rec);
    }

    fn event(&self, step: &str, text: &str) {
        store::append_event(&self.root(), &self.ctx.vault, &self.rec, step, text);
    }

    /// 걸음을 바꾸고 기록하고 화면에 알린다.
    fn step(&mut self, step: &str, text: &str) {
        self.rec.step = step.to_string();
        self.save();
        self.event(step, text);
        self.ctx.host.changed(Some(&self.rec.task_key));
    }

    fn work_dir(&self) -> PathBuf {
        store::run_dir(&self.root(), &self.ctx.vault, &self.rec.task_key, &self.rec.run_id).join("work")
    }

    /// 읽기 전용 작업 폴더를 준비한다. 이미 있으면 그대로 쓰고, 없으면(앱을 껐다 켠 뒤 사라졌거나 처음이면) 만든다.
    fn prepare_workspace(&mut self) -> Result<PathBuf, String> {
        let work = self.work_dir();
        if work.join(".git").exists() {
            return Ok(work);
        }
        self.step("받아오기", &format!("{} 저장소 사본을 최신으로 맞추고 읽기 전용 작업 폴더를 만든다", self.task.repo));
        let clone = self.ctx.host.clone_dir(&self.task.repo)?;
        host::create_worktree(&clone, &work, self.task.base.as_deref())?;
        self.rec.workdir = Some(work.to_string_lossy().into_owned());
        self.save();
        Ok(work)
    }

    /// 정리: 작업 폴더를 지운다.
    fn cleanup(&mut self) {
        if let Some(dir) = self.rec.workdir.take() {
            self.event("정리", "읽기 전용 작업 폴더를 지웠다");
            self.ctx.discard_workspace(&self.task.repo, &dir);
        }
        self.save();
    }

    fn mcp_config(&self) -> Result<String, String> {
        let reply = self.ctx.host.engine(&self.ctx.vault, &["claude-mcp".to_string(), "config".to_string()])?;
        if reply.status != 0 {
            return Err(format!("MCP 설정을 만들지 못했다: {}", protocol::clip(&reply.stderr, 300)));
        }
        protocol::mcp_config_for(&reply.stdout, &self.ctx.vault, self.ctx.host.is_guest_build())
    }

    fn work(&mut self, mut input: Input) -> Ending {
        // 판단: 읽기 전용 실행에 필요한 것이 갖춰졌는지
        self.step("판단", "읽기 전용으로 조사한다");
        if self.task.repo.is_empty() {
            return self.fail("저장소가 비어 있어 작업 폴더를 만들 수 없다");
        }
        let mcp = match self.mcp_config() {
            Ok(m) => m,
            Err(e) => return self.fail(&e),
        };
        let work = match self.prepare_workspace() {
            Ok(w) => w,
            Err(e) => return self.fail(&format!("저장소를 준비하지 못했다: {e}")),
        };
        // 작업 폴더의 실제 경로로 읽기 허용 규칙을 만든다(심볼릭 링크를 풀어야 규칙과 맞는다)
        let work = work.canonicalize().unwrap_or(work);
        let mut forced_once = false;
        let mut lost_session_retry = false;
        loop {
            let worked = Duration::from_secs(self.rec.worked_secs);
            let budget = Duration::from_secs(MAX_WORK_MINUTES * 60);
            if worked >= budget {
                return self.fail(&format!("일하는 시간 제한({MAX_WORK_MINUTES}분)을 넘겼다"));
            }
            let (prompt, resume) = match &input {
                Input::First => (protocol::build_prompt(&self.task), false),
                Input::Answer { question, answer } => (protocol::answer_prompt(question, answer, false), true),
                Input::Recover => (protocol::recover_prompt(), true),
                Input::ForceReport { question } => (protocol::answer_prompt(question, "(질문 한도에 닿았다. 더 답할 수 없다)", true), true),
            };
            let args = protocol::claude_args(&self.rec.session_id, resume, &self.ctx.settings.model, &mcp, &work);
            self.step("일하기", if resume { "세션을 이어서 조사한다" } else { "claude에게 조사를 맡겼다" });
            let started = Instant::now();
            let call = ClaudeCall {
                workdir: work.clone(),
                vault: self.ctx.vault.clone(),
                args,
                prompt,
                timeout: budget - worked,
                cancel: self.cancel.clone(),
            };
            let outcome = self.ctx.host.run_claude(call);
            self.rec.turns += 1;
            self.rec.worked_secs += started.elapsed().as_secs();
            let outcome = match outcome {
                Ok(o) => o,
                Err(e) => return self.fail(&e),
            };
            if outcome.cancelled {
                return self.stop();
            }
            if outcome.timed_out {
                return self.fail(&format!("일하는 시간 제한({MAX_WORK_MINUTES}분)을 넘겨 멈췄다"));
            }
            if outcome.exit_code != Some(0) {
                let detail = format!("{}\n{}", outcome.stderr, outcome.stdout);
                if resume && !lost_session_retry && detail.contains("No conversation found") {
                    // 이 컴퓨터에서 세션 기록이 사라졌다. 태스크와 댓글(질문과 답이 들어 있다)로 새 세션을 연다
                    lost_session_retry = true;
                    self.event("일하기", "세션 기록을 찾지 못해 새 세션으로 이어 간다");
                    self.rec.session_id = store::new_uuid();
                    if let Ok(fresh) = self.ctx.get_task(&self.rec.task_key) {
                        self.task = fresh;
                    }
                    input = Input::First;
                    continue;
                }
                return self.fail(&format!(
                    "claude가 종료 코드 {}로 끝났다: {}",
                    outcome.exit_code.map(|c| c.to_string()).unwrap_or_else(|| "없음".into()),
                    protocol::clip(if outcome.stderr.trim().is_empty() { &outcome.stdout } else { &outcome.stderr }, 400)
                ));
            }
            let result = match protocol::parse_claude_output(&outcome.stdout) {
                Ok(r) => r,
                Err(e) => return self.fail(&e),
            };
            self.rec.cost_usd += result.cost_usd;
            self.rec.input_tokens += result.input_tokens;
            self.rec.output_tokens += result.output_tokens;
            self.save();
            if result.is_error {
                return self.fail(&format!("claude가 오류를 알렸다: {}", protocol::clip(&result.text, 400)));
            }
            match protocol::parse_reply(&result.text) {
                Reply::Report(text) => return self.report(&text),
                Reply::Question(q) if q.is_empty() => return self.report(&result.text),
                Reply::Question(q) => {
                    if self.rec.questions >= MAX_QUESTIONS {
                        if forced_once {
                            // 한도를 알려 줬는데도 또 물었다. 마지막 질문을 보고에 붙여서 마무리한다
                            return self.report(&format!("(질문 한도 {MAX_QUESTIONS}개에 닿아 더 묻지 못했다. 마지막 질문: {q})"));
                        }
                        forced_once = true;
                        input = Input::ForceReport { question: q };
                        continue;
                    }
                    return self.ask(&q);
                }
            }
        }
    }

    /// 내보내기(보고): 보고를 댓글로 남기고 In Review로 보낸다.
    fn report(&mut self, text: &str) -> Ending {
        self.step("내보내기", "보고를 댓글로 남기고 In Review로 보낸다");
        let body = format!("**분석 보고**\n\n{}", text.trim());
        if let Err(e) = self.ctx.comment(&self.rec.task_key, "runner", &body) {
            return self.fail(&format!("보고를 댓글로 남기지 못했다: {e}"));
        }
        if let Err(e) = self.ctx.transition(&self.rec.task_key, "in-review", "runner", "in-progress") {
            // 보고는 남았는데 태스크가 그 사이 옮겨졌다. 실패로 덮지 않고 사람이 옮긴 것으로 본다
            self.rec.phase = Phase::Abandoned;
            self.rec.error = Some(format!("보고는 남겼지만 In Review로 옮기지 못했다: {e}"));
            self.rec.summary = Some(protocol::clip(text, 300));
            self.rec.ended_at = Some(store::now_ms());
            self.cleanup();
            self.save();
            return Ending::Abandoned;
        }
        self.rec.phase = Phase::Done;
        self.rec.step = "정리".to_string();
        self.rec.summary = Some(protocol::clip(text, 300));
        self.rec.ended_at = Some(store::now_ms());
        self.cleanup();
        self.save();
        self.ctx.host.notify(
            &self.ctx.vault,
            NewItem {
                kind: "pr".into(),
                title: "리뷰할 일이 생겼어요".into(),
                body: format!("{} — 분석 보고가 올라왔어요", self.task.title),
                task_key: Some(self.rec.task_key.clone()),
                id: Some(format!("run-{}-done", self.rec.run_id)),
            },
        );
        Ending::Reported
    }

    /// 내보내기(질문): 질문을 댓글로 남기고 Waiting Answer로 보낸 채 답을 기다린다. 작업 폴더와 세션은 그대로 둔다.
    fn ask(&mut self, question: &str) -> Ending {
        self.step("내보내기", "질문을 댓글로 남기고 사람의 답을 기다린다");
        let body = format!("**질문**\n\n{}", question.trim());
        if let Err(e) = self.ctx.comment(&self.rec.task_key, "runner", &body) {
            return self.fail(&format!("질문을 댓글로 남기지 못했다: {e}"));
        }
        if let Err(e) = self.ctx.transition(&self.rec.task_key, "waiting-answer", "runner", "in-progress") {
            self.rec.phase = Phase::Abandoned;
            self.rec.error = Some(format!("Waiting Answer로 옮기지 못했다: {e}"));
            self.rec.ended_at = Some(store::now_ms());
            self.cleanup();
            self.save();
            return Ending::Abandoned;
        }
        self.rec.questions += 1;
        self.rec.phase = Phase::WaitingAnswer;
        self.rec.question = Some(question.trim().to_string());
        self.save();
        self.ctx.host.notify(
            &self.ctx.vault,
            NewItem {
                kind: "question".into(),
                title: "답이 필요해요".into(),
                body: format!("{} — {}", self.task.title, protocol::clip(question, 160)),
                task_key: Some(self.rec.task_key.clone()),
                id: Some(format!("run-{}-q{}", self.rec.run_id, self.rec.questions)),
            },
        );
        Ending::Asked
    }

    /// 실패로 끝낸다. 까닭을 댓글로 남기고 Failed로 보낸다.
    fn fail(&mut self, message: &str) -> Ending {
        self.event(&self.rec.step.clone(), &format!("실패: {message}"));
        let _ = self.ctx.comment(&self.rec.task_key, "runner", &format!("**실행이 실패했어요**\n\n{message}\n\n다시 하려면 Draft로 되돌린 뒤 Ready For Develop로 보내세요."));
        let _ = self.ctx.transition(&self.rec.task_key, "failed", "runner", "in-progress");
        self.rec.phase = Phase::Failed;
        self.rec.error = Some(message.to_string());
        self.rec.ended_at = Some(store::now_ms());
        self.cleanup();
        self.save();
        self.ctx.host.notify(
            &self.ctx.vault,
            NewItem {
                kind: "failed".into(),
                title: "실행이 실패했어요".into(),
                body: format!("{} — {}", self.task.title, protocol::clip(message, 160)),
                task_key: Some(self.rec.task_key.clone()),
                id: Some(format!("run-{}-failed", self.rec.run_id)),
            },
        );
        Ending::Failed
    }

    /// claude가 멈춰서 돌아왔다. 앱을 닫는 중이면 기록을 남겨 두고, 사람이 중지한 것이면 Cancelled로 끝낸다.
    fn stop(&mut self) -> Ending {
        if self.shutdown.load(Ordering::SeqCst) {
            self.event("일하기", "앱을 닫아 멈췄다. 다시 켜면 이어 간다");
            self.save();
            return Ending::Interrupted;
        }
        self.event("일하기", "사람이 중지했다");
        let _ = self.ctx.comment(&self.rec.task_key, "human", "**실행을 중지했어요**");
        let _ = self.ctx.transition(&self.rec.task_key, "cancelled", "human", "in-progress");
        self.rec.phase = Phase::Cancelled;
        self.rec.ended_at = Some(store::now_ms());
        self.cleanup();
        self.save();
        Ending::Stopped
    }
}
