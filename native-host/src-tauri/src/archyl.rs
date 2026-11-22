//! Archyl 연결과 MCP 읽기·쓰기.
#[cfg(target_os = "macos")]
use crate::github::{KeychainStore, SecretStore};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ring::{
    digest,
    rand::{SecureRandom, SystemRandom},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{AppHandle, State, Url};
use tauri_plugin_opener::OpenerExt;

const ISSUER: &str = "https://api.archyl.com";
const MCP_ENDPOINT: &str = "https://api.archyl.com/mcp";
const SCOPE: &str = "mcp:read mcp:write";
#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE: &str = "co.kcd.aisurada.archyl";
const OAUTH_ACCOUNT: &str = "oauth";
const API_KEY_ACCOUNT: &str = "api-key";
const RECONNECT: &str = "설정에서 Archyl을 다시 연결해 주세요.";
const NETWORK: &str = "Archyl에 연결하지 못했어요. 인터넷 연결을 확인하고 다시 시도해 주세요.";
const RESPONSE: &str = "Archyl 응답을 읽지 못했어요.";
const API_KEY_REJECTED: &str =
    "Archyl에서 API 키를 거절했어요. 키가 맞는지 또는 만료됐는지 확인해 주세요.";
const ADDRESS_LIMIT: &str =
    "Archyl이 이 접속 주소의 요청을 임시로 제한하고 있어요. 잠시 뒤 다시 시도해 주세요.";
const RATE_LIMIT: &str = "Archyl 요청 한도에 도달했어요. 잠시 뒤 다시 시도해 주세요.";

type Result<T> = std::result::Result<T, String>;

#[derive(Default, Clone)]
pub struct ArchylState {
    lock: Arc<Mutex<()>>,
}

#[derive(Deserialize, Serialize)]
struct Client {
    client_id: String,
    client_secret: String,
}

#[derive(Deserialize, Serialize)]
struct Tokens {
    access_token: String,
    refresh_token: Option<String>,
    scope: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct Session {
    client: Client,
    tokens: Tokens,
}

#[derive(Deserialize, Serialize)]
struct SavedApiKey {
    key: String,
    active: bool,
}

#[cfg(target_os = "macos")]
fn load_credential<T: serde::de::DeserializeOwned>(account: &str) -> Result<Option<T>> {
    KeychainStore::new(KEYCHAIN_SERVICE, account)
        .get()?
        .map(|saved| serde_json::from_str(&saved).map_err(|_| RECONNECT.into()))
        .transpose()
}

#[cfg(target_os = "macos")]
fn save_credential<T: Serialize>(account: &str, credential: &T) -> Result<()> {
    let value = serde_json::to_string(credential).map_err(|_| RESPONSE.to_string())?;
    KeychainStore::new(KEYCHAIN_SERVICE, account).set(&value)
}

#[cfg(target_os = "macos")]
fn delete_credential(account: &str) -> Result<()> {
    KeychainStore::new(KEYCHAIN_SERVICE, account).delete()
}

#[cfg(not(target_os = "macos"))]
fn load_credential<T: serde::de::DeserializeOwned>(_: &str) -> Result<Option<T>> {
    Ok(None)
}
#[cfg(not(target_os = "macos"))]
fn save_credential<T: Serialize>(_: &str, _: &T) -> Result<()> {
    Err("Archyl 연결은 macOS에서 쓸 수 있어요.".into())
}
#[cfg(not(target_os = "macos"))]
fn delete_credential(_: &str) -> Result<()> {
    Ok(())
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .into()
}

fn json_body<T: serde::de::DeserializeOwned>(
    mut response: ureq::http::Response<ureq::Body>,
) -> Result<T> {
    let body = response
        .body_mut()
        .with_config()
        .limit(8 * 1024 * 1024)
        .read_to_vec()
        .map_err(|_| RESPONSE.to_string())?;
    serde_json::from_slice(&body).map_err(|_| RESPONSE.into())
}

fn rejected_request(
    response: ureq::http::Response<ureq::Body>,
    fallback: &'static str,
) -> &'static str {
    match response.status().as_u16() {
        429 => RATE_LIMIT,
        403 => {
            let body = json_body::<Value>(response).unwrap_or_default();
            let message = body["message"]
                .as_str()
                .or_else(|| body["error"]["message"].as_str());
            if message.is_some_and(|message| {
                message
                    .to_ascii_lowercase()
                    .starts_with("too many refused requests from this address")
            }) {
                ADDRESS_LIMIT
            } else {
                fallback
            }
        }
        _ => fallback,
    }
}

fn random_urlsafe() -> Result<String> {
    let mut bytes = [0u8; 32];
    SystemRandom::new()
        .fill(&mut bytes)
        .map_err(|_| "로그인 요청을 만들지 못했어요.")?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn callback_code(listener: TcpListener, redirect: &str, expected: &str) -> Result<String> {
    listener
        .set_nonblocking(true)
        .map_err(|_| NETWORK.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(180);
    while Instant::now() < deadline {
        let (mut socket, _) = match listener.accept() {
            Ok(connection) => connection,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
            Err(_) => return Err(NETWORK.into()),
        };
        let timeout = Some(Duration::from_secs(2));
        let _ = socket.set_read_timeout(timeout);
        let _ = socket.set_write_timeout(timeout);
        let mut line = String::new();
        let _ = BufReader::new((&mut socket).take(16 * 1024)).read_line(&mut line);
        let url = line
            .strip_prefix("GET ")
            .and_then(|line| line.split_ascii_whitespace().next())
            .filter(|path| path.starts_with("/callback?"))
            .and_then(|path| Url::parse(redirect).ok()?.join(path).ok());
        let params: std::collections::HashMap<_, _> =
            url.iter().flat_map(Url::query_pairs).collect();
        let expected_state = params.get("state").is_some_and(|state| state == expected);
        let code = params.get("code").filter(|code| !code.is_empty());
        let (status, body) = if expected_state && code.is_some() {
            (
                "200 OK",
                "Archyl 인증 응답을 받았어요. 이 창을 닫고 Aisurada로 돌아가세요.",
            )
        } else {
            ("400 Bad Request", "Archyl 로그인 요청을 확인할 수 없어요.")
        };
        let _ = write!(socket, "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}", body.len());
        if expected_state {
            return code
                .map(ToString::to_string)
                .ok_or_else(|| "Archyl 연결이 취소됐어요.".into());
        }
    }
    Err("Archyl 로그인 시간이 지났어요. 다시 시도해 주세요.".into())
}

fn token_request(agent: &ureq::Agent, client: &Client, grant: &[(&str, &str)]) -> Result<Tokens> {
    let response = agent
        .post(format!("{ISSUER}/oauth/token"))
        .send_form(
            [
                ("client_id", client.client_id.as_str()),
                ("client_secret", client.client_secret.as_str()),
            ]
            .into_iter()
            .chain(grant.iter().copied()),
        )
        .map_err(|_| NETWORK.to_string())?;
    if !response.status().is_success() {
        let fallback = if response.status().is_client_error() {
            RECONNECT
        } else {
            NETWORK
        };
        return Err(rejected_request(response, fallback).into());
    }
    let tokens: Tokens = json_body(response)?;
    if tokens.access_token.is_empty()
        || tokens.scope.as_deref().is_some_and(|scope| {
            let granted: std::collections::HashSet<_> = scope.split_whitespace().collect();
            !granted.contains("mcp:read") || !granted.contains("mcp:write")
        })
    {
        return Err("Archyl에서 읽기 및 쓰기 권한을 확인하지 못했어요.".into());
    }
    Ok(tokens)
}

fn login(app: &AppHandle, agent: &ureq::Agent) -> Result<Session> {
    // 지역 포트와 PKCE를 사용해 이 로그인 요청의 응답만 받는다.
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|_| NETWORK.to_string())?;
    let port = listener
        .local_addr()
        .map_err(|_| NETWORK.to_string())?
        .port();
    let redirect = format!("http://127.0.0.1:{port}/callback");
    let response = agent
        .post(format!("{ISSUER}/register"))
        .content_type("application/json")
        .send(
            json!({
                "client_name": "Aisurada",
                "redirect_uris": [&redirect],
                "grant_types": ["authorization_code", "refresh_token"],
                "response_types": ["code"],
                "token_endpoint_auth_method": "client_secret_post",
                "scope": SCOPE,
            })
            .to_string(),
        )
        .map_err(|_| NETWORK.to_string())?;
    if !response.status().is_success() {
        return Err(
            rejected_request(response, "Archyl에서 로그인 요청을 등록하지 못했어요.").into(),
        );
    }
    let client: Client = json_body(response)?;
    let verifier = random_urlsafe()?;
    let challenge = URL_SAFE_NO_PAD.encode(digest::digest(&digest::SHA256, verifier.as_bytes()));
    let state = random_urlsafe()?;
    let mut authorize =
        Url::parse(&format!("{ISSUER}/oauth/authorize")).map_err(|_| RESPONSE.to_string())?;
    authorize.query_pairs_mut().extend_pairs([
        ("response_type", "code"),
        ("client_id", &client.client_id),
        ("redirect_uri", &redirect),
        ("scope", SCOPE),
        ("state", &state),
        ("resource", ISSUER),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
    ]);
    app.opener()
        .open_url(authorize.as_str(), None::<&str>)
        .map_err(|_| "기본 브라우저를 열지 못했어요.".to_string())?;
    let code = callback_code(listener, &redirect, &state)?;
    let tokens = token_request(
        agent,
        &client,
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", &redirect),
            ("code_verifier", &verifier),
            ("resource", ISSUER),
        ],
    )?;
    Ok(Session { client, tokens })
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadTool {
    ListProjects,
    GetProject,
    GetProjectC4Model,
}

impl ReadTool {
    fn name(&self) -> &'static str {
        match self {
            Self::ListProjects => "list_projects",
            Self::GetProject => "get_project",
            Self::GetProjectC4Model => "get_project_c4_model",
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WriteTool {
    CreateSystem,
    CreateContainer,
    CreateComponent,
    CreateRelationship,
    UpdatePositions,
    UpdateSystem,
    DeleteSystem,
    DeleteRelationship,
}

impl WriteTool {
    fn name(&self) -> &'static str {
        match self {
            Self::CreateSystem => "create_system",
            Self::CreateContainer => "create_container",
            Self::CreateComponent => "create_component",
            Self::CreateRelationship => "create_relationship",
            Self::UpdatePositions => "update_positions",
            Self::UpdateSystem => "update_system",
            Self::DeleteSystem => "delete_system",
            Self::DeleteRelationship => "delete_relationship",
        }
    }
}

enum McpError {
    Unauthorized,
    Other(String),
}

#[derive(Clone, Copy)]
enum RequestAuth<'a> {
    Oauth(&'a str),
    ApiKey(&'a str),
}

fn mcp_result(response: &mut ureq::http::Response<ureq::Body>, id: u64) -> Result<Value> {
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok());
    let message: Value = if content_type.is_some_and(|value| value.starts_with("text/event-stream"))
    {
        let mut reader = BufReader::new(
            response
                .body_mut()
                .with_config()
                .limit(8 * 1024 * 1024)
                .reader(),
        );
        let mut line = String::new();
        let mut data = String::new();
        loop {
            line.clear();
            if reader
                .read_line(&mut line)
                .map_err(|_| RESPONSE.to_string())?
                == 0
            {
                return Err(RESPONSE.into());
            }
            let line = line.trim_end_matches(['\r', '\n']);
            if let Some(value) = line.strip_prefix("data:") {
                data.push_str(value.strip_prefix(' ').unwrap_or(value));
                data.push('\n');
            } else if line.is_empty() && !data.is_empty() {
                let event =
                    serde_json::from_str::<Value>(&data).map_err(|_| RESPONSE.to_string())?;
                if event["id"].as_u64() == Some(id) {
                    break event;
                }
                data.clear();
            }
        }
    } else {
        let bytes = response
            .body_mut()
            .with_config()
            .limit(8 * 1024 * 1024)
            .read_to_vec()
            .map_err(|_| RESPONSE.to_string())?;
        serde_json::from_slice(&bytes).map_err(|_| RESPONSE.to_string())?
    };
    let id_matches = message["id"].as_u64() == Some(id)
        || message["id"].as_str().and_then(|s| s.parse::<u64>().ok()) == Some(id);
    if !id_matches || !message["error"].is_null() {
        return Err(RESPONSE.into());
    }
    message
        .get("result")
        .cloned()
        .ok_or_else(|| RESPONSE.into())
}

fn tool_call(
    agent: &ureq::Agent,
    auth: RequestAuth<'_>,
    tool_name: &str,
    args: &Value,
    endpoint: &str,
) -> std::result::Result<Value, McpError> {
    let (header, value) = match auth {
        RequestAuth::Oauth(token) => ("Authorization", format!("Bearer {token}")),
        RequestAuth::ApiKey(key) => ("X-API-Key", key.to_owned()),
    };
    let mut value =
        ureq::http::HeaderValue::from_str(&value).map_err(|_| McpError::Other(RESPONSE.into()))?;
    value.set_sensitive(true);

    let mut session_id = None;
    let mut version = "2025-03-26".to_string();
    let send = |message: Value, session: Option<&str>, version: &str| {
        let mut request = agent
            .post(endpoint)
            .header(header, value.clone())
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", version)
            .content_type("application/json");
        if let Some(id) = session {
            request = request.header("Mcp-Session-Id", id);
        }
        let response = request
            .send(message.to_string())
            .map_err(|_| McpError::Other(NETWORK.into()))?;
        match response.status().as_u16() {
            401 => Err(McpError::Unauthorized),
            403 | 429 => Err(McpError::Other(
                rejected_request(response, "이 Archyl 프로젝트를 읽거나 쓸 권한이 없어요.").into(),
            )),
            status if (200..300).contains(&status) => Ok(response),
            _ => Err(McpError::Other(RESPONSE.into())),
        }
    };
    let mut response = send(
        json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": { "protocolVersion": version, "clientInfo": { "name": "aisurada", "version": env!("CARGO_PKG_VERSION") }, "capabilities": {} }
        }),
        None,
        &version,
    )?;
    if let Some(id) = response
        .headers()
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok())
    {
        session_id = Some(id.to_string());
    }
    let hello = mcp_result(&mut response, 1).map_err(McpError::Other)?;
    if let Some(negotiated) = hello["protocolVersion"].as_str() {
        version = negotiated.to_string();
    }
    send(
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        session_id.as_deref(),
        &version,
    )?;
    let mut response = send(
        json!({
            "jsonrpc":"2.0", "id":2, "method":"tools/call", "params":{"name":tool_name,"arguments":args}
        }),
        session_id.as_deref(),
        &version,
    )?;
    let result = mcp_result(&mut response, 2).map_err(McpError::Other)?;
    if result["isError"].as_bool() == Some(true) {
        let err_msg = result["content"]
            .as_array()
            .and_then(|items| {
                items
                    .iter()
                    .find(|item| item["type"] == "text")
                    .and_then(|item| item["text"].as_str())
            })
            .unwrap_or("Archyl에서 요청한 작업을 처리하지 못했어요.");
        let err_msg = match auth {
            RequestAuth::ApiKey(key) => err_msg.replace(key, "[가림]"),
            RequestAuth::Oauth(_) => err_msg.to_owned(),
        };
        return Err(McpError::Other(err_msg));
    }
    if let Some(value) = result
        .get("structuredContent")
        .filter(|value| !value.is_null())
    {
        return Ok(value.clone());
    }
    let content = result["content"].as_array();
    if tool_name == "list_projects" && content.map_or(false, |items| items.is_empty()) {
        return Ok(json!([]));
    }
    let text = content.and_then(|items| {
        items
            .iter()
            .find(|item| item["type"] == "text")
            .and_then(|item| item["text"].as_str())
    });
    let Some(text) = text else {
        if tool_name == "list_projects" && matches!(auth, RequestAuth::Oauth(_)) {
            return Ok(json!([]));
        }
        return Err(McpError::Other(RESPONSE.into()));
    };
    if tool_name == "list_projects"
        && (text.trim().is_empty() || text.trim() == "No projects found in this organization.")
    {
        return Ok(json!([]));
    }
    match serde_json::from_str(text) {
        Ok(parsed) => Ok(parsed),
        Err(_) if tool_name == "list_projects" && matches!(auth, RequestAuth::Oauth(_)) => {
            Ok(json!([]))
        }
        Err(_) => Ok(json!({ "raw": text })),
    }
}

