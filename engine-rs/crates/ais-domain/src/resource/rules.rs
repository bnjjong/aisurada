//! 자원(스킬·에이전트) 쓰기의 규칙: 만들기·고치기 인자를 읽고 검사하는 일, 종류와 범위가 바뀌지 않는 조건,
//! 프로젝트 참조 목록을 더하고 빼는 계산. 파일도 기록장도 읽고 쓰지 않는다.
//!
//! 앞의 `aggregate`가 「이 변경을 바로 적용할지 승인을 기다릴지」를 정한다면, 여기는 「이 변경이 무엇인지」를 정한다.
//! 명령은 저장소에서 자원과 참조 목록을 읽어 여기 넘기고, 돌려받은 새 값을 쓴다.
//! 오류가 나오는 차례를 지키려고 명령이 읽기 사이사이에 이 함수들을 한 걸음씩 부른다.
//!
//! 한계: 고칠 칸 묶음 [`Fields`](super::store::Fields)과 참조 한 줄 [`Entry`](super::refs::Entry)은 저장소 모듈에 정의돼 있다.
//! 값 타입만 쓰고 그 모듈의 파일 읽기·쓰기는 부르지 않는다. 타입을 도메인으로 옮기는 일은 크레이트를 가르는 8단계에서 본다.

use super::format;
use super::{Entry, Fields, Resource, ResourceKind, tools_equal};
use crate::error::EngineError;
use crate::kernel::java::java_trim;
use crate::text::{is_blank, trim};
use serde_json::Value as Json;

/// 명령이 가리킨 범위. `given`은 범위를 주기는 했는가이고, `project`가 `None`이면 글로벌이다.
#[derive(Debug, PartialEq, Eq)]
pub struct Scope {
    pub given: bool,
    pub project: Option<String>,
}

/// `scope`와 `project` 인자를 읽는다. 빈 글은 안 준 것으로 본다. 둘 다 주면 거절하고, `required`인데 둘 다 없어도 거절한다.
pub fn scope_of(
    scope: Option<&str>,
    project: Option<&str>,
    required: bool,
) -> Result<Scope, EngineError> {
    let scope = scope.filter(|s| !is_blank(s));
    let project = project.filter(|p| !is_blank(p));
    if scope.is_some() && project.is_some() {
        return Err(EngineError::invalid_argument(
            "scope=global 과 project=<프로젝트 slug> 는 함께 줄 수 없다",
        ));
    }
    if let Some(scope) = scope {
        if java_trim(scope) != "global" {
            return Err(EngineError::invalid_argument(format!(
                "scope 는 global 이어야 한다. 프로젝트 자원은 project=<프로젝트 slug> 로 준다: {scope}"
            )));
        }
        return Ok(Scope {
            given: true,
            project: None,
        });
    }
    if let Some(project) = project {
        return Ok(Scope {
            given: true,
            project: Some(java_trim(project).to_string()),
        });
    }
    if required {
        return Err(EngineError::invalid_argument(
            "scope=global 이나 project=<프로젝트 slug> 가 필요하다",
        ));
    }
    Ok(Scope {
        given: false,
        project: None,
    })
}

/// `kind` 인자를 읽는다. `skill`이나 `agent`여야 한다.
pub fn kind_of(raw: Option<&str>) -> Result<ResourceKind, EngineError> {
    raw.and_then(ResourceKind::parse).ok_or_else(|| {
        EngineError::invalid_argument(format!(
            "kind 는 skill 이나 agent 여야 한다: {}",
            raw.unwrap_or("null")
        ))
    })
}

/// 스킬은 `permission`·`skills`·`repo`를 쓰지 않는다(에이전트 전용). 하나라도 주었으면 거절한다.
/// `given`은 이름으로 「이 인자를 주었는가」를 답한다.
pub fn reject_agent_only_for_skill(
    kind: ResourceKind,
    given: impl Fn(&str) -> bool,
) -> Result<(), EngineError> {
    if kind == ResourceKind::Skill && ["permission", "skills", "repo"].iter().any(|k| given(k)) {
        return Err(EngineError::invalid_argument(
            "permission, skills, repo 는 에이전트에만 쓴다",
        ));
    }
    Ok(())
}

