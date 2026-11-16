//! Chapter 5: the only native-host bridge to the engine's command line (`ais-cli`).
//!
//! 앱 안에서 Rust `ais-cli`(`ais_cli::run`)를 같은 프로세스로 부른다. 사이드카도, 프로세스 띄우기도, Java도 없다.
use crate::lifecycle::HostState;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Component, Path, PathBuf},
};
use tauri::{Manager, Runtime, State};

pub type Environment = BTreeMap<OsString, OsString>;
/// 엔진이 「AI 세션이 보낸 요청」으로 읽는 환경변수(`SessionAgents`). 앱이 엔진에 넘기는 환경에서는 지운다.
const AI_SESSION_ENV: [&str; 5] = [
    "AISURADA_SESSION_ID",
    "AISURADA_SESSION_AGENT",
    "CLAUDE_CODE_SESSION_ID",
    "CODEX_SESSION_ID",
    "CODEX_THREAD_ID",
];
#[derive(Debug, Serialize, PartialEq)]
pub struct Reply {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}
#[derive(Clone)]
pub(crate) struct Context {
    resource: Option<PathBuf>,
    bundled: bool,
    cwd: PathBuf,
}

fn context<R: Runtime>(app: &tauri::AppHandle<R>) -> Result<Context, String> {
    Ok(Context {
        resource: app.path().resource_dir().ok(),
        bundled: !tauri::is_dev(),
        cwd: std::env::current_dir().map_err(|e| format!("작업 폴더를 못 읽었어요: {e}"))?,
    })
}

