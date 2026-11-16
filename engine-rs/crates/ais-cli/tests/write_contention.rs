//! 프로세스 여럿이 같은 vault와 같은 노트를 동시에 쓰는 경합 시험([#63](https://github.com/bnjjong/aisurada/issues/63)).
//!
//! 잠금이 서로를 막는 것은 `flock_lock`의 단위 시험이 확인한다. 여기서는 실제 `ais-cli` 프로세스를 여럿 띄워서
//! 잠금이 쓰기를 한 줄로 세우는 결과(잃어버린 갱신이 없고, 반쯤 쓴 파일이 없고, 쥐고 있던 쪽이 죽어도 풀린다)를 본다.
//!
//! 모든 시험은 임시 폴더의 HOME, vault, 잠금 폴더만 쓴다. 진짜 HOME과 vault는 건드리지 않는다.

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

use ais_core::adapters::flock_lock as lock;
use serde_json::Value;

/// 시험 하나가 쓰는 임시 세계: HOME, vault, 잠금 폴더
struct World {
    base: PathBuf,
}

impl World {
    fn new(name: &str) -> World {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let base = std::env::temp_dir().join(format!(
            "ais-contention-{}-{}-{name}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("vault")).unwrap();
        std::fs::create_dir_all(base.join("home")).unwrap();
        let world = World { base };
        let init = world.ais(&["init-workspace", "name=contention"]);
        assert_eq!(init.0, 0, "워크스페이스를 못 만들었다: {}", init.2);
        world
    }

    fn vault(&self) -> PathBuf {
        self.base.join("vault")
    }

    fn locks(&self) -> PathBuf {
        self.base.join("locks")
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_ais-cli"));
        command
            .args(args)
            .env_clear()
            .env("HOME", self.base.join("home"))
            .env("AISURADA_VAULT", self.vault())
            .env("AISURADA_LOCK_DIR", self.locks());
        command
    }

    /// (종료 코드, 표준 출력, 표준 오류)
    fn ais(&self, args: &[&str]) -> (i32, String, String) {
        let output = self.command(args).output().expect("ais-cli를 못 돌렸다");
        (
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    }

    /// 성공해야 하는 호출. 응답 JSON을 돌려준다.
    fn ok(&self, args: &[&str]) -> Value {
        let (code, stdout, stderr) = self.ais(args);
        assert_eq!(code, 0, "{args:?} 실패: {stderr}");
        serde_json::from_str(&stdout)
            .unwrap_or_else(|e| panic!("{args:?} 응답이 JSON이 아니다: {e}\n{stdout}"))
    }

    /// 새 노트를 만들고 `(id, precondition)`을 돌려준다.
    fn create_note(&self, title: &str) -> (String, String) {
        let response = self.ok(&[
            "create-doc",
            "agenda=demo",
            "type=free",
            &format!("title={title}"),
            "body=본문",
        ]);
        (text(&response, "id"), text(&response, "precondition"))
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

fn text(value: &Value, key: &str) -> String {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("{key}가 글이 아니다: {value}"))
        .to_owned()
}

/// vault 안에 원자적 쓰기의 임시 파일(`*.tmp`)이 남아 있지 않은지 본다.
fn assert_no_temp_files(vault: &Path) {
    let mut stack = vec![vault.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let name = entry.file_name().to_string_lossy().into_owned();
                assert!(
                    !name.ends_with(".tmp"),
                    "임시 파일이 남았다: {}",
                    path.display()
                );
            }
        }
    }
}

/// 일꾼 `workers`개를 한꺼번에 출발시켜 각자 `work(번호)`를 돌린다. 결과를 번호 순으로 모은다.
fn run_workers<T: Send + 'static>(
    workers: usize,
    work: impl Fn(usize) -> T + Send + Sync + 'static,
) -> Vec<T> {
    let start = Arc::new(Barrier::new(workers));
    let work = Arc::new(work);
    let handles: Vec<_> = (0..workers)
        .map(|n| {
            let (start, work) = (start.clone(), work.clone());
            std::thread::spawn(move || {
                start.wait();
                work(n)
            })
        })
        .collect();
    handles.into_iter().map(|h| h.join().unwrap()).collect()
}

#[test]
fn comments_from_many_processes_are_all_kept() {
    // 코멘트 쓰기는 파일을 읽고 고쳐 다시 쓴다. 잠금이 없으면 한 쪽의 코멘트가 사라진다.
    const WORKERS: usize = 4;
    const EACH: usize = 6;
    let world = Arc::new(World::new("comments"));
    let (id, _) = world.create_note("코멘트 경합");

    let (w, note) = (world.clone(), id.clone());
    let results = run_workers(WORKERS, move |n| {
        (0..EACH)
            .map(|i| w.ais(&["create-comment", &note, &format!("body=w{n}-{i}")]))
            .collect::<Vec<_>>()
    });
    for (code, _, stderr) in results.iter().flatten() {
        assert_eq!(*code, 0, "코멘트 쓰기가 졌다: {stderr}");
    }

    let listed = world.ok(&["list-comments", &id]);
    let comments = listed["comments"].as_array().expect("comments 배열");
    let bodies: BTreeSet<String> = comments.iter().map(|c| text(c, "body")).collect();
    let ids: BTreeSet<String> = comments.iter().map(|c| text(c, "id")).collect();
    let expected: BTreeSet<String> = (0..WORKERS)
        .flat_map(|n| (0..EACH).map(move |i| format!("w{n}-{i}")))
        .collect();
    assert_eq!(comments.len(), WORKERS * EACH, "코멘트가 사라졌거나 겹쳤다");
    assert_eq!(bodies, expected, "어느 코멘트가 사라졌다");
    assert_eq!(ids.len(), comments.len(), "코멘트 id가 겹쳤다");
    assert_no_temp_files(&world.vault());
}

#[test]
fn only_one_of_two_simultaneous_updates_with_the_same_token_wins() {
    // 같은 f1(내용 확인 토큰)을 든 두 프로세스가 동시에 같은 노트를 고친다. 확인과 쓰기가 한 잠금 안에 있어야
    // 둘 다 통과하는 일이 없다. 늦게 온 쪽은 AIS-412(종료 코드 5)로 서고 파일은 이긴 쪽의 글이다.
    const ROUNDS: usize = 12;
    let world = Arc::new(World::new("one-winner"));
    for round in 0..ROUNDS {
        let (id, token) = world.create_note(&format!("승자 {round}"));
        let (w, note, f1) = (world.clone(), id.clone(), token.clone());
        let results = run_workers(2, move |n| {
            w.ais(&[
                "update-doc",
                &note,
                &format!("f1={f1}"),
                &format!("body=쓴 쪽 {n}"),
            ])
        });
        let codes: Vec<i32> = results.iter().map(|r| r.0).collect();
        let winners: Vec<usize> = codes
            .iter()
            .enumerate()
            .filter(|(_, c)| **c == 0)
            .map(|(n, _)| n)
            .collect();
        assert_eq!(
            winners.len(),
            1,
            "{round}회: 이긴 쪽이 하나여야 한다: {codes:?}"
        );
        let loser = 1 - winners[0];
        assert_eq!(
            codes[loser], 5,
            "{round}회: 진 쪽은 종료 코드 5여야 한다: {}",
            results[loser].2
        );
        assert!(results[loser].2.contains("AIS-412"), "{}", results[loser].2);

        let after = world.ok(&["get-doc", &id]);
        assert_eq!(
            text(&after, "body").trim_end(),
            format!("쓴 쪽 {}", winners[0]),
            "{round}회: 진 쪽의 글이 섞이면 안 된다"
        );
    }
    assert_no_temp_files(&world.vault());
}

#[test]
fn appends_with_retry_lose_nothing_under_contention() {
    // 일꾼마다 「읽고, 본문 끝에 자기 표시를 붙여, 읽은 토큰으로 쓴다」를 되풀이한다. 충돌(종료 코드 5)이면 다시 읽고
    // 다시 붙인다. 끝에 모든 표시가 한 번씩만 있어야 한다. 잠금이 헐거우면 한 쪽의 덧붙임이 덮어써져 사라진다.
    const WORKERS: usize = 4;
    const EACH: usize = 4;
    let world = Arc::new(World::new("append"));
    let (id, _) = world.create_note("덧붙이기 경합");

    let (w, note) = (world.clone(), id.clone());
    let conflicts: usize = run_workers(WORKERS, move |n| {
        let mut conflicts = 0;
        for i in 0..EACH {
            let mark = format!("w{n}-{i}");
            loop {
                let current = w.ok(&["get-doc", &note]);
                let body = format!("{}\n{mark}", text(&current, "body").trim_end());
                let (code, _, stderr) = w.ais(&[
                    "update-doc",
                    &note,
                    &format!("f1={}", text(&current, "precondition")),
                    &format!("body={body}"),
                ]);
                match code {
                    0 => break,
                    5 => conflicts += 1,
                    _ => panic!("뜻밖의 실패({code}): {stderr}"),
                }
            }
        }
        conflicts
    })
    .into_iter()
    .sum();
    eprintln!("덧붙이기 경합: 충돌 {conflicts}번을 다시 읽어 넘겼다");

    let body = text(&world.ok(&["get-doc", &id]), "body");
    let lines: Vec<&str> = body.lines().collect();
    assert_eq!(lines[0], "본문");
    let marks: BTreeSet<&str> = lines[1..].iter().copied().collect();
    let expected: BTreeSet<String> = (0..WORKERS)
        .flat_map(|n| (0..EACH).map(move |i| format!("w{n}-{i}")))
        .collect();
    assert_eq!(
        lines.len() - 1,
        WORKERS * EACH,
        "표시가 사라졌거나 겹쳤다:\n{body}"
    );
    assert_eq!(
        marks.iter().map(|s| s.to_string()).collect::<BTreeSet<_>>(),
        expected,
        "어느 덧붙임이 사라졌다"
    );
    assert_no_temp_files(&world.vault());
}

#[test]
fn notes_created_at_the_same_time_all_land_and_show_in_the_listing() {
    const WORKERS: usize = 4;
    const EACH: usize = 4;
    let world = Arc::new(World::new("create"));

    let w = world.clone();
    let created: Vec<String> = run_workers(WORKERS, move |n| {
        (0..EACH)
            .map(|i| {
                let response = w.ok(&[
                    "create-doc",
                    "agenda=demo",
                    "type=free",
                    &format!("title=동시 {n}-{i}"),
                    "body=본문",
                ]);
                text(&response, "id")
            })
            .collect::<Vec<_>>()
    })
    .into_iter()
    .flatten()
    .collect();
    let distinct: BTreeSet<&String> = created.iter().collect();
    assert_eq!(distinct.len(), WORKERS * EACH, "id가 겹쳤다");

    let listed = world.ok(&["list-docs", "agenda=demo"]);
    let items = listed
        .as_array()
        .cloned()
        .or_else(|| listed["items"].as_array().cloned())
        .expect("목록");
    let listed_ids: BTreeSet<String> = items.iter().map(|d| text(d, "id")).collect();
    assert_eq!(
        listed_ids.len(),
        WORKERS * EACH,
        "목록에서 빠진 노트가 있다"
    );
    assert_eq!(listed_ids, created.into_iter().collect());
    assert_no_temp_files(&world.vault());
}

#[test]
fn a_removed_lock_folder_is_made_again_by_the_next_write() {
    // 잠금 파일은 지우지 않지만, 아무 프로세스도 안 도는 때 폴더째 지우는 것이 안전하다는 안내(개발자 가이드)의 근거다.
    let world = World::new("lock-folder");
    let (id, _) = world.create_note("잠금 폴더");
    let before: Vec<_> = std::fs::read_dir(world.locks())
        .expect("쓰기가 잠금 폴더를 만들었어야 한다")
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert_eq!(
        before.len(),
        1,
        "vault 하나에 잠금 파일 하나여야 한다: {before:?}"
    );

    std::fs::remove_dir_all(world.locks()).unwrap();
    world.ok(&["create-comment", &id, "body=지운 뒤"]);

    let after: Vec<_> = std::fs::read_dir(world.locks())
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert_eq!(
        after, before,
        "같은 vault의 잠금 파일은 같은 이름으로 다시 생긴다"
    );
    assert_eq!(
        std::fs::metadata(world.locks().join(&after[0]))
            .unwrap()
            .len(),
        0,
        "잠금 파일은 빈 파일이다"
    );
}

/// 잠금을 쥔 채 죽을 때까지 기다리는 자식 프로세스. 시험이 `--ignored`로만 부른다.
#[test]
#[ignore = "killed_lock_holder 시험이 자식 프로세스로 부른다"]
fn hold_vault_lock_child() {
    let (Some(vault), Some(locks)) = (
        std::env::var_os("AIS_HOLD_VAULT"),
        std::env::var_os("AIS_HOLD_LOCKS"),
    ) else {
        return;
    };
    let _guard = lock::acquire(Path::new(&vault), Path::new(&locks)).unwrap();
    println!("AIS-LOCK-HELD");
    std::thread::sleep(Duration::from_secs(60));
}

#[test]
fn a_writer_waits_for_another_process_and_goes_on_when_it_is_killed() {
    let world = World::new("killed-holder");
    let (id, _) = world.create_note("잠금 쥔 쪽이 죽는다");

    // 다른 프로세스가 vault 잠금을 쥔다
    let mut holder = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "hold_vault_lock_child",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("AIS_HOLD_VAULT", world.vault())
        .env("AIS_HOLD_LOCKS", world.locks())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(holder.stdout.take().unwrap()).lines();
    let started = Instant::now();
    loop {
        let line = lines
            .next()
            .expect("잠금을 쥔 자식이 아무 말 없이 끝났다")
            .unwrap();
        // libtest가 「test 이름 ... 」을 줄바꿈 없이 먼저 찍어서 줄 끝으로 알아본다
        if line.trim_end().ends_with("AIS-LOCK-HELD") {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "자식이 잠금을 못 잡았다"
        );
    }

    // 이쪽 프로세스에서는 잠금이 안 잡힌다
    assert!(
        lock::try_acquire(&world.vault(), &world.locks())
            .unwrap()
            .is_none(),
        "다른 프로세스가 쥔 잠금이 잡혔다"
    );

    // 쓰기 명령은 끝나지 않고 기다린다
    let mut writer = world
        .command(&["create-comment", &id, "body=기다린 코멘트"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    assert!(
        writer.try_wait().unwrap().is_none(),
        "쓰기가 잠금을 무시하고 끝났다"
    );
    assert_eq!(
        world.ok(&["list-comments", &id])["state"],
        "absent",
        "기다리는 쓰기가 코멘트를 먼저 썼다"
    );

    // 쥔 쪽이 SIGKILL로 죽으면 운영체제가 잠금을 풀고 기다리던 쓰기가 이어서 끝난다
    holder.kill().unwrap();
    holder.wait().unwrap();
    let waited = Instant::now();
    let status = loop {
        if let Some(status) = writer.try_wait().unwrap() {
            break status;
        }
        assert!(
            waited.elapsed() < Duration::from_secs(20),
            "쥔 쪽이 죽었는데 쓰기가 안 풀린다"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    assert!(status.success(), "풀린 뒤 쓰기가 실패했다: {status:?}");

    let listed = world.ok(&["list-comments", &id]);
    let bodies: Vec<String> = listed["comments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| text(c, "body"))
        .collect();
    assert_eq!(bodies, vec!["기다린 코멘트".to_string()]);
    assert_no_temp_files(&world.vault());
}
