#[cfg(feature = "claude")]
mod change;
mod mcp;
mod model;
mod oauth;

use oauth::Session;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tauri::{AppHandle, State};
use ureq::http::Response;

pub type Result<T> = std::result::Result<T, String>;
const NETWORK: &str = "Archyl에 연결하지 못했어요. 잠시 뒤 다시 시도해 주세요.";
const RECONNECT: &str = "설정에서 Archyl에 다시 로그인해 주세요.";
const RESPONSE: &str = "Archyl 응답을 읽지 못했어요.";

#[derive(Clone, Default)]
pub struct ArchylState(Arc<StateInner>);

#[derive(Default)]
struct StateInner {
    network: Mutex<()>,
    login: AtomicBool,
    generation: AtomicU64,
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .max_redirects(0)
        .timeout_connect(Some(Duration::from_secs(8)))
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into()
}

fn json_body<T: DeserializeOwned>(mut response: Response<ureq::Body>) -> Result<T> {
    let body = response
        .body_mut()
        .with_config()
        .limit(256 * 1024)
        .read_to_vec()
        .map_err(|_| RESPONSE)?;
    serde_json::from_slice(&body).map_err(|_| RESPONSE.into())
}

fn http_error(response: Response<ureq::Body>, fallback: &str) -> String {
    match response.status().as_u16() {
        429 => "Archyl 요청 한도에 도달했어요. 잠시 뒤 다시 시도해 주세요.".into(),
        403 => {
            let body = json_body::<Value>(response).unwrap_or_default();
            let message = body["message"]
                .as_str()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if message.contains("refused requests from this address") {
                "Archyl이 이 접속 주소의 요청을 제한하고 있어요. 잠시 뒤 다시 시도해 주세요.".into()
            } else {
                "Archyl에서 요청을 허용하지 않았어요. 계정 권한을 확인해 주세요.".into()
            }
        }
        400 => {
            let body = json_body::<Value>(response).unwrap_or_default();
            match body["error"].as_str() {
                Some("invalid_scope") => {
                    "Archyl에서 요청한 권한을 허용하지 않았어요. 조회 전용으로 다시 연결해 주세요."
                        .into()
                }
                Some("invalid_redirect_uri") => {
                    "Archyl에서 데스크톱 로그인 응답 주소를 허용하지 않았어요.".into()
                }
                _ => fallback.into(),
            }
        }
        _ => fallback.into(),
    }
}

#[cfg(target_os = "macos")]
fn credential() -> impl crate::github::SecretStore {
    crate::github::KeychainStore::new("co.kcd.aisurada.archyl", "desktop-oauth-v2")
}

#[cfg(target_os = "macos")]
fn load() -> Result<Option<Session>> {
    use crate::github::SecretStore;
    credential()
        .get()?
        .map(|text| serde_json::from_str(&text).map_err(|_| RECONNECT.into()))
        .transpose()
}

#[cfg(target_os = "macos")]
fn save(session: &Session) -> Result<()> {
    use crate::github::SecretStore;
    credential().set(&serde_json::to_string(session).map_err(|_| RESPONSE)?)
}

#[cfg(target_os = "macos")]
fn remove() -> Result<()> {
    use crate::github::SecretStore;
    credential().delete()
}

#[cfg(not(target_os = "macos"))]
fn load() -> Result<Option<Session>> {
    Ok(None)
}
#[cfg(not(target_os = "macos"))]
fn save(_: &Session) -> Result<()> {
    Err("이 기기에서는 Archyl 연결을 저장할 수 없어요.".into())
}
#[cfg(not(target_os = "macos"))]
fn remove() -> Result<()> {
    Ok(())
}

#[derive(Serialize)]
pub struct Connection {
    writable: bool,
    projects: Vec<model::Project>,
}

