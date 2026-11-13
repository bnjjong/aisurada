//! 러너와 claude 사이의 약속. 태스크를 프롬프트로 바꾸고, claude의 마지막 답을 `[보고]`와 `[질문]`으로 가르고,
//! claude를 읽기 전용으로 띄우는 인자를 만든다. 프로세스를 띄우거나 파일을 만지지 않는 순수한 계산만 둔다.
use serde_json::Value;
use std::path::Path;

pub const REPORT_MARK: &str = "[보고]";
pub const QUESTION_MARK: &str = "[질문]";
/// 댓글 하나에 싣는 글자 수의 상한. claude가 아주 긴 답을 해도 태스크 파일이 부풀지 않게 한다.
pub const COMMENT_CHARS: usize = 12_000;

/// `get-task` 응답에서 러너에 필요한 것만 뽑은 것.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskInfo {
    pub key: String,
    pub title: String,
    pub kind: String,
    pub priority: String,
    pub status: String,
    pub repo: String,
    pub base: Option<String>,
    pub body: String,
    pub criteria: Vec<(String, bool)>,
    pub refs: Vec<TaskRef>,
    pub links: Vec<TaskLink>,
    pub comments: Vec<Comment>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaskRef {
    pub kind: String,
    pub target: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaskLink {
    pub kind: String,
    pub key: String,
    pub title: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Comment {
    pub by: String,
    pub body: String,
}

fn text(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

impl TaskInfo {
    pub fn from_json(v: &Value) -> Result<TaskInfo, String> {
        let key = text(v, "key");
        if key.is_empty() {
            return Err("태스크 응답에 key가 없다".into());
        }
        let list = |name: &str| v.get(name).and_then(Value::as_array).cloned().unwrap_or_default();
        Ok(TaskInfo {
            key,
            title: text(v, "title"),
            kind: text(v, "type"),
            priority: text(v, "priority"),
            status: text(v, "status"),
            repo: text(v, "repo"),
            base: v.get("base").and_then(Value::as_str).map(str::trim).filter(|b| !b.is_empty()).map(str::to_string),
            body: text(v, "body"),
            criteria: list("criteria")
                .iter()
                .map(|c| (text(c, "text"), c.get("done").and_then(Value::as_bool).unwrap_or(false)))
                .collect(),
            refs: list("refs").iter().map(|r| TaskRef { kind: text(r, "kind"), target: text(r, "target"), title: text(r, "title") }).collect(),
            links: list("links")
                .iter()
                .map(|l| TaskLink { kind: text(l, "type"), key: text(l, "key"), title: text(l, "title"), status: text(l, "status") })
                .collect(),
            comments: list("activity")
                .iter()
                .filter(|a| a.get("kind").and_then(Value::as_str) == Some("comment"))
                .map(|a| Comment { by: text(a, "by"), body: text(a, "body") })
                .collect(),
        })
    }
}

fn link_label(kind: &str) -> &str {
    match kind {
        "blocked-by" => "이 태스크를 막고 있음",
        "blocks" => "이 태스크가 막고 있음",
        "relates" => "관련",
        "duplicates" => "중복",
        "duplicated-by" => "중복됨",
        other => other,
    }
}

/// 첫 호출에 줄 프롬프트. 태스크 전체와 할 수 있는 일, 끝내는 법을 담는다.
pub fn build_prompt(t: &TaskInfo) -> String {
    let mut p = String::new();
    p.push_str("너는 aisurada의 개발 보조 에이전트다. 지금은 **읽기 전용 분석** 단계다. 코드와 노트를 고칠 수 없다. ");
    p.push_str("아래 태스크를 조사하고, 완료 기준마다 어디까지 알아냈는지 보고하라.\n\n");
    p.push_str(&format!("## 태스크 {} ({}, 우선순위 {})\n", t.key, or_dash(&t.kind), or_dash(&t.priority)));
    p.push_str(&format!("제목: {}\n", t.title));
    p.push_str(&format!(
        "저장소: {}{}\n",
        t.repo,
        t.base.as_deref().map(|b| format!(" (기준 브랜치: {b})")).unwrap_or_default()
    ));
    if !t.body.trim().is_empty() {
        p.push_str(&format!("\n설명:\n{}\n", t.body.trim()));
    }
    p.push_str("\n완료 기준:\n");
    for (text, done) in &t.criteria {
        p.push_str(&format!("- [{}] {}\n", if *done { "x" } else { " " }, text));
    }
    if !t.refs.is_empty() {
        p.push_str("\n연결한 자원:\n");
        for r in &t.refs {
            let title = if r.title.is_empty() { String::new() } else { format!(" — {}", r.title) };
            match r.kind.as_str() {
                "note" => p.push_str(&format!("- 노트 {}{} (aisurada의 get_doc 도구로 읽는다)\n", r.target, title)),
                "github" => p.push_str(&format!("- GitHub {}{} (웹에는 접근할 수 없으니 저장소 사본과 노트에서 찾는다)\n", r.target, title)),
                other => p.push_str(&format!("- {} {}{}\n", other, r.target, title)),
            }
        }
    }
    if !t.links.is_empty() {
        p.push_str("\n연결된 태스크:\n");
        for l in &t.links {
            p.push_str(&format!("- {} {} \"{}\" ({})\n", link_label(&l.kind), l.key, l.title, l.status));
        }
    }
    if !t.comments.is_empty() {
        p.push_str("\n지금까지의 댓글(오래된 것이 앞):\n");
        for c in &t.comments {
            p.push_str(&format!("- [{}] {}\n", c.by, c.body.trim().replace('\n', "\n  ")));
        }
    }
    p.push_str(INSTRUCTIONS);
    p
}

const INSTRUCTIONS: &str = "
## 할 수 있는 일
- Read, Grep, Glob: 현재 폴더(저장소 사본)만 읽을 수 있다. 다른 폴더는 막혀 있다.
- aisurada 도구(search_docs, get_doc, list_docs, get_task 따위): vault의 노트를 읽는다. 사람이 알려 줄 만한 맥락이 노트에 있을 수 있으니, 질문하기 전에 먼저 찾아본다.
- 파일 수정, 명령 실행, 웹 접근은 막혀 있다. 막힌 것을 우회하려고 하지 않는다.

## 끝내는 법
마지막 답의 **첫 줄**을 아래 둘 중 하나로 시작한다.
- `[보고]` 조사 결과. 완료 기준마다 근거(파일 경로와 줄)와 판단을 쓰고, 이어서 할 일을 제안한다. 확인하지 못한 것은 못 했다고 쓴다.
- `[질문]` 노트와 코드를 찾아봐도 풀리지 않고 사람만 답할 수 있는 질문. 한 번에 하나만 묻고, 선택지가 있으면 보기와 추천을 붙인다.
";

fn or_dash(s: &str) -> &str {
    if s.is_empty() {
        "-"
    } else {
        s
    }
}

/// 사람의 답이 왔을 때 이어 가는 호출에 줄 프롬프트.
pub fn answer_prompt(question: &str, answer: &str, last_question: bool) -> String {
    let mut p = format!("사람이 네 질문에 답했다.\n\n질문: {}\n답: {}\n\n이 답을 반영해서 이어 조사하라.", question.trim(), answer.trim());
    if last_question {
        p.push_str(" 질문 한도에 닿았으니 더는 묻지 말고, 지금까지 알아낸 것으로 `[보고]`를 쓴다. 모르는 부분은 모른다고 적는다.");
    } else {
        p.push_str(" 끝낼 때는 처음 알려 준 대로 `[보고]`나 `[질문]`으로 시작한다.");
    }
    p
}

/// 앱을 껐다 켜서 끊긴 실행을 이어 가는 호출에 줄 프롬프트.
pub fn recover_prompt() -> String {
    "앱이 닫히면서 작업이 중간에 끊겼다. 지금까지 한 조사를 이어서 마저 하고, 처음 알려 준 대로 `[보고]`나 `[질문]`으로 시작하는 답으로 끝낸다.".to_string()
}

/// claude의 마지막 답이 가리키는 다음 일.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    Report(String),
    Question(String),
}

/// 마지막 답을 가른다. 첫 줄 머리에 `[질문]`이 있으면 질문이고, `[보고]`나 표시가 없으면 보고다.
/// 표시 앞의 마크다운 꾸밈(`**`, `#`, `>`)은 걷어 내고 본다.
pub fn parse_reply(raw: &str) -> Reply {
    let trimmed = raw.trim();
    let bare = trimmed.trim_start_matches(|c: char| matches!(c, '*' | '#' | '>' | '`' | ' ' | '_'));
    if let Some(rest) = bare.strip_prefix(QUESTION_MARK) {
        return Reply::Question(rest.trim_start_matches(|c: char| matches!(c, '*' | '`' | '_' | ':' | ' ')).trim().to_string());
    }
    if let Some(rest) = bare.strip_prefix(REPORT_MARK) {
        return Reply::Report(rest.trim_start_matches(|c: char| matches!(c, '*' | '`' | '_' | ':' | ' ')).trim().to_string());
    }
    Reply::Report(trimmed.to_string())
}

/// claude `--output-format json`의 결과 한 건.
#[derive(Debug, Clone, PartialEq)]
pub struct ClaudeResult {
    pub text: String,
    pub is_error: bool,
    pub session_id: Option<String>,
    pub cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// 표준출력을 읽는다. 마지막 줄이 JSON이 아니면(경고가 앞에 붙는 경우) 앞에서부터 JSON이 시작하는 줄을 찾는다.
pub fn parse_claude_output(stdout: &str) -> Result<ClaudeResult, String> {
    let trimmed = stdout.trim();
    let value: Value = serde_json::from_str(trimmed)
        .ok()
        .or_else(|| {
            trimmed
                .lines()
                .rev()
                .find_map(|l| serde_json::from_str::<Value>(l.trim()).ok().filter(|v| v.get("type").is_some() || v.get("result").is_some()))
        })
        .ok_or_else(|| format!("claude의 결과를 읽지 못했다: {}", clip(trimmed, 200)))?;
    // stream-json 모양이 섞여 배열로 오면 마지막 result를 쓴다
    let value = match value {
        Value::Array(items) => items
            .into_iter()
            .rev()
            .find(|v| v.get("type").and_then(Value::as_str) == Some("result"))
            .ok_or("claude의 결과 배열에 result가 없다")?,
        v => v,
    };
    let usage = value.get("usage");
    let n = |key: &str| usage.and_then(|u| u.get(key)).and_then(Value::as_u64).unwrap_or(0);
    Ok(ClaudeResult {
        text: value.get("result").and_then(Value::as_str).unwrap_or("").to_string(),
        is_error: value.get("is_error").and_then(Value::as_bool).unwrap_or(false),
        session_id: value.get("session_id").and_then(Value::as_str).map(str::to_string),
        cost_usd: value.get("total_cost_usd").and_then(Value::as_f64).unwrap_or(0.0),
        input_tokens: n("input_tokens") + n("cache_creation_input_tokens") + n("cache_read_input_tokens"),
        output_tokens: n("output_tokens"),
    })
}

/// 글을 [max]자로 자른다(넘으면 끝에 …).
pub fn clip(s: &str, max: usize) -> String {
    let t = s.trim();
    if t.chars().count() <= max {
        t.to_string()
    } else {
        let mut out: String = t.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

/// claude를 읽기 전용으로 띄우는 인자.
///
/// - `dontAsk`: 미리 허용한 도구만 쓰고, 그 밖은 묻지 않고 거절한다.
/// - 허용: 작업 폴더 안의 Read/Grep/Glob(`Read(<경로>/**)` 규칙 하나가 셋을 같은 폴더로 가둔다)과 aisurada 읽기 도구.
///   MCP 서버는 `--strict-mcp-config`로 이 호출이 준 것만 쓰고, 그 서버는 `mcp`(쓰기 도구 없음)로 띄운 읽기 전용이다.
/// - 거절: 셸, 파일 쓰기, 웹, 하위 에이전트.
pub fn claude_args(session_id: &str, resume: bool, model: &str, mcp_config: &str, workdir: &Path) -> Vec<String> {
    let mut a: Vec<String> = vec!["-p".into(), if resume { "--resume".into() } else { "--session-id".into() }, session_id.into()];
    if !model.trim().is_empty() {
        a.push("--model".into());
        a.push(model.trim().into());
    }
    for s in [
        "--output-format",
        "json",
        "--permission-mode",
        "dontAsk",
        "--permission-prompts",
        "none",
        "--strict-mcp-config",
        "--mcp-config",
    ] {
        a.push(s.into());
    }
    a.push(mcp_config.into());
    a.push("--allowedTools".into());
    a.push(format!("Read({}/**)", workdir.display()));
    a.push("mcp__aisurada".into());
    a.push("--disallowedTools".into());
    for t in ["Bash", "Edit", "Write", "NotebookEdit", "WebFetch", "WebSearch", "Agent", "Task"] {
        a.push(t.into());
    }
    a
}

/// `claude-mcp config`가 낸 `mcpServers` 조각에 이 실행이 다루는 vault를 박고, 한 줄 JSON으로 돌려준다.
/// 서버는 `--write` 없이 뜬 읽기 전용이어야 하므로 args에 `--write`가 있으면 거절한다.
pub fn mcp_config_for(config_output: &str, vault: &str, keep_registration: bool) -> Result<String, String> {
    let mut v: Value = serde_json::from_str(config_output.trim()).map_err(|e| format!("MCP 설정을 읽지 못했다: {e}"))?;
    let server = v
        .get_mut("mcpServers")
        .and_then(|s| s.get_mut("aisurada"))
        .ok_or("MCP 설정에 aisurada 서버가 없다")?;
    if server.get("args").and_then(Value::as_array).is_some_and(|a| a.iter().any(|x| x.as_str() == Some("--write"))) {
        return Err("MCP 서버가 쓰기 모드로 설정되어 있어 읽기 전용 실행에 쓸 수 없다".into());
    }
    let env = server
        .as_object_mut()
        .ok_or("MCP 서버 설정이 객체가 아니다")?
        .entry("env")
        .or_insert_with(|| Value::Object(Default::default()));
    let env = env.as_object_mut().ok_or("MCP env가 객체가 아니다")?;
    env.insert("AISURADA_VAULT".into(), Value::String(vault.to_string()));
    if keep_registration && !env.contains_key("AISURADA_SIDECAR_JAR") {
        env.insert("AISURADA_SIDECAR_KEEP_REGISTRATION".into(), Value::String("1".into()));
    }
    serde_json::to_string(&v).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Value {
        json!({
            "key": "TSK-7", "title": "빌드 방법 정리", "type": "feature", "priority": "high", "status": "ready-for-develop",
            "repo": "bnjjong/aisurada", "base": "main", "body": "README를 읽고\n정리해 줘",
            "criteria": [{"text": "README에 빌드 방법이 있다", "done": false}, {"text": "시험 명령이 있다", "done": true}],
            "refs": [{"kind": "note", "target": "ndabc-1234", "title": "작업 계획"}, {"kind": "github", "target": "bnjjong/aisurada#53"}],
            "links": [{"type": "blocked-by", "key": "TSK-2", "title": "먼저 할 일", "status": "done"}],
            "activity": [
                {"kind": "created", "by": "human"},
                {"kind": "comment", "by": "human", "body": "첫 줄\n둘째 줄"},
                {"kind": "comment", "by": "runner", "body": "질문입니다"}
            ]
        })
    }

    #[test]
    fn task_info_reads_the_engine_shape() {
        let t = TaskInfo::from_json(&sample()).unwrap();
        assert_eq!((t.key.as_str(), t.repo.as_str(), t.base.as_deref()), ("TSK-7", "bnjjong/aisurada", Some("main")));
        assert_eq!(t.criteria, vec![("README에 빌드 방법이 있다".to_string(), false), ("시험 명령이 있다".to_string(), true)]);
        assert_eq!(t.refs.len(), 2);
        assert_eq!(t.links[0].kind, "blocked-by");
        assert_eq!(t.comments.len(), 2, "댓글만 뽑는다");
        assert!(TaskInfo::from_json(&json!({"title": "x"})).is_err());
        // 빈 base 는 없는 것으로 본다
        assert_eq!(TaskInfo::from_json(&json!({"key": "TSK-1", "base": " "})).unwrap().base, None);
    }

    #[test]
    fn prompt_carries_the_task_and_the_rules() {
        let p = build_prompt(&TaskInfo::from_json(&sample()).unwrap());
        for needle in [
            "TSK-7", "빌드 방법 정리", "bnjjong/aisurada (기준 브랜치: main)", "README를 읽고\n정리해 줘",
            "- [ ] README에 빌드 방법이 있다", "- [x] 시험 명령이 있다", "노트 ndabc-1234 — 작업 계획", "GitHub bnjjong/aisurada#53",
            "이 태스크를 막고 있음 TSK-2", "[human] 첫 줄\n  둘째 줄", "[runner] 질문입니다", "[보고]", "[질문]", "읽기 전용",
        ] {
            assert!(p.contains(needle), "프롬프트에 '{needle}'가 없다:\n{p}");
        }
        // 댓글·자원·연결이 없으면 그 절을 만들지 않는다
        let bare = TaskInfo::from_json(&json!({"key": "TSK-1", "title": "t", "repo": "a/b", "criteria": [{"text": "c"}]})).unwrap();
        let p = build_prompt(&bare);
        assert!(!p.contains("연결한 자원") && !p.contains("연결된 태스크") && !p.contains("지금까지의 댓글"));
    }

    #[test]
    fn answer_and_recover_prompts() {
        let a = answer_prompt("어느 브랜치?", "main", false);
        assert!(a.contains("질문: 어느 브랜치?") && a.contains("답: main") && a.contains("[질문]"));
        let last = answer_prompt("q", "a", true);
        assert!(last.contains("더는 묻지 말고"));
        assert!(recover_prompt().contains("끊겼다"));
    }

    #[test]
    fn reply_is_split_by_the_first_marker() {
        assert_eq!(parse_reply("[보고] 결과입니다\n자세히"), Reply::Report("결과입니다\n자세히".into()));
        assert_eq!(parse_reply("  [질문] main인가요?"), Reply::Question("main인가요?".into()));
        assert_eq!(parse_reply("**[질문]** 어느 쪽?"), Reply::Question("어느 쪽?".into()));
        assert_eq!(parse_reply("## [보고]\n내용"), Reply::Report("내용".into()));
        assert_eq!(parse_reply("[질문]: 콜론"), Reply::Question("콜론".into()));
        // 표시가 없으면 보고로 본다(말을 들었는데 일이 사라지지 않게)
        assert_eq!(parse_reply("그냥 결과"), Reply::Report("그냥 결과".into()));
        // 본문 가운데의 표시는 무시한다
        assert_eq!(parse_reply("요약입니다. [질문] 아님"), Reply::Report("요약입니다. [질문] 아님".into()));
        assert_eq!(parse_reply(""), Reply::Report(String::new()));
    }

    #[test]
    fn claude_output_is_read_with_usage_and_cost() {
        let out = r#"{"type":"result","subtype":"success","is_error":false,"result":"[보고] 끝","session_id":"abc","total_cost_usd":0.0123,"usage":{"input_tokens":6,"cache_creation_input_tokens":100,"cache_read_input_tokens":50,"output_tokens":40}}"#;
        let r = parse_claude_output(out).unwrap();
        assert_eq!((r.text.as_str(), r.is_error, r.session_id.as_deref()), ("[보고] 끝", false, Some("abc")));
        assert_eq!((r.input_tokens, r.output_tokens), (156, 40));
        assert!((r.cost_usd - 0.0123).abs() < 1e-9);
        // 앞에 경고 줄이 붙어도 읽는다
        let noisy = format!("warning: x\n{out}\n");
        assert_eq!(parse_claude_output(&noisy).unwrap().text, "[보고] 끝");
        // 오류 결과
        let err = parse_claude_output(r#"{"type":"result","is_error":true,"result":"실패"}"#).unwrap();
        assert!(err.is_error);
        // 배열
        let arr = format!("[{{\"type\":\"system\"}},{out}]");
        assert_eq!(parse_claude_output(&arr).unwrap().session_id.as_deref(), Some("abc"));
        assert!(parse_claude_output("not json").is_err());
        assert!(parse_claude_output("").is_err());
    }

    #[test]
    fn claude_args_are_read_only_and_scoped_to_the_workdir() {
        let w = Path::new("/work/TSK-1");
        let a = claude_args("sess", false, "sonnet", "{\"mcpServers\":{}}", w);
        let joined = a.join(" ");
        assert_eq!(&a[..3], ["-p", "--session-id", "sess"]);
        assert!(a.windows(2).any(|p| p == ["--permission-mode", "dontAsk"]));
        assert!(a.windows(2).any(|p| p == ["--permission-prompts", "none"]));
        assert!(a.contains(&"--strict-mcp-config".to_string()));
        assert!(a.windows(2).any(|p| p == ["--model", "sonnet"]));
        assert!(a.windows(2).any(|p| p == ["--allowedTools", "Read(/work/TSK-1/**)"]));
        assert!(a.contains(&"mcp__aisurada".to_string()));
        for denied in ["Bash", "Edit", "Write", "WebFetch", "WebSearch"] {
            let at = a.iter().position(|x| x == "--disallowedTools").unwrap();
            assert!(a[at..].contains(&denied.to_string()), "{denied}");
        }
        // 허용 목록에는 셸·쓰기가 없다
        let allowed: Vec<&String> = a.iter().skip_while(|x| *x != "--allowedTools").skip(1).take_while(|x| !x.starts_with("--")).collect();
        assert_eq!(allowed.len(), 2, "{joined}");
        assert!(!allowed.iter().any(|x| x.starts_with("Bash") || x.starts_with("Write") || x.starts_with("Edit")));
        // 이어 가기와 모델 생략
        let r = claude_args("sess", true, " ", "{}", w);
        assert_eq!(&r[..3], ["-p", "--resume", "sess"]);
        assert!(!r.contains(&"--model".to_string()));
    }

    #[test]
    fn mcp_config_pins_the_vault_and_refuses_write_mode() {
        let cfg = r#"{"mcpServers":{"aisurada":{"type":"stdio","command":"/j","args":["-jar","/a.jar","mcp"],"env":{"AISURADA_SIDECAR_JAR":"/s.jar"}}}}"#;
        let out: Value = serde_json::from_str(&mcp_config_for(cfg, "/my vault", true).unwrap()).unwrap();
        let env = &out["mcpServers"]["aisurada"]["env"];
        assert_eq!(env["AISURADA_VAULT"], "/my vault");
        assert_eq!(env["AISURADA_SIDECAR_JAR"], "/s.jar");
        assert!(env.get("AISURADA_SIDECAR_KEEP_REGISTRATION").is_none(), "엔진 jar가 있으면 등록 유지 스위치는 필요 없다");
        // env 가 비어 있으면 만들어서 박는다
        let no_env = r#"{"mcpServers":{"aisurada":{"type":"stdio","command":"/j","args":["-jar","/a.jar","mcp"]}}}"#;
        let out: Value = serde_json::from_str(&mcp_config_for(no_env, "/v", true).unwrap()).unwrap();
        assert_eq!(out["mcpServers"]["aisurada"]["env"]["AISURADA_SIDECAR_KEEP_REGISTRATION"], "1");
        let out: Value = serde_json::from_str(&mcp_config_for(no_env, "/v", false).unwrap()).unwrap();
        assert!(out["mcpServers"]["aisurada"]["env"].get("AISURADA_SIDECAR_KEEP_REGISTRATION").is_none());
        let write = r#"{"mcpServers":{"aisurada":{"command":"/j","args":["-jar","/a.jar","mcp","--write"]}}}"#;
        assert!(mcp_config_for(write, "/v", false).unwrap_err().contains("쓰기"));
        assert!(mcp_config_for("{}", "/v", false).is_err());
        assert!(mcp_config_for("망가짐", "/v", false).is_err());
    }

    #[test]
    fn clip_cuts_by_characters() {
        assert_eq!(clip(" 짧다 ", 10), "짧다");
        assert_eq!(clip("가나다라마", 3), "가나…");
    }
}
