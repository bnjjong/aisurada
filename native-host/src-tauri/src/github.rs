//! GitHub App 연결(설치 토큰 방식).
//!
//! 사용자가 만든 GitHub App의 App ID와 개인 키(PEM)로 JWT를 서명해 설치 토큰을 받는다. 개인 키는 키체인에 두고
//! 화면(웹뷰)에는 절대 보내지 않는다. App ID 같은 비밀이 아닌 값은 앱 지원 폴더의 `github-app.json`에 둔다.
//! GitHub와 말하는 것은 이 모듈뿐이다. 엔진과 에이전트(MCP)는 토큰을 볼 수 없다.

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use ring::rand::SystemRandom;
use ring::signature::{RsaKeyPair, RSA_PKCS1_SHA256};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::State;

pub const DEFAULT_API: &str = "https://api.github.com";
const DEFAULT_GIT_BASE: &str = "https://github.com";
const KEYCHAIN_SERVICE: &str = "co.kcd.aisurada.github-app";
const KEYCHAIN_ACCOUNT: &str = "private-key";
const SETTINGS_FILE: &str = "github-app.json";
/// 설치 토큰은 1시간짜리다. 만료 5분 전부터는 새로 받는다.
const TOKEN_MARGIN_SECS: u64 = 300;
const MAX_PAGES: u32 = 20;
const CLONE_TIMEOUT: Duration = Duration::from_secs(600);

// ---------------------------------------------------------------------------
// 시각
// ---------------------------------------------------------------------------

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// `2026-11-12T04:17:41Z` 꼴을 유닉스 초로 바꾼다. 다른 꼴이면 None이다. (그레고리력 날짜 → 일수 변환)
fn parse_utc(text: &str) -> Option<u64> {
    let b = text.as_bytes();
    if b.len() != 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' || b[19] != b'Z' {
        return None;
    }
    let num = |from: usize, to: usize| text.get(from..to)?.parse::<i64>().ok();
    let (y, m, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hh, mm, ss) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let secs = days * 86_400 + hh * 3600 + mm * 60 + ss;
    u64::try_from(secs).ok()
}

// ---------------------------------------------------------------------------
// JWT (RS256)
// ---------------------------------------------------------------------------

/// PEM 글에서 DER 바이트를 꺼낸다. PKCS#1(`RSA PRIVATE KEY`)과 PKCS#8(`PRIVATE KEY`)을 받는다.
fn pem_to_key_pair(pem: &str) -> Result<RsaKeyPair, String> {
    let (label, pkcs8) = if pem.contains("-----BEGIN RSA PRIVATE KEY-----") {
        ("RSA PRIVATE KEY", false)
    } else if pem.contains("-----BEGIN PRIVATE KEY-----") {
        ("PRIVATE KEY", true)
    } else {
        return Err("개인 키(PEM) 모양이 아니다. GitHub App 설정에서 받은 .pem 파일이어야 한다".into());
    };
    let begin = format!("-----BEGIN {label}-----");
    let end = format!("-----END {label}-----");
    let start = pem.find(&begin).ok_or("PEM 시작 줄이 없다")? + begin.len();
    let stop = pem[start..].find(&end).ok_or("PEM 끝 줄이 없다")? + start;
    let compact: String = pem[start..stop].chars().filter(|c| !c.is_whitespace()).collect();
    let der = STANDARD.decode(compact).map_err(|_| "PEM 본문을 읽지 못했다")?;
    let pair = if pkcs8 { RsaKeyPair::from_pkcs8(&der) } else { RsaKeyPair::from_der(&der) };
    pair.map_err(|_| "RSA 개인 키로 읽지 못했다. 키가 깨졌거나 지원하지 않는 모양이다".into())
}

/// GitHub가 받는 앱 JWT. 만든 시각을 60초 앞당기고(시계 어긋남 대비) 10분 안에 만료한다.
fn app_jwt(app_id: &str, pem: &str, now: u64) -> Result<String, String> {
    let key = pem_to_key_pair(pem)?;
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
    let payload = URL_SAFE_NO_PAD.encode(
        serde_json::json!({ "iat": now.saturating_sub(60), "exp": now + 540, "iss": app_id }).to_string(),
    );
    let signing_input = format!("{header}.{payload}");
    let mut signature = vec![0u8; key.public().modulus_len()];
    key.sign(&RSA_PKCS1_SHA256, &SystemRandom::new(), signing_input.as_bytes(), &mut signature)
        .map_err(|_| "JWT 서명에 실패했다")?;
    Ok(format!("{signing_input}.{}", URL_SAFE_NO_PAD.encode(signature)))
}

// ---------------------------------------------------------------------------
// GitHub REST
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq)]
pub enum GithubError {
    /// 연결하지 못했다(네트워크, TLS, 시간 초과)
    Network(String),
    /// GitHub가 거절했다
    Status { status: u16, message: String },
    /// 응답을 읽지 못했다
    Parse(String),
}

impl std::fmt::Display for GithubError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GithubError::Network(m) => write!(f, "GitHub에 연결하지 못했다: {m}"),
            GithubError::Status { status: 401, .. } => {
                write!(f, "GitHub가 인증을 거절했다(401). 개인 키가 이 앱의 것이 아니거나 이 컴퓨터의 시계가 어긋났다")
            }
            GithubError::Status { status: 404, message } => write!(f, "GitHub에서 찾지 못했다(404): {message}"),
            GithubError::Status { status, message } => write!(f, "GitHub가 거절했다({status}): {message}"),
            GithubError::Parse(m) => write!(f, "GitHub 응답을 읽지 못했다: {m}"),
        }
    }
}

/// 화면에 보내는 저장소 한 개
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RepoInfo {
    pub full_name: String,
    pub id: u64,
    pub private: bool,
    pub default_branch: String,
    pub html_url: String,
    pub installation_id: u64,
}

#[derive(Clone, Debug, PartialEq)]
struct Installation {
    id: u64,
    account: String,
}

struct Github {
    api: String,
    agent: ureq::Agent,
}

