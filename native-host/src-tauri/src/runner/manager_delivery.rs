//! 매니저 태스크가 들어오거나 상태가 바뀌었을 때 알린다.

use super::{job::Ctx, protocol::REPORT_NOTE_LINK, store};
use crate::manager_webhook::{self, delivery_id, DeliveryError, EVENT};
use crate::notifications::NewItem;
use ais_cli::{env::Env, manager};
use ais_domain::task::{format::slack_thread, TaskStatus};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeSet, HashMap, HashSet, VecDeque},
    fs,
    io::ErrorKind,
};

const RETRY_WINDOW_MS: u64 = 45 * 60 * 1000;
const INITIAL_RETRY_MS: u64 = 15 * 1000;
const MAX_RETRY_MS: u64 = 5 * 60 * 1000;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    id: String,
    key: String,
    title: String,
    manager: String,
    repo: String,
    project: Option<String>,
    status: String,
    #[serde(default)]
    status_changes: Vec<StatusChange>,
}

#[derive(Deserialize)]
struct StatusChange {
    id: String,
    status: String,
    at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "platform", rename_all = "snake_case")]
enum Destination {
    Slack {
        channel: String,
        thread: String,
    },
    Telegram {
        chat: String,
        message: String,
        thread: Option<String>,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
struct Pending {
    id: String,
    task_key: String,
    task_id: String,
    manager: String,
    status: String,
    attempts: u8,
    #[serde(default)]
    registration: String,
    #[serde(default)]
    first_attempt_at: Option<u64>,
    #[serde(default)]
    next_attempt_at: u64,
    #[serde(default, alias = "in_flight")]
    sending: bool,
    #[serde(default)]
    change: Option<String>,
    #[serde(default)]
    changed_at: Option<String>,
    #[serde(default)]
    destination: Option<Destination>,
    #[serde(default)]
    webhook: Option<String>,
}

#[derive(Clone, Default, Deserialize, PartialEq, Serialize)]
struct History {
    statuses: HashMap<String, String>,
    pending: VecDeque<Pending>,
    #[serde(default)]
    observed: BTreeSet<String>,
    #[serde(default)]
    outcomes: VecDeque<DeliveryOutcome>,
}

#[derive(Clone, Deserialize, PartialEq, Serialize)]
struct DeliveryOutcome {
    id: String,
    change: Option<String>,
    destination: Option<Destination>,
    result: String,
}

pub(super) fn poll(ctx: &Ctx) -> Result<(), String> {
    let managers: Vec<_> = manager::list(&Env::from_process())?
        .into_iter()
        .filter(|item| item.in_vault(&ctx.vault) && item.scope().is_some())
        .collect();
    if managers.is_empty() {
        return Ok(());
    }
    let list = ctx.call(&["manager", "status-tasks"])?;
    let rows = list["tasks"]
        .as_array()
        .ok_or("태스크 목록을 읽지 못했다")?;
    let path = ctx
        .host
        .root()
        .join(format!("managers-{}.json", delivery_id(&ctx.vault)));
    let saved = match fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice::<History>(&bytes) {
            Ok(history) => Some(history),
            Err(error) => {
                return Err(format!("매니저 알림 상태를 읽지 못했어요. {error}"));
            }
        },
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => return Err(error.to_string()),
    };
    let initial = saved.is_none();
    let mut next = saved.clone().unwrap_or_default();
    let previous_statuses = std::mem::take(&mut next.statuses);
    let mut arrived = Vec::new();
    for row in rows
        .iter()
        .filter_map(|row| serde_json::from_value::<Row>(row.clone()).ok())
    {
        let configured = managers.iter().find(|item| {
            item.name == row.manager
                && item.enabled
                && item.repos.contains(&row.repo)
                && row.project.as_deref() == Some(item.project.as_str())
        });
        if let Some(configured) = configured {
            if !initial && !previous_statuses.contains_key(&row.id) {
                arrived.push(NewItem {
                    kind: "manager".into(),
                    title: format!("{}가 태스크를 접수했어요", configured.label),
                    body: row.title.clone(),
                    task_key: Some(row.key.clone()),
                    note_ref: None,
                    intake: None,
                    id: Some(format!("manager-{}", row.id)),
                });
            }
        }
        queue_changes(&mut next, &row, configured, || {
            ctx.call(&["get-task", &row.key, "activity=1"])
        });
        next.statuses.insert(row.id, row.status);
    }
    if saved.as_ref() != Some(&next) {
        save_history(&path, &next)?;
    }
    let refresh_tasks = !arrived.is_empty();
    for item in arrived {
        ctx.host.notify(&ctx.vault, item);
    }
    if refresh_tasks {
        ctx.host.changed(None);
    }

    let now = store::now_ms();
    let mut tasks = HashSet::new();
    let due = next
        .pending
        .iter()
        .position(|item| tasks.insert(&item.task_id) && item.next_attempt_at <= now);
    if let Some(index) = due {
        run_pending(ctx, &mut next, &path, index, now)?;
    }
    Ok(())
}

fn queue_changes(
    history: &mut History,
    row: &Row,
    manager: Option<&manager::Manager>,
    load_task: impl FnOnce() -> Result<Value, String>,
) {
    let unseen: Vec<_> = row
        .status_changes
        .iter()
        .map(|change| (format!("{}:{}", row.id, change.id), change))
        .filter(|(key, _)| !history.observed.contains(key))
        .collect();
    let subscribed = |change: &StatusChange| {
        manager.is_some_and(|manager| {
            manager.notifies(&change.status)
                && (manager.webhook.is_some() || manager.telegram_webhook.is_some())
        })
    };
    let task = if unseen.iter().any(|(_, change)| subscribed(change)) {
        match load_task() {
            Ok(task) if task["id"].as_str() == Some(&row.id) => Some(task),
            _ => return,
        }
    } else {
        None
    };
    let target = task
        .as_ref()
        .and_then(|task| original_destination(task, &manager?.project_id))
        .filter(|target| manager.is_some_and(|manager| target.webhook(manager).is_some()));
    for (key, change) in unseen {
        if let Some((target, manager)) = target.as_ref().zip(manager).filter(|_| subscribed(change))
        {
            history.pending.push_back(Pending {
                id: delivery_id(&key),
                task_key: row.key.clone(),
                task_id: row.id.clone(),
                manager: row.manager.clone(),
                status: change.status.clone(),
                attempts: 0,
                registration: manager.registration().into(),
                first_attempt_at: None,
                next_attempt_at: 0,
                sending: false,
                change: Some(change.id.clone()),
                changed_at: Some(change.at.clone()),
                destination: Some(target.clone()),
                webhook: target.webhook(manager).map(str::to_owned),
            });
        }
        history.observed.insert(key);
    }
}

fn save_history(path: &std::path::Path, history: &History) -> Result<(), String> {
    store::write_atomic(
        path,
        &serde_json::to_vec(history).map_err(|e| e.to_string())?,
    )
}

fn terminal_before_send(
    item: &Pending,
    now: u64,
    deadline: u64,
) -> Option<(DeliveryError, &'static str)> {
    if item.sending {
        // 앞선 실행에서 발송을 시작한 기록이 있으면 자동으로 다시 보내지 않는다.
        return Some((
            DeliveryError::Unconfirmed,
            delivery_error(&DeliveryError::Unconfirmed),
        ));
    }
    (now >= deadline).then_some((
        DeliveryError::Unavailable,
        "재시도 시간이 지나 알림을 보낼 수 없었어요.",
    ))
}

fn run_pending(
    ctx: &Ctx,
    history: &mut History,
    path: &std::path::Path,
    index: usize,
    now: u64,
) -> Result<(), String> {
    let mut item = history.pending[index].clone();
    let deadline = item
        .first_attempt_at
        .unwrap_or(now)
        .saturating_add(RETRY_WINDOW_MS);
    let (result, reason) = if let Some((error, reason)) = terminal_before_send(&item, now, deadline)
    {
        (Err(error), Some(reason))
    } else {
        if item.destination.is_none() {
            let env = Env::from_process();
            if let Ok(manager) = delivery_manager(ctx, &env, &item) {
                item.destination = ctx
                    .call(&["get-task", &item.task_key, "activity=1"])
                    .ok()
                    .and_then(|task| original_destination(&task, &manager.project_id));
                item.webhook = item
                    .destination
                    .as_ref()
                    .and_then(|target| target.webhook(&manager))
                    .map(str::to_owned);
            }
        }
        item.first_attempt_at.get_or_insert(now);
        item.sending = true;
        history.pending[index] = item.clone();
        save_history(path, history)?;
        (deliver_and_record(ctx, &item, false), None)
    };
    if matches!(
        result,
        Err(DeliveryError::Retry | DeliveryError::MissingSecret | DeliveryError::Keychain)
    ) {
        let delay = (INITIAL_RETRY_MS << item.attempts.min(5)).min(MAX_RETRY_MS);
        item.attempts = item.attempts.saturating_add(1);
        item.next_attempt_at = now.saturating_add(delay).min(deadline);
        item.sending = false;
        history.pending[index] = item;
        return save_history(path, history);
    }
    if let Some(reason) = reason {
        record_terminal(ctx, &item, reason);
    }
    history.pending.remove(index);
    let outcome = match result {
        Ok(_) => "delivered",
        Err(DeliveryError::Unconfirmed) => "unconfirmed",
        Err(_) if now >= deadline => "expired",
        Err(_) => "failed",
    };
    history.outcomes.push_front(DeliveryOutcome {
        id: item.id,
        change: item.change,
        destination: item.destination,
        result: outcome.into(),
    });
    history.outcomes.truncate(1000);
    save_history(path, history)
}

fn record_terminal(ctx: &Ctx, item: &Pending, reason: &str) {
    let env = Env::from_process();
    if let Ok(manager) = delivery_manager(ctx, &env, item) {
        record_result(&env, &manager, item, Some(reason));
    }
}

fn delivery_manager(
    ctx: &Ctx,
    env: &Env,
    item: &Pending,
) -> Result<manager::Manager, DeliveryError> {
    let manager = manager::find(env, &item.manager).map_err(|_| DeliveryError::Unavailable)?;
    if !manager.in_vault(&ctx.vault)
        || (!item.registration.is_empty() && manager.registration() != item.registration)
    {
        return Err(DeliveryError::Unavailable);
    }
    Ok(manager)
}

fn deliver_and_record(ctx: &Ctx, item: &Pending, manual: bool) -> Result<u16, DeliveryError> {
    let env = Env::from_process();
    let manager = delivery_manager(ctx, &env, item)?;
    let result = attempt_delivery(ctx, item, manual, &manager);
    if !matches!(result, Err(DeliveryError::NoSource))
        && (item.attempts == 0
            || result.is_ok()
            || matches!(
                result,
                Err(DeliveryError::Unavailable | DeliveryError::Unconfirmed)
            ))
    {
        record_result(
            &env,
            &manager,
            item,
            result.as_ref().err().map(delivery_error),
        );
    }
    result
}

fn record_result(env: &Env, manager: &manager::Manager, item: &Pending, reason: Option<&str>) {
    if let Err(error) = manager::record_webhook(env, manager, &item.task_key, reason) {
        eprintln!("매니저 상태 알림을 기록하지 못했다: {error}");
    }
}

fn delivery_error(error: &DeliveryError) -> &'static str {
    match error {
        DeliveryError::Unavailable => "매니저의 알림 설정과 제보 대화의 허용 여부를 확인해 주세요.",
        DeliveryError::NoSource => "원래 제보의 알림 대상을 찾을 수 없어요.",
        DeliveryError::MissingSecret => "상태 알림 설정에서 서명 비밀을 만들어 주세요.",
        DeliveryError::Keychain => "macOS 키체인에서 서명 비밀을 읽을 수 없어요.",
        DeliveryError::Unconfirmed => {
            "전달 여부를 확인할 수 없어요. 원래 대화의 알림을 확인해 주세요."
        }
        DeliveryError::Retry => "지금 Hermes에 연결할 수 없어요. 다시 연결을 시도해요.",
    }
}

