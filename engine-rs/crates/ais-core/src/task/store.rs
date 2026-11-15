//! 태스크 파일을 vault에서 읽고 쓰는 곳. 규칙과 오류 문구는 Java 엔진의 `TaskStore`와 같다.
//!
//! ```text
//! .aisurada/tasks/
//!   TSK-14.md                  태스크(머리말 + 설명)
//!   TSK-14/activity.jsonl      활동(만들기, 상태 바뀜, 고침, 연결, 댓글). 한 줄에 사건 하나, 최근 500줄까지
//!   .trash/<id>/               지운 태스크
//! ```
//!
//! 연결은 건 쪽 파일에만 적고 반대쪽(막혀 있음 따위)은 읽을 때 계산하므로 두 파일이 어긋날 일이 없다.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use crate::kernel::task_link::TaskLinkKind;

use serde_json::Value as Json;

use super::format::{
    self, DEFAULT_PRIORITY, DEFAULT_TYPE, KEY, MAX_FILE_BYTES, MAX_LINKS,
};
use super::transitions;
use super::{Criterion, Link, Ref, Task, TaskStatus};
use crate::error::EngineError;
use crate::json::{self, JsonValue};
use crate::text::compare_utf16;
use crate::kernel::token::compute_token;
use crate::vault::project::scan_projects;
use crate::write::apparatus::{next_id, now_millis};
use crate::write::atomic::write_atomic;
use crate::write::stamp::Stamp;
use crate::write::sub_note::remove_tree;

pub const ROOT: &str = ".aisurada/tasks";
pub const ACTIVITY_KEEP: usize = 500;

/// 훑은 결과. `skipped`는 못 읽어 건너뛴 파일과 이유다.
pub struct Scan {
    pub items: Vec<Task>,
    pub skipped: Vec<String>,
}

impl Scan {
    pub fn by_key(&self, key: &str) -> Option<&Task> {
        self.items.iter().find(|t| t.key == key)
    }

    pub fn by_id(&self, id: &str) -> Option<&Task> {
        self.items.iter().find(|t| t.id == id)
    }
}

/// 고칠 값. `None`은 「안 줌(그대로 둔다)」이다. `repo`, `project`, `base_branch`는 빈 글이 「지운다」이고,
/// 목록 칸(`labels`, `criteria`, `refs`)은 빈 목록이 「비운다」다.
#[derive(Default)]
pub struct Fields {
    pub title: Option<String>,
    pub task_type: Option<String>,
    pub priority: Option<String>,
    pub repo: Option<String>,
    pub project: Option<String>,
    pub base_branch: Option<String>,
    pub body: Option<String>,
    pub labels: Option<Vec<String>>,
    pub criteria: Option<Vec<Criterion>>,
    pub refs: Option<Vec<Ref>>,
}

pub struct TrashEntry {
    pub id: String,
    pub key: String,
    pub title: String,
    pub rel_path: String,
    pub deleted_at: String,
}

/// 이 태스크에 걸린 연결 한 줄(건 쪽과 받은 쪽을 모두 담는다)
pub struct Related {
    pub kind: String,
    pub key: String,
    pub title: String,
    pub status: TaskStatus,
}

pub fn root(vault: &Path) -> PathBuf {
    vault.join(ROOT)
}

fn file_path(vault: &Path, key: &str) -> PathBuf {
    root(vault).join(format!("{key}.md"))
}

fn activity_path(vault: &Path, key: &str) -> PathBuf {
    root(vault).join(key).join("activity.jsonl")
}

fn rel(vault: &Path, file: &Path) -> String {
    file.strip_prefix(vault)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = read.flatten().map(|e| e.path()).collect();
    out.sort_by(|a, b| {
        compare_utf16(
            &a.file_name().unwrap_or_default().to_string_lossy(),
            &b.file_name().unwrap_or_default().to_string_lossy(),
        )
    });
    out
}

fn is_file(path: &Path) -> bool {
    fs::metadata(path).map(|m| m.is_file()).unwrap_or(false)
}