impl Github {
    fn new(api: &str) -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .http_status_as_error(false)
            .user_agent("aisurada")
            .build();
        Github { api: api.trim_end_matches('/').to_string(), agent: config.into() }
    }

    fn request(&self, method: &str, path: &str, bearer: &str) -> Result<Value, GithubError> {
        let url = format!("{}{}", self.api, path);
        let bearer = format!("Bearer {bearer}");
        let result = match method {
            "POST" => self
                .agent
                .post(&url)
                .header("Authorization", &bearer)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .send("{}"),
            _ => self
                .agent
                .get(&url)
                .header("Authorization", &bearer)
                .header("Accept", "application/vnd.github+json")
                .header("X-GitHub-Api-Version", "2022-11-28")
                .call(),
        };
        let mut response = result.map_err(|e| GithubError::Network(e.to_string()))?;
        let status = response.status().as_u16();
        let text = response.body_mut().read_to_string().map_err(|e| GithubError::Network(e.to_string()))?;
        if !(200..300).contains(&status) {
            let message = serde_json::from_str::<Value>(&text)
                .ok()
                .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(str::to_string))
                .unwrap_or_default();
            return Err(GithubError::Status { status, message });
        }
        serde_json::from_str(&text).map_err(|e| GithubError::Parse(e.to_string()))
    }

    /// `GET /app` — 앱 JWT가 맞는지와 앱 이름을 본다.
    fn app(&self, jwt: &str) -> Result<(String, String, String), GithubError> {
        let v = self.request("GET", "/app", jwt)?;
        let text = |key: &str| v.get(key).map(|x| x.as_str().map(str::to_string).unwrap_or_else(|| x.to_string())).unwrap_or_default();
        Ok((text("id"), text("slug"), text("name")))
    }

    /// `GET /app/installations` — 이 앱이 설치된 계정 목록. 쪽마다 100개씩 읽는다.
    fn installations(&self, jwt: &str) -> Result<Vec<Installation>, GithubError> {
        let mut all = Vec::new();
        for page in 1..=MAX_PAGES {
            let v = self.request("GET", &format!("/app/installations?per_page=100&page={page}"), jwt)?;
            let items = v.as_array().ok_or_else(|| GithubError::Parse("설치 목록이 배열이 아니다".into()))?;
            for item in items {
                let id = item.get("id").and_then(Value::as_u64).ok_or_else(|| GithubError::Parse("설치 id가 없다".into()))?;
                let account = item.pointer("/account/login").and_then(Value::as_str).unwrap_or("").to_string();
                all.push(Installation { id, account });
            }
            if items.len() < 100 {
                break;
            }
        }
        Ok(all)
    }

    /// `POST /app/installations/{id}/access_tokens` — 설치 토큰과 만료 시각(유닉스 초).
    fn installation_token(&self, jwt: &str, installation_id: u64) -> Result<(String, u64), GithubError> {
        let v = self.request("POST", &format!("/app/installations/{installation_id}/access_tokens"), jwt)?;
        let token = v.get("token").and_then(Value::as_str).ok_or_else(|| GithubError::Parse("token이 없다".into()))?.to_string();
        let expires = v.get("expires_at").and_then(Value::as_str).and_then(parse_utc).unwrap_or_else(|| now_secs() + 3000);
        Ok((token, expires))
    }

    /// `GET /installation/repositories` — 설치 토큰으로 볼 수 있는 저장소. 쪽마다 100개씩 읽는다.
    fn installation_repositories(&self, token: &str, installation_id: u64) -> Result<Vec<RepoInfo>, GithubError> {
        let mut all = Vec::new();
        for page in 1..=MAX_PAGES {
            let v = self.request("GET", &format!("/installation/repositories?per_page=100&page={page}"), token)?;
            let items = v.get("repositories").and_then(Value::as_array).ok_or_else(|| GithubError::Parse("repositories가 없다".into()))?;
            for item in items {
                all.push(RepoInfo {
                    full_name: item.get("full_name").and_then(Value::as_str).unwrap_or("").to_string(),
                    id: item.get("id").and_then(Value::as_u64).unwrap_or(0),
                    private: item.get("private").and_then(Value::as_bool).unwrap_or(false),
                    default_branch: item.get("default_branch").and_then(Value::as_str).unwrap_or("").to_string(),
                    html_url: item.get("html_url").and_then(Value::as_str).unwrap_or("").to_string(),
                    installation_id,
                });
            }
            if items.len() < 100 {
                break;
            }
        }
        Ok(all)
    }

    /// `GET /repos/{owner}/{repo}/installation` — 그 저장소에 설치된 설치 id.
    fn repo_installation(&self, jwt: &str, owner: &str, name: &str) -> Result<u64, GithubError> {
        let v = self.request("GET", &format!("/repos/{owner}/{name}/installation"), jwt)?;
        v.get("id").and_then(Value::as_u64).ok_or_else(|| GithubError::Parse("설치 id가 없다".into()))
    }
}

// ---------------------------------------------------------------------------
// 비밀 보관
// ---------------------------------------------------------------------------

/// 개인 키(PEM) 하나를 보관하는 곳. 실제로는 키체인이고, 시험은 메모리를 쓴다.
pub trait SecretStore: Send + Sync {
    fn get(&self) -> Result<Option<String>, String>;
    fn set(&self, value: &str) -> Result<(), String>;
    fn delete(&self) -> Result<(), String>;
}

#[cfg(target_os = "macos")]
pub struct KeychainStore {
    service: String,
    account: String,
}

#[cfg(target_os = "macos")]
impl KeychainStore {
    /// 앱이 쓰는 키체인 항목(서비스 `co.kcd.aisurada.github-app`, 계정 `private-key`)
    pub fn app() -> Self {
        KeychainStore { service: KEYCHAIN_SERVICE.into(), account: KEYCHAIN_ACCOUNT.into() }
    }
}

#[cfg(target_os = "macos")]
impl SecretStore for KeychainStore {
    fn get(&self) -> Result<Option<String>, String> {
        use security_framework::passwords::get_generic_password;
        match get_generic_password(&self.service, &self.account) {
            Ok(bytes) => String::from_utf8(bytes).map(Some).map_err(|_| "키체인의 값이 글이 아니다".to_string()),
            // errSecItemNotFound = -25300: 아직 저장한 적이 없다
            Err(e) if e.code() == -25300 => Ok(None),
            Err(e) => Err(format!("키체인을 읽지 못했다: {e}")),
        }
    }

    fn set(&self, value: &str) -> Result<(), String> {
        security_framework::passwords::set_generic_password(&self.service, &self.account, value.as_bytes())
            .map_err(|e| format!("키체인에 저장하지 못했다: {e}"))
    }

    fn delete(&self) -> Result<(), String> {
        match security_framework::passwords::delete_generic_password(&self.service, &self.account) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == -25300 => Ok(()),
            Err(e) => Err(format!("키체인에서 지우지 못했다: {e}")),
        }
    }
}