fn tool_error(error: McpError) -> String {
    match error {
        McpError::Unauthorized => RECONNECT.into(),
        McpError::Other(message) => message,
    }
}

fn execute_tool_with_session(
    agent: &ureq::Agent,
    session: &mut Session,
    tool_name: &str,
    args: &Value,
) -> Result<Value> {
    match tool_call(
        agent,
        RequestAuth::Oauth(&session.tokens.access_token),
        tool_name,
        args,
        MCP_ENDPOINT,
    ) {
        Err(McpError::Unauthorized) => {
            let refresh = session
                .tokens
                .refresh_token
                .as_deref()
                .ok_or_else(|| RECONNECT.to_string())?;
            let mut tokens = token_request(
                agent,
                &session.client,
                &[
                    ("grant_type", "refresh_token"),
                    ("refresh_token", refresh),
                    ("resource", ISSUER),
                    ("scope", SCOPE),
                ],
            )?;
            if tokens.refresh_token.is_none() {
                tokens.refresh_token = session.tokens.refresh_token.clone();
            }
            session.tokens = tokens;
            save_credential(OAUTH_ACCOUNT, session)?;
            tool_call(
                agent,
                RequestAuth::Oauth(&session.tokens.access_token),
                tool_name,
                args,
                MCP_ENDPOINT,
            )
            .map_err(tool_error)
        }
        result => result.map_err(tool_error),
    }
}

