//! 저장된 에이전트의 순위별 모델로 한 번의 판단을 요청한다. 작업 파일과 GitHub 자격은 호스트가 관리한다.
use super::job::Ctx;
use crate::{
    ask,
    process::{self, Outcome},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap, path::PathBuf, process::Command, sync::atomic::AtomicBool, sync::Arc,
    time::Duration,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Tier {
    pub engine: String,
    pub model: String,
    pub effort: String,
}
#[derive(Clone, Debug)]
pub(super) struct Agent {
    pub instructions: String,
    pub tiers: Vec<Tier>,
    pub editable: bool,
}

pub(super) fn agents(ctx: &Ctx) -> Result<HashMap<String, Agent>, String> {
    let list = ctx.call(&["list-resources", "scope=global", "bodies=true"])?;
    let rows = list["resources"]
        .as_array()
        .ok_or("글로벌 에이전트 목록이 없다")?;
    let mut agents = HashMap::new();
    for r in rows {
        let meta = &r["tools"]["aisurada"];
        let Some(key) = meta["key"].as_str() else {
            continue;
        };
        if r["kind"] != "agent" || meta["enabled"] == false {
            continue;
        }
        let mut instructions = r["body"].as_str().unwrap_or_default().to_owned();
        if let Some(skills) = r["skills"].as_array() {
            for id in skills {
                let skill = rows
                    .iter()
                    .find(|s| s["kind"] == "skill" && s["id"] == *id)
                    .ok_or_else(|| format!("{key}에 연결된 스킬을 찾을 수 없다: {id}"))?;
                instructions.push_str(&format!(
                    "\n\n## {}\n{}",
                    skill["name"],
                    skill["body"].as_str().unwrap_or_default()
                ));
            }
        }
        let mut tiers = vec![];
        for (e, m, f) in [
            ("engine", "model", "effort"),
            ("fallbackEngine", "fallbackModel", "fallbackEffort"),
            ("thirdEngine", "thirdModel", "thirdEffort"),
        ] {
            let Some(engine) = meta[e].as_str() else {
                break;
            };
            let tool = &r["tools"][engine];
            let model = meta[m]
                .as_str()
                .or_else(|| tool["model"].as_str())
                .ok_or_else(|| format!("{key}의 {engine} 모델을 찾을 수 없다"))?;
            let effort = meta[f]
                .as_str()
                .or_else(|| tool["effort"].as_str())
                .unwrap_or("default");
            tiers.push(Tier {
                engine: engine.into(),
                model: model.into(),
                effort: effort.into(),
            });
        }
        if agents
            .insert(
                key.to_owned(),
                Agent {
                    instructions,
                    tiers,
                    editable: matches!(r["permission"].as_str(), Some("edit" | "publish")),
                },
            )
            .is_some()
        {
            return Err(format!("같은 키의 글로벌 에이전트가 둘이다: {key}"));
        }
    }
    Ok(agents)
}

fn binary(engine: &str) -> Option<PathBuf> {
    match engine {
        "claude" => ask::resolve_claude(),
        "gemini" => ask::resolve_gemini(),
        "codex" => std::env::var_os("AISURADA_CODEX_BIN")
            .map(PathBuf::from)
            .filter(|p| p.is_file())
            .or_else(|| {
                std::env::split_paths(&ask::augmented_path())
                    .map(|p| p.join("codex"))
                    .find(|p| p.is_file())
            }),
        _ => None,
    }
}

pub(super) fn call(
    t: &Tier,
    prompt: &str,
    cancel: Arc<AtomicBool>,
    timeout: Duration,
) -> Result<String, String> {
    let bin = binary(&t.engine).ok_or_else(|| format!("{} 실행 파일을 찾지 못했다", t.engine))?;
    // 저장소가 제공하는 프로젝트 설정이나 훅을 모델 CLI가 불러오지 않게 빈 폴더에서 시작한다.
    let empty = tempfile::tempdir().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(&bin);
    cmd.current_dir(empty.path())
        .env("PATH", ask::augmented_path())
        .env_remove("AISURADA_VAULT");
    let last = empty.path().join("answer.txt");
    let mut stdin = Some(prompt.as_bytes().to_vec());
    match t.engine.as_str() {
        "claude" => {
            // 화면에 저장한 모델 ID와 Claude CLI가 받는 표기를 맞춘다.
            let model = match t.model.as_str() {
                "claude-sonnet-5.5" => "claude-sonnet-5-5",
                "claude-opus-5.5" => "claude-opus-5-5",
                other => other,
            };
            cmd.args([
                "-p",
                "--restricted",
                "--safe-mode",
                "--strict-mcp-config",
                "--tools",
                "",
                "--no-session-persistence",
                "--permission-mode",
                "dontAsk",
                "--output-format",
                "text",
                "--model",
                model,
            ]);
            if t.effort != "default" {
                cmd.args(["--effort", &t.effort]);
            }
        }
        "codex" => {
            cmd.args([
                "exec",
                "--ephemeral",
                "--ignore-user-config",
                "--ignore-rules",
                "--skip-git-repo-check",
                "--sandbox",
                "read-only",
                "--disable",
                "shell_tool",
                "--disable",
                "plugins",
                "-m",
                &t.model,
            ]);
            if t.effort != "default" {
                cmd.args([
                    "-c",
                    &format!(
                        "model_reasoning_effort={}",
                        serde_json::Value::String(t.effort.clone())
                    ),
                ]);
            }
            cmd.arg("-o").arg(&last).arg("-");
        }
        "gemini" => {
            if bin.file_stem() == Some(std::ffi::OsStr::new("gemini")) {
                return Err("이 개발 실행에는 Antigravity CLI가 필요하다".into());
            }
            cmd.args([
                "--sandbox",
                "--mode",
                "plan",
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "--model",
                ask::map_gemini_model(&t.model),
            ]);
            if t.effort != "default" {
                cmd.args(["--effort", &t.effort]);
            }
            cmd.args(["-p", ""]);
            // 태스크 내용은 명령줄에 넣지 않고 표준입력으로 건넨다.
            let event = serde_json::json!({
                "event": "user",
                "message": {"role": "user", "content": [{"type": "text", "text": prompt}]}
            });
            stdin = Some(format!("{event}\n").into_bytes());
        }
        _ => return Err(format!("지원하지 않는 엔진: {}", t.engine)),
    }
    match process::run_typed_cancellable(&mut cmd, stdin, timeout, Some(cancel))
        .map_err(|e| e.into_io().to_string())?
    {
        Outcome::Finished(o) if o.status.success() => match t.engine.as_str() {
            "codex" => std::fs::read_to_string(last).map_err(|e| e.to_string()),
            "gemini" => gemini_answer(&o.stdout),
            _ => Ok(String::from_utf8_lossy(&o.stdout).into_owned()),
        },
        Outcome::Finished(o) => Err(format!(
            "{}: {}",
            o.status,
            ask::tail_chars(&String::from_utf8_lossy(&o.stderr))
        )),
        Outcome::TimedOut => Err("모델 응답 시간 초과".into()),
        Outcome::Cancelled => Err("실행 중지".into()),
    }
}

fn gemini_answer(stdout: &[u8]) -> Result<String, String> {
    let output = String::from_utf8_lossy(stdout);
    let event = output
        .lines()
        .rev()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .find(|event| event["event"] == "result")
        .ok_or("Antigravity 결과를 찾지 못했다")?;
    let result = &event["result"];
    if result["status"] == "SUCCESS" {
        return result["response"]
            .as_str()
            .map(str::to_owned)
            .ok_or("Antigravity 응답이 비어 있다".into());
    }
    Err(result["error"]
        .as_str()
        .unwrap_or("Antigravity 실행이 실패했다")
        .to_owned())
}
