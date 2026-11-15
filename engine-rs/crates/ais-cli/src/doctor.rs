//! `ais-cli doctor [--prune [--dry-run]]`: 지금 기기의 상태를 읽기만 하며 보여 준다.
//!
//! Java 시절에는 사이드카의 소켓·상태 파일·plist를 진단했다. 사이드카가 없는 지금은 어느 vault를 쓰는지, vault의 git 상태, 이 실행 파일,
//! 에이전트에 등록된 MCP 서버, 그리고 걷어 내야 할 옛 사이드카 흔적을 보여 준다. `--prune`은 걷어 내기(`sweep`)까지 한다.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use ais_core::json::{self, JsonValue};
use ais_core::vault::git::Git;

use crate::Outcome;
use crate::env::{Env, normalize_path, resolve_vault};
use crate::old_sidecar;
use crate::register::{self, Kind};

/// "얼마 전" 글: 60초 미만은 초, 60분 미만은 분, 24시간 미만은 시간, 그 위는 일
fn ago(epoch_secs: i64) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    if epoch_secs > now {
        return format!("(시각이 미래다: {epoch_secs})");
    }
    let seconds = now - epoch_secs;
    match seconds {
        0..=59 => format!("{seconds}초 전"),
        60..=3599 => format!("{}분 전", seconds / 60),
        3600..=86399 => format!("{}시간 전", seconds / 3600),
        _ => format!("{}일 전", seconds / 86400),
    }
}

/// vault의 git 진단 줄들. git 저장소가 아니면 한 줄이다.
fn vault_git(vault: &str, lines: &mut Vec<String>) {
    let git_dir = Path::new(vault).join(".git");
    if !git_dir.is_dir() {
        lines.push("vault git  git 저장소가 아니다. 자동 커밋이 안 돈다".to_string());
        return;
    }
    let git = Git::legacy(Path::new(vault));
    let log = git.run(
        &[
            "--no-optional-locks",
            "log",
            "-1",
            "--author=aisurada-auto",
            "--format=%ct %h %s",
        ],
        b"",
    );
    let status = git.run(
        &[
            "--no-optional-locks",
            "status",
            "--porcelain",
            "--untracked-files=all",
        ],
        b"",
    );

    let mut commit = "자동 커밋이 아직 없다".to_string();
    if log.code == 0 {
        let text = log.text();
        let mut parts = text.trim().splitn(3, ' ');
        if let (Some(epoch), Some(hash)) = (parts.next(), parts.next())
            && let Ok(epoch) = epoch.parse::<i64>()
        {
            commit = format!(
                "마지막 자동 커밋 {} ({hash} {})",
                ago(epoch),
                parts.next().unwrap_or("")
            );
        }
    }
    let mut outside = "커밋 밖 (못 쟀다)".to_string();
    if status.code == 0 {
        // `*.tmp`는 뺀다. git은 한글 같은 글자가 든 경로를 따옴표로 감싸서 줄 끝이 `.tmp"`이다
        let count = status
            .text()
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.ends_with(".tmp") && !l.ends_with(".tmp\""))
            .count();
        outside = format!("커밋 밖 {count}개");
    }
    lines.push(format!("vault git  {commit}  {outside}"));

    if git_dir.join("index.lock").exists() {
        let age = std::fs::metadata(git_dir.join("index.lock"))
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| ago(d.as_secs() as i64))
            .unwrap_or_else(|| "(모름)".to_string());
        lines.push(format!(
            "⚠ .git/index.lock 이 있다({age}에 생겼다). 자동 커밋과 되돌리기가 이것에 막힌다"
        ));
    } else if [
        "rebase-merge",
        "rebase-apply",
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
    ]
    .iter()
    .any(|name| git_dir.join(name).exists())
    {
        lines.push(
            "⚠ vault 에서 병합이나 리베이스가 진행 중이다. 끝날 때까지 자동 커밋을 건너뛴다"
                .to_string(),
        );
    }
}

