//! 의존 방향 시험: 모듈 사이 참조와 바깥(파일·시계·환경 변수·프로세스) 직접 호출의 지금 모양을 막아 둔다.
//!
//! DDD·헥사고날 리팩토링(`docs/refactor-ddd-hexagonal-plan.md`)의 안전망이다. 소스를 읽어 아래 둘을 센다.
//! - 모듈 사이 참조: `ais-core/src`의 모듈(`commands`, `vault`, `write`, …)이 다른 모듈을 `crate::모듈`로 적은 횟수
//! - 바깥 직접 호출: `fs::`(파일), `SystemTime::now`(시계), `env::var`(환경 변수), `Command::new`(프로세스), `tzset`·`localtime_r`(시간대)를
//!   적은 횟수. 파일별로 센다
//!
//! 센 값은 `tests/architecture-baseline.txt`에 박제해 둔다. **새 참조·새 호출이 생기거나 횟수가 늘면 실패한다**(거꾸로 된 의존이 번지는 것을 막는다).
//! 리팩토링으로 횟수가 **줄어도** 실패한다. 줄어든 값을 기준에 반영해 다시는 되돌아가지 못하게 하려는 것이다.
//! 기준을 새로 쓸 때는 `UPDATE_ARCH_BASELINE=1 cargo test -p ais-core --test architecture`를 돌린다(바뀐 줄은 `git diff`로 리뷰한다).
//!
//! 「거꾸로 된 의존」(안쪽 모듈이 명령 층이나 옆 컨텍스트를 쓰는 길) 10줄기를 1단계(공용 커널)에서 모두 끊었다. 이제 새로 생기면 실패한다.
//! 끊은 줄기: `write→commands`, `write→task`, `vault→write`, `task→commands`, `resource→commands`, `resource→task`, `session→commands`,
//! `session→task`, `session→write`, `model→commands`. 여러 영역이 같이 쓰던 코드는 `kernel`로 옮겼다.
//!
//! 세는 방법(규칙을 여기 못박는다): `//`로 시작하는 줄(주석, 문서 주석)은 뺀다. 문자열 안의 글자와 `#[cfg(test)]` 코드는 구별하지 않는다.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const BASELINE: &str = "tests/architecture-baseline.txt";

/// 바깥 직접 호출을 세는 패턴(이름, 소스에서 찾을 글). 글이 나온 횟수를 센다.
const PATTERNS: &[(&str, &str)] = &[
    ("fs", "fs::"),
    ("systemtime", "SystemTime::now"),
    ("env", "env::var"),
    ("process", "Command::new"),
    ("tz", "tzset"),
    ("tz", "localtime_r"),
];

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .flatten()
        .collect();
    entries.sort_by_key(|e| e.path());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// 주석 줄을 뺀 코드 줄