fn is_dir(path: &Path) -> bool {
    fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false)
}

// ---------------------------------------------------------------- 읽기

/// 모든 태스크를 훑는다. 못 읽는 파일은 건너뛰고 이유를 `skipped`에 적는다. 점으로 시작하는 폴더(`.trash`)는 안 본다.
pub fn scan(vault: &Path) -> Scan {
    let mut items: Vec<Task> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut ids: HashSet<String> = HashSet::new();
    let mut keys: HashSet<String> = HashSet::new();
    for file in sorted_entries(&root(vault)) {
        let name = file
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if name.starts_with('.') || !name.ends_with(".md") || !is_file(&file) {
            continue;
        }
        let rel_path = rel(vault, &file);
        let size = fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
        if size > MAX_FILE_BYTES as u64 {
            skipped.push(format!("{rel_path}: 파일이 너무 크다"));
            continue;
        }
        let Ok(bytes) = fs::read(&file) else {
            skipped.push(format!("{rel_path}: 파일을 못 읽었다"));
            continue;
        };
        match format::parse(&bytes, &rel_path) {
            Err(reason) => skipped.push(format!("{rel_path}: {reason}")),
            Ok(task) => {
                if name != format!("{}.md", task.key) {
                    skipped.push(format!(
                        "{rel_path}: 파일 이름과 key({})가 다르다",
                        task.key
                    ));
                    continue;
                }
                if !ids.insert(task.id.clone()) {
                    skipped.push(format!(
                        "{rel_path}: id 가 다른 태스크와 겹친다: {}",
                        task.id
                    ));
                    continue;
                }
                if !keys.insert(task.key.clone()) {
                    skipped.push(format!(
                        "{rel_path}: key 가 다른 태스크와 겹친다: {}",
                        task.key
                    ));
                    continue;
                }
                items.push(task);
            }
        }
    }
    items.sort_by_key(Task::number);
    Scan { items, skipped }
}

/// 키(`TSK-14`)나 id(`tk…`)로 태스크 하나를 찾는다. 없으면 `AIS-404`다.
pub fn find<'a>(scan: &'a Scan, reference: &str) -> Result<&'a Task, EngineError> {
    if format::java_blank(reference) {
        return Err(EngineError::invalid_argument(
            "태스크 키(TSK-14)나 id(tk…)가 필요하다",
        ));
    }
    let r = format::java_trim(reference);
    if KEY.is_match(r) {
        return scan
            .by_key(r)
            .ok_or_else(|| EngineError::not_found(format!("태스크를 못 찾았다: {r}")));
    }
    if format::ID.is_match(r) {
        return scan
            .by_id(r)
            .ok_or_else(|| EngineError::not_found(format!("태스크를 못 찾았다: {r}")));
    }
    Err(EngineError::invalid_argument(format!(
        "태스크 키(TSK-14)나 id(tk…) 모양이 아니다: {reference}"
    )))
}

/// 이 태스크를 막고 있으면서 아직 Done이 아닌 태스크 키들. 비면 막히지 않았다.
pub fn unfinished_blockers(scan: &Scan, key: &str) -> Vec<String> {
    scan.items
        .iter()
        .filter(|t| t.status != TaskStatus::Done)
        .filter(|t| t.links.iter().any(|l| l.kind == TaskLinkKind::Blocks && l.to == key))
        .map(|t| t.key.clone())
        .collect()
}

