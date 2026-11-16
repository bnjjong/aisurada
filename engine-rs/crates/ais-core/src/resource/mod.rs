//! 스킬과 에이전트 정의(자원)의 규칙: 모양(`format`), 승인 기록(`approvals`), 대기 변경(`changes`), 참조 목록(`refs`),
//! 쓸 수 있는 자원 풀기(`resolver`), vault에서 읽고 쓰는 곳(`store`). 외부 설명서에 없는 확장이다.

pub mod aggregate;
pub mod approvals;
pub mod changes;
pub mod format;
pub mod refs;
pub mod resolver;
pub mod rules;
pub mod store;

use crate::kernel::ids::RESOURCE_ID as ID_SHAPE;

use crate::frontmatter::Value;

/// 자원 종류
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ResourceKind {
    Skill,
    Agent,
}

impl ResourceKind {
    pub const ALL: [ResourceKind; 2] = [ResourceKind::Skill, ResourceKind::Agent];

    pub fn label(self) -> &'static str {
        match self {
            ResourceKind::Skill => "skill",
            ResourceKind::Agent => "agent",
        }
    }

    pub fn id_prefix(self) -> &'static str {
        match self {
            ResourceKind::Skill => "sk",
            ResourceKind::Agent => "ap",
        }
    }

    /// 범위 폴더 아래 이 종류의 폴더 이름
    pub fn dir(self) -> &'static str {
        match self {
            ResourceKind::Skill => "skills",
            ResourceKind::Agent => "agents",
        }
    }

    pub fn parse(raw: &str) -> Option<ResourceKind> {
        let v = raw.trim_matches(|c: char| c <= ' ').to_lowercase();
        ResourceKind::ALL.into_iter().find(|k| k.label() == v)
    }

    pub fn is_id(s: &str) -> bool {
        ID_SHAPE.is_match(s)
    }

    pub fn of_id(id: &str) -> Option<ResourceKind> {
        if !Self::is_id(id) {
            return None;
        }
        Some(if id.starts_with("sk") {
            ResourceKind::Skill
        } else {
            ResourceKind::Agent
        })
    }
}

#[derive(Clone, Debug)]
pub struct Resource {
    pub id: String,
    pub kind: ResourceKind,
    /// 프로젝트 slug. `None`이면 글로벌 자원이다.
    pub project: Option<String>,
    pub name: String,
    pub description: String,
    /// 에이전트만(`read`, `edit`, `publish`)
    pub permission: Option<String>,
    pub skills: Vec<String>,
    pub repo: Option<String>,
    /// 도구별 선택 칸. 안쪽 값은 검사하지 않고 그대로 보존한다.
    pub tools: Vec<(String, Value)>,
    pub body: String,
    pub edited_at: Option<String>,
    pub edited_session: Option<String>,
    pub edited_agent: Option<String>,
    /// vault 기준 상대 경로
    pub rel_path: String,
    pub token: Option<String>,
}

impl Resource {
    pub fn is_global(&self) -> bool {
        self.project.is_none()
    }

    pub fn scope(&self) -> &'static str {
        if self.project.is_none() {
            "global"
        } else {
            "project"
        }
    }
}

/// `Map.equals`처럼 맵은 차례를 안 따지고 같은지 본다.
pub fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Map(x), Value::Map(y)) => {
            x.len() == y.len()
                && x.iter().all(|(k, v)| {
                    y.iter()
                        .find(|(k2, _)| k2 == k)
                        .is_some_and(|(_, v2)| values_equal(v, v2))
                })
        }
        (Value::List(x), Value::List(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| values_equal(p, q))
        }
        _ => a == b,
    }
}

/// 도구 칸 두 개가 같은가(`Objects.equals`)
pub fn tools_equal(a: &[(String, Value)], b: &[(String, Value)]) -> bool {
    a.len() == b.len()
        && a.iter().all(|(k, v)| {
            b.iter()
                .find(|(k2, _)| k2 == k)
                .is_some_and(|(_, v2)| values_equal(v, v2))
        })
}
