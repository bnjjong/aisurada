//! 기기의 매니저 등록과 프로젝트 범위, 호출 기록을 관리한다.

use std::collections::HashSet;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use ais_core::engine::{ManagerProject, manager_project};
use ais_core::valid_manager_name;
use ais_core::write::{acquire_lock, write_atomic};
use ais_domain::task::format;
use ais_infra::ports::{clock, fs, ids};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{
    Outcome,
    env::{Env, resolved_vault_path},
};

pub const DEFAULT_TOOLS: [&str; 7] = [
    "list_tasks",
    "get_task",
    "search_docs",
    "get_doc",
    "create_task",
    "add_task_comment",
    "create_doc",
];
pub const OPTIONAL_TOOLS: [&str; 7] = [
    "list_docs",
    "list_tags",
    "list_sub_notes",
    "get_sub_note",
    "list_attachments",
    "list_comments",
    "list_decisions",
];
const DEFAULT_STATES: [&str; 4] = ["ready-for-develop", "in-review", "done", "failed"];
pub const NOTIFY_STATES: [&str; 6] = [
    "ready-for-develop",
    "in-review",
    "done",
    "failed",
    "in-progress",
    "waiting-answer",
];

fn default_states() -> Vec<String> {
    DEFAULT_STATES.into_iter().map(str::to_owned).collect()
}

fn default_kind() -> String {
    "hermes".into()
}

fn on() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Manager {
    pub name: String,
    pub label: String,
    pub enabled: bool,
    pub tools: Vec<String>,
    pub repos: Vec<String>,
    pub agendas: Vec<String>,
    pub webhook: Option<String>,
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub vault: String,
    #[serde(default)]
    pub slack_channels: Vec<String>,
    #[serde(default = "default_states")]
    pub notify: Vec<String>,
    #[serde(default = "on")]
    pub notify_enabled: bool,
    #[serde(default)]
    registration: String,
    #[serde(default)]
    legacy_key: bool,
}