/// 에이전트 하나의 MCP 등록 상태를 한 줄로. 등록 명령의 `status` 출력을 읽는다.
fn registration_line(label: &str, found_key: &str, output: &str) -> String {
    let Ok(status) = json::parse(output) else {
        return format!("  {label}  (상태를 못 읽었다)");
    };
    let flag =
        |key: &str| matches!(status.get(key), Some(JsonValue::Literal(raw)) if raw == "true");
    if !flag(found_key) {
        return format!("  {label}  설치되어 있지 않다(건너뜀)");
    }
    if !flag("registered") {
        return format!("  {label}  등록 안 됨");
    }
    let mode = if flag("write") {
        "읽기+쓰기"
    } else {
        "읽기 전용"
    };
    if flag("stale") {
        format!(
            "  {label}  ★낡은 등록({mode}). Java 시절의 jar를 가리켜 앱을 새로 받으면 못 뜬다. `ais-cli {} add{}` 로 다시 등록한다",
            match found_key {
                "claudeFound" => "claude-mcp",
                "codexFound" => "codex-mcp",
                _ => "gemini-mcp",
            },
            if flag("write") { " --write" } else { "" }
        )
    } else {
        format!("  {label}  등록됨({mode})")
    }
}

pub fn run(args: &[String], env: &Env) -> Outcome {
    let mut prune = false;
    let mut dry_run = false;
    let mut notes: Vec<String> = Vec::new();
    for arg in args {
        match arg.as_str() {
            "--prune" => prune = true,
            "--dry-run" => dry_run = true,
            "--plist" => notes.push(
                "알림: 사이드카가 없어서 plist를 만들지 않는다(--plist는 이제 아무 일도 안 한다)"
                    .to_string(),
            ),
            _ => {}
        }
    }

    let mut lines: Vec<String> = Vec::new();
    match resolve_vault(env) {
        Some(raw) => {
            let trimmed = ais_core::text::trim(&raw).to_string();
            let vault = normalize_path(&trimmed, &env.cwd).unwrap_or(trimmed);
            let from = if env
                .var("AISURADA_VAULT")
                .is_some_and(|v| !ais_core::text::is_blank(v))
            {
                "환경변수 AISURADA_VAULT"
            } else {
                "앱이 열어 둔 vault"
            };
            lines.push(format!("vault      {vault}  ({from})"));
            if !Path::new(&vault).is_dir() {
                lines.push("⚠ vault 폴더가 없다".to_string());
            } else {
                vault_git(&vault, &mut lines);
            }
        }
        None => lines.push(
            "vault      (못 정했다. AISURADA_VAULT를 주거나 앱을 열어 vault를 연다)".to_string(),
        ),
    }
    lines.push(format!(
        "엔진       ais-cli {}  {}  (사이드카 없이 이 프로세스가 직접 돈다)",
        env!("CARGO_PKG_VERSION"),
        if env.exe.is_empty() {
            "(자리를 모름)"
        } else {
            &env.exe
        }
    ));

    lines.push("MCP 등록".to_string());
    let status = |outcome: Outcome| outcome.stdout;
    let args_status = vec!["status".to_string()];
    lines.push(registration_line(
        "Claude Code",
        "claudeFound",
        &status(register::claude_mcp(&args_status, env)),
    ));
    lines.push(registration_line(
        "Codex",
        "codexFound",
        &status(register::agent_mcp(Kind::Codex, &args_status, env)),
    ));
    lines.push(registration_line(
        "Gemini",
        "geminiFound",
        &status(register::agent_mcp(Kind::Gemini, &args_status, env)),
    ));

    let home = Path::new(&env.home);
    let inventory = old_sidecar::inventory(home);
    if inventory.is_empty() {
        lines.push("옛 사이드카  없다".to_string());
    } else {
        lines.push(format!(
            "옛 사이드카  {}개, jar {}개 남아 있다. `ais-cli sweep` 으로 걷어 낸다",
            inventory.traces.len(),
            inventory.jars.len()
        ));
        for trace in &inventory.traces {
            lines.push(format!(
                "  {}  vault={}  ({})",
                trace.label,
                trace.vault.as_deref().unwrap_or("(못 읽음)"),
                if trace.alive { "돈다" } else { "안 돈다" }
            ));
        }
    }

    let mut outcome = Outcome {
        stdout: lines.join("\n") + "\n",
        stderr: notes.iter().map(|n| format!("{n}\n")).collect(),
        code: 0,
    };
    if prune {
        let report = old_sidecar::sweep(env, home, dry_run);
        outcome.stdout.push('\n');
        outcome.stdout.push_str(&(report.lines.join("\n") + "\n"));
        for left in &report.left {
            outcome.stderr.push_str(&format!("{left}\n"));
        }
        if !report.left.is_empty() {
            outcome.code = 1;
        }
    }
    outcome
}
