//! Chapter 15. Secret values deliberately implement neither Debug nor Serialize.
use serde::Serialize;
use std::{
    collections::HashMap,
    io::Read,
    process::{Command, Stdio},
    sync::{mpsc, Mutex},
    time::Duration,
};
use tauri::State;
pub struct Values {
    values: Vec<String>,
    asked: usize,
    read: usize,
    short: Vec<String>,
    unreadable: Vec<String>,
}
pub enum Table {
    Empty,
    Ready(Values),
    Unavailable(String),
}
pub struct SecretState {
    pub text: &'static str,
    #[cfg(test)]
    pub source: &'static str,
    table: Mutex<Table>,
}
impl Default for SecretState {
    fn default() -> Self {
        Self {
            text: include_str!(env!("WITHLOG_SECRET_TABLE")),
            #[cfg(test)]
            source: env!("WITHLOG_SECRET_TABLE_SOURCE"),
            table: Mutex::new(Table::Empty),
        }
    }
}
#[derive(Serialize)]
pub struct Status {
    ready: bool,
    reason: Option<String>,
    asked: usize,
    read: usize,
    maskable: usize,
    short: Vec<String>,
    unreadable: Vec<String>,
}
fn status(t: &Table) -> Status {
    match t {
        Table::Ready(v) => Status {
            ready: true,
            reason: None,
            asked: v.asked,
            read: v.read,
            maskable: v.values.len(),
            short: v.short.clone(),
            unreadable: v.unreadable.clone(),
        },
        _ => Status {
            ready: false,
            reason: Some(match t {
                Table::Unavailable(e) => e.clone(),
                _ => "표를 아직 안 떴다".into(),
            }),
            asked: 0,
            read: 0,
            maskable: 0,
            short: vec![],
            unreadable: vec![],
        },
    }
}
#[tauri::command]
pub fn secret_table_status(state: State<'_, SecretState>) -> Status {
    status(&state.table.lock().unwrap())
}
fn names(text: &str) -> Result<Vec<String>, String> {
    let (mut inside, mut begin, mut end, mut exports) = (false, false, false, 0);
    let mut names = vec![];
    for l in text.split('\n').map(|l| l.trim_end_matches('\r')) {
        if l.starts_with("# === KCD secrets BEGIN") {
            inside = true;
            begin = true;
            continue;
        }
        if l.starts_with("# === KCD secrets END") {
            inside = false;
            end = true;
            continue;
        }
        if !inside {
            continue;
        }
        if let Some(rest) = l.trim_start().strip_prefix("export ") {
            exports += 1;
            if let Some((n, _)) = rest.split_once('=') {
                if !n.is_empty()
                    && n.bytes().enumerate().all(|(i, b)| {
                        b.is_ascii_uppercase() || b == b'_' || (i > 0 && b.is_ascii_digit())
                    })
                {
                    names.push(n.into());
                }
            }
        }
    }
    if !begin {
        return Err("# === KCD secrets BEGIN 머리줄을 못 찾았다".into());
    }
    if !end {
        return Err("# === KCD secrets END 꼬리줄을 못 찾았다".into());
    }
    if names.is_empty() {
        return Err("managed block 에서 이름을 하나도 못 뽑았다".into());
    }
    if exports != names.len() {
        return Err(format!("블록 안 export 줄이 {exports}개인데 이름은 {}개만 뽑혔다. secret.sh 가 줄 모양을 바꿨다",names.len()));
    }
    Ok(names)
}
fn classes(text: &str) -> Result<HashMap<String, bool>, String> {
    let mut out = HashMap::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut words = line.split_whitespace();
        let name = words.next().unwrap();
        let kind = words
            .next()
            .ok_or_else(|| format!("{}행 {name}: 분류가 없다", i + 1))?;
        let secret = match kind {
            "secret" => true,
            "config" => false,
            _ => return Err(format!("{}행 {name}: 모르는 분류 {kind}", i + 1)),
        };
        out.entry(name.into()).or_insert(secret);
    }
    if out.is_empty() {
        return Err("분류표가 비었다".into());
    }
    Ok(out)
}
fn environment(bytes: &[u8]) -> Result<HashMap<String, String>, String> {
    if bytes.is_empty() {
        return Err("프로브가 아무것도 안 냈다".into());
    }
    let mut out = HashMap::new();
    for item in bytes.split(|b| *b == 0) {
        let Ok(item) = std::str::from_utf8(item) else {
            continue;
        };
        if let Some((n, v)) = item.split_once('=') {
            if !n.is_empty() {
                out.entry(n.into()).or_insert_with(|| v.into());
            }
        }
    }
    if out.is_empty() {
        return Err("프로브 출력에서 항목을 하나도 못 읽었다".into());
    }
    Ok(out)
}
fn build(
    names: &[String],
    classes: &HashMap<String, bool>,
    env: &HashMap<String, String>,
) -> Result<Values, String> {
    let unknown = names
        .iter()
        .filter(|n| !classes.contains_key(*n))
        .cloned()
        .collect::<Vec<_>>();
    if !unknown.is_empty() {
        return Err(format!("분류표에 없는 이름이 {}개다: {}. secret-names.txt 에 secret 인지 config 인지 적어야 한다",unknown.len(),unknown.join(",")));
    }
    let names = names.iter().filter(|n| classes[*n]).collect::<Vec<_>>();
    if names.is_empty() {
        return Err("비밀로 분류된 이름이 하나도 없다. 분류표가 어긋났다".into());
    }
    let mut v = Values {
        values: vec![],
        asked: names.len(),
        read: 0,
        short: vec![],
        unreadable: vec![],
    };
    for n in names {
        if let Some(value) = env.get(n).filter(|v| !v.is_empty()) {
            v.read += 1;
            if value.chars().count() < 8 {
                v.short.push(n.clone())
            } else if !v.values.contains(value) {
                v.values.push(value.clone());
            }
        } else {
            v.unreadable.push(n.clone())
        }
    }
    if v.read == 0 {
        return Err(format!(
            "비밀로 분류된 이름 {}개 중 값을 받은 것이 0개다. Keychain 이 잠겼을 수 있다",
            v.asked
        ));
    }
    Ok(v)
}
fn probe(text: &str) -> Result<Values, String> {
    let home = std::env::var_os("HOME").ok_or("HOME 을 못 읽었다")?;
    let rc = std::path::Path::new(&home).join(".zshrc");
    let source =
        std::fs::read_to_string(&rc).map_err(|e| format!("{} 를 못 읽었다: {e}", rc.display()))?;
    let names = names(&source)?;
    let classes = classes(text).map_err(|e| format!("분류표가 어긋났다: {e}"))?;
    let bytes = login_env(Duration::from_secs(30))?;
    build(&names, &classes, &environment(&bytes)?)
}
// The historical probe only stops waiting at 30 seconds. A background owner still reaps
// the child; no output, exit diagnostics, or environment values enter logs or errors.
fn login_env(timeout: Duration) -> Result<Vec<u8>, String> {
    let mut child = Command::new("/bin/zsh")
        .args(["-l", "-i", "-c", "env -0"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("프로브를 못 띄웠다: {e}"))?;
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let out = std::thread::spawn(move || {
            let mut b = vec![];
            let _ = stdout.read_to_end(&mut b);
            b
        });
        let err = std::thread::spawn(move || {
            let _ = std::io::copy(&mut stderr, &mut std::io::sink());
        });
        let result = child
            .wait()
            .map_err(|e| format!("프로브를 못 띄웠다: {e}"))
            .and_then(|s| {
                if s.success() {
                    Ok(())
                } else {
                    Err(format!(
                        "프로브가 종료 코드 {} 로 끝났다",
                        s.code()
                            .map(|c| c.to_string())
                            .unwrap_or_else(|| "없음".into())
                    ))
                }
            });
        let bytes = out.join().unwrap_or_default();
        let _ = err.join();
        let _ = tx.send(result.map(|_| bytes));
    });
    rx.recv_timeout(timeout)
        .map_err(|_| "프로브가 30초 안에 안 끝났다".to_owned())?
}
fn install(table: &mut Table, result: Result<Values, String>) {
    match result {
        Ok(mut new) => {
            if let Table::Ready(old) = table {
                let mut values = std::mem::take(&mut old.values);
                for v in new.values {
                    if !values.contains(&v) {
                        values.push(v)
                    }
                }
                new.values = values;
            }
            *table = Table::Ready(new)
        }
        Err(e) => *table = Table::Unavailable(e),
    }
}
#[tauri::command]
pub async fn secret_table_probe(state: State<'_, SecretState>) -> Result<Status, String> {
    let text = state.text;
    let result = tauri::async_runtime::spawn_blocking(move || probe(text))
        .await
        .map_err(|e| format!("표 뜨기가 끊겼다: {e}"));
    let mut table = state.table.lock().unwrap();
    install(&mut table, result.unwrap_or_else(Err));
    Ok(status(&table))
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Masked {
    text: String,
    hits: usize,
    masked_values: usize,
    table_size: usize,
    short: Vec<String>,
    unreadable: Vec<String>,
    shapes: Vec<Shape>,
}
fn mask(table: &Table, lines: Vec<String>) -> Result<Masked, String> {
    let v = match table {
        Table::Empty => return Err("표를 아직 안 떴다. 굳히기 전에 표를 떠야 한다".into()),
        Table::Unavailable(e) => return Err(e.clone()),
        Table::Ready(v) => v,
    };
    let mut text = lines.join("\n");
    let mut values = v.values.iter().collect::<Vec<_>>();
    values.sort_by_key(|s| std::cmp::Reverse(s.chars().count()));
    let (mut hits, mut masked_values) = (0, 0);
    for value in values {
        let n = text.matches(value.as_str()).count();
        if n > 0 {
            hits += n;
            masked_values += 1;
            text = text.replace(value.as_str(), &format!("⟨비밀{masked_values}⟩"));
        }
    }
    let shapes = shapes(&text);
    Ok(Masked {
        text,
        hits,
        masked_values,
        table_size: v.values.len(),
        short: v.short.clone(),
        unreadable: v.unreadable.clone(),
        shapes,
    })
}
#[tauri::command]
pub fn mask_scrollback(
    state: State<'_, SecretState>,
    lines: Vec<String>,
) -> Result<Masked, String> {
    mask(&state.table.lock().unwrap(), lines)
}
#[derive(Debug, Serialize, PartialEq)]
pub struct Shape {
    kind: &'static str,
    label: &'static str,
    count: usize,
}
const KINDS: [(&str, &str); 10] = [
    ("aws-access-key", "AWS 액세스 키"),
    ("github-token", "GitHub 토큰"),
    ("slack-token", "Slack 토큰"),
    ("anthropic-key", "Anthropic 키"),
    ("google-api-key", "Google API 키"),
    ("npm-token", "npm 토큰"),
    ("generic-sk-key", "sk- 로 시작하는 키"),
    ("jwt", "JWT"),
    ("pem-private-key", "PEM 개인키"),
    ("url-credential", "URL 에 박힌 자격증명"),
];
fn body(b: u8, kind: u8) -> bool {
    match kind {
        0 => b.is_ascii_uppercase() || b.is_ascii_digit(),
        1 => b.is_ascii_alphanumeric(),
        _ => b.is_ascii_alphanumeric() || b == b'_' || b == b'-',
    }
}
fn pem(b: &[u8]) -> Option<usize> {
    let rest = b.strip_prefix(b"-----BEGIN ")?;
    for n in 0..=30 {
        if rest.get(n..)?.starts_with(b"PRIVATE KEY-----") {
            return Some(11 + n + 16);
        }
        if !rest
            .get(n)
            .is_some_and(|b| b.is_ascii_uppercase() || *b == b' ')
        {
            break;
        }
    }
    None
}
fn jwt(b: &[u8]) -> Option<usize> {
    if !b.starts_with(b"eyJ") {
        return None;
    }
    let mut i = 3;
    for part in 0..3 {
        let start = i;
        while i < b.len() && body(b[i], 2) {
            i += 1
        }
        if i - start < 8 {
            return None;
        }
        if part < 2 {
            if b.get(i) != Some(&b'.') {
                return None;
            }
            i += 1
        }
    }
    Some(i)
}
fn url(b: &[u8]) -> Option<usize> {
    if !b.first()?.is_ascii_lowercase() {
        return None;
    }
    let mut i = 1;
    while i < b.len()
        && (b[i].is_ascii_lowercase() || b[i].is_ascii_digit() || b"+.-".contains(&b[i]))
    {
        i += 1
    }
    if !(2..=16).contains(&i) || !b.get(i..)?.starts_with(b"://") {
        return None;
    }
    i += 3;
    let blocked = |i: usize, user: bool| {
        b.get(i)
            .is_none_or(|c| b"/@\"'\t\n\x0c\r ".contains(c) || (user && *c == b':'))
            || b.get(i..)
                .is_some_and(|s| s.starts_with("⟨비밀".as_bytes()))
    };
    let start = i;
    while !blocked(i, true) && i - start <= 64 {
        i += 1
    }
    if !(1..=64).contains(&(i - start)) || b.get(i) != Some(&b':') {
        return None;
    }
    i += 1;
    let start = i;
    while !blocked(i, false) && i - start <= 120 {
        i += 1
    }
    if !(1..=120).contains(&(i - start)) || b.get(i) != Some(&b'@') {
        return None;
    }
    Some(i + 1)
}
pub fn shapes(text: &str) -> Vec<Shape> {
    let b = text.as_bytes();
    let mut counts = [0; 10];
    let mut i = 0;
    while i < b.len() {
        if let Some(n) = pem(&b[i..]) {
            counts[8] += 1;
            i += n;
            continue;
        }
        if i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_' || b[i - 1] >= 128) {
            i += 1;
            continue;
        }
        let mut found = None;
        for (prefix, kind, min, index) in [
            ("AKIA", 0, 16, 0),
            ("ASIA", 0, 16, 0),
            ("ghp_", 1, 36, 1),
            ("gho_", 1, 36, 1),
            ("ghu_", 1, 36, 1),
            ("ghs_", 1, 36, 1),
            ("ghr_", 1, 36, 1),
            ("xoxb-", 2, 10, 2),
            ("xoxa-", 2, 10, 2),
            ("xoxp-", 2, 10, 2),
            ("xoxr-", 2, 10, 2),
            ("xoxs-", 2, 10, 2),
            ("sk-ant-", 2, 20, 3),
            ("AIza", 2, 35, 4),
            ("npm_", 1, 36, 5),
            ("sk-", 1, 32, 6),
        ] {
            if b[i..].starts_with(prefix.as_bytes()) {
                let start = i + prefix.len();
                let n = b[start..].iter().take_while(|c| body(**c, kind)).count();
                if n >= min {
                    found = Some((index, prefix.len() + n));
                    break;
                }
            }
        }
        if found.is_none() {
            found = jwt(&b[i..])
                .map(|n| (7, n))
                .or_else(|| url(&b[i..]).map(|n| (9, n)));
        }
        if let Some((kind, n)) = found {
            counts[kind] += 1;
            i += n
        } else {
            i += 1
        }
    }
    KINDS
        .iter()
        .enumerate()
        .filter(|(i, _)| counts[*i] > 0)
        .map(|(i, (kind, label))| Shape {
            kind,
            label,
            count: counts[i],
        })
        .collect()
}
#[tauri::command]
pub fn scan_text_shapes(text: String) -> Vec<Shape> {
    shapes(&text)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fail_closed_parser_classification_and_nul_environment() {
        assert!(names("export X=y").is_err());
        let ns=names("# === KCD secrets END\n# === KCD secrets BEGIN\nexport EXAMPLE_A=x\nexport EXAMPLE_A=x\n").unwrap();
        assert_eq!(ns.len(), 2);
        assert!(names("# === KCD secrets BEGIN\nexport lower=x\n# === KCD secrets END").is_err());
        assert!(classes("A wrong").is_err());
        let env =
            environment(b"EXAMPLE_A=fakevalue123\0EXAMPLE_A=ignored\0bad\0BAD=\xff\0").unwrap();
        let c = classes("EXAMPLE_A secret\nEXAMPLE_A config").unwrap();
        let v = build(&ns, &c, &env).unwrap();
        assert_eq!(v.read, 2);
        assert_eq!(v.values.len(), 1);
        assert!(build(&["MISSING".into()], &c, &env).is_err());
    }
    #[test]
    fn mask_longest_first_multiline_and_union_rotations() {
        let mut t = Table::Empty;
        assert!(mask(&t, vec![]).is_err());
        let value = |s: &str| Values {
            values: vec![s.into()],
            asked: 1,
            read: 1,
            short: vec![],
            unreadable: vec![],
        };
        install(&mut t, Ok(value("longfake\nvalue")));
        install(&mut t, Ok(value("longfake")));
        let m = mask(&t, vec!["longfake".into(), "value longfake".into()]).unwrap();
        assert_eq!(m.text, "⟨비밀1⟩ ⟨비밀2⟩");
        assert_eq!(m.hits, 2);
        assert_eq!(m.table_size, 2);
        install(&mut t, Err("unavailable".into()));
        assert!(!status(&t).ready);
        assert!(mask(&t, vec![]).is_err());
    }
    #[test]
    fn shapes_are_count_only_ordered_and_skip_masked_credentials() {
        let input = format!(
            "-----BEGIN RSA PRIVATE KEY----- redis://u:pw@h https://user:pass@h\n{} {} {}",
            format!("AKIA{}", "A".repeat(16)),
            format!("ghp_{}", "b".repeat(36)),
            format!("eyJ{}.{}.{}", "a".repeat(8), "b".repeat(8), "c".repeat(8))
        );
        let s = shapes(&input);
        assert_eq!(
            s.iter().map(|s| s.kind).collect::<Vec<_>>(),
            vec![
                "aws-access-key",
                "github-token",
                "jwt",
                "pem-private-key",
                "url-credential"
            ]
        );
        assert_eq!(s.last().unwrap().count, 2);
        assert!(shapes("redis://u:⟨비밀1⟩@h harmless nd1234 hash1234").is_empty());
        assert!(shapes(&format!("한AKIA{}", "A".repeat(16))).is_empty());
        assert!(shapes("-----END RSA PRIVATE KEY-----").is_empty());
    }
    #[test]
    fn sample_table_contains_no_values() {
        let s = SecretState::default();
        if s.source == "sample" {
            assert!(s
                .text
                .lines()
                .all(|l| l.trim().is_empty() || l.starts_with('#')));
        }
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    #[test]
    fn all_prefix_thresholds_and_word_boundaries() {
        for (prefix, min, kind) in [
            ("AKIA", 16, "aws-access-key"),
            ("ASIA", 16, "aws-access-key"),
            ("ghp_", 36, "github-token"),
            ("gho_", 36, "github-token"),
            ("ghu_", 36, "github-token"),
            ("ghs_", 36, "github-token"),
            ("ghr_", 36, "github-token"),
            ("xoxb-", 10, "slack-token"),
            ("xoxa-", 10, "slack-token"),
            ("xoxp-", 10, "slack-token"),
            ("xoxr-", 10, "slack-token"),
            ("xoxs-", 10, "slack-token"),
            ("sk-ant-", 20, "anthropic-key"),
            ("AIza", 35, "google-api-key"),
            ("npm_", 36, "npm-token"),
            ("sk-", 32, "generic-sk-key"),
        ] {
            let s = format!("{prefix}{}", "A".repeat(min));
            let out = shapes(&s);
            assert_eq!(out.len(), 1, "{prefix}");
            assert_eq!(out[0].kind, kind);
            assert!(shapes(&format!("{prefix}{}", "A".repeat(min - 1))).is_empty());
            assert!(shapes(&format!("_{s}")).is_empty());
        }
    }
    #[test]
    fn urls_pem_and_jwt_have_bounded_grammar() {
        assert!(shapes("x://u:p@h").is_empty());
        assert_eq!(shapes("ab://u:p@h")[0].count, 1);
        assert!(shapes(&format!("ab://{}:p@h", "u".repeat(65))).is_empty());
        assert!(shapes(&format!("ab://u:{}@h", "p".repeat(121))).is_empty());
        assert!(shapes("ab://⟨비밀1⟩:p@h").is_empty());
        assert!(shapes("ab://u:⟨비밀1⟩@h").is_empty());
        assert!(shapes("-----BEGIN rsa PRIVATE KEY-----").is_empty());
        assert!(shapes("eyJaaaaaaa.bbbbbbbb.cccccccc").is_empty());
    }
    #[test]
    fn short_values_unreadable_and_status_never_serialize_values() {
        let ns = vec![
            "EXAMPLE_LONG".into(),
            "EXAMPLE_SHORT".into(),
            "EXAMPLE_MISSING".into(),
        ];
        let c =
            classes("EXAMPLE_LONG secret\nEXAMPLE_SHORT secret\nEXAMPLE_MISSING secret").unwrap();
        let env =
            environment("EXAMPLE_LONG=fictional-value-123\0EXAMPLE_SHORT=비밀값\0".as_bytes())
                .unwrap();
        let table = Table::Ready(build(&ns, &c, &env).unwrap());
        let status = status(&table);
        assert!(status.ready);
        assert_eq!(status.asked, 3);
        assert_eq!(status.read, 2);
        assert_eq!(status.maskable, 1);
        assert_eq!(status.short, vec!["EXAMPLE_SHORT"]);
        assert_eq!(status.unreadable, vec!["EXAMPLE_MISSING"]);
        let json = serde_json::to_string(&status).unwrap();
        assert!(!json.contains("fictional-value-123"));
        assert!(json.contains("\"reason\":null"));
        let masked = mask(&table, vec!["fictional-value-123".into()]).unwrap();
        assert!(!serde_json::to_string(&masked)
            .unwrap()
            .contains("fictional-value-123"));
    }
}