fn slack_source(task: &Value) -> Option<(String, String)> {
    task["refs"].as_array()?.iter().find_map(|reference| {
        if reference["kind"] != "slack" {
            return None;
        }
        slack_thread(reference["target"].as_str()?)
    })
}

fn original_destination(task: &Value, project_id: &str) -> Option<Destination> {
    let original = task["intakeSources"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|item| item["taskOrigin"] == "created" && item["manager"] == task["manager"]);
    let Some(original) = original else {
        let (channel, thread) = slack_source(task)?;
        return Some(Destination::Slack { channel, thread });
    };
    if original["projectId"].as_str() != Some(project_id) {
        return None;
    }
    match original["source"]["kind"].as_str() {
        None => {
            let (channel, thread) = slack_thread(original["url"].as_str()?)?;
            Some(Destination::Slack { channel, thread })
        }
        Some("telegram") => {
            let target = &original["destination"];
            let chat = target["conversation"]
                .as_str()
                .filter(|value| manager::valid_telegram_chat_id(value))?;
            let message = target["message"]
                .as_str()
                .filter(|value| telegram_message_id(value))?;
            let thread = target["thread"].as_str();
            if thread.is_some_and(|value| !telegram_message_id(value)) {
                return None;
            }
            Some(Destination::Telegram {
                chat: chat.into(),
                message: message.into(),
                thread: thread.map(str::to_owned),
            })
        }
        Some(_) => None,
    }
}