/// 이 태스크에 걸린 모든 연결. 이 태스크가 건 것과, 다른 태스크가 이 태스크에 건 것(반대쪽 이름)을 함께 낸다. 없는 태스크를 가리키면 뺀다.
pub fn related(scan: &Scan, task: &Task) -> Vec<Related> {
    let mut out: Vec<Related> = Vec::new();
    for link in &task.links {
        if let Some(other) = scan.by_key(&link.to) {
            out.push(Related {
                kind: link.kind.as_str().to_string(),
                key: other.key.clone(),
                title: other.title.clone(),
                status: other.status,
            });
        }
    }
    for other in &scan.items {
        if other.key == task.key {
            continue;
        }
        for link in &other.links {
            if link.to != task.key {
                continue;
            }
            let inverse = link.kind.inverse_label();
            if link.kind == TaskLinkKind::Relates
                && task
                    .links
                    .iter()
                    .any(|x| x.kind == TaskLinkKind::Relates && x.to == other.key)
            {
                continue;
            }
            out.push(Related {
                kind: inverse.to_string(),
                key: other.key.clone(),
                title: other.title.clone(),
                status: other.status,
            });
        }
    }
    out.sort_by(|a, b| compare_utf16(&a.kind, &b.kind).then_with(|| compare_utf16(&a.key, &b.key)));
    out
}

// ---------------------------------------------------------------- 만들기

fn require_project(vault: &Path, slug: &str) -> Result<(), EngineError> {
    if format::java_blank(slug) {
        return Err(EngineError::invalid_argument("프로젝트 slug 가 필요하다"));
    }
    if scan_projects(vault).iter().any(|p| p.slug == slug) {
        return Ok(());
    }
    Err(EngineError::not_found(format!(
        "프로젝트를 못 찾았다: {slug}"
    )))
}

/// 새 태스크를 Draft로 만든다. 요약(title), 저장소(repo), 완료 기준(criteria 1개 이상)이 필수다.
pub fn create(vault: &Path, f: Fields, stamp: &Stamp, actor: &str) -> Result<Task, EngineError> {
    let Some(title) = f.title else {
        return Err(EngineError::invalid_argument("title 이 필요하다"));
    };
    let repo = match f.repo {
        Some(r) if !r.is_empty() => r,
        _ => {
            return Err(EngineError::invalid_argument(
                "repo 가 필요하다(owner/name)",
            ));
        }
    };
    let criteria = match f.criteria {
        Some(c) if !c.is_empty() => c,
        _ => {
            return Err(EngineError::invalid_argument(
                "완료 기준(criteria)을 하나 이상 써야 만들 수 있다. 일이 끝났는지 이 기준으로 확인한다",
            ));
        }
    };
    let project = f.project.filter(|p| !p.is_empty());
    if let Some(project) = &project {
        require_project(vault, project)?;
    }

    let scan = scan(vault);
    let number = next_number(vault, &scan);
    let key = format!("TSK-{number}");
    let id = loop {
        let candidate = next_id("tk");
        if scan.by_id(&candidate).is_none() {
            break candidate;
        }
    };
    let path = file_path(vault, &key);
    let task = Task {
        key: key.clone(),
        id,
        title,
        task_type: f.task_type.unwrap_or_else(|| DEFAULT_TYPE.to_string()),
        priority: f.priority.unwrap_or_else(|| DEFAULT_PRIORITY.to_string()),
        status: TaskStatus::Draft,
        repo: Some(repo),
        project,
        base_branch: f.base_branch.filter(|b| !b.is_empty()),
        labels: f.labels.unwrap_or_default(),
        criteria: normalize_criteria(&[], &criteria, stamp, actor),
        links: Vec::new(),
        refs: f.refs.unwrap_or_default(),
        body: f.body.unwrap_or_default(),
        extra: Vec::new(),
        created_at: Some(stamp.at.clone()),
        edited_at: Some(stamp.at.clone()),
        edited_session: stamp.session.clone(),
        edited_agent: stamp.agent.clone(),
        rel_path: rel(vault, &path),
        token: None,
    };
    let bytes = format::render(&task)?;
    write_atomic(&path, &bytes)?;
    append_activity(vault, &key, event(stamp, actor, "created", Vec::new()))?;
    Ok(Task {
        token: Some(compute_token(&bytes)),
        ..task
    })
}

/// 다음 번호. 지금 있는 태스크와 휴지통에 있는 태스크의 가장 큰 번호 + 1이다(지운 번호를 다시 쓰지 않는다).
fn next_number(vault: &Path, scan: &Scan) -> u32 {
    let mut max = scan.items.iter().map(Task::number).max().unwrap_or(0);
    for entry in trash(vault) {
        max = max.max(format::number_of(&entry.key));
    }
    max + 1
}