fn code(text: &str) -> String {
    text.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `crate::` 바로 뒤에 오는 모듈 이름들. `crate::a::b`는 `a`, `crate::{a::b, c}`는 `a`와 `c`다.
fn crate_targets(code: &str) -> Vec<String> {
    let bytes = code.as_bytes();
    let needle = b"crate::";
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle.len() <= bytes.len() {
        if &bytes[i..i + needle.len()] == needle
            && (i == 0 || !(bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_'))
        {
            let mut j = i + needle.len();
            if bytes.get(j) == Some(&b'{') {
                // 중괄호 묶음: 깊이 1의 항목 첫 이름만 모은다
                let (mut depth, mut start) = (0usize, j + 1);
                let mut k = j;
                while k < bytes.len() {
                    match bytes[k] {
                        b'{' => {
                            depth += 1;
                            if depth == 1 {
                                start = k + 1;
                            }
                        }
                        b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                push_ident(&code[start..k], &mut out);
                                break;
                            }
                        }
                        b',' if depth == 1 => {
                            push_ident(&code[start..k], &mut out);
                            start = k + 1;
                        }
                        _ => {}
                    }
                    k += 1;
                }
                j = k;
            } else {
                push_ident(&code[j..], &mut out);
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    out
}

fn push_ident(text: &str, out: &mut Vec<String>) {
    let ident: String = text
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if !ident.is_empty() {
        out.push(ident);
    }
}

/// 현재 소스에서 센 값(열쇠 → 횟수)
fn measure() -> BTreeMap<String, usize> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();

    // 모듈 사이 참조: ais-core/src 아래 첫 폴더(또는 파일)가 모듈이다
    let core_src = manifest().join("src");
    let mut files = Vec::new();
    rust_files(&core_src, &mut files);
    for file in &files {
        let rel = file.strip_prefix(&core_src).unwrap();
        let module = rel
            .components()
            .next()
            .unwrap()
            .as_os_str()
            .to_string_lossy()
            .trim_end_matches(".rs")
            .to_string();
        if module == "lib" {
            continue;
        }
        let text = code(&fs::read_to_string(file).unwrap());
        for target in crate_targets(&text) {
            if target != module {
                *counts
                    .entry(format!("edge {module} -> {target}"))
                    .or_default() += 1;
            }
        }
    }

    // 바깥 직접 호출: ais-core와 ais-cli의 src를 파일별로
    for (krate, src) in [
        ("core", manifest().join("src")),
        ("cli", manifest().join("../ais-cli/src")),
    ] {
        let mut files = Vec::new();
        rust_files(&src, &mut files);
        for file in files {
            let rel = file
                .strip_prefix(&src)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let text = code(&fs::read_to_string(&file).unwrap());
            for (name, pattern) in PATTERNS {
                let n = text.matches(pattern).count();
                if n > 0 {
                    *counts
                        .entry(format!("{name} {krate}/{rel}"))
                        .or_default() += n;
                }
            }
        }
    }
    counts
}

fn render(counts: &BTreeMap<String, usize>) -> String {
    let mut text = String::from(
        "# 의존 방향 시험의 기준. UPDATE_ARCH_BASELINE=1 cargo test -p ais-core --test architecture 로 새로 쓴다.\n\
         # edge <부르는 모듈> -> <불리는 모듈> <횟수> / <패턴> <크레이트>/<파일> <횟수>\n",
    );
    for (key, n) in counts {
        text.push_str(&format!("{key} {n}\n"));
    }
    text
}

fn parse(text: &str) -> BTreeMap<String, usize> {
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .map(|l| {
            let (key, n) = l.rsplit_once(' ').expect("기준 줄 모양");
            (key.to_string(), n.parse().expect("횟수"))
        })
        .collect()
}

#[test]
fn dependencies_and_outside_calls_match_the_baseline() {
    let now = measure();
    let path = manifest().join(BASELINE);
    if std::env::var_os("UPDATE_ARCH_BASELINE").is_some() {
        fs::write(&path, render(&now)).expect("기준 파일을 못 썼다");
        return;
    }
    let want = parse(&fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}를 못 읽었다({e}). UPDATE_ARCH_BASELINE=1로 먼저 만든다",
            path.display()
        )
    }));

    let mut grew = Vec::new();
    let mut shrank = Vec::new();
    for (key, n) in &now {
        match want.get(key) {
            None => grew.push(format!("새로 생겼다: {key} {n}")),
            Some(w) if n > w => grew.push(format!("늘었다: {key} {w} -> {n}")),
            Some(w) if n < w => shrank.push(format!("줄었다: {key} {w} -> {n}")),
            _ => {}
        }
    }
    for (key, w) in &want {
        if !now.contains_key(key) {
            shrank.push(format!("없어졌다: {key} {w} -> 0"));
        }
    }
    assert!(
        grew.is_empty(),
        "거꾸로 된 의존이나 바깥 직접 호출이 늘었다. 포트나 알맞은 모듈을 거치게 고친다.\n  {}",
        grew.join("\n  ")
    );
    assert!(
        shrank.is_empty(),
        "의존이 줄었다. 좋은 일이다. 줄어든 값을 기준에 반영해 다시 늘지 않게 한다(UPDATE_ARCH_BASELINE=1 cargo test -p ais-core --test architecture).\n  {}",
        shrank.join("\n  ")
    );
}

