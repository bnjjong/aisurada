//! Java 엔진(단발 모드)과 Rust `ais-cli`가 같은 명령줄에 같은 결과를 내는지 맞댄다.
//!
//! 사용: `cargo run -p ais-cli --example cli_compat -- <기준 ais-engine.jar> <합성 vault> [--verbose]`
//! 기준 jar는 `engine-rs/tools/compat/build-oracle.sh`가 태그 `java-engine-final`에서 지어 `engine-rs/target/oracle/`에 둔다.
//!
//! 합성 vault를 사본 둘(Java용, Rust용)로 복사해 같은 명령줄을 차례로 돌린다. 명령마다 종료 코드, 표준 오류 글, 표준 출력 JSON(키 차례는
//! 무시)을 견주고, 쓰기 명령 뒤에는 두 사본 폴더 전체의 바이트도 견준다. 엔진 호환(`ais-core`의 `compat_check`)이 엔진 값을 이미 맞댔으므로
//! 여기서는 명령줄 해석, 자리 인자 이름, 글→수 변환, 옛 응답 모양 되돌림, 오류 글과 종료 코드를 본다.
//!
//! 진짜 vault에는 주지 않는다(쓰기 명령이 있다). 합성 vault는 `engine-rs/tools/compat/make-fixture.sh`로 만든다.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::{env, fs};

use ais_cli::env::Env;
use serde_json::{Value, json};

#[allow(dead_code)]
#[path = "../../ais-core/examples/support/tape.rs"]
mod tape;

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
    /// 기준(Java) 응답을 녹화하고 재생하는 테이프(`--record`, `--replay`)
    tape: RefCell<tape::Tape>,
    root: PathBuf,
    a: PathBuf,
    b: PathBuf,
    step: u32,
    verbose: bool,
}

impl Harness {
    fn new(jar: &str, fixture: &Path, verbose: bool, mut tape: tape::Tape) -> Harness {
        let root = env::temp_dir().join(format!("ais-cli-compat-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("임시 폴더");
        tape.register_root(&root);
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
            tape: RefCell::new(tape),
            root,
            a,
            b,
            step: 0,
            verbose,
        }
    }

    /// 기준(Java) 호출. 테이프가 재생이면 엔진을 돌리지 않고 녹화한 결과를 돌려준다.
    fn java(&self, args: &[String], extra_env: &[(&str, &str)]) -> Raw {
        let key: Vec<String> = {
            let tape = self.tape.borrow();
            args.iter().map(|a| tape.norm_str(a)).collect()
        };
        if self.tape.borrow().is_replay() {
            let v = self
                .tape
                .borrow_mut()
                .next("cli", &key)
                .unwrap_or_else(|e| panic!("{e}"));
            return Raw {
                code: v["code"].as_i64().unwrap_or(-1) as i32,
                stdout: v["stdout"].as_str().unwrap_or_default().to_string(),
                stderr: v["stderr"].as_str().unwrap_or_default().to_string(),
            };
        }
        let raw = self.launch_java(args, extra_env);
        if self.tape.borrow().is_record() {
            let value = json!({"code": raw.code, "stdout": raw.stdout, "stderr": raw.stderr});
            self.tape.borrow_mut().push("cli", key, &value);
        }
        raw
    }

    /// 두 사본 폴더가 같은지 본다. 녹화는 기준 쪽(`a`) 폴더의 파일별 해시를 적고, 재생은 Rust 쪽(`b`) 폴더를 그 녹화와 견준다.
    fn folders(&self) -> String {
        if self.tape.borrow().is_replay() {
            let recorded = self
                .tape
                .borrow_mut()
                .next_tree("vault")
                .unwrap_or_else(|e| panic!("{e}"));
            let local = self.tape.borrow().digest_files(&tree(&self.b));
            return tape::digest_diff(&recorded, &local);
        }
        let diff = tree_diff(&self.a, &self.b);
        if self.tape.borrow().is_record() {
            let digests = self.tape.borrow().digest_files(&tree(&self.a));
            self.tape.borrow_mut().push_tree("vault", &digests);
        }
        diff
    }

    fn launch_java(&self, args: &[String], extra_env: &[(&str, &str)]) -> Raw {
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
            exe: String::new(),
            session: None,
        };
        cli_env.vars.insert(
            "AISURADA_VAULT".into(),
            self.b.to_string_lossy().into_owned(),
        );
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
        // Java 이후 목록 조건에 추가한 서브 아젠다 옵션은 옛 오류 설명에서 빠진다.
        let rust_err = if args.first().is_some_and(|command| command == "list-docs") {
            rust.stderr
                .replace(", subAgendaId", "")
                .replace("[subAgendaId=<분류 ID>] ", "")
        } else {
            rust.stderr.clone()
        };
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
        // 안쪽 실패(종료 코드 1)는 Java가 스택 추적을 붙이고, 휴지통 경고는 Java 로거가 시각을 붙여 찍으므로(Rust는 이 경고를 내지 않는다) 글을 견주지 않는다
        if java.code != 1 && !java_err.contains("aisurada.cmd.TrashCommands") {
            self.record(format!("{label} 오류 글"), java_err == rust_err, || {
                format!(
                    "Java: {}\n    Rust: {}",
                    clip(&java_err),
                    clip(&rust.stderr)
                )
            });
        }
        let parsed = (
            serde_json::from_str::<Value>(&java_out),
            serde_json::from_str::<Value>(&rust.stdout),
        );
        match parsed {
            (Ok(mut j), Ok(mut r)) => {
                // 걸린 시간은 돌릴 때마다 다르다
                strip_key(&mut j, "elapsedMs");
                strip_key(&mut r, "elapsedMs");
                self.record(format!("{label} 응답"), j == r, || {
                    first_diff(&j, &r, "$")
                });
            }
            _ => {
                self.record(format!("{label} 출력"), java_out == rust.stdout, || {
                    format!(
                        "Java: {}\n    Rust: {}",
                        clip(&java_out),
                        clip(&rust.stdout)
                    )
                });
            }
        }
        if write {
            let diff = self.folders();
            self.record(format!("{label} 폴더"), diff.is_empty(), || diff.clone());
        }
        serde_json::from_str(&rust.stdout).ok()
    }

    fn record(&mut self, label: String, ok: bool, detail: impl FnOnce() -> String) {
        self.report.checked += 1;
        if !ok {
            self.report
                .failures
                .push(format!("{label}\n    {}", detail()));
        }
    }

