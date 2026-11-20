//! 수동 실행은 승인된 글로벌 정의를 읽어 역할별로 호출한다. 태스크가 질문으로 멈추면 같은 역할과 작업 사본을 이어 쓴다.
use super::{Ending, Run};
use crate::{
    github::TaskPullRequest,
    runner::{host, protocol, providers, store, verification},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Component, Path},
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

const LEAD: &str = "orchestrator";
const REVIEW: &str = "pr-review-merge";
const LIMIT: u32 = 120;

#[derive(Default, Serialize, Deserialize)]
struct Progress {
    role: String,
    assignment: String,
    observations: Vec<String>,
    decisions: Vec<String>,
    #[serde(default)]
    active_engine: HashMap<String, String>,
    check_passed: Option<bool>,
    qa_tree: Option<String>,
    reviewed_tree: Option<String>,
    publication: Option<TaskPullRequest>,
}
impl Progress {
    fn observe(&mut self, s: &str) {
        self.observations.push(protocol::clip(s, 18000));
        if self.observations.len() > 9 {
            self.observations.remove(0);
        }
    }
    fn save(&self, path: &Path) -> Result<(), String> {
        store::write_atomic(path, &serde_json::to_vec(self).map_err(|e| e.to_string())?)
    }
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
enum Action {
    List {
        #[serde(default)]
        prefix: String,
    },
    Read {
        path: String,
        #[serde(default)]
        line: usize,
    },
    Search {
        text: String,
    },
    Diff {
        #[serde(default)]
        path: String,
    },
    Note {
        id: String,
    },
    Delegate {
        role: String,
        task: String,
    },
    Patch {
        patch: String,
    },
    Check {
        command: String,
    },
    Result {
        ok: bool,
        evidence: String,
        #[serde(default)]
        inspection_only: bool,
    },
    Question {
        text: String,
    },
    Publish {
        title: String,
        body: String,
    },
    Finish {
        summary: String,
    },
}

const CONTRACT: &str = r#"호스트가 도구와 역할별 권한을 집행한다. 매번 JSON 객체 하나만 답한다. 셸이나 CLI 자체 도구를 직접 호출하지 않는다.
자료 읽기: {"action":"list","prefix":""}, {"action":"read","path":"README.md","line":1}, {"action":"search","text":"검색어"}, {"action":"diff","path":""}, 연결된 노트는 {"action":"note","id":"노트 id"}.
지휘자는 저장소 규칙을 읽고 {"action":"delegate","role":"dev","task":"목적·범위·완료 기준"}로 실제 개발을 맡긴다. 자식은 자기 지시와 태스크, 동일 소스만 본다.
dev는 {"action":"patch","patch":"diff --git ..."}로 표준 Git 패치(새 파일은 /dev/null)를 낸다. 호스트가 인덱스와 사본에 적용한다.
개발이나 QA는 {"action":"check","command":"저장소가 지정한 검사 명령"}으로 현재 변경을 담은 임시 사본에서 실제 검사한다. 각 명령은 새 사본에서 시작하므로 연속 명령은 한 번에 묶는다.
자식은 {"action":"result","ok":true,"evidence":"근거와 결과"}로 돌려준다. QA가 실행할 검사가 없어 소스를 읽어 확인했으면 inspection_only:true와 파일·줄·이유를 적는다.
개발 뒤에 qa와 pr-review-merge에게 차례로 독립적으로 검토를 맡긴다. 막는 문제가 있으면 수정 뒤 다시 검사와 리뷰를 받는다. 그 밖의 등록된 역할도 delegate로 부른다.
사람만 풀 수 있으면 {"action":"question","text":"질문"}으로 멈춘다. PR 본문에는 노트 원문과 인증 정보를 넣지 않는다. 최종 승인된 변경은 {"action":"publish","title":"커밋과 PR 제목","body":"검토할 변경 · 확인 근거 · 남은 일"}로 게시한다. 커밋, 푸시, PR 열기는 호스트가 수행하며 머지는 사람이 한다. 변경할 필요가 없는 태스크만 {"action":"finish","summary":"근거"}로 끝낸다."#;

enum Next {
    Continue,
    Ask(String),
    Done(String),
}
fn repo_path(raw: &str) -> Result<&str, String> {
    if raw.is_empty()
        || raw.starts_with('-')
        || raw.contains(['\0', '\n', ':'])
        || !Path::new(raw)
            .components()
            .all(|c| matches!(c,Component::Normal(n) if n!=".git"))
    {
        Err("저장소 안의 상대 경로를 지정해야 한다".into())
    } else {
        Ok(raw)
    }
}

impl Run<'_> {
    pub(super) fn orchestrate(&mut self) -> Ending {
        match self.drive() {
            Ok(Next::Done(s)) => self.report(&s),
            Ok(Next::Ask(q)) => self.ask(&q),
            _ if self.cancel.load(Ordering::SeqCst) => self.stop(),
            Err(e) => self.fail(&format!("{} 단계: {e}", self.rec.step)),
            _ => self.fail("개발 실행이 결과 없이 끝났다"),
        }
    }