// ---------------------------------------------------------------- 고치기

/// 태스크의 내용을 고친다. 직전에 읽은 `f1` 토큰이 지금 파일과 맞아야 쓴다(맞지 않으면 `AIS-412`). 실제로 달라지는 게 없으면 파일도
/// 활동도 안 바꾸고 `None`을 돌려준다. 상태와 연결은 여기서 못 바꾼다(`transition`, `link`).
pub fn update(
    vault: &Path,
    cur: &Task,
    f: Fields,
    f1: Option<&str>,
    stamp: &Stamp,
    actor: &str,
) -> Result<Option<Task>, EngineError> {
    let Some(f1) = f1.filter(|t| !format::java_blank(t)) else {
        return Err(EngineError::precondition_failed(format!(
            "f1 이 필요하다. get-task 로 읽은 precondition 을 f1= 으로 넘긴다: {}",
            cur.rel_path
        )));
    };
    if Some(format::java_trim(f1)) != cur.token.as_deref() {
        return Err(EngineError::precondition_failed(format!(
            "읽은 뒤에 파일이 바뀌었다. 다시 읽고 고친다: {}",
            cur.rel_path
        )));
    }
    let title = f.title.unwrap_or_else(|| cur.title.clone());
    let task_type = f.task_type.unwrap_or_else(|| cur.task_type.clone());
    let priority = f.priority.unwrap_or_else(|| cur.priority.clone());
    let repo = match f.repo {
        Some(r) => Some(r).filter(|r| !r.is_empty()),
        None => cur.repo.clone(),
    };
    let project = match f.project {
        Some(p) => Some(p).filter(|p| !p.is_empty()),
        None => cur.project.clone(),
    };
    let base = match f.base_branch {
        Some(b) => Some(b).filter(|b| !b.is_empty()),
        None => cur.base_branch.clone(),
    };
    let body = f.body.unwrap_or_else(|| cur.body.clone());
    let labels = f.labels.unwrap_or_else(|| cur.labels.clone());
    let criteria = match &f.criteria {
        Some(given) => normalize_criteria(&cur.criteria, given, stamp, actor),
        None => cur.criteria.clone(),
    };
    let refs = f.refs.unwrap_or_else(|| cur.refs.clone());
    if let Some(project) = &project
        && cur.project.as_ref() != Some(project)
    {
        require_project(vault, project)?;
    }

    let same = Task {
        title: title.clone(),
        task_type: task_type.clone(),
        priority: priority.clone(),
        repo: repo.clone(),
        project: project.clone(),
        base_branch: base.clone(),
        labels: labels.clone(),
        criteria: criteria.clone(),
        refs: refs.clone(),
        body: body.clone(),
        ..cur.clone()
    };
    if format::render(&same)? == format::render(cur)? {
        return Ok(None);
    }
    let next = Task {
        edited_at: Some(stamp.at.clone()),
        edited_session: stamp.session.clone(),
        edited_agent: stamp.agent.clone(),
        token: None,
        ..same
    };
    let bytes = format::render(&next)?;
    write_atomic(&vault.join(&cur.rel_path), &bytes)?;

    let mut changed: Vec<&str> = Vec::new();
    if title != cur.title {
        changed.push("title");
    }
    if task_type != cur.task_type {
        changed.push("type");
    }
    if priority != cur.priority {
        changed.push("priority");
    }
    if repo != cur.repo {
        changed.push("repo");
    }
    if project != cur.project {
        changed.push("project");
    }
    if base != cur.base_branch {
        changed.push("baseBranch");
    }
    if body != cur.body {
        changed.push("body");
    }
    if labels != cur.labels {
        changed.push("labels");
    }
    if criteria != cur.criteria {
        changed.push("criteria");
    }
    if refs != cur.refs {
        changed.push("refs");
    }
    let fields = JsonValue::Array(
        changed
            .iter()
            .map(|c| JsonValue::Str((*c).to_string()))
            .collect(),
    );
    append_activity(
        vault,
        &cur.key,
        event(stamp, actor, "edited", vec![("fields".to_string(), fields)]),
    )?;
    Ok(Some(Task {
        token: Some(compute_token(&bytes)),
        ..next
    }))
}

