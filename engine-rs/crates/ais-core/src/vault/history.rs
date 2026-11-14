//! 노트의 판(version) 목록: 경로 이력, 커밋 꼬리 줄, 옛 스냅숏 참조를 한데 모읍니다.
//!
//! 판은 vault의 git 저장소(`main` 브랜치)에서 찾습니다. git이 실패하면 그 걸음은 빈 결과로 보고 다음으로 넘어갑니다.
//! 저장소가 없는 vault는 판이 하나도 없습니다.

use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use super::git::Git;
use super::note_file::split_text;
use super::note_meta::{civil_from_days, read_agent, text as text_of};
use crate::frontmatter::Value;
use crate::text::{is_blank, trim, utf16_len};

/// 판 시각을 보여 줄 때 쓰는 시간대(+09:00)의 초 단위 어긋남
const TAKEN_AT_OFFSET_SECONDS: i64 = 9 * 3600;
/// Java 시각 표현이 다루는 초의 범위(`Instant`)
const INSTANT_LIMIT: i64 = 31_556_889_864_403_199;
const TRAILER: &str = "Aisurada-Notes:";

static HEADER_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[\p{L}\p{N}_-]+$").unwrap());

/// 노트 한 판이 어느 커밋의 어느 경로에 있는지
#[derive(Debug, Clone)]
pub struct Version {
    pub commit: String,
    pub seconds: i64,
    /// `auto`(앱이 자동으로 남긴 커밋), `manual`, `snapshot`(옛 스냅숏 참조)
    pub source: &'static str,
    pub path: String,
    pub bytes: i64,
    /// 파일 내용의 git 객체 이름. 같은 내용을 접을 때 쓴다.
    pub blob: Option<String>,
}

/// 한 판의 내용
pub struct Content {
    /// 파일 전체 글
    pub full: String,
    /// 머리를 뗀 본문. 머리를 못 읽었으면 파일 전체 글이다.
    pub body: String,
    pub frontmatter: Option<Vec<(Value, Value)>>,
}

/// 판 목록의 한 줄
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionRow {
    pub index: usize,
    pub taken_at: String,
    pub source: &'static str,
    pub commit: String,
    pub path: String,
    pub bytes: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_session_agent: Option<String>,
}

pub struct VaultHistory {
    vault: PathBuf,
    git: Git,
    legacy: Git,
}

impl VaultHistory {
    pub fn new(vault: &Path) -> Self {
        VaultHistory {
            vault: vault.to_path_buf(),
            git: Git::strict(vault),
            legacy: Git::legacy(vault),
        }
    }

    /// `id` 노트의 판을 최근 것부터 돌려준다. 안전하지 않은 id이거나 저장소가 없으면 빈 목록이다.
    pub fn versions(&self, id: &str) -> Vec<Version> {
        if !safe_id(id) || !self.vault.join(".git").is_dir() {
            return Vec::new();
        }
        let mut versions = self.path_history(id);
        let found: HashSet<String> = versions.iter().map(|v| v.commit.clone()).collect();
        versions.extend(self.trailer_commits(id, &found));

        // 같은 커밋과 경로는 처음 모은 하나만 남긴다
        let mut seen = HashSet::new();
        versions.retain(|v| seen.insert((v.commit.clone(), v.path.clone())));
        versions.extend(self.snapshots(id));
        self.fold(versions)
    }

    /// 판들의 내용을 읽는다. 읽지 못한 판은 `None`이다.
    pub fn read(&self, versions: &[Version]) -> Vec<Option<Content>> {
        self.read_bytes(versions)
            .into_iter()
            .map(|bytes| {
                bytes.map(|bytes| {
                    let full = String::from_utf8_lossy(&bytes).into_owned();
                    let split = split_text(&full);
                    let body = if split.frontmatter.is_some() {
                        split.body
                    } else {
                        full.clone()
                    };
                    Content {
                        full,
                        body,
                        frontmatter: split.frontmatter,
                    }
                })
            })
            .collect()
    }