/// 고칠 때 종류가 지금 자원과 같은지 본다. 종류를 주었는데 다르면 거절한다.
/// 범위를 해석하기 전에 불러야 한다: 종류와 범위가 둘 다 틀리면 종류 오류가 먼저 나온다(오류 차례: 종류 → 범위).
pub fn check_same_kind(live: &Resource, given_kind: Option<&str>) -> Result<(), EngineError> {
    if let Some(raw) = given_kind
        && kind_of(Some(raw))? != live.kind
    {
        return Err(EngineError::invalid_argument(format!(
            "종류는 바꿀 수 없다: {}",
            live.kind.label()
        )));
    }
    Ok(())
}

/// 고칠 때 범위가 지금 자원과 같은지 본다. 범위를 주었는데 다르면 거절한다. 종류 검사([`check_same_kind`]) 뒤에 부른다.
pub fn check_same_scope(live: &Resource, scope: &Scope) -> Result<(), EngineError> {
    if scope.given && scope.project != live.project {
        return Err(EngineError::invalid_argument(format!(
            "범위는 바꿀 수 없다: {}",
            match &live.project {
                None => "global".to_string(),
                Some(p) => format!("project={p}"),
            }
        )));
    }
    Ok(())
}

/// 칸 인자를 읽어 검사해 고칠 값 묶음을 만든다. `get`은 이름으로 준 글을 답한다. 안 준 칸은 `None`(그대로 둔다)이다.
pub fn fields_of<'a>(get: impl Fn(&str) -> Option<&'a str>) -> Result<Fields, EngineError> {
    let json_text = |v: &str| Json::String(v.to_string());
    Ok(Fields {
        name: get("name").map(format::name).transpose()?,
        description: get("description").map(format::description).transpose()?,
        body: get("body").map(format::body).transpose()?,
        permission: get("permission").map(format::permission).transpose()?,
        skills: get("skills")
            .map(|v| format::skill_ids(&json_text(v)))
            .transpose()?,
        repo: get("repo").map(|v| trim(v).to_string()),
        tools: get("tools")
            .map(|v| format::tools(&json_text(v)))
            .transpose()?,
    })
}

/// 참조 목록을 바꾼 결과
#[derive(Debug, PartialEq, Eq)]
pub struct RefsChange {
    /// 새 목록. 이름 힌트는 지금 이름으로 새로 적었다
    pub next: Vec<Entry>,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

/// 프로젝트의 참조 목록(`entries`, 승인한 목록)에 글로벌 자원 `id`를 더하거나 뺀다.
/// 더할 때는 그 자원이 있고 글로벌이어야 한다. `with_skills`면 에이전트가 가진 글로벌 스킬도 함께 더한다.
/// `lookup`은 id로 자원을 찾는다(승인한 판). 새 목록의 이름 힌트는 `lookup`에 있는 자원의 지금 이름으로 고친다.
pub fn change_refs<'a>(
    mut entries: Vec<Entry>,
    id: &str,
    remove: bool,
    with_skills: bool,
    lookup: impl Fn(&str) -> Option<&'a Resource>,
) -> Result<RefsChange, EngineError> {
    let mut added: Vec<String> = Vec::new();
    let mut removed: Vec<String> = Vec::new();
    if remove {
        let before = entries.len();
        entries.retain(|e| e.id != id);
        if entries.len() != before {
            removed.push(id.to_string());
        }
    } else {
        let Some(g) = lookup(id) else {
            return Err(EngineError::not_found(format!("자원을 못 찾았다: {id}")));
        };
        if !g.is_global() {
            return Err(EngineError::invalid_argument(format!(
                "글로벌 자원만 참조한다. 프로젝트 자원은 참조하지 않아도 쓸 수 있다: {id}"
            )));
        }
        add_ref(&mut entries, g, &mut added);
        if with_skills && g.kind == ResourceKind::Agent {
            for sid in &g.skills {
                if let Some(s) = lookup(sid).filter(|s| s.is_global()) {
                    add_ref(&mut entries, s, &mut added);
                }
            }
        }
    }
    // 이름 힌트를 지금 이름으로 새로 적는다(ID는 그대로다)
    let next = entries
        .into_iter()
        .map(|e| Entry {
            hint: lookup(&e.id).map_or(e.hint.clone(), |r| r.name.clone()),
            id: e.id,
        })
        .collect();
    Ok(RefsChange {
        next,
        added,
        removed,
    })
}