impl Manager {
    fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            label: name.into(),
            enabled: true,
            tools: DEFAULT_TOOLS.into_iter().map(str::to_owned).collect(),
            repos: Vec::new(),
            agendas: Vec::new(),
            webhook: None,
            kind: default_kind(),
            project: String::new(),
            project_id: String::new(),
            vault: String::new(),
            slack_channels: Vec::new(),
            notify: default_states(),
            notify_enabled: false,
            registration: registration_id(),
            legacy_key: false,
        }
    }

    pub fn legacy(&self) -> bool {
        self.project.is_empty() && self.project_id.is_empty() && self.vault.is_empty()
    }

    pub fn in_vault(&self, path: &str) -> bool {
        !self.legacy() && fs::canonicalize(path).is_ok_and(|root| root == Path::new(&self.vault))
    }

    pub fn scope(&self) -> Option<ManagerProject> {
        let root = Path::new(&self.vault);
        if fs::canonicalize(root).ok()?.as_path() != root {
            return None;
        }
        manager_project(root, &self.project).filter(|project| project.id == self.project_id)
    }

    pub fn same_registration(&self, other: &Self) -> bool {
        self.registration == other.registration
            && self.vault == other.vault
            && self.project == other.project
            && self.project_id == other.project_id
    }

    pub fn registration(&self) -> &str {
        &self.registration
    }

    pub fn keychain_account(&self) -> String {
        if self.legacy_key {
            self.name.clone()
        } else {
            format!("{}/{}", self.name, self.registration)
        }
    }

    pub fn webhook_for_status(&self, status: &str) -> Option<&str> {
        (self.notify_enabled && self.notify.iter().any(|value| value == status))
            .then_some(self.webhook.as_deref())
            .flatten()
    }

    fn validate(&self) -> Result<(), String> {
        if !valid_manager_name(&self.name) {
            return Err(
                "매니저 이름은 소문자로 시작하는 소문자·숫자·하이픈 1~32자여야 한다".into(),
            );
        }
        if self.label.trim().is_empty()
            || self.label.chars().count() > 40
            || self.label.chars().any(char::is_control)
        {
            return Err("표시 이름은 1~40자여야 한다".into());
        }
        if !matches!(self.kind.as_str(), "hermes" | "mcp") {
            return Err("매니저 종류는 hermes 또는 mcp여야 한다".into());
        }
        if self.tools.is_empty()
            || self.tools.iter().any(|tool| {
                !DEFAULT_TOOLS.contains(&tool.as_str()) && !OPTIONAL_TOOLS.contains(&tool.as_str())
            })
            || self.tools.iter().collect::<HashSet<_>>().len() != self.tools.len()
        {
            return Err("매니저에게 열 수 있는 도구를 중복 없이 하나 이상 고른다".into());
        }
        if self
            .repos
            .iter()
            .any(|repo| !matches!(format::repo(repo), Ok(Some(value)) if &value == repo))
        {
            return Err("저장소는 owner/name 모양이어야 한다".into());
        }
        if self.agendas.iter().any(|agenda| {
            agenda.is_empty() || agenda.trim() != agenda || agenda.chars().any(char::is_control)
        }) {
            return Err("아젠다 식별자가 비어 있거나 잘못됐다".into());
        }
        if self.slack_channels.len() > 20
            || self.slack_channels.iter().any(|channel| {
                channel.len() > 80
                    || channel.chars().any(char::is_whitespace)
                    || channel.chars().any(char::is_control)
            })
        {
            return Err("Slack 채널은 공백 없이 20개까지 적는다".into());
        }
        if self
            .notify
            .iter()
            .any(|status| !NOTIFY_STATES.contains(&status.as_str()))
            || self.notify.iter().collect::<HashSet<_>>().len() != self.notify.len()
        {
            return Err("알릴 상태를 목록에서 중복 없이 고른다".into());
        }
        if !self.legacy()
            && (self.project.is_empty()
                || self.project_id.is_empty()
                || !Path::new(&self.vault).is_absolute()
                || !self.registration.bytes().all(|byte| byte.is_ascii_digit())
                || self.registration.is_empty())
        {
            return Err("매니저 등록부의 프로젝트 정보가 올바르지 않다".into());
        }
        if let Some(webhook) = &self.webhook {
            validate_webhook(webhook)?;
        }
        Ok(())
    }

    fn check_targets(&self, project: &ManagerProject) -> Result<(), String> {
        if self.repos.iter().any(|repo| !project.repos.contains(repo)) {
            return Err("고른 프로젝트에 속한 GitHub 저장소만 추가할 수 있다".into());
        }
        if self
            .agendas
            .iter()
            .any(|agenda| !project.agendas.contains(agenda))
        {
            return Err("고른 프로젝트에 속한 아젠다만 추가할 수 있다".into());
        }
        Ok(())
    }

    pub fn config(&self, exe: &str) -> Result<Value, String> {
        if exe.is_empty() {
            return Err("ais-cli 실행 파일 경로를 찾지 못했다".into());
        }
        if self.legacy() {
            return Err("먼저 기존 매니저에 프로젝트를 지정해 주세요".into());
        }
        Ok(json!({
            "mcp_servers": {"aisurada": {
                "command": exe, "args": ["mcp", "--manager", self.name],
                "tools": {"include": self.tools}
            }}
        }))
    }
}

fn safe_webhook_segment(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let Some(encoded) = segment.get(i + 1..i + 3) else {
                return false;
            };
            let Ok(byte) = u8::from_str_radix(encoded, 16) else {
                return false;
            };
            if byte.is_ascii_control() || matches!(byte, b'/' | b'\\' | b'.') {
                return false;
            }
            i += 2;
        }
        i += 1;
    }
    true
}

pub fn validate_webhook(webhook: &str) -> Result<(), String> {
    let endpoint = ["http://127.0.0.1:", "http://[::1]:", "http://localhost:"]
        .iter()
        .find_map(|prefix| webhook.strip_prefix(prefix));
    if endpoint
        .and_then(|value| value.split_once('/'))
        .is_some_and(|(port, path)| {
            port.bytes().all(|byte| byte.is_ascii_digit())
                && port.parse::<u16>().is_ok_and(|value| value != 0)
                && !path.is_empty()
                && path
                    .split('/')
                    .all(|segment| !matches!(segment, "" | "." | ".."))
                && path
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-_/%.~".contains(&byte))
                && path.split('/').all(safe_webhook_segment)
        })
    {
        return Ok(());
    }
    Err(
        "웹훅은 이 기기의 포트와 경로를 지정한다. 예: http://127.0.0.1:8644/webhooks/aisurada"
            .into(),
    )
}