// ---------------------------------------------------------------------------
// 서비스
// ---------------------------------------------------------------------------

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GithubStatus {
    pub connected: bool,
    pub app_id: Option<String>,
    pub app_name: Option<String>,
    pub app_slug: Option<String>,
    /// GitHub 전용 저장소를 받는 폴더. 화면이 "받으면 여기에 생긴다"는 경로를 미리 알아 세션 붙이기에 쓴다.
    pub clone_root: String,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RepoList {
    pub repositories: Vec<RepoInfo>,
    /// 설치 하나가 실패해도 나머지는 보여 준다. 실패한 설치마다 사람이 읽을 글 한 줄.
    pub errors: Vec<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CloneResult {
    pub path: String,
    /// 이번에 새로 받았으면 true, 이미 있어서 그대로 썼으면 false
    pub cloned: bool,
}

pub struct GithubService {
    dir: PathBuf,
    store: Box<dyn SecretStore>,
    api: String,
    git_base: String,
    /// 설치 id → (토큰, 만료 유닉스 초). 메모리에만 둔다.
    tokens: Mutex<HashMap<u64, (String, u64)>>,
}

impl GithubService {
    /// [dir]은 앱 지원 폴더(`~/Library/Application Support/aisurada`). 시험은 임시 폴더를 준다.
    pub fn new(dir: PathBuf, store: Box<dyn SecretStore>, api: &str, git_base: &str) -> Self {
        GithubService { dir, store, api: api.to_string(), git_base: git_base.trim_end_matches('/').to_string(), tokens: Mutex::new(HashMap::new()) }
    }

    /// 저장소를 받는 폴더(`<앱 지원 폴더>/repos`)
    pub fn clone_root(&self) -> PathBuf {
        self.dir.join("repos")
    }

    fn settings_path(&self) -> PathBuf {
        self.dir.join(SETTINGS_FILE)
    }

    fn read_settings(&self) -> Option<Value> {
        serde_json::from_str(&std::fs::read_to_string(self.settings_path()).ok()?).ok()
    }

    pub fn status(&self) -> GithubStatus {
        let settings = self.read_settings();
        let text = |key: &str| settings.as_ref().and_then(|s| s.get(key)).and_then(Value::as_str).map(str::to_string);
        let has_key = matches!(self.store.get(), Ok(Some(_)));
        let app_id = text("appId");
        GithubStatus {
            connected: has_key && app_id.is_some(),
            app_id,
            app_name: text("appName"),
            app_slug: text("appSlug"),
            clone_root: self.clone_root().to_string_lossy().into_owned(),
        }
    }

    /// 앱을 연결한다. 키로 JWT를 서명해 `GET /app`을 불러 App ID가 맞는지 보고, 맞을 때만 키체인과 설정 파일에 저장한다.
    pub fn connect(&self, app_id: &str, pem: &str) -> Result<GithubStatus, String> {
        let app_id = app_id.trim();
        if app_id.is_empty() || !app_id.chars().all(|c| c.is_ascii_digit()) {
            return Err("App ID는 숫자여야 한다".into());
        }
        let jwt = app_jwt(app_id, pem, now_secs())?;
        let github = Github::new(&self.api);
        let (id, slug, name) = github.app(&jwt).map_err(|e| e.to_string())?;
        if id != app_id {
            return Err(format!("App ID가 키와 맞지 않는다: GitHub는 이 키의 앱을 {id}번이라고 답했다"));
        }
        self.store.set(pem)?;
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("설정 폴더를 만들지 못했다: {e}"))?;
        let json = serde_json::json!({ "appId": app_id, "appSlug": slug, "appName": name });
        std::fs::write(self.settings_path(), format!("{json}\n")).map_err(|e| format!("설정을 쓰지 못했다: {e}"))?;
        self.tokens.lock().map_err(|_| "토큰 잠금을 얻지 못했다")?.clear();
        Ok(self.status())
    }

    /// 연결을 끊는다. 키체인의 키와 설정 파일과 메모리의 토큰을 모두 지운다. 없어도 성공이다.
    pub fn disconnect(&self) -> Result<(), String> {
        self.store.delete()?;
        match std::fs::remove_file(self.settings_path()) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("설정을 지우지 못했다: {e}")),
        }
        self.tokens.lock().map_err(|_| "토큰 잠금을 얻지 못했다")?.clear();
        Ok(())
    }

    fn credentials(&self) -> Result<(String, String), String> {
        let app_id = self
            .read_settings()
            .and_then(|s| s.get("appId").and_then(Value::as_str).map(str::to_string))
            .ok_or("GitHub App이 연결돼 있지 않다. 설정에서 App ID와 개인 키를 먼저 연결한다")?;
        let pem = self.store.get()?.ok_or("키체인에 개인 키가 없다. 설정에서 다시 연결한다")?;
        Ok((app_id, pem))
    }

    fn token_for(&self, github: &Github, jwt: &str, installation_id: u64) -> Result<String, GithubError> {
        let now = now_secs();
        if let Some((token, expires)) = self.tokens.lock().ok().and_then(|t| t.get(&installation_id).cloned()) {
            if expires > now + TOKEN_MARGIN_SECS {
                return Ok(token);
            }
        }
        let (token, expires) = github.installation_token(jwt, installation_id)?;
        if let Ok(mut cache) = self.tokens.lock() {
            cache.insert(installation_id, (token.clone(), expires));
        }
        Ok(token)
    }

    /// 앱이 설치된 모든 계정의 저장소를 모은다. 설치 하나가 실패해도 나머지는 돌려주고 실패는 `errors`에 적는다.
    pub fn repositories(&self) -> Result<RepoList, String> {
        let (app_id, pem) = self.credentials()?;
        let jwt = app_jwt(&app_id, &pem, now_secs())?;
        let github = Github::new(&self.api);
        let installations = github.installations(&jwt).map_err(|e| e.to_string())?;
        let mut repositories = Vec::new();
        let mut errors = Vec::new();
        for installation in installations {
            let outcome = self
                .token_for(&github, &jwt, installation.id)
                .and_then(|token| github.installation_repositories(&token, installation.id));
            match outcome {
                Ok(mut repos) => repositories.append(&mut repos),
                Err(e) => errors.push(format!("{}: {e}", installation.account)),
            }
        }
        repositories.sort_by(|a, b| a.full_name.to_lowercase().cmp(&b.full_name.to_lowercase()));
        Ok(RepoList { repositories, errors })
    }

    /// 저장소를 [root]/<소유자>/<이름>에 받는다. 이미 있으면(폴더에 `.git`이 있으면) 그대로 쓴다.
    pub fn ensure_clone(&self, full_name: &str, root: &Path) -> Result<CloneResult, String> {
        let (owner, name) = split_full_name(full_name)?;
        let dest = root.join(&owner).join(&name);
        if dest.join(".git").exists() {
            return Ok(CloneResult { path: dest.to_string_lossy().into_owned(), cloned: false });
        }
        if dest.exists() && dest.read_dir().map(|mut d| d.next().is_some()).unwrap_or(true) {
            return Err(format!("받을 자리에 이미 다른 폴더가 있다: {}", dest.display()));
        }
        let token = self.repo_token(&owner, &name)?;
        std::fs::create_dir_all(dest.parent().ok_or("받을 자리를 정하지 못했다")?).map_err(|e| format!("폴더를 만들지 못했다: {e}"))?;
        let url = format!("{}/{}/{}.git", self.git_base, owner, name);
        run_git_clone(&url, &dest, &token)?;
        Ok(CloneResult { path: dest.to_string_lossy().into_owned(), cloned: true })
    }

    #[cfg(feature = "claude")]
    /// 받아 둔 저장소를 GitHub의 최신으로 맞춘다(`git fetch --prune origin`). 아직 없으면 먼저 받는다.
    /// 러너가 읽기 전용 작업 폴더를 만들기 직전에 불러서, 태스크가 낡은 코드를 읽지 않게 한다.
    pub fn refresh_clone(&self, full_name: &str, root: &Path) -> Result<PathBuf, String> {
        let cloned = self.ensure_clone(full_name, root)?;
        let dest = PathBuf::from(&cloned.path);
        if !cloned.cloned {
            let (owner, name) = split_full_name(full_name)?;
            let token = self.repo_token(&owner, &name)?;
            run_git_fetch(&dest, &token)?;
        }
        Ok(dest)
    }

    /// 그 저장소에 설치된 GitHub 앱의 설치 토큰.
    fn repo_token(&self, owner: &str, name: &str) -> Result<String, String> {
        let (app_id, pem) = self.credentials()?;
        let jwt = app_jwt(&app_id, &pem, now_secs())?;
        let github = Github::new(&self.api);
        let installation = github.repo_installation(&jwt, owner, name).map_err(|e| e.to_string())?;
        self.token_for(&github, &jwt, installation).map_err(|e| e.to_string())
    }
}

