//! Java 엔진(단발 모드)과 Rust `ais-cli`가 같은 명령줄에 같은 결과를 내는지 맞댄다.
//!
//! 사용: `cargo run -p ais-cli --example cli_compat -- <ais-engine.jar> <합성 vault> [--verbose]`
//!
//! 합성 vault를 사본 둘(Java용, Rust용)로 복사해 같은 명령줄을 차례로 돌린다. 명령마다 종료 코드, 표준 오류 글, 표준 출력 JSON(키 차례는
//! 무시)을 견주고, 쓰기 명령 뒤에는 두 사본 폴더 전체의 바이트도 견준다. 엔진 호환(`ais-core`의 `compat_check`)이 엔진 값을 이미 맞댔으므로
//! 여기서는 명령줄 해석, 자리 인자 이름, 글→수 변환, 옛 응답 모양 되돌림, 오류 글과 종료 코드를 본다.
//!
//! 진짜 vault에는 주지 않는다(쓰기 명령이 있다). 합성 vault는 `engine-rs/tools/compat/make-fixture.sh`로 만든다.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::{env, fs};

use ais_cli::env::Env;
use serde_json::{Value, json};

const NOW: &str = "1794700000000";

/// 한 번의 호출 결과
#[derive(Debug, Clone)]
struct Raw {
    code: i32,
    stdout: String,
    stderr: String,
}

#[derive(Default)]
struct Report {
    checked: usize,
    failures: Vec<String>,
    /// Java가 성공(종료 코드 0)으로 답한 호출 수. 양쪽이 똑같이 실패해서 맞아 보이는 일을 가려내는 데 쓴다.
    succeeded: usize,
}

struct Harness {
    report: Report,
    jar: String,
    root: PathBuf,
    a: PathBuf,
    b: PathBuf,
    step: u32,
    verbose: bool,
}