fn add_ref(entries: &mut Vec<Entry>, g: &Resource, added: &mut Vec<String>) {
    if !entries.iter().any(|e| e.id == g.id) {
        entries.push(Entry {
            id: g.id.clone(),
            hint: g.name.clone(),
        });
        added.push(g.id.clone());
    }
}

/// 변경 한 건을 사람이 읽는 한 줄로 요약한다(승인 대기 목록에 보인다).
/// `command`는 명령 이름이고, `before`·`after`는 바뀌기 전과 후의 자원(없으면 `None`), `ref_info`는 참조 변경일 때의 재료다.
pub fn summarize_change(
    command: &str,
    before: Option<&Resource>,
    after: Option<&Resource>,
    ref_info: Option<&RefInfo>,
) -> String {
    if command == "set-resource-ref"
        && let Some(info) = ref_info
    {
        let mut s = format!("프로젝트 '{}'의 글로벌 자원 참조 ", info.project);
        if !info.added.is_empty() {
            s.push_str(&format!("{}개 켜기", info.added.len()));
        }
        if !info.added.is_empty() && !info.removed.is_empty() {
            s.push_str(", ");
        }
        if !info.removed.is_empty() {
            s.push_str(&format!("{}개 끄기", info.removed.len()));
        }
        return s;
    }
    let any = after.or(before).expect("바꿀 자원이 있다");
    let what = format!(
        "{}{} '{}'",
        if any.is_global() {
            "글로벌 "
        } else {
            "프로젝트 "
        },
        if any.kind == ResourceKind::Skill {
            "스킬"
        } else {
            "에이전트"
        },
        any.name
    );
    match command {
        "delete-resource" => format!("{what} 지우기"),
        "restore-resource" => format!("{what} 되살리기"),
        _ => match (before, after) {
            (Some(b), Some(a)) => format!("{what} 고치기({})", changed_fields(b, a).join(", ")),
            _ => format!("{what} 만들기"),
        },
    }
}

pub fn changed_fields(a: &Resource, b: &Resource) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if a.name != b.name {
        out.push("이름".into());
    }
    if a.description != b.description {
        out.push("설명".into());
    }
    if a.body != b.body {
        out.push("지침".into());
    }
    if a.permission != b.permission {
        let show = |p: &Option<String>| p.clone().unwrap_or_else(|| "null".to_string());
        out.push(format!(
            "권한 {} → {}",
            show(&a.permission),
            show(&b.permission)
        ));
    }
    if a.skills != b.skills {
        out.push("스킬 목록".into());
    }
    if a.repo != b.repo {
        out.push("저장소".into());
    }
    if !tools_equal(&a.tools, &b.tools) {
        out.push("도구 칸".into());
    }
    out
}

/// 참조를 바꾼 변경의 요약 재료: 프로젝트와 켠 자원 id, 끈 자원 id
#[derive(Debug)]
pub struct RefInfo {
    pub project: String,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn res(id: &str, name: &str, project: Option<&str>, skills: &[&str]) -> Resource {
        Resource {
            id: id.into(),
            kind: if id.starts_with("ap") {
                ResourceKind::Agent
            } else {
                ResourceKind::Skill
            },
            project: project.map(str::to_string),
            name: name.into(),
            description: "d".into(),
            body: String::new(),
            permission: None,
            skills: skills.iter().map(|s| s.to_string()).collect(),
            repo: None,
            tools: Vec::new(),
            created_at: "2026-11-16T10:00:00Z".into(),
            edited_at: "2026-11-16T10:00:00Z".into(),
            missing_timestamps: false,
            edited_session: None,
            edited_agent: None,
            rel_path: String::new(),
            token: None,
        }
    }

    fn entry(id: &str, hint: &str) -> Entry {
        Entry {
            id: id.into(),
            hint: hint.into(),
        }
    }