fn telegram_message_id(value: &str) -> bool {
    value
        .parse::<u32>()
        .is_ok_and(|id| id > 0 && id.to_string() == value)
}

impl Destination {
    fn webhook<'a>(&self, manager: &'a manager::Manager) -> Option<&'a str> {
        match self {
            Self::Slack { channel, .. } => manager
                .allows_slack_channel(channel)
                .then_some(manager.webhook.as_deref())
                .flatten(),
            Self::Telegram { chat, .. } => manager
                .telegram_chats
                .contains(chat)
                .then_some(manager.telegram_webhook.as_deref())
                .flatten(),
        }
    }

    fn payload(&self, item: &Pending, message: String) -> Value {
        let mut payload = serde_json::json!({
            "event_type": EVENT,
            "task_key": item.task_key,
            "status": item.status,
            "status_change_id": item.change,
            "message": message,
        });
        match self {
            Self::Slack {
                channel, thread, ..
            } => {
                payload["platform"] = "slack".into();
                payload["channel"] = channel.as_str().into();
                payload["thread_id"] = thread.as_str().into();
            }
            Self::Telegram {
                chat,
                message,
                thread,
            } => {
                payload["platform"] = "telegram".into();
                payload["chat_id"] = chat.as_str().into();
                payload["original_message_id"] = message.as_str().into();
                payload["message_thread_id"] = thread.as_deref().unwrap_or_default().into();
            }
        }
        payload
    }
}