    /// 판 목록의 한 줄. 그 판의 머리에 편집 정보가 있으면 함께 싣는다.
    pub fn row(version: &Version, index: usize, content: Option<&Content>) -> VersionRow {
        let mut row = VersionRow {
            index,
            taken_at: format_taken_at(version.seconds),
            source: version.source,
            commit: version.commit.clone(),
            path: version.path.clone(),
            bytes: version.bytes,
            edited_by: None,
            edited_session_id: None,
            edited_session_agent: None,
        };
        let Some(map) = content.and_then(|c| c.frontmatter.as_deref()) else {
            return row;
        };
        let edited_by = text_of(map, "editedBy").filter(|s| !is_blank(s));
        let edited_at = text_of(map, "editedAt").filter(|s| !is_blank(s));
        if let (Some(by), Some(_)) = (edited_by, edited_at) {
            row.edited_by = Some(trim(by).to_string());
            if let Some(session) = text_of(map, "editedSessionId").filter(|s| !is_blank(s)) {
                row.edited_session_id = Some(trim(session).to_string());
                row.edited_session_agent =
                    read_agent(super::note_meta::get(map, "editedSessionAgent"));
            }
        }
        row
    }

    /// 1단계: `note.md`로 끝나는 파일 가운데 이 id가 든 것의 경로 이력
    fn path_history(&self, id: &str) -> Vec<Version> {
        let mut versions = Vec::new();
        let paths = self
            .git
            .run(&["grep", "-l", "-z", "-F", "-e", id, "--", "*note.md"], &[]);
        if paths.code != 0 {
            return versions;
        }
        for path in paths.text().split('\0') {
            if !safe_path(path) || !self.header_matches(path, id) {
                continue;
            }
            let log = self.git.run(
                &[
                    "log",
                    "main",
                    "--follow",
                    "--format=%x1e%H%x1f%ct%x1f%an",
                    "--name-only",
                    "--",
                    path,
                ],
                &[],
            );
            if log.code != 0 {
                continue;
            }
            for record in log.text().split('\u{1e}') {
                let lines: Vec<&str> = java_lines(record)
                    .into_iter()
                    .filter(|s| !is_blank(s))
                    .collect();
                if lines.len() < 2 {
                    continue;
                }
                let fields: Vec<&str> = lines[0].split('\u{1f}').collect();
                if let Some(version) = entry(&fields, trim(lines[1])) {
                    versions.push(version);
                }
            }
        }
        versions
    }

    /// 작업 트리의 파일 머리 4096바이트에서 읽은 id가 같은가
    fn header_matches(&self, path: &str, id: &str) -> bool {
        let Ok(file) = File::open(self.vault.join(path)) else {
            return false;
        };
        let mut head = Vec::new();
        if file.take(4096).read_to_end(&mut head).is_err() {
            return false;
        }
        header_id(&head).as_deref() == Some(id)
    }

    /// 2단계: 파일 이력에는 없지만 커밋 메시지 꼬리 줄(`Aisurada-Notes: id id …`)이 이 노트를 가리키는 커밋
    fn trailer_commits(&self, id: &str, found: &HashSet<String>) -> Vec<Version> {
        let mut versions = Vec::new();
        let grep = format!("--grep={id}");
        let trailers = self.git.run(
            &[
                "log",
                "main",
                "--format=%x1e%H%x1f%ct%x1f%an%x1f%B",
                "-F",
                &grep,
            ],
            &[],
        );
        if trailers.code != 0 {
            return versions;
        }
        for record in trailers.text().split('\u{1e}') {
            let fields: Vec<&str> = record.splitn(4, '\u{1f}').collect();
            if fields.len() != 4 || found.contains(fields[0]) || !mentions(fields[3], id) {
                continue;
            }
            let Some(version) = entry(&fields, "") else {
                continue;
            };
            let diff = self.git.run(
                &[
                    "diff-tree",
                    "-r",
                    "--root",
                    "--no-commit-id",
                    "--name-only",
                    "-z",
                    &version.commit,
                ],
                &[],
            );
            if diff.code != 0 {
                continue;
            }
            let options: Vec<Version> = diff
                .text()
                .split('\0')
                .filter(|path| safe_path(path) && path.rsplit('/').next() == Some("note.md"))
                .map(|path| Version {
                    path: path.to_string(),
                    ..version.clone()
                })
                .collect();
            let bodies = self.read_bytes(&options);
            if let Some(found) = options
                .iter()
                .zip(&bodies)
                .find(|(_, body)| body.as_deref().and_then(header_id).as_deref() == Some(id))
            {
                versions.push(found.0.clone());
            }
        }
        versions
    }

