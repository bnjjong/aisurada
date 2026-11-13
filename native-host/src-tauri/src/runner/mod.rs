//! 태스크 시스템 3단계: 읽기 전용 러너.
//!
//! Ready For Develop인 태스크를 집어 claude에게 저장소를 읽게 하고(`job`), 보고나 질문을 태스크에 남긴다.
//! 이 모듈은 「누구를 언제 돌릴지」를 맡는다: 주기적으로 집을 태스크를 찾고, 동시에 도는 수를 지키고,
//! 앱을 다시 켰을 때 끊긴 실행을 이어 가고, 앱을 닫을 때 일하던 실행을 멈춘다. 코드를 고치고 PR을 올리는 일은 4단계다.
//!
//! ```text
//!   tick(15초) ─ 열린 vault? ─ 처음이면 reconcile(끊긴 실행 이어 가기)
//!       │
//!       └ 자동 집기가 켜져 있으면 list-tasks status=ready-for-develop → 자리가 있는 만큼 job::execute (일꾼 스레드)
//!   화면 명령: runner_run(지금 실행) · runner_stop · runner_answer · runner_settings_set · runner_status · runner_runs
//! ```
pub mod host;
pub mod job;
pub mod protocol;
pub mod store;
#[cfg(test)]
mod tests;

use host::{AppHost, Host};
use job::{Ctx, Ending, Mode};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use store::{Phase, RunRecord, Settings};
use tauri::{Manager, Runtime, State};

/// 집을 태스크를 찾는 주기. `ais-cli`는 호출마다 JVM을 띄우므로 너무 짧게 잡지 않는다. 급하면 「지금 실행」이 있다.
pub const POLL_INTERVAL: Duration = Duration::from_secs(15);
/// 집지 못한 태스크(막혀 있거나 그 사이 바뀐 것)를 다시 시도하기까지 기다리는 시간.
const BACKOFF: Duration = Duration::from_secs(60);
/// 앱을 닫을 때 일하던 실행이 멈추기를 기다리는 시간(자식 프로세스 종료 유예 3초 + 여유).
pub const SHUTDOWN_WAIT: Duration = Duration::from_secs(8);

struct Active {
    cancel: Arc<AtomicBool>,
}

pub struct Runner {
    host: Arc<dyn Host>,
    active: Mutex<HashMap<String, Active>>,
    backoff: Mutex<HashMap<String, Instant>>,
    reconciled: Mutex<Option<String>>,
    shutdown: Arc<AtomicBool>,
    wake: (Mutex<bool>, Condvar),
}

/// 화면이 보는 러너의 지금 모습.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusOut {
    pub settings: Settings,
    pub claude_available: bool,
    /// 지금 일하는 스레드의 수. 앱을 닫을 때 묻는 숫자다.
    pub running: usize,
    /// 일하는 중인 실행(끊겨서 이어 가기를 기다리는 것 포함)
    pub active: Vec<RunRecord>,
    /// 사람의 답을 기다리는 실행
    pub waiting: Vec<RunRecord>,
}

#[derive(Debug, Serialize)]
pub struct RunsOut {
    pub runs: Vec<RunRecord>,
    /// 가장 최근 실행의 이벤트(마지막 80줄)
    pub events: Vec<store::Event>,
}

impl Runner {
    pub fn new(host: Arc<dyn Host>) -> Arc<Runner> {
        Arc::new(Runner {
            host,
            active: Mutex::new(HashMap::new()),
            backoff: Mutex::new(HashMap::new()),
            reconciled: Mutex::new(None),
            shutdown: Arc::new(AtomicBool::new(false)),
            wake: (Mutex::new(false), Condvar::new()),
        })
    }

    /// 주기 스레드를 띄운다. 앱이 끝날 때까지 돈다.
    pub fn start(self: &Arc<Self>) {
        let me = self.clone();
        thread::spawn(move || {
            while !me.shutdown.load(Ordering::SeqCst) {
                me.tick();
                let (lock, cv) = &me.wake;
                let mut woken = lock.lock().unwrap();
                if !*woken {
                    woken = cv.wait_timeout(woken, POLL_INTERVAL).unwrap().0;
                }
                *woken = false;
            }
        });
    }

    fn ctx(&self, vault: String) -> Ctx {
        Ctx { host: self.host.clone(), vault, settings: store::load_settings(&self.host.root()) }
    }

    /// 한 번 훑는다: 처음이면 끊긴 실행을 이어 가고, 자동 집기가 켜져 있으면 집는다.
    pub fn tick(self: &Arc<Self>) {
        if self.shutdown.load(Ordering::SeqCst) {
            return;
        }
        let Some(vault) = self.host.vault() else { return };
        let ctx = self.ctx(vault.clone());
        if self.reconciled.lock().unwrap().as_deref() != Some(vault.as_str()) {
            *self.reconciled.lock().unwrap() = Some(vault.clone());
            self.reconcile(&ctx);
        }
        if ctx.settings.auto_pickup && self.host.claude_available() {
            self.pickup(&ctx);
        }
    }