#[test]
fn inner_modules_do_not_use_the_command_layer_or_sideways_contexts() {
    // 거꾸로 된 의존 10줄기는 1단계에서 모두 끊었다. 목록은 비었고, 여기에 줄이 늘면 새 위반을 허용하는 것이다.
    const KNOWN: &[(&str, &str)] = &[];
    // 안쪽 모듈은 명령 층(commands)을 못 쓴다. 안쪽 모듈이 옆 컨텍스트(task, vault, write)를 쓰는 것도 알려진 줄만 허용한다.
    let inner = ["write", "vault", "task", "resource", "session", "model"];
    let now = measure();
    let mut unknown = Vec::new();
    for key in now.keys().filter_map(|k| k.strip_prefix("edge ")) {
        let (from, to) = key.split_once(" -> ").unwrap();
        let to_commands = to == "commands" && inner.contains(&from);
        let sideways = ["task", "write"].contains(&to) && inner.contains(&from);
        // task→write와 resource→write는 도메인 후보가 쓰기 기반 부품을 직접 보는 길이라 5단계(저장소 포트)가 없앨 때까지 둔다(위반 목록과 별개)
        let transitional = matches!((from, to), ("task", "write") | ("resource", "write"));
        if (to_commands || sideways) && !transitional && !KNOWN.contains(&(from, to)) {
            unknown.push(key.to_string());
        }
    }
    assert!(unknown.is_empty(), "새 거꾸로 된 의존이다: {unknown:?}");
}

#[test]
fn the_kernel_uses_no_context_module() {
    // 공용 커널은 영역(vault, write, task, resource, session, commands, model, engine)을 쓰지 않는다. 쓰는 것은 바탕 모듈뿐이다.
    const BASE: &[&str] = &["text", "json", "frontmatter", "error", "kernel"];
    let now = measure();
    let bad: Vec<&str> = now
        .keys()
        .filter_map(|k| k.strip_prefix("edge kernel -> "))
        .filter(|to| !BASE.contains(to))
        .collect();
    assert!(bad.is_empty(), "커널이 영역을 쓴다: {bad:?}");
}

/// 값 객체(`kernel`)로 모은 닫힌 집합의 글자는 `kernel` 밖에 쓰지 않는다. 층, 문서 유형, 연결 종류, 색의 목록과 판정은 한 곳이 정본이다.
/// 허용은 「이 파일에 이 글자가 이만큼」 꼴로만 적는다(표기 그대로 응답이나 시험 값으로 쓰는 곳).
#[test]
fn closed_set_literals_live_only_in_the_kernel() {
    // (찾을 글, 이유, 허용 목록 [(파일, 횟수)])
    #[allow(clippy::type_complexity)]
    let rules: &[(&str, &[(&str, usize)])] = &[
        ("\"record\"", &[]),
        ("\"reading\"", &[]),
        ("\"plan\", \"build\"", &[]),
        ("[plan, build", &[]),
        ("\"indirect\"", &[]),
        ("\"duplicates\"", &[]),
        ("\"relates\"", &[]),
        // 태스크 응답의 `blocks` 칸 이름이다(연결 종류가 아니라 응답 필드)
        ("\"blocks\"", &[("commands/task_commands.rs", 1)]),
    ];
    let src = manifest().join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    let mut bad = Vec::new();
    for file in files {
        let rel = file
            .strip_prefix(&src)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        if rel.starts_with("kernel/") {
            continue;
        }
        let text = code(&fs::read_to_string(&file).unwrap());
        for (pattern, allowed) in rules {
            let n = text.matches(pattern).count();
            let ok = allowed
                .iter()
                .find(|(f, _)| *f == rel)
                .map_or(0, |(_, count)| *count);
            if n > ok {
                bad.push(format!("{rel}: {pattern} {n}번(허용 {ok})"));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "kernel로 모은 글자가 밖에 있다. 값 객체(Tier, DocType, LinkKind, TaskLinkKind, Hue)를 쓴다.\n  {}",
        bad.join("\n  ")
    );
}