fn review_has_report_note(task: &Value) -> bool {
    let mut events = task["activity"].as_array().into_iter().flatten().rev();
    let Some(review) = events.find(|event| event["kind"] == "status" && event["to"] == "in-review")
    else {
        return false;
    };
    review["by"] == "runner"
        && events
            .take_while(|event| event["kind"] != "status")
            .find(|event| event["kind"] == "comment" && event["by"] == "runner")
            .and_then(|comment| comment["body"].as_str())
            .and_then(|body| body.strip_prefix("**분석 보고**\n\n"))
            .and_then(|body| body.strip_prefix(REPORT_NOTE_LINK))
            .and_then(|body| body.split_once(')'))
            .is_some_and(|(id, _)| {
                id.starts_with("nd")
                    && id
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            })
}

fn attempt_delivery(
    ctx: &Ctx,
    item: &Pending,
    manual: bool,
    manager: &manager::Manager,
) -> Result<u16, DeliveryError> {
    if ctx.host.vault().as_deref() != Some(ctx.vault.as_str()) {
        return Err(DeliveryError::Retry);
    }
    let mut argv = vec!["get-task", item.task_key.as_str()];
    if manual || item.status != "in-review" {
        argv.push("activity=1");
    }
    let task = ctx.call(&argv).map_err(|_| DeliveryError::Retry)?;
    let target = original_destination(&task, &manager.project_id).ok_or(DeliveryError::NoSource)?;
    if item
        .destination
        .as_ref()
        .is_some_and(|saved| saved != &target)
    {
        return Err(DeliveryError::Unavailable);
    }
    if !manager.enabled || manager.scope().is_none() {
        return Err(DeliveryError::Unavailable);
    }
    if !manager.notify_enabled || (!manual && !manager.notifies(&item.status)) {
        return Err(DeliveryError::Unavailable);
    }
    let url = target.webhook(manager).ok_or(DeliveryError::Unavailable)?;
    if item.webhook.as_deref().is_some_and(|saved| saved != url) {
        return Err(DeliveryError::Unavailable);
    }
    let repo = task["repo"].as_str().ok_or(DeliveryError::Unavailable)?;
    if task["id"].as_str() != Some(&item.task_id)
        || task["manager"].as_str() != Some(manager.name.as_str())
        || task["project"].as_str() != Some(manager.project.as_str())
        || !manager.repos.iter().any(|allowed| allowed == repo)
        || !manager
            .scope()
            .is_some_and(|project| project.repos.iter().any(|entry| entry == repo))
    {
        return Err(DeliveryError::Unavailable);
    }
    let status = TaskStatus::parse(&item.status).ok_or(DeliveryError::Unavailable)?;
    let secret = manager_webhook::keychain_secret(manager)
        .map_err(|_| DeliveryError::Keychain)?
        .ok_or(DeliveryError::MissingSecret)?;
    if ctx.host.vault().as_deref() != Some(ctx.vault.as_str()) {
        return Err(DeliveryError::Retry);
    }
    let mut message = if manual {
        format!("{}의 현재 상태는 {}입니다.", item.task_key, status.title())
    } else {
        format!(
            "{} 상태가 {}(으)로 바뀌었어요.",
            item.task_key,
            status.title()
        )
    };
    let latest_review = task["activity"]
        .as_array()
        .into_iter()
        .flatten()
        .rev()
        .find(|event| event["kind"] == "status" && event["to"] == "in-review");
    if !manual
        && status == TaskStatus::InReview
        && item.changed_at.as_deref().map_or(true, |at| {
            latest_review.is_some_and(|event| event["at"].as_str() == Some(at))
        })
        && review_has_report_note(&task)
    {
        message.push_str("\n조사 노트가 저장됐어요. aisurada의 태스크 댓글에서 열 수 있어요.");
    }
    let payload = target.payload(item, message);
    manager_webhook::send_webhook(url, &secret, &item.id, &payload)
}

