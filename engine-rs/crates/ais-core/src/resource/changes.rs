//! 승인을 기다리는 변경(대기 변경)을 vault 안에 두는 곳(#53). 외부 설명서에 없는 확장이다.
//!
//! 변경 하나가 파일 하나다: `.aisurada/pending/<변경 id>.json`. AI가 쓴 변경이 승인이 필요하면 바로 적용하지 않고 여기에 올린다.
//! 대기 중인 AI 초안도 지식이라 vault 안에 두고, 어떤 AI가 무엇을 제안했는지가 이력에 남고 vault를 옮겨도 따라온다. 노트 목록과 검색은
//! `.aisurada/` 아래를 보지 않아서 이 파일들은 거기에 나타나지 않는다. 결정이 난 변경(승인·거절·기준이 바뀌어 무효)도 상태만 바꿔 남겨서
//! 「누가 썼고 누가 승인했는지」를 볼 수 있게 하고, 오래된 것부터 [`DECIDED_KEEP`]개를 넘으면 지운다.
//!
//! 변경은 JSON 객체다. 칸은 다음과 같다.
//! `id`, `status`(pending|approved|rejected|stale), `command`(save-resource|delete-resource|restore-resource|set-resource-ref),
//! `createdAt`, `proposer{session,agent,channel}`, `categories[코드]`, `reasons[코드]`, `fixed`, `target{kind,scope,project,id,name}`,
//! `args{명령에 준 옵션}`, `positional[명령에 준 자리 인자]`, `forcedId`(만들기), `before{…}`, `after{…}`(보여 주기용 칸), `summary`,
//! `decidedAt`, `decidedBy`, `reason`. 적용할 때는 `command`, `args`, `positional`로 같은 명령 경로를 한 번 더 지난다.

use crate::kernel::ids::CHANGE_ID as ID_SHAPE;
use crate::ports::fs;
use std::path::{Path, PathBuf};

use super::aggregate::ChangeStatus;
use super::format::value_to_json;
use super::{Resource, ResourceKind};
use crate::error::EngineError;
use crate::json::{self, JsonValue};
use crate::text::compare_utf16;
use crate::write::apparatus::next_id;
use crate::write::atomic::write_atomic;
use crate::write::json_out::{literal, string, to_file_bytes};

pub const DIR: &str = ".aisurada/pending";
pub const DECIDED_KEEP: usize = 200;

pub const PENDING: &str = ChangeStatus::Pending.as_str();
pub const APPROVED: &str = ChangeStatus::Approved.as_str();
pub const REJECTED: &str = ChangeStatus::Rejected.as_str();
pub const STALE: &str = ChangeStatus::Stale.as_str();

/// 변경 하나. JSON 객체이고 칸 차례를 지킨다.
pub type Change = JsonValue;

pub fn dir(vault: &Path) -> PathBuf {
    vault.join(DIR)
}

pub fn is_id(id: &str) -> bool {
    ID_SHAPE.is_match(id)
}

/// 새 변경 ID(`pc…`)
pub fn new_id() -> String {
    next_id("pc")
}

pub fn get<'a>(change: &'a Change, key: &str) -> Option<&'a JsonValue> {
    change.get(key)
}

pub fn get_str<'a>(change: &'a Change, key: &str) -> Option<&'a str> {
    match change.get(key) {
        Some(JsonValue::Str(s)) => Some(s),
        _ => None,
    }
}

/// 칸을 넣는다. 이미 있으면 그 자리에서 값만 바꾸고, 없으면 끝에 붙인다(`LinkedHashMap.put`).
pub fn put(change: &mut Change, key: &str, value: JsonValue) {
    if let JsonValue::Object(entries) = change {
        match entries.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = value,
            None => entries.push((key.to_string(), value)),
        }
    }
}

/// 대기에 올린다. 변경에는 `id`가 이미 들어 있어야 한다.
pub fn write(vault: &Path, change: &Change) -> Result<(), EngineError> {
    let id = get_str(change, "id").unwrap_or("null");
    if !is_id(id) {
        return Err(EngineError::internal(format!(
            "변경 id 모양이 아니다: {id}"
        )));
    }
    write_atomic(
        &dir(vault).join(format!("{id}.json")),
        &to_file_bytes(change),
    )
}