/// 완료 기준을 다듬는다. 새로 확인된 항목에는 확인한 시각과 쪽을 적고, 확인이 풀린 항목은 그 기록을 지운다. 이미 확인된 항목은 기록을 그대로 둔다.
fn normalize_criteria(
    old: &[Criterion],
    given: &[Criterion],
    stamp: &Stamp,
    actor: &str,
) -> Vec<Criterion> {
    given
        .iter()
        .map(|c| {
            if !c.done {
                return Criterion {
                    text: c.text.clone(),
                    done: false,
                    checked_at: None,
                    checked_by: None,
                };
            }
            if let Some(before) = old.iter().find(|o| o.done && o.text == c.text) {
                return before.clone();
            }
            Criterion {
                text: c.text.clone(),
                done: true,
                checked_at: Some(c.checked_at.clone().unwrap_or_else(|| stamp.at.clone())),
                checked_by: Some(c.checked_by.clone().unwrap_or_else(|| actor.to_string())),
            }
        })
        .collect()
}

// ---------------------------------------------------------------- 상태

/// 상태를 바꾼다. 규칙과 조건은 `transitions`가 검사한다. `expected_from`을 주면 지금 상태가 그것과 같을 때만 바꾼다(그 사이에
/// 러너나 다른 창이 먼저 바꿨으면 `AIS-409`).
pub fn transition(
    vault: &Path,
    cur: &Task,
    to: TaskStatus,
    actor: &str,
    expected_from: Option<&str>,
    stamp: &Stamp,
) -> Result<Task, EngineError> {
    if let Some(expected) = expected_from {
        let Some(exp) = TaskStatus::parse(expected) else {
            return Err(EngineError::invalid_argument(format!(
                "from 은 상태 이름이어야 한다: {expected}"
            )));
        };
        if exp != cur.status {
            return Err(EngineError::conflict(format!(
                "상태가 이미 바뀌었다: 지금 {}, 기대한 것 {}",
                cur.status.label(),
                exp.label()
            )));
        }
    }
    let scan = scan(vault);
    transitions::require(cur, to, actor, &unfinished_blockers(&scan, &cur.key))?;
    let next = Task {
        status: to,
        edited_at: Some(stamp.at.clone()),
        edited_session: stamp.session.clone(),
        edited_agent: stamp.agent.clone(),
        token: None,
        ..cur.clone()
    };
    let bytes = format::render(&next)?;
    write_atomic(&vault.join(&cur.rel_path), &bytes)?;
    append_activity(
        vault,
        &cur.key,
        event(
            stamp,
            actor,
            "status",
            vec![
                (
                    "from".to_string(),
                    JsonValue::Str(cur.status.label().to_string()),
                ),
                ("to".to_string(), JsonValue::Str(to.label().to_string())),
            ],
        ),
    )?;
    Ok(Task {
        token: Some(compute_token(&bytes)),
        ..next
    })
}

// ---------------------------------------------------------------- 연결

