//! 엔진의 진입점. 앱의 `engine_call`, `ais-cli`, MCP가 모두 이 한 곳으로 들어옵니다.
//!
//! 받는 명령은 [`table`]의 표 하나에 적혀 있다. 명령 목록, 쓰기 목록, 디스패처가 모두 이 표에서 나온다.

mod intake;
mod table;

pub use intake::IntakeOperation;

use crate::kernel::actor::CallContext;
use crate::ports::Ports;
use crate::vault::{agenda, project};
use crate::write::{
    marker::check_workspace_marker,
    pipeline::{self, WriteScope},
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::EngineError;

pub use table::{Access, Scope};

/// 명령 표의 한 줄을 바깥에 보여 주는 모양. 표의 처리 함수는 보여 주지 않는다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandInfo {
    /// 밑줄 이름(`list_docs`)
    pub name: &'static str,
    /// vault를 고치는 명령인가
    pub access: Access,
    /// 쓰기 범위를 여는 방식
    pub scope: Scope,
}

impl CommandInfo {
    /// vault를 고치는 명령인가
    pub fn is_write(&self) -> bool {
        self.access == Access::Write
    }
}

/// 이 엔진이 처리하는 명령 전부(앱에 알리는 차례)
pub fn catalog() -> impl Iterator<Item = CommandInfo> {
    table::TABLE.iter().map(|def| CommandInfo {
        name: def.name,
        access: def.access,
        scope: def.scope,
    })
}

/// 매니저를 등록할 때 확인하는 프로젝트와 그 프로젝트에 속한 쓰기 대상.
pub struct ManagerProject {
    pub id: String,
    pub repos: Vec<String>,
    pub agendas: Vec<String>,
}

pub fn manager_project(root: &Path, slug: &str) -> Option<ManagerProject> {
    check_workspace_marker(root).ok()?;
    let project = project::scan_projects(root)
        .into_iter()
        .find(|item| item.slug == slug)?;
    let scan = agenda::scan_agendas(root);
    Some(ManagerProject {
        id: project.id,
        repos: project
            .repos
            .into_iter()
            .filter_map(|repo| repo.github)
            .collect(),
        agendas: scan
            .table
            .infos()
            .filter(|item| {
                item.project.as_deref() == Some(slug)
                    && scan
                        .table
                        .by_slug(&item.slug)
                        .is_some_and(|current| current.id == item.id)
            })
            .map(|item| item.slug.clone())
            .collect(),
    })
}

/// vault 하나를 맡는 엔진. 요청마다 vault를 새로 훑으므로 상태가 없다.
pub struct Engine {
    vault_root: PathBuf,
    /// 이 엔진이 명령을 부르는 동안 현재 스레드에 거는 약속 묶음. 없으면 기본 어댑터(환경 변수와 실제 시계)를 쓴다.
    ports: Option<Arc<Ports>>,
}

impl Engine {
    pub fn new(vault_root: impl AsRef<Path>) -> Self {
        Engine {
            vault_root: vault_root.as_ref().to_path_buf(),
            ports: None,
        }
    }

    /// 시계, 난수, 환경 변수, 잠금, git, sqlite3를 바꿔 끼운 약속 묶음으로 일하는 엔진(시험과 다른 저장소 구현을 위한 자리).
    /// 명령을 부르는 동안만 이 묶음이 현재 스레드에 걸린다. 엔진이 새로 만든 스레드는 기본 어댑터를 쓴다.
    #[cfg(feature = "test-support")]
    pub fn with_ports(vault_root: impl AsRef<Path>, ports: Ports) -> Self {
        Engine {
            vault_root: vault_root.as_ref().to_path_buf(),
            ports: Some(Arc::new(ports)),
        }
    }

    /// 쓰기 명령인가
    pub fn is_write(command: &str) -> bool {
        find(command).is_some_and(|def| def.access == Access::Write)
    }

    /// 이 엔진이 처리하는 명령인가(아니면 호출 쪽이 다른 길로 보낸다)
    pub fn supports(command: &str) -> bool {
        find(command).is_some()
    }

    /// 호출 맥락(누가, 어느 세션에서)을 인자 밖에서 받아 명령을 실행한다. 앱, 명령줄, MCP가 부르는 입구다.
    /// 맥락은 입구가 정하는 값이다. 요청 타입은 맥락 칸을 갖지 않으므로 인자에 `caller` 따위가 있어도 읽지 않는다.
    pub fn call_as(
        &self,
        context: &CallContext,
        command: &str,
        args: Value,
    ) -> Result<Value, EngineError> {
        let _installed = self.ports.clone().map(crate::ports::install);
        match find(command) {
            Some(def) => (def.run)(self, context, args),
            None => Err(EngineError::unsupported(format!(
                "이 엔진에는 {command} 명령이 없다"
            ))),
        }
    }

    /// 이름과 인자 객체로 명령을 실행하고 응답 JSON을 돌려줍니다.
    /// 과도기 입구: 호출 맥락을 인자의 `caller`·`session`·`sessionAgent` 칸에서 읽어 [`Engine::call_as`]로 넘긴다. 시험과 녹화 하네스가 쓴다.
    /// 새 호출은 [`Engine::call_as`]를 쓴다.
    pub fn call(&self, command: &str, args: Value) -> Result<Value, EngineError> {
        let context = context_from_args(command, &args)?;
        self.call_as(&context, command, args)
    }

