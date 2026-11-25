//! 호출자. 앱과 AI는 보이는 것이 다르다(AI에게는 기록 층 노트가 안 보인다).

use serde::Deserialize;

/// 호출자. 앱과 AI는 보이는 것이 다르다(AI에게는 기록 층 노트가 안 보인다).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Caller {
    #[default]
    App,
    Ai,
}

impl Caller {
    /// 요청 JSON의 `caller` 칸에 적는 글
    pub fn as_str(self) -> &'static str {
        match self {
            Caller::App => "app",
            Caller::Ai => "ai",
        }
    }
}

/// 호출 맥락: 이 호출을 누가, 어느 세션에서 보냈는지. 앱과 AI 도구는 보이는 것과 자동 커밋에서 세는 방식이 다르다.
/// 요청의 인자(무엇을 할지)와 달리 호출을 보낸 입구(앱, 명령줄, MCP)가 정하는 값이라 인자 밖에서 받는다(`Engine::call_as`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CallContext {
    pub caller: Caller,
    /// AI 도구가 보낸 세션 id
    pub session: Option<String>,
    /// 세션을 연 에이전트 이름(`codex` 따위). 없으면 claude로 본다. 노트에 적는 편집 도장이 읽는다.
    pub session_agent: Option<String>,
    #[serde(skip)]
    pub manager: Option<ManagerContext>,
    #[serde(skip)]
    local_authority: LocalAuthority,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum LocalAuthority {
    #[default]
    None,
    Operator,
    IntakeNotifier,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagerContext {
    pub name: String,
    pub project: String,
    pub project_id: String,
    pub repos: Vec<String>,
    pub agendas: Vec<String>,
    pub intake_work: IntakeWorkPermissions,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IntakeWorkPermissions {
    pub create_task: bool,
    pub comment_task: bool,
}

/// 등록부와 파일에 쓸 수 있는 매니저 이름인지 본다.
pub fn valid_manager_name(value: &str) -> bool {
    (1..=32).contains(&value.len())
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

impl CallContext {
    /// 앱(화면)의 호출. 세션이 없다.
    pub fn app() -> Self {
        Self {
            local_authority: LocalAuthority::Operator,
            ..Self::default()
        }
    }

    pub fn intake_notifier() -> Self {
        Self {
            local_authority: LocalAuthority::IntakeNotifier,
            ..Self::default()
        }
    }

    pub fn is_local_operator(&self) -> bool {
        self.local_authority == LocalAuthority::Operator
    }

    pub fn is_intake_notifier(&self) -> bool {
        self.local_authority == LocalAuthority::IntakeNotifier
    }

    pub fn has_agent_provenance(&self) -> bool {
        self.caller == Caller::Ai
            || self
                .session
                .as_deref()
                .is_some_and(|session| !super::java::java_blank(session))
    }

    pub fn clear_local_authority(&mut self) {
        self.local_authority = LocalAuthority::None;
    }

    /// AI 도구(명령줄 도구 표면, MCP)의 호출
    pub fn ai(session: Option<String>, session_agent: Option<String>) -> Self {
        CallContext {
            caller: Caller::Ai,
            session,
            session_agent,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_data_cannot_grant_local_authority() {
        let request: CallContext = serde_json::from_value(serde_json::json!({
            "caller": "app", "localAuthority": "Operator", "manager": {"name": "demo"}
        }))
        .unwrap();
        assert_eq!(request.caller, Caller::App);
        assert!(request.manager.is_none());
        assert!(!request.is_local_operator());
        assert!(!request.is_intake_notifier());
        assert!(CallContext::app().is_local_operator());
        assert!(!CallContext::intake_notifier().is_local_operator());
    }
}