fn execute_with_api_key(
    agent: &ureq::Agent,
    key: &str,
    tool: &str,
    args: &Value,
    endpoint: &str,
) -> Result<Value> {
    tool_call(agent, RequestAuth::ApiKey(key), tool, args, endpoint).map_err(|error| match error {
        McpError::Unauthorized => API_KEY_REJECTED.into(),
        McpError::Other(message) => message,
    })
}

fn verify_api_key(agent: &ureq::Agent, key: &str, endpoint: &str) -> Result<()> {
    if key.is_empty() || !key.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err("Archyl API 키를 한 줄로 입력해 주세요.".into());
    }
    let value = execute_with_api_key(
        agent,
        key,
        ReadTool::ListProjects.name(),
        &json!({}),
        endpoint,
    )?;
    let projects = value
        .get("data")
        .filter(|data| !data.is_null())
        .unwrap_or(&value);
    if projects.is_array()
        || projects
            .get("projects")
            .filter(|value| !value.is_null())
            .or_else(|| projects.get("items"))
            .is_some_and(Value::is_array)
    {
        return Ok(());
    }
    Err("Archyl에서 프로젝트 목록을 확인하지 못했어요.".into())
}

fn execute_configured_tool(tool: &str, args: &Value) -> Result<Value> {
    let agent = agent();
    if let Some(api_key) = load_credential::<SavedApiKey>(API_KEY_ACCOUNT)?.filter(|key| key.active)
    {
        return execute_with_api_key(&agent, &api_key.key, tool, args, MCP_ENDPOINT);
    }
    let mut session = load_credential(OAUTH_ACCOUNT)?
        .ok_or_else(|| "설정에서 Archyl을 연결해 주세요.".to_string())?;
    execute_tool_with_session(&agent, &mut session, tool, args)
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum ConnectionMethod {
    ApiKey,
    Oauth,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatus {
    method: Option<ConnectionMethod>,
    saved_api_key: bool,
    saved_oauth: bool,
}

#[derive(Deserialize)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum ConnectionRequest {
    ApiKey { key: Option<String> },
    Oauth { reuse: bool },
}

#[tauri::command]
pub fn archyl_status() -> Result<ConnectionStatus> {
    let api_key = load_credential::<SavedApiKey>(API_KEY_ACCOUNT)?;
    let api_key_active = api_key.as_ref().is_some_and(|key| key.active);
    let oauth = load_credential::<Session>(OAUTH_ACCOUNT);
    let (method, saved_oauth) = if api_key_active {
        (
            Some(ConnectionMethod::ApiKey),
            oauth.is_ok_and(|session| session.is_some()),
        )
    } else {
        let saved = oauth?.is_some();
        (saved.then_some(ConnectionMethod::Oauth), saved)
    };
    Ok(ConnectionStatus {
        method,
        saved_api_key: api_key.is_some(),
        saved_oauth,
    })
}

#[tauri::command]
pub async fn archyl_connect(
    app: AppHandle,
    state: State<'_, ArchylState>,
    connection: ConnectionRequest,
) -> Result<()> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = state
            .lock
            .try_lock()
            .map_err(|_| "다른 Archyl 요청이 진행 중이에요.")?;
        let agent = agent();
        match connection {
            ConnectionRequest::ApiKey { key } => {
                let key = match key {
                    Some(value) => value.trim().to_owned(),
                    None => {
                        load_credential::<SavedApiKey>(API_KEY_ACCOUNT)?
                            .ok_or("저장한 Archyl API 키가 없어요.")?
                            .key
                    }
                };
                verify_api_key(&agent, &key, MCP_ENDPOINT)?;
                save_credential(API_KEY_ACCOUNT, &SavedApiKey { key, active: true })
            }
            ConnectionRequest::Oauth { reuse } => {
                let api_key = load_credential::<SavedApiKey>(API_KEY_ACCOUNT)?;
                if reuse {
                    let mut session = load_credential(OAUTH_ACCOUNT)?.ok_or(RECONNECT)?;
                    execute_tool_with_session(
                        &agent,
                        &mut session,
                        ReadTool::ListProjects.name(),
                        &json!({}),
                    )?;
                } else {
                    let session = login(&app, &agent)?;
                    save_credential(OAUTH_ACCOUNT, &session)?;
                }
                if let Some(mut api_key) = api_key.filter(|key| key.active) {
                    api_key.active = false;
                    save_credential(API_KEY_ACCOUNT, &api_key)?;
                }
                Ok(())
            }
        }
    })
    .await
    .map_err(|_| NETWORK.to_string())?
}