    /// 자리가 있는 만큼 Ready For Develop 태스크를 집는다. 우선순위가 높은 것, 키가 작은(오래된) 것이 먼저다.
    fn pickup(self: &Arc<Self>, ctx: &Ctx) {
        let slots = (ctx.settings.concurrency as usize).saturating_sub(self.active.lock().unwrap().len());
        if slots == 0 {
            return;
        }
        let Ok(list) = ctx.call(&["list-tasks", "status=ready-for-develop"]) else { return };
        let mut rows: Vec<(u8, u64, String)> = list
            .get("tasks")
            .and_then(|t| t.as_array())
            .into_iter()
            .flatten()
            .filter_map(|t| {
                let key = t.get("key")?.as_str()?.to_string();
                let rank = match t.get("priority").and_then(|p| p.as_str()) {
                    Some("high") => 0,
                    Some("low") => 2,
                    _ => 1,
                };
                let number = key.strip_prefix("TSK-")?.parse().ok()?;
                Some((rank, number, key))
            })
            .collect();
        rows.sort();
        let now = Instant::now();
        let mut started = 0;
        for (_, _, key) in rows {
            if started >= slots {
                break;
            }
            if self.backoff.lock().unwrap().get(&key).is_some_and(|until| *until > now) {
                continue;
            }
            if self.spawn_job(&ctx.vault, &key, Mode::Fresh) {
                started += 1;
            }
        }
    }

    /// 앱을 껐다 켜서 끊긴 실행을 정리한다(결정 7-나). 일하던 것은 `--resume`으로 이어 가고,
    /// 태스크가 그 사이 옮겨졌거나 답이 이미 도착해 있으면 그에 맞게 처리한다.
    fn reconcile(self: &Arc<Self>, ctx: &Ctx) {
        for rec in store::list_open_runs(&self.host.root(), &ctx.vault) {
            match rec.phase {
                // 일하다 끊긴 실행(또는 답이 이미 기록된 실행)은 같은 세션으로 이어 간다. 태스크가 옮겨졌으면 job이 닫는다.
                Phase::Running => {
                    self.spawn_job(&ctx.vault, &rec.task_key.clone(), Mode::Recover { run_id: rec.run_id.clone() });
                }
                // 사람의 답을 기다리는 중이면 그대로 둔다. 태스크가 그 사이 옮겨졌으면 닫는다.
                Phase::WaitingAnswer => match ctx.get_task(&rec.task_key) {
                    Ok(t) if t.status == "waiting-answer" => {}
                    Ok(_) => self.abandon_waiting(ctx, rec),
                    // 읽지 못한 것은 태스크가 없어졌는지 일시 오류인지 모른다. 지우지 않고 다음에 다시 본다.
                    Err(_) => {}
                },
                _ => {}
            }
        }
    }

    /// 답을 기다리던 실행의 태스크가 옮겨졌다(취소 등). 작업 폴더를 지우고 기록을 닫는다.
    fn abandon_waiting(&self, ctx: &Ctx, mut rec: RunRecord) {
        if let Some(dir) = rec.workdir.take() {
            ctx.discard_workspace(&rec.repo, &dir);
        }
        rec.phase = Phase::Abandoned;
        rec.ended_at = Some(store::now_ms());
        rec.error = Some("답을 기다리던 태스크가 다른 상태로 옮겨져 이어 가지 않았다".to_string());
        let _ = store::save_run(&self.host.root(), &ctx.vault, &rec);
        store::append_event(&self.host.root(), &ctx.vault, &rec, "정리", "태스크가 옮겨져 실행을 닫았다");
        self.host.changed(Some(&rec.task_key));
    }