fn folder(env: &Env) -> Result<PathBuf, String> {
    if env.home.is_empty() {
        return Err("사용자 홈 폴더를 찾지 못했다".into());
    }
    Ok(PathBuf::from(&env.home).join("Library/Application Support/aisurada"))
}

fn canonical_vault(env: &Env) -> Result<String, String> {
    let path = resolved_vault_path(env).ok_or("vault를 정하지 못했다")?;
    fs::canonicalize(path)
        .map_err(|error| format!("vault 폴더를 열지 못했다: {error}"))?
        .into_os_string()
        .into_string()
        .map_err(|_| "vault 경로가 UTF-8이 아니다".into())
}

pub fn list(env: &Env) -> Result<Vec<Manager>, String> {
    let bytes = match fs::read(folder(env)?.join("managers.json")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("매니저 등록부를 읽지 못했다: {error}")),
    };
    let managers: Vec<Manager> = serde_json::from_slice(&bytes)
        .map_err(|error| format!("매니저 등록부를 읽지 못했다: {error}"))?;
    let mut names = HashSet::new();
    for manager in &managers {
        manager.validate()?;
        if !names.insert(&manager.name) {
            return Err(format!("등록부에 이름이 중복됐다: {}", manager.name));
        }
    }
    Ok(managers)
}

pub fn find(env: &Env, name: &str) -> Result<Manager, String> {
    list(env)?
        .into_iter()
        .find(|row| row.name == name)
        .ok_or_else(|| format!("등록되지 않은 매니저다: {name}"))
}

pub fn find_enabled(env: &Env, name: &str) -> Result<Manager, String> {
    let manager = find(env, name)?;
    if manager.legacy() {
        return Err("먼저 기존 매니저에 프로젝트를 지정해 주세요".into());
    }
    if !manager.enabled {
        return Err(format!("꺼 둔 매니저다: {name}"));
    }
    if manager.scope().is_none() {
        return Err("매니저의 프로젝트를 찾을 수 없다".into());
    }
    Ok(manager)
}

fn set_options(manager: &mut Manager, options: &[String]) -> Result<(), String> {
    let items = |value: &str| -> Result<Vec<String>, String> {
        if value.starts_with('[') {
            return serde_json::from_str(value)
                .map_err(|_| "설정 목록은 문자열 배열로 적어 주세요".into());
        }
        Ok(value
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect())
    };
    for option in options {
        let (key, value) = option
            .split_once('=')
            .ok_or("설정은 이름=값으로 지정한다")?;
        match key {
            "label" => manager.label = value.to_owned(),
            "kind" => manager.kind = value.to_owned(),
            "tools" => manager.tools = items(value)?,
            "repos" => manager.repos = items(value)?,
            "agendas" => manager.agendas = items(value)?,
            "project" => manager.project = value.to_owned(),
            "webhook" => manager.webhook = (!value.is_empty()).then(|| value.to_owned()),
            "slackChannels" => manager.slack_channels = items(value)?,
            "notify" => manager.notify = items(value)?,
            "notifyEnabled" => {
                manager.notify_enabled = value
                    .parse()
                    .map_err(|_| "notifyEnabled에는 true 또는 false를 준다")?
            }
            "enabled" => {
                manager.enabled = value
                    .parse()
                    .map_err(|_| "enabled에는 true 또는 false를 준다")?
            }
            _ => return Err(format!("지원하지 않는 설정이다: {key}")),
        }
    }
    Ok(())
}

fn bind(manager: &mut Manager, env: &Env) -> Result<(), String> {
    manager.vault = canonical_vault(env)?;
    let project = manager_project(Path::new(&manager.vault), &manager.project)
        .ok_or("현재 vault의 프로젝트를 지정해 주세요")?;
    manager.project_id = project.id.clone();
    manager.check_targets(&project)
}

