//! 승인 기록(#53): AI가 보낸 자원 변경은 정책에 따라 바로 적용하거나 승인을 기다리게 하고, 승인한 판의 지문과 사본을 앱 폴더에 둔다.
//! 규칙과 오류 문구는 Java 엔진의 `Approvals`와 같다.
//!
//! 기록은 vault가 아니라 앱 폴더(`~/Library/Application Support/aisurada/approvals/<vault 경로 해시>/`)에 있다:
//! `ledger.json`(정책과 항목별 지문)과 `copies/`(승인한 파일 사본). 환경 변수 `AISURADA_APPROVALS_DIR`로 바탕 폴더를 바꿀 수 있다(시험용).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use sha2::{Digest, Sha256};

use super::format::PERMISSIONS;
use super::{Resource, ResourceKind, tools_equal};
use crate::error::EngineError;
use crate::json::{self, JsonValue};
use crate::kernel::token::compute_token;
use crate::write::apparatus::{instant_text, now_millis};
use crate::write::atomic::write_atomic;
use crate::write::confine::absolute_normalized;
use crate::write::json_out::{literal, string, to_file_bytes};

pub const AUTO: &str = "auto";
pub const APPROVE: &str = "approve";

/// 이 프로세스 안에서 기록을 한 번에 하나만 읽고 쓰게 한다.
static LOCK: Mutex<()> = Mutex::new(());

/// 변경의 갈래. `fixed`는 정책과 상관없이 늘 승인을 거치는 것이다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Category {
    PermissionRaise,
    ToolsChange,
    RefAdd,
    PermissionLower,
    RefRemove,
    GlobalChange,
    ProjectContent,
}

impl Category {
    pub const ALL: [Category; 7] = [
        Category::PermissionRaise,
        Category::ToolsChange,
        Category::RefAdd,
        Category::PermissionLower,
        Category::RefRemove,
        Category::GlobalChange,
        Category::ProjectContent,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Category::PermissionRaise => "permission-raise",
            Category::ToolsChange => "tools-change",
            Category::RefAdd => "ref-add",
            Category::PermissionLower => "permission-lower",
            Category::RefRemove => "ref-remove",
            Category::GlobalChange => "global-change",
            Category::ProjectContent => "project-content",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Category::PermissionRaise => "권한 올림",
            Category::ToolsChange => "도구 칸 변경",
            Category::RefAdd => "참조 켜기",
            Category::PermissionLower => "권한 내림",
            Category::RefRemove => "참조 끄기",
            Category::GlobalChange => "글로벌 자원 변경",
            Category::ProjectContent => "프로젝트 자원 변경",
        }
    }

    pub fn fixed(self) -> bool {
        matches!(
            self,
            Category::PermissionRaise | Category::ToolsChange | Category::RefAdd
        )
    }

    pub fn of_code(code: &str) -> Option<Category> {
        Category::ALL.into_iter().find(|c| c.code() == code)
    }
}

/// 승인 정책 네 칸. 각 칸은 `auto`(AI도 바로 적용)나 `approve`(승인 필요)다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    pub project: String,
    pub global: String,
    pub refs: String,
    pub permission: String,
}

pub const POLICY_KEYS: [&str; 4] = ["project", "global", "refs", "permission"];

impl Policy {
    pub fn defaults() -> Policy {
        Policy {
            project: AUTO.to_string(),
            global: APPROVE.to_string(),
            refs: APPROVE.to_string(),
            permission: APPROVE.to_string(),
        }
    }

    pub fn with(&self, key: &str, value: &str) -> Result<Policy, EngineError> {
        if value != AUTO && value != APPROVE {
            return Err(EngineError::invalid_argument(format!(
                "{key} 는 auto 나 approve 여야 한다: {value}"
            )));
        }
        let mut next = self.clone();
        match key {
            "project" => next.project = value.to_string(),
            "global" => next.global = value.to_string(),
            "refs" => next.refs = value.to_string(),
            "permission" => next.permission = value.to_string(),
            _ => {
                return Err(EngineError::invalid_argument(format!(
                    "정책 칸은 [{}] 가운데 하나여야 한다: {key}",
                    POLICY_KEYS.join(", ")
                )));
            }
        }
        Ok(next)
    }

