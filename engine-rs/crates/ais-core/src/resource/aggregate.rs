//! 자원 애그리거트: 자원(스킬·에이전트)의 승인 규칙을 한 곳에 모은 도메인 코드. 파일도 기록장도 읽고 쓰지 않는다.
//!
//! 규칙(바뀌지 않는 조건):
//! - **변경의 갈래와 정책**: 자원 변경은 갈래(권한 올림·내림, 도구 칸 변경, 참조 켜기·끄기, 글로벌·프로젝트 자원 변경)로 나뉘고,
//!   정책 네 칸(`auto`·`approve`)이 AI의 변경을 바로 적용할지 승인을 기다리게 할지 정한다. `fixed` 갈래는 정책과 상관없이 늘 승인을 거친다.
//!   앱 통로(사람)의 변경은 늘 바로 적용된다.
//! - **믿을 수 있는 상태**: 파일의 지문이 승인 기록과 같으면 `approved`, 다르면 `dirty`(승인해 둔 사본을 쓴다), 기록이 없으면 `new`(쓰지 않는다)다.
//! - **대기 변경의 일생**: 대기(`pending`) 변경은 승인·거절·낡음(`stale`)으로 한 번만 결정된다. 이미 결정된 변경은 다시 결정하지 못한다.
//!   승인했는데 적용하지 못한 변경은 기준이 바뀐 것이라 낡음으로 남는다.
//!
//! 승인 기록(`approvals.rs`)·대기 변경 파일(`changes.rs`)·자원 파일(`store.rs`)에 쓰고 읽는 일은 저장소가 한다.

use super::format::PERMISSIONS;
use super::{Resource, ResourceKind, tools_equal};
use crate::error::{EngineError, ErrorKind};

pub const AUTO: &str = "auto";
pub const APPROVE: &str = "approve";

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

/// 지금 기록의 정책과 갈래 목록을 응답용 JSON으로 만든다.
pub fn categories_json() -> Vec<serde_json::Value> {
    Category::ALL
        .iter()
        .map(|c| serde_json::json!({"category": c.code(), "label": c.label(), "fixed": c.fixed()}))
        .collect()
}

// ---------------------------------------------------------------- 믿을 수 있는 상태

/// 승인 기록과 지금 파일을 견준 상태. 자원 파일과 참조 목록 파일이 같은 규칙을 쓴다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrustState {
    /// 승인한 판과 같다.
    Approved,
    /// 승인한 뒤에 파일이 바뀌었다. 승인해 둔 사본을 쓴다.
    Dirty,
    /// 승인 기록이 없는데 파일이 있다. 쓰지 않는다.
    Untrusted,
}

impl TrustState {
    /// `approved`는 승인 기록의 확인 값(없으면 `None`), `live`는 지금 파일의 확인 값이다.
    pub fn judge(approved: Option<&str>, live: Option<&str>) -> TrustState {
        match approved {
            None => TrustState::Untrusted,
            Some(token) if Some(token) == live => TrustState::Approved,
            Some(_) => TrustState::Dirty,
        }
    }

    /// 응답에 적는 이름. 기록 없는 새 파일은 `new`다.
    pub const fn label(self) -> &'static str {
        match self {
            TrustState::Approved => "approved",
            TrustState::Dirty => "dirty",
            TrustState::Untrusted => "new",
        }
    }

}

// ---------------------------------------------------------------- 대기 변경의 일생

/// 대기 변경의 상태
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChangeStatus {
    Pending,
    Approved,
    Rejected,
    /// 승인했지만 기준이 바뀌어 적용하지 못했다
    Stale,
}

impl ChangeStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            ChangeStatus::Pending => "pending",
            ChangeStatus::Approved => "approved",
            ChangeStatus::Rejected => "rejected",
            ChangeStatus::Stale => "stale",
        }
    }
}

/// 승인한 변경을 적용하다 난 오류가 「변경이 더는 맞지 않는다」는 뜻인가. 그러면 변경은 낡음으로 남고, 아니면(내부 오류 따위) 대기로 남는다.
/// 예전에는 오류 코드 글자(`AIS-400`·`404`·`409`·`412`)로 가렸다. 같은 오류들이다: 인자가 틀림, 찾는 것이 없음, 지금 상태와 부딪힘,
/// 지원하지 않음(코드는 `409`), 읽은 뒤 바뀜.
pub fn goes_stale(error: &EngineError) -> bool {
    matches!(
        error.kind,
        ErrorKind::InvalidArgument
            | ErrorKind::NotFound
            | ErrorKind::Conflict
            | ErrorKind::Unsupported
            | ErrorKind::PreconditionFailed
    )
}

/// 대기 상태가 아닌 변경을 결정하려 할 때의 오류(`AIS-409`). `current`는 파일에 적힌 지금 상태 글이다.
pub fn already_decided(id: &str, current: &str) -> EngineError {
    EngineError::conflict(format!("이미 처리된 변경이다({current}): {id}"))
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

    #[test]
    fn trust_state_compares_the_ledger_token_with_the_live_file() {
        assert_eq!(TrustState::judge(None, Some("f1:a")), TrustState::Untrusted);
        assert_eq!(TrustState::judge(None, None), TrustState::Untrusted);
        assert_eq!(
            TrustState::judge(Some("f1:a"), Some("f1:a")),
            TrustState::Approved
        );
        assert_eq!(
            TrustState::judge(Some("f1:a"), Some("f1:b")),
            TrustState::Dirty
        );
        assert_eq!(TrustState::judge(Some("f1:a"), None), TrustState::Dirty);
        // 응답에 적는 이름: 기록 없는 새 파일은 `new`다
        assert_eq!(TrustState::Untrusted.label(), "new");
        assert_eq!(TrustState::Dirty.label(), "dirty");
        assert_eq!(TrustState::Approved.label(), "approved");
    }

    #[test]
    fn only_changes_that_no_longer_apply_go_stale() {
        // 오류 코드 글자(AIS-400·404·409·412)로 가리던 것과 같은 오류들이다
        for stale in [
            EngineError::invalid_argument("x"),
            EngineError::not_found("x"),
            EngineError::conflict("x"),
            EngineError::unsupported("x"),
            EngineError::precondition_failed("x"),
        ] {
            assert!(goes_stale(&stale), "{stale}");
            assert!(matches!(
                stale.code,
                "AIS-400" | "AIS-404" | "AIS-409" | "AIS-412"
            ));
        }
        // 호출자에게 허락되지 않은 일과 내부 오류는 낡음이 아니다
        for kept in [EngineError::forbidden("x"), EngineError::internal("x")] {
            assert!(!goes_stale(&kept), "{kept}");
        }
    }

    #[test]
    fn change_status_names_and_the_decided_error() {
        let names: Vec<&str> = [
            ChangeStatus::Pending,
            ChangeStatus::Approved,
            ChangeStatus::Rejected,
            ChangeStatus::Stale,
        ]
        .iter()
        .map(|s| s.as_str())
        .collect();
        assert_eq!(names, ["pending", "approved", "rejected", "stale"]);
        let err = already_decided("pcmwh21ips-00090001", "approved");
        assert_eq!(err.code, "AIS-409");
        assert_eq!(
            err.message,
            "이미 처리된 변경이다(approved): pcmwh21ips-00090001"
        );
    }
}
