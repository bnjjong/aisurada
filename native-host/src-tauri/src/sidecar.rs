//! Chapter 5: the only native-host bridge to the Java CLI.
use crate::{
    lifecycle::HostState,
    process::{self, Outcome},
};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Component, Path, PathBuf},
    process::Command,
    time::Duration,
};
use tauri::{Manager, Runtime, State};

pub type Environment = BTreeMap<OsString, OsString>;
const TIMEOUT: Duration = Duration::from_secs(120);
/// 엔진(`ais-cli`)이 이미 있는 launchd 등록을 덮어쓰지 않고 그대로 쓰게 하는 환경 변수.
const KEEP_REGISTRATION: &str = "AISURADA_SIDECAR_KEEP_REGISTRATION";
/// 엔진이 「AI 세션이 보낸 요청」으로 읽는 환경변수(`SessionAgents`). 앱이 엔진에 넘기는 환경에서는 지운다.
const AI_SESSION_ENV: [&str; 5] = [
    "AISURADA_SESSION_ID",
    "AISURADA_SESSION_AGENT",
    "CLAUDE_CODE_SESSION_ID",
    "CODEX_SESSION_ID",
    "CODEX_THREAD_ID",
];
const TIMEOUT_MESSAGE: &str = "ais-cli 가 120초 안에 안 끝나서 끊었다. 사이드카가 답하지 않는다.";
#[derive(Debug, Serialize, PartialEq)]
pub struct Reply {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}
#[derive(Clone)]
struct Context {
    resource: Option<PathBuf>,
    bundled: bool,
    cwd: PathBuf,
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
    let sub = argv[0].trim().to_ascii_lowercase();
    if sub == "install" || sub == "sweep" || sub == "--sidecar" || argv.iter().any(|a| a == "--sidecar") {
        return Err(format!("허용되지 않은 사이드카 명령이에요: {}", argv[0]));
    }
    let vault = state
        .vault
        .lock()
        .map_err(|_| "vault 잠금을 얻지 못했다")?
        .clone()
        .ok_or("vault가 아직 열리지 않았어요")?;
    let context = Context {
        resource: app.path().resource_dir().ok(),
        bundled: !tauri::is_dev(),
        cwd: std::env::current_dir().map_err(|e| format!("작업 폴더를 못 읽었어요: {e}"))?,
    };
    tauri::async_runtime::spawn_blocking(move || {
        execute(
            &context,
            std::env::vars_os().collect(),
            &vault,
            &argv,
            TIMEOUT,
        )
    })
    .await
    .map_err(|e| format!("사이드카 호출이 끊겼어요: {e}"))?
}
/// 화면을 거치지 않고 호스트 안에서(러너와 승인 알림처럼) 엔진 명령을 한 번 부른다. `sidecar_call`과 같은 길(`ais-cli`)을 쓰므로
/// 화면이 받는 것과 같은 응답과 같은 거절을 받는다. 호출 스레드를 막으므로 화면 스레드가 아닌 일꾼 스레드에서만 부른다.
pub fn call_blocking<R: Runtime>(app: &tauri::AppHandle<R>, vault: &str, argv: &[String]) -> Result<Reply, String> {
    let context = Context {
        resource: app.path().resource_dir().ok(),
        bundled: !tauri::is_dev(),
        cwd: std::env::current_dir().map_err(|e| format!("작업 폴더를 못 읽었어요: {e}"))?,
    };
    execute(&context, std::env::vars_os().collect(), vault, argv, TIMEOUT)
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
fn module_candidates(context: &Context, module: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if context.bundled {
        if let Some(resource) = &context.resource {
            candidates.push(resource.join(format!("{module}.jar")));
        }
    }
    let relative = format!("aisurada-backend/{module}/build/libs/{module}.jar");
    if let Some(parent) = context.cwd.parent() {
        candidates.push(parent.join(&relative));
    }
    candidates.push(context.cwd.join(&relative));
    // This reconstruction builds Java jars in dist/, rather than the original Gradle modules.
    // Only a recognizable source checkout may supply this additional development candidate.
    for root in context.cwd.ancestors().take(3) {
        if root.join("build.sh").is_file() && root.join("src/aisurada").is_dir() {
            candidates.push(root.join(format!("dist/{module}.jar")));
            break;
        }
    }
    candidates
}
fn choose_with(
    env: &Environment,
    key: &str,
    candidates: impl FnOnce() -> Vec<PathBuf>,
) -> (Option<PathBuf>, Vec<PathBuf>) {
    let candidates = if let Some(value) = env_value(env, key) {
        vec![PathBuf::from(value)]
    } else {
        candidates()
    };
    let found = candidates.iter().find(|p| p.exists()).cloned();
    (found, candidates)
}
#[cfg(test)]
fn choose(
    env: &Environment,
    key: &str,
    candidates: Vec<PathBuf>,
) -> (Option<PathBuf>, Vec<PathBuf>) {
    choose_with(env, key, || candidates)
}
fn missing(what: &str, key: &str, paths: &[PathBuf]) -> String {
    format!(
        "{what}를 못 찾았어요. {key} 환경변수에 절대 경로를 넣어 주세요.\n찾아본 자리:\n{}",
        paths
            .iter()
            .map(|p| format!("  - {}", p.display()))
            .collect::<Vec<_>>()
            .join("\n")
    )
}
fn java_candidates(context: &Context, env: &Environment) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(resource) = &context.resource {
        let runtime = resource.join("runtime/bin/java");
        let exists = runtime.exists();
        candidates.push(runtime);
        if exists {
            return candidates;
        }
    }
    if let Some(path) = env.get(std::ffi::OsStr::new("PATH")) {
        for directory in std::env::split_paths(path) {
            let file = context.cwd.join(directory).join("java");
            if file.is_file() {
                candidates.push(file);
                break;
            }
        }
    }
    candidates.push(PathBuf::from("/usr/bin/java"));
    candidates
}
fn execute(
    context: &Context,
    mut env: Environment,
    vault: &str,
    argv: &[String],
    timeout: Duration,
) -> Result<Reply, String> {
    let (jar, paths) = choose_with(&env, "AISURADA_CLI_JAR", || {
        module_candidates(context, "ais-cli")
    });
    let jar = jar.ok_or_else(|| missing("ais-cli.jar", "AISURADA_CLI_JAR", &paths))?;
    let (java, paths) = choose_with(&env, "AISURADA_SIDECAR_JAVA", || {
        java_candidates(context, &env)
    });
    let java = java.ok_or_else(|| missing("자바 실행 파일", "AISURADA_SIDECAR_JAVA", &paths))?;
    // 개발 실행(`tauri dev`)과 `target/…/bundle` 아래의 번들은 엔진 사본을 믿지 않아 jar를 넘기지 않는다.
    // 이런 앱이 정식 앱의 엔진 등록을 설치본 jar로 덮어쓰지 않게, jar를 직접 지정하지 않았다면 등록 유지 스위치를 켠다.
    let guest = !context.bundled
        || context
            .resource
            .as_ref()
            .is_some_and(|p| under_build_tree(p));
    let explicit_jar = env_value(&env, "AISURADA_SIDECAR_JAR").is_some();
    let (sidecar, _) = choose_with(&env, "AISURADA_SIDECAR_JAR", || {
        if guest {
            Vec::new()
        } else {
            module_candidates(context, "ais-sidecar")
        }
    });
    env.insert("AISURADA_VAULT".into(), vault.into());
    // 앱 화면과 러너가 부르는 엔진 명령은 사람(앱)의 통로다. 앱을 AI 세션의 터미널에서 띄웠다면 그 세션 표식이 환경에 남아 있을 수 있는데,
    // 그대로 넘기면 엔진이 화면의 저장과 승인을 AI의 것으로 보고(#53) 승인을 거절하거나 대기에 올린다.
    for key in AI_SESSION_ENV {
        env.remove(std::ffi::OsStr::new(key));
    }
    env.remove(std::ffi::OsStr::new("AISURADA_SIDECAR_JAR"));
    env.remove(std::ffi::OsStr::new(KEEP_REGISTRATION));
    if let Some(path) = sidecar {
        env.insert("AISURADA_SIDECAR_JAR".into(), path.into_os_string());
    } else if guest && !explicit_jar {
        env.insert(KEEP_REGISTRATION.into(), "1".into());
    }
    let mut command = Command::new(&java);
    command
        .arg("-jar")
        .arg(&jar)
        .args(argv)
        .env_clear()
        .envs(env);
    match process::run(&mut command, None, timeout).map_err(|e| {
        format!(
            "ais-cli 를 못 띄웠어요: {e} (java={}, jar={})",
            java.display(),
            jar.display()
        )
    })? {
        Outcome::Finished(output) => Ok(Reply {
            status: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }),
        Outcome::TimedOut => Ok(Reply {
            status: -1,
            stdout: String::new(),
            stderr: TIMEOUT_MESSAGE.into(),
        }),
        Outcome::Cancelled => Ok(Reply {
            status: -1,
            stdout: String::new(),
            stderr: "취소되었습니다.".into(),
        }),
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
    fn jar_search_order_override_and_directory_existence() {
        let temp = tempfile::tempdir().unwrap();
        let dev = context(temp.path(), false);
        let bundle = context(temp.path(), true);
        let resources = dev.resource.as_ref().unwrap().join("ais-cli.jar");
        touch(&resources);
        let parent = temp
            .path()
            .join("aisurada-backend/ais-cli/build/libs/ais-cli.jar");
        touch(&parent);
        let local = dev.cwd.join("aisurada-backend/ais-cli/build/libs/ais-cli.jar");
        touch(&local);
        assert_eq!(
            choose(
                &env(&[]),
                "AISURADA_CLI_JAR",
                module_candidates(&dev, "ais-cli")
            )
            .0,
            Some(parent.clone())
        );
        assert_eq!(
            choose(
                &env(&[]),
                "AISURADA_CLI_JAR",
                module_candidates(&bundle, "ais-cli")
            )
            .0,
            Some(resources)
        );
        assert!(choose(
            &env(&[("AISURADA_CLI_JAR", "/no-such-jar")]),
            "AISURADA_CLI_JAR",
            module_candidates(&dev, "ais-cli")
        )
        .0
        .is_none());
        let directory = dev.cwd.clone();
        assert_eq!(
            choose(
                &env(&[("AISURADA_CLI_JAR", directory.to_str().unwrap())]),
                "AISURADA_CLI_JAR",
                vec![]
            )
            .0,
            Some(directory)
        );
        fs::remove_file(&parent).unwrap();
        assert_eq!(
            choose(
                &env(&[]),
                "AISURADA_CLI_JAR",
                module_candidates(&dev, "ais-cli")
            )
            .0,
            Some(local)
        );
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
    fn java_uses_runtime_then_regular_path_files_and_empty_path_entry() {
        let temp = tempfile::tempdir().unwrap();
        let c = context(temp.path(), false);
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        fs::create_dir_all(first.join("java")).unwrap();
        touch(&second.join("java"));
        touch(&c.cwd.join("java"));
        let path = format!("{}:{}", first.display(), second.display());
        let candidates = java_candidates(&c, &env(&[("PATH", &path)]));
        assert_eq!(candidates[1], second.join("java"));
        assert_eq!(
            java_candidates(&c, &env(&[("PATH", "")]))[1],
            c.cwd.join("java")
        );
        touch(&c.resource.as_ref().unwrap().join("runtime/bin/java"));
        assert_eq!(
            choose(&env(&[]), "AISURADA_SIDECAR_JAVA", candidates).0,
            Some(c.resource.unwrap().join("runtime/bin/java"))
        );
    }
    #[cfg(unix)]
    fn script(path: &Path, text: &str) {
        use std::os::unix::fs::PermissionsExt;
        touch(path);
        fs::write(path, format!("#!/bin/sh\n{text}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    #[test]
    #[cfg(unix)]
    fn process_contract_preserves_argv_environment_and_nonzero_output() {
        let temp = tempfile::tempdir().unwrap();
        let c = context(temp.path(), false);
        let jar = temp.path().join("cli.jar");
        touch(&jar);
        let java = temp.path().join("java");
        script(&java,"printf '%s\\n' \"$@\"; printf '%s|%s|%s|%s' \"$AISURADA_VAULT\" \"${AISURADA_SIDECAR_JAR-unset}\" \"$AISURADA_CLI_JAR\" \"$AISURADA_SIDECAR_JAVA\" >&2; exit 5");
        let e = env(&[
            ("AISURADA_CLI_JAR", jar.to_str().unwrap()),
            ("AISURADA_SIDECAR_JAVA", java.to_str().unwrap()),
            ("AISURADA_SIDECAR_JAR", "/missing"),
        ]);
        let result = execute(
            &c,
            e,
            "/vault//",
            &["get-doc".into(), "a=b\n한글".into()],
            TIMEOUT,
        )
        .unwrap();
        assert_eq!(result.status, 5);
        assert_eq!(
            result.stdout,
            format!("-jar\n{}\nget-doc\na=b\n한글\n", jar.display())
        );
        assert_eq!(
            result.stderr,
            format!("/vault//|unset|{}|{}", jar.display(), java.display())
        );
    }
    #[test]
    #[cfg(unix)]
    fn lossy_utf8_signal_timeout_and_spawn_errors() {
        let temp = tempfile::tempdir().unwrap();
        let c = context(temp.path(), false);
        let jar = temp.path().join("cli.jar");
        touch(&jar);
        let java = temp.path().join("java");
        let e = env(&[
            ("AISURADA_CLI_JAR", jar.to_str().unwrap()),
            ("AISURADA_SIDECAR_JAVA", java.to_str().unwrap()),
        ]);
        script(&java, "printf '\\377'; printf '\\376' >&2; exit 3");
        let r = execute(&c, e.clone(), "/v", &[], TIMEOUT).unwrap();
        assert_eq!(r.stdout, "�");
        assert_eq!(r.stderr, "�");
        assert_eq!(r.status, 3);
        script(&java, "kill -TERM $$");
        assert_eq!(
            execute(&c, e.clone(), "/v", &[], TIMEOUT).unwrap().status,
            -1
        );
        script(&java, "exec /bin/sleep 10");
        let r = execute(&c, e.clone(), "/v", &[], Duration::from_millis(50)).unwrap();
        assert_eq!(
            r,
            Reply {
                status: -1,
                stdout: String::new(),
                stderr: TIMEOUT_MESSAGE.into()
            }
        );
        fs::remove_file(&java).unwrap();
        fs::create_dir(&java).unwrap();
        assert!(execute(&c, e, "/v", &[], TIMEOUT)
            .unwrap_err()
            .contains("못 띄웠어요"));
    }
    #[test]
    #[cfg(unix)]
    fn ai_session_variables_do_not_reach_the_engine() {
        let temp = tempfile::tempdir().unwrap();
        let c = context(temp.path(), false);
        let jar = temp.path().join("cli.jar");
        touch(&jar);
        let java = temp.path().join("java");
        script(
            &java,
            "printf '%s|%s|%s|%s|%s|%s' \"${AISURADA_SESSION_ID-unset}\" \"${AISURADA_SESSION_AGENT-unset}\" \"${CLAUDE_CODE_SESSION_ID-unset}\" \"${CODEX_SESSION_ID-unset}\" \"${CODEX_THREAD_ID-unset}\" \"${KEEP_ME-unset}\"",
        );
        let e = env(&[
            ("AISURADA_CLI_JAR", jar.to_str().unwrap()),
            ("AISURADA_SIDECAR_JAVA", java.to_str().unwrap()),
            ("AISURADA_SESSION_ID", "s1"),
            ("AISURADA_SESSION_AGENT", "codex"),
            ("CLAUDE_CODE_SESSION_ID", "s2"),
            ("CODEX_SESSION_ID", "s3"),
            ("CODEX_THREAD_ID", "s4"),
            ("KEEP_ME", "yes"),
        ]);
        // 세션 표식은 지우고 나머지 환경은 그대로 넘긴다
        assert_eq!(
            execute(&c, e, "/v", &[], TIMEOUT).unwrap().stdout,
            "unset|unset|unset|unset|unset|yes"
        );
    }
    #[test]
    #[cfg(unix)]
    fn sidecar_export_obeys_dev_build_bundle_and_explicit_override() {
        let temp = tempfile::tempdir().unwrap();
        let mut c = context(temp.path(), true);
        let jar = temp.path().join("cli.jar");
        touch(&jar);
        let java = temp.path().join("java");
        // 엔진 jar와 등록 유지 스위치를 `jar|스위치`로 찍는다.
        script(
            &java,
            "printf '%s|%s' \"${AISURADA_SIDECAR_JAR-unset}\" \"${AISURADA_SIDECAR_KEEP_REGISTRATION-unset}\"",
        );
        let e = env(&[
            ("AISURADA_CLI_JAR", jar.to_str().unwrap()),
            ("AISURADA_SIDECAR_JAVA", java.to_str().unwrap()),
        ]);
        let sidecar = c.resource.as_ref().unwrap().join("ais-sidecar.jar");
        touch(&sidecar);
        let shown = |c: &Context, e: &Environment| {
            execute(c, e.clone(), "/v", &[], TIMEOUT).unwrap().stdout
        };
        // 정식 번들: 앱의 jar를 넘기고, 등록은 평소처럼 앱 것으로 쓴다.
        assert_eq!(
            shown(&c, &e),
            format!("{}|unset", sidecar.to_string_lossy())
        );
        // 개발 실행: jar를 안 넘기고, 이미 있는 등록은 덮어쓰지 않는다.
        c.bundled = false;
        assert_eq!(shown(&c, &e), "unset|1");
        c.bundled = true;
        c.resource = Some(
            temp.path()
                .join("target/debug/bundle/macos/app/Contents/Resources"),
        );
        touch(&c.resource.as_ref().unwrap().join("ais-sidecar.jar"));
        // 빌드 폴더의 번들도 같다.
        assert_eq!(shown(&c, &e), "unset|1");
        // 사용자가 jar를 정했으면 그 jar로 등록한다(스위치는 끈다).
        let mut explicit = e.clone();
        explicit.insert("AISURADA_SIDECAR_JAR".into(), sidecar.as_os_str().into());
        assert_eq!(
            shown(&c, &explicit),
            format!("{}|unset", sidecar.to_string_lossy())
        );
        // 정한 jar가 없는 파일이면 jar는 안 넘기지만, 정하겠다는 뜻이 있었으므로 스위치도 켜지 않는다.
        let mut missing = e.clone();
        missing.insert(
            "AISURADA_SIDECAR_JAR".into(),
            "/no/such/ais-sidecar.jar".into(),
        );
        assert_eq!(shown(&c, &missing), "unset|unset");
        // 호스트가 정한다. 바깥 환경에서 스위치를 물려받지 않는다.
        let mut inherited = e;
        inherited.insert(KEEP_REGISTRATION.into(), "1".into());
        c.resource = Some(temp.path().join("resources"));
        assert_eq!(
            shown(&c, &inherited),
            format!("{}|unset", sidecar.to_string_lossy())
        );
    }
    #[test]
    fn reconstruction_uses_dist_only_in_a_recognizable_source_checkout() {
        let temp = tempfile::tempdir().unwrap();
        let mut c = context(temp.path(), false);
        c.cwd = temp.path().join("native-host/src-tauri");
        let jar = temp.path().join("dist/ais-cli.jar");
        touch(&jar);
        assert!(choose(
            &env(&[]),
            "AISURADA_CLI_JAR",
            module_candidates(&c, "ais-cli")
        )
        .0
        .is_none());
        touch(&temp.path().join("build.sh"));
        fs::create_dir_all(temp.path().join("src/aisurada")).unwrap();
        assert_eq!(
            choose(
                &env(&[]),
                "AISURADA_CLI_JAR",
                module_candidates(&c, "ais-cli")
            )
            .0,
            Some(jar)
        );
    }
    #[test]
    fn explicit_override_never_evaluates_fallback_candidates() {
        let e = env(&[("AISURADA_CLI_JAR", "\u{3000}/missing-override\u{3000}")]);
        let result = choose_with(&e, "AISURADA_CLI_JAR", || panic!("fallback evaluated"));
        assert!(result.0.is_none());
        assert_eq!(result.1, vec![PathBuf::from("/missing-override")]);
    }
}