    fn drive(&mut self) -> Result<Next, String> {
        let work = self.work_dir();
        let record = work
            .parent()
            .ok_or("실행 폴더가 없다")?
            .join("orchestration.json");
        let mut state = match fs::read(&record) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| format!("개발 실행 기록을 읽지 못했다: {e}"))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Progress {
                role: LEAD.into(),
                ..Default::default()
            },
            Err(e) => return Err(e.to_string()),
        };
        if self.rec.workdir.is_some() && !work.join(".git").is_file() {
            return Err("진행하던 작업 폴더가 없어 중단했다".into());
        }
        self.step("판단", "글로벌 에이전트와 실행 설정을 읽는다");
        let agents = providers::agents(self.ctx)?;
        for role in [LEAD, "dev", "qa", REVIEW] {
            if !agents.get(role).is_some_and(|a| !a.tiers.is_empty()) {
                return Err(format!(
                    "활성 글로벌 에이전트 {role}의 모델을 설정해야 한다"
                ));
            }
        }
        if !agents["dev"].editable {
            return Err("개발 에이전트에 파일 수정 권한이 필요하다".into());
        }
        let work = self
            .prepare_workspace()
            .map_err(|e| format!("저장소를 준비하지 못했다: {e}"))?;
        let mut roles: Vec<_> = agents.keys().cloned().collect();
        roles.sort();
        let description = protocol::task_description(&self.task);
        let deadline = Instant::now()
            + Duration::from_secs(
                (store::MAX_WORK_MINUTES * 60).saturating_sub(self.rec.worked_secs),
            );
        loop {
            if self.cancel.load(Ordering::SeqCst) {
                return Err("실행 중지".into());
            }
            if let Some(pr) = &state.publication {
                self.step(
                    "내보내기",
                    "검사와 리뷰를 마친 커밋을 푸시하고 PR을 확인한다",
                );
                if self.ctx.get_task(&self.task.key)?.status != "in-progress" {
                    return Err("태스크의 상태가 바뀌었다".into());
                }
                if self.cancel.load(Ordering::SeqCst) {
                    return Err("게시 전에 실행을 중지했다".into());
                }
                let url = match &self.rec.pull_request {
                    Some(url) => url.clone(),
                    None => self.ctx.host.publish(&work, pr)?,
                };
                self.rec.pull_request = Some(url.clone());
                self.save();
                return Ok(Next::Done(format!("[PR 열기]({url})\n\n{}", pr.body)));
            }
            if Instant::now() >= deadline || self.rec.turns >= LIMIT {
                return Err("개발 실행의 시간 또는 호출 한도에 닿았다".into());
            }
            let agent = agents
                .get(&state.role)
                .ok_or_else(|| format!("활성 글로벌 에이전트 {} 정의가 없다", state.role))?;
            let history = if state.role == LEAD {
                state.decisions.join("\n")
            } else {
                String::new()
            };
            let prompt=format!("{}\n\n플랫폼 행동 규칙\n{CONTRACT}\n\n{description}\n등록된 역할: {}\n현재 역할: {}\n맡은 일: {}\n지휘 기록:\n{}\n이 역할의 최근 관찰:\n{}",
                agent.instructions,roles.join(", "),state.role,state.assignment,protocol::clip(&history,20000),state.observations.join("\n"));
            let timer = Instant::now();
            let action = self.agent_reply(
                &state.role,
                agent,
                &mut state.active_engine,
                &prompt,
                deadline,
            );
            let action = match action {
                Ok(action) => action,
                Err(error) => {
                    self.rec.worked_secs += timer.elapsed().as_secs();
                    return Err(error);
                }
            };
            let result = self.act(action, &mut state, &agents, &work, deadline);
            self.rec.worked_secs += timer.elapsed().as_secs();
            self.save();
            if self.cancel.load(Ordering::SeqCst) {
                return Err("실행 중지".into());
            }
            let next = match result {
                Ok(n) => n,
                Err(e) => {
                    state.observe(&format!("행동 거절: {e}"));
                    self.event("일하기", &protocol::clip(&e, 500));
                    Next::Continue
                }
            };
            state.save(&record)?;
            if !matches!(next, Next::Continue) {
                return Ok(next);
            }
        }
    }

    fn agent_reply(
        &mut self,
        role: &str,
        agent: &providers::Agent,
        active_engine: &mut HashMap<String, String>,
        prompt: &str,
        deadline: Instant,
    ) -> Result<Action, String> {
        let mut errors = vec![];
        let start = active_engine
            .get(role)
            .and_then(|engine| agent.tiers.iter().position(|tier| &tier.engine == engine))
            .unwrap_or(0);
        for tier in &agent.tiers[start..] {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() || self.cancel.load(Ordering::SeqCst) {
                return Err("호출이 중단됐다".into());
            }
            self.rec.turns += 1;
            self.step(
                "일하기",
                &format!(
                    "{role} · {} · {} · {}",
                    tier.engine, tier.model, tier.effort
                ),
            );
            let result = self
                .ctx
                .host
                .ask_agent(tier, prompt, self.cancel.clone(), remaining)
                .and_then(|reply| {
                    let json = reply
                        .trim()
                        .trim_start_matches("```json")
                        .trim_start_matches("```")
                        .trim_end_matches("```")
                        .trim();
                    serde_json::from_str(json)
                        .map_err(|error| format!("행동을 JSON으로 읽지 못했다: {error}"))
                });
            match result {
                Ok(action) => {
                    active_engine.insert(role.to_owned(), tier.engine.clone());
                    return Ok(action);
                }
                Err(error) => {
                    let reason = format!("{} {}: {error}", tier.engine, tier.model);
                    self.event("일하기", &reason);
                    errors.push(reason);
                }
            }
        }
        Err(format!(
            "{role}의 모델을 호출하지 못했다: {}",
            errors.join(" / ")
        ))
    }

    fn act(
        &mut self,
        action: Action,
        s: &mut Progress,
        agents: &HashMap<String, providers::Agent>,
        work: &Path,
        deadline: Instant,
    ) -> Result<Next, String> {
        let git = |args: &[&str]| host::git(work, args);
        let tree = || git(&["write-tree"]);
        let text = match action {
            Action::List { prefix } => {
                if prefix.is_empty() {
                    git(&["ls-files"])?
                } else {
                    git(&["--literal-pathspecs", "ls-files", "--", repo_path(&prefix)?])?
                }
            }
            Action::Read { path, line } => {
                let path = repo_path(&path)?;
                let blob = format!(":{path}");
                let length = git(&["cat-file", "-s", &blob])?
                    .parse::<u64>()
                    .map_err(|e| e.to_string())?;
                if length > 1_000_000 {
                    return Err("파일이 1MB를 넘는다. 파일 안에서 검색한다".into());
                }
                let body = git(&["cat-file", "blob", &blob])?;
                let lines = body.lines().count();
                let excerpt = body
                    .lines()
                    .enumerate()
                    .skip(line.saturating_sub(1))
                    .take(300)
                    .map(|(i, l)| format!("{} {l}", i + 1))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("파일 {path} ({lines}줄)\n{excerpt}")
            }
            Action::Search { text } => {
                git(&["grep", "--cached", "-n", "-I", "-F", "-e", &text, "--"])?
            }
            Action::Diff { path } => {
                let mut args = vec![
                    "--literal-pathspecs",
                    "diff",
                    "--cached",
                    "--no-ext-diff",
                    "--no-textconv",
                    "HEAD",
                    "--",
                ];
                if !path.is_empty() {
                    args.push(repo_path(&path)?);
                }
                git(&args)?
            }
            Action::Note { id }
                if self
                    .task
                    .refs
                    .iter()
                    .any(|r| r.kind == "note" && r.target == id) =>
            {
                let doc = self.ctx.call(&["get-doc", &id])?;
                doc["body"].as_str().ok_or("노트 본문이 없다")?.to_string()
            }
            Action::Delegate { role, task }
                if s.role == LEAD && role != LEAD && agents.contains_key(&role) =>
            {
                s.decisions
                    .push(format!("{role}에 맡김: {}", protocol::clip(&task, 2000)));
                s.role = role;
                s.assignment = task;
                s.observations.clear();
                s.check_passed = None;
                self.event("일하기", &format!("{}에게 맡겼다", s.role));
                return Ok(Next::Continue);
            }
            Action::Patch { patch } if s.role == "dev" && agents[&s.role].editable => {
                verification::apply(work, &patch, self.cancel.clone())?;
                s.qa_tree = None;
                s.reviewed_tree = None;
                "패치를 적용했다".into()
            }
            Action::Check { command } if matches!(s.role.as_str(), "qa" | "dev") => {
                let (pass, output) = verification::check(
                    work,
                    &command,
                    deadline
                        .saturating_duration_since(Instant::now())
                        .min(Duration::from_secs(900)),
                    self.cancel.clone(),
                )?;
                s.check_passed = Some(pass);
                let msg = format!(
                    "검사 {} · {}\n{}",
                    if pass { "통과" } else { "실패" },
                    command,
                    output
                );
                self.event("일하기", &protocol::clip(&msg, 1000));
                msg
            }
            Action::Result {
                ok,
                evidence,
                inspection_only,
            } if s.role != LEAD && !evidence.trim().is_empty() => {
                if s.role == "qa" {
                    if ok
                        && !(s.check_passed == Some(true)
                            || (s.check_passed.is_none() && inspection_only))
                    {
                        return Err(
                            "검사 명령의 통과 결과 또는 코드 검토만 한 이유가 필요하다".into()
                        );
                    }
                    s.qa_tree = if ok { Some(tree()?) } else { None };
                    s.reviewed_tree = None;
                }
                if s.role == REVIEW {
                    let current = tree()?;
                    s.reviewed_tree = if ok && s.qa_tree.as_ref() == Some(&current) {
                        Some(current)
                    } else {
                        None
                    };
                }
                let outcome = format!(
                    "{} {}: {evidence}",
                    s.role,
                    if ok { "통과" } else { "보완 필요" }
                );
                self.event("일하기", &protocol::clip(&outcome, 800));
                s.decisions.push(protocol::clip(&outcome, 4000));
                s.role = LEAD.into();
                s.assignment.clear();
                s.observations.clear();
                outcome
            }
            Action::Question { text }
                if !text.trim().is_empty() && self.rec.questions < store::MAX_QUESTIONS =>
            {
                return Ok(Next::Ask(text))
            }
            Action::Finish { summary }
                if s.role == LEAD && tree()? == git(&["rev-parse", "HEAD^{tree}"])? =>
            {
                return Ok(Next::Done(summary))
            }
            Action::Publish { title, body }
                if s.role == LEAD && !title.trim().is_empty() && !body.trim().is_empty() =>
            {
                let current = tree()?;
                if s.qa_tree.as_ref() != Some(&current)
                    || s.reviewed_tree.as_ref() != Some(&current)
                {
                    return Err("현재 변경에 QA 통과와 리뷰 승인이 모두 필요하다".into());
                }
                if current == git(&["rev-parse", "HEAD^{tree}"])? {
                    return Err("PR에 넣을 변경이 없다".into());
                }
                git(&["diff", "--cached", "--check", "HEAD"])?;
                let base = match self.task.base.as_ref() {
                    Some(base) => base.clone(),
                    None => git(&["symbolic-ref", "--short", "refs/remotes/origin/HEAD"])?
                        .strip_prefix("origin/")
                        .ok_or("기준 브랜치가 없다")?
                        .into(),
                };
                let commit = git(&[
                    "-c",
                    "user.name=aisurada",
                    "-c",
                    "user.email=aisurada@users.noreply.github.com",
                    "-c",
                    "commit.gpgsign=false",
                    "commit-tree",
                    &current,
                    "-p",
                    "HEAD",
                    "-m",
                    &title,
                    "-m",
                    &body,
                ])?;
                let title = protocol::clip(title.lines().next().unwrap_or_default(), 160);
                let body = format!(
                    "{}\n\n태스크 {}\n\n{}",
                    protocol::clip(&body, 8000),
                    self.task.key,
                    s.decisions
                        .iter()
                        .rev()
                        .filter(|entry| entry.starts_with("qa 통과:")
                            || entry.starts_with(&format!("{REVIEW} 통과:")))
                        .take(2)
                        .map(|entry| protocol::clip(entry, 1000))
                        .collect::<Vec<_>>()
                        .join("\n\n")
                );
                s.publication = Some(TaskPullRequest {
                    repo: self.task.repo.clone(),
                    base,
                    commit,
                    branch: format!("aisurada/{}-{}", self.task.key, self.rec.run_id),
                    title,
                    body,
                });
                return Ok(Next::Continue);
            }
            _ => return Err("이 역할이나 현재 단계에서 사용할 수 없는 행동이다".into()),
        };
        s.observe(&text);
        Ok(Next::Continue)
    }
}