impl Harness {
    fn new(jar: &str, fixture: &Path, verbose: bool) -> Harness {
        let root = env::temp_dir().join(format!("ais-cli-compat-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("임시 폴더");
        let copy = |name: &str| -> PathBuf {
            let dst = root.join(name);
            let status = Command::new("cp")
                .arg("-R")
                .arg(fixture)
                .arg(&dst)
                .status()
                .expect("cp");
            assert!(status.success(), "사본을 못 만들었다");
            // 쓰기 명령은 워크스페이스 표식이 있어야 일한다
            fs::write(dst.join(".aisurada/workspace.json"), "{}\n").unwrap();
            dst
        };
        // SAFETY: 단일 스레드에서 한 번만 바꾼다.
        unsafe {
            env::set_var("AISURADA_TEST_NOW", NOW);
            env::set_var("AISURADA_LOCK_DIR", root.join("locks"));
            env::set_var("AISURADA_VAULT_AUTOCOMMIT_ENABLED", "false");
            env::set_var("AISURADA_APPROVALS_DIR", root.join("approvals-rust"));
            env::set_var("AISURADA_CLAUDE_HOME", root.join("session-home"));
            env::set_var("AISURADA_CODEX_HOME", root.join("session-home"));
            env::set_var("AISURADA_GEMINI_HOME", root.join("session-home"));
        }
        let (a, b) = (copy("java"), copy("rust"));
        Harness {
            report: Report::default(),
            jar: jar.to_string(),
            root,
            a,
            b,
            step: 0,
            verbose,
        }
    }

    fn java(&self, args: &[String], extra_env: &[(&str, &str)]) -> Raw {
        let random = self.random();
        let mut command = Command::new("java");
        command
            .arg(format!("-Duser.home={}", self.root.join("home").display()))
            .arg(format!("-Daisurada.test.now={NOW}"))
            .arg(format!("-Daisurada.test.random={random}"))
            .arg("-Daisurada.vault.autocommit.enabled=false")
            .arg(format!(
                "-Daisurada.approvals.dir={}",
                self.root.join("approvals-java").display()
            ))
            .arg(format!(
                "-Daisurada.claude.home={}",
                self.root.join("session-home").display()
            ))
            .arg(format!(
                "-Daisurada.codex.home={}",
                self.root.join("session-home").display()
            ))
            .arg(format!(
                "-Daisurada.gemini.home={}",
                self.root.join("session-home").display()
            ))
            .arg("-jar")
            .arg(&self.jar)
            .args(args)
            .arg(format!("--aisurada.vault={}", self.a.display()))
            .env("AISURADA_LOCK_DIR", self.root.join("locks"));
        for (key, value) in extra_env {
            command.env(key, value);
        }
        let output = command.output().expect("java를 못 돌렸다");
        Raw {
            code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }

    fn rust(&self, args: &[String], extra_env: &[(&str, &str)]) -> Raw {
        // SAFETY: 단일 스레드에서 호출 직전에만 바꾼다.
        unsafe {
            env::set_var("AISURADA_TEST_RANDOM", self.random());
        }
        ais_core::write::apparatus::reset_random_queue();
        let mut cli_env = Env {
            vars: BTreeMap::new(),
            home: self.root.join("home").to_string_lossy().into_owned(),
            cwd: self.root.to_string_lossy().into_owned(),
        };
        cli_env
            .vars
            .insert("AISURADA_VAULT".into(), self.b.to_string_lossy().into_owned());
        for (key, value) in extra_env {
            cli_env.vars.insert((*key).into(), (*value).into());
        }
        let outcome = ais_cli::run(args, &cli_env);
        Raw {
            code: outcome.code,
            stdout: outcome.stdout,
            stderr: outcome.stderr,
        }
    }

    fn random(&self) -> String {
        let n = self.step;
        format!("{n:04x}0001,{n:04x}0002,{n:04x}0003,{n:04x}0004")
    }

    /// 명령줄 하나를 두 사본에 돌려 견준다. `write`면 폴더 전체 바이트도 견준다. Java가 성공했으면 Rust의 응답 JSON을 돌려준다.
    fn run(&mut self, label: &str, args: &[&str], write: bool) -> Option<Value> {
        self.run_with(label, args, write, &[])
    }

    fn run_with(
        &mut self,
        label: &str,
        args: &[&str],
        write: bool,
        extra_env: &[(&str, &str)],
    ) -> Option<Value> {
        self.step += 1;
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        let java = self.java(&args, extra_env);
        let rust = self.rust(&args, extra_env);
        let (a_path, b_path) = (
            self.a.to_string_lossy().into_owned(),
            self.b.to_string_lossy().into_owned(),
        );
        // 두 사본의 폴더 경로가 글에 들어간 곳(딥링크, 오류 글)은 Java 쪽을 Rust 사본 경로로 바꿔 견준다
        let swap = |text: &str| -> String {
            text.replace(
                &ais_core::text::encode_uri_component(&a_path),
                &ais_core::text::encode_uri_component(&b_path),
            )
            .replace(&a_path, &b_path)
        };
        let java_err = swap(&java.stderr);
        let java_out = swap(&java.stdout);
        if java.code == 0 {
            self.report.succeeded += 1;
        }
        if self.verbose || env::var_os("HARNESS_TRACE").is_some() {
            eprintln!(
                "TRACE {} java={} rust={} {label}: {}",
                self.step,
                java.code,
                rust.code,
                args.join(" ")
            );
        }

        let code_ok = java.code == rust.code;
        self.record(format!("{label} 종료 코드"), code_ok, || {
            format!(
                "Java {} / Rust {}\n    Java 오류: {}\n    Rust 오류: {}",
                java.code,
                rust.code,
                clip(&java_err),
                clip(&rust.stderr)
            )
        });
        // 안쪽 실패(종료 코드 1)는 Java가 스택 추적을 붙이므로 글을 견주지 않는다
        if java.code != 1 {
            self.record(format!("{label} 오류 글"), java_err == rust.stderr, || {
                format!("Java: {}\n    Rust: {}", clip(&java_err), clip(&rust.stderr))
            });
        }
        let parsed = (
            serde_json::from_str::<Value>(&java_out),
            serde_json::from_str::<Value>(&rust.stdout),
        );
        match parsed {
            (Ok(j), Ok(r)) => {
                self.record(format!("{label} 응답"), j == r, || first_diff(&j, &r, "$"));
            }
            _ => {
                self.record(format!("{label} 출력"), java_out == rust.stdout, || {
                    format!("Java: {}\n    Rust: {}", clip(&java_out), clip(&rust.stdout))
                });
            }
        }
        if write {
            let diff = tree_diff(&self.a, &self.b);
            self.record(format!("{label} 폴더"), diff.is_empty(), || diff.clone());
        }
        serde_json::from_str(&rust.stdout).ok()
    }

    fn record(&mut self, label: String, ok: bool, detail: impl FnOnce() -> String) {
        self.report.checked += 1;
        if !ok {
            self.report.failures.push(format!("{label}\n    {}", detail()));
        }
    }

    /// Rust 사본에서 명령 하나를 읽어 JSON을 얻는다(사례를 고르고 확인 값을 얻는 데 쓴다).
    fn peek(&self, args: &[&str]) -> Value {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        let mut cli_env = Env {
            vars: BTreeMap::new(),
            home: self.root.join("home").to_string_lossy().into_owned(),
            cwd: self.root.to_string_lossy().into_owned(),
        };
        cli_env
            .vars
            .insert("AISURADA_VAULT".into(), self.b.to_string_lossy().into_owned());
        let outcome = ais_cli::run(&args, &cli_env);
        serde_json::from_str(&outcome.stdout).unwrap_or(Value::Null)
    }

    fn token(&self, id: &str) -> String {
        self.peek(&["get-doc", id])["precondition"]
            .as_str()
            .unwrap_or("")
            .to_string()
    }
}

fn clip(text: &str) -> String {
    let flat = text.replace('\n', "⏎");
    let mut out: String = flat.chars().take(200).collect();
    if flat.chars().count() > 200 {
        out.push('…');
    }
    out
}

fn first_diff(a: &Value, b: &Value, path: &str) -> String {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for key in x.keys().chain(y.keys()) {
                match (x.get(key), y.get(key)) {
                    (Some(l), Some(r)) if l != r => {
                        return first_diff(l, r, &format!("{path}.{key}"));
                    }
                    (Some(l), None) => {
                        return format!("{path}.{key}: Java에만 있다 ({})", clip(&l.to_string()));
                    }
                    (None, Some(r)) => {
                        return format!("{path}.{key}: Rust에만 있다 ({})", clip(&r.to_string()));
                    }
                    _ => {}
                }
            }
            String::new()
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return format!("{path}: 길이가 다르다 (Java {}, Rust {})", x.len(), y.len());
            }
            for (i, (l, r)) in x.iter().zip(y).enumerate() {
                if l != r {
                    return first_diff(l, r, &format!("{path}[{i}]"));
                }
            }
            String::new()
        }
        _ => format!("{path}: Java {} / Rust {}", clip(&a.to_string()), clip(&b.to_string())),
    }
}