/// 연결을 걸거나 푼다. 연결은 건 쪽(`from`) 파일에만 적는다. 푼다(`remove`)면 두 방향 가운데 있는 쪽을 지운다.
/// 자기 자신에게 걸 수 없고, 막고 있음(`blocks`)은 서로를 막는 순환을 만들 수 없다. 이미 있는 연결을 다시 걸면 바뀐 게 없다.
/// 파일을 고쳤으면 고친 태스크를, 바뀐 게 없으면 `None`을 돌려준다.
pub fn link(
    vault: &Path,
    from: &Task,
    to: &Task,
    kind: &str,
    remove: bool,
    stamp: &Stamp,
    actor: &str,
) -> Result<Option<Task>, EngineError> {
    let Some(kind) = TaskLinkKind::parse(kind) else {
        return Err(EngineError::invalid_argument(format!(
            "type 은 [{}] 가운데 하나여야 한다: {kind}",
            TaskLinkKind::NAMES_TEXT
        )));
    };
    if from.key == to.key {
        return Err(EngineError::invalid_argument(format!(
            "자기 자신에게는 연결할 수 없다: {}",
            from.key
        )));
    }
    let forward = Link {
        kind,
        to: to.key.clone(),
    };
    let backward = Link {
        kind,
        to: from.key.clone(),
    };
    let (owner, links, event_kind): (&Task, Vec<Link>, &str);
    if remove {
        let (chosen, target) = if from.links.contains(&forward) {
            (from, &forward)
        } else if to.links.contains(&backward) {
            (to, &backward)
        } else {
            return Ok(None);
        };
        let mut list = chosen.links.clone();
        if let Some(at) = list.iter().position(|l| l == target) {
            list.remove(at);
        }
        owner = chosen;
        links = list;
        event_kind = "unlinked";
    } else {
        if from.links.contains(&forward) {
            return Ok(None);
        }
        if kind == TaskLinkKind::Relates && to.links.contains(&backward) {
            return Ok(None);
        }
        if kind == TaskLinkKind::Blocks
            && to.links.contains(&Link {
                kind: TaskLinkKind::Blocks,
                to: from.key.clone(),
            })
        {
            return Err(EngineError::invalid_argument(format!(
                "{} 가 이미 {} 를 막고 있어 서로 막게 할 수 없다",
                to.key, from.key
            )));
        }
        if from.links.len() >= MAX_LINKS {
            return Err(EngineError::invalid_argument(format!(
                "연결은 태스크마다 {MAX_LINKS}개까지다"
            )));
        }
        let mut list = from.links.clone();
        list.push(forward);
        owner = from;
        links = list;
        event_kind = "linked";
    }
    let next = Task {
        links,
        edited_at: Some(stamp.at.clone()),
        edited_session: stamp.session.clone(),
        edited_agent: stamp.agent.clone(),
        token: None,
        ..owner.clone()
    };
    let bytes = format::render(&next)?;
    write_atomic(&vault.join(&owner.rel_path), &bytes)?;
    for side in [from, to] {
        let other = if side.key == from.key {
            &to.key
        } else {
            &from.key
        };
        let direction = if side.key == owner.key { "out" } else { "in" };
        append_activity(
            vault,
            &side.key,
            event(
                stamp,
                actor,
                event_kind,
                vec![
                    ("type".to_string(), JsonValue::Str(kind.as_str().to_string())),
                    ("with".to_string(), JsonValue::Str(other.clone())),
                    (
                        "direction".to_string(),
                        JsonValue::Str(direction.to_string()),
                    ),
                ],
            ),
        )?;
    }
    Ok(Some(Task {
        token: Some(compute_token(&bytes)),
        ..next
    }))
}

// ---------------------------------------------------------------- 활동

fn event(stamp: &Stamp, actor: &str, kind: &str, extra: Vec<(String, JsonValue)>) -> JsonValue {
    let mut entries = vec![
        ("at".to_string(), JsonValue::Str(stamp.at.clone())),
        ("kind".to_string(), JsonValue::Str(kind.to_string())),
        ("by".to_string(), JsonValue::Str(actor.to_string())),
    ];
    if let Some(session) = &stamp.session {
        entries.push(("session".to_string(), JsonValue::Str(session.clone())));
    }
    if let Some(agent) = &stamp.agent {
        entries.push(("agent".to_string(), JsonValue::Str(agent.clone())));
    }
    entries.extend(extra);
    JsonValue::Object(entries)
}

/// 댓글을 활동에 남기고 그 사건을 돌려준다.
pub fn comment(
    vault: &Path,
    task: &Task,
    body: &str,
    stamp: &Stamp,
    actor: &str,
) -> Result<Json, EngineError> {
    let ev = event(
        stamp,
        actor,
        "comment",
        vec![("body".to_string(), JsonValue::Str(body.to_string()))],
    );
    append_activity(vault, &task.key, ev.clone())?;
    Ok(activity_json(&ev))
}

