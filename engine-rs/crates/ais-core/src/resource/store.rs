//! 자원 파일을 vault에서 읽고 쓰는 곳. 외부 설명서에 없는 확장이다.
//!
//! ```text
//! .aisurada/resources/
//!   global/{skills/<slug>/SKILL.md, agents/<slug>.md}     글로벌 자원
//!   projects/<프로젝트 slug>/{skills/…, agents/…, refs.md}   프로젝트 자원과 참조 목록
//!   .trash/<id>/   지운 자원      .history/<id>/   고치기 전의 판
//! ```
//!
//! 설명서가 정한 `notes/`와 `.aisurada/projects/` 등을 건드리지 않는 새 영역이다. 자원의 범위는 파일 위치로 정해지고,
//! 이름을 바꿔도 `id`는 머리말 안에 있어서 참조가 끊기지 않는다.

use std::collections::HashSet;
use crate::ports::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use super::{Resource, ResourceKind, approvals, format, refs};
use crate::error::EngineError;
use crate::frontmatter::Value;
use crate::json::{self, JsonValue};
use crate::kernel::token::compute_token;
use crate::text::compare_utf16;
use crate::vault::project::{ProjectInfo, scan_projects};
use crate::write::apparatus::{instant_text, next_id, now_millis, to_base36};
use crate::write::atomic::write_atomic;
pub use crate::write::stamp::Stamp;
use crate::write::sub_note::remove_tree;

pub const ROOT: &str = ".aisurada/resources";
pub const HISTORY_KEEP: usize = 30;

static VERSION_SHAPE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[0-9a-z]+(-[0-9]+)?$").unwrap());

/// 훑은 결과. `skipped`는 못 읽어 건너뛴 파일과 이유다.
pub struct Scan {
    pub items: Vec<Resource>,
    pub skipped: Vec<String>,
}

impl Scan {
    pub fn by_id(&self, id: &str) -> Option<&Resource> {
        self.items.iter().find(|r| r.id == id)
    }
}

/// 고칠 값. `None`은 「안 줌(그대로 둔다)」이다. 글 칸의 빈 값은 `repo`만 「지운다」이고, `skills`와 `tools`는 빈 목록·빈 맵이 「비운다」이다.
#[derive(Default, Clone)]
pub struct Fields {
    pub name: Option<String>,
    pub description: Option<String>,
    pub body: Option<String>,
    pub permission: Option<String>,
    pub skills: Option<Vec<String>>,
    pub repo: Option<String>,
    pub tools: Option<Vec<(String, Value)>>,
}

/// 지운 자원 한 줄
pub struct TrashEntry {
    pub id: String,
    pub kind: ResourceKind,
    pub project: Option<String>,
    pub name: Option<String>,
    pub rel_path: String,
    pub deleted_at: String,
}

/// 고치기 전의 판 한 줄. 새것이 앞이다.
pub struct Version {
    pub version: String,
    pub saved_at: i64,
    pub bytes: usize,
    pub edited_at: Option<String>,
    pub edited_session: Option<String>,
}

/// 쓰기 전에 계산해 둔 변경. `before`는 고치기 전의 판(만들기면 `None`), `after`는 쓸 판(아직 파일에 쓰지 않았고 token은 `None`)이다.
pub struct Plan {
    pub before: Option<Resource>,
    pub after: Resource,
    pub changed: bool,
}

// ---------------------------------------------------------------- 경로

pub fn root(vault: &Path) -> PathBuf {
    vault.join(ROOT)
}

/// 범위 폴더. `project`가 `None`이면 글로벌이다.
pub fn scope_dir(vault: &Path, project: Option<&str>) -> PathBuf {
    match project {
        None => root(vault).join("global"),
        Some(p) => root(vault).join("projects").join(p),
    }
}

fn entry_path(vault: &Path, kind: ResourceKind, project: Option<&str>, slug: &str) -> PathBuf {
    let dir = scope_dir(vault, project).join(kind.dir());
    match kind {
        ResourceKind::Skill => dir.join(slug).join("SKILL.md"),
        ResourceKind::Agent => dir.join(format!("{slug}.md")),
    }
}