/// `owner/name`을 나눈다. 폴더 이름으로 쓰므로 GitHub가 허용하는 글자만 받는다(`..`, 빗금, 앞의 점은 거절).
fn split_full_name(full_name: &str) -> Result<(String, String), String> {
    let bad = || format!("저장소 이름이 owner/name 모양이 아니다: {full_name}");
    let (owner, name) = full_name.split_once('/').ok_or_else(bad)?;
    let owner_ok = !owner.is_empty()
        && owner.len() <= 39
        && !owner.starts_with('-')
        && owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    let name_ok = !name.is_empty()
        && name.len() <= 100
        && name != "."
        && name != ".."
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
    if owner_ok && name_ok {
        Ok((owner.to_string(), name.to_string()))
    } else {
        Err(bad())
    }
}

// ---------------------------------------------------------------------------
// git clone
// ---------------------------------------------------------------------------

pub(crate) fn git_binary() -> PathBuf {
    if let Some(custom) = std::env::var_os("AISURADA_GIT_BIN") {
        return PathBuf::from(custom);
    }
    for candidate in ["/opt/homebrew/bin/git", "/usr/local/bin/git", "/usr/bin/git"] {
        if Path::new(candidate).is_file() {
            return PathBuf::from(candidate);
        }
    }
    PathBuf::from("git")
}

/// git에 줄 환경. 토큰은 명령줄(`ps`에 보임)이 아니라 환경변수로만 싣고, 키체인 자격 증명 도우미가 토큰을 저장하지 못하게 비운다.
fn clone_env(token: &str) -> Vec<(String, String)> {
    let basic = STANDARD.encode(format!("x-access-token:{token}"));
    vec![
        ("GIT_TERMINAL_PROMPT".into(), "0".into()),
        ("GIT_CONFIG_COUNT".into(), "2".into()),
        ("GIT_CONFIG_KEY_0".into(), "credential.helper".into()),
        ("GIT_CONFIG_VALUE_0".into(), String::new()),
        ("GIT_CONFIG_KEY_1".into(), "http.extraheader".into()),
        ("GIT_CONFIG_VALUE_1".into(), format!("Authorization: Basic {basic}")),
    ]
}

fn run_git_clone(url: &str, dest: &Path, token: &str) -> Result<(), String> {
    let mut command = Command::new(git_binary());
    command.args(["clone", "--quiet", url]).arg(dest);
    // 실패하면 반쯤 받은 폴더를 남기지 않는다
    run_git_with_token(command, token, Some(dest), "저장소를 받는 데 시간이 너무 오래 걸려 멈췄다", "저장소를 받지 못했다")
}

#[cfg(feature = "claude")]
fn run_git_fetch(repo: &Path, token: &str) -> Result<(), String> {
    let mut command = Command::new(git_binary());
    command.arg("-C").arg(repo).args(["fetch", "--quiet", "--prune", "origin"]);
    run_git_with_token(command, token, None, "저장소를 최신으로 맞추는 데 시간이 너무 오래 걸려 멈췄다", "저장소를 최신으로 맞추지 못했다")
}

/// git 명령 하나를 토큰 환경으로 돌리고 상한 시간을 지킨다. 토큰은 오류 글에서 가린다. [cleanup]은 실패했을 때 지울 폴더다.
fn run_git_with_token(mut command: Command, token: &str, cleanup: Option<&Path>, timeout_message: &str, failure_message: &str) -> Result<(), String> {
    command.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    for (key, value) in clone_env(token) {
        command.env(key, value);
    }
    let mut child = command.spawn().map_err(|e| format!("git을 실행하지 못했다: {e}"))?;
    let mut stderr = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(s) = stderr.as_mut() {
            let _ = s.read_to_string(&mut text);
        }
        text
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait().map_err(|e| format!("git 상태를 읽지 못했다: {e}"))? {
            Some(status) => break status,
            None if started.elapsed() > CLONE_TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                if let Some(dir) = cleanup {
                    let _ = std::fs::remove_dir_all(dir);
                }
                return Err(timeout_message.into());
            }
            None => std::thread::sleep(Duration::from_millis(100)),
        }
    };
    let message = reader.join().unwrap_or_default().replace(token, "***");
    if status.success() {
        Ok(())
    } else {
        if let Some(dir) = cleanup {
            let _ = std::fs::remove_dir_all(dir);
        }
        Err(format!("{failure_message}: {}", message.trim()))
    }
}