/// Java `Files.readAllLines`처럼 `\n`, `\r`, `\r\n`으로 줄을 가른다.
fn read_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let bytes = text.as_bytes();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                lines.push(&text[start..i]);
                i += 1;
                start = i;
            }
            b'\r' => {
                lines.push(&text[start..i]);
                i += if bytes.get(i + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
                start = i;
            }
            _ => i += 1,
        }
    }
    if start < bytes.len() {
        lines.push(&text[start..]);
    }
    lines
}

fn read_activity_text(vault: &Path, path: &Path) -> Result<String, EngineError> {
    fs::read(path)
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .ok_or_else(|| {
            EngineError::internal(format!("활동 기록을 못 읽었다: {}", rel(vault, path)))
        })
}

fn append_activity(vault: &Path, key: &str, ev: JsonValue) -> Result<(), EngineError> {
    let path = activity_path(vault, key);
    let mut lines: Vec<String> = Vec::new();
    if is_file(&path) {
        let text = read_activity_text(vault, &path)?;
        for line in read_lines(&text) {
            if !format::java_blank(line) {
                lines.push(line.to_string());
            }
        }
    }
    lines.push(json::compact(&ev));
    while lines.len() > ACTIVITY_KEEP {
        lines.remove(0);
    }
    write_atomic(&path, format!("{}\n", lines.join("\n")).as_bytes())
}

/// 활동 사건을 응답 값으로. 정수꼴 글(18자리까지)은 수로, 그 밖의 따옴표 없는 글은 글 그대로다(Java가 그렇다).
fn activity_json(value: &JsonValue) -> Json {
    match value {
        JsonValue::Str(s) => Json::String(s.clone()),
        JsonValue::Array(items) => Json::Array(items.iter().map(activity_json).collect()),
        JsonValue::Object(entries) => Json::Object(
            entries
                .iter()
                .map(|(k, v)| (k.clone(), activity_json(v)))
                .collect(),
        ),
        JsonValue::Literal(raw) => match raw.as_str() {
            "true" => Json::Bool(true),
            "false" => Json::Bool(false),
            "null" => Json::Null,
            text if is_long_text(text) => text
                .parse::<i64>()
                .map(Json::from)
                .unwrap_or_else(|_| Json::String(text.to_string())),
            text => Json::String(text.to_string()),
        },
    }
}

/// `-?\d{1,18}`
fn is_long_text(text: &str) -> bool {
    let digits = text.strip_prefix('-').unwrap_or(text);
    (1..=18).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit())
}

/// 활동을 오래된 것부터 읽는다. `limit`이 양수면 가장 최근 그만큼만 낸다. 못 읽는 줄은 건너뛴다.
pub fn activity(vault: &Path, key: &str, limit: usize) -> Result<Vec<Json>, EngineError> {
    let path = activity_path(vault, key);
    if !is_file(&path) {
        return Ok(Vec::new());
    }
    let text = read_activity_text(vault, &path)?;
    let mut out: Vec<Json> = Vec::new();
    for line in read_lines(&text) {
        if format::java_blank(line) {
            continue;
        }
        if let Ok(parsed @ JsonValue::Object(_)) = json::parse(line) {
            out.push(activity_json(&parsed));
        }
    }
    if limit > 0 && out.len() > limit {
        out.drain(..out.len() - limit);
    }
    Ok(out)
}

// ---------------------------------------------------------------- 지우기·되살리기