    /// 3단계: 옛 스냅숏 참조(`refs/aisurada/snapshots/<id>`)의 커밋들
    fn snapshots(&self, id: &str) -> Vec<Version> {
        let mut result = Vec::new();
        let reference = format!("refs/aisurada/snapshots/{id}");
        let log = self
            .legacy
            .run(&["log", "--format=%H %ct", &reference], &[]);
        if log.code != 0 {
            return result;
        }
        for line in java_lines(&log.text()) {
            let fields: Vec<&str> = trim(line).split(' ').collect();
            if fields.len() != 2 || !commit_shape(fields[0]) {
                continue;
            }
            let Some(seconds) = parse_seconds(fields[1]) else {
                continue;
            };
            let tree = self.legacy.run(&["ls-tree", "-l", fields[0]], &[]);
            if tree.code != 0 {
                continue;
            }
            let text = tree.text();
            let first = java_lines(&text)
                .into_iter()
                .find(|s| !is_blank(s))
                .unwrap_or("");
            let mut pair = first.splitn(2, '\t');
            let (Some(meta), Some(name)) = (pair.next(), pair.next()) else {
                continue;
            };
            let Some(size) = meta.rsplit(' ').next().and_then(|s| s.parse::<i64>().ok()) else {
                continue;
            };
            result.push(Version {
                commit: fields[0].to_string(),
                seconds,
                source: "snapshot",
                path: trim(name).to_string(),
                bytes: size,
                blob: None,
            });
        }
        result
    }

    /// 실제로 읽히는 판만 남기고 시각 내림차순으로 놓은 뒤, 바로 앞 판과 내용이 같은 판은 접는다.
    fn fold(&self, versions: Vec<Version>) -> Vec<Version> {
        if versions.is_empty() {
            return Vec::new();
        }
        let specs: Vec<String> = versions.iter().map(spec).collect();
        let check = self
            .git
            .run(&["cat-file", "--batch-check"], &batch_input(&specs));
        if check.code != 0 {
            return Vec::new();
        }
        let text = check.text();
        let lines: Vec<&str> = java_lines(&text)
            .into_iter()
            .filter(|s| !is_blank(s))
            .collect();
        let mut valid = Vec::new();
        for (line, version) in lines.iter().zip(&versions) {
            let fields: Vec<&str> = line.split(' ').collect();
            if fields.len() != 3 || !commit_shape(fields[0]) || fields[1] != "blob" {
                continue;
            }
            let Ok(bytes) = fields[2].parse::<i64>() else {
                continue;
            };
            if bytes < 0 {
                continue;
            }
            valid.push(Version {
                bytes,
                blob: Some(fields[0].to_string()),
                ..version.clone()
            });
        }
        // 같은 시각이면 모은 차례를 지킨다(안정 정렬)
        valid.sort_by_key(|a| std::cmp::Reverse(a.seconds));
        let mut folded: Vec<Version> = Vec::new();
        for version in valid {
            if folded.last().is_none_or(|last| last.blob != version.blob) {
                folded.push(version);
            }
        }
        folded
    }