fn copy_manager(
    source: &Manager,
    name: &str,
    options: &[String],
    env: &Env,
) -> Result<Manager, String> {
    if source.legacy() {
        return Err("먼저 기존 매니저에 프로젝트를 지정해 주세요".into());
    }
    let mut target = Manager::new(name);
    target.label.clone_from(&source.label);
    target.kind.clone_from(&source.kind);
    target.tools.clone_from(&source.tools);
    target.notify.clone_from(&source.notify);
    target.notify_enabled = source.notify_enabled;
    let slug = options
        .iter()
        .rev()
        .find_map(|option| option.strip_prefix("project="))
        .ok_or("복사할 프로젝트를 지정해 주세요")?;
    if let Some((base, _)) = source.webhook.as_ref().and_then(|url| url.rsplit_once('/')) {
        let segment = slug
            .bytes()
            .map(|byte| {
                if byte.is_ascii_alphanumeric() || b"-_.".contains(&byte) {
                    (byte as char).to_string()
                } else {
                    format!("%{byte:02X}")
                }
            })
            .collect::<String>();
        target.webhook = Some(format!("{base}/{segment}"));
    }
    set_options(&mut target, options)?;
    target.enabled = false;
    bind(&mut target, env)?;
    if target.vault == source.vault && target.project_id == source.project_id {
        return Err("다른 프로젝트로만 복사할 수 있다".into());
    }
    Ok(target)
}

fn registration_id() -> String {
    format!(
        "{}{:020}",
        clock().now_millis().max(0),
        ids().next_random_u64()
    )
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    at: u64,
    tool: String,
    result: String,
    target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct History {
    first_call: Option<u64>,
    events: Vec<Event>,
}

fn history_path(env: &Env, manager: &Manager) -> Result<PathBuf, String> {
    Ok(folder(env)?
        .join("manager-history")
        .join(format!("{}-{}.json", manager.name, manager.registration)))
}

fn history(env: &Env, manager: &Manager) -> Result<History, String> {
    match fs::read(history_path(env, manager)?) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| error.to_string()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(History::default()),
        Err(error) => Err(error.to_string()),
    }
}

/// 호출 본문과 오류 원문은 읽거나 보관하지 않는다. 대상은 호출 결과에 든 태스크·노트 식별자다.
pub fn record(
    env: &Env,
    manager: &Manager,
    tool: &str,
    result: &str,
    target: Option<&str>,
) -> Result<(), String> {
    record_event(env, manager, tool, result, target, None)
}

pub fn record_webhook(
    env: &Env,
    manager: &Manager,
    target: &str,
    reason: Option<&str>,
) -> Result<(), String> {
    let result = if reason.is_some() { "failed" } else { "ok" };
    record_event(env, manager, "status_webhook", result, Some(target), reason)
}

fn record_event(
    env: &Env,
    manager: &Manager,
    tool: &str,
    result: &str,
    target: Option<&str>,
    reason: Option<&str>,
) -> Result<(), String> {
    if manager.legacy() {
        return Ok(());
    }
    let folder = folder(env)?;
    let _lock = acquire_lock(&folder).map_err(|error| error.to_string())?;
    if !find(env, &manager.name).is_ok_and(|item| item.same_registration(manager)) {
        return Ok(());
    }
    let path = history_path(env, manager)?;
    fs::create_dir_all(path.parent().ok_or("매니저 기록 폴더가 없다")?)
        .map_err(|error| error.to_string())?;
    let mut history = match fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice::<History>(&bytes) {
            Ok(history) => history,
            Err(error) => {
                let backup = path.with_extension(format!("corrupt-{}.json", registration_id()));
                fs::rename(&path, &backup).map_err(|error| error.to_string())?;
                eprintln!("매니저 호출 기록을 별도로 보관하고 다시 만들어요: {error}");
                History::default()
            }
        },
        Err(error) if error.kind() == ErrorKind::NotFound => History::default(),
        Err(error) => return Err(error.to_string()),
    };
    let at = clock().now_millis().max(0) as u64;
    if !matches!(tool, "status_webhook" | "test_webhook") && result == "ok" {
        history.first_call.get_or_insert(at);
    }
    let target = target.filter(|value| {
        let is_id = value.starts_with("TSK-") || value.starts_with("nd") || value.starts_with("sn");
        is_id
            && value.len() <= 40
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    });
    history.events.insert(
        0,
        Event {
            at,
            tool: tool.into(),
            result: result.into(),
            target: target.map(str::to_owned),
            reason: reason.map(str::to_owned),
        },
    );
    history.events.truncate(1000);
    let bytes = serde_json::to_vec(&history).map_err(|error| error.to_string())?;
    write_atomic(&path, &bytes).map_err(|error| error.to_string())
}