    pub fn requires(&self, category: Category) -> bool {
        if category.fixed() {
            return true;
        }
        let value = match category {
            Category::GlobalChange => &self.global,
            Category::ProjectContent => &self.project,
            Category::RefRemove => &self.refs,
            Category::PermissionLower => &self.permission,
            _ => APPROVE,
        };
        value == APPROVE
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "project": self.project,
            "global": self.global,
            "refs": self.refs,
            "permission": self.permission,
        })
    }

    fn to_value(&self) -> JsonValue {
        JsonValue::Object(vec![
            ("project".into(), string(&self.project)),
            ("global".into(), string(&self.global)),
            ("refs".into(), string(&self.refs)),
            ("permission".into(), string(&self.permission)),
        ])
    }
}

/// 승인한 파일 하나의 기록
#[derive(Clone, Debug)]
pub struct Entry {
    pub token: String,
    pub at: Option<String>,
    pub by: Option<String>,
}

pub struct Ledger {
    pub policy: Policy,
    /// 키는 `res:<자원 id>`나 `refs:<프로젝트 slug>`. 넣은 차례를 지킨다.
    pub items: Vec<(String, Entry)>,
}

impl Ledger {
    pub fn get(&self, key: &str) -> Option<&Entry> {
        self.items.iter().find(|(k, _)| k == key).map(|(_, e)| e)
    }
}

/// 이 변경을 대기에 올려야 하는가와 그 까닭
pub struct Decision {
    pub queue: bool,
    pub reasons: Vec<Category>,
}

fn rank(permission: Option<&str>) -> usize {
    PERMISSIONS
        .iter()
        .position(|p| *p == permission.unwrap_or("read"))
        .unwrap_or(0)
}

/// 자원 변경의 갈래. `before`는 고치기 전(만들기면 `None`), `after`는 고친 뒤(지우기면 `None`)다.
pub fn classify(before: Option<&Resource>, after: Option<&Resource>) -> Vec<Category> {
    let any = after.or(before).expect("before와 after 가운데 하나는 있다");
    let mut out = vec![if any.is_global() {
        Category::GlobalChange
    } else {
        Category::ProjectContent
    }];
    if let Some(after) = after
        && any.kind == ResourceKind::Agent
    {
        let b = before.map_or(0, |r| rank(r.permission.as_deref()));
        let a = rank(after.permission.as_deref());
        if a > b {
            out.push(Category::PermissionRaise);
        } else if a < b {
            out.push(Category::PermissionLower);
        }
    }
    if let Some(after) = after {
        let empty: Vec<(String, crate::frontmatter::Value)> = Vec::new();
        let bt = before.map_or(&empty, |r| &r.tools);
        if !tools_equal(bt, &after.tools) {
            out.push(Category::ToolsChange);
        }
    }
    out
}

pub fn classify_refs(added: &[String], removed: &[String]) -> Vec<Category> {
    let mut out = Vec::new();
    if !added.is_empty() {
        out.push(Category::RefAdd);
    }
    if !removed.is_empty() {
        out.push(Category::RefRemove);
    }
    out
}

/// AI 통로가 아니면 바로 적용한다. AI 통로면 정책이 승인을 요구하는 갈래가 하나라도 있을 때 대기에 올린다.
pub fn decide(policy: &Policy, categories: &[Category], ai_channel: bool) -> Decision {
    if !ai_channel {
        return Decision {
            queue: false,
            reasons: Vec::new(),
        };
    }
    let reasons: Vec<Category> = categories
        .iter()
        .copied()
        .filter(|c| policy.requires(*c))
        .collect();
    Decision {
        queue: !reasons.is_empty(),
        reasons,
    }
}

pub fn res_key(id: &str) -> String {
    format!("res:{id}")
}