// ---------------------------------------------------------------------------
// 화면이 부르는 명령
// ---------------------------------------------------------------------------

/// 앱이 쓰는 서비스 하나. 실제 키체인과 GitHub, 앱 지원 폴더를 쓴다.
pub struct GithubState(Arc<GithubService>);

impl GithubState {
    pub fn new() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        let dir = home.join("Library/Application Support/aisurada");
        #[cfg(target_os = "macos")]
        let store: Box<dyn SecretStore> = Box::new(KeychainStore::app());
        #[cfg(not(target_os = "macos"))]
        let store: Box<dyn SecretStore> = Box::new(UnsupportedStore);
        GithubState(Arc::new(GithubService::new(dir, store, DEFAULT_API, DEFAULT_GIT_BASE)))
    }

    fn clone_root(&self) -> PathBuf {
        self.0.clone_root()
    }

    #[cfg(feature = "claude")]
    /// 러너처럼 화면 밖에서 저장소를 받아야 하는 곳이 같은 서비스(같은 키체인, 같은 토큰 저장)를 쓰게 한다.
    pub fn service(&self) -> Arc<GithubService> {
        self.0.clone()
    }
}

impl Default for GithubState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(target_os = "macos"))]
struct UnsupportedStore;

#[cfg(not(target_os = "macos"))]
impl SecretStore for UnsupportedStore {
    fn get(&self) -> Result<Option<String>, String> {
        Ok(None)
    }
    fn set(&self, _value: &str) -> Result<(), String> {
        Err("개인 키 보관은 macOS 키체인에서만 지원한다".into())
    }
    fn delete(&self) -> Result<(), String> {
        Ok(())
    }
}

async fn blocking<T: Send + 'static>(work: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work).await.map_err(|e| format!("작업이 중간에 끊겼다: {e}"))?
}

#[tauri::command]
pub async fn github_status(state: State<'_, GithubState>) -> Result<GithubStatus, String> {
    let service = state.0.clone();
    blocking(move || Ok(service.status())).await
}

/// 개인 키는 파일 경로로만 받는다. 키 내용은 이 함수 안에서만 읽고 화면으로 돌려보내지 않는다.
#[tauri::command]
pub async fn github_connect(state: State<'_, GithubState>, app_id: String, pem_path: String) -> Result<GithubStatus, String> {
    let service = state.0.clone();
    blocking(move || {
        let pem = std::fs::read_to_string(&pem_path).map_err(|e| format!("개인 키 파일을 읽지 못했다: {e}"))?;
        service.connect(&app_id, &pem)
    })
    .await
}

#[tauri::command]
pub async fn github_disconnect(state: State<'_, GithubState>) -> Result<(), String> {
    let service = state.0.clone();
    blocking(move || service.disconnect()).await
}

#[tauri::command]
pub async fn github_repositories(state: State<'_, GithubState>) -> Result<RepoList, String> {
    let service = state.0.clone();
    blocking(move || service.repositories()).await
}

/// 앱 전용 폴더(`~/Library/Application Support/aisurada/repos/<소유자>/<이름>`)에 받는다. 이미 있으면 그대로 쓴다.
#[tauri::command]
pub async fn github_clone(state: State<'_, GithubState>, full_name: String) -> Result<CloneResult, String> {
    let service = state.0.clone();
    let root = state.clone_root();
    blocking(move || service.ensure_clone(&full_name, &root)).await
}