    fn read_bytes(&self, versions: &[Version]) -> Vec<Option<Vec<u8>>> {
        let mut result: Vec<Option<Vec<u8>>> = vec![None; versions.len()];
        let mut readable = Vec::new();
        let mut specs = Vec::new();
        for (i, version) in versions.iter().enumerate() {
            if commit_shape(&version.commit)
                && safe_path(&version.path)
                && version.bytes <= i64::from(i32::MAX)
            {
                readable.push(i);
                specs.push(spec(version));
            }
        }
        if specs.is_empty() {
            return result;
        }
        let batch = self.git.run(&["cat-file", "--batch"], &batch_input(&specs));
        if batch.code != 0 {
            return result;
        }
        for (i, body) in readable
            .into_iter()
            .zip(read_batch(specs.len(), &batch.output))
        {
            result[i] = body;
        }
        result
    }
}

/// 커밋 이름 모양: 소문자 16진수 40자
pub fn commit_shape(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|b| matches!(b, b'a'..=b'f' | b'0'..=b'9'))
}

fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn safe_path(path: &str) -> bool {
    !(path.is_empty()
        || utf16_len(path) > 1024
        || path.starts_with('/')
        || path.starts_with('-')
        || path.contains('\0')
        || path.contains('\n')
        || path.split('/').any(|part| part == ".."))
}

/// 파일 앞부분 바이트에서 머리의 `id:` 줄을 읽는다. 머리가 없거나 id 모양이 아니면 `None`.
fn header_id(bytes: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(bytes);
    if !text.starts_with("---") {
        return None;
    }
    let end = 3 + text[3..].find("\n---")?;
    for line in text[3..end].split('\n') {
        let Some(rest) = line.strip_prefix("id:") else {
            continue;
        };
        let id = trim(rest).trim_matches(['\'', '"']);
        return (utf16_len(id) <= 128 && HEADER_ID.is_match(id)).then(|| id.to_string());
    }
    None
}

/// 로그 한 건(`커밋 \x1f 시각 \x1f 작성자`)을 판으로 바꾼다.
fn entry(fields: &[&str], path: &str) -> Option<Version> {
    if fields.len() < 3 || !commit_shape(fields[0]) {
        return None;
    }
    let seconds = parse_seconds(fields[1])?;
    Some(Version {
        commit: fields[0].to_string(),
        seconds,
        source: if trim(fields[2]) == "aisurada-auto" {
            "auto"
        } else {
            "manual"
        },
        path: path.to_string(),
        bytes: 0,
        blob: None,
    })
}

fn parse_seconds(text: &str) -> Option<i64> {
    text.parse::<i64>()
        .ok()
        .filter(|s| (-INSTANT_LIMIT..=INSTANT_LIMIT).contains(s))
}

/// 커밋 메시지의 꼬리 줄 `Aisurada-Notes:`가 이 id를 가리키는가
fn mentions(message: &str, id: &str) -> bool {
    java_lines(message)
        .into_iter()
        .filter_map(|line| line.strip_prefix(TRAILER))
        .any(|rest| trim(rest).split(' ').any(|name| name == id))
}

fn spec(version: &Version) -> String {
    format!("{}:{}", version.commit, version.path)
}

fn batch_input(specs: &[String]) -> Vec<u8> {
    format!("{}\n", specs.join("\n")).into_bytes()
}

/// `git cat-file --batch` 응답을 본문마다 자른다. 본문의 줄바꿈이 아니라 머리의 바이트 수를 따른다.
fn read_batch(count: usize, output: &[u8]) -> Vec<Option<Vec<u8>>> {
    let mut bodies: Vec<Option<Vec<u8>>> = Vec::new();
    let mut offset = 0;
    for _ in 0..count {
        let mut end = offset;
        while end < output.len() && output[end] != b'\n' {
            end += 1;
        }
        if end == output.len() {
            break;
        }
        let header_text = String::from_utf8_lossy(&output[offset..end]).into_owned();
        let header: Vec<&str> = header_text.split(' ').collect();
        offset = end + 1;
        let is_blob = header.len() == 3 && header[1] == "blob";
        if !is_blob {
            bodies.push(None);
        }
        if header.len() != 3 {
            continue;
        }
        // blob이 아니어도 크기를 싣고 본문을 내므로 다음 응답 전에 그 바이트를 건너뛴다
        let Some(size) = header[2]
            .parse::<i32>()
            .ok()
            .filter(|s| *s >= 0 && (*s as usize) < output.len() - offset)
            .map(|s| s as usize)
        else {
            break;
        };
        if is_blob {
            bodies.push(Some(output[offset..offset + size].to_vec()));
        }
        offset += size + 1;
    }
    bodies.resize(count, None);
    bodies
}

