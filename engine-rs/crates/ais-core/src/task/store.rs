//! 태스크 파일을 vault에서 읽고 쓰는 곳. 파일에 기록하는 일시는 태스크 모델에서 검사한다.
//!
//! ```text
//! .aisurada/tasks/
//!   TSK-14.md                  태스크(머리말 + 설명)
//!   TSK-14/activity.jsonl      상세 활동 최근 500줄과 오늘 집계에 쓰는 매니저 댓글
//!   .trash/<id>/               지운 태스크
//! ```
//!
//! 연결은 건 쪽 파일에만 적고 반대쪽(막혀 있음 따위)은 읽을 때 계산하므로 두 파일이 어긋날 일이 없다.

use crate::ports::{clock, fs};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::Value as Json;

use super::aggregate::{Fields, NewTask, Scan, TaskEvent, unfinished_blockers};
use super::format::{self, KEY, MAX_FILE_BYTES};
use super::{Task, TaskStatus};
use crate::error::EngineError;
use crate::json::{self, JsonValue};
use crate::kernel::{actor::ManagerContext, stamp::Stamp, time::parse_instant};
use crate::kernel::token::compute_token;
use crate::text::compare_utf16;
use crate::vault::project::scan_projects;
use crate::write::apparatus::{next_id, now_millis};
use crate::write::atomic::write_atomic;
use crate::write::sub_note::remove_tree;

pub const ROOT: &str = ".aisurada/tasks";
pub const ACTIVITY_KEEP: usize = 500;
const RECENT_WORK_MILLIS: i64 = 48 * 60 * 60 * 1000;