    /// Rust 사본에서 명령 하나를 읽어 JSON을 얻는다(사례를 고르고 확인 값을 얻는 데 쓴다).
    fn peek(&self, args: &[&str]) -> Value {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        let mut cli_env = Env {
            vars: BTreeMap::new(),
            home: self.root.join("home").to_string_lossy().into_owned(),
            cwd: self.root.to_string_lossy().into_owned(),
            exe: String::new(),
            session: None,
        };
        cli_env.vars.insert(
            "AISURADA_VAULT".into(),
            self.b.to_string_lossy().into_owned(),
        );
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

/// 값 안의 모든 객체에서 키 하나를 뺀다.
fn strip_key(value: &mut Value, key: &str) {
    match value {
        Value::Object(map) => {
            map.remove(key);
            map.values_mut().for_each(|v| strip_key(v, key));
        }
        Value::Array(items) => items.iter_mut().for_each(|v| strip_key(v, key)),
        _ => {}
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
        _ => format!(
            "{path}: Java {} / Rust {}",
            clip(&a.to_string()),
            clip(&b.to_string())
        ),
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
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .to_string();
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
    // 승인 대기 파일(`.aisurada/pending/*.json`)은 앱이 읽는 JSON이라 키 차례가 다른 것은 같은 것으로 본다
    // (Java는 넣은 차례, Rust는 이름 차례로 쓴다).
    let same_json = |path: &str, a: &[u8], b: &[u8]| {
        path.starts_with(".aisurada/pending/")
            && path.ends_with(".json")
            && matches!(
                (serde_json::from_slice::<Value>(a), serde_json::from_slice::<Value>(b)),
                (Ok(x), Ok(y)) if x == y
            )
    };
    for (path, bytes) in &left {
        match right.get(path) {
            None => diffs.push(format!("Java에만 있다: {path}")),
            Some(other) if other != bytes && !same_json(path, bytes, other) => diffs.push(format!(
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
        (
            "double dash는 버린다",
            vec!["list-docs", "--json", "limit=1"],
        ),
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
        (
            "list-templates 갈래 잘못",
            vec!["list-templates", "category=zzz"],
        ),
    ] {
        h.run(label, &args, false);
    }
}

/// 읽기 명령: 목록 변형, 노트마다 열기, 검색, 태그, 템플릿, 아젠다
fn cases_reads(h: &mut Harness) {
    for (label, args) in [
        ("list-docs 기본", vec!["list-docs"]),
        ("list-docs 전부", vec!["list-docs", "limit=0"]),
        (
            "list-docs 본문",
            vec!["list-docs", "limit=0", "bodies=true"],
        ),
        ("list-docs 1건", vec!["list-docs", "limit=1"]),
        ("list-docs 3건", vec!["list-docs", "limit=3", "bodies=true"]),
        (
            "list-docs 빈 bodies",
            vec!["list-docs", "limit=0", "bodies="],
        ),
        (
            "list-docs favorite",
            vec!["list-docs", "limit=0", "favorite=true"],
        ),
        (
            "list-docs type=plan",
            vec!["list-docs", "limit=0", "type=plan"],
        ),
        (
            "list-docs type=PLAN",
            vec!["list-docs", "limit=0", "type=PLAN"],
        ),
        (
            "list-docs agenda=demo",
            vec!["list-docs", "limit=0", "agenda=demo"],
        ),
        (
            "list-docs from",
            vec!["list-docs", "limit=0", "from=2026-01-01"],
        ),
        (
            "list-docs to",
            vec!["list-docs", "limit=0", "to=2026-06-30"],
        ),
        (
            "list-docs 기간",
            vec!["list-docs", "limit=0", "from=2026-01-01", "to=2026-12-31"],
        ),
        ("list-agendas", vec!["list-agendas"]),
        ("list-projects", vec!["list-projects"]),
        ("list-trash", vec!["list-trash"]),
        ("list-templates", vec!["list-templates"]),
        ("list-templates 본문", vec!["list-templates", "bodies=true"]),
        (
            "list-templates doc",
            vec!["list-templates", "category=doc", "bodies=true"],
        ),
        (
            "list-templates ritual",
            vec!["list-templates", "category=RITUAL"],
        ),
        ("list-templates type", vec!["list-templates", "type=plan"]),
        ("list-tags", vec!["list-tags"]),
        ("list-tags 2건", vec!["list-tags", "limit=2"]),
        ("list-tags 질의", vec!["list-tags", "query=a"]),
        (
            "list-tags 질의 limit",
            vec!["list-tags", "query=a", "limit=1"],
        ),
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
        h.run(
            &format!("get-agenda-facts {id}"),
            &["get-agenda-facts", id],
            false,
        );
    }
    let templates = strings(&h.peek(&["list-templates"]), "id");
    for id in &templates {
        h.run(&format!("get-template {id}"), &["get-template", id], false);
    }
    let rows = h.peek(&["list-docs", "limit=0"]);
    let ids = strings(&rows, "id");
    for id in &ids {
        h.run(&format!("get-doc {id}"), &["get-doc", id], false);
        h.run(
            &format!("list-comments {id}"),
            &["list-comments", id],
            false,
        );
        h.run(
            &format!("list-attachments {id}"),
            &["list-attachments", id],
            false,
        );
        h.run(
            &format!("list-sub-notes {id}"),
            &["list-sub-notes", id],
            false,
        );
        h.run(
            &format!("list-sub-notes bodies {id}"),
            &["list-sub-notes", id, "bodies=true"],
            false,
        );
        let subs = h.peek(&["list-sub-notes", id]);
        for sub in strings(&subs, "id") {
            h.run(
                &format!("get-sub-note {id} {sub}"),
                &["get-sub-note", id, &sub],
                false,
            );
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
            h.run(
                &format!("search-docs {title}"),
                &["search-docs", title],
                false,
            );
        }
        for tag in row["tags"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .take(1)
        {
            if let Some(tag) = tag.as_str()
                && !tag.contains(',')
            {
                let arg = format!("tag={tag}");
                h.run(
                    &format!("list-docs {arg}"),
                    &["list-docs", "limit=0", &arg],
                    false,
                );
                h.run(&format!("search-docs {tag}"), &["search-docs", tag], false);
            }
        }
    }
}

/// 쓰기 명령: 아젠다·프로젝트·템플릿·노트·후속 노트·코멘트·첨부·휴지통과 세션 표식. 명령마다 폴더 전체 바이트를 견준다.
fn cases_writes(h: &mut Harness) {
    // 아젠다와 프로젝트
    let created = h.run(
        "create-agenda",
        &[
            "create-agenda",
            "title=신규 아젠다",
            "hue=teal",
            "repo=example/new",
            "summary=설명 한 줄",
        ],
        true,
    );
    let agenda_id = created
        .as_ref()
        .and_then(|v| v["id"].as_str())
        .unwrap_or("agfx0001-0000000a")
        .to_string();
    h.run(
        "create-agenda 제목 없음",
        &["create-agenda", "hue=teal"],
        true,
    );
    h.run(
        "update-agenda",
        &["update-agenda", &agenda_id, "summary=바뀐 설명", "hue=blue"],
        true,
    );
    h.run(
        "update-agenda 빈 repo",
        &["update-agenda", &agenda_id, "repo="],
        true,
    );
    h.run(
        "update-agenda 없는 id",
        &["update-agenda", "zzzz", "title=x"],
        true,
    );
    let project = h.run(
        "create-project",
        &[
            "create-project",
            "title=새 프로젝트",
            "hue=orange",
            "summary=요약",
        ],
        true,
    );
    let project_id = project
        .as_ref()
        .and_then(|v| v["id"].as_str())
        .unwrap_or("pjfx0001-000000a1")
        .to_string();
    h.run(
        "update-project",
        &["update-project", &project_id, "title=고친 프로젝트"],
        true,
    );
    h.run(
        "update-project repos",
        &[
            "update-project",
            &project_id,
            "repos=[{\"name\":\"web\",\"github\":\"example/web\"}]",
        ],
        true,
    );
    h.run("delete-project", &["delete-project", &project_id], true);
    h.run(
        "update-agenda-facts",
        &["update-agenda-facts", &agenda_id, "facts=지금도 참인 사실"],
        true,
    );
    h.run(
        "get-agenda-facts 새 아젠다",
        &["get-agenda-facts", &agenda_id],
        false,
    );
    h.run(
        "update-agenda-facts 지움",
        &["update-agenda-facts", &agenda_id, "facts="],
        true,
    );
    h.run("delete-agenda", &["delete-agenda", &agenda_id], true);

    // 템플릿
    let template = h.run(
        "save-template",
        &[
            "save-template",
            "name=명령줄 템플릿",
            "type=plan",
            "desc=설명",
            "body=# 제목\n\n본문",
        ],
        true,
    );
    let template_id = template
        .as_ref()
        .and_then(|v| v["id"].as_str())
        .unwrap_or("cli-t")
        .to_string();
    h.run(
        "save-template 필수 없음",
        &["save-template", "desc=이름이 없다"],
        true,
    );
    h.run(
        "save-template 고치기",
        &[
            "save-template",
            &format!("id={template_id}"),
            "desc=바뀐 설명",
        ],
        true,
    );
    h.run("get-template", &["get-template", &template_id], false);
    h.run(
        "save-ritual-template",
        &[
            "save-ritual-template",
            "cli-r",
            "name=리추얼",
            "body=# 아침",
        ],
        true,
    );
    h.run(
        "delete-doc-template",
        &["delete-doc-template", &template_id],
        true,
    );

    // 노트
    let doc = h.run(
        "create-doc",
        &[
            "create-doc",
            "agenda=demo",
            "type=plan",
            "title=명령줄 노트",
            "body=# 본문\n\n내용",
            "date=2026-11-15",
            "time=10:00",
            "format=markdown",
            "tags=[\"a\",\"b\"]",
        ],
        true,
    );
    let id = doc
        .as_ref()
        .and_then(|v| v["id"].as_str())
        .unwrap_or("")
        .to_string();
    h.run("create-doc 필수 없음", &["create-doc", "agenda=demo"], true);
    h.run(
        "create-doc 없는 아젠다",
        &["create-doc", "agenda=zzz", "type=plan", "title=x", "body=y"],
        true,
    );
    if !id.is_empty() {
        let t = format!("f1={}", h.token(&id));
        h.run(
            "update-doc",
            &[
                "update-doc",
                &id,
                "title=고친 제목",
                "body=# 고침\n\n새 내용",
                &t,
            ],
            true,
        );
        let t = format!("f1={}", h.token(&id));
        h.run(
            "update-doc 태그",
            &["update-doc", &id, "tags=[\"x\"]", &t],
            true,
        );
        h.run(
            "update-doc f1 없음",
            &["update-doc", &id, "title=확인값 없음"],
            true,
        );
        h.run(
            "update-doc f1 틀림",
            &["update-doc", &id, "title=틀림", "f1=f1:zzz"],
            true,
        );
        let t = format!("f1={}", h.token(&id));
        h.run(
            "set-favorite",
            &["set-favorite", &id, "favorite=true", "category=중요", &t],
            true,
        );
        let t = format!("f1={}", h.token(&id));
        h.run(
            "evaluate-doc",
            &["evaluate-doc", &id, "score=4", "note=좋음", &t],
            true,
        );
        let t = format!("f1={}", h.token(&id));
        h.run(
            "evaluate-doc 점수 잘못",
            &["evaluate-doc", &id, "score=abc", &t],
            true,
        );
        let other = h.peek(&["list-docs", "limit=0"]);
        let other = strings(&other, "id")
            .into_iter()
            .find(|candidate| *candidate != id)
            .unwrap_or_default();
        let t = format!("f1={}", h.token(&id));
        h.run(
            "link-docs",
            &["link-docs", &id, &other, "kind=indirect", &t],
            true,
        );
        let t = format!("f1={}", h.token(&id));
        h.run(
            "link-docs 해제",
            &["link-docs", &id, &other, "remove=true", &t],
            true,
        );

        // 후속 노트와 코멘트, 첨부
        let sub = h.run(
            "create-sub-note",
            &[
                "create-sub-note",
                &id,
                "title=후속",
                "body=# 후속\n\n내용",
                "type=free",
            ],
            true,
        );
        let sub_id = sub
            .as_ref()
            .and_then(|v| v["id"].as_str())
            .unwrap_or("")
            .to_string();
        if !sub_id.is_empty() {
            let sub_token = h.peek(&["get-sub-note", &id, &sub_id])["precondition"]
                .as_str()
                .unwrap_or("")
                .to_string();
            let t = format!("f1={sub_token}");
            h.run(
                "update-sub-note",
                &["update-sub-note", &id, &sub_id, "title=고친 후속", &t],
                true,
            );
            h.run(
                "get-sub-note 새 후속",
                &["get-sub-note", &id, &sub_id],
                false,
            );
            h.run("list-sub-notes 새 후속", &["list-sub-notes", &id], false);
            h.run(
                "create-comment 후속",
                &[
                    "create-comment",
                    &id,
                    &sub_id,
                    "comment_id=cmcli-0000000a",
                    "body=후속 코멘트",
                ],
                true,
            );
            h.run("delete-sub-note", &["delete-sub-note", &id, &sub_id], true);
        }
        h.run(
            "create-comment",
            &[
                "create-comment",
                &id,
                "comment_id=cmcli-00000001",
                "body=첫 코멘트",
                "anchor_quote=본문",
            ],
            true,
        );
        h.run(
            "update-comment",
            &[
                "update-comment",
                &id,
                "comment_id=cmcli-00000001",
                "body=고친 코멘트",
            ],
            true,
        );
        h.run("list-comments 새 코멘트", &["list-comments", &id], false);
        h.run(
            "delete-comment",
            &["delete-comment", &id, "comment_id=cmcli-00000001"],
            true,
        );
        h.run(
            "write-attachment",
            &["write-attachment", &id, "name=memo.md", "content=첨부 내용"],
            true,
        );
        h.run(
            "list-attachments 새 첨부",
            &["list-attachments", &id],
            false,
        );
        h.run("get-doc 새 노트", &["get-doc", &id], false);

        // 휴지통
        h.run("delete-doc", &["delete-doc", &id], true);
        let trash = h.peek(&["list-trash"]);
        let trash_ids = strings(&trash, "trashId");
        if let Some(first) = trash_ids.first() {
            h.run("restore-doc", &["restore-doc", first], true);
        }
        h.run("list-trash 뒤", &["list-trash"], false);
        h.run("purge-doc 없는 항목", &["purge-doc", "trzzzz-0000"], true);
    }
    h.run("empty-trash", &["empty-trash"], true);

    // 날짜별 도구와 색인
    h.run(
        "plan-day dryRun",
        &["plan-day", "date=2026-11-15", "dryRun=true"],
        true,
    );
    h.run(
        "plan-day 저장",
        &["plan-day", "date=2026-11-15", "agenda=demo"],
        true,
    );
    h.run("check-notes", &["check-notes", "date=2026-11-15"], true);
    h.run(
        "facts-brief",
        &[
            "facts-brief",
            "from=2026-01-01",
            "to=2026-12-31",
            "min-new-file=1",
        ],
        true,
    );
    h.run("export-markdown", &["export-markdown"], true);
    h.run("verify-shard", &["verify-shard"], false);
    h.run("shard-notes 미리보기", &["shard-notes"], true);
    h.run("reindex", &["reindex"], true);
}

/// 도구 표면(JSON 인자) 읽기: 필터·검색·노트 열기·후속 노트·템플릿·자원·태스크와 인자 검증 오류
fn cases_tools_read(h: &mut Harness) {
    let rows = h.peek(&["list-docs", "limit=0"]);
    let ids = strings(&rows, "id");
    let first = ids.first().cloned().unwrap_or_default();
    let parent_note = "ndfx0002-00000002";
    for (label, tool, json) in [
        ("list_docs", "list_docs", "{}"),
        ("list_docs 빈 글", "list_docs", ""),
        ("list_docs agenda", "list_docs", r#"{"agenda":"demo"}"#),
        ("list_docs type", "list_docs", r#"{"type":"plan"}"#),
        ("list_docs tag", "list_docs", r#"{"tag":"공통"}"#),
        ("list_docs favorite", "list_docs", r#"{"favorite":true}"#),
        (
            "list_docs favorite false",
            "list_docs",
            r#"{"favorite":false}"#,
        ),
        (
            "list_docs 기간",
            "list_docs",
            r#"{"dateFrom":"2026-01-01","dateTo":"2026-12-31"}"#,
        ),
        ("list_docs date", "list_docs", r#"{"date":"2026-11-05"}"#),
        ("list_docs 모르는 키", "list_docs", r#"{"zzz":1,"aaa":2}"#),
        (
            "list_docs favorite 문자열",
            "list_docs",
            r#"{"favorite":"yes"}"#,
        ),
        ("list_docs agenda 숫자", "list_docs", r#"{"agenda":5}"#),
        (
            "list_docs 날짜 잘못",
            "list_docs",
            r#"{"date":"2026-13-40"}"#,
        ),
        ("list_docs 배열", "list_docs", "[1]"),
        ("list_docs 깨진 JSON", "list_docs", "{"),
        ("search_docs", "search_docs", r#"{"query":"e"}"#),
        ("search_docs 한글", "search_docs", r#"{"query":"데모"}"#),
        (
            "search_docs limit",
            "search_docs",
            r#"{"query":"e","limit":1}"#,
        ),
        (
            "search_docs 필터",
            "search_docs",
            r#"{"query":"e","agenda":"demo","type":"plan"}"#,
        ),
        ("search_docs 질의 없음", "search_docs", "{}"),
        ("search_docs 질의 공백", "search_docs", r#"{"query":"  "}"#),
        (
            "search_docs limit 0",
            "search_docs",
            r#"{"query":"e","limit":0}"#,
        ),
        (
            "search_docs limit 201",
            "search_docs",
            r#"{"query":"e","limit":201}"#,
        ),
        (
            "search_docs limit 소수",
            "search_docs",
            r#"{"query":"e","limit":1.5}"#,
        ),
        (
            "search_docs limit 문자열",
            "search_docs",
            r#"{"query":"e","limit":"5"}"#,
        ),
        ("list_tags", "list_tags", "{}"),
        ("list_tags 질의", "list_tags", r#"{"query":"a","limit":2}"#),
        ("list_tags limit 0", "list_tags", r#"{"limit":0}"#),
        ("list_templates", "list_templates", "{}"),
        (
            "list_templates 갈래",
            "list_templates",
            r#"{"category":"doc","type":"plan"}"#,
        ),
        (
            "list_templates 갈래 잘못",
            "list_templates",
            r#"{"category":"x"}"#,
        ),
        ("list_trash", "list_trash", "{}"),
        ("list_trash 모르는 키", "list_trash", r#"{"a":1}"#),
        ("get_template 없음", "get_template", r#"{"id":"zzzz"}"#),
        ("get_doc 없음", "get_doc", r#"{"id":"zzzz-0000"}"#),
        ("get_doc id 없음", "get_doc", "{}"),
        ("get_doc id 숫자", "get_doc", r#"{"id":5}"#),
        (
            "list_sub_notes 없음",
            "list_sub_notes",
            r#"{"parent":"zzzz"}"#,
        ),
        (
            "list_comments parent 셋",
            "list_comments",
            r#"{"parent":"a/b/c"}"#,
        ),
        (
            "list_comments parent 빈",
            "list_comments",
            r#"{"parent":" / "}"#,
        ),
        (
            "list_attachments parent 빈",
            "list_attachments",
            r#"{"parent":""}"#,
        ),
        ("list_resources", "list_resources", "{}"),
        (
            "list_resources 조건",
            "list_resources",
            r#"{"kind":"skill","scope":"global","bodies":true}"#,
        ),
        (
            "list_resources bodies 문자열",
            "list_resources",
            r#"{"bodies":"true"}"#,
        ),
        ("get_resource 없음", "get_resource", r#"{"id":"zzzz"}"#),
        ("list_resource_changes", "list_resource_changes", "{}"),
        ("list_tasks", "list_tasks", "{}"),
        (
            "list_tasks 조건",
            "list_tasks",
            r#"{"status":"draft","bodies":true,"blocked":false}"#,
        ),
        ("get_task 없음", "get_task", r#"{"id":"TSK-999"}"#),
        (
            "get_task activity",
            "get_task",
            r#"{"id":"TSK-999","activity":3}"#,
        ),
        (
            "get_task activity 잘못",
            "get_task",
            r#"{"id":"TSK-1","activity":-1}"#,
        ),
        ("미지원 도구", "export_markdown", r#"{"x":1}"#),
    ] {
        let mut args = vec![tool];
        if !json.is_empty() {
            args.push(json);
        }
        h.run(&format!("도구 {label}"), &args, false);
    }
    h.run("도구 인자 둘", &["get_doc", "{}", "{}"], false);
    h.run(
        "도구 -- 칸은 버린다",
        &["get_doc", "--x", r#"{"id":"zzzz-0000"}"#],
        false,
    );

    // 노트마다 열기: 기록 층 노트는 도구 표면에서 막혀야 한다
    for id in &ids {
        h.run(
            &format!("도구 get_doc {id}"),
            &["get_doc", &format!(r#"{{"id":"{id}"}}"#)],
            false,
        );
        h.run(
            &format!("도구 list_comments {id}"),
            &["list_comments", &format!(r#"{{"parent":"{id}"}}"#)],
            false,
        );
        h.run(
            &format!("도구 list_attachments {id}"),
            &["list_attachments", &format!(r#"{{"parent":"{id}"}}"#)],
            false,
        );
        h.run(
            &format!("도구 list_sub_notes {id}"),
            &["list_sub_notes", &format!(r#"{{"parent":"{id}"}}"#)],
            false,
        );
    }
    for sub in strings(&h.peek(&["list-sub-notes", parent_note]), "id") {
        h.run(
            &format!("도구 get_sub_note {sub}"),
            &[
                "get_sub_note",
                &format!(r#"{{"parent":"{parent_note}","id":"{sub}"}}"#),
            ],
            false,
        );
        h.run(
            &format!("도구 list_comments 후속 {sub}"),
            &[
                "list_comments",
                &format!(r#"{{"parent":"{parent_note}/{sub}"}}"#),
            ],
            false,
        );
        h.run(
            &format!("도구 list_attachments 후속 {sub}"),
            &[
                "list_attachments",
                &format!(r#"{{"parent":"{parent_note}/{sub}"}}"#),
            ],
            false,
        );
    }
    let _ = first;
    for tpl in strings(&h.peek(&["list-templates"]), "id") {
        h.run(
            &format!("도구 get_template {tpl}"),
            &["get_template", &format!(r#"{{"id":"{tpl}"}}"#)],
            false,
        );
    }
}

/// 도구 표면 쓰기: 노트 머리·후속 노트·첨부·템플릿·휴지통과 자원 변경(승인 대기). AI 호출이라 기록 층과 확인 값 규칙이 다르다.
fn cases_tools_write(h: &mut Harness) {
    let created = h.run(
        "도구 create_doc",
        &[
            "create_doc",
            r##"{"agenda":"demo","type":"plan","title":"도구 노트","body":"# 도구\n\n본문","date":"2026-11-15","time":"11:00","tags":["a","b"],"links":[]}"##,
        ],
        true,
    );
    let id = created
        .as_ref()
        .and_then(|v| v["id"].as_str())
        .unwrap_or("")
        .to_string();
    h.run(
        "도구 create_doc 필수 없음",
        &["create_doc", r#"{"agenda":"demo"}"#],
        true,
    );
    h.run(
        "도구 create_doc tags 잘못",
        &[
            "create_doc",
            r#"{"agenda":"demo","type":"plan","title":"x","body":"y","tags":[1]}"#,
        ],
        true,
    );
    h.run(
        "도구 create_doc links 잘못",
        &[
            "create_doc",
            r#"{"agenda":"demo","type":"plan","title":"x","body":"y","links":["a"]}"#,
        ],
        true,
    );
    h.run(
        "도구 create_doc template 길이",
        &[
            "create_doc",
            &format!(
                r#"{{"agenda":"demo","type":"plan","title":"x","body":"y","template":"{}"}}"#,
                "t".repeat(201)
            ),
        ],
        true,
    );
    if id.is_empty() {
        return;
    }
    let token = h.token(&id);
    h.run(
        "도구 update_doc",
        &[
            "update_doc",
            &format!(
                r##"{{"id":"{id}","title":"고친 도구 노트","body":"# 고침\n","f1":"{token}"}}"##
            ),
        ],
        true,
    );
    h.run(
        "도구 update_doc f1 없음",
        &["update_doc", &format!(r#"{{"id":"{id}","title":"x"}}"#)],
        true,
    );
    let token = h.token(&id);
    h.run(
        "도구 set_favorite",
        &[
            "set_favorite",
            &format!(r#"{{"id":"{id}","favorite":true,"category":"중요","f1":"{token}"}}"#),
        ],
        true,
    );
    let token = h.token(&id);
    h.run(
        "도구 evaluate_doc",
        &[
            "evaluate_doc",
            &format!(r#"{{"id":"{id}","score":4,"note":"좋음","f1":"{token}"}}"#),
        ],
        true,
    );
    let token = h.token(&id);
    h.run(
        "도구 evaluate_doc 점수 잘못",
        &[
            "evaluate_doc",
            &format!(r#"{{"id":"{id}","score":"x","f1":"{token}"}}"#),
        ],
        true,
    );
    let other = strings(&h.peek(&["list-docs", "limit=0"]), "id")
        .into_iter()
        .find(|candidate| *candidate != id)
        .unwrap_or_default();
    let token = h.token(&id);
    h.run(
        "도구 link_docs",
        &[
            "link_docs",
            &format!(r#"{{"from":"{id}","to":"{other}","kind":"indirect","f1":"{token}"}}"#),
        ],
        true,
    );
    let sub = h.run(
        "도구 create_sub_note",
        &[
            "create_sub_note",
            &format!(
                r##"{{"parent":"{id}","title":"도구 후속","body":"# 후속\n","type":"free"}}"##
            ),
        ],
        true,
    );
    let sub_id = sub
        .as_ref()
        .and_then(|v| v["id"].as_str())
        .unwrap_or("")
        .to_string();
    if !sub_id.is_empty() {
        let token = h.peek(&["get-sub-note", &id, &sub_id])["precondition"]
            .as_str()
            .unwrap_or("")
            .to_string();
        h.run(
            "도구 update_sub_note",
            &[
                "update_sub_note",
                &format!(
                    r#"{{"parent":"{id}","id":"{sub_id}","title":"고친 후속","f1":"{token}"}}"#
                ),
            ],
            true,
        );
        h.run(
            "도구 write_attachment 후속",
            &[
                "write_attachment",
                &format!(r#"{{"parent":"{id}/{sub_id}","name":"n.md","content":"후속 첨부"}}"#),
            ],
            true,
        );
        h.run(
            "도구 delete_sub_note",
            &[
                "delete_sub_note",
                &format!(r#"{{"parent":"{id}","id":"{sub_id}"}}"#),
            ],
            true,
        );
    }
    h.run(
        "도구 write_attachment",
        &[
            "write_attachment",
            &format!(r#"{{"parent":"{id}","name":"memo.md","content":"첨부 내용"}}"#),
        ],
        true,
    );
    h.run(
        "도구 write_attachment content 없음",
        &[
            "write_attachment",
            &format!(r#"{{"parent":"{id}","name":"memo.md"}}"#),
        ],
        true,
    );
    h.run(
        "도구 get_doc 새 노트",
        &["get_doc", &format!(r#"{{"id":"{id}"}}"#)],
        false,
    );
    h.run(
        "도구 save_template",
        &[
            "save_template",
            r##"{"name":"도구 템플릿","type":"plan","desc":"설명","body":"# 본문"}"##,
        ],
        true,
    );
    h.run(
        "도구 plan_day",
        &["plan_day", r#"{"date":"2026-11-15","dryRun":true}"#],
        true,
    );
    h.run(
        "도구 plan_day dryRun 문자열",
        &["plan_day", r#"{"dryRun":"true"}"#],
        true,
    );
    h.run("도구 export_markdown", &["export_markdown", "{}"], true);
    h.run(
        "도구 delete_doc",
        &["delete_doc", &format!(r#"{{"id":"{id}"}}"#)],
        true,
    );
    let trash = strings(&h.peek(&["list-trash"]), "trashId");
    if let Some(entry) = trash.first() {
        h.run(
            "도구 restore_doc",
            &["restore_doc", &format!(r#"{{"trashId":"{entry}"}}"#)],
            true,
        );
    }
    h.run(
        "도구 purge_doc 없음",
        &["purge_doc", r#"{"trashId":"trzzzz-0000"}"#],
        true,
    );

    // 자원: AI의 변경은 승인 대기에 오를 수 있다
    h.run(
        "도구 save_resource",
        &["save_resource", r##"{"kind":"skill","scope":"global","name":"도구 스킬","description":"설명","body":"# 지침"}"##],
        true,
    );
    h.run("도구 list_resources 뒤", &["list_resources", "{}"], false);
    h.run(
        "도구 list_resource_changes 뒤",
        &["list_resource_changes", "{}"],
        false,
    );
}

/// `reindex-paths`: 지정 경로만 훑기. 폴더, 노트 하나, 후속 노트 폴더, 없는 자리, 바깥으로 나가는 경로, 200칸 초과
fn cases_reindex_paths(h: &mut Harness) {
    let many: Vec<String> = (0..201).map(|i| format!("notes/demo/n{i}")).collect();
    let mut many_args: Vec<&str> = vec!["reindex-paths"];
    many_args.extend(many.iter().map(String::as_str));
    for (label, args) in [
        ("경로 없음", vec!["reindex-paths"]),
        ("아젠다 폴더", vec!["reindex-paths", "notes/demo"]),
        (
            "여러 곳",
            vec![
                "reindex-paths",
                "notes/demo",
                "notes/work",
                "/notes/demo/",
                " notes/demo ",
            ],
        ),
        ("노트 파일", vec!["reindex-paths", "notes/work/ext.md"]),
        ("월별 폴더", vec!["reindex-paths", "notes/demo/2026-11"]),
        (
            "폴더 노트",
            vec!["reindex-paths", "notes/demo/2026-11/parent"],
        ),
        (
            "후속 노트 폴더",
            vec!["reindex-paths", "notes/demo/2026-11/parent/sub"],
        ),
        (
            "없는 자리",
            vec!["reindex-paths", "notes/demo/nothing-here"],
        ),
        ("없는 아젠다", vec!["reindex-paths", "notes/zzz"]),
        (
            "notes 밖",
            vec!["reindex-paths", "other/place", ".aisurada/agendas"],
        ),
        ("notes만", vec!["reindex-paths", "notes"]),
        ("바깥으로", vec!["reindex-paths", "notes/demo/../../.."]),
        ("조각이 빈", vec!["reindex-paths", "notes/ /x"]),
        (
            "충돌 사본",
            vec!["reindex-paths", "notes/demo/a (conflicted copy)"],
        ),
        ("옵션 모양", vec!["reindex-paths", "--x", "notes/work"]),
    ] {
        h.run(&format!("reindex-paths {label}"), &args, false);
    }
    // 칸이 64개를 넘는 요청은 Java ais-cli(클라이언트)가 보내기 전에 거절한다. 그래서 200칸 초과의 전체 훑기 물러서기는 엔진끼리만 견준다.
    let cli = h.rust(
        &many_args.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
        &[],
    );
    h.record(
        "reindex-paths 201칸은 칸 수 한도로 거절".to_string(),
        cli.code == 1,
        || format!("{cli:?}"),
    );
    h.step += 1;
    let java = h.java(
        &many_args.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
        &[],
    );
    let rust = ais_core::engine::Engine::new(&h.b).call("reindex_paths", json!({"paths": many}));
    let mut java_value: Value = serde_json::from_str(&java.stdout).unwrap_or(Value::Null);
    let mut rust_value = rust.ok().unwrap_or(Value::Null);
    strip_key(&mut java_value, "elapsedMs");
    strip_key(&mut rust_value, "elapsedMs");
    h.record(
        "reindex-paths 201칸 엔진".to_string(),
        java.code == 0 && java_value == rust_value && java_value["mode"] == "full",
        || first_diff(&java_value, &rust_value, "$"),
    );
}

/// `history`: git 이력이 있는 합성 vault에서 판 목록, 번호·커밋·검색어로 고르기, 오류
fn cases_history(h: &mut Harness) {
    let ids = [
        "ndfx0002-00000002",
        "ndfx0001-00000001",
        "ndfx0003-00000003",
        "ndfx0040-00000040",
        "ndfx0041-00000041",
        "ndfx0099-00000099",
    ];
    for id in ids {
        h.run(&format!("history {id}"), &["history", id], false);
        h.run(&format!("history {id} 1"), &["history", id, "1"], false);
        h.run(&format!("history {id} 2"), &["history", id, "2"], false);
        h.run(&format!("history {id} 9"), &["history", id, "9"], false);
        h.run(
            &format!("history {id} 검색"),
            &["history", id, "query=내용"],
            false,
        );
        // 첫 판의 커밋으로 고르기
        let first = h.peek(&["history", id]);
        if let Some(commit) = first["versions"]
            .as_array()
            .and_then(|v| v.first())
            .and_then(|v| v["commit"].as_str())
        {
            h.run(
                &format!("history {id} 커밋"),
                &["history", id, &format!("commit={commit}")],
                false,
            );
            h.run(
                &format!("history {id} 번호와 커밋"),
                &["history", id, "1", &format!("commit={commit}")],
                false,
            );
        }
    }
    h.run(
        "history 커밋 모양",
        &["history", "ndfx0002-00000002", "commit=zz"],
        false,
    );
    h.run(
        "history 없는 커밋",
        &[
            "history",
            "ndfx0002-00000002",
            &format!("commit={}", "a".repeat(40)),
        ],
        false,
    );
    h.run("commit-vault", &["commit-vault"], true);
}

/// 태스크·자원·승인·문답·대화·세션: 명령줄 인자(자리 인자 이름, 글 그대로 넘기는 옵션, JSON 글)가 엔진까지 제대로 닿는지
fn cases_families(h: &mut Harness) {
    // 태스크
    let task = h.run(
        "create-task",
        &[
            "create-task",
            "title=명령줄 태스크",
            "repo=example/demo",
            "criteria=[\"첫 기준\",{\"text\":\"둘째 기준\"}]",
            "type=feature",
            "priority=high",
            "base=main",
            "body=설명",
            "labels=[\"cli\"]",
        ],
        true,
    );
    let key = task
        .as_ref()
        .and_then(|v| v["key"].as_str().or(v["id"].as_str()))
        .unwrap_or("TSK-1")
        .to_string();
    h.run(
        "create-task 기준 없음",
        &["create-task", "title=x", "repo=example/demo"],
        true,
    );
    let second = h.run(
        "create-task 둘째",
        &[
            "create-task",
            "title=둘째 태스크",
            "repo=example/demo",
            "criteria=[\"기준\"]",
        ],
        true,
    );
    let second_key = second
        .as_ref()
        .and_then(|v| v["key"].as_str().or(v["id"].as_str()))
        .unwrap_or("TSK-2")
        .to_string();
    h.run("get-task", &["get-task", &key], false);
    h.run(
        "get-task activity",
        &["get-task", &key, "activity=1"],
        false,
    );
    h.run("list-tasks", &["list-tasks"], false);
    h.run(
        "list-tasks 조건",
        &[
            "list-tasks",
            "status=draft",
            "type=feature",
            "repo=example/demo",
            "bodies=true",
        ],
        false,
    );
    h.run("list-tasks 휴지통", &["list-tasks", "trash=true"], false);
    let token = h.peek(&["get-task", &key])["precondition"]
        .as_str()
        .unwrap_or("")
        .to_string();
    h.run(
        "update-task",
        &[
            "update-task",
            &key,
            "title=고친 태스크",
            &format!("f1={token}"),
        ],
        true,
    );
    h.run(
        "transition-task",
        &[
            "transition-task",
            &key,
            "to=ready-for-develop",
            "actor=human",
        ],
        true,
    );
    h.run(
        "transition-task 어긋남",
        &["transition-task", &key, "to=done", "from=draft"],
        true,
    );
    h.run(
        "link-tasks",
        &["link-tasks", &key, &second_key, "type=blocks"],
        true,
    );
    h.run(
        "add-task-comment",
        &["add-task-comment", &key, "body=댓글", "actor=human"],
        true,
    );
    h.run("delete-task", &["delete-task", &second_key], true);
    h.run("restore-task", &["restore-task", &second_key], true);

    // 자원과 승인
    let skill = h.run(
        "save-resource",
        &[
            "save-resource",
            "kind=skill",
            "scope=global",
            "name=명령줄 스킬",
            "description=설명",
            "body=# 지침",
        ],
        true,
    );
    let rid = skill
        .as_ref()
        .and_then(|v| v["id"].as_str())
        .unwrap_or("")
        .to_string();
    h.run("list-resources", &["list-resources"], false);
    h.run(
        "list-resources 본문",
        &[
            "list-resources",
            "bodies=true",
            "kind=skill",
            "scope=global",
        ],
        false,
    );
    if !rid.is_empty() {
        h.run("get-resource", &["get-resource", &rid], false);
        let token = h.peek(&["get-resource", &rid])["precondition"]
            .as_str()
            .unwrap_or("")
            .to_string();
        h.run(
            "save-resource 고치기",
            &[
                "save-resource",
                &format!("id={rid}"),
                "description=바뀐 설명",
                &format!("f1={token}"),
            ],
            true,
        );
        h.run(
            "list-resource-versions",
            &["list-resource-versions", &rid],
            false,
        );
        let project = strings(&h.peek(&["list-projects"]), "slug")
            .into_iter()
            .next()
            .unwrap_or_else(|| "alpha".to_string());
        h.run(
            "set-resource-ref",
            &["set-resource-ref", &project, &rid, "with-skills=true"],
            true,
        );
        h.run(
            "usable-resources",
            &["usable-resources", &project, "bodies=true"],
            false,
        );
        h.run(
            "set-resource-ref 해제",
            &["set-resource-ref", &project, &rid, "remove=true"],
            true,
        );
        h.run("delete-resource", &["delete-resource", &rid], true);
        h.run("restore-resource", &["restore-resource", &rid], true);
    }
    h.run("list-resource-changes", &["list-resource-changes"], false);
    h.run(
        "list-resource-changes 결정",
        &["list-resource-changes", "decided=true"],
        false,
    );
    h.run(
        "get-resource-change 없음",
        &["get-resource-change", "rczzzz"],
        false,
    );
    h.run(
        "approve-resource-change 없음",
        &["approve-resource-change", "rczzzz"],
        true,
    );
    h.run(
        "reject-resource-change 없음",
        &["reject-resource-change", "rczzzz", "reason=이유"],
        true,
    );
    h.run(
        "resolve-resource-edit 없음",
        &["resolve-resource-edit", "rczzzz", "action=accept"],
        true,
    );
    h.run("get-resource-policy", &["get-resource-policy"], false);
    h.run(
        "set-resource-policy",
        &["set-resource-policy", "global=approve", "project=auto"],
        true,
    );
    h.run(
        "set-resource-policy 값 잘못",
        &["set-resource-policy", "global=manual"],
        true,
    );
    h.run(
        "set-resource-policy 칸 없음",
        &["set-resource-policy"],
        true,
    );

    // 문답과 대화
    let rows = h.peek(&["list-docs", "limit=0"]);
    let note = strings(&rows, "id").into_iter().next().unwrap_or_default();
    h.run("get-ask", &["get-ask", &note], false);
    h.run(
        "append-ask-turn",
        &[
            "append-ask-turn",
            &note,
            "session-id=11111111-1111-1111-1111-111111111111",
            "turn-id=t1",
            "mark-id=m1",
            "asked-at=2026-11-15T09:00:00+09:00",
            "quote=본문",
            "question=무엇인가",
            "answer=이것이다",
            "model=sonnet",
            "effort=low",
            "elapsed-ms=1200",
            "session-lines=3",
        ],
        true,
    );
    h.run("get-ask 뒤", &["get-ask", &note], false);
    h.run("list-chats", &["list-chats", &note], false);
    h.run(
        "append-chat-turn",
        &[
            "append-chat-turn",
            &note,
            "chat-id=cc1",
            "source-session=22222222-2222-2222-2222-222222222222",
            "turn-id=t1",
            "asked-at=2026-11-15T09:10:00+09:00",
            "question=대화 질문",
            "answer=대화 답",
            "model=sonnet",
            "effort=low",
            "elapsed-ms=800",
        ],
        true,
    );
    h.run("list-chats 뒤", &["list-chats", &note], false);

    // 세션 읽기(합성 세션 홈이 비어 있어 오류 글과 빈 결과를 견준다)
    h.run(
        "read-session 없음",
        &["read-session", "33333333-3333-3333-3333-333333333333"],
        false,
    );
    h.run(
        "read-session 옵션",
        &[
            "read-session",
            "33333333-3333-3333-3333-333333333333",
            "limit=5",
            "before=2",
            "urls=true",
        ],
        false,
    );
    h.run("read-session 모양 잘못", &["read-session", "x"], false);
    h.run(
        "session-titles",
        &[
            "session-titles",
            "33333333-3333-3333-3333-333333333333",
            "44444444-4444-4444-4444-444444444444",
        ],
        false,
    );
    h.run(
        "session-titles 모양 잘못",
        &["session-titles", "a", "b"],
        false,
    );
    h.run("recent-sessions", &["recent-sessions", "days=3"], false);
    h.run(
        "recent-sessions 잘못",
        &["recent-sessions", "days=x"],
        false,
    );
}

/// 세션 표식(환경변수)은 도구 표면(AI)의 호출에만 뜻이 있다(Java `CreateDoc`: "도구 표면 전용"). 명령 표면은 남기지 않고, 도구 표면은
/// 만든 노트에 `sessionId`와(claude가 아니면) `sessionAgent`를 남긴다. Java 단발 모드는 세션 표식을 받을 길이 없어(표식이 NUL 글자를
/// 쓰는 소켓 요청에만 실린다) Rust 사본에만 돌리고 결과 값을 직접 본다. 마지막에 둔다(이 뒤로는 두 사본이 달라진다).
fn cases_session_env(h: &mut Harness) {
    type Case = (
        &'static str,
        bool,
        Vec<(&'static str, &'static str)>,
        Option<&'static str>,
        Option<&'static str>,
    );
    let cases: Vec<Case> = vec![
        (
            "명령 표면 claude",
            false,
            vec![("CLAUDE_CODE_SESSION_ID", "sess-claude-1")],
            None,
            None,
        ),
        (
            "도구 claude",
            true,
            vec![("CLAUDE_CODE_SESSION_ID", "sess-claude-1")],
            Some("sess-claude-1"),
            None,
        ),
        (
            "도구 codex",
            true,
            vec![("CODEX_THREAD_ID", "sess-codex-1")],
            Some("sess-codex-1"),
            Some("codex"),
        ),
        (
            "도구 고정 쌍",
            true,
            vec![
                ("AISURADA_SESSION_ID", "sess-pin-1"),
                ("AISURADA_SESSION_AGENT", "gemini"),
            ],
            Some("sess-pin-1"),
            Some("gemini"),
        ),
        (
            "도구 둘 다 보임",
            true,
            vec![("CLAUDE_CODE_SESSION_ID", "c"), ("CODEX_THREAD_ID", "x")],
            None,
            None,
        ),
        ("도구 없음", true, vec![], None, None),
    ];
    for (label, tool, vars, want_id, want_agent) in cases {
        h.step += 1;
        let title = format!("세션 {label}");
        let args: Vec<String> = if tool {
            vec![
                "create_doc".into(),
                format!(
                    r##"{{"agenda":"demo","type":"plan","title":"{title}","body":"# 세션\n"}}"##
                ),
            ]
        } else {
            vec![
                "create-doc".into(),
                "agenda=demo".into(),
                "type=plan".into(),
                format!("title={title}"),
                "body=# 세션\n".into(),
            ]
        };
        let outcome = h.rust(&args, &vars);
        let id = serde_json::from_str::<Value>(&outcome.stdout)
            .ok()
            .and_then(|v| v["id"].as_str().map(str::to_string))
            .unwrap_or_default();
        let doc = h.peek(&["get-doc", &id]);
        let (got_id, got_agent) = (doc["sessionId"].as_str(), doc["sessionAgent"].as_str());
        h.record(
            format!("세션 표식 {label}"),
            outcome.code == 0 && got_id == want_id && got_agent == want_agent,
            || format!("종료 {} id {got_id:?} agent {got_agent:?} 기대 {want_id:?} {want_agent:?}\n    {}", outcome.code, clip(&outcome.stderr)),
        );
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let verbose = args.iter().any(|a| a == "--verbose");
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let record: Option<PathBuf> = args
        .iter()
        .find_map(|a| a.strip_prefix("--record="))
        .map(PathBuf::from);
    let replay: Option<PathBuf> = args
        .iter()
        .find_map(|a| a.strip_prefix("--replay="))
        .map(PathBuf::from);
    // 재생은 기준 엔진(jar)이 없으므로 위치 인자는 합성 vault 하나다
    let (jar, fixture) = match (&replay, positional.as_slice()) {
        (Some(_), [fixture]) => (String::new(), (*fixture).clone()),
        (None, [jar, fixture]) => ((*jar).clone(), (*fixture).clone()),
        _ => {
            eprintln!(
                "사용법: cli_compat <기준 ais-engine.jar(build-oracle.sh)> <합성 vault> [--verbose] [--history] [--record=<테이프>]\n        cli_compat --replay=<테이프> <합성 vault> [--history]"
            );
            return ExitCode::from(2);
        }
    };
    let tape = match (&replay, &record) {
        (Some(path), _) => match tape::Tape::replay(path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::from(2);
            }
        },
        (None, Some(path)) => tape::Tape::record(path.clone()),
        (None, None) => tape::Tape::live(),
    };
    let mut h = Harness::new(&jar, Path::new(&fixture), verbose, tape);

    cases_surface(&mut h);
    cases_parse_errors(&mut h);
    cases_reads(&mut h);
    cases_tools_read(&mut h);
    cases_reindex_paths(&mut h);
    if args.iter().any(|a| a == "--history") {
        cases_history(&mut h);
    }
    cases_writes(&mut h);
    cases_families(&mut h);
    cases_tools_write(&mut h);
    cases_session_env(&mut h);

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
    // 어긋남이 있으면 녹화를 버린다. 재생은 테이프가 남지 않았는지 본다.
    let tape_error = if failed {
        None
    } else {
        h.tape.borrow().finish().err()
    };
    if let Some(e) = &tape_error {
        println!("테이프 오류: {e}");
    } else if !failed && h.tape.borrow().is_record() {
        println!("테이프 {}줄을 적었다", h.tape.borrow().recorded_len());
    }
    let failed = failed || tape_error.is_some();
    let _ = fs::remove_dir_all(&h.root);
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
