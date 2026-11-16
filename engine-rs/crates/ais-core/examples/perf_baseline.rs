//! 엔진의 속도와 최대 메모리를 노트 수별로 재는 개발용 도구입니다. 결과 표는 `docs/developer-guide.md`의 「성능 기준선」에 옮겨 적습니다.
//!
//! 사용(꼭 release로 돌립니다. debug는 열 배쯤 느립니다):
//! `cargo run --release -q -p ais-core --example perf_baseline -- [노트 수 ...]`
//! 노트 수를 안 주면 200, 2000, 10000이다. 크기마다 자기 자신을 자식 프로세스로 다시 띄워서 최대 메모리(`ru_maxrss`)가 크기별로 갈린다.
//!
//! 합성 vault는 임시 폴더에 만들고 끝나면 지운다. 진짜 vault는 건드리지 않는다. 자동 커밋은 끄고(쓰기 시간에 git 커밋이 섞이지 않게),
//! 노트 한 건(`history` 대상)만 판이 60개 쌓이게 git에 커밋한다.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use ais_core::engine::Engine;
use serde_json::{Value, json};

const AGENDAS: usize = 10;
const VERSIONS: usize = 60;
const RUNS: usize = 5;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--child") {
        child(args[1].parse().expect("노트 수"));
        return;
    }
    let sizes: Vec<usize> = if args.is_empty() {
        vec![200, 2000, 10000]
    } else {
        args.iter().map(|a| a.parse().expect("노트 수")).collect()
    };
    if cfg!(debug_assertions) {
        eprintln!("경고: debug로 돌고 있다. 기준선은 --release로 잰다.");
    }
    println!("{}\n", machine());
    println!("| 노트 수 | 항목 | 첫 호출(ms) | 가운데 값(ms) | 그때까지 최대 메모리(MB) |");
    println!("|--:|---|--:|--:|--:|");
    let exe = std::env::current_exe().expect("실행 파일");
    for size in sizes {
        let output = Command::new(&exe)
            .args(["--child", &size.to_string()])
            .output()
            .expect("자식을 못 돌렸다");
        assert!(
            output.status.success(),
            "자식이 실패했다({size}): {}",
            String::from_utf8_lossy(&output.stderr)
        );
        print!("{}", String::from_utf8_lossy(&output.stdout));
    }
}

fn machine() -> String {
    let brand = Command::new("sysctl")
        .args(["-n", "machdep.cpu.brand_string"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "알 수 없는 CPU".into());
    format!(
        "{} / {} / {} / {}, 한 항목을 {RUNS}번 재서 가운데 값을 적는다.",
        std::env::consts::OS,
        std::env::consts::ARCH,
        brand,
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    )
}

/// 이 프로세스의 지금까지 최대 상주 메모리(바이트)
fn max_rss_bytes() -> u64 {
    // SAFETY: getrusage는 0으로 채운 구조체에 쓰기만 한다.
    let usage = unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut usage);
        usage
    };
    let raw = usage.ru_maxrss as u64;
    // macOS는 바이트, Linux는 킬로바이트다
    if cfg!(target_os = "macos") {
        raw
    } else {
        raw * 1024
    }
}