/// 폴더 안의 파일을 (상대 경로 → 내용)으로 모은다. `.git`은 뺀다.
fn tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let rel = path.strip_prefix(root).unwrap().to_string_lossy().to_string();
            if rel == ".git" {
                continue;
            }
            let meta = fs::symlink_metadata(&path).unwrap();
            if meta.file_type().is_symlink() {
                let target = fs::read_link(&path)
                    .map(|t| t.to_string_lossy().to_string())
                    .unwrap_or_default();
                out.insert(rel, format!("-> {target}").into_bytes());
            } else if meta.is_dir() {
                out.insert(format!("{rel}/"), Vec::new());
                walk(root, &path, out);
            } else {
                out.insert(rel, fs::read(&path).unwrap_or_default());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}

fn tree_diff(a: &Path, b: &Path) -> String {
    let (left, right) = (tree(a), tree(b));
    let mut diffs = Vec::new();
    for (path, bytes) in &left {
        match right.get(path) {
            None => diffs.push(format!("Java에만 있다: {path}")),
            Some(other) if other != bytes => diffs.push(format!(
                "내용이 다르다: {path}\n      Java: {}\n      Rust: {}",
                clip(&String::from_utf8_lossy(bytes)),
                clip(&String::from_utf8_lossy(other))
            )),
            _ => {}
        }
    }
    for path in right.keys().filter(|p| !left.contains_key(*p)) {
        diffs.push(format!("Rust에만 있다: {path}"));
    }
    diffs.truncate(5);
    diffs.join("\n    ")
}

