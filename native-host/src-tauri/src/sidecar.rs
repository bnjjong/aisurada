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
const TIMEOUT_MESSAGE: &str = "wl-cli 가 120초 안에 안 끝나서 끊었다. 사이드카가 답하지 않는다.";
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
    let relative = format!("withlog-backend/{module}/build/libs/{module}.jar");
    if let Some(parent) = context.cwd.parent() {
        candidates.push(parent.join(&relative));
    }
    candidates.push(context.cwd.join(&relative));
    // This reconstruction builds Java jars in dist/, rather than the original Gradle modules.
    // Only a recognizable source checkout may supply this additional development candidate.
    for root in context.cwd.ancestors().take(3) {
        if root.join("build.sh").is_file() && root.join("src/withlog").is_dir() {
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
    let (jar, paths) = choose_with(&env, "WITHLOG_CLI_JAR", || {
        module_candidates(context, "wl-cli")
    });
    let jar = jar.ok_or_else(|| missing("wl-cli.jar", "WITHLOG_CLI_JAR", &paths))?;
    let (java, paths) = choose_with(&env, "WITHLOG_SIDECAR_JAVA", || {
        java_candidates(context, &env)
    });
    let java = java.ok_or_else(|| missing("자바 실행 파일", "WITHLOG_SIDECAR_JAVA", &paths))?;
    let (sidecar, _) = choose_with(&env, "WITHLOG_SIDECAR_JAR", || {
        if !context.bundled
            || context
                .resource
                .as_ref()
                .is_some_and(|p| under_build_tree(p))
        {
            Vec::new()
        } else {
            module_candidates(context, "wl-sidecar")
        }
    });
    env.insert("WITHLOG_VAULT".into(), vault.into());
    env.remove(std::ffi::OsStr::new("WITHLOG_SIDECAR_JAR"));
    if let Some(path) = sidecar {
        env.insert("WITHLOG_SIDECAR_JAR".into(), path.into_os_string());
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
            "wl-cli 를 못 띄웠어요: {e} (java={}, jar={})",
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
        let resources = dev.resource.as_ref().unwrap().join("wl-cli.jar");
        touch(&resources);
        let parent = temp
            .path()
            .join("withlog-backend/wl-cli/build/libs/wl-cli.jar");
        touch(&parent);
        let local = dev.cwd.join("withlog-backend/wl-cli/build/libs/wl-cli.jar");
        touch(&local);
        assert_eq!(
            choose(
                &env(&[]),
                "WITHLOG_CLI_JAR",
                module_candidates(&dev, "wl-cli")
            )
            .0,
            Some(parent.clone())
        );
        assert_eq!(
            choose(
                &env(&[]),
                "WITHLOG_CLI_JAR",
                module_candidates(&bundle, "wl-cli")
            )
            .0,
            Some(resources)
        );
        assert!(choose(
            &env(&[("WITHLOG_CLI_JAR", "/no-such-jar")]),
            "WITHLOG_CLI_JAR",
            module_candidates(&dev, "wl-cli")
        )
        .0
        .is_none());
        let directory = dev.cwd.clone();
        assert_eq!(
            choose(
                &env(&[("WITHLOG_CLI_JAR", directory.to_str().unwrap())]),
                "WITHLOG_CLI_JAR",
                vec![]
            )
            .0,
            Some(directory)
        );
        fs::remove_file(&parent).unwrap();
        assert_eq!(
            choose(
                &env(&[]),
                "WITHLOG_CLI_JAR",
                module_candidates(&dev, "wl-cli")
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
            choose(&env(&[]), "WITHLOG_SIDECAR_JAVA", candidates).0,
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
        script(&java,"printf '%s\\n' \"$@\"; printf '%s|%s|%s|%s' \"$WITHLOG_VAULT\" \"${WITHLOG_SIDECAR_JAR-unset}\" \"$WITHLOG_CLI_JAR\" \"$WITHLOG_SIDECAR_JAVA\" >&2; exit 5");
        let e = env(&[
            ("WITHLOG_CLI_JAR", jar.to_str().unwrap()),
            ("WITHLOG_SIDECAR_JAVA", java.to_str().unwrap()),
            ("WITHLOG_SIDECAR_JAR", "/missing"),
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
            ("WITHLOG_CLI_JAR", jar.to_str().unwrap()),
            ("WITHLOG_SIDECAR_JAVA", java.to_str().unwrap()),
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
    fn sidecar_export_obeys_dev_build_bundle_and_explicit_override() {
        let temp = tempfile::tempdir().unwrap();
        let mut c = context(temp.path(), true);
        let jar = temp.path().join("cli.jar");
        touch(&jar);
        let java = temp.path().join("java");
        script(&java, "printf '%s' \"${WITHLOG_SIDECAR_JAR-unset}\"");
        let e = env(&[
            ("WITHLOG_CLI_JAR", jar.to_str().unwrap()),
            ("WITHLOG_SIDECAR_JAVA", java.to_str().unwrap()),
        ]);
        let sidecar = c.resource.as_ref().unwrap().join("wl-sidecar.jar");
        touch(&sidecar);
        assert_eq!(
            execute(&c, e.clone(), "/v", &[], TIMEOUT).unwrap().stdout,
            sidecar.to_string_lossy()
        );
        c.bundled = false;
        assert_eq!(
            execute(&c, e.clone(), "/v", &[], TIMEOUT).unwrap().stdout,
            "unset"
        );
        c.bundled = true;
        c.resource = Some(
            temp.path()
                .join("target/debug/bundle/macos/app/Contents/Resources"),
        );
        touch(&c.resource.as_ref().unwrap().join("wl-sidecar.jar"));
        assert_eq!(
            execute(&c, e.clone(), "/v", &[], TIMEOUT).unwrap().stdout,
            "unset"
        );
        let mut e = e;
        e.insert("WITHLOG_SIDECAR_JAR".into(), sidecar.as_os_str().into());
        assert_eq!(
            execute(&c, e, "/v", &[], TIMEOUT).unwrap().stdout,
            sidecar.to_string_lossy()
        );
    }
    #[test]
    fn reconstruction_uses_dist_only_in_a_recognizable_source_checkout() {
        let temp = tempfile::tempdir().unwrap();
        let mut c = context(temp.path(), false);
        c.cwd = temp.path().join("native-host/src-tauri");
        let jar = temp.path().join("dist/wl-cli.jar");
        touch(&jar);
        assert!(choose(
            &env(&[]),
            "WITHLOG_CLI_JAR",
            module_candidates(&c, "wl-cli")
        )
        .0
        .is_none());
        touch(&temp.path().join("build.sh"));
        fs::create_dir_all(temp.path().join("src/withlog")).unwrap();
        assert_eq!(
            choose(
                &env(&[]),
                "WITHLOG_CLI_JAR",
                module_candidates(&c, "wl-cli")
            )
            .0,
            Some(jar)
        );
    }
    #[test]
    fn explicit_override_never_evaluates_fallback_candidates() {
        let e = env(&[("WITHLOG_CLI_JAR", "\u{3000}/missing-override\u{3000}")]);
        let result = choose_with(&e, "WITHLOG_CLI_JAR", || panic!("fallback evaluated"));
        assert!(result.0.is_none());
        assert_eq!(result.1, vec![PathBuf::from("/missing-override")]);
    }
}