/// 변경 하나를 읽는다. 없으면 `AIS-404`, 모양이 틀리면 `AIS-400`이다.
pub fn read(vault: &Path, id: &str) -> Result<Change, EngineError> {
    if !is_id(id) {
        return Err(EngineError::invalid_argument(format!(
            "변경 id 모양이 아니다(pc…): {id}"
        )));
    }
    let f = dir(vault).join(format!("{id}.json"));
    if !fs::metadata(&f).map(|m| m.is_file()).unwrap_or(false) {
        return Err(EngineError::not_found(format!(
            "대기 변경을 못 찾았다: {id}"
        )));
    }
    read_file(&f).ok_or_else(|| EngineError::internal(format!("대기 변경 파일을 못 읽었다: {id}")))
}

/// 모든 변경. 올린 시각이 늦은 것이 앞이다. 못 읽는 파일은 건너뛴다.
pub fn list(vault: &Path) -> Vec<Change> {
    let mut out: Vec<Change> = Vec::new();
    let Ok(read_dir) = fs::read_dir(dir(vault)) else {
        return out;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") || path.file_stem().is_none() {
            continue;
        }
        if let Some(change) = read_file(&path)
            && get_str(&change, "id").is_some_and(is_id)
        {
            out.push(change);
        }
    }
    out.sort_by(|a, b| {
        compare_utf16(&text_of(b, "createdAt"), &text_of(a, "createdAt"))
            .then_with(|| compare_utf16(&text_of(b, "id"), &text_of(a, "id")))
    });
    out
}

fn text_of(change: &Change, key: &str) -> String {
    match change.get(key) {
        Some(JsonValue::Str(s)) => s.clone(),
        Some(JsonValue::Literal(raw)) => raw.clone(),
        Some(other) => json::compact(other),
        None => "null".to_string(),
    }
}

/// 대기 중인 것(`status=pending`)만
pub fn pending(vault: &Path) -> Vec<Change> {
    list(vault)
        .into_iter()
        .filter(|c| get_str(c, "status") == Some(PENDING))
        .collect()
}

/// 결정이 난 변경이 너무 많으면 오래된 것부터 지운다.
pub fn prune(vault: &Path) {
    let decided: Vec<Change> = list(vault)
        .into_iter()
        .filter(|c| get_str(c, "status") != Some(PENDING))
        .collect();
    for change in decided.iter().skip(DECIDED_KEEP) {
        // 못 지운 기록은 다음에 다시 지운다
        let _ = fs::remove_file(dir(vault).join(format!("{}.json", text_of(change, "id"))));
    }
}

// ---------------------------------------------------------------- JSON

fn read_file(path: &Path) -> Option<Change> {
    let text = String::from_utf8(fs::read(path).ok()?).ok()?;
    let value = json::parse(&text).ok()?;
    if !matches!(value, JsonValue::Object(_)) || !valid_numbers(&value) {
        return None;
    }
    Some(value)
}

/// 숫자 자리의 글이 참거짓·null·정수·실수가 아니면 못 읽는 파일이다(Java가 `Double.valueOf`에서 던진다).
fn valid_numbers(value: &JsonValue) -> bool {
    match value {
        JsonValue::Str(_) => true,
        JsonValue::Literal(raw) => {
            matches!(raw.as_str(), "true" | "false" | "null")
                || raw.parse::<i64>().is_ok()
                || raw.parse::<f64>().is_ok()
        }
        JsonValue::Array(items) => items.iter().all(valid_numbers),
        JsonValue::Object(entries) => entries.iter().all(|(_, v)| valid_numbers(v)),
    }
}

/// 자원을 보여 주기용 칸으로 줄인다(비교 화면이 쓴다). 지침은 길어서 그대로 싣는다.
pub fn view(r: &Resource) -> JsonValue {
    let mut m: Vec<(String, JsonValue)> = vec![
        ("id".into(), string(&r.id)),
        ("kind".into(), string(r.kind.label())),
        ("scope".into(), string(r.scope())),
    ];
    if let Some(project) = &r.project {
        m.push(("project".into(), string(project)));
    }
    m.push(("name".into(), string(&r.name)));
    m.push(("description".into(), string(&r.description)));
    if r.kind == ResourceKind::Agent {
        m.push((
            "permission".into(),
            r.permission
                .as_deref()
                .map_or_else(|| literal("null"), string),
        ));
        m.push((
            "skills".into(),
            JsonValue::Array(r.skills.iter().map(string).collect()),
        ));
        if let Some(repo) = &r.repo {
            m.push(("repo".into(), string(repo)));
        }
    }
    if !r.tools.is_empty() {
        m.push((
            "tools".into(),
            JsonValue::Object(
                r.tools
                    .iter()
                    .map(|(k, v)| (k.clone(), value_to_json(v)))
                    .collect(),
            ),
        ));
    }
    m.push(("body".into(), string(&r.body)));
    JsonValue::Object(m)
}