    /// 쓰기 범위를 **이 한 곳에서** 연다(잠금 → 표식·색인 맞추기 → 자동 커밋 기록). 명령은 범위를 열지 않고 열린 범위(`&WriteScope`)를 받는다.
    /// 범위는 응답을 만든 뒤에, 성공이든 실패든 놓인다. 어떻게 열지는 명령 표의 [`Scope`]가 정한다.
    fn open_scope(&self, scope: Scope, context: &CallContext) -> Result<WriteScope, EngineError> {
        let opened = pipeline::open(&self.vault_root, scope != Scope::NoMarker, context)?;
        if scope == Scope::WithLedger {
            // 범위를 연 직후, 쓰기보다 먼저: 승인 기록장이 없으면 기준선을 만든다
            crate::resource::approvals::ledger(&self.vault_root)?;
        }
        Ok(opened)
    }
}

/// 과도기 입구용: 인자 객체의 `caller`·`session`·`sessionAgent` 칸을 호출 맥락으로 읽는다. 객체가 아닌 인자는 맥락이 없는 것으로 보고
/// (요청 읽기가 같은 오류를 낸다), 칸 값이 틀리면 요청 읽기와 같은 글로 거절한다.
fn context_from_args(command: &str, args: &Value) -> Result<CallContext, EngineError> {
    match args {
        Value::Object(_) => serde_json::from_value(args.clone())
            .map_err(|e| EngineError::invalid_argument(format!("{command} 인자가 잘못됐다: {e}"))),
        _ => Ok(CallContext::default()),
    }
}

fn find(command: &str) -> Option<&'static table::Def> {
    table::TABLE.iter().find(|def| def.name == command)
}

fn parse_args<T: DeserializeOwned>(command: &str, args: Value) -> Result<T, EngineError> {
    // 인자가 없으면 빈 객체로 본다
    let args = if args.is_null() {
        Value::Object(Default::default())
    } else {
        args
    };
    serde_json::from_value(args)
        .map_err(|e| EngineError::invalid_argument(format!("{command} 인자가 잘못됐다: {e}")))
}

fn to_json<T: serde::Serialize>(value: T) -> Result<Value, EngineError> {
    serde_json::to_value(value)
        .map_err(|e| EngineError::internal(format!("응답을 JSON으로 못 만들었다: {e}")))
}

/// 처리 함수의 결과를 응답 JSON으로 바꾼다.
fn finish<T: serde::Serialize>(result: Result<T, EngineError>) -> Result<Value, EngineError> {
    result.and_then(to_json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use std::collections::BTreeSet;

    /// 같은 이름이 두 줄이면 뒤 줄은 영영 불리지 않는다.
    #[test]
    fn command_names_are_unique() {
        let names: Vec<&str> = catalog().map(|c| c.name).collect();
        let unique: BTreeSet<&str> = names.iter().copied().collect();
        assert_eq!(names.len(), unique.len(), "표에 같은 이름이 있다");
    }

    /// 쓰기 범위를 여는 명령은 쓰기 명령뿐이다. 읽기 명령이 잠금과 색인 맞추기를 타면 안 된다.
    #[test]
    fn only_write_commands_open_a_scope() {
        for command in catalog() {
            if command.access == Access::Read {
                assert_eq!(command.scope, Scope::None, "{}", command.name);
            }
        }
    }

    #[test]
    fn lookups_follow_the_table() {
        assert!(Engine::supports("get_doc") && !Engine::is_write("get_doc"));
        assert!(Engine::supports("create_doc") && Engine::is_write("create_doc"));
        assert!(Engine::supports("commit_vault") && Engine::is_write("commit_vault"));
        assert!(!Engine::supports("no_such_command") && !Engine::is_write("no_such_command"));
    }

    /// 과도기 입구는 인자의 맥락 칸을 읽고, 새 입구는 인자의 같은 이름 칸을 읽지 않는다.
    #[test]
    fn the_legacy_entry_reads_the_context_fields_and_the_new_entry_ignores_them() {
        let args = serde_json::json!({"id": "n1", "caller": "ai", "session": "s1", "sessionAgent": "codex"});
        assert_eq!(
            context_from_args("x", &args).unwrap(),
            CallContext::ai(Some("s1".into()), Some("codex".into()))
        );
        assert_eq!(
            context_from_args("x", &serde_json::json!({"id": "n1"})).unwrap(),
            CallContext::app()
        );
        assert_eq!(
            context_from_args("x", &Value::Null).unwrap(),
            CallContext::app()
        );
        // 객체가 아닌 인자는 맥락이 없는 것으로 보고 요청 읽기가 거절한다
        assert_eq!(
            context_from_args("x", &serde_json::json!([1])).unwrap(),
            CallContext::app()
        );
        // 칸 값이 틀리면 요청 읽기와 같은 글로 거절한다
        let error =
            context_from_args("get_doc", &serde_json::json!({"caller": "robot"})).unwrap_err();
        assert!(
            error
                .message
                .starts_with("get_doc 인자가 잘못됐다: unknown variant `robot`"),
            "{}",
            error.message
        );
    }

    #[test]
    fn an_unknown_command_is_unsupported() {
        let error = Engine::new(".")
            .call("no_such_command", Value::Null)
            .unwrap_err();
        assert_eq!(error.kind, ErrorKind::Unsupported);
        assert_eq!(error.message, "이 엔진에는 no_such_command 명령이 없다");
    }
}
