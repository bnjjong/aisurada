//! 매니저 상태 알림의 서명과 이 기기의 웹훅 전송을 맡는다.

use crate::lifecycle::HostState;
use ais_cli::{env::Env, manager};
use ring::{
    digest, hmac,
    rand::{SecureRandom, SystemRandom},
};
use serde_json::{json, Value};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{Runtime, State, WebviewWindow};

pub const EVENT: &str = "aisurada.task.status";
#[cfg(target_os = "macos")]
use manager::{KEYCHAIN_ITEM_NOT_FOUND, KEYCHAIN_SERVICE};

#[derive(Debug)]
pub enum DeliveryError {
    Unavailable,
    #[cfg(feature = "claude")]
    NoSource,
    #[cfg(feature = "claude")]
    MissingSecret,
    #[cfg(feature = "claude")]
    Keychain,
    Unconfirmed,
    Retry,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn delivery_id(value: &str) -> String {
    hex(digest::digest(&digest::SHA256, value.as_bytes()).as_ref())
}

fn active_manager<R: Runtime>(
    window: &WebviewWindow<R>,
    state: &HostState,
    name: &str,
) -> Result<(Env, manager::Manager), String> {
    if window.label() != "main" {
        return Err("매니저 설정은 메인 창에서 열어 주세요".into());
    }
    let vault = state
        .vault
        .lock()
        .map_err(|_| "열린 vault를 확인하지 못했어요")?
        .clone()
        .ok_or("열린 vault가 없어요")?;
    let env = Env::from_process();
    let manager = manager::find(&env, name)?;
    if !manager.in_vault(&vault) || manager.scope().is_none() {
        return Err("현재 vault에서 이 매니저의 프로젝트를 찾지 못했어요".into());
    }
    Ok((env, manager))
}

#[cfg(target_os = "macos")]
fn ensure_secret(
    env: &Env,
    manager: &manager::Manager,
    regenerate: bool,
) -> Result<Vec<u8>, String> {
    manager::with_current_registration(env, manager, || write_secret(manager, regenerate))
}

#[cfg(target_os = "macos")]
fn write_secret(manager: &manager::Manager, regenerate: bool) -> Result<Vec<u8>, String> {
    if let Some(secret) = keychain_secret(manager)? {
        if !regenerate {
            return Ok(secret);
        }
    }
    let mut bytes = [0; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| "서명 비밀을 만들지 못했어요")?;
    let secret = hex(&bytes).into_bytes();
    security_framework::passwords::set_generic_password(
        KEYCHAIN_SERVICE,
        &manager.keychain_account(),
        &secret,
    )
    .map_err(|error| format!("키체인에 서명 비밀을 저장하지 못했어요. {error}"))?;
    Ok(secret)
}

#[cfg(not(target_os = "macos"))]
fn ensure_secret(
    _env: &Env,
    _manager: &manager::Manager,
    _regenerate: bool,
) -> Result<Vec<u8>, String> {
    Err("서명 비밀은 macOS 키체인에 저장할 수 있어요".into())
}

#[tauri::command]
pub async fn manager_webhook_secret<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, HostState>,
    name: String,
    action: String,
) -> Result<String, String> {
    let (env, manager) = active_manager(&window, &state, &name)?;
    tauri::async_runtime::spawn_blocking(move || {
        if !matches!(action.as_str(), "ensure" | "copy" | "rotate") {
            return Err("지원하지 않는 서명 비밀 작업이에요".into());
        }
        let secret = ensure_secret(&env, &manager, action == "rotate")?;
        let secret = String::from_utf8(secret).map_err(|_| "서명 비밀을 글자로 읽지 못했어요")?;
        if action == "copy" {
            Ok(secret)
        } else {
            let suffix: String = secret
                .chars()
                .rev()
                .take(4)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            Ok(format!("••••••••••••{suffix}"))
        }
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn manager_test_webhook<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, HostState>,
    name: String,
    url: String,
    channel: String,
    platform: Option<String>,
) -> Result<Value, String> {
    let (env, manager) = active_manager(&window, &state, &name)?;
    manager::validate_webhook(&url)?;
    let payload = test_payload(&manager, platform.as_deref().unwrap_or("slack"), &channel)?;
    tauri::async_runtime::spawn_blocking(move || {
        let secret = ensure_secret(&env, &manager, false)?;
        let id = delivery_id(&format!("test:{}:{:?}", manager.name, SystemTime::now()));
        let started = Instant::now();
        let result = send_webhook(&url, &secret, &id, &payload);
        let outcome = if result.is_ok() { "ok" } else { "failed" };
        if let Err(error) = manager::record(&env, &manager, "test_webhook", outcome, None) {
            eprintln!("매니저 시험 알림을 기록하지 못했다: {error}");
        }
        let status = result.map_err(|error| match error {
            DeliveryError::Unconfirmed => {
                "전달 여부를 확인할 수 없어요. 선택한 대화에서 시험 알림을 확인해 주세요."
            }
            _ => "시험 알림을 전달하지 못했어요. 웹훅 주소와 서명 비밀을 확인해 주세요.",
        })?;
        Ok(json!({"status": status, "elapsed": started.elapsed().as_millis() as u64}))
    })
    .await
    .map_err(|error| error.to_string())?
}

fn test_payload(
    manager: &manager::Manager,
    platform: &str,
    channel: &str,
) -> Result<Value, String> {
    let mut payload = json!({
        "event_type": EVENT, "platform": platform, "status": "test",
        "message": "aisurada 매니저 연결 시험 알림이에요.",
    });
    match platform {
        "slack" => {
            if !slack_channel_id(channel) || !manager.allows_slack_channel(channel) {
                return Err("매니저의 연결 탭에서 등록한 Slack 채널 ID를 골라 주세요".into());
            }
            payload["channel"] = channel.into();
            payload["thread_id"] = "".into();
        }
        "telegram" => {
            if !manager::valid_telegram_chat_id(channel)
                || !manager.telegram_chats.iter().any(|chat| chat == channel)
            {
                return Err("허용할 텔레그램 대화 ID를 저장한 뒤 골라 주세요".into());
            }
            payload["chat_id"] = channel.into();
            payload["message_thread_id"] = "".into();
        }
        _ => return Err("지원하는 시험 알림 대상을 골라 주세요".into()),
    }
    Ok(payload)
}

fn slack_channel_id(channel: &str) -> bool {
    (2..=64).contains(&channel.len())
        && matches!(channel.as_bytes()[0], b'C' | b'G')
        && channel
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

#[cfg(target_os = "macos")]
pub fn keychain_secret(manager: &manager::Manager) -> Result<Option<Vec<u8>>, String> {
    match security_framework::passwords::get_generic_password(
        KEYCHAIN_SERVICE,
        &manager.keychain_account(),
    ) {
        Ok(secret) if !secret.is_empty() => Ok(Some(secret)),
        Ok(_) => Err("키체인의 서명 비밀이 비어 있어요".into()),
        Err(error) if error.code() == KEYCHAIN_ITEM_NOT_FOUND => Ok(None),
        Err(error) => Err(format!("키체인의 서명 비밀을 읽지 못했어요. {error}")),
    }
}

#[cfg(not(target_os = "macos"))]
pub fn keychain_secret(_manager: &manager::Manager) -> Result<Option<Vec<u8>>, String> {
    Ok(None)
}

pub fn send_webhook(
    url: &str,
    secret: &[u8],
    id: &str,
    payload: &Value,
) -> Result<u16, DeliveryError> {
    manager::validate_webhook(url).map_err(|_| DeliveryError::Unavailable)?;
    // localhost도 이름 조회 없이 루프백 주소로 직접 연결한다.
    let url = if let Some(endpoint) = url.strip_prefix("http://localhost:") {
        format!("http://127.0.0.1:{endpoint}")
    } else {
        url.to_owned()
    };
    let parsed = tauri::Url::parse(&url).map_err(|_| DeliveryError::Unavailable)?;
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
        .map_err(|_| DeliveryError::Retry)?
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
        .map_err(|error| match error {
            ureq::Error::ConnectionFailed => DeliveryError::Retry,
            ureq::Error::Io(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
                DeliveryError::Retry
            }
            _ => DeliveryError::Unconfirmed,
        })?;
    if !response.status().is_success() {
        return Err(match response.status().as_u16() {
            429 => DeliveryError::Retry,
            400 | 401 | 403 | 404 => DeliveryError::Unavailable,
            _ => DeliveryError::Unconfirmed,
        });
    }
    let status = response.status().as_u16();
    let bytes = response
        .body_mut()
        .with_config()
        .limit(4096)
        .read_to_vec()
        .map_err(|_| DeliveryError::Unconfirmed)?;
    let acknowledgment: Value =
        serde_json::from_slice(&bytes).map_err(|_| DeliveryError::Unconfirmed)?;
    match acknowledgment["status"].as_str() {
        Some("delivered") => Ok(status),
        Some("duplicate") => Err(DeliveryError::Unconfirmed),
        _ => Err(DeliveryError::Unconfirmed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::HashMap,
        io::{BufRead, BufReader, Read, Write},
        net::TcpListener,
        thread,
    };

    #[test]
    fn test_notice_uses_the_selected_platform_and_allowed_conversation() {
        let manager: manager::Manager = serde_json::from_value(json!({
            "name": "hermes-demo", "label": "Hermes", "enabled": true,
            "tools": [], "repos": [], "agendas": [],
            "slackChannels": ["CDEMO123"], "telegramChats": ["-10012345"]
        }))
        .unwrap();
        let telegram = test_payload(&manager, "telegram", "-10012345").unwrap();
        assert_eq!(telegram["chat_id"], "-10012345");
        assert_eq!(telegram["message_thread_id"], "");
        assert!(test_payload(&manager, "telegram", "12345").is_err());
        let slack = test_payload(&manager, "slack", "CDEMO123").unwrap();
        assert_eq!(slack["channel"], "CDEMO123");
        assert!(test_payload(&manager, "slack", "CDEMO456").is_err());
    }

    #[test]
    fn delivers_signed_status_to_loopback_receiver() {
        assert_eq!(send_with_reply(200, "delivered").unwrap(), 200);
        assert!(matches!(
            send_with_reply(200, "duplicate"),
            Err(DeliveryError::Unconfirmed)
        ));
        assert!(matches!(
            send_with_reply(429, "rate_limited"),
            Err(DeliveryError::Retry)
        ));
        assert!(matches!(
            send_with_reply(502, "error"),
            Err(DeliveryError::Unconfirmed)
        ));
    }

    fn send_with_reply(status: u16, acknowledgment: &'static str) -> Result<u16, DeliveryError> {
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
            let body = json!({"status": acknowledgment}).to_string();
            write!(
                socket,
                "HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        let payload = serde_json::json!({
            "event_type": EVENT, "channel": "CDEMO123", "thread_id": "1234567890.123456",
            "task_key": "TSK-1", "status": "in-review",
        });
        let result = send_webhook(&url, secret, "demo-event", &payload);
        response.join().unwrap();
        result
    }
}