pub fn refs_key(project: &str) -> String {
    format!("refs:{project}")
}

fn sha256_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// 기록 폴더들을 두는 바탕 폴더. 환경 변수 `AISURADA_APPROVALS_DIR`가 있으면 그것, 없으면 `~/Library/Application Support/aisurada/approvals`다.
pub fn base_dir() -> Result<PathBuf, EngineError> {
    match std::env::var("AISURADA_APPROVALS_DIR") {
        Ok(over) if !over.trim().is_empty() => Ok(PathBuf::from(over)),
        _ => {
            let home = std::env::var("HOME").unwrap_or_default();
            if home.trim().is_empty() {
                return Err(EngineError::internal(
                    "승인 기록 폴더를 정하지 못했다(HOME 이 없다)",
                ));
            }
            Ok(Path::new(&home).join("Library/Application Support/aisurada/approvals"))
        }
    }
}

/// 바탕 폴더 아래 이 vault의 기록 폴더(vault 경로의 해시 앞 16자)
pub fn dir_under(base: &Path, vault: &Path) -> PathBuf {
    let key = absolute_normalized(vault).to_string_lossy().into_owned();
    base.join(&sha256_hex(&key)[..16])
}

/// 이 vault의 기록 폴더
pub fn dir(vault: &Path) -> Result<PathBuf, EngineError> {
    Ok(dir_under(&base_dir()?, vault))
}

fn ledger_file(vault: &Path) -> Result<PathBuf, EngineError> {
    Ok(dir(vault)?.join("ledger.json"))
}

fn copy_path(vault: &Path, key: &str) -> Result<PathBuf, EngineError> {
    Ok(dir(vault)?
        .join("copies")
        .join(format!("{}.bin", &sha256_hex(key)[..32])))
}

fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// 지금 기록. 기록장이 없으면 지금 있는 자원 파일과 참조 목록을 모두 승인한 것으로 보고 기준선을 만든다.
pub fn ledger(vault: &Path) -> Result<Ledger, EngineError> {
    let _guard = lock();
    load(vault)
}

pub fn policy(vault: &Path) -> Result<Policy, EngineError> {
    Ok(ledger(vault)?.policy)
}

pub fn set_policy(vault: &Path, policy: &Policy) -> Result<Policy, EngineError> {
    let _guard = lock();
    let current = load(vault)?;
    store(
        vault,
        &Ledger {
            policy: policy.clone(),
            items: current.items,
        },
    )?;
    Ok(policy.clone())
}

/// 이 바이트를 승인한 판으로 기록하고 사본을 둔다.
pub fn record(vault: &Path, key: &str, bytes: &[u8], by: &str) -> Result<(), EngineError> {
    let _guard = lock();
    let current = load(vault)?;
    let mut items = current.items;
    let entry = Entry {
        token: compute_token(bytes),
        at: Some(instant_text(now_millis())),
        by: Some(by.to_string()),
    };
    match items.iter_mut().find(|(k, _)| k == key) {
        Some(slot) => slot.1 = entry,
        None => items.push((key.to_string(), entry)),
    }
    write_atomic(&copy_path(vault, key)?, bytes)?;
    store(
        vault,
        &Ledger {
            policy: current.policy,
            items,
        },
    )
}

/// 승인 기록과 사본을 지운다. 기록이 없으면 아무것도 안 한다.
pub fn forget(vault: &Path, key: &str) -> Result<(), EngineError> {
    let _guard = lock();
    let current = load(vault)?;
    if current.get(key).is_none() {
        return Ok(());
    }
    let items: Vec<(String, Entry)> = current
        .items
        .into_iter()
        .filter(|(k, _)| k != key)
        .collect();
    let _ = fs::remove_file(copy_path(vault, key)?);
    store(
        vault,
        &Ledger {
            policy: current.policy,
            items,
        },
    )
}