/// 태스크를 휴지통으로 보낸다(활동 기록도 함께). 되살릴 수 있다.
pub fn delete(vault: &Path, task: &Task) -> Result<(), EngineError> {
    let trash = root(vault).join(".trash").join(&task.id);
    if trash.exists() {
        return Err(EngineError::conflict(format!(
            "휴지통에 같은 id 가 이미 있다: {}",
            task.id
        )));
    }
    let src = vault.join(&task.rel_path);
    let activity_dir = root(vault).join(&task.key);
    let moved = (|| -> std::io::Result<()> {
        fs::create_dir_all(&trash)?;
        fs::rename(&src, trash.join("item.md"))?;
        if is_dir(&activity_dir) {
            fs::rename(&activity_dir, trash.join("activity"))?;
        }
        Ok(())
    })();
    if moved.is_err() {
        return Err(EngineError::internal(format!(
            "휴지통으로 옮기지 못했다: {}",
            task.rel_path
        )));
    }
    let meta = JsonValue::Object(vec![
        ("id".to_string(), JsonValue::Str(task.id.clone())),
        ("key".to_string(), JsonValue::Str(task.key.clone())),
        ("title".to_string(), JsonValue::Str(task.title.clone())),
        ("relPath".to_string(), JsonValue::Str(task.rel_path.clone())),
        (
            "deletedAt".to_string(),
            JsonValue::Str(crate::write::apparatus::instant_text(now_millis())),
        ),
    ]);
    write_atomic(
        &trash.join("meta.json"),
        format!("{}\n", json::compact(&meta)).as_bytes(),
    )
}

/// 휴지통의 태스크를 되살린다. 같은 키 자리가 이미 차 있으면 `AIS-409`다.
pub fn restore(vault: &Path, reference: &str) -> Result<Task, EngineError> {
    let entry = find_trash(vault, reference)?;
    let trash = root(vault).join(".trash").join(&entry.id);
    let target = vault.join(&entry.rel_path);
    if target.exists() {
        return Err(EngineError::conflict(format!(
            "제자리에 같은 키의 태스크가 이미 있어 되살리지 못했다: {}",
            entry.rel_path
        )));
    }
    let moved = (|| -> std::io::Result<()> {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(trash.join("item.md"), &target)?;
        let activity = trash.join("activity");
        if is_dir(&activity) {
            fs::rename(&activity, root(vault).join(&entry.key))?;
        }
        Ok(())
    })();
    if moved.is_err() {
        return Err(EngineError::internal(format!(
            "되살리지 못했다: {}",
            entry.rel_path
        )));
    }
    remove_tree(&trash)?;
    let scan = scan(vault);
    find(&scan, &entry.key).cloned()
}

/// 휴지통 목록. 지운 시각이 늦은 것이 앞이다.
pub fn trash(vault: &Path) -> Vec<TrashEntry> {
    let mut out: Vec<TrashEntry> = sorted_entries(&root(vault).join(".trash"))
        .iter()
        .filter_map(|dir| read_trash_entry(dir))
        .collect();
    out.sort_by(|a, b| compare_utf16(&b.deleted_at, &a.deleted_at));
    out
}

fn find_trash(vault: &Path, reference: &str) -> Result<TrashEntry, EngineError> {
    trash(vault)
        .into_iter()
        .find(|e| e.id == reference || e.key == reference)
        .ok_or_else(|| EngineError::not_found(format!("휴지통에서 못 찾았다: {reference}")))
}

fn read_trash_entry(dir: &Path) -> Option<TrashEntry> {
    let meta = dir.join("meta.json");
    if !is_file(&meta) {
        return None;
    }
    let text = String::from_utf8(fs::read(&meta).ok()?).ok()?;
    let JsonValue::Object(entries) = json::parse(&text).ok()? else {
        return None;
    };
    let get = |key: &str| {
        entries
            .iter()
            .find(|(k, _)| k == key)
            .and_then(|(_, v)| match v {
                JsonValue::Str(s) => Some(s.clone()),
                _ => None,
            })
    };
    let (id, key, rel_path) = (get("id")?, get("key")?, get("relPath")?);
    if !format::ID.is_match(&id) || !KEY.is_match(&key) {
        return None;
    }
    Some(TrashEntry {
        id,
        key,
        title: get("title").unwrap_or_else(|| "null".to_string()),
        rel_path,
        deleted_at: get("deletedAt").unwrap_or_else(|| "null".to_string()),
    })
}
