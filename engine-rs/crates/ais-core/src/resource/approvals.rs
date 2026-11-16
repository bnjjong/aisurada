//! 승인 기록(#53): AI가 보낸 자원 변경은 정책에 따라 바로 적용하거나 승인을 기다리게 하고, 승인한 판의 지문과 사본을 앱 폴더에 둔다.
//! 규칙과 오류 문구는 Java 엔진의 `Approvals`와 같다.
//!
//! 기록은 vault가 아니라 앱 폴더(`~/Library/Application Support/aisurada/approvals/<vault 경로 해시>/`)에 있다:
//! `ledger.json`(정책과 항목별 지문)과 `copies/`(승인한 파일 사본). 환경 변수 `AISURADA_APPROVALS_DIR`로 바탕 폴더를 바꿀 수 있다(시험용).

use crate::ports::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use sha2::{Digest, Sha256};

use super::aggregate::{POLICY_KEYS, Policy};
use crate::error::EngineError;
use crate::json::{self, JsonValue};
use crate::kernel::token::compute_token;
use crate::write::apparatus::{instant_text, now_millis};
use crate::write::atomic::write_atomic;
use crate::write::confine::absolute_normalized;
use crate::write::json_out::{literal, string, to_file_bytes};

/// 이 프로세스 안에서 기록을 한 번에 하나만 읽고 쓰게 한다.
static LOCK: Mutex<()> = Mutex::new(());

/// 정책을 기록장의 JSON 객체로 만든다(`Policy` 타입은 도메인 크레이트에 있어 여기서 메서드를 붙일 수 없다).
fn policy_to_value(policy: &Policy) -> JsonValue {
    JsonValue::Object(vec![
        ("project".into(), string(&policy.project)),
        ("global".into(), string(&policy.global)),
        ("refs".into(), string(&policy.refs)),
        ("permission".into(), string(&policy.permission)),
    ])
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
    let env = crate::ports::env();
    match env.var("AISURADA_APPROVALS_DIR") {
        Some(over) if !over.trim().is_empty() => Ok(PathBuf::from(over)),
        _ => {
            let home = env.var("HOME").unwrap_or_default();
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
        ("policy".to_string(), policy_to_value(&ledger.policy)),
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
