//! 앱이 연 vault를 적어 둔 쪽지(`current-vault.json`)의 읽기 규칙. 지운 Java `VaultPointerTest`를 옮긴 것이다.
//! 이 시험은 임시 HOME 아래에만 쓰고 진짜 HOME과 환경 변수는 건드리지 않는다.

use ais_cli::env::{Env, resolve_vault};
use std::path::{Path, PathBuf};

fn home(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ais-pointer-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn pointer_file(home: &Path) -> PathBuf {
    home.join("Library/Application Support/aisurada/current-vault.json")
}

fn put(home: &Path, text: &str) {
    let file = pointer_file(home);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, text).unwrap();
}

fn env_with(home: &Path, vault_var: Option<&str>) -> Env {
    let mut env = Env {
        home: home.to_string_lossy().into_owned(),
        ..Env::default()
    };
    if let Some(value) = vault_var {
        env.vars.insert("AISURADA_VAULT".into(), value.into());
    }
    env
}

#[test]
fn no_file_or_no_home_means_no_vault() {
    let dir = home("none");
    assert_eq!(resolve_vault(&env_with(&dir, None)), None, "파일이 없으면");
    assert_eq!(resolve_vault(&Env::default()), None, "홈을 모르면");
    assert_eq!(
        resolve_vault(&env_with(&dir, Some(""))),
        None,
        "빈 환경변수도 파일이 없으면 없다"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_environment_variable_wins_unless_it_is_blank() {
    let dir = home("priority");
    put(
        &dir,
        "{\"vault\":\"/Users/me/other\",\"updatedAt\":\"2026-11-16T00:00:00Z\"}\n",
    );
    assert_eq!(
        resolve_vault(&env_with(&dir, Some("/env/vault"))).as_deref(),
        Some("/env/vault"),
        "환경변수가 이긴다"
    );
    for blank in ["   ", ""] {
        assert_eq!(
            resolve_vault(&env_with(&dir, Some(blank))).as_deref(),
            Some("/Users/me/other"),
            "{blank:?}는 없는 것"
        );
    }
    assert_eq!(
        resolve_vault(&env_with(&dir, None)).as_deref(),
        Some("/Users/me/other"),
        "환경변수가 없으면 앱이 적은 값"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn spaces_hangul_and_the_shapes_the_host_writes_are_read() {
    let dir = home("shapes");
    put(
        &dir,
        "{\"vault\":\"/Users/me/df ai/보관함\",\"updatedAt\":\"x\"}",
    );
    assert_eq!(
        resolve_vault(&env_with(&dir, None)).as_deref(),
        Some("/Users/me/df ai/보관함"),
        "공백과 한글이 든 경로"
    );
    // 키 차례가 다르고 갱신 시각이 숫자여도 읽는다(호스트가 쓰는 모양)
    put(
        &dir,
        "{\"vault\":\"/from/host\",\"updatedAt\":1794438764661}\n",
    );
    assert_eq!(
        resolve_vault(&env_with(&dir, None)).as_deref(),
        Some("/from/host")
    );
    // 앞뒤 공백은 뗀다
    put(&dir, "{\"vault\":\"  /padded/vault  \"}");
    assert_eq!(
        resolve_vault(&env_with(&dir, None)).as_deref(),
        Some("/padded/vault")
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_broken_file_is_no_vault_and_never_an_error() {
    let dir = home("broken");
    for bad in [
        "",
        "not json",
        "[]",
        "{}",
        "{\"vault\":5}",
        "{\"vault\":\"relative/path\"}",
        "{\"vault\":\"\"}",
        "{\"vault\":\"  \"}",
    ] {
        put(&dir, bad);
        assert_eq!(
            resolve_vault(&env_with(&dir, None)),
            None,
            "망가진 파일은 없는 것: {bad:?}"
        );
    }
    // 파일 자리가 폴더여도 없는 것이다
    let file = pointer_file(&dir);
    std::fs::remove_file(&file).unwrap();
    std::fs::create_dir(&file).unwrap();
    assert_eq!(
        resolve_vault(&env_with(&dir, None)),
        None,
        "파일 자리가 폴더"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