#[tauri::command]
pub async fn archyl_disconnect(state: State<'_, ArchylState>) -> Result<()> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = state
            .lock
            .try_lock()
            .map_err(|_| "다른 Archyl 요청이 진행 중이에요.")?;
        let api_key = load_credential::<SavedApiKey>(API_KEY_ACCOUNT)?;
        if api_key.is_some_and(|key| key.active) {
            delete_credential(OAUTH_ACCOUNT)?;
            delete_credential(API_KEY_ACCOUNT)
        } else {
            delete_credential(API_KEY_ACCOUNT)?;
            delete_credential(OAUTH_ACCOUNT)
        }
    })
    .await
    .map_err(|_| NETWORK.to_string())?
}

#[tauri::command]
pub async fn archyl_read(
    state: State<'_, ArchylState>,
    tool: ReadTool,
    project_id: Option<String>,
) -> Result<Value> {
    let args = match (tool, project_id) {
        (ReadTool::ListProjects, None) => json!({}),
        (ReadTool::GetProject | ReadTool::GetProjectC4Model, Some(id)) if valid_id(&id) => {
            json!({"projectId": id})
        }
        _ => return Err("Archyl 프로젝트를 확인할 수 없어요.".into()),
    };
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = state.lock.lock().map_err(|_| NETWORK.to_string())?;
        execute_configured_tool(tool.name(), &args)
    })
    .await
    .map_err(|_| NETWORK.to_string())?
}