    #[test]
    fn scope_is_global_or_a_project_but_never_both() {
        let msg = |s, p, r| scope_of(s, p, r).err().unwrap().message;
        assert_eq!(
            msg(Some("global"), Some("p"), false),
            "scope=global 과 project=<프로젝트 slug> 는 함께 줄 수 없다"
        );
        assert!(msg(Some("project"), None, false).starts_with("scope 는 global 이어야 한다."));
        assert_eq!(
            msg(None, None, true),
            "scope=global 이나 project=<프로젝트 slug> 가 필요하다"
        );
        assert_eq!(
            scope_of(Some(" global "), None, true).unwrap(),
            Scope {
                given: true,
                project: None
            }
        );
        assert_eq!(
            scope_of(None, Some(" p1 "), true).unwrap(),
            Scope {
                given: true,
                project: Some("p1".into())
            }
        );
        // 빈 글은 안 준 것이다
        assert_eq!(
            scope_of(Some(" "), Some(""), false).unwrap(),
            Scope {
                given: false,
                project: None
            }
        );
    }

    #[test]
    fn kind_must_be_skill_or_agent() {
        assert_eq!(kind_of(Some("skill")).unwrap(), ResourceKind::Skill);
        assert_eq!(
            kind_of(None).err().unwrap().message,
            "kind 는 skill 이나 agent 여야 한다: null"
        );
        assert_eq!(
            kind_of(Some("x")).err().unwrap().message,
            "kind 는 skill 이나 agent 여야 한다: x"
        );
    }

    #[test]
    fn a_skill_refuses_agent_only_arguments() {
        let only = |name: &'static str| move |k: &str| k == name;
        assert!(reject_agent_only_for_skill(ResourceKind::Skill, only("name")).is_ok());
        for k in ["permission", "skills", "repo"] {
            assert_eq!(
                reject_agent_only_for_skill(ResourceKind::Skill, only(k))
                    .err()
                    .unwrap()
                    .message,
                "permission, skills, repo 는 에이전트에만 쓴다"
            );
            assert!(reject_agent_only_for_skill(ResourceKind::Agent, only(k)).is_ok());
        }
    }

    #[test]
    fn kind_and_scope_cannot_change_on_edit() {
        let live = res("sk1", "s", Some("p1"), &[]);
        let same = Scope {
            given: false,
            project: None,
        };
        assert!(check_same_kind(&live, Some("skill")).is_ok());
        assert!(check_same_kind(&live, None).is_ok());
        assert!(check_same_scope(&live, &same).is_ok());
        assert_eq!(
            check_same_kind(&live, Some("agent")).err().unwrap().message,
            "종류는 바꿀 수 없다: skill"
        );
        let other = Scope {
            given: true,
            project: None,
        };
        assert_eq!(
            check_same_scope(&live, &other).err().unwrap().message,
            "범위는 바꿀 수 없다: project=p1"
        );
        let global = res("sk2", "s", None, &[]);
        let to_project = Scope {
            given: true,
            project: Some("p1".into()),
        };
        assert_eq!(
            check_same_scope(&global, &to_project)
                .err()
                .unwrap()
                .message,
            "범위는 바꿀 수 없다: global"
        );
    }

    #[test]
    fn fields_keep_unsent_slots_as_none_and_trim_repo() {
        let given = |k: &str| match k {
            "name" => Some("이름"),
            "repo" => Some("  r  "),
            "skills" => Some("[]"),
            _ => None,
        };
        let f = fields_of(given).unwrap();
        assert_eq!(f.name.as_deref(), Some("이름"));
        assert_eq!(f.repo.as_deref(), Some("r"));
        assert_eq!(f.skills, Some(Vec::new()));
        assert!(
            f.description.is_none()
                && f.body.is_none()
                && f.permission.is_none()
                && f.tools.is_none()
        );
        // 모양이 틀린 칸은 거절한다
        assert!(fields_of(|k| (k == "skills").then_some("not json")).is_err());
    }

    #[test]
    fn adding_a_global_resource_appends_once_and_refreshes_hints() {
        let a = res("sk1", "새 이름", None, &[]);
        let all = [a];
        let lookup = |id: &str| all.iter().find(|r| r.id == id);
        // 이미 있으면 더하지 않고, 힌트는 지금 이름으로 고친다
        let c = change_refs(vec![entry("sk1", "옛 이름")], "sk1", false, false, lookup).unwrap();
        assert_eq!(
            c,
            RefsChange {
                next: vec![entry("sk1", "새 이름")],
                added: vec![],
                removed: vec![]
            }
        );
        let c = change_refs(vec![], "sk1", false, false, lookup).unwrap();
        assert_eq!(
            (c.added.as_slice(), c.next),
            (
                ["sk1".to_string()].as_slice(),
                vec![entry("sk1", "새 이름")]
            )
        );
    }