fn dashboard(env: &Env, managers: &[Manager]) -> Result<Value, String> {
    let vault = canonical_vault(env)?;
    let items: Vec<_> = managers
        .iter()
        .filter(|item| item.legacy() || item.vault == vault)
        .map(|item| {
            let history = history(env, item);
            let issue = if item.legacy() {
                Some("프로젝트를 지정해 주세요")
            } else if item.scope().is_none() {
                Some("프로젝트를 찾을 수 없어요")
            } else {
                None
            };
            let history_issue = history.is_err().then_some("호출 기록을 읽을 수 없어요");
            json!({"manager": item, "issue": issue, "historyIssue": history_issue, "history": history.unwrap_or_default()})
        })
        .collect();
    Ok(json!({
        "items": items,
        "names": managers.iter().map(|item| &item.name).collect::<Vec<_>>(),
        "defaultTools": DEFAULT_TOOLS,
        "optionalTools": OPTIONAL_TOOLS,
        "defaultNotify": DEFAULT_STATES,
    }))
}

fn command(args: &[String], env: &Env) -> Result<Value, String> {
    let folder = folder(env)?;
    let _lock = if matches!(
        args.first().map(String::as_str),
        Some("add" | "update" | "enable" | "disable" | "copy" | "remove")
    ) {
        fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
        Some(acquire_lock(&folder).map_err(|error| error.to_string())?)
    } else {
        None
    };
    let mut managers = list(env)?;
    match args {
        [action] if action == "list" => return Ok(json!(managers)),
        [action] if action == "dashboard" => return dashboard(env, &managers),
        _ => {}
    }
    let [action, name, options @ ..] = args else {
        return Err("사용법: ais-cli manager list | add NAME project=… […] | update NAME […] | enable NAME | disable NAME | config NAME | copy NAME NEW_NAME project=… | remove NAME".into());
    };
    let existing = managers.iter().position(|item| item.name == *name);
    let missing = || format!("등록되지 않은 매니저다: {name}");
    let updated = match (action.as_str(), existing, options) {
        ("config", Some(i), []) => return managers[i].config(&env.exe),
        ("add", None, options) => {
            let mut item = Manager::new(name);
            set_options(&mut item, options)?;
            bind(&mut item, env)?;
            managers.push(item.clone());
            item
        }
        ("add", Some(_), _) => return Err(format!("이미 등록된 매니저다: {name}")),
        ("copy", Some(i), [new_name, options @ ..]) => {
            if managers.iter().any(|item| &item.name == new_name) {
                return Err(format!("이미 등록된 매니저다: {new_name}"));
            }
            let item = copy_manager(&managers[i], new_name, options, env)?;
            managers.push(item.clone());
            item
        }
        ("update", Some(i), options) => {
            let item = &mut managers[i];
            let previous = item.clone();
            set_options(item, options)?;
            if previous.legacy() {
                item.registration = registration_id();
                item.legacy_key = true;
                bind(item, env)?;
            } else {
                if item.project != previous.project {
                    return Err("프로젝트를 바꾸려면 다른 프로젝트로 복사해 주세요".into());
                }
                let project = item.scope().ok_or("매니저의 프로젝트를 찾을 수 없다")?;
                if item.repos != previous.repos
                    || item.agendas != previous.agendas
                    || item.tools != previous.tools
                    || (item.enabled && !previous.enabled)
                {
                    item.check_targets(&project)?;
                }
            }
            item.clone()
        }
        ("enable" | "disable", Some(i), []) => {
            let item = &mut managers[i];
            if action == "enable" {
                item.check_targets(
                    &item
                        .scope()
                        .ok_or("매니저가 속한 프로젝트를 먼저 확인해 주세요")?,
                )?;
            }
            item.enabled = action == "enable";
            item.clone()
        }
        ("remove", Some(i), []) => managers.remove(i),
        (_, None, _) => return Err(missing()),
        _ => return Err(format!("매니저 명령의 인자를 확인해 주세요: {action}")),
    };
    updated.validate()?;
    let bytes = serde_json::to_vec_pretty(&managers).map_err(|error| error.to_string())?;
    write_atomic(&folder.join("managers.json"), &bytes).map_err(|error| error.to_string())?;
    if action == "remove" && !updated.legacy() {
        let _ = fs::remove_file(history_path(env, &updated)?);
    }
    Ok(json!(updated))
}

pub fn run(args: &[String], env: &Env) -> Outcome {
    match command(args, env) {
        Ok(value) => Outcome::out(format!("{value:#}\n")),
        Err(error) => Outcome::err(format!("{error}\n"), 2),
    }
}