pub struct TrashEntry {
    pub id: String,
    pub key: String,
    pub title: String,
    pub rel_path: String,
    pub deleted_at: String,
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

fn read_task(file: &Path, rel_path: &str) -> Result<Task, String> {
    if fs::metadata(file).is_ok_and(|m| m.len() > MAX_FILE_BYTES as u64) {
        return Err("파일이 너무 크다".to_string());
    }
    let bytes = fs::read(file).map_err(|_| "파일을 못 읽었다".to_string())?;
    format::parse(&bytes, rel_path)
}

/// 키에 해당하는 활성 태스크 파일 하나를 읽는다.
pub(crate) fn read_by_key(vault: &Path, key: &str) -> Option<Task> {
    if !KEY.is_match(key) {
        return None;
    }
    let path = file_path(vault, key);
    read_task(&path, &rel(vault, &path)).ok().filter(|task| task.key == key)
}

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
        match read_task(&file, &rel_path) {
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

/// 새 태스크를 Draft로 만든다. 요약(title), 저장소(repo), 완료 기준(criteria 1개 이상)이 필수다(규칙은 `NewTask`가 검사한다).
pub fn create(
    vault: &Path,
    new: NewTask,
    stamp: &Stamp,
    actor: &str,
    receipt: Option<&str>,
    manager: Option<&ManagerContext>,
) -> Result<Task, EngineError> {
    if let Some(project) = &new.project {
        require_project(vault, project)?;
    }

    let scan = scan(vault);
    let number = next_number(vault, &scan)?;
    let key = format!("TSK-{number}");
    let id = loop {
        let candidate = next_id("tk");
        if scan.by_id(&candidate).is_none() {
            break candidate;
        }
    };
    let path = file_path(vault, &key);
    let (mut task, mut event) = new.into_task(key.clone(), id, rel(vault, &path), stamp, actor);
    if let Some(receipt) = receipt {
        task.set_intake_receipt(receipt);
    }
    if let Some(manager) = manager {
        task.record_manager_project(&manager.project_id);
        event.record_manager_project(&manager.project_id);
    }
    let bytes = format::render(&task)?;
    write_atomic(&path, &bytes)?;
    append_activity(vault, &key, &event)?;
    Ok(Task {
        token: Some(compute_token(&bytes)),
        ..task
    })
}

/// 이미 쓰인 번호보다 큰 번호를 고르고, 읽기에서 건너뛴 파일의 자리도 피한다.
fn next_number(vault: &Path, scan: &Scan) -> Result<u32, EngineError> {
    let mut number = scan.items.iter().map(Task::number).max().unwrap_or(0);
    for entry in trash(vault) {
        number = number.max(format::number_of(&entry.key));
    }
    loop {
        number = number
            .checked_add(1)
            .ok_or_else(|| EngineError::conflict("쓸 수 있는 태스크 번호가 없다"))?;
        let path = file_path(vault, &format!("TSK-{number}"));
        match fs::symlink_metadata(&path) {
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(number),
            Err(_) => {
                return Err(EngineError::internal(format!(
                    "태스크 파일 자리를 확인하지 못했다: {}",
                    rel(vault, &path)
                )));
            }
        }
    }
}

// ---------------------------------------------------------------- 고치기

/// 태스크의 내용을 고친다(규칙은 `Task::edit`). 달라지는 게 없으면 파일도 활동도 안 바꾸고 `None`을 돌려준다.
pub fn update(
    vault: &Path,
    cur: &Task,
    f: Fields,
    f1: Option<&str>,
    stamp: &Stamp,
    actor: &str,
) -> Result<Option<Task>, EngineError> {
    let Some(changed) = cur.edit(f, f1, stamp, actor, |project| {
        require_project(vault, project)
    })?
    else {
        return Ok(None);
    };
    let bytes = format::render(&changed.next)?;
    write_atomic(&vault.join(&cur.rel_path), &bytes)?;
    append_activity(vault, &cur.key, &changed.event)?;
    Ok(Some(Task {
        token: Some(compute_token(&bytes)),
        ..changed.next
    }))
}

// ---------------------------------------------------------------- 상태

/// 상태를 바꾼다(규칙은 `Task::transition_to`). 막고 있는 태스크는 기대한 상태를 검사한 뒤에 vault를 다시 훑어 구한다.
pub fn transition(
    vault: &Path,
    cur: &Task,
    to: TaskStatus,
    actor: &str,
    expected_from: Option<&str>,
    stamp: &Stamp,
) -> Result<Task, EngineError> {
    let changed = cur.transition_to(
        to,
        actor,
        expected_from,
        || unfinished_blockers(&scan(vault), &cur.key),
        stamp,
    )?;
    let bytes = format::render(&changed.next)?;
    write_atomic(&vault.join(&cur.rel_path), &bytes)?;
    append_activity(vault, &cur.key, &changed.event)?;
    Ok(Task {
        token: Some(compute_token(&bytes)),
        ..changed.next
    })
}

// ---------------------------------------------------------------- 연결

/// 연결을 걸거나 푼다(규칙은 `Task::link`). 파일은 연결을 적는 쪽에 쓰고, 사건은 두 태스크의 활동에 붙인다.
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
    let Some(change) = Task::link(from, to, kind, remove, stamp, actor)? else {
        return Ok(None);
    };
    let bytes = format::render(&change.next)?;
    write_atomic(&vault.join(&change.next.rel_path), &bytes)?;
    for (key, event) in &change.events {
        append_activity(vault, key, event)?;
    }
    Ok(Some(Task {
        token: Some(compute_token(&bytes)),
        ..change.next
    }))
}

// ---------------------------------------------------------------- 활동

/// 댓글을 활동에 남기고 그 사건을 돌려준다.
pub fn comment(
    vault: &Path,
    task: &Task,
    body: &str,
    stamp: &Stamp,
    actor: &str,
    manager: Option<&ManagerContext>,
) -> Result<Json, EngineError> {
    let mut event = task.comment(body, stamp, actor);
    if let Some(manager) = manager {
        event.record_manager_project(&manager.project_id);
    }
    append_activity(vault, &task.key, &event)?;
    Ok(activity_json(&event.to_json_value()))
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

fn append_activity(vault: &Path, key: &str, event: &TaskEvent) -> Result<(), EngineError> {
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
    lines.push(json::compact(&event.to_json_value()));
    if lines.len() > ACTIVITY_KEEP {
        let recent = lines.split_off(lines.len() - ACTIVITY_KEEP);
        let cutoff = now_millis() - RECENT_WORK_MILLIS;
        // 상세 활동 기록이 잘려도 오늘 매니저가 남긴 댓글은 계속 집계할 수 있다.
        lines.retain(|line| {
            let Some(event) = activity_line(line) else {
                return false;
            };
            manager_comment(&event).is_some_and(|(_, at)| at >= cutoff)
        });
        lines.extend(recent);
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
    read_activity(vault, &activity_path(vault, key), limit)
}

fn read_activity(vault: &Path, path: &Path, limit: usize) -> Result<Vec<Json>, EngineError> {
    if !is_file(path) {
        return Ok(Vec::new());
    }
    let text = read_activity_text(vault, path)?;
    let mut out: Vec<Json> = read_lines(&text).into_iter().filter_map(activity_line).collect();
    if limit > 0 && out.len() > limit {
        out.drain(..out.len() - limit);
    }
    Ok(out)
}

fn activity_line(line: &str) -> Option<Json> {
    let parsed @ JsonValue::Object(_) = json::parse(line).ok()? else {
        return None;
    };
    Some(activity_json(&parsed))
}

fn work_time(value: &str) -> Option<i64> {
    i64::try_from(parse_instant(value)?.div_euclid(1_000_000)).ok()
}

fn manager_comment(event: &Json) -> Option<(&str, i64)> {
    if event["kind"] != "comment" {
        return None;
    }
    Some((event["manager"].as_str()?, work_time(event["at"].as_str()?)?))
}

struct ManagerWork<'a> {
    manager: &'a ManagerContext,
    tasks: usize,
    comments: usize,
}

impl ManagerWork<'_> {
    fn matches_project(&self, task: &Task, recorded_id: Option<&str>) -> bool {
        recorded_id.map_or_else(
            || task.project.as_deref() == Some(self.manager.project.as_str()),
            |id| id == self.manager.project_id,
        )
    }
}

pub fn manager_work_today(vault: &Path, managers: &[ManagerContext]) -> Result<Json, EngineError> {
    if managers.is_empty() {
        return Ok(serde_json::json!({}));
    }
    let mut work: BTreeMap<_, _> = managers.iter().map(|manager| (
        manager.name.as_str(),
        ManagerWork { manager, tasks: 0, comments: 0 },
    )).collect();
    let clock = clock();
    let now = clock.now_millis();
    let today = clock.local_clock(now).date;
    let is_today = |at: i64| {
        (now - RECENT_WORK_MILLIS..=now).contains(&at) && clock.local_clock(at).date == today
    };
    let mut count = |task: &Task, activity: &Path| -> Result<(), EngineError> {
        if let Some(row) = task.manager.as_deref().and_then(|name| work.get_mut(name))
            && row.matches_project(task, task.manager_project_id())
            && work_time(&task.created_at).is_some_and(is_today)
        {
            row.tasks += 1;
        }
        for event in read_activity(vault, activity, 0)? {
            if let Some((manager, at)) = manager_comment(&event)
                && let Some(row) = work.get_mut(manager)
                && row.matches_project(task, event["managerProjectId"].as_str())
                && is_today(at)
            {
                row.comments += 1;
            }
        }
        Ok(())
    };

    let scan = scan(vault);
    for task in &scan.items {
        count(task, &activity_path(vault, &task.key))?;
    }
    for deleted in trash(vault) {
        if scan.by_id(&deleted.id).is_some() {
            continue;
        }
        let folder = root(vault).join(".trash").join(&deleted.id);
        if let Ok(task) = read_task(&folder.join("item.md"), &deleted.rel_path)
            && task.id == deleted.id
        {
            count(&task, &folder.join("activity/activity.jsonl"))?;
        }
    }
    Ok(Json::Object(work.into_iter().map(|(name, row)| (
        name.into(), serde_json::json!({"tasks": row.tasks, "comments": row.comments}),
    )).collect()))
}

// ---------------------------------------------------------------- 지우기·되살리기

/// 태스크를 휴지통으로 보낸다(활동 기록도 함께). 되살릴 수 있다.
pub fn delete(vault: &Path, task: &Task) -> Result<(), EngineError> {
    let trash = root(vault).join(".trash").join(&task.id);
    if fs::exists(&trash) {
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
    if fs::exists(&target) {
        return Err(EngineError::conflict(format!(
            "제자리에 같은 키의 태스크가 이미 있어 되살리지 못했다: {}",
            entry.rel_path
        )));
    }
    let item = trash.join("item.md");
    read_task(&item, &entry.rel_path).map_err(|reason| {
        EngineError::conflict(format!(
            "태스크 파일을 읽을 수 없어 되살리지 못했다: {}: {reason}",
            entry.rel_path
        ))
    })?;
    let moved = (|| -> std::io::Result<()> {
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&item, &target)?;
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