    /// 일꾼 스레드를 띄워 실행을 시작한다. 이미 그 태스크가 돌고 있으면 false.
    fn spawn_job(self: &Arc<Self>, vault: &str, key: &str, mode: Mode) -> bool {
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut active = self.active.lock().unwrap();
            if active.contains_key(key) || self.shutdown.load(Ordering::SeqCst) {
                return false;
            }
            active.insert(key.to_string(), Active { cancel: cancel.clone() });
        }
        let me = self.clone();
        let (vault, key) = (vault.to_string(), key.to_string());
        thread::spawn(move || {
            let ctx = me.ctx(vault);
            let ending = job::execute(&ctx, &key, mode, cancel, me.shutdown.clone());
            if let Ending::NotPicked(_) = ending {
                me.backoff.lock().unwrap().insert(key.clone(), Instant::now() + BACKOFF);
            }
            me.active.lock().unwrap().remove(&key);
            me.host.changed(Some(&key));
        });
        true
    }

    pub fn status(&self) -> Result<StatusOut, String> {
        let vault = self.host.vault().ok_or("vault가 아직 열리지 않았어요")?;
        let open = store::list_open_runs(&self.host.root(), &vault);
        Ok(StatusOut {
            settings: store::load_settings(&self.host.root()),
            claude_available: self.host.claude_available(),
            running: self.active.lock().unwrap().len(),
            active: open.iter().filter(|r| r.phase == Phase::Running).cloned().collect(),
            waiting: open.into_iter().filter(|r| r.phase == Phase::WaitingAnswer).collect(),
        })
    }

    pub fn set_settings(&self, settings: Settings) -> Result<Settings, String> {
        let saved = store::save_settings(&self.host.root(), &settings)?;
        let (lock, cv) = &self.wake;
        *lock.lock().unwrap() = true;
        cv.notify_all();
        self.host.changed(None);
        Ok(saved)
    }

    pub fn runs(&self, key: &str) -> Result<RunsOut, String> {
        let vault = self.host.vault().ok_or("vault가 아직 열리지 않았어요")?;
        if !store::is_task_key(key) {
            return Err(format!("태스크 키가 이상해요: {key}"));
        }
        let runs = store::list_runs(&self.host.root(), &vault, key);
        let events = runs.first().map(|r| store::read_events(&self.host.root(), &vault, key, &r.run_id)).unwrap_or_default();
        Ok(RunsOut { runs, events })
    }

    /// 「지금 실행」: 자동 집기가 꺼져 있어도 이 태스크를 지금 집는다. 집을 수 없으면 까닭을 돌려준다.
    pub fn run_now(self: &Arc<Self>, key: &str) -> Result<(), String> {
        let vault = self.host.vault().ok_or("vault가 아직 열리지 않았어요")?;
        if !store::is_task_key(key) {
            return Err(format!("태스크 키가 이상해요: {key}"));
        }
        if !self.host.claude_available() {
            return Err("claude 실행 파일을 못 찾았어요. 설정에서 Claude 연결을 확인하거나 AISURADA_CLAUDE_BIN 으로 알려 주세요.".into());
        }
        let ctx = self.ctx(vault.clone());
        let limit = ctx.settings.concurrency as usize;
        if self.active.lock().unwrap().len() >= limit {
            return Err(format!("동시에 돌릴 수 있는 수({limit}개)에 닿았어요. 하나가 끝난 뒤에 눌러 주세요."));
        }
        // 막혀 있는지는 엔진이 가장 정확히 안다: 사람 몫의 전이 목록에서 In Progress의 막힌 까닭을 읽는다
        let detail = ctx.call(&["get-task", key])?;
        let task = protocol::TaskInfo::from_json(&detail)?;
        if task.status != "ready-for-develop" {
            return Err(format!("Ready For Develop 상태에서만 실행할 수 있어요(지금은 {})", task.status));
        }
        if let Some(blocked) = detail
            .get("transitions")
            .and_then(|t| t.as_array())
            .into_iter()
            .flatten()
            .find(|t| t.get("to").and_then(|v| v.as_str()) == Some("in-progress") && t.get("ok").and_then(|v| v.as_bool()) == Some(false))
        {
            return Err(blocked.get("reason").and_then(|r| r.as_str()).unwrap_or("다른 태스크에 막혀 있어요").to_string());
        }
        self.backoff.lock().unwrap().remove(key);
        if self.spawn_job(&vault, key, Mode::Fresh) {
            Ok(())
        } else {
            Err("이 태스크는 이미 실행 중이에요".into())
        }
    }

    /// 중지: 일하는 중이면 claude를 멈추고(Cancelled로 끝난다), 답을 기다리는 중이면 바로 Cancelled로 끝낸다.
    pub fn stop(&self, key: &str) -> Result<(), String> {
        let vault = self.host.vault().ok_or("vault가 아직 열리지 않았어요")?;
        if !store::is_task_key(key) {
            return Err(format!("태스크 키가 이상해요: {key}"));
        }
        if let Some(active) = self.active.lock().unwrap().get(key) {
            active.cancel.store(true, Ordering::SeqCst);
            return Ok(());
        }
        let root = self.host.root();
        let mut rec = store::list_runs(&root, &vault, key)
            .into_iter()
            .find(|r| r.phase == Phase::WaitingAnswer)
            .ok_or("멈출 실행이 없어요")?;
        let ctx = self.ctx(vault.clone());
        ctx.comment(key, "human", "**실행을 중지했어요**")?;
        ctx.transition(key, "cancelled", "human", "waiting-answer")?;
        if let Some(dir) = rec.workdir.take() {
            ctx.discard_workspace(&rec.repo, &dir);
        }
        rec.phase = Phase::Cancelled;
        rec.ended_at = Some(store::now_ms());
        store::save_run(&root, &vault, &rec)?;
        store::append_event(&root, &vault, &rec, "정리", "사람이 중지했다");
        self.host.changed(Some(key));
        Ok(())
    }

    /// 사람의 답을 태스크에 남기고, 기다리던 실행을 같은 세션으로 이어 간다.
    pub fn answer(self: &Arc<Self>, key: &str, text: &str) -> Result<(), String> {
        let vault = self.host.vault().ok_or("vault가 아직 열리지 않았어요")?;
        if !store::is_task_key(key) {
            return Err(format!("태스크 키가 이상해요: {key}"));
        }
        let text = text.trim();
        if text.is_empty() {
            return Err("답이 비어 있어요".into());
        }
        if text.chars().count() > 4000 {
            return Err("답이 너무 길어요(4000자까지)".into());
        }
        let root = self.host.root();
        let mut rec = store::list_runs(&root, &vault, key)
            .into_iter()
            .find(|r| r.phase == Phase::WaitingAnswer)
            .ok_or("답을 기다리는 실행이 없어요")?;
        let ctx = self.ctx(vault.clone());
        ctx.comment(key, "human", &format!("**답변**\n\n{text}"))?;
        ctx.transition(key, "in-progress", "human", "waiting-answer")?;
        rec.phase = Phase::Running;
        rec.answer = Some(text.to_string());
        rec.step = "일하기".to_string();
        store::save_run(&root, &vault, &rec)?;
        store::append_event(&root, &vault, &rec, "일하기", "사람의 답이 도착했다");
        self.host.changed(Some(key));
        // 사람이 직접 누른 일이라 동시에 도는 수의 한도를 따지지 않는다
        if self.spawn_job(&vault, key, Mode::Answer { run_id: rec.run_id.clone() }) {
            Ok(())
        } else {
            Err("이 태스크는 이미 실행 중이에요".into())
        }
    }

    /// 앱을 닫을 때: 일하던 실행을 멈추고(기록은 running으로 남아 다음에 이어 간다) 끝나기를 기다린다.
    pub fn shutdown(&self, wait: Duration) {
        self.shutdown.store(true, Ordering::SeqCst);
        for active in self.active.lock().unwrap().values() {
            active.cancel.store(true, Ordering::SeqCst);
        }
        let (lock, cv) = &self.wake;
        *lock.lock().unwrap() = true;
        cv.notify_all();
        let until = Instant::now() + wait;
        while Instant::now() < until && !self.active.lock().unwrap().is_empty() {
            thread::sleep(Duration::from_millis(50));
        }
    }
}