/// 줄바꿈(`\n`, `\r\n`, `\r`)으로 줄을 나눈다. 마지막에 남은 빈 줄은 없다.
fn java_lines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let bytes = text.as_bytes();
    let (mut start, mut i) = (0, 0);
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => {
                lines.push(&text[start..i]);
                i += 1;
                start = i;
            }
            b'\r' => {
                lines.push(&text[start..i]);
                i += if bytes.get(i + 1) == Some(&b'\n') {
                    2
                } else {
                    1
                };
                start = i;
            }
            _ => i += 1,
        }
    }
    if start < bytes.len() {
        lines.push(&text[start..]);
    }
    lines
}

/// 판 시각을 `yyyy-MM-ddTHH:mm:ss+09:00`으로 보여 준다.
fn format_taken_at(seconds: i64) -> String {
    let local = seconds.saturating_add(TAKEN_AT_OFFSET_SECONDS);
    let (year, month, day) = civil_from_days(local.div_euclid(86_400));
    let rest = local.rem_euclid(86_400);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}+09:00",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes() {
        assert!(commit_shape(&"a1".repeat(20)));
        assert!(!commit_shape(&"A1".repeat(20)));
        assert!(!commit_shape("abc"));
        assert!(safe_id("ndfx0001-00000001") && !safe_id("a b") && !safe_id(""));
        assert!(
            safe_path("notes/a/note.md")
                && !safe_path("../x")
                && !safe_path("/abs")
                && !safe_path("-x")
                && !safe_path("a/../b")
        );
    }

    #[test]
    fn reads_header_id_with_quotes() {
        assert_eq!(
            header_id("---\nid: 'abc-1'\n---\n".as_bytes()).as_deref(),
            Some("abc-1")
        );
        assert_eq!(
            header_id("---\ntitle: x\nid: \"한글_1\"\n---".as_bytes()).as_deref(),
            Some("한글_1")
        );
        assert_eq!(header_id("---\nid: a b\n---".as_bytes()), None);
        assert_eq!(header_id("id: a".as_bytes()), None);
        assert_eq!(header_id("---\nid: a".as_bytes()), None);
    }

    #[test]
    fn formats_taken_at_in_plus_nine() {
        assert_eq!(format_taken_at(0), "1970-01-01T09:00:00+09:00");
        assert_eq!(format_taken_at(1_700_000_000), "2023-11-15T07:13:20+09:00");
    }

    #[test]
    fn cuts_batch_output_by_size() {
        let out = b"aaaa blob 5\nab\ncd\nbbbb missing\ncccc blob 0\n\n";
        let bodies = read_batch(3, out);
        assert_eq!(bodies[0].as_deref(), Some(&b"ab\ncd"[..]));
        assert_eq!(bodies[1], None);
        assert_eq!(bodies[2].as_deref(), Some(&b""[..]));
        // 모자라면 None으로 채운다
        assert_eq!(read_batch(2, b"aaaa blob 1\nx\n").len(), 2);
    }

    #[test]
    fn splits_lines_like_java() {
        assert_eq!(java_lines("a\r\nb\rc\n\nd"), ["a", "b", "c", "", "d"]);
        assert_eq!(java_lines("a\n"), ["a"]);
        assert!(java_lines("").is_empty());
    }

    #[test]
    fn trailer_lines_name_ids() {
        let message = "subject\n\nAisurada-Notes: ndaa-1 ndbb-2\n";
        assert!(mentions(message, "ndbb-2") && !mentions(message, "ndbb"));
        assert!(!mentions("Aisurada-Notes-X: ndaa-1", "ndaa-1"));
    }
}