#[tauri::command]
pub async fn sidecar_call<R: Runtime>(
    app: tauri::AppHandle<R>,
    state: State<'_, HostState>,
    argv: Vec<String>,
) -> Result<Reply, String> {
    if argv.is_empty() {
        return Err("사이드카 인자가 비어있어요".into());
    }
    crate::call_log::sidecar(&argv);
    let sub = argv[0].trim().to_ascii_lowercase();
    if sub == "install"
        || sub == "sweep"
        || sub == "--sidecar"
        || argv.iter().any(|a| a == "--sidecar")
    {
        return Err(format!("허용되지 않은 사이드카 명령이에요: {}", argv[0]));
    }
    let vault = state
        .vault
        .lock()
        .map_err(|_| "vault 잠금을 얻지 못했다")?
        .clone()
        .ok_or("vault가 아직 열리지 않았어요")?;
    let context = context(&app)?;
    // 파일을 읽고 쓰는 일이라 화면 스레드가 아닌 일꾼 스레드에서 돈다
    tauri::async_runtime::spawn_blocking(move || {
        execute(&context, std::env::vars_os().collect(), &vault, &argv)
    })
    .await
    .map_err(|e| format!("엔진 호출이 끊겼어요: {e}"))
}
/// 화면을 거치지 않고 호스트 안에서(러너와 승인 알림처럼) 엔진 명령을 한 번 부른다. `sidecar_call`과 같은 길(`ais-cli`)을 쓰므로
/// 화면이 받는 것과 같은 응답과 같은 거절을 받는다. 호출 스레드를 막으므로 화면 스레드가 아닌 일꾼 스레드에서만 부른다.
pub fn call_blocking<R: Runtime>(
    app: &tauri::AppHandle<R>,
    vault: &str,
    argv: &[String],
) -> Result<Reply, String> {
    Ok(execute(
        &context(app)?,
        std::env::vars_os().collect(),
        vault,
        argv,
    ))
}
fn env_value(env: &Environment, key: &str) -> Option<String> {
    env.get(std::ffi::OsStr::new(key))
        .and_then(|v| v.to_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}
fn under_build_tree(path: &Path) -> bool {
    let parts = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(p) => p.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>();
    parts
        .windows(3)
        .any(|p| p[0] == "target" && matches!(p[1], "debug" | "release") && p[2] == "bundle")
}
/// 앱에 든 `ais-cli` 실행 파일의 자리. MCP 서버로 에이전트에 등록할 명령이 이것이다.
/// 환경변수 `AISURADA_CLI_BIN`, 앱 번들의 `bin/ais-cli`, 소스 저장소의 빌드 폴더(`engine-rs/target/{release,debug}`) 차례다. 못 찾으면 빈 글이다.
fn cli_exe(context: &Context, env: &Environment) -> String {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(explicit) = env_value(env, "AISURADA_CLI_BIN") {
        candidates.push(PathBuf::from(explicit));
    }
    if let Some(resource) = &context.resource {
        candidates.push(resource.join("bin/ais-cli"));
    }
    for root in context.cwd.ancestors().take(3) {
        if root.join("engine-rs/Cargo.toml").is_file() {
            for profile in ["release", "debug"] {
                candidates.push(root.join(format!("engine-rs/target/{profile}/ais-cli")));
            }
            break;
        }
    }
    candidates
        .into_iter()
        .find(|p| p.is_file())
        .map(|p| p.canonicalize().unwrap_or(p).to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// `ais_cli`가 읽는 환경. 앱이 부르는 명령은 사람(앱)의 통로이므로 AI 세션 표식은 지운다(#53). 호스트의 나머지 환경은 그대로 넘긴다
/// (엔진의 잠금 폴더, 자동 커밋 설정 같은 변수를 `ais-core`가 프로세스 환경에서 읽기 때문이다).
pub(crate) fn cli_env(context: &Context, env: &Environment, vault: &str) -> ais_cli::env::Env {
    let mut vars: BTreeMap<String, String> = env
        .iter()
        .filter_map(|(k, v)| Some((k.to_str()?.to_string(), v.to_str()?.to_string())))
        .collect();
    for key in AI_SESSION_ENV {
        vars.remove(key);
    }
    vars.insert("AISURADA_VAULT".into(), vault.into());
    ais_cli::env::Env {
        home: vars.get("HOME").cloned().unwrap_or_default(),
        cwd: context.cwd.to_string_lossy().into_owned(),
        exe: cli_exe(context, env),
        session: None,
        vars,
    }
}

/// 정식 앱(번들된 앱, 개발 실행과 빌드 폴더의 번들이 아닌 것)이고 번들에 `bin/ais-cli`가 있을 때만 `ais-cli`의 환경을 돌려준다.
/// 기기 전체에 영향을 주는 일(옛 사이드카 정리, MCP 재등록)은 이런 앱에서만 하려는 것이다.
pub(crate) fn official_cli_env<R: Runtime>(app: &tauri::AppHandle<R>) -> Option<ais_cli::env::Env> {
    let context = context(app).ok()?;
    let env: Environment = std::env::vars_os().collect();
    let in_build_tree = context
        .resource
        .as_ref()
        .is_some_and(|p| under_build_tree(p));
    if !context.bundled || in_build_tree {
        return None;
    }
    let cli_env = cli_env(&context, &env, "");
    (!cli_env.exe.is_empty()
        && context.resource.as_ref().is_some_and(|r| {
            std::path::Path::new(&cli_env.exe)
                .starts_with(r.canonicalize().unwrap_or_else(|_| r.clone()))
        }))
    .then_some(cli_env)
}

/// 앱 안에서 Rust `ais-cli`를 부른다. 엔진 안의 예상 밖 실패(패닉)가 앱을 죽이지 않게 종료 코드 1의 답으로 바꾼다.
fn execute(context: &Context, env: Environment, vault: &str, argv: &[String]) -> Reply {
    let cli_env = cli_env(context, &env, vault);
    let argv = argv.to_vec();
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ais_cli::run(&argv, &cli_env)
    })) {
        Ok(outcome) => Reply {
            status: outcome.code,
            stdout: outcome.stdout,
            stderr: outcome.stderr,
        },
        Err(panic) => {
            let detail = panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            Reply {
                status: 1,
                stdout: String::new(),
                stderr: format!("엔진 안에서 예외가 났다: {detail}\n"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    fn context(root: &Path, bundled: bool) -> Context {
        Context {
            resource: Some(root.join("resources")),
            cwd: root.join("work"),
            bundled,
        }
    }
    fn touch(path: &Path) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"fixture").unwrap();
    }
    fn env(pairs: &[(&str, &str)]) -> Environment {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string().into(), v.to_string().into()))
            .collect()
    }
    #[test]
    fn the_rust_cli_runs_in_process_and_reports_exit_codes() {
        let temp = tempfile::tempdir().unwrap();
        let vault = temp.path().join("vault");
        fs::create_dir_all(&vault).unwrap();
        let c = context(temp.path(), false);
        let e = env(&[("HOME", temp.path().to_str().unwrap())]);
        let listed = execute(
            &c,
            e.clone(),
            vault.to_str().unwrap(),
            &["list-docs".into()],
        );
        assert_eq!(
            (listed.status, listed.stdout.as_str()),
            (0, "[]\n"),
            "{}",
            listed.stderr
        );
        let unknown = execute(&c, e.clone(), vault.to_str().unwrap(), &["nope".into()]);
        assert_eq!(unknown.status, 2);
        assert!(unknown.stderr.starts_with("알 수 없는 이름: nope"));
        // vault가 없는 폴더는 종료 코드 2의 오류로 알린다
        let missing = execute(&c, e, "/no/such/vault", &["list-docs".into()]);
        assert_eq!(missing.status, 2);
        assert!(missing.stderr.contains("vault 뿌리가 폴더가 아니다"));
    }
    #[test]
    fn the_cli_environment_drops_ai_session_markers_and_names_the_vault() {
        let temp = tempfile::tempdir().unwrap();
        let c = context(temp.path(), true);
        let bin = c.resource.as_ref().unwrap().join("bin/ais-cli");
        touch(&bin);
        let e = env(&[
            ("HOME", "/home/x"),
            ("PATH", "/usr/bin"),
            ("AISURADA_VAULT", "/stale"),
            ("AISURADA_SESSION_ID", "s1"),
            ("CLAUDE_CODE_SESSION_ID", "s2"),
            ("CODEX_THREAD_ID", "s3"),
            ("KEEP_ME", "yes"),
        ]);
        let cli = cli_env(&c, &e, "/my vault");
        assert_eq!(
            cli.vars.get("AISURADA_VAULT").map(String::as_str),
            Some("/my vault")
        );
        assert_eq!(cli.vars.get("KEEP_ME").map(String::as_str), Some("yes"));
        for key in AI_SESSION_ENV {
            assert!(!cli.vars.contains_key(key), "{key}는 지워야 한다");
        }
        assert_eq!(cli.home, "/home/x");
        assert!(cli.session.is_none());
        // 등록할 실행 파일은 앱 번들의 bin/ais-cli다
        assert_eq!(Path::new(&cli.exe), bin.canonicalize().unwrap());
    }
    #[test]
    fn the_cli_executable_is_found_by_env_then_bundle_then_source_checkout() {
        let temp = tempfile::tempdir().unwrap();
        let mut c = context(temp.path(), false);
        assert_eq!(cli_exe(&c, &env(&[])), "", "아무것도 없으면 빈 글");
        let checkout = temp.path().join("work");
        c.cwd = checkout.join("native-host/src-tauri");
        touch(&checkout.join("engine-rs/Cargo.toml"));
        touch(&checkout.join("engine-rs/target/debug/ais-cli"));
        assert!(cli_exe(&c, &env(&[])).ends_with("engine-rs/target/debug/ais-cli"));
        touch(&checkout.join("engine-rs/target/release/ais-cli"));
        assert!(
            cli_exe(&c, &env(&[])).ends_with("engine-rs/target/release/ais-cli"),
            "release가 먼저"
        );
        touch(&c.resource.as_ref().unwrap().join("bin/ais-cli"));
        assert!(
            cli_exe(&c, &env(&[])).contains("resources/bin/ais-cli"),
            "번들이 먼저"
        );
        let explicit = temp.path().join("custom-cli");
        touch(&explicit);
        assert!(cli_exe(
            &c,
            &env(&[("AISURADA_CLI_BIN", explicit.to_str().unwrap())])
        )
        .ends_with("custom-cli"));
    }
    #[test]
    fn build_bundle_detection_is_by_adjacent_components() {
        for path in [
            "/a/target/debug/bundle/macos/app/Contents/Resources",
            "/a/target/release/bundle",
        ] {
            assert!(under_build_tree(Path::new(path)));
        }
        for path in [
            "/a/mytarget/debug/bundle",
            "/a/target/debug/x/bundle",
            "/a/target/other/bundle",
            "/a/target/debug/bundles",
        ] {
            assert!(!under_build_tree(Path::new(path)));
        }
    }
    #[test]
    fn unusual_arguments_come_back_as_an_error_reply() {
        // 인자 하나에 NUL이 든 경우처럼 예상 밖 입력도 앱을 죽이지 않고 오류 답으로 돌아온다
        let temp = tempfile::tempdir().unwrap();
        let c = context(temp.path(), false);
        let reply = execute(
            &c,
            env(&[("HOME", temp.path().to_str().unwrap())]),
            temp.path().to_str().unwrap(),
            &["get-doc".into(), "a\0b".into()],
        );
        assert_ne!(reply.status, 0);
    }
}