/// 승인해 둔 사본. 없으면 `None`이다.
pub fn copy(vault: &Path, key: &str) -> Option<Vec<u8>> {
    let path = copy_path(vault, key).ok()?;
    if fs::metadata(&path).map(|m| m.is_file()).unwrap_or(false) {
        fs::read(path).ok()
    } else {
        None
    }
}

fn str_of(entries: &[(String, JsonValue)], key: &str) -> Option<String> {
    entries
        .iter()
        .find(|(k, _)| k == key)
        .and_then(|(_, v)| match v {
            JsonValue::Str(s) => Some(s.clone()),
            _ => None,
        })
}

fn load(vault: &Path) -> Result<Ledger, EngineError> {
    let file = ledger_file(vault)?;
    if !file.exists() {
        return adopt_baseline(vault);
    }
    let failed = |message: &str| {
        EngineError::internal(format!(
            "승인 기록을 읽지 못했다: {} ({message}). 파일을 확인한다",
            file.display()
        ))
    };
    let text = fs::read(&file)
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .ok_or_else(|| failed("파일을 못 읽었다"))?;
    let JsonValue::Object(root) = json::parse(&text).map_err(|m| failed(&m))? else {
        return Err(failed("객체가 아니다"));
    };
    let mut policy = Policy::defaults();
    if let Some((_, JsonValue::Object(po))) = root.iter().find(|(k, _)| k == "policy") {
        for key in POLICY_KEYS {
            if let Some(value) = str_of(po, key) {
                policy = policy.with(key, &value).map_err(|e| failed(&e.message))?;
            }
        }
    }
    let mut items: Vec<(String, Entry)> = Vec::new();
    if let Some((_, JsonValue::Object(io))) = root.iter().find(|(k, _)| k == "items") {
        for (key, value) in io {
            if let JsonValue::Object(eo) = value
                && let Some(token) = str_of(eo, "token")
            {
                items.push((
                    key.clone(),
                    Entry {
                        token,
                        at: str_of(eo, "at"),
                        by: str_of(eo, "by"),
                    },
                ));
            }
        }
    }
    Ok(Ledger { policy, items })
}

fn store(vault: &Path, ledger: &Ledger) -> Result<(), EngineError> {
    let items: Vec<(String, JsonValue)> = ledger
        .items
        .iter()
        .map(|(key, entry)| {
            let mut row = vec![("token".to_string(), string(&entry.token))];
            if let Some(at) = &entry.at {
                row.push(("at".to_string(), string(at)));
            }
            if let Some(by) = &entry.by {
                row.push(("by".to_string(), string(by)));
            }
            (key.clone(), JsonValue::Object(row))
        })
        .collect();
    let root = JsonValue::Object(vec![
        ("version".to_string(), literal("1")),
        ("policy".to_string(), ledger.policy.to_value()),
        ("items".to_string(), JsonValue::Object(items)),
    ]);
    write_atomic(&ledger_file(vault)?, &to_file_bytes(&root))
}

fn read_quiet(path: &Path) -> Option<Vec<u8>> {
    if fs::metadata(path).map(|m| m.is_file()).unwrap_or(false) {
        fs::read(path).ok()
    } else {
        None
    }
}

fn adopt_baseline(vault: &Path) -> Result<Ledger, EngineError> {
    let mut items: Vec<(String, Entry)> = Vec::new();
    let at = instant_text(now_millis());
    for resource in super::store::scan(vault).items {
        let Some(bytes) = read_quiet(&vault.join(&resource.rel_path)) else {
            continue;
        };
        let key = res_key(&resource.id);
        write_atomic(&copy_path(vault, &key)?, &bytes)?;
        items.push((
            key,
            Entry {
                token: compute_token(&bytes),
                at: Some(at.clone()),
                by: Some("baseline".into()),
            },
        ));
    }
    for project in super::refs::projects_with_refs_file(vault) {
        let Some(bytes) = read_quiet(&super::refs::file(vault, &project)) else {
            continue;
        };
        let key = refs_key(&project);
        write_atomic(&copy_path(vault, &key)?, &bytes)?;
        items.push((
            key,
            Entry {
                token: compute_token(&bytes),
                at: Some(at.clone()),
                by: Some("baseline".into()),
            },
        ));
    }
    let ledger = Ledger {
        policy: Policy::defaults(),
        items,
    };
    store(vault, &ledger)?;
    Ok(ledger)
}

