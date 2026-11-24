//! 매니저 태스크가 들어오거나 상태가 바뀌었을 때 알린다.

use super::{job::Ctx, protocol::REPORT_NOTE_LINK, store};
use crate::manager_webhook::{self, delivery_id, DeliveryError, EVENT};
use crate::notifications::NewItem;
use ais_cli::{env::Env, manager};
use ais_domain::task::{format::slack_thread, TaskStatus};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet, VecDeque},
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
    edited_at: String,
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
}

#[derive(Default, Deserialize, PartialEq, Serialize)]
struct History {
    statuses: HashMap<String, String>,
    pending: VecDeque<Pending>,
}

pub(super) fn poll(ctx: &Ctx) -> Result<(), String> {
    let managers: Vec<_> = manager::list(&Env::from_process())?
        .into_iter()
        .filter(|item| item.in_vault(&ctx.vault) && item.scope().is_some())
        .collect();
    if managers.is_empty() {
        return Ok(());
    }
    let list = ctx.call(&["list-tasks"])?;
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
                let backup = path.with_extension(format!("corrupt-{}.json", store::now_ms()));
                fs::rename(&path, &backup).map_err(|error| error.to_string())?;
                eprintln!("매니저 알림 상태 파일을 별도로 보관하고 다시 만들어요: {error}");
                None
            }
        },
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => return Err(error.to_string()),
    };
    let mut next = History::default();
    if let Some(previous) = &saved {
        next.pending = previous.pending.clone();
    }
    let mut arrived = Vec::new();
    for row in rows
        .iter()
        .filter_map(|row| serde_json::from_value::<Row>(row.clone()).ok())
    {
        if let Some(previous) = &saved {
            let configured = managers.iter().find(|item| {
                item.name == row.manager
                    && item.enabled
                    && item.repos.contains(&row.repo)
                    && row.project.as_deref() == Some(item.project.as_str())
            });
            if let Some(configured) = configured {
                match previous.statuses.get(&row.id) {
                    None => arrived.push(NewItem {
                        kind: "manager".into(),
                        title: format!("{}가 태스크를 접수했어요", configured.label),
                        body: row.title,
                        task_key: Some(row.key),
                        note_ref: None,
                        intake: None,
                        id: Some(format!("manager-{}", row.id)),
                    }),
                    Some(old)
                        if old != &row.status
                            && configured.webhook_for_status(&row.status).is_some() =>
                    {
                        next.pending.push_back(Pending {
                            id: delivery_id(&format!(
                                "{}:{}:{}",
                                row.id, row.status, row.edited_at
                            )),
                            task_key: row.key,
                            task_id: row.id.clone(),
                            manager: row.manager,
                            status: row.status.clone(),
                            attempts: 0,
                            registration: configured.registration().into(),
                            first_attempt_at: None,
                            next_attempt_at: 0,
                        });
                    }
                    _ => {}
                }
            }
        }
        next.statuses.insert(row.id, row.status);
    }
    if saved.as_ref() != Some(&next) {
        let bytes = serde_json::to_vec(&next).map_err(|e| e.to_string())?;
        store::write_atomic(&path, &bytes)?;
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
        if next.pending[index].first_attempt_at.is_none() {
            next.pending[index].first_attempt_at = Some(now);
            let bytes = serde_json::to_vec(&next).map_err(|e| e.to_string())?;
            store::write_atomic(&path, &bytes)?;
        }
        let mut item = next.pending.remove(index).expect("대기열에서 확인한 항목");
        let deadline = item
            .first_attempt_at
            .unwrap_or(now)
            .saturating_add(RETRY_WINDOW_MS);
        if now >= deadline {
            record_expired(ctx, &item);
        } else {
            let result = deliver_and_record(ctx, &item, false);
            if matches!(
                result,
                Err(DeliveryError::Retry { .. }
                    | DeliveryError::MissingSecret
                    | DeliveryError::Keychain)
            ) {
                let delay = (INITIAL_RETRY_MS << item.attempts.min(5)).min(MAX_RETRY_MS);
                item.attempts = item.attempts.saturating_add(1);
                if matches!(result, Err(DeliveryError::Retry { rejected: true })) {
                    // Slack 전달 실패가 확인됐으므로 Hermes의 중복 판정을 피할 새 ID를 쓴다.
                    item.id = delivery_id(&format!("{}:{}", item.id, item.attempts));
                }
                item.next_attempt_at = now.saturating_add(delay).min(deadline);
                next.pending.insert(index, item);
            }
        }
        let bytes = serde_json::to_vec(&next).map_err(|e| e.to_string())?;
        store::write_atomic(&path, &bytes)?;
    }
    Ok(())
}

fn record_expired(ctx: &Ctx, item: &Pending) {
    if ctx
        .call(&["get-task", &item.task_key, "activity=1"])
        .is_ok_and(|task| slack_source(&task).is_none())
    {
        return;
    }
    let env = Env::from_process();
    if let Ok(manager) = delivery_manager(ctx, &env, item) {
        record_result(
            &env,
            &manager,
            item,
            Some("재시도 시간이 지나 알림을 보낼 수 없었어요."),
        );
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
    if !matches!(result, Err(DeliveryError::NoSlackSource))
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
        DeliveryError::Unavailable => "매니저 설정이나 현재 태스크의 Slack 출처를 확인해 주세요.",
        DeliveryError::NoSlackSource => {
            "Slack 원글 주소가 있는 태스크에만 상태 알림을 보낼 수 있어요."
        }
        DeliveryError::MissingSecret => "상태 알림 설정에서 서명 비밀을 만들어 주세요.",
        DeliveryError::Keychain => "macOS 키체인에서 서명 비밀을 읽을 수 없어요.",
        DeliveryError::Unconfirmed => {
            "Hermes가 중복 요청으로 답해 Slack 전달 여부를 확인하지 못했어요."
        }
        DeliveryError::Retry { rejected: true } => "Hermes가 Slack에 알림을 전달하지 못했어요.",
        DeliveryError::Retry { rejected: false } => "Hermes의 알림 응답을 받지 못했어요.",
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
        return Err(DeliveryError::Retry { rejected: false });
    }
    let mut argv = vec!["get-task", item.task_key.as_str()];
    if manual || item.status != "in-review" {
        argv.push("activity=1");
    }
    let task = ctx
        .call(&argv)
        .map_err(|_| DeliveryError::Retry { rejected: false })?;
    let (channel, thread) = slack_source(&task).ok_or(DeliveryError::NoSlackSource)?;
    if !manager.enabled || manager.scope().is_none() {
        return Err(DeliveryError::Unavailable);
    }
    let url = if manual && manager.notify_enabled {
        manager.webhook.as_deref()
    } else {
        manager.webhook_for_status(&item.status)
    }
    .ok_or(DeliveryError::Unavailable)?;
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
        return Err(DeliveryError::Retry { rejected: false });
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
    if !manual && status == TaskStatus::InReview && review_has_report_note(&task) {
        message.push_str("\n조사 노트가 저장됐어요. aisurada의 태스크 댓글에서 열 수 있어요.");
    }
    let payload = serde_json::json!({
        "event_type": EVENT,
        "channel": channel,
        "thread_id": thread,
        "task_key": item.task_key,
        "status": item.status,
        "message": message,
    });
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
