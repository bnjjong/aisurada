//! 앱 안에 들어온 Rust 엔진(`ais-core`)을 화면이 부르는 길.
//!
//! 옮겨진 명령은 `sidecar_call`(`ais-cli`의 명령줄 길) 대신 이 길로 앱 안에서 바로 처리한다.
//! 화면은 먼저 `engine_commands`로 어떤 명령이 옮겨졌는지 묻고, 옮겨진 명령만 `engine_call`로 보낸다.

use crate::lifecycle::HostState;
use ais_core::{engine::Engine, error::EngineError};
use serde_json::Value;
use tauri::State;

/// `java`로 두면 옮겨진 명령도 Java 엔진으로 보낸다(Rust 엔진에 문제가 생겼을 때 되돌리는 스위치).
const ENGINE_SWITCH: &str = "AISURADA_ENGINE";

fn rust_engine_enabled() -> bool {
    !std::env::var(ENGINE_SWITCH).is_ok_and(|v| v.trim().eq_ignore_ascii_case("java"))
}

/// 쓰기 명령을 옛 Java 엔진으로 되돌리는 스위치. `java`일 때만 쓰기가 꺼진다. 5단계 전에는 쓰기가 기본 꺼짐(`rust`로 켬)이었으나,
/// 실제 창 확인을 마치고 기본을 켬으로 바꿨다. 읽기 스위치(`AISURADA_ENGINE=java`)가 우선이라 그것이 `java`면 쓰기도 꺼진다.
const WRITE_SWITCH: &str = "AISURADA_ENGINE_WRITE";
/// 옛 스냅숏 쓰기(설정으로 켜는 선택 기능, 기본 꺼짐)는 Rust 엔진이 하지 않는다. 켜져 있으면 쓰기를 Java 엔진에 맡긴다.
const SNAPSHOT_SWITCH: &str = "AISURADA_VAULT_SNAPSHOT_ENABLED";

fn rust_write_enabled() -> bool {
    write_enabled(|name| std::env::var(name).ok())
}

/// 환경 읽는 법을 받는 [`rust_write_enabled`]의 몸(시험이 환경을 건드리지 않고 확인한다).
fn write_enabled(get: impl Fn(&str) -> Option<String>) -> bool {
    let is = |name: &str, want: &str| get(name).is_some_and(|v| v.trim().eq_ignore_ascii_case(want));
    !is(ENGINE_SWITCH, "java") && !is(WRITE_SWITCH, "java") && !is(SNAPSHOT_SWITCH, "true")
}

/// 알릴 명령 목록. 쓰기 명령은 쓰기 스위치가 켜졌을 때만 낀다.
fn commands(write_enabled: bool) -> Vec<&'static str> {
    ais_core::engine::COMMANDS
        .iter()
        .copied()
        .filter(|c| write_enabled || !Engine::is_write(c))
        .collect()
}

/// Rust 엔진이 처리하는 명령 이름. 스위치가 `java`면 비어 있어서 모든 명령이 Java 엔진으로 간다.
#[tauri::command]
pub fn engine_commands() -> Vec<&'static str> {
    if rust_engine_enabled() {
        commands(rust_write_enabled())
    } else {
        Vec::new()
    }
}

/// Rust 엔진으로 명령 하나를 처리한다. 지금 열린 vault를 대상으로 하고, 요청마다 vault를 새로 훑는다.
/// 파일을 읽느라 걸리는 일이라 화면 스레드가 아닌 일꾼 스레드에서 돈다.
#[tauri::command]
pub async fn engine_call(
    state: State<'_, HostState>,
    command: String,
    args: Option<Value>,
) -> Result<Value, EngineError> {
    if Engine::is_write(&command) && !rust_write_enabled() {
        return Err(EngineError::unsupported(format!(
            "쓰기 명령 {command}는 Rust 엔진에서 꺼져 있다({WRITE_SWITCH}=java나 스냅숏 쓰기 설정 때문이다)"
        )));
    }
    let vault = state
        .vault
        .lock()
        .map_err(|_| EngineError::internal("vault 잠금을 얻지 못했다"))?
        .clone()
        .ok_or_else(|| EngineError::invalid_argument("vault가 아직 열리지 않았어요"))?;
    let started = std::time::Instant::now();
    let traced = command.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        Engine::new(vault).call(&command, args.unwrap_or(Value::Null))
    })
    .await
    .map_err(|e| EngineError::internal(format!("엔진 호출이 끊겼어요: {e}")))?;
    if trace_enabled() {
        let outcome = match &result {
            Ok(_) => "ok".to_string(),
            Err(e) => format!("{} {:?}", e.code, e.kind),
        };
        eprintln!(
            "[engine] {traced} {outcome} {}ms",
            started.elapsed().as_millis()
        );
    }
    result
}