/// vault 기준 슬래시 경로 글
pub fn rel(vault: &Path, file: &Path) -> String {
    file.strip_prefix(vault)
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

pub fn sorted_entries(dir: &Path) -> Vec<PathBuf> {
    let Ok(read) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = read.flatten().map(|e| e.path()).collect();
    out.sort_by(|a, b| compare_utf16(&file_name(a), &file_name(b)));
    out
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

pub fn is_file(path: &Path) -> bool {
    fs::metadata(path).map(|m| m.is_file()).unwrap_or(false)
}

pub fn is_dir(path: &Path) -> bool {
    fs::metadata(path).map(|m| m.is_dir()).unwrap_or(false)
}

/// 자원 파일의 폴더·파일 이름(slug). 스킬은 폴더 이름, 에이전트는 파일 이름이다.
pub fn slug_of(r: &Resource) -> String {
    let rel = &r.rel_path;
    if r.kind == ResourceKind::Skill {
        let parent = &rel[..rel.rfind('/').unwrap_or(0)];
        return parent[parent.rfind('/').map_or(0, |i| i + 1)..].to_string();
    }
    let file = &rel[rel.rfind('/').map_or(0, |i| i + 1)..];
    file[..file.len().saturating_sub(3)].to_string()
}

// ---------------------------------------------------------------- 읽기

/// 모든 범위의 자원을 훑는다. 못 읽는 파일은 건너뛰고 이유를 `skipped`에 적는다. 점으로 시작하는 폴더(`.trash` 등)는 안 본다.
pub fn scan(vault: &Path) -> Scan {
    let mut items: Vec<Resource> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut ids: HashSet<String> = HashSet::new();
    scan_scope(
        vault,
        None,
        &scope_dir(vault, None),
        &mut items,
        &mut skipped,
        &mut ids,
    );
    for dir in sorted_entries(&root(vault).join("projects")) {
        let name = file_name(&dir);
        if name.starts_with('.') || !is_dir(&dir) {
            continue;
        }
        scan_scope(vault, Some(&name), &dir, &mut items, &mut skipped, &mut ids);
    }
    Scan { items, skipped }
}

fn scan_scope(
    vault: &Path,
    project: Option<&str>,
    scope: &Path,
    items: &mut Vec<Resource>,
    skipped: &mut Vec<String>,
    ids: &mut HashSet<String>,
) {
    for kind in ResourceKind::ALL {
        for entry in sorted_entries(&scope.join(kind.dir())) {
            let name = file_name(&entry);
            if name.starts_with('.') {
                continue;
            }
            let file = if kind == ResourceKind::Skill {
                if !is_dir(&entry) {
                    continue;
                }
                entry.join("SKILL.md")
            } else {
                if !name.ends_with(".md") {
                    continue;
                }
                entry
            };
            if !is_file(&file) {
                continue;
            }
            let rel_path = rel(vault, &file);
            let Ok(meta) = fs::metadata(&file) else {
                skipped.push(format!("{rel_path}: 파일을 못 읽었다"));
                continue;
            };
            if meta.len() > format::MAX_FILE_BYTES as u64 {
                skipped.push(format!("{rel_path}: 파일이 너무 크다"));
                continue;
            }
            let Ok(bytes) = fs::read(&file) else {
                skipped.push(format!("{rel_path}: 파일을 못 읽었다"));
                continue;
            };
            match format::parse(&bytes, kind, project, &rel_path) {
                Ok(r) => {
                    if !ids.insert(r.id.clone()) {
                        skipped.push(format!("{rel_path}: id 가 다른 자원과 겹친다: {}", r.id));
                        continue;
                    }
                    items.push(r);
                }
                Err(message) => skipped.push(format!("{rel_path}: {message}")),
            }
        }
    }
}

/// 승인한 판으로 본 훑기. 화면 목록, AI가 읽는 조회, 실행에 내려 주는 입력은 모두 이 결과만 써야 한다.
/// 지금 파일의 지문이 승인 기록과 맞으면 그 파일을, 어긋났으면(`dirty`) 승인해 둔 사본을 쓴다. 승인 기록이 없는 파일은 `untrusted`로
/// 따로 모으고 `items`에는 넣지 않는다.
pub struct Served {
    pub items: Vec<Resource>,
    pub skipped: Vec<String>,
    /// id별 상태(`approved` 또는 `dirty`)
    pub states: Vec<(String, &'static str)>,
    pub untrusted: Vec<Resource>,
}

impl Served {
    pub fn by_id(&self, id: &str) -> Option<&Resource> {
        self.items.iter().find(|r| r.id == id)
    }

    pub fn state_of(&self, id: &str) -> Option<&'static str> {
        self.states.iter().find(|(k, _)| k == id).map(|(_, s)| *s)
    }
}

pub fn scan_served(vault: &Path) -> Result<Served, EngineError> {
    let raw = scan(vault);
    let ledger = approvals::ledger(vault)?;
    let mut items: Vec<Resource> = Vec::new();
    let mut untrusted: Vec<Resource> = Vec::new();
    let mut states: Vec<(String, &'static str)> = Vec::new();
    for r in raw.items {
        match ledger.get(&approvals::res_key(&r.id)) {
            None => untrusted.push(r),
            Some(e) if Some(&e.token) == r.token.as_ref() => {
                states.push((r.id.clone(), "approved"));
                items.push(r);
            }
            Some(e) => match approved_copy(vault, &r, &e.token) {
                None => untrusted.push(r),
                Some(approved) => {
                    states.push((r.id.clone(), "dirty"));
                    items.push(approved);
                }
            },
        }
    }
    Ok(Served {
        items,
        skipped: raw.skipped,
        states,
        untrusted,
    })
}

/// 승인해 둔 사본을 자원으로 읽는다. 사본이 없거나 지문이 기록과 안 맞거나 못 읽으면 `None`이다.
fn approved_copy(vault: &Path, live: &Resource, token: &str) -> Option<Resource> {
    let copy = approvals::copy(vault, &approvals::res_key(&live.id))?;
    if compute_token(&copy) != token {
        return None;
    }
    format::parse(&copy, live.kind, live.project.as_deref(), &live.rel_path).ok()
}

/// 고칠 바탕이 되는 판. 승인한 판과 같으면 파일 그대로이고, 직접 고쳐졌으면 승인해 둔 사본(토큰만 지금 파일의 것)이다.
/// 승인 기록이 없는 파일은 고칠 수 없다(`AIS-409`): 먼저 승인하거나 지워야 한다.
pub fn base_for_edit(vault: &Path, live: &Resource) -> Result<Resource, EngineError> {
    let ledger = approvals::ledger(vault)?;
    let Some(e) = ledger.get(&approvals::res_key(&live.id)) else {
        return Err(EngineError::conflict(format!(
            "승인되지 않은 새 파일이라 고칠 수 없다. 승인하거나 지운 뒤 다시 한다(resolve-resource-edit): {}",
            live.rel_path
        )));
    };
    if Some(&e.token) == live.token.as_ref() {
        return Ok(live.clone());
    }
    let Some(approved) = approved_copy(vault, live, &e.token) else {
        return Err(EngineError::conflict(format!(
            "승인해 둔 판을 찾지 못해 고칠 수 없다. 승인하거나 지운 뒤 다시 한다(resolve-resource-edit): {}",
            live.rel_path
        )));
    };
    Ok(Resource {
        rel_path: live.rel_path.clone(),
        token: live.token.clone(),
        ..approved
    })
}

/// 승인 상태. 승인 기록이 없으면 `new`, 지문이 안 맞으면 `dirty`, 맞으면 `approved`다.
pub fn approval_state(vault: &Path, live: &Resource) -> Result<&'static str, EngineError> {
    let ledger = approvals::ledger(vault)?;
    Ok(match ledger.get(&approvals::res_key(&live.id)) {
        None => "new",
        Some(e) if Some(&e.token) == live.token.as_ref() => "approved",
        Some(_) => "dirty",
    })
}

/// 직접 고쳐진 파일을 승인해 둔 판으로 되돌린다. 고쳐진 내용은 이력에 남긴다. 승인해 둔 사본이 없거나 지문이 안 맞으면 `AIS-409`다.
pub fn revert_to_approved(vault: &Path, live: &Resource) -> Result<(), EngineError> {
    let ledger = approvals::ledger(vault)?;
    let key = approvals::res_key(&live.id);
    let copy = ledger
        .get(&key)
        .and_then(|e| approvals::copy(vault, &key).map(|c| (e, c)))
        .filter(|(e, c)| compute_token(c) == e.token);
    let Some((_, copy)) = copy else {
        return Err(EngineError::conflict(format!(
            "승인해 둔 판을 찾지 못해 되돌릴 수 없다: {}",
            live.rel_path
        )));
    };
    snapshot(vault, &live.id, &live_bytes(vault, live)?)?;
    write_atomic(&vault.join(&live.rel_path), &copy)
}

/// 참조 목록 파일을 주어진 바이트로 되돌린다. 빈 배열이면 파일을 지운다.
pub fn restore_refs_bytes(vault: &Path, project: &str, bytes: &[u8]) -> Result<(), EngineError> {
    let f = refs::file(vault, project);
    if bytes.is_empty() {
        return refs::remove_file(vault, &f);
    }
    write_atomic(&f, bytes)
}

/// 자원 파일의 지금 바이트
pub fn live_bytes(vault: &Path, r: &Resource) -> Result<Vec<u8>, EngineError> {
    fs::read(vault.join(&r.rel_path))
        .map_err(|_| EngineError::internal(format!("자원 파일을 못 읽었다: {}", r.rel_path)))
}

/// ID로 자원 하나를 찾는다. 없으면 `AIS-404`다.
pub fn find(vault: &Path, id: &str) -> Result<Resource, EngineError> {
    if !ResourceKind::is_id(id) {
        return Err(EngineError::invalid_argument(format!(
            "자원 id 모양이 아니다(sk… 또는 ap…): {id}"
        )));
    }
    scan(vault)
        .items
        .into_iter()
        .find(|r| r.id == id)
        .ok_or_else(|| EngineError::not_found(format!("자원을 못 찾았다: {id}")))
}

/// 프로젝트 slug가 실제 프로젝트인지 확인한다. 아니면 `AIS-404`다.
pub fn require_project(vault: &Path, slug: &str) -> Result<ProjectInfo, EngineError> {
    if crate::kernel::java::java_blank(slug) {
        return Err(EngineError::invalid_argument("프로젝트 slug 가 필요하다"));
    }
    scan_projects(vault)
        .into_iter()
        .find(|p| p.slug == slug)
        .ok_or_else(|| EngineError::not_found(format!("프로젝트를 못 찾았다: {slug}")))
}

// ---------------------------------------------------------------- 만들기

/// 새 자원을 검사하고 쓸 판을 계산한다(파일은 안 쓴다). `forced_id`가 있고 비어 있으면 그 ID를 쓴다(승인된 대기 변경을 적용할 때,
/// 대기에 올릴 때 보여 준 ID 그대로 만들기 위해서다).
pub fn plan_create(
    vault: &Path,
    kind: ResourceKind,
    project: Option<&str>,
    f: &Fields,
    stamp: &Stamp,
    forced_id: Option<&str>,
) -> Result<Plan, EngineError> {
    if let Some(p) = project {
        require_project(vault, p)?;
    }
    let Some(name) = &f.name else {
        return Err(EngineError::invalid_argument("name 이 필요하다"));
    };
    let Some(description) = &f.description else {
        return Err(EngineError::invalid_argument("description 이 필요하다"));
    };
    let scan = scan(vault);
    let slug = format::slug_of(name)?;
    ensure_free(&scan, kind, project, &slug, None)?;

    let (mut permission, mut skills, mut repo) = (None, Vec::new(), None);
    if kind == ResourceKind::Agent {
        permission = Some(f.permission.clone().unwrap_or_else(|| "read".to_string()));
        skills = f.skills.clone().unwrap_or_default();
        repo = f.repo.clone().filter(|r| !r.is_empty());
        validate_agent_links(vault, &scan, project, &skills, repo.as_deref())?;
    }
    let id = match forced_id {
        Some(forced)
            if ResourceKind::of_id(forced) == Some(kind) && scan.by_id(forced).is_none() =>
        {
            forced.to_string()
        }
        _ => loop {
            let candidate = next_id(kind.id_prefix());
            if scan.by_id(&candidate).is_none() {
                break candidate;
            }
        },
    };
    let path = entry_path(vault, kind, project, &slug);
    let after = Resource {
        id,
        kind,
        project: project.map(str::to_string),
        name: name.clone(),
        description: description.clone(),
        permission,
        skills,
        repo,
        tools: f.tools.clone().unwrap_or_default(),
        body: f.body.clone().unwrap_or_default(),
        edited_at: Some(stamp.at.clone()),
        edited_session: stamp.session.clone(),
        edited_agent: stamp.agent.clone(),
        rel_path: rel(vault, &path),
        token: None,
    };
    Ok(Plan {
        before: None,
        after,
        changed: true,
    })
}

/// 계산해 둔 새 자원을 파일에 쓴다.
pub fn commit_create(vault: &Path, plan: &Plan) -> Result<Resource, EngineError> {
    let r = &plan.after;
    let bytes = format::render(r)?;
    write_atomic(&vault.join(&r.rel_path), &bytes)?;
    Ok(Resource {
        token: Some(compute_token(&bytes)),
        ..r.clone()
    })
}

// ---------------------------------------------------------------- 고치기

/// 고칠 값을 검사하고 쓸 판을 계산한다(파일은 안 쓴다). `cur`는 고칠 바탕이 되는 판이고, 그 `token`은 지금 파일의 토큰이어야 한다.
/// 바탕이 파일의 지금 모습이 아니라 승인해 둔 사본일 수 있다(직접 고쳐진 파일은 그 내용을 이어받지 않는다).
pub fn plan_update(
    vault: &Path,
    cur: &Resource,
    f: &Fields,
    f1: Option<&str>,
    stamp: &Stamp,
) -> Result<Plan, EngineError> {
    let Some(f1) = f1.filter(|t| !crate::kernel::java::java_blank(t)) else {
        return Err(EngineError::precondition_failed(format!(
            "f1 이 필요하다. get-resource 로 읽은 precondition 을 f1= 으로 넘긴다: {}",
            cur.rel_path
        )));
    };
    if Some(crate::kernel::java::java_trim(f1)) != cur.token.as_deref() {
        return Err(EngineError::precondition_failed(format!(
            "읽은 뒤에 파일이 바뀌었다. 다시 읽고 고친다: {}",
            cur.rel_path
        )));
    }
    let scan = scan(vault);
    let name = f.name.clone().unwrap_or_else(|| cur.name.clone());
    let description = f
        .description
        .clone()
        .unwrap_or_else(|| cur.description.clone());
    let body = f.body.clone().unwrap_or_else(|| cur.body.clone());
    let tools = f.tools.clone().unwrap_or_else(|| cur.tools.clone());

    let mut permission = cur.permission.clone();
    let mut skills = cur.skills.clone();
    let mut repo = cur.repo.clone();
    if cur.kind == ResourceKind::Agent {
        if let Some(p) = &f.permission {
            permission = Some(p.clone());
        }
        if let Some(s) = &f.skills {
            skills = s.clone();
        }
        if let Some(r) = &f.repo {
            repo = if r.is_empty() { None } else { Some(r.clone()) };
        }
        if f.skills.is_some() || f.repo.is_some() {
            validate_agent_links(
                vault,
                &scan,
                cur.project.as_deref(),
                &skills,
                repo.as_deref(),
            )?;
        }
    }

    let old_slug = slug_of(cur);
    let new_slug = format::slug_of(&name)?;
    if new_slug != old_slug {
        ensure_free(
            &scan,
            cur.kind,
            cur.project.as_deref(),
            &new_slug,
            Some(&cur.id),
        )?;
    }

    let same = Resource {
        name: name.clone(),
        description: description.clone(),
        permission: permission.clone(),
        skills: skills.clone(),
        repo: repo.clone(),
        tools: tools.clone(),
        body: body.clone(),
        ..cur.clone()
    };
    if new_slug == old_slug && format::render(&same)? == format::render(cur)? {
        return Ok(Plan {
            before: Some(cur.clone()),
            after: cur.clone(),
            changed: false,
        });
    }
    let new_path = entry_path(vault, cur.kind, cur.project.as_deref(), &new_slug);
    let next = Resource {
        id: cur.id.clone(),
        kind: cur.kind,
        project: cur.project.clone(),
        name,
        description,
        permission,
        skills,
        repo,
        tools,
        body,
        edited_at: Some(stamp.at.clone()),
        edited_session: stamp.session.clone(),
        edited_agent: stamp.agent.clone(),
        rel_path: rel(vault, &new_path),
        token: None,
    };
    Ok(Plan {
        before: Some(cur.clone()),
        after: next,
        changed: true,
    })
}

/// 계산해 둔 고침을 쓴다. 파일의 지금 모습은 이력에 남긴다. 이름이 바뀌었으면 파일(스킬은 폴더)을 옮긴다.
pub fn commit_update(vault: &Path, plan: &Plan) -> Result<Resource, EngineError> {
    let cur = plan.before.as_ref().expect("고침에는 예전 판이 있다");
    let next = &plan.after;
    let old_path = vault.join(&cur.rel_path);
    let old_bytes = fs::read(&old_path)
        .map_err(|_| EngineError::internal(format!("자원 파일을 못 읽었다: {}", cur.rel_path)))?;
    snapshot(vault, &cur.id, &old_bytes)?;

    let new_path = vault.join(&next.rel_path);
    if slug_of(next) != slug_of(cur) {
        let (from, to) = if cur.kind == ResourceKind::Skill {
            (old_path.parent(), new_path.parent())
        } else {
            (Some(old_path.as_path()), Some(new_path.as_path()))
        };
        let moved = match (from, to) {
            // `Files.move`는 자리가 이미 차 있으면 실패한다
            (Some(from), Some(to)) if fs::symlink_metadata(to).is_err() => {
                fs::rename(from, to).is_ok()
            }
            _ => false,
        };
        if !moved {
            return Err(EngineError::internal(format!(
                "이름을 바꾸며 옮기지 못했다: {}",
                cur.rel_path
            )));
        }
    }
    let bytes = format::render(next)?;
    write_atomic(&new_path, &bytes)?;
    Ok(Resource {
        token: Some(compute_token(&bytes)),
        ..next.clone()
    })
}

fn ensure_free(
    scan: &Scan,
    kind: ResourceKind,
    project: Option<&str>,
    slug: &str,
    except_id: Option<&str>,
) -> Result<(), EngineError> {
    for r in &scan.items {
        if r.kind != kind || r.project.as_deref() != project || Some(r.id.as_str()) == except_id {
            continue;
        }
        if slug_of(r) == slug {
            return Err(EngineError::invalid_argument(format!(
                "같은 이름의 {}이 이 범위에 이미 있다: {}",
                if kind == ResourceKind::Skill {
                    "스킬"
                } else {
                    "에이전트"
                },
                r.name
            )));
        }
    }
    Ok(())
}

/// 에이전트가 쓸 스킬과 저장소를 검사한다. 글로벌 에이전트는 글로벌 스킬만 쓸 수 있고(프로젝트 것에 기대면 다른 프로젝트에서 깨진다),
/// 프로젝트 에이전트는 글로벌 스킬과 자기 프로젝트의 스킬을 쓸 수 있다. 저장소는 프로젝트 에이전트만 정하고, 프로젝트의 저장소 이름이어야 한다.
fn validate_agent_links(
    vault: &Path,
    scan: &Scan,
    project: Option<&str>,
    skills: &[String],
    repo: Option<&str>,
) -> Result<(), EngineError> {
    for id in skills {
        let ok = scan.by_id(id).is_some_and(|s| {
            s.kind == ResourceKind::Skill
                && (s.is_global() || (project.is_some() && project == s.project.as_deref()))
        });
        if !ok {
            return Err(EngineError::invalid_argument(format!(
                "skills 의 {id} 는 이 에이전트가 쓸 수 있는 스킬이 아니다{}",
                if project.is_none() {
                    "(글로벌 에이전트는 글로벌 스킬만 쓴다)"
                } else {
                    "(없거나 다른 프로젝트의 것이다)"
                }
            )));
        }
    }
    if let Some(repo) = repo {
        let Some(project) = project else {
            return Err(EngineError::invalid_argument(
                "글로벌 에이전트에는 repo 를 줄 수 없다. 저장소는 작업을 시킬 때 프로젝트의 것을 고른다",
            ));
        };
        let info = require_project(vault, project)?;
        let names: Vec<&str> = info.repos.iter().map(|r| r.name.as_str()).collect();
        if !names.contains(&repo) {
            return Err(EngineError::invalid_argument(format!(
                "repo 는 프로젝트의 저장소 이름이어야 한다: {repo}. 있는 이름: [{}]",
                names.join(", ")
            )));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- 지우기·되살리기

fn trash_item(kind: ResourceKind) -> &'static str {
    if kind == ResourceKind::Skill {
        "item"
    } else {
        "item.md"
    }
}

/// 자원을 휴지통으로 보낸다(스킬은 폴더째). 되살릴 수 있다.
pub fn delete(vault: &Path, r: &Resource) -> Result<(), EngineError> {
    let trash = root(vault).join(".trash").join(&r.id);
    if trash.exists() {
        return Err(EngineError::conflict(format!(
            "휴지통에 같은 id 가 이미 있다: {}",
            r.id
        )));
    }
    let src = vault.join(&r.rel_path);
    let item = trash.join(trash_item(r.kind));
    let from: Option<&Path> = if r.kind == ResourceKind::Skill {
        src.parent()
    } else {
        Some(&src)
    };
    let moved = fs::create_dir_all(&trash).is_ok()
        && from.is_some_and(|from| fs::rename(from, &item).is_ok());
    if !moved {
        return Err(EngineError::internal(format!(
            "휴지통으로 옮기지 못했다: {}",
            r.rel_path
        )));
    }
    let meta = format!(
        "{{\"id\":{},\"kind\":{},\"project\":{},\"name\":{},\"relPath\":{},\"deletedAt\":{}}}\n",
        q(&r.id),
        q(r.kind.label()),
        r.project.as_deref().map_or_else(|| "null".to_string(), q),
        q(&r.name),
        q(&r.rel_path),
        q(&instant_text(now_millis()))
    );
    write_atomic(&trash.join("meta.json"), meta.as_bytes())
}

/// 휴지통의 자원을 되살린다. 제자리에 같은 이름이 이미 있으면 `AIS-409`다.
pub fn restore(vault: &Path, id: &str) -> Result<Resource, EngineError> {
    let trash = root(vault).join(".trash").join(id);
    let Some(e) = read_trash_entry(&trash) else {
        return Err(EngineError::not_found(format!(
            "휴지통에서 못 찾았다: {id}"
        )));
    };
    let target = vault.join(&e.rel_path);
    let destination = if e.kind == ResourceKind::Skill {
        target
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| target.clone())
    } else {
        target.clone()
    };
    if destination.exists() {
        return Err(EngineError::conflict(format!(
            "제자리에 같은 이름이 이미 있어 되살리지 못했다. 그 이름을 바꾸거나 지운 뒤 다시 한다: {}",
            e.rel_path
        )));
    }
    let item = trash.join(trash_item(e.kind));
    let moved = destination
        .parent()
        .is_some_and(|parent| fs::create_dir_all(parent).is_ok())
        && fs::rename(&item, &destination).is_ok();
    if !moved {
        return Err(EngineError::internal(format!(
            "되살리지 못했다: {}",
            e.rel_path
        )));
    }
    remove_tree(&trash)?;
    find(vault, id)
}

/// 휴지통에 있는 자원의 내용을 읽는다(되살리기 전에 무엇이 돌아오는지 보려고). 없으면 `AIS-404`다.
pub fn peek_trash(vault: &Path, id: &str) -> Result<Resource, EngineError> {
    let trash = root(vault).join(".trash").join(id);
    let Some(e) = read_trash_entry(&trash) else {
        return Err(EngineError::not_found(format!(
            "휴지통에서 못 찾았다: {id}"
        )));
    };
    let item = trash.join(trash_item(e.kind));
    let file = if e.kind == ResourceKind::Skill {
        item.join("SKILL.md")
    } else {
        item
    };
    fs::read(&file)
        .ok()
        .and_then(|bytes| format::parse(&bytes, e.kind, e.project.as_deref(), &e.rel_path).ok())
        .ok_or_else(|| EngineError::internal(format!("휴지통의 자원을 못 읽었다: {id}")))
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
    let id = get("id")?;
    let kind = ResourceKind::parse(&get("kind")?)?;
    let rel_path = get("relPath")?;
    if ResourceKind::of_id(&id) != Some(kind) {
        return None;
    }
    Some(TrashEntry {
        id,
        kind,
        project: get("project"),
        name: get("name"),
        rel_path,
        deleted_at: get("deletedAt").unwrap_or_else(|| "null".to_string()),
    })
}

/// 휴지통 메타 파일의 글 값. 따옴표, 역슬래시, 줄바꿈·탭·캐리지 리턴, 그 밖의 제어 글자(`\u00XX`)만 이스케이프한다.
fn q(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// ---------------------------------------------------------------- 이력

fn history_dir(vault: &Path, id: &str) -> PathBuf {
    root(vault).join(".history").join(id)
}

/// 길이가 짧은 것이 앞, 같은 길이면 글자 차례(36진수 시각이 시간 차례가 되게 한다)
fn by_length_then_name(a: &Path, b: &Path) -> std::cmp::Ordering {
    let (x, y) = (file_name(a), file_name(b));
    x.encode_utf16()
        .count()
        .cmp(&y.encode_utf16().count())
        .then_with(|| compare_utf16(&x, &y))
}

fn snapshot(vault: &Path, id: &str, bytes: &[u8]) -> Result<(), EngineError> {
    let dir = history_dir(vault, id);
    let base = to_base36(now_millis());
    let mut name = base.clone();
    let mut n = 1;
    while dir.join(format!("{name}.md")).exists() {
        name = format!("{base}-{n}");
        n += 1;
    }
    write_atomic(&dir.join(format!("{name}.md")), bytes)?;
    let mut all = sorted_entries(&dir);
    all.sort_by(|a, b| by_length_then_name(a, b));
    let excess = all.len().saturating_sub(HISTORY_KEEP);
    for old in all.iter().take(excess) {
        remove_tree(old)?;
    }
    Ok(())
}

/// 고치기 전의 판 목록. 새것이 앞이다.
pub fn versions(vault: &Path, r: &Resource) -> Vec<Version> {
    let mut all = sorted_entries(&history_dir(vault, &r.id));
    all.sort_by(|a, b| by_length_then_name(b, a));
    let mut out: Vec<Version> = Vec::new();
    for p in all {
        let file = file_name(&p);
        let Some(version) = file.strip_suffix(".md") else {
            continue;
        };
        let Ok(bytes) = fs::read(&p) else {
            continue;
        };
        let Ok(old) = format::parse(&bytes, r.kind, r.project.as_deref(), &r.rel_path) else {
            continue;
        };
        let stamp = version.split('-').next().unwrap_or(version);
        let Ok(saved_at) = i64::from_str_radix(stamp, 36) else {
            continue;
        };
        out.push(Version {
            version: version.to_string(),
            saved_at,
            bytes: bytes.len(),
            edited_at: old.edited_at,
            edited_session: old.edited_session,
        });
    }
    out
}

/// 이력의 한 판을 자원으로 읽는다. 없으면 `AIS-404`다.
pub fn read_version(vault: &Path, r: &Resource, version: &str) -> Result<Resource, EngineError> {
    if !VERSION_SHAPE.is_match(version) {
        return Err(EngineError::invalid_argument(format!(
            "version 모양이 아니다: {version}"
        )));
    }
    let p = history_dir(vault, &r.id).join(format!("{version}.md"));
    if !is_file(&p) {
        return Err(EngineError::not_found(format!(
            "그 판을 못 찾았다: {version}"
        )));
    }
    fs::read(&p)
        .ok()
        .and_then(|bytes| format::parse(&bytes, r.kind, r.project.as_deref(), &r.rel_path).ok())
        .ok_or_else(|| EngineError::internal(format!("그 판을 못 읽었다: {version}")))
}