pub(crate) fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[tauri::command]
pub async fn archyl_write(
    state: State<'_, ArchylState>,
    tool: WriteTool,
    args: Value,
) -> Result<Value> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = state.lock.lock().map_err(|_| NETWORK.to_string())?;
        execute_configured_tool(tool.name(), &args)
    })
    .await
    .map_err(|_| NETWORK.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream;

    #[test]
    fn callback_rejects_unrelated_state() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            callback_code(listener, &format!("http://{addr}/callback"), "current")
        });
        for (state, code, status) in [("other", "wrong", "400"), ("current", "right", "200")] {
            let mut browser = TcpStream::connect(addr).unwrap();
            browser
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            write!(
                browser,
                "GET /callback?code={code}&state={state} HTTP/1.1\r\nHost: {addr}\r\n\r\n"
            )
            .unwrap();
            let mut line = String::new();
            BufReader::new(browser).read_line(&mut line).unwrap();
            assert!(line.starts_with(&format!("HTTP/1.1 {status} ")));
        }
        assert_eq!(worker.join().unwrap().unwrap(), "right");
    }

    #[test]
    fn mcp_sends_session_and_reads_streamed_tool_result() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            for step in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut reader = BufReader::new(&mut stream);
                let mut headers = Vec::new();
                loop {
                    let mut line = String::new();
                    assert_ne!(reader.read_line(&mut line).unwrap(), 0);
                    if line == "\r\n" {
                        break;
                    }
                    headers.push(line.trim().to_ascii_lowercase());
                }
                assert!(headers.contains(&"authorization: bearer fixture-token".to_string()));
                if step > 0 {
                    assert!(headers.contains(&"mcp-session-id: fixture-session".to_string()));
                    assert!(headers.contains(&"mcp-protocol-version: 2025-11-25".to_string()));
                }
                let size = headers
                    .iter()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap();
                let mut body = vec![0; size];
                reader.read_exact(&mut body).unwrap();
                let request: Value = serde_json::from_slice(&body).unwrap();
                let (status, extra, response) = match step {
                    0 => {
                        assert_eq!(request["method"], "initialize");
                        (
                            "200 OK",
                            "Content-Type: application/json\r\nMcp-Session-Id: fixture-session\r\n",
                            r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25"}}"#
                                .to_string(),
                        )
                    }
                    1 => {
                        assert_eq!(request["method"], "notifications/initialized");
                        ("202 Accepted", "", String::new())
                    }
                    _ => {
                        assert_eq!(request["params"]["name"], "get_project_c4_model");
                        assert_eq!(
                            request["params"]["arguments"],
                            json!({"projectId":"project1"})
                        );
                        let event = json!({"jsonrpc":"2.0","id":2,"result":{"content":[
                            {"type":"text","text":r#"{"systems":[],"relationships":[]}"#}
                        ]}});
                        (
                            "200 OK",
                            "Content-Type: text/event-stream\r\n",
                            format!("event: message\r\ndata: {event}\r\n\r\n"),
                        )
                    }
                };
                write!(stream, "HTTP/1.1 {status}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).unwrap();
            }
        });
        let model = tool_call(
            &agent(),
            RequestAuth::Oauth("fixture-token"),
            ReadTool::GetProjectC4Model.name(),
            &json!({"projectId":"project1"}),
            &endpoint,
        )
        .map_err(tool_error)
        .unwrap();
        assert_eq!(model, json!({"systems":[],"relationships":[]}));
        worker.join().unwrap();
    }

    #[test]
    fn list_projects_handles_empty_or_non_json_text_gracefully() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            for step in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut reader = BufReader::new(&mut stream);
                loop {
                    let mut line = String::new();
                    assert_ne!(reader.read_line(&mut line).unwrap(), 0);
                    if line == "\r\n" {
                        break;
                    }
                }
                let (status, extra, response) = match step {
                    0 => (
                        "200 OK",
                        "Content-Type: application/json\r\n",
                        r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-06-18"}}"#
                            .to_string(),
                    ),
                    1 => ("202 Accepted", "", String::new()),
                    _ => {
                        let event = json!({"jsonrpc":"2.0","id":2,"result":{"content":[
                            {"type":"text","text":"No projects found in this organization."}
                        ]}});
                        (
                            "200 OK",
                            "Content-Type: application/json\r\n",
                            event.to_string(),
                        )
                    }
                };
                write!(stream, "HTTP/1.1 {status}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).unwrap();
            }
        });
        let projects = tool_call(
            &agent(),
            RequestAuth::Oauth("fixture-token"),
            ReadTool::ListProjects.name(),
            &json!({}),
            &endpoint,
        )
        .map_err(tool_error)
        .unwrap();
        assert_eq!(projects, json!([]));
        worker.join().unwrap();
    }

    const FIXTURE_API_KEY: &str = "fixture-archyl-key";

    fn api_key_fixture(
        responses: Vec<(&'static str, Value)>,
    ) -> (String, std::thread::JoinHandle<Vec<Value>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, response) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut reader = BufReader::new(&mut stream);
                let mut key_present = false;
                let mut size = 0;
                loop {
                    let mut line = String::new();
                    assert_ne!(reader.read_line(&mut line).unwrap(), 0);
                    let line = line.trim_end().to_ascii_lowercase();
                    if line.is_empty() {
                        break;
                    }
                    if let Some(key) = line.strip_prefix("x-api-key: ") {
                        assert_eq!(key, FIXTURE_API_KEY);
                        key_present = true;
                    }
                    assert!(!line.starts_with("authorization:"));
                    if let Some(length) = line.strip_prefix("content-length: ") {
                        size = length.parse().unwrap();
                    }
                }
                assert!(key_present);
                let mut body = vec![0; size];
                reader.read_exact(&mut body).unwrap();
                requests.push(serde_json::from_slice(&body).unwrap());
                let body = if response.is_null() {
                    String::new()
                } else {
                    response.to_string()
                };
                write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
            requests
        });
        (endpoint, worker)
    }

    fn mcp_fixture(result: Value) -> Vec<(&'static str, Value)> {
        vec![
            (
                "200 OK",
                json!({"jsonrpc":"2.0", "id":1, "result":{"protocolVersion":"2025-03-26"}}),
            ),
            ("202 Accepted", Value::Null),
            ("200 OK", json!({"jsonrpc":"2.0", "id":2, "result": result})),
        ]
    }

    #[test]
    fn api_key_verification_requires_a_project_listing() {
        let (endpoint, worker) =
            api_key_fixture(mcp_fixture(json!({"structuredContent":{"projects":[]}})));
        verify_api_key(&agent(), FIXTURE_API_KEY, &endpoint).unwrap();
        let requests = worker.join().unwrap();
        assert_eq!(requests[2]["params"]["name"], "list_projects");

        let invalid = json!({"content":[{"type":"text","text":"Unknown response"}]});
        let (endpoint, worker) = api_key_fixture(mcp_fixture(invalid));
        assert!(verify_api_key(&agent(), FIXTURE_API_KEY, &endpoint).is_err());
        worker.join().unwrap();
    }

    #[test]
    fn api_key_also_authenticates_write_tools() {
        let (endpoint, worker) =
            api_key_fixture(mcp_fixture(json!({"structuredContent":{"id":"system-1"}})));
        let result = execute_with_api_key(
            &agent(),
            FIXTURE_API_KEY,
            WriteTool::CreateSystem.name(),
            &json!({"projectId":"project-1", "name":"fixture"}),
            &endpoint,
        )
        .unwrap();
        assert_eq!(result["id"], "system-1");
        assert_eq!(worker.join().unwrap()[2]["params"]["name"], "create_system");
    }

    #[test]
    fn api_key_distinguishes_auth_failure_from_address_limit() {
        let errors = [
            (
                "401 Unauthorized",
                json!({"message":"Invalid API key"}),
                API_KEY_REJECTED,
            ),
            (
                "403 Forbidden",
                json!({"message":"Too many refused requests from this address, try again later"}),
                ADDRESS_LIMIT,
            ),
        ];
        for (status, body, expected) in errors {
            let (endpoint, worker) = api_key_fixture(vec![(status, body)]);
            assert_eq!(
                verify_api_key(&agent(), FIXTURE_API_KEY, &endpoint),
                Err(expected.into())
            );
            assert_eq!(worker.join().unwrap().len(), 1);
        }
    }
}