// ---------------------------------------------------------------------------
// 시험
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{UnparsedPublicKey, RSA_PKCS1_2048_8192_SHA256};
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    /// 시험 전용으로 만든 일회용 RSA 키다. 어떤 GitHub 앱의 것도 아니다.
    const TEST_KEY: &str = "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA5FkXll7jUgPME1ikbXDr3HpY5aX8p63bpQJQWYN0EuagZx8N\nYuIqhAqDBtD59lahF1rcfkWEqXvVbdQ4KU2yaHVwyr6aONFIPKzJ4BRSAAH35uMZ\nvO7ZT70OPhUS2LbE03zPD4a4OQWjUBvFvAlbPs1rMlcejUdX4lo3lAsJq2k2eYcq\ntN+MvIXtSdPMwFimZnolosGgpx5mRgLbx6WXEIneEdEpKSrbmX6/9US48LkQoBE7\nOvi2lRNi0zY38ZVFVylUfXnKMlEEFMOGltp0mxCU46i30K+mN8/7ah41zooT8Z9s\ncLQmOcNxmynuPRpnxAxxV5AoQzLW8mbU3qYJdQIDAQABAoIBAQCeeJ4AA3YrMutr\n+baTJSzq5BXRunIvRF8MkyCEHtBtVZS4PDqkB2WOIWVxU5JFvTUq6infkAtTO9eQ\n0xjuwTcxe199tO1+xO8m/JtVYYTO8I7Es5LnJbO2vPOnbiSYNa8/PnzMv6gWGq2O\nKNf/eVbTrOfaFc/kKED7m7S6jzZ0t83mThW+S/bhwXGTkaC8AiK+h9WcE1+uTVVQ\nf+ejvR+qNXJC/FIVGGQGYGaNBCp+M1keP/1wE8CD0xCmnZMmlJ3HF42YUOt/UzAB\n/9b/7tDPu4Iy0IgNWlTuzmQ15s1V6ydMlnoBqF8cAec19foOJfrgr+RM+RqYhgif\nJhB+Dz95AoGBAPyoKi6tBGEdHCLaZfmN2Hemcr91zynixWIWxO+mSvGvwnWh8mZV\n58XoL0iVUOign8iatDsV1E2NC2qczKpcLT8xxvHjNfp512l0U12GDI3gN6o08dVA\nlcI/XFogjNkzOQVTH8WbrflK700mOmmNevqS4uyZGuur1QuXNnC+E6ZHAoGBAOde\nlbtVWjOG3uc21/Hlz8/TXZJ8Ax1dNvzssZbeyTTSMGDy0mfndG4UcF2qqZNDVX/7\nlfa48Uyb/KRkqrjpZcsGXP01sK5L7eIl9eySGwUqj96AGB6tT7iIqQRnxsUruKOW\nTf4qK+TifC7b9HAHQoa9/EUxNLleQUvCrDBGn2RjAoGARyW30zMHk61V2f60TqUB\n+xeVIpMzRjFEsg2FYZLrccmzsdpr3yN7xABRKFhb01ZACIaAEE9iXIBGYbxL6gFR\nAIWs5zq7XZBn95s5SpHQATfNlRGZtBqe0Xd0MzENWLCS8wvoW4YuZJAUeR41vrmH\nkv5ABq5KpWja8oufWfo8oEsCgYB0GJT/hEuXsWRmFrHy8oL9YK0CVPHnKR/veAGA\nl9fUhme6Lf4AHzbgCad8iKdAergLWT2dqJ2D+H2kK9MyqALpBg9hCzDVtMnwAWZO\nR94BEKNSXWo88nAfQ4uz5zLpOxASHdiS4axW1ILFrJ8SG8J5BBnyykcPgMk4770h\nGD7gywKBgAe7g1ZUdz83500DkxzbFUxPqz8JsRmSauXTy0AH6JGlVmxQ7RhXOpds\nwSyNPg2UBw0US+C7UsXX6VxbqgEQP8y4ttSndV9bB2wuWhbGODubDP//YjTv07Yq\nUuJaqtygJ2GMG2hlDkvgBbg19IG8XTi0wt1S59QtotoLxsyOWtCg\n-----END RSA PRIVATE KEY-----";

    struct MemoryStore(Mutex<Option<String>>);
    impl MemoryStore {
        fn new() -> Self {
            MemoryStore(Mutex::new(None))
        }
    }
    impl SecretStore for MemoryStore {
        fn get(&self) -> Result<Option<String>, String> {
            Ok(self.0.lock().unwrap().clone())
        }
        fn set(&self, value: &str) -> Result<(), String> {
            *self.0.lock().unwrap() = Some(value.to_string());
            Ok(())
        }
        fn delete(&self) -> Result<(), String> {
            *self.0.lock().unwrap() = None;
            Ok(())
        }
    }

    /// 가짜 GitHub. 경로마다 정해 둔 JSON을 돌려주고, 받은 요청을 기록한다.
    struct FakeGithub {
        base: String,
        requests: Arc<Mutex<Vec<(String, String, String)>>>, // (메서드, 경로, Authorization)
    }

    fn fake_github(routes: Vec<(&'static str, &'static str, u16, String)>) -> FakeGithub {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let log = requests.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut first = String::new();
                if reader.read_line(&mut first).is_err() {
                    continue;
                }
                let mut parts = first.split_whitespace();
                let (method, path) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("").to_string());
                let mut auth = String::new();
                let mut length = 0usize;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    let lower = line.to_lowercase();
                    if let Some(v) = lower.strip_prefix("authorization:") {
                        auth = line[line.len() - v.len()..].trim().to_string();
                    }
                    if let Some(v) = lower.strip_prefix("content-length:") {
                        length = v.trim().parse().unwrap_or(0);
                    }
                }
                let mut body = vec![0u8; length];
                let _ = reader.read_exact(&mut body);
                log.lock().unwrap().push((method.clone(), path.clone(), auth));
                let (status, text) = routes
                    .iter()
                    .find(|(m, p, _, _)| *m == method && path.starts_with(p))
                    .map(|(_, _, s, t)| (*s, t.clone()))
                    .unwrap_or((404, r#"{"message":"Not Found"}"#.to_string()));
                let reply = format!(
                    "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                    text.len()
                );
                let _ = stream.write_all(reply.as_bytes());
            }
        });
        FakeGithub { base, requests }
    }

    fn service(fake: &FakeGithub, dir: &Path, git_base: &str) -> GithubService {
        GithubService::new(dir.to_path_buf(), Box::new(MemoryStore::new()), &fake.base, git_base)
    }

    fn repos_json(names: &[(&str, u64)]) -> String {
        let items: Vec<String> = names
            .iter()
            .map(|(n, id)| format!(r#"{{"full_name":"{n}","id":{id},"private":true,"default_branch":"main","html_url":"https://github.com/{n}"}}"#))
            .collect();
        format!(r#"{{"total_count":{},"repositories":[{}]}}"#, items.len(), items.join(","))
    }

    #[test]
    fn utc_times_become_unix_seconds() {
        assert_eq!(parse_utc("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_utc("2026-11-12T04:17:41Z"), Some(1_794_457_061));
        assert_eq!(parse_utc("2000-02-29T23:59:59Z"), Some(951_868_799));
        assert_eq!(parse_utc("2026-11-12 04:17:41"), None);
        assert_eq!(parse_utc("2026-13-12T04:17:41Z"), None);
        assert_eq!(parse_utc(""), None);
    }

    #[test]
    fn the_app_jwt_is_a_valid_rs256_token_github_accepts() {
        let jwt = app_jwt("5722327", TEST_KEY, 1_700_000_000).unwrap();
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3);
        let header: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap();
        assert_eq!(header, serde_json::json!({"alg":"RS256","typ":"JWT"}));
        let payload: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
        assert_eq!(payload["iss"], "5722327");
        assert_eq!(payload["iat"], 1_700_000_000u64 - 60, "시계가 어긋나도 되게 60초 앞당긴다");
        assert_eq!(payload["exp"], 1_700_000_000u64 + 540, "10분을 넘기지 않는다");
        // 서명은 같은 키의 공개 키로 검증된다
        let key = pem_to_key_pair(TEST_KEY).unwrap();
        let public = UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, key.public().as_ref().to_vec());
        let signature = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
        public.verify(format!("{}.{}", parts[0], parts[1]).as_bytes(), &signature).unwrap();
        // 깨지거나 모양이 틀린 키는 알아듣게 거절한다
        assert!(app_jwt("1", "아무 글", 0).unwrap_err().contains("PEM"));
        assert!(app_jwt("1", "-----BEGIN RSA PRIVATE KEY-----\n!!!\n-----END RSA PRIVATE KEY-----", 0).is_err());
    }

    #[test]
    fn connecting_checks_the_app_id_and_keeps_the_key_only_in_the_store() {
        let fake = fake_github(vec![("GET", "/app", 200, r#"{"id":5722327,"slug":"my-app","name":"My App"}"#.into())]);
        let dir = tempfile::tempdir().unwrap();
        let svc = service(&fake, dir.path(), "file:///unused");
        assert!(!svc.status().connected);
        assert!(svc.connect("abc", TEST_KEY).unwrap_err().contains("숫자"));
        let wrong = svc.connect("999", TEST_KEY).unwrap_err();
        assert!(wrong.contains("맞지 않는다") && wrong.contains("5722327"), "{wrong}");
        assert!(!svc.status().connected, "틀린 ID로는 아무것도 저장하지 않는다");
        let status = svc.connect(" 5722327 ", TEST_KEY).unwrap();
        assert_eq!(
            status,
            GithubStatus {
                connected: true,
                app_id: Some("5722327".into()),
                app_name: Some("My App".into()),
                app_slug: Some("my-app".into()),
                clone_root: dir.path().join("repos").to_string_lossy().into_owned(),
            }
        );
        let saved = std::fs::read_to_string(dir.path().join("github-app.json")).unwrap();
        assert!(!saved.contains("PRIVATE KEY") && !saved.contains("MIIE"), "개인 키는 설정 파일에 안 쓴다: {saved}");
        let seen = fake.requests.lock().unwrap().clone();
        assert!(seen.iter().all(|(_, _, auth)| auth.starts_with("Bearer ") && auth.matches('.').count() == 2), "{seen:?}");
        svc.disconnect().unwrap();
        assert!(!svc.status().connected);
        assert!(!dir.path().join("github-app.json").exists());
        svc.disconnect().unwrap();
    }

    #[test]
    fn a_rejected_key_is_reported_and_not_saved() {
        let fake = fake_github(vec![("GET", "/app", 401, r#"{"message":"A JSON web token could not be decoded"}"#.into())]);
        let dir = tempfile::tempdir().unwrap();
        let svc = service(&fake, dir.path(), "file:///unused");
        let error = svc.connect("5722327", TEST_KEY).unwrap_err();
        assert!(error.contains("401") && error.contains("시계"), "{error}");
        assert!(!svc.status().connected);
    }

    #[test]
    fn repositories_are_gathered_across_installations_and_tokens_are_reused() {
        let fake = fake_github(vec![
            ("GET", "/app/installations", 200, r#"[{"id":1,"account":{"login":"alice"}},{"id":2,"account":{"login":"org"}}]"#.into()),
            ("POST", "/app/installations/1/access_tokens", 201, r#"{"token":"ghs_one","expires_at":"2099-01-01T00:00:00Z"}"#.into()),
            ("POST", "/app/installations/2/access_tokens", 403, r#"{"message":"This installation has been suspended"}"#.into()),
            ("GET", "/installation/repositories", 200, repos_json(&[("alice/zeta", 3), ("alice/Alpha", 1)])),
            ("GET", "/app", 200, r#"{"id":7,"slug":"s","name":"n"}"#.into()),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let svc = service(&fake, dir.path(), "file:///unused");
        assert!(svc.repositories().unwrap_err().contains("연결돼 있지 않다"));
        svc.connect("7", TEST_KEY).unwrap();
        let list = svc.repositories().unwrap();
        let names: Vec<&str> = list.repositories.iter().map(|r| r.full_name.as_str()).collect();
        assert_eq!(names, vec!["alice/Alpha", "alice/zeta"], "이름 차례로, 대소문자는 무시");
        assert_eq!(list.repositories[0].installation_id, 1);
        assert!(list.repositories[0].private && list.repositories[0].default_branch == "main");
        assert_eq!(list.errors.len(), 1, "정지된 설치 하나가 전체를 막지 않는다");
        assert!(list.errors[0].contains("org") && list.errors[0].contains("suspended"), "{:?}", list.errors);
        let again = svc.repositories().unwrap();
        assert_eq!(again.repositories.len(), 2);
        let token_requests = fake.requests.lock().unwrap().iter().filter(|(m, p, _)| m == "POST" && p.ends_with("/1/access_tokens")).count();
        assert_eq!(token_requests, 1, "만료 전 토큰은 다시 받지 않는다");
        let bearer_for_repos: Vec<String> = fake.requests.lock().unwrap().iter().filter(|(_, p, _)| p.starts_with("/installation/repositories")).map(|(_, _, a)| a.clone()).collect();
        assert!(bearer_for_repos.iter().all(|a| a == "Bearer ghs_one"), "{bearer_for_repos:?}");
    }

    #[test]
    fn an_expiring_token_is_replaced() {
        let fake = fake_github(vec![
            ("GET", "/app/installations", 200, r#"[{"id":1,"account":{"login":"alice"}}]"#.into()),
            ("POST", "/app/installations/1/access_tokens", 201, r#"{"token":"ghs_fresh","expires_at":"2099-01-01T00:00:00Z"}"#.into()),
            ("GET", "/installation/repositories", 200, repos_json(&[("alice/a", 1)])),
            ("GET", "/app", 200, r#"{"id":7,"slug":"s","name":"n"}"#.into()),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let svc = service(&fake, dir.path(), "file:///unused");
        svc.connect("7", TEST_KEY).unwrap();
        svc.tokens.lock().unwrap().insert(1, ("ghs_old".into(), now_secs() + 100));
        svc.repositories().unwrap();
        let used: Vec<String> = fake.requests.lock().unwrap().iter().filter(|(_, p, _)| p.starts_with("/installation/repositories")).map(|(_, _, a)| a.clone()).collect();
        assert_eq!(used, vec!["Bearer ghs_fresh".to_string()], "만료까지 5분이 안 남았으면 새로 받는다");
    }

    #[test]
    fn full_names_are_checked_before_they_become_folders() {
        assert_eq!(split_full_name("bnjjong/aisurada").unwrap(), ("bnjjong".into(), "aisurada".into()));
        assert_eq!(split_full_name("a-b/x_y.z-1").unwrap(), ("a-b".into(), "x_y.z-1".into()));
        for bad in ["", "noslash", "/x", "x/", "../x", "x/..", "x/.", "a/b/c", "-a/b", "a b/c", "a/b c", "a/é", "a/b;rm"] {
            assert!(split_full_name(bad).is_err(), "거절해야 한다: {bad}");
        }
    }

    #[test]
    fn the_clone_token_goes_in_the_environment_never_the_command_line() {
        let env = clone_env("ghs_secret");
        let get = |k: &str| env.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone()).unwrap();
        assert_eq!(get("GIT_TERMINAL_PROMPT"), "0");
        assert_eq!(get("GIT_CONFIG_KEY_0"), "credential.helper");
        assert_eq!(get("GIT_CONFIG_VALUE_0"), "", "키체인 도우미가 토큰을 저장하지 못하게 비운다");
        assert_eq!(get("GIT_CONFIG_KEY_1"), "http.extraheader");
        let header = get("GIT_CONFIG_VALUE_1");
        let basic = header.strip_prefix("Authorization: Basic ").unwrap();
        assert_eq!(String::from_utf8(STANDARD.decode(basic).unwrap()).unwrap(), "x-access-token:ghs_secret");
    }

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git").args(args).current_dir(dir).env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t").stdout(Stdio::null()).stderr(Stdio::null()).status().unwrap();
        assert!(status.success(), "git {args:?}");
    }

    /// 실제 키체인 왕복. 시험용 이름으로만 쓰고 끝나면 지운다. `cargo test -- --ignored keychain`으로만 돌린다.
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "실제 키체인을 쓴다"]
    fn keychain_round_trip() {
        let store = KeychainStore { service: "co.kcd.aisurada.github-app.test".into(), account: "round-trip".into() };
        store.delete().unwrap();
        assert_eq!(store.get().unwrap(), None, "없는 항목은 오류가 아니라 None");
        store.set("첫 값\n두 줄").unwrap();
        assert_eq!(store.get().unwrap().as_deref(), Some("첫 값\n두 줄"), "줄바꿈이 든 PEM 같은 글도 그대로");
        store.set("바뀐 값").unwrap();
        assert_eq!(store.get().unwrap().as_deref(), Some("바뀐 값"), "다시 저장하면 바뀐다");
        store.delete().unwrap();
        assert_eq!(store.get().unwrap(), None);
        store.delete().unwrap();
    }

    /// 실제 GitHub로 연결, 목록, clone까지. 환경변수 AISURADA_REAL_APP_ID, AISURADA_REAL_PEM_FILE, AISURADA_REAL_REPO가 있을 때만 돈다.
    /// 키는 메모리 저장소에만 두고 clone은 임시 폴더에 받아 지운다. `cargo test -- --ignored real_github`로만 돌린다.
    #[test]
    #[ignore = "실제 GitHub와 실제 자격 증명을 쓴다"]
    fn real_github_smoke() {
        let (Ok(app_id), Ok(pem_file), Ok(repo)) = (
            std::env::var("AISURADA_REAL_APP_ID"),
            std::env::var("AISURADA_REAL_PEM_FILE"),
            std::env::var("AISURADA_REAL_REPO"),
        ) else {
            panic!("AISURADA_REAL_APP_ID, AISURADA_REAL_PEM_FILE, AISURADA_REAL_REPO가 필요하다");
        };
        let pem = std::fs::read_to_string(pem_file).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let svc = GithubService::new(dir.path().to_path_buf(), Box::new(MemoryStore::new()), DEFAULT_API, DEFAULT_GIT_BASE);
        let status = svc.connect(&app_id, &pem).unwrap();
        println!("연결: {status:?}");
        assert!(status.connected);
        let list = svc.repositories().unwrap();
        println!("저장소: {:?} 오류: {:?}", list.repositories.iter().map(|r| (&r.full_name, r.id, r.private)).collect::<Vec<_>>(), list.errors);
        assert!(list.repositories.iter().any(|r| r.full_name == repo), "{repo}가 목록에 있어야 한다");
        let root = dir.path().join("repos");
        let first = svc.ensure_clone(&repo, &root).unwrap();
        println!("clone: {first:?}");
        assert!(first.cloned && Path::new(&first.path).join(".git").exists());
        let config = std::fs::read_to_string(Path::new(&first.path).join(".git/config")).unwrap();
        println!("remote 설정에 토큰 흔적: {}", config.contains("x-access-token") || config.to_lowercase().contains("authorization") || config.contains("ghs_"));
        assert!(!config.contains("ghs_") && !config.to_lowercase().contains("authorization"));
        assert!(!svc.ensure_clone(&repo, &root).unwrap().cloned, "두 번째는 다시 받지 않는다");
    }

    #[test]
    fn a_repository_is_cloned_once_without_leaving_the_token_behind() {
        // 받을 원본: <git_base>/owner/repo.git 자리에 로컬 저장소를 둔다
        let remote = tempfile::tempdir().unwrap();
        let work = remote.path().join("work");
        std::fs::create_dir_all(&work).unwrap();
        git(&work, &["init", "-q", "-b", "main"]);
        std::fs::write(work.join("README.md"), "hello").unwrap();
        git(&work, &["add", "."]);
        git(&work, &["commit", "-q", "-m", "init"]);
        let bare_parent = remote.path().join("owner");
        std::fs::create_dir_all(&bare_parent).unwrap();
        git(remote.path(), &["clone", "-q", "--bare", work.to_str().unwrap(), bare_parent.join("repo.git").to_str().unwrap()]);

        let fake = fake_github(vec![
            ("GET", "/repos/owner/repo/installation", 200, r#"{"id":9}"#.into()),
            ("POST", "/app/installations/9/access_tokens", 201, r#"{"token":"ghs_clone","expires_at":"2099-01-01T00:00:00Z"}"#.into()),
            ("GET", "/app", 200, r#"{"id":7,"slug":"s","name":"n"}"#.into()),
        ]);
        let dir = tempfile::tempdir().unwrap();
        let svc = service(&fake, dir.path(), &format!("file://{}", remote.path().display()));
        let root = dir.path().join("repos");
        assert!(svc.ensure_clone("owner/repo", &root).unwrap_err().contains("연결돼 있지 않다"));
        svc.connect("7", TEST_KEY).unwrap();
        let first = svc.ensure_clone("owner/repo", &root).unwrap();
        assert!(first.cloned);
        assert_eq!(first.path, root.join("owner/repo").to_string_lossy());
        assert_eq!(std::fs::read_to_string(root.join("owner/repo/README.md")).unwrap(), "hello");
        let config = std::fs::read_to_string(root.join("owner/repo/.git/config")).unwrap();
        assert!(!config.contains("ghs_clone") && !config.to_lowercase().contains("authorization") && !config.contains("x-access-token"), "토큰이 저장소 설정에 남으면 안 된다: {config}");
        let second = svc.ensure_clone("owner/repo", &root).unwrap();
        assert!(!second.cloned, "이미 있으면 다시 받지 않는다");
        let installs = fake.requests.lock().unwrap().iter().filter(|(_, p, _)| p.starts_with("/repos/owner/repo/installation")).count();
        assert_eq!(installs, 1, "이미 있으면 GitHub를 부르지도 않는다");
        // 못 받는 저장소는 반쯤 받은 폴더를 남기지 않는다
        let missing = svc.ensure_clone("owner/ghost", &root);
        assert!(missing.is_err());
        assert!(!root.join("owner/ghost").exists());
        // 폴더 이름으로 쓸 수 없는 이름은 GitHub를 부르기 전에 거절한다
        assert!(svc.ensure_clone("../etc/passwd", &root).unwrap_err().contains("owner/name"));
    }
}
