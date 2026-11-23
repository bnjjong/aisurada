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
const KEYCHAIN_SERVICE: &str = "aisurada.manager.webhook";
#[cfg(target_os = "macos")]
const KEYCHAIN_ITEM_NOT_FOUND: i32 = -25300;
#[cfg(target_os = "macos")]
static KEYCHAIN_WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Debug)]
pub enum DeliveryError {
    Unavailable,
    #[cfg(feature = "claude")]
    NoSlackSource,
    #[cfg(feature = "claude")]
    MissingSecret,
    #[cfg(feature = "claude")]
    Keychain,
    Unconfirmed,
    Retry {
        rejected: bool,
    },
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
fn ensure_secret(manager: &manager::Manager, regenerate: bool) -> Result<Vec<u8>, String> {
    let _write = KEYCHAIN_WRITE
        .lock()
        .map_err(|_| "키체인 작업을 완료하지 못했어요")?;
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
fn ensure_secret(_manager: &manager::Manager, _regenerate: bool) -> Result<Vec<u8>, String> {
    Err("서명 비밀은 macOS 키체인에 저장할 수 있어요".into())
}

#[tauri::command]
pub async fn manager_webhook_secret<R: Runtime>(
    window: WebviewWindow<R>,
    state: State<'_, HostState>,
    name: String,
    action: String,
) -> Result<String, String> {
    let (_, manager) = active_manager(&window, &state, &name)?;
    tauri::async_runtime::spawn_blocking(move || {
        if !matches!(action.as_str(), "ensure" | "copy" | "rotate") {
            return Err("지원하지 않는 서명 비밀 작업이에요".into());
        }
        let secret = ensure_secret(&manager, action == "rotate")?;
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
) -> Result<Value, String> {
    let (env, manager) = active_manager(&window, &state, &name)?;
    manager::validate_webhook(&url)?;
    if !slack_channel_id(&channel) {
        return Err("시험 알림을 받을 Slack 채널 ID를 적어 주세요".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let secret = ensure_secret(&manager, false)?;
        let id = delivery_id(&format!("test:{}:{:?}", manager.name, SystemTime::now()));
        let payload = json!({
            "event_type": EVENT, "channel": channel, "thread_id": "", "status": "test",
            "message": "aisurada 매니저 연결 시험 알림이에요.",
        });
        let started = Instant::now();
        let result = send_webhook(&url, &secret, &id, &payload);
        let outcome = if result.is_ok() { "ok" } else { "failed" };
        if let Err(error) = manager::record(&env, &manager, "test_webhook", outcome, None) {
            eprintln!("매니저 시험 알림을 기록하지 못했다: {error}");
        }
        let status = result.map_err(|error| match error {
            DeliveryError::Retry { rejected: true } => {
                "웹훅이 Slack에 알림을 전달하지 못했어요. 채널 ID와 봇 연결을 확인해 주세요."
            }
            DeliveryError::Unconfirmed => {
                "Hermes가 중복 요청으로 답해 Slack 전달 여부를 확인하지 못했어요."
            }
            _ => "시험 알림을 전달하지 못했어요. 웹훅 주소와 서명 비밀을 확인해 주세요.",
        })?;
        Ok(json!({"status": status, "elapsed": started.elapsed().as_millis() as u64}))
    })
    .await
    .map_err(|error| error.to_string())?
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
    let status = response.status().as_u16();
    let bytes = response
        .body_mut()
        .with_config()
        .limit(4096)
        .read_to_vec()
        .map_err(|_| DeliveryError::Retry { rejected: false })?;
    let acknowledgment: Value =
        serde_json::from_slice(&bytes).map_err(|_| DeliveryError::Retry { rejected: false })?;
    match acknowledgment["status"].as_str() {
        Some("delivered") => Ok(status),
        Some("duplicate") => Err(DeliveryError::Unconfirmed),
        _ => Err(DeliveryError::Retry { rejected: false }),
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
    fn delivers_signed_status_to_loopback_receiver() {
        assert_eq!(send_with_reply("delivered").unwrap(), 200);
        assert!(matches!(
            send_with_reply("duplicate"),
            Err(DeliveryError::Unconfirmed)
        ));
    }

    fn send_with_reply(acknowledgment: &'static str) -> Result<u16, DeliveryError> {
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
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
