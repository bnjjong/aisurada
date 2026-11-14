//! 엔진의 진입점. 앱의 `engine_call`, `ais-cli`, MCP가 모두 이 한 곳으로 들어옵니다.

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::commands::{
    list_agendas, list_docs, list_projects, list_templates, list_trash, reindex,
};
use crate::error::EngineError;
use crate::vault::index::NoteIndex;

/// vault 하나를 맡는 엔진. 요청마다 vault를 새로 훑으므로 상태가 없다.
pub struct Engine {
    vault_root: PathBuf,
}

/// 이 엔진이 처리하는 명령 이름
pub const COMMANDS: &[&str] = &[
    "list_docs",
    "list_agendas",
    "list_projects",
    "list_templates",
    "list_trash",
    "reindex",
];

impl Engine {
    pub fn new(vault_root: impl AsRef<Path>) -> Self {
        Engine {
            vault_root: vault_root.as_ref().to_path_buf(),
        }
    }

    /// 이 엔진이 처리하는 명령인가(아니면 호출 쪽이 다른 길로 보낸다)
    pub fn supports(command: &str) -> bool {
        COMMANDS.contains(&command)
    }

    /// 이름과 인자 객체로 명령을 실행하고 응답 JSON을 돌려줍니다.
    pub fn call(&self, command: &str, args: Value) -> Result<Value, EngineError> {
        match command {
            "list_docs" => {
                let request: list_docs::ListDocsRequest = parse_args(command, args)?;
                to_json(list_docs::execute(
                    &NoteIndex::fresh(&self.vault_root),
                    &request,
                )?)
            }
            "list_agendas" => to_json(list_agendas::execute(&NoteIndex::fresh(&self.vault_root))),
            "list_projects" => to_json(list_projects::execute(&self.vault_root)),
            "list_templates" => {
                let request: list_templates::ListTemplatesRequest = parse_args(command, args)?;
                to_json(list_templates::execute(&self.vault_root, &request)?)
            }
            "list_trash" => to_json(list_trash::execute(&self.vault_root)),
            "reindex" => to_json(reindex::execute(&self.vault_root)),
            other => Err(EngineError::unsupported(format!(
                "이 엔진에는 {other} 명령이 없다"
            ))),
        }
    }
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