    #[test]
    fn adding_needs_an_existing_global_resource() {
        let project_one = res("sk9", "p", Some("p1"), &[]);
        let all = [project_one];
        let lookup = |id: &str| all.iter().find(|r| r.id == id);
        assert_eq!(
            change_refs(vec![], "sk0", false, false, lookup)
                .err()
                .unwrap()
                .message,
            "자원을 못 찾았다: sk0"
        );
        assert!(
            change_refs(vec![], "sk9", false, false, lookup)
                .err()
                .unwrap()
                .message
                .starts_with("글로벌 자원만 참조한다.")
        );
        // 빼기는 자원이 없어도 된다
        let c = change_refs(vec![entry("sk0", "x")], "sk0", true, false, lookup).unwrap();
        assert_eq!((c.removed, c.next), (vec!["sk0".to_string()], vec![]));
        let c = change_refs(vec![], "sk0", true, false, lookup).unwrap();
        assert!(c.removed.is_empty());
    }

    #[test]
    fn with_skills_adds_the_agents_global_skills_after_it() {
        let agent = res("ap1", "에이전트", None, &["sk1", "sk2", "sk3"]);
        let all = [
            agent,
            res("sk1", "스킬1", None, &[]),
            res("sk2", "프로젝트 스킬", Some("p1"), &[]),
        ];
        let lookup = |id: &str| all.iter().find(|r| r.id == id);
        let c = change_refs(vec![entry("sk1", "스킬1")], "ap1", false, true, lookup).unwrap();
        // sk1은 이미 있고, sk2는 프로젝트 자원이라, sk3은 없어서 빠진다
        assert_eq!(c.added, vec!["ap1".to_string()]);
        let c = change_refs(vec![], "ap1", false, true, lookup).unwrap();
        assert_eq!(c.added, vec!["ap1".to_string(), "sk1".to_string()]);
        // with_skills가 아니면 에이전트 하나만
        let c = change_refs(vec![], "ap1", false, false, lookup).unwrap();
        assert_eq!(c.added, vec!["ap1".to_string()]);
    }

    #[test]
    fn a_change_is_summarized_in_one_readable_line() {
        let before = res("sk1", "옛", None, &[]);
        let mut after = res("sk1", "새", None, &[]);
        after.body = "지침".into();
        assert_eq!(
            summarize_change("save-resource", Some(&before), Some(&after), None),
            "글로벌 스킬 '새' 고치기(이름, 지침)"
        );
        assert_eq!(
            summarize_change("save-resource", None, Some(&after), None),
            "글로벌 스킬 '새' 만들기"
        );
        let project_agent = res("ap1", "에이전트", Some("p1"), &[]);
        assert_eq!(
            summarize_change("delete-resource", Some(&project_agent), None, None),
            "프로젝트 에이전트 '에이전트' 지우기"
        );
        assert_eq!(
            summarize_change("restore-resource", None, Some(&project_agent), None),
            "프로젝트 에이전트 '에이전트' 되살리기"
        );
        let info = RefInfo {
            project: "p1".into(),
            added: vec!["a".into(), "b".into()],
            removed: vec!["c".into()],
        };
        assert_eq!(
            summarize_change("set-resource-ref", None, None, Some(&info)),
            "프로젝트 'p1'의 글로벌 자원 참조 2개 켜기, 1개 끄기"
        );
    }

    #[test]
    fn changed_fields_lists_the_slots_in_a_fixed_order_with_the_permission_move() {
        let a = res("ap1", "a", None, &[]);
        let mut b = res("ap1", "b", None, &["sk1"]);
        b.permission = Some("edit".into());
        b.repo = Some("r".into());
        assert_eq!(
            changed_fields(&a, &b),
            vec!["이름", "권한 null → edit", "스킬 목록", "저장소"]
        );
        assert!(changed_fields(&a, &a).is_empty());
    }
}
