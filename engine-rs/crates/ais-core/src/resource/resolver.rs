//! 「프로젝트가 쓸 수 있는 자원」을 계산하는 유일한 곳. 외부 설명서에 없는 확장이다.
//!
//! 규칙: 쓸 수 있는 자원 = 프로젝트 자원 + 참조한 글로벌 자원. 참조하지 않은 글로벌 자원은 여기에 나오지 않고, 화면의 선택 목록, AI가 보는
//! 목록, 실행에 내려 주는 입력은 모두 이 결과만 써야 한다. 그래서 규칙이 한 곳에서만 지켜진다.
//! 여기서 보는 것은 승인한 판뿐이다(#53): 자원은 `scan_served`, 참조 목록은 `read_approved`로 읽는다.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::{Resource, ResourceKind, refs, store};
use crate::error::EngineError;

/// 쓸 수 있는 자원 하나와 그 출처(`own` = 프로젝트 자원, `ref` = 참조한 글로벌 자원)
pub struct Item {
    pub resource: Resource,
    pub source: &'static str,
}

/// 참조는 했는데 글로벌 자원이 없는(지웠거나 휴지통에 있는) 항목
pub struct Broken {
    pub id: String,
    pub hint: String,
}

/// 쓸 수 있는 에이전트가 필요로 하지만 쓸 수 없는 스킬. `skill`은 그 ID로 찾은 스킬이고, 없으면 `None`이다.
pub struct Missing {
    pub agent: Resource,
    pub skill_id: String,
    pub skill: Option<Resource>,
}

impl Missing {
    /// 글로벌 스킬이면 참조해서 채울 수 있다.
    pub fn can_reference(&self) -> bool {
        self.skill.as_ref().is_some_and(Resource::is_global)
    }
}

pub struct Usable {
    pub project: String,
    pub skills: Vec<Item>,
    pub agents: Vec<Item>,
    pub broken: Vec<Broken>,
    pub missing_skills: Vec<Missing>,
}

pub fn resolve(vault: &Path, project: &str) -> Result<Usable, EngineError> {
    store::require_project(vault, project)?;
    let served = store::scan_served(vault)?;
    let mut globals: HashMap<&str, &Resource> = HashMap::new();
    let mut all: HashMap<&str, &Resource> = HashMap::new();
    for r in &served.items {
        all.insert(&r.id, r);
        if r.is_global() {
            globals.insert(&r.id, r);
        }
    }
    let mut skills: Vec<Item> = Vec::new();
    let mut agents: Vec<Item> = Vec::new();
    let mut add = |r: &Resource, source: &'static str| {
        let item = Item {
            resource: r.clone(),
            source,
        };
        if r.kind == ResourceKind::Skill {
            skills.push(item);
        } else {
            agents.push(item);
        }
    };
    for r in &served.items {
        if r.project.as_deref() == Some(project) {
            add(r, "own");
        }
    }
    let mut broken: Vec<Broken> = Vec::new();
    for e in refs::read_approved(vault, project)? {
        match globals.get(e.id.as_str()) {
            Some(g) => add(g, "ref"),
            None => broken.push(Broken {
                id: e.id,
                hint: e.hint,
            }),
        }
    }
    let usable_skill_ids: HashSet<String> = skills.iter().map(|s| s.resource.id.clone()).collect();
    let mut missing: Vec<Missing> = Vec::new();
    for a in &agents {
        for sid in &a.resource.skills {
            if !usable_skill_ids.contains(sid) {
                let found = all
                    .get(sid.as_str())
                    .filter(|r| r.kind == ResourceKind::Skill)
                    .map(|r| (*r).clone());
                missing.push(Missing {
                    agent: a.resource.clone(),
                    skill_id: sid.clone(),
                    skill: found,
                });
            }
        }
    }
    Ok(Usable {
        project: project.to_string(),
        skills,
        agents,
        broken,
        missing_skills: missing,
    })
}
