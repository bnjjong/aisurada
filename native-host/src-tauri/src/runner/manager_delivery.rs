//! 매니저 태스크가 들어오거나 상태가 바뀌었을 때 알린다.

use super::{job::Ctx, store};
use crate::manager_webhook::{self, delivery_id, DeliveryError, EVENT};
use crate::notifications::NewItem;
use ais_cli::{env::Env, manager};
use ais_domain::task::{format::slack_thread, TaskStatus};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    fs,
    io::ErrorKind,
};

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
        Ok(bytes) => Some(serde_json::from_slice::<History>(&bytes).map_err(|e| e.to_string())?),
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
                        });
                    }
                    _ => {}
                }
            }
        }
        next.statuses.insert(row.id, row.status);
    }
    next.pending.retain(|item| {
        next.statuses.contains_key(&item.task_id)
            && managers.iter().any(|manager| {
                manager.name == item.manager
                    && manager.enabled
                    && manager.webhook_for_status(&item.status).is_some()
            })
    });
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

    if let Some(mut item) = next.pending.pop_front() {
        let result = deliver_and_record(ctx, &item, false);
        match result {
            Ok(_) => {}
            Err(DeliveryError::Unavailable | DeliveryError::MissingSecret) => {}
            Err(DeliveryError::Retry { rejected }) if item.attempts < 5 => {
                item.attempts += 1;
                if rejected {
                    // Hermes가 실패 응답을 주기 전에 요청 ID를 기록하므로 재시도에는 새 ID를 쓴다.
                    item.id = delivery_id(&format!("{}:{}", item.id, item.attempts));
                }
                next.pending.push_back(item);
            }
            Err(DeliveryError::Retry { .. }) => {
                eprintln!("{} 매니저 알림을 여러 번 보내지 못했다", item.task_key);
            }
        }
        let bytes = serde_json::to_vec(&next).map_err(|e| e.to_string())?;
        store::write_atomic(&path, &bytes)?;
    }
    Ok(())
}

fn deliver_and_record(ctx: &Ctx, item: &Pending, manual: bool) -> Result<u16, DeliveryError> {
    let env = Env::from_process();
    let manager =
        manager::find_enabled(&env, &item.manager).map_err(|_| DeliveryError::Unavailable)?;
    if !manager.in_vault(&ctx.vault) {
        return Err(DeliveryError::Unavailable);
    }
    let result = attempt_delivery(ctx, item, manual, &manager);
    if result.is_ok() || manual || !matches!(result, Err(DeliveryError::Unavailable)) {
        let outcome = if result.is_ok() { "ok" } else { "failed" };
        if let Err(error) = manager::record(
            &env,
            &manager,
            "status_webhook",
            outcome,
            Some(&item.task_key),
        ) {
            eprintln!("매니저 상태 알림을 기록하지 못했다: {error}");
        }
    }
    result
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
    let url = if manual && manager.notify_enabled {
        manager.webhook.as_deref()
    } else {
        manager.webhook_for_status(&item.status)
    }
    .ok_or(DeliveryError::Unavailable)?;
    let task = ctx
        .call(&["get-task", &item.task_key, "activity=1"])
        .map_err(|_| DeliveryError::Retry { rejected: false })?;
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
    let (channel, thread) = task["refs"]
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|reference| {
            if reference["kind"] == "slack" {
                slack_thread(reference["target"].as_str()?)
            } else {
                None
            }
        })
        .ok_or(DeliveryError::Unavailable)?;
    let status = TaskStatus::parse(&item.status).ok_or(DeliveryError::Unavailable)?;
    let secret = manager_webhook::keychain_secret(manager).ok_or(DeliveryError::MissingSecret)?;
    let message = if manual {
        format!("{}의 현재 상태는 {}입니다.", item.task_key, status.title())
    } else {
        format!(
            "{} 상태가 {}(으)로 바뀌었어요.",
            item.task_key,
            status.title()
        )
    };
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
    };
    deliver_and_record(ctx, &item, true)
        .map(|_| ())
        .map_err(|error| match error {
            DeliveryError::Unavailable | DeliveryError::MissingSecret => {
                "매니저 설정, 웹훅 서명 키 또는 Slack 출처를 확인해 주세요.".into()
            }
            DeliveryError::Retry { .. } => "상태 알림을 전달하지 못했어요.".into(),
        })
}