fn connection(client: &mut mcp::Mcp<'_>, session: &Session) -> Result<Connection> {
    let projects = model::projects(&client.call("list_projects", json!({}))?)?;
    let writable = session.writable()
        && client.tools()?.iter().any(|tool| {
            matches!(
                tool["name"].as_str(),
                Some(
                    "create_system"
                        | "update_system"
                        | "delete_system"
                        | "create_relationship"
                        | "update_relationship"
                        | "delete_relationship"
                )
            )
        });
    Ok(Connection { writable, projects })
}

impl ArchylState {
    fn with_client<T>(
        &self,
        action: impl FnOnce(&mut mcp::Mcp<'_>, &Session) -> Result<T>,
    ) -> Result<T> {
        let _lock = self.0.network.lock().map_err(|_| NETWORK)?;
        let agent = agent();
        let mut session = load()?.ok_or("설정에서 Archyl에 로그인해 주세요.")?;
        if session.expiring() {
            session.refresh(&agent)?;
            save(&session)?;
        }
        let mut client = match mcp::Mcp::open(&agent, &session.access_token) {
            Err(mcp::Error::Unauthorized) => {
                session.refresh(&agent)?;
                save(&session)?;
                mcp::Mcp::open(&agent, &session.access_token)?
            }
            opened => opened?,
        };
        action(&mut client, &session)
    }
}

async fn blocking<T: Send + 'static>(
    job: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tauri::async_runtime::spawn_blocking(job)
        .await
        .map_err(|_| NETWORK)?
}

#[tauri::command]
pub async fn archyl_status(state: State<'_, ArchylState>) -> Result<Option<Connection>> {
    let state = state.inner().clone();
    blocking(move || {
        if load()?.is_none() {
            return Ok(None);
        }
        state.with_client(connection).map(Some)
    })
    .await
}

#[tauri::command]
pub async fn archyl_login(
    app: AppHandle,
    state: State<'_, ArchylState>,
    read_only: bool,
) -> Result<Connection> {
    let state = state.inner().clone();
    let attempt = state.0.generation.load(Ordering::SeqCst);
    blocking(move || {
        let login = &state.0.login;
        if login.swap(true, Ordering::SeqCst) {
            return Err("이미 Archyl 로그인을 기다리고 있어요.".into());
        }
        struct Reset<'a>(&'a AtomicBool);
        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let _reset = Reset(login);
        let agent = agent();
        let session = oauth::login(&app, &agent, read_only, &state.0.generation, attempt)?;
        let mut client = mcp::Mcp::open(&agent, &session.access_token)?;
        let connected = connection(&mut client, &session)?;
        let _lock = state.0.network.lock().map_err(|_| NETWORK)?;
        if state.0.generation.load(Ordering::SeqCst) != attempt {
            return Err("Archyl 로그인을 취소했어요.".into());
        }
        save(&session)?;
        Ok(connected)
    })
    .await
}

#[tauri::command]
pub fn archyl_cancel_login(state: State<'_, ArchylState>) {
    state.0.generation.fetch_add(1, Ordering::SeqCst);
}

#[tauri::command]
pub async fn archyl_disconnect(state: State<'_, ArchylState>) -> Result<()> {
    let state = state.inner().clone();
    state.0.generation.fetch_add(1, Ordering::SeqCst);
    blocking(move || {
        let _lock = state.0.network.lock().map_err(|_| NETWORK)?;
        remove()
    })
    .await
}

#[tauri::command]
pub async fn archyl_model(
    state: State<'_, ArchylState>,
    project_id: String,
) -> Result<model::Model> {
    let state = state.inner().clone();
    blocking(move || state.with_client(|client, _| model::fetch(client, &project_id))).await
}

#[cfg(feature = "claude")]
#[tauri::command]
pub async fn archyl_chat(
    state: State<'_, ArchylState>,
    project_id: String,
    selected_id: Option<String>,
    request: String,
    history: Vec<change::Message>,
) -> Result<change::Reply> {
    let state = state.inner().clone();
    blocking(move || {
        change::run(
            &state,
            &project_id,
            selected_id.as_deref(),
            request.trim(),
            &history,
        )
    })
    .await
}