fn strings(value: &Value, key: &str) -> Vec<String> {
    value
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row[key].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------------------------------
// 사례
// ---------------------------------------------------------------------------------------------------------------------

/// 이름·도움말·오류 글처럼 vault를 읽지 않는 명령줄
fn cases_surface(h: &mut Harness) {
    // 인자 없이, 도움말, 목록은 Java ais-cli(클라이언트)만 낸다. 엔진 단발에는 같은 것이 없어 Rust 쪽 글을 따로 확인한다.
    for args in [vec![], vec!["--help"], vec!["-l"]] {
        let args: Vec<String> = args.iter().map(|a: &&str| a.to_string()).collect();
        let outcome = h.rust(&args, &[]);
        h.record(
            format!("도움말 {args:?}"),
            outcome.code == 0 && outcome.stdout.contains("사이드카 고유 명령:"),
            || format!("{outcome:?}"),
        );
    }
    let unknown = h.rust(&["nope".to_string()], &[]);
    h.record(
        "알 수 없는 이름".to_string(),
        unknown.code == 2 && unknown.stderr.starts_with("알 수 없는 이름: nope\n"),
        || format!("{unknown:?}"),
    );
}

/// 옵션·자리 인자 해석 오류
fn cases_parse_errors(h: &mut Harness) {
    for (label, args) in [
        ("모르는 옵션", vec!["list-docs", "nope=1"]),
        ("모르는 옵션 get-doc", vec!["get-doc", "x=1"]),
        ("자리 인자 초과", vec!["get-doc", "a", "b"]),
        ("자리 인자 초과 list-docs", vec!["list-docs", "a"]),
        ("double dash는 버린다", vec!["list-docs", "--json", "limit=1"]),
        ("limit 숫자 아님", vec!["list-docs", "limit=abc"]),
        ("limit 범위 밖", vec!["list-docs", "limit=99999999999"]),
        ("limit 빈 글", vec!["list-docs", "limit="]),
        ("limit 음수", vec!["list-docs", "limit=-5"]),
        ("limit 부호", vec!["list-docs", "limit=+2"]),
        ("bodies 잘못", vec!["list-docs", "bodies=maybe"]),
        ("favorite 잘못", vec!["list-docs", "favorite=maybe"]),
        ("favorite false", vec!["list-docs", "favorite=false"]),
        ("favorite 대문자", vec!["list-docs", "favorite=TRUE"]),
        ("tag 줄바꿈", vec!["list-docs", "tag=a\nb"]),
        ("날짜 잘못", vec!["list-docs", "date=2026-13-40"]),
        ("유형 잘못", vec!["list-docs", "type=zzz"]),
        ("검색어 없음", vec!["search-docs"]),
        ("검색어 공백", vec!["search-docs", "  "]),
        ("검색어 옵션", vec!["search-docs", "query=제목"]),
        ("검색 limit 0", vec!["search-docs", "e", "limit=0"]),
        ("검색 limit 201", vec!["search-docs", "e", "limit=201"]),
        ("검색 limit 숫자 아님", vec!["search-docs", "e", "limit=x"]),
        ("태그 limit 0", vec!["list-tags", "limit=0"]),
        ("태그 limit 숫자 아님", vec!["list-tags", "limit=x"]),
        ("history 번호 아님", vec!["history", "x", "abc"]),
        ("history 번호 0", vec!["history", "x", "0"]),
        ("get-doc id 없음", vec!["get-doc"]),
        ("get-doc 없는 id", vec!["get-doc", "zzzz-0000"]),
        ("get-template 없음", vec!["get-template", "zzzz"]),
        ("list-sub-notes 부모 없음", vec!["list-sub-notes"]),
        ("list-sub-notes 없는 부모", vec!["list-sub-notes", "zzzz"]),
        ("get-sub-note 없는 id", vec!["get-sub-note", "a", "b"]),
        ("list-comments 없는 id", vec!["list-comments", "zzzz"]),
        ("list-attachments 없는 id", vec!["list-attachments", "zzzz"]),
        ("get-agenda-facts 없는 id", vec!["get-agenda-facts", "zzzz"]),
        ("list-templates 갈래 잘못", vec!["list-templates", "category=zzz"]),
    ] {
        h.run(label, &args, false);
    }
}

/// 읽기 명령: 목록 변형, 노트마다 열기, 검색, 태그, 템플릿, 아젠다
fn cases_reads(h: &mut Harness) {
    for (label, args) in [
        ("list-docs 기본", vec!["list-docs"]),
        ("list-docs 전부", vec!["list-docs", "limit=0"]),
        ("list-docs 본문", vec!["list-docs", "limit=0", "bodies=true"]),
        ("list-docs 1건", vec!["list-docs", "limit=1"]),
        ("list-docs 3건", vec!["list-docs", "limit=3", "bodies=true"]),
        ("list-docs 빈 bodies", vec!["list-docs", "limit=0", "bodies="]),
        ("list-docs favorite", vec!["list-docs", "limit=0", "favorite=true"]),
        ("list-docs type=plan", vec!["list-docs", "limit=0", "type=plan"]),
        ("list-docs type=PLAN", vec!["list-docs", "limit=0", "type=PLAN"]),
        ("list-docs agenda=demo", vec!["list-docs", "limit=0", "agenda=demo"]),
        ("list-docs from", vec!["list-docs", "limit=0", "from=2026-01-01"]),
        ("list-docs to", vec!["list-docs", "limit=0", "to=2026-06-30"]),
        ("list-docs 기간", vec!["list-docs", "limit=0", "from=2026-01-01", "to=2026-12-31"]),
        ("list-agendas", vec!["list-agendas"]),
        ("list-projects", vec!["list-projects"]),
        ("list-trash", vec!["list-trash"]),
        ("list-templates", vec!["list-templates"]),
        ("list-templates 본문", vec!["list-templates", "bodies=true"]),
        ("list-templates doc", vec!["list-templates", "category=doc", "bodies=true"]),
        ("list-templates ritual", vec!["list-templates", "category=RITUAL"]),
        ("list-templates type", vec!["list-templates", "type=plan"]),
        ("list-tags", vec!["list-tags"]),
        ("list-tags 2건", vec!["list-tags", "limit=2"]),
        ("list-tags 질의", vec!["list-tags", "query=a"]),
        ("list-tags 질의 limit", vec!["list-tags", "query=a", "limit=1"]),
        ("search-docs e", vec!["search-docs", "e"]),
        ("search-docs 1건", vec!["search-docs", "e", "limit=1"]),
        ("search-docs 200건", vec!["search-docs", "e", "limit=200"]),
        ("search-docs 한글", vec!["search-docs", "데모"]),
    ] {
        h.run(label, &args, false);
    }

    // 아젠다·템플릿·노트마다 열기
    let agendas = strings(&h.peek(&["list-agendas"]), "id");
    for id in &agendas {
        h.run(&format!("get-agenda-facts {id}"), &["get-agenda-facts", id], false);
    }
    let templates = strings(&h.peek(&["list-templates"]), "id");
    for id in &templates {
        h.run(&format!("get-template {id}"), &["get-template", id], false);
    }
    let rows = h.peek(&["list-docs", "limit=0"]);
    let ids = strings(&rows, "id");
    for id in &ids {
        h.run(&format!("get-doc {id}"), &["get-doc", id], false);
        h.run(&format!("list-comments {id}"), &["list-comments", id], false);
        h.run(&format!("list-attachments {id}"), &["list-attachments", id], false);
        h.run(&format!("list-sub-notes {id}"), &["list-sub-notes", id], false);
        h.run(
            &format!("list-sub-notes bodies {id}"),
            &["list-sub-notes", id, "bodies=true"],
            false,
        );
        let subs = h.peek(&["list-sub-notes", id]);
        for sub in strings(&subs, "id") {
            h.run(&format!("get-sub-note {id} {sub}"), &["get-sub-note", id, &sub], false);
            h.run(
                &format!("list-comments {id} {sub}"),
                &["list-comments", id, &sub],
                false,
            );
            h.run(
                &format!("list-attachments {id} {sub}"),
                &["list-attachments", id, &sub],
                false,
            );
        }
    }
    // 노트의 제목·태그를 검색어로
    for row in rows.as_array().cloned().unwrap_or_default().iter().take(12) {
        if let Some(title) = row["title"].as_str()
            && !title.contains('=')
            && !title.starts_with('-')
        {
            h.run(&format!("search-docs {title}"), &["search-docs", title], false);
        }
        for tag in row["tags"].as_array().cloned().unwrap_or_default().iter().take(1) {
            if let Some(tag) = tag.as_str()
                && !tag.contains(',')
            {
                let arg = format!("tag={tag}");
                h.run(&format!("list-docs {arg}"), &["list-docs", "limit=0", &arg], false);
                h.run(&format!("search-docs {tag}"), &["search-docs", tag], false);
            }
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let verbose = args.iter().any(|a| a == "--verbose");
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    if positional.len() < 2 {
        eprintln!("사용법: cli_compat <ais-engine.jar> <합성 vault> [--verbose]");
        return ExitCode::from(2);
    }
    let mut h = Harness::new(positional[0], Path::new(positional[1]), verbose);

    cases_surface(&mut h);
    cases_parse_errors(&mut h);
    cases_reads(&mut h);

    let _ = json!(null);
    let report = &h.report;
    println!(
        "ais-cli 맞대기: 확인 {}건, 어긋남 {}건 (Java가 성공으로 답한 호출 {}건)",
        report.checked,
        report.failures.len(),
        report.succeeded
    );
    for failure in report.failures.iter().take(40) {
        println!("  어긋남: {failure}");
    }
    let failed = !report.failures.is_empty();
    let _ = fs::remove_dir_all(&h.root);
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