/// 지금 기록의 정책과 갈래 목록을 응답용 JSON으로 만든다.
pub fn categories_json() -> Vec<serde_json::Value> {
    Category::ALL
        .iter()
        .map(|c| serde_json::json!({"category": c.code(), "label": c.label(), "fixed": c.fixed()}))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontmatter::Value;

    fn agent(project: Option<&str>, permission: &str, tools: Vec<(String, Value)>) -> Resource {
        Resource {
            id: "ap1-00000001".into(),
            kind: ResourceKind::Agent,
            project: project.map(str::to_string),
            name: "n".into(),
            description: "d".into(),
            permission: Some(permission.into()),
            skills: Vec::new(),
            repo: None,
            tools,
            body: String::new(),
            edited_at: None,
            edited_session: None,
            edited_agent: None,
            rel_path: "p".into(),
            token: None,
        }
    }

    #[test]
    fn fixed_categories_need_approval_whatever_the_policy_says() {
        let all_auto = Policy::defaults()
            .with("project", AUTO)
            .and_then(|p| p.with("global", AUTO))
            .and_then(|p| p.with("refs", AUTO))
            .and_then(|p| p.with("permission", AUTO))
            .unwrap();
        for category in Category::ALL {
            assert_eq!(
                all_auto.requires(category),
                category.fixed(),
                "{category:?}"
            );
        }
        assert!(Policy::defaults().requires(Category::GlobalChange));
        assert!(!Policy::defaults().requires(Category::ProjectContent));
    }

    #[test]
    fn rejects_unknown_policy_values_and_keys() {
        let p = Policy::defaults();
        assert_eq!(
            p.with("project", "maybe").unwrap_err().message,
            "project 는 auto 나 approve 여야 한다: maybe"
        );
        assert_eq!(
            p.with("nope", AUTO).unwrap_err().message,
            "정책 칸은 [project, global, refs, permission] 가운데 하나여야 한다: nope"
        );
    }

    #[test]
    fn classifies_permission_and_tools_changes() {
        let before = agent(None, "read", Vec::new());
        let raised = agent(None, "edit", Vec::new());
        assert_eq!(
            classify(Some(&before), Some(&raised)),
            vec![Category::GlobalChange, Category::PermissionRaise]
        );
        assert_eq!(
            classify(Some(&raised), Some(&before)),
            vec![Category::GlobalChange, Category::PermissionLower]
        );
        let tooled = agent(
            Some("alpha"),
            "read",
            vec![("claude".into(), Value::Bool(true))],
        );
        assert_eq!(
            classify(
                Some(&agent(Some("alpha"), "read", Vec::new())),
                Some(&tooled)
            ),
            vec![Category::ProjectContent, Category::ToolsChange]
        );
        // 만들기는 권한이 read보다 높으면 올림이고, 지우기는 권한을 안 본다
        assert_eq!(
            classify(None, Some(&raised)),
            vec![Category::GlobalChange, Category::PermissionRaise]
        );
        assert_eq!(classify(Some(&raised), None), vec![Category::GlobalChange]);
        assert_eq!(
            classify_refs(&["a".to_string()], &["b".to_string()]),
            vec![Category::RefAdd, Category::RefRemove]
        );
    }

    #[test]
    fn people_apply_directly_and_ai_waits_for_the_required_categories() {
        let policy = Policy::defaults();
        let cats = [Category::ProjectContent];
        assert!(!decide(&policy, &cats, true).queue);
        assert!(!decide(&policy, &[Category::GlobalChange], false).queue);
        let waiting = decide(&policy, &[Category::ProjectContent, Category::RefAdd], true);
        assert!(waiting.queue);
        assert_eq!(waiting.reasons, vec![Category::RefAdd]);
    }
}