pub(super) fn send_current(ctx: &Ctx, key: &str) -> Result<(), String> {
    let task = ctx.call(&["get-task", key, "activity=1"])?;
    let required = |field: &str| {
        task[field]
            .as_str()
            .ok_or_else(|| format!("태스크에 {field} 정보가 없다"))
    };
    let item = Pending {
        id: delivery_id(&format!("manual:{}:{}", required("id")?, store::now_ms())),
        task_key: required("key")?.into(),
        task_id: required("id")?.into(),
        manager: required("manager")?.into(),
        status: required("status")?.into(),
        attempts: 0,
        registration: String::new(),
        first_attempt_at: None,
        next_attempt_at: 0,
        sending: false,
        change: None,
        changed_at: None,
        destination: None,
        webhook: None,
    };
    deliver_and_record(ctx, &item, true)
        .map(|_| ())
        .map_err(|error| delivery_error(&error).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn queues_each_saved_change_once_for_the_original_telegram_topic() {
        let row: Row = serde_json::from_value(json!({
            "id":"task-1","key":"TSK-1","title":"로그인","status":"done","project":"demo",
            "manager":"hermes-demo","repo":"demo/shared",
            "statusChanges":[
                {"id":"st-one","status":"waiting-answer","at":"2026-11-25T06:10:00Z"},
                {"id":"st-two","status":"done","at":"2026-11-25T06:11:00Z"}
            ]
        }))
        .unwrap();
        let mut manager: manager::Manager = serde_json::from_value(json!({
            "name":"hermes-demo","label":"Hermes","enabled":true,"tools":["list_tasks"],"repos":["demo/shared"],
            "agendas":[],"project":"demo","projectId":"project-id","notifyEnabled":true,
            "notify":["waiting-answer","done"],"webhook":"http://127.0.0.1:8644/webhooks/slack",
            "telegramWebhook":"http://127.0.0.1:8644/webhooks/telegram","telegramChats":["-10012345"]
        })).unwrap();
        let task = json!({"id":"task-1","manager":"hermes-demo",
            "refs":[{"kind":"slack","target":"https://demo.slack.com/archives/CDEMO123/p1234567890123456"}],
            "intakeSources":[
                {"taskOrigin":"created","manager":"hermes-demo","projectId":"project-id","source":{"kind":"telegram"},
                 "destination":{"conversation":"-10012345","message":"42","thread":"11"}},
                {"taskOrigin":"commented","manager":"hermes-demo","projectId":"project-id","source":{"kind":"telegram"},
                 "destination":{"conversation":"98765","message":"88"}}
            ]
        });
        let mut history = History::default();
        queue_changes(&mut history, &row, Some(&manager), || Ok(task.clone()));
        assert_eq!(history.pending.len(), 2);
        assert_ne!(history.pending[0].id, history.pending[1].id);
        let first = &history.pending[0];
        let destination = first.destination.as_ref().unwrap();
        let payload = destination.payload(first, "시험".into());
        assert_eq!(payload["platform"], "telegram");
        assert_eq!(payload["chat_id"], "-10012345");
        assert_eq!(payload["message_thread_id"], "11");
        assert_eq!(payload["original_message_id"], "42");
        assert_eq!(payload["status_change_id"], "st-one");
        assert_eq!(
            first.webhook.as_deref(),
            manager.telegram_webhook.as_deref()
        );
        let mut restored: History =
            serde_json::from_slice(&serde_json::to_vec(&history).unwrap()).unwrap();
        queue_changes(&mut restored, &row, Some(&manager), || {
            panic!("stored events are already queued")
        });
        assert_eq!(restored.pending, history.pending);
        restored.pending[0].sending = true;
        let reopened: History =
            serde_json::from_slice(&serde_json::to_vec(&restored).unwrap()).unwrap();
        assert!(matches!(
            terminal_before_send(&reopened.pending[0], 0, 1),
            Some((DeliveryError::Unconfirmed, _))
        ));
        manager.telegram_chats.clear();
        let mut rejected = History::default();
        queue_changes(&mut rejected, &row, Some(&manager), || Ok(task));
        assert!(rejected.pending.is_empty());
    }

    #[test]
    fn finds_only_a_slack_message_source() {
        assert_eq!(slack_source(&json!({"refs": []})), None);
        assert_eq!(
            slack_source(&json!({"refs": [{"kind": "slack", "target": "https://example.com"}]})),
            None
        );
        let linked = json!({"refs": [{"kind": "slack", "target": "https://example.slack.com/archives/CDEMO123/p1234567890123456"}]});
        assert_eq!(
            slack_source(&linked),
            Some(("CDEMO123".into(), "1234567890.123456".into()))
        );
    }

    #[test]
    fn mentions_a_note_only_for_the_corresponding_runner_review() {
        let activity = json!({"activity": [
            {"kind": "status", "by": "runner", "to": "in-progress"},
            {"kind": "comment", "by": "runner", "body": format!("**분석 보고**\n\n{REPORT_NOTE_LINK}ndtest-00000001)\n\n보고")},
            {"kind": "status", "by": "runner", "to": "in-review"},
            {"kind": "comment", "by": "human", "body": "확인 중"},
        ]});
        assert!(review_has_report_note(&activity));

        let mut without_note = activity.clone();
        without_note["activity"][1]["body"] = json!("**분석 보고**\n\n보고만 남김");
        assert!(!review_has_report_note(&without_note));

        let mut second_review = activity;
        second_review["activity"].as_array_mut().unwrap().extend([
            json!({"kind": "status", "by": "runner", "to": "in-progress"}),
            json!({"kind": "status", "by": "runner", "to": "in-review"}),
        ]);
        assert!(!review_has_report_note(&second_review));
    }
}