fn child(size: usize) {
    let work = std::env::temp_dir().join(format!("ais-perf-{}-{size}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    let vault = work.join("vault");
    // SAFETY: 아직 스레드를 만들기 전이다.
    unsafe {
        std::env::set_var("AISURADA_LOCK_DIR", work.join("locks"));
        std::env::set_var("AISURADA_VAULT_AUTOCOMMIT_ENABLED", "false");
    }
    let target = build_vault(&vault, size);
    let engine = Engine::new(&vault);
    let call = |command: &str, args: Value| {
        engine
            .call(command, args)
            .unwrap_or_else(|e| panic!("{command}: {e:?}"))
    };
    let token = |id: &str| {
        call("get_doc", json!({"id": id}))["precondition"]
            .as_str()
            .unwrap()
            .to_string()
    };

    let mut table = String::new();
    let mut measure = |label: &str, mut run: Box<dyn FnMut() + '_>| {
        let mut millis = Vec::new();
        for _ in 0..RUNS {
            let started = Instant::now();
            run();
            millis.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        let first = millis[0];
        millis.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let _ = writeln!(
            table,
            "| {size} | {label} | {first:.1} | {:.1} | {:.0} |",
            millis[RUNS / 2],
            max_rss_bytes() as f64 / 1_048_576.0
        );
    };

    measure(
        "list_docs (메타, 전부)",
        Box::new(|| {
            let page = call("list_docs", json!({"limit": 0}));
            assert_eq!(page["total"], size);
        }),
    );
    measure(
        "list_docs (본문 포함, 전부)",
        Box::new(|| {
            let page = call("list_docs", json!({"limit": 0, "bodies": true}));
            assert_eq!(page["total"], size);
        }),
    );
    measure(
        "search_docs (많이 걸리는 말)",
        Box::new(|| {
            let found = call("search_docs", json!({"query": "성능"}));
            assert!(found["total"].as_u64().unwrap() > 0);
        }),
    );
    measure(
        "search_docs (한 건만 걸리는 말)",
        Box::new(|| {
            let found = call("search_docs", json!({"query": "희귀어"}));
            assert_eq!(found["total"], 1);
        }),
    );
    measure(
        "search_docs (안 걸리는 말)",
        Box::new(|| {
            let found = call("search_docs", json!({"query": "없는말없는말"}));
            assert_eq!(found["total"], 0);
        }),
    );
    measure(
        "list_tags",
        Box::new(|| {
            call("list_tags", Value::Null);
        }),
    );
    measure(
        "get_doc (한 건)",
        Box::new(|| {
            call("get_doc", json!({"id": target}));
        }),
    );
    measure(
        "history (판 60개)",
        Box::new(|| {
            let history = call("history", json!({"id": target}));
            assert_eq!(history["count"], VERSIONS);
        }),
    );
    measure(
        "update_doc (훑기와 쓰기 포함)",
        Box::new(|| {
            let f1 = token(&target);
            call(
                "update_doc",
                json!({"id": target, "title": "바뀐 제목", "f1": f1}),
            );
        }),
    );
    measure(
        "create_doc (훑기와 쓰기 포함)",
        Box::new(|| {
            call(
                "create_doc",
                json!({"agenda": "a0", "type": "free", "title": "새 노트", "body": "# 새 노트\n\n본문\n"}),
            );
        }),
    );
    print!("{table}");
    let _ = std::fs::remove_dir_all(&work);
}

/// 합성 vault를 만들고 `history` 대상 노트의 id를 돌려준다.
fn build_vault(vault: &Path, size: usize) -> String {
    let agendas = vault.join(".aisurada/agendas");
    std::fs::create_dir_all(&agendas).unwrap();
    std::fs::write(vault.join(".aisurada/workspace.json"), "{}\n").unwrap();
    for a in 0..AGENDAS {
        std::fs::write(
            agendas.join(format!("a{a}.md")),
            format!(
                "---\nid: agperf{a}-{a:08x}\nslug: a{a}\ntitle: 아젠다 {a}\n---\n# 아젠다 {a}\n"
            ),
        )
        .unwrap();
    }
    const SENTENCES: [&str; 8] = [
        "오늘은 배포 파이프라인의 성능을 다시 살펴보고 병목이 어디인지 정리했다.",
        "회의에서 나온 결정 사항은 아래 목록과 같고 각자 맡은 일을 이번 주 안에 끝내기로 했다.",
        "코드 리뷰에서 지적받은 부분을 고치고 시험을 다시 돌려 통과하는 것을 확인했다.",
        "다음 단계로 넘어가기 전에 위험 요소를 적어 두고 되돌리는 방법도 같이 남겼다.",
        "사용자가 보낸 의견을 읽고 화면의 문구와 순서를 손보기로 했다.",
        "데이터가 늘어났을 때 목록과 검색이 얼마나 느려지는지는 아직 재 보지 않았다.",
        "참고한 문서와 링크는 맨 아래에 모았고 나중에 다시 찾기 쉽게 이름을 붙였다.",
        "오래 걸리는 작업은 쪼개서 하루 단위로 확인할 수 있게 계획을 세웠다.",
    ];
    let mut target = String::new();
    for i in 0..size {
        let id = format!("ndperf{i}-{i:08x}");
        let agenda = i % AGENDAS;
        let month = 1 + i % 12;
        let dir = vault.join(format!("notes/a{agenda}/2026-{month:02}/n{i}"));
        std::fs::create_dir_all(&dir).unwrap();
        let mut body = format!("# 기록 {i}\n\n");
        for k in 0..6 {
            body.push_str(SENTENCES[(i + k) % SENTENCES.len()]);
            body.push(' ');
            if k % 2 == 1 {
                body.push_str("\n\n");
            }
        }
        if i % 7 == 0 {
            body.push_str("\n성능 점검 항목을 따로 적는다.\n");
        }
        if i == size / 2 {
            body.push_str("\n이 노트에만 희귀어가 들어 있다.\n");
            target = id.clone();
        }
        let day = 1 + i % 28;
        std::fs::write(
            dir.join("note.md"),
            format!(
                "---\nid: {id}\nagenda: a{agenda}\ntype: free\ntitle: 기록 {i}\ncreated: '2026-{month:02}-{day:02}T09:30:00+09:00'\ntags: [tag{}, 공통]\n---\n{body}",
                i % 20
            ),
        )
        .unwrap();
    }
    git(vault, &["init", "-q", "--initial-branch=main"]);
    git(vault, &["config", "user.name", "perf"]);
    git(vault, &["config", "user.email", "perf@example.invalid"]);
    git(vault, &["add", "-A"]);
    git(vault, &["commit", "-q", "-m", "seed"]);
    // history 대상 노트의 판을 쌓는다. 마지막 판(현재)이 60번째가 되도록 59번 더 고친다
    let dir = find_note_dir(vault, &target);
    let file = dir.join("note.md");
    for v in 1..VERSIONS {
        let mut text = std::fs::read_to_string(&file).unwrap();
        text.push_str(&format!("\n판 {v}에서 덧붙인 줄.\n"));
        std::fs::write(&file, text).unwrap();
        git(vault, &["add", "-A"]);
        git(vault, &["commit", "-q", "-m", &format!("v{v}")]);
    }
    target
}

fn find_note_dir(vault: &Path, id: &str) -> PathBuf {
    let n: usize = id
        .strip_prefix("ndperf")
        .and_then(|rest| rest.split('-').next())
        .unwrap()
        .parse()
        .unwrap();
    vault.join(format!(
        "notes/a{}/2026-{:02}/n{n}",
        n % AGENDAS,
        1 + n % 12
    ))
}

fn git(vault: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(vault)
        .env("GIT_AUTHOR_NAME", "perf")
        .env("GIT_COMMITTER_NAME", "perf")
        .env("GIT_AUTHOR_EMAIL", "perf@example.invalid")
        .env("GIT_COMMITTER_EMAIL", "perf@example.invalid")
        .status()
        .expect("git을 못 돌렸다");
    assert!(status.success(), "git {args:?}");
}