/// 손으로 확인할 때 어느 명령이 Rust 엔진으로 갔는지 터미널에 한 줄씩 찍는 스위치(`AISURADA_ENGINE_TRACE=1`)
fn trace_enabled() -> bool {
    std::env::var("AISURADA_ENGINE_TRACE").is_ok_and(|v| !v.trim().is_empty() && v.trim() != "0")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{path::Path, process::Command};
    use tauri::{
        ipc::{CallbackFn, InvokeBody},
        test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime},
        webview::InvokeRequest,
        Manager, WebviewWindow,
    };

    fn invoke(
        window: &WebviewWindow<MockRuntime>,
        name: &str,
        value: Value,
    ) -> Result<Value, Value> {
        get_ipc_response(
            window,
            InvokeRequest {
                cmd: name.into(),
                callback: CallbackFn(0),
                error: CallbackFn(1),
                url: "tauri://localhost".parse().unwrap(),
                body: InvokeBody::Json(value),
                headers: Default::default(),
                invoke_key: tauri::test::INVOKE_KEY.into(),
            },
        )
        .map(|v| v.deserialize().unwrap())
    }

    /// 실제 Tauri 호출 경로(IPC)로 `engine_call`을 부르면, 열린 vault를 읽은 응답이나 `{code, kind, message}` 오류가 화면에 닿는다.
    #[test]
    fn engine_call_reads_the_open_vault_through_ipc() {
        let vault = std::env::temp_dir().join(format!("ais-host-engine-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&vault);
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../engine-rs/tools/compat/make-fixture.sh");
        let made = Command::new("bash")
            .arg(script)
            .arg(&vault)
            .output()
            .unwrap();
        assert!(
            made.status.success(),
            "합성 vault를 못 만들었다: {}",
            String::from_utf8_lossy(&made.stderr)
        );

        let app = mock_builder()
            .manage(HostState::default())
            .invoke_handler(tauri::generate_handler![engine_commands, engine_call])
            .build(mock_context(noop_assets()))
            .unwrap();
        let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();

        // 지원 목록
        let commands = invoke(&window, "engine_commands", json!({})).unwrap();
        assert!(commands
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c == "list_docs"));

        // vault가 열리기 전에는 인자 오류로 거절한다
        let refused = invoke(
            &window,
            "engine_call",
            json!({"command": "list_docs", "args": {"limit": 0}}),
        )
        .unwrap_err();
        assert_eq!(refused["kind"], "invalid_argument");

        // vault를 열면 읽는다
        *app.state::<HostState>().vault.lock().unwrap() = Some(vault.to_string_lossy().to_string());
        let docs = invoke(
            &window,
            "engine_call",
            json!({"command": "list_docs", "args": {"limit": 0, "bodies": true}}),
        )
        .unwrap();
        assert_eq!(docs["total"], 21);
        assert!(docs["items"][0]["body"].is_string());

        // 노트 하나를 여는 길: 본문과 코멘트가 붙은 노트, 후속 노트(부모 사슬로), 오류 종류
        let doc = invoke(
            &window,
            "engine_call",
            json!({"command": "get_doc", "args": {"id": "ndfx0002-00000002"}}),
        )
        .unwrap();
        assert_eq!(doc["comments"]["state"], "loaded");
        let subs = invoke(
            &window,
            "engine_call",
            json!({"command": "list_sub_notes", "args": {"parent": "ndfx0002-00000002/snfx0002-00000012", "bodies": true}}),
        )
        .unwrap();
        assert_eq!(subs["total"], 2);
        let no_doc = invoke(
            &window,
            "engine_call",
            json!({"command": "get_doc", "args": {"id": "ndzzzz-00000000"}}),
        )
        .unwrap_err();
        assert_eq!(no_doc["kind"], "not_found");

        // 판 목록: git 저장소가 없는 vault는 판이 없고, 번호를 주면 conflict다
        let history = invoke(
            &window,
            "engine_call",
            json!({"command": "history", "args": {"id": "ndfx0002-00000002"}}),
        )
        .unwrap();
        assert_eq!(
            (
                history["count"].as_u64(),
                history["versions"].as_array().map(Vec::len)
            ),
            (Some(0), Some(0))
        );
        let no_version = invoke(
            &window,
            "engine_call",
            json!({"command": "history", "args": {"id": "ndfx0002-00000002", "number": 1}}),
        )
        .unwrap_err();
        assert_eq!(no_version["kind"], "conflict");

        // 인자가 없어도(null) 빈 인자로 읽고, 틀린 인자와 모르는 값은 종류로 가른다
        let agendas = invoke(&window, "engine_call", json!({"command": "list_agendas"})).unwrap();
        assert_eq!(agendas["total"], 4);
        let missing = invoke(
            &window,
            "engine_call",
            json!({"command": "list_docs", "args": {"agenda": "없는-아젠다"}}),
        )
        .unwrap_err();
        assert_eq!(
            (missing["code"].as_str(), missing["kind"].as_str()),
            (Some("AIS-404"), Some("not_found"))
        );
        let bad_args = invoke(
            &window,
            "engine_call",
            json!({"command": "list_docs", "args": {"limit": "많이"}}),
        )
        .unwrap_err();
        assert_eq!(bad_args["kind"], "invalid_argument");
        // 쓰기 명령도 기본으로 Rust 엔진이 받는다(되돌리는 스위치를 안 켰다면). 거절은 쓰기 스위치가 아니라 인자 때문이다
        let write = invoke(
            &window,
            "engine_call",
            json!({"command": "update_doc", "args": {}}),
        )
        .unwrap_err();
        assert_ne!(write["kind"], "unsupported");

        let _ = std::fs::remove_dir_all(&vault);
    }

    #[test]
    fn write_commands_are_hidden_until_the_write_switch_is_on() {
        let reads = commands(false);
        let all = commands(true);
        assert!(
            reads.contains(&"list_docs")
                && !reads.contains(&"create_agenda")
                && !reads.contains(&"commit_vault")
        );
        assert!(
            all.contains(&"create_agenda")
                && all.contains(&"commit_vault")
                && all.contains(&"list_docs")
        );
        assert_eq!(
            all.len(),
            reads.len() + ais_core::engine::WRITE_COMMANDS.len()
        );
    }

    #[test]
    fn lists_the_commands_the_engine_handles() {
        let commands = engine_commands();
        assert!(
            commands.contains(&"list_docs")
                && commands.contains(&"reindex")
                && commands.contains(&"get_doc")
                && commands.contains(&"search_docs")
                && commands.contains(&"history")
                && commands.contains(&"update_doc"),
            "읽기와 쓰기 명령을 모두 알린다"
        );
    }

    #[test]
    fn writes_are_on_by_default_and_each_rollback_switch_turns_them_off() {
        let with = |pairs: &'static [(&'static str, &'static str)]| {
            write_enabled(move |name| {
                pairs.iter().find(|(k, _)| *k == name).map(|(_, v)| v.to_string())
            })
        };
        assert!(with(&[]), "기본은 켜짐");
        assert!(with(&[("AISURADA_ENGINE_WRITE", "rust")]), "옛 값 rust도 켜짐");
        assert!(!with(&[("AISURADA_ENGINE_WRITE", " JAVA ")]));
        assert!(!with(&[("AISURADA_ENGINE", "java")]));
        assert!(!with(&[("AISURADA_VAULT_SNAPSHOT_ENABLED", "true")]));
        assert!(with(&[("AISURADA_VAULT_SNAPSHOT_ENABLED", "false")]));
    }

    #[test]
    fn engine_error_reaches_the_screen_with_code_and_kind() {
        let error = Engine::new("/nonexistent-vault")
            .call("no_such_command", Value::Null)
            .unwrap_err();
        let json = serde_json::to_value(&error).unwrap();
        assert_eq!(json["code"], "AIS-409");
        assert_eq!(json["kind"], "unsupported");
        assert!(json["message"].as_str().unwrap().contains("no_such_command"));
    }
}
