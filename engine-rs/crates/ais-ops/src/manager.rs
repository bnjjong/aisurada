//! 매니저와 매니저별 도구·쓰기 범위를 앱 지원 폴더에 보관한다.

use std::collections::HashSet;
use std::io::ErrorKind;
use std::path::PathBuf;

use ais_core::valid_manager_name;
use ais_core::write::{acquire_lock, write_atomic};
use ais_domain::task::format;
use ais_infra::ports::fs;
use serde::{Deserialize, Serialize};

use crate::{Outcome, env::Env};

pub const TOOLS: [&str; 7] = [
    "list_tasks",
    "get_task",
    "search_docs",
    "get_doc",
    "create_task",
    "add_task_comment",
    "create_doc",
];

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manager {
    pub name: String,
    pub label: String,
    pub enabled: bool,
    pub tools: Vec<String>,
    pub repos: Vec<String>,
    pub agendas: Vec<String>,
    pub webhook: Option<String>,
}

impl Manager {
    fn new(name: &str) -> Self {
        Self {
            name: name.into(),
            label: name.into(),
            enabled: true,
            tools: TOOLS.into_iter().map(str::to_string).collect(),
            repos: Vec::new(),
            agendas: Vec::new(),
            webhook: None,
        }
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
        if self.tools.is_empty()
            || self
                .tools
                .iter()
                .any(|tool| !TOOLS.contains(&tool.as_str()))
            || self.tools.iter().collect::<HashSet<_>>().len() != self.tools.len()
        {
            return Err(format!(
                "도구는 허용 목록에서 중복 없이 고른다: {}",
                TOOLS.join(", ")
            ));
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
            return Err("아젠다 이름 또는 식별자가 비어 있거나 잘못됐다".into());
        }
        if let Some(webhook) = &self.webhook {
            let endpoint = ["http://127.0.0.1:", "http://[::1]:"]
                .iter()
                .find_map(|prefix| webhook.strip_prefix(prefix));
            if !endpoint
                .and_then(|value| value.split_once('/'))
                .is_some_and(|(port, path)| {
                    port.parse::<u16>().is_ok_and(|value| value != 0)
                        && !path.is_empty()
                        && path
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || b"-_/.".contains(&byte))
                })
            {
                return Err("웹훅은 이 기기의 포트와 경로를 지정한다. 예: http://127.0.0.1:8644/webhooks/aisurada".into());
            }
        }
        Ok(())
    }

    pub fn config(&self, exe: &str) -> Result<serde_json::Value, String> {
        if exe.is_empty() {
            return Err("ais-cli 실행 파일 경로를 찾지 못했다".into());
        }
        Ok(serde_json::json!({
            "mcp_servers": {"aisurada": {
                "command": exe, "args": ["mcp", "--manager", self.name],
                "tools": {"include": self.tools}
            }}
        }))
    }
}

fn file(env: &Env) -> Result<PathBuf, String> {
    if env.home.is_empty() {
        return Err("사용자 홈 폴더를 찾지 못했다".into());
    }
    Ok(PathBuf::from(&env.home).join("Library/Application Support/aisurada/managers.json"))
}

pub fn list(env: &Env) -> Result<Vec<Manager>, String> {
    let bytes = match fs::read(file(env)?) {
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

pub fn find_enabled(env: &Env, name: &str) -> Result<Manager, String> {
    let manager = list(env)?
        .into_iter()
        .find(|row| row.name == name)
        .ok_or_else(|| format!("등록되지 않은 매니저다: {name}"))?;
    if !manager.enabled {
        return Err(format!("꺼 둔 매니저다: {name}"));
    }
    Ok(manager)
}

fn set_options(manager: &mut Manager, options: &[String]) -> Result<(), String> {
    let items = |value: &str| {
        value
            .split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect()
    };
    for option in options {
        let (key, value) = option
            .split_once('=')
            .ok_or_else(|| format!("이름=값으로 지정한다: {option}"))?;
        match key {
            "label" => manager.label = value.to_string(),
            "tools" => manager.tools = items(value),
            "repos" => manager.repos = items(value),
            "agendas" => manager.agendas = items(value),
            "webhook" => manager.webhook = (!value.is_empty()).then(|| value.to_string()),
            _ => return Err(format!("지원하지 않는 설정이다: {key}")),
        }
    }
    manager.validate()
}

fn command(args: &[String], env: &Env) -> Result<String, String> {
    let destination = file(env)?;
    let folder = destination.parent().ok_or("앱 지원 폴더를 찾지 못했다")?;
    let _lock = if matches!(
        args.first().map(String::as_str),
        Some("add" | "update" | "enable" | "disable")
    ) {
        fs::create_dir_all(folder).map_err(|e| e.to_string())?;
        Some(acquire_lock(folder).map_err(|e| e.to_string())?)
    } else {
        None
    };
    let mut managers = list(env)?;
    if let [action] = args
        && action == "list"
    {
        return serde_json::to_string_pretty(&managers).map_err(|e| e.to_string());
    }
    let [action, name, options @ ..] = args else {
        return Err("사용법: ais-cli manager list | add NAME [label= repos= agendas= tools= webhook=] | update NAME [...] | enable NAME | disable NAME | config NAME".into());
    };
    let existing = managers.iter().position(|item| item.name == *name);
    if action == "config" && options.is_empty() {
        let manager = existing
            .map(|i| &managers[i])
            .ok_or_else(|| format!("등록되지 않은 매니저다: {name}"))?;
        return serde_json::to_string_pretty(&manager.config(&env.exe)?).map_err(|e| e.to_string());
    }

    let index = match (action.as_str(), existing) {
        ("add", None) => {
            managers.push(Manager::new(name));
            managers.len() - 1
        }
        ("add", Some(_)) => return Err(format!("이미 등록된 매니저다: {name}")),
        ("update" | "enable" | "disable", Some(i)) => i,
        ("update" | "enable" | "disable", None) => {
            return Err(format!("등록되지 않은 매니저다: {name}"));
        }
        _ => return Err(format!("지원하지 않는 매니저 명령이다: {action}")),
    };
    match action.as_str() {
        "enable" | "disable" if options.is_empty() => managers[index].enabled = action == "enable",
        "add" | "update" => set_options(&mut managers[index], options)?,
        _ => return Err(format!("{action} 명령에는 매니저 이름만 준다")),
    }
    let bytes = serde_json::to_vec_pretty(&managers).map_err(|e| e.to_string())?;
    write_atomic(&destination, &bytes).map_err(|e| e.to_string())?;
    serde_json::to_string(&managers[index]).map_err(|e| e.to_string())
}

pub fn run(args: &[String], env: &Env) -> Outcome {
    match command(args, env) {
        Ok(stdout) => Outcome::out(format!("{stdout}\n")),
        Err(error) => Outcome::err(format!("{error}\n"), 2),
    }
}
