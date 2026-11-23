//! 매니저 태스크가 들어오거나 상태가 바뀌었을 때 알린다.

use super::{job::Ctx, store};
use crate::notifications::NewItem;
use ais_cli::{env::Env, manager};
use ais_domain::task::{format::slack_thread, TaskStatus};
use ring::{digest, hmac};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, VecDeque},
    fs,
    io::ErrorKind,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const EVENT: &str = "aisurada.task.status";
const KEYCHAIN_SERVICE: &str = "aisurada.manager.webhook";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Row {
    id: String,
    key: String,
    title: String,
    manager: String,
    repo: String,
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

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn delivery_id(value: &str) -> String {
    hex(digest::digest(&digest::SHA256, value.as_bytes()).as_ref())
}

fn should_send(status: &str) -> bool {
    matches!(
        status,
        "ready-for-develop" | "in-review" | "done" | "failed"
    )
}

pub(super) fn poll(ctx: &Ctx) -> Result<(), String> {
    let managers = manager::list(&Env::from_process())?;
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
                item.name == row.manager && item.enabled && item.repos.contains(&row.repo)
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
                            && should_send(&row.status)
                            && configured.webhook.is_some() =>
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
                manager.name == item.manager && manager.enabled && manager.webhook.is_some()
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
        let result = attempt_delivery(ctx, &item, false);
        match result {
            Ok(()) => {}
            Err(DeliveryError::Unavailable) => {}
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

#[derive(Debug)]
enum DeliveryError {
    Unavailable,
    Retry { rejected: bool },
}

fn attempt_delivery(ctx: &Ctx, item: &Pending, manual: bool) -> Result<(), DeliveryError> {
    if ctx.host.vault().as_deref() != Some(ctx.vault.as_str()) {
        return Err(DeliveryError::Retry { rejected: false });
    }
    let manager = manager::find_enabled(&Env::from_process(), &item.manager)
        .map_err(|_| DeliveryError::Unavailable)?;
    let url = manager
        .webhook
        .as_deref()
        .ok_or(DeliveryError::Unavailable)?;
    let task = ctx
        .call(&["get-task", &item.task_key, "activity=1"])
        .map_err(|_| DeliveryError::Retry { rejected: false })?;
    let repo = task["repo"].as_str().ok_or(DeliveryError::Unavailable)?;
    if task["id"].as_str() != Some(&item.task_id)
        || !manager.repos.iter().any(|allowed| allowed == repo)
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
    let secret = keychain_secret(&manager.name).ok_or(DeliveryError::Unavailable)?;
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
    send_webhook(url, &secret, &item.id, &payload)
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
    attempt_delivery(ctx, &item, true).map_err(|error| match error {
        DeliveryError::Unavailable => {
            "매니저 설정, 웹훅 서명 키 또는 Slack 출처를 확인해 주세요.".into()
        }
        DeliveryError::Retry { .. } => "상태 알림을 전달하지 못했어요.".into(),
    })
}

#[cfg(target_os = "macos")]
fn keychain_secret(manager: &str) -> Option<Vec<u8>> {
    security_framework::passwords::get_generic_password(KEYCHAIN_SERVICE, manager)
        .ok()
        .filter(|secret| !secret.is_empty())
}

#[cfg(not(target_os = "macos"))]
fn keychain_secret(_manager: &str) -> Option<Vec<u8>> {
    None
}

fn send_webhook(url: &str, secret: &[u8], id: &str, payload: &Value) -> Result<(), DeliveryError> {
    let parsed = tauri::Url::parse(url).map_err(|_| DeliveryError::Unavailable)?;
    if parsed.scheme() != "http"
        || !matches!(parsed.host_str(), Some("127.0.0.1" | "[::1]"))
        || parsed.port_or_known_default().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(DeliveryError::Unavailable);
    }
    let body = payload.to_string();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| DeliveryError::Retry { rejected: false })?
        .as_secs()
        .to_string();
    let signature = hmac::sign(
        &hmac::Key::new(hmac::HMAC_SHA256, secret),
        format!("{timestamp}.{body}").as_bytes(),
    );
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .proxy(None)
        .max_redirects(0)
        .timeout_global(Some(Duration::from_secs(3)))
        .build()
        .into();
    let mut response = agent
        .post(parsed.as_str())
        .header("Content-Type", "application/json")
        .header("X-Webhook-Timestamp", &timestamp)
        .header("X-Webhook-Signature-V2", hex(signature.as_ref()))
        .header("X-Request-ID", id)
        .send(body)
        .map_err(|_| DeliveryError::Retry { rejected: false })?;
    if !response.status().is_success() {
        return Err(DeliveryError::Retry {
            rejected: response.status().as_u16() == 502,
        });
    }
    let bytes = response
        .body_mut()
        .with_config()
        .limit(4096)
        .read_to_vec()
        .map_err(|_| DeliveryError::Retry { rejected: false })?;
    let acknowledgment: Value =
        serde_json::from_slice(&bytes).map_err(|_| DeliveryError::Retry { rejected: false })?;
    if !matches!(
        acknowledgment["status"].as_str(),
        Some("delivered" | "duplicate")
    ) {
        return Err(DeliveryError::Retry { rejected: false });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::TcpListener,
        thread,
    };

    #[test]
    fn delivers_signed_status_to_loopback_receiver() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let url = format!(
            "http://{}/webhooks/aisurada",
            listener.local_addr().unwrap()
        );
        let secret = b"isolated-test-key";
        let response = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut reader = BufReader::new(&mut socket);
            let mut request = String::new();
            reader.read_line(&mut request).unwrap();
            assert_eq!(request, "POST /webhooks/aisurada HTTP/1.1\r\n");
            let mut headers = HashMap::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                let (key, value) = line.split_once(':').unwrap();
                headers.insert(key.to_ascii_lowercase(), value.trim().to_string());
            }
            let mut body = vec![0; headers["content-length"].parse().unwrap()];
            reader.read_exact(&mut body).unwrap();
            let payload: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(payload["thread_id"], "1234567890.123456");
            assert_eq!(payload["status"], "in-review");
            assert_eq!(headers["x-request-id"], "demo-event");
            let timestamp = &headers["x-webhook-timestamp"];
            let signed = [timestamp.as_bytes(), b".", &body].concat();
            let expected = hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, secret), &signed);
            assert_eq!(headers["x-webhook-signature-v2"], hex(expected.as_ref()));
            let body = r#"{"status":"delivered"}"#;
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        let payload = serde_json::json!({
            "event_type": EVENT, "channel": "CDEMO123", "thread_id": "1234567890.123456",
            "task_key": "TSK-1", "status": "in-review",
        });
        send_webhook(&url, secret, "demo-event", &payload).unwrap();
        response.join().unwrap();
    }
}