// ---------------------------------------------------------------------------
// 화면이 부르는 명령
// ---------------------------------------------------------------------------

/// 앱이 쓰는 러너 하나.
pub struct RunnerState(pub Arc<Runner>);

/// 앱이 뜰 때 한 번 부른다: 러너를 만들어 앱 상태에 얹고 주기 스레드를 띄운다.
pub fn install<R: Runtime>(app: &tauri::AppHandle<R>) {
    let runner = Runner::new(Arc::new(AppHost::new(app.clone())));
    runner.start();
    app.manage(RunnerState(runner));
}

/// 앱을 닫기 직전에 부른다. 러너가 없으면(시험) 아무 일도 하지 않는다.
pub fn shutdown_for_exit<R: Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(state) = app.try_state::<RunnerState>() {
        state.0.shutdown(SHUTDOWN_WAIT);
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| format!("러너 작업이 끊겼어요: {e}"))?
}

#[tauri::command]
pub fn runner_status(state: State<'_, RunnerState>) -> Result<StatusOut, String> {
    state.0.status()
}

#[tauri::command]
pub async fn runner_settings_set(state: State<'_, RunnerState>, settings: Settings) -> Result<Settings, String> {
    let runner = state.0.clone();
    blocking(move || runner.set_settings(settings)).await
}

#[tauri::command]
pub async fn runner_run(state: State<'_, RunnerState>, task_key: String) -> Result<(), String> {
    let runner = state.0.clone();
    blocking(move || runner.run_now(&task_key)).await
}

#[tauri::command]
pub async fn runner_stop(state: State<'_, RunnerState>, task_key: String) -> Result<(), String> {
    let runner = state.0.clone();
    blocking(move || runner.stop(&task_key)).await
}

#[tauri::command]
pub async fn runner_answer(state: State<'_, RunnerState>, task_key: String, text: String) -> Result<(), String> {
    let runner = state.0.clone();
    blocking(move || runner.answer(&task_key, &text)).await
}

#[tauri::command]
pub fn runner_runs(state: State<'_, RunnerState>, task_key: String) -> Result<RunsOut, String> {
    state.0.runs(&task_key)
}
