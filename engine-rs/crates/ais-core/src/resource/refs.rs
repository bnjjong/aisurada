//! 프로젝트의 참조 목록(켜 둔 글로벌 자원). 외부 설명서에 없는 확장이다.
//!
//! `.aisurada/resources/projects/<slug>/refs.md`에 줄마다 `<ID>  # <이름 힌트>`로 적는다. 엔진은 ID만 읽고 힌트는 읽는 사람을 위한 것이다
//! (이름이 바뀌어도 ID가 같으면 같은 자원이다). 프로젝트 파일에 적지 않는 까닭은, 설명서가 프로젝트 파일의 줄을 정해 두었고 그 파일을 모르는
//! 옛 빌드가 다시 쓸 때 모르는 줄을 버리기 때문이다.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use super::{ResourceKind, approvals, store};
use crate::error::EngineError;
use crate::kernel::java::java_trim;
use crate::vault::note_file::compute_token;
use crate::write::atomic::write_atomic;

/// 목록 한 줄. `hint`는 쓴 때의 이름이다.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: String,
    pub hint: String,
}

// Java 정규식의 `\S`, `\s`, `.`와 같게 ASCII 공백과 줄 끝 글자만으로 가른다
static LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([^ \t\n\x0B\x0C\r]+)(?:[ \t\n\x0B\x0C\r]+#[ \t\n\x0B\x0C\r]*([^\n\r\x{85}\x{2028}\x{2029}]*))?$").unwrap()
});
const HEADER: &str =
    "# 이 프로젝트가 쓰려고 참조한 글로벌 자원. 줄마다 `ID  # 이름 힌트`. 엔진은 ID만 읽는다.\n";

pub fn file(vault: &Path, project: &str) -> PathBuf {
    store::scope_dir(vault, Some(project)).join("refs.md")
}

/// 참조 목록을 너그럽게 읽는다. ID 모양이 아닌 줄은 건너뛰고, 겹친 ID는 처음 것만 둔다. 파일의 지금 모습 그대로이고 승인 여부는 보지 않는다.
pub fn read(vault: &Path, project: &str) -> Vec<Entry> {
    let bytes = live_bytes(vault, project);
    if bytes.is_empty() {
        Vec::new()
    } else {
        parse(&String::from_utf8_lossy(&bytes))
    }
}

/// 참조 목록 글을 읽는다.
pub fn parse(text: &str) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    for raw in text.split('\n') {
        let line = java_trim(raw);
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(m) = LINE.captures(line) else {
            continue;
        };
        let id = &m[1];
        if !ResourceKind::is_id(id) || out.iter().any(|e| e.id == id) {
            continue;
        }
        out.push(Entry {
            id: id.to_string(),
            hint: m
                .get(2)
                .map_or_else(String::new, |h| java_trim(h.as_str()).to_string()),
        });
    }
    out
}

/// 파일의 지금 바이트. 파일이 없거나 못 읽으면 빈 배열이다.
pub fn live_bytes(vault: &Path, project: &str) -> Vec<u8> {
    let f = file(vault, project);
    if fs::metadata(&f).map(|m| m.is_file()).unwrap_or(false) {
        fs::read(&f).unwrap_or_default()
    } else {
        Vec::new()
    }
}

/// 참조 목록 파일이 있는 프로젝트들의 slug(승인 기준선을 만들 때 쓴다).
pub fn projects_with_refs_file(vault: &Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let projects = store::root(vault).join("projects");
    for p in store::sorted_entries(&projects) {
        let name = p
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        if !name.starts_with('.') && store::is_dir(&p) && store::is_file(&p.join("refs.md")) {
            out.push(name);
        }
    }
    out.sort_by(|a, b| crate::text::compare_utf16(a, b));
    out
}

/// 참조 목록의 승인 상태
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    /// 승인한 판과 같다(또는 승인 기록도 파일도 없다).
    Approved,
    /// 승인한 뒤에 파일이 바뀌었다. 승인해 둔 사본을 쓴다.
    Dirty,
    /// 승인 기록이 없는데 파일이 있다. 쓰지 않는다(빈 목록으로 본다).
    Untrusted,
}

pub fn state(vault: &Path, project: &str) -> Result<State, EngineError> {
    let live = live_bytes(vault, project);
    let ledger = approvals::ledger(vault)?;
    Ok(match ledger.get(&approvals::refs_key(project)) {
        None if live.is_empty() => State::Approved,
        None => State::Untrusted,
        Some(e) if compute_token(&live) == e.token => State::Approved,
        Some(_) => State::Dirty,
    })
}

/// 승인한 판의 참조 목록. 실행과 목록이 쓰는 것은 이쪽이다. 직접 고친 파일은 승인해 둔 사본으로 읽고, 기록 없는 새 파일은 비어 있는 것으로 본다.
pub fn read_approved(vault: &Path, project: &str) -> Result<Vec<Entry>, EngineError> {
    Ok(match state(vault, project)? {
        State::Approved => read(vault, project),
        State::Dirty => match approvals::copy(vault, &approvals::refs_key(project)) {
            Some(copy) => parse(&String::from_utf8_lossy(&copy)),
            None => Vec::new(),
        },
        State::Untrusted => Vec::new(),
    })
}

/// 참조 목록을 쓴다. 비면 파일을 지운다.
pub fn write(vault: &Path, project: &str, entries: &[Entry]) -> Result<(), EngineError> {
    let f = file(vault, project);
    if entries.is_empty() {
        return remove_file(vault, &f);
    }
    let mut text = String::from(HEADER);
    for e in entries {
        text.push_str(&e.id);
        let hint = e.hint.replace(['\n', '\r'], " ");
        let hint = java_trim(&hint);
        if !hint.is_empty() {
            text.push_str("  # ");
            text.push_str(hint);
        }
        text.push('\n');
    }
    write_atomic(&f, text.as_bytes())
}

/// 참조 목록 파일을 지운다. 없으면 아무것도 안 한다.
pub fn remove_file(vault: &Path, f: &Path) -> Result<(), EngineError> {
    match fs::remove_file(f) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(EngineError::internal(format!(
            "참조 목록을 지우지 못했다: {}",
            store::rel(vault, f)
        ))),
    }
}

/// 이 ID를 참조한 프로젝트들의 slug.
pub fn projects_referencing(vault: &Path, id: &str) -> Vec<String> {
    let projects = store::root(vault).join("projects");
    let mut names: Vec<String> = store::sorted_entries(&projects)
        .iter()
        .filter(|p| store::is_dir(p))
        .map(|p| {
            p.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|n| !n.starts_with('.'))
        .collect();
    names.sort_by(|a, b| crate::text::compare_utf16(a, b));
    names
        .into_iter()
        .filter(|n| read(vault, n).iter().any(|e| e.id == id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_ids_and_hints_and_skips_bad_and_repeated_lines() {
        let text = "# 머리\n\nsk1abc-0123abcd  # 이름 힌트 \nap9-deadbeef\nnot-an-id # x\nsk1abc-0123abcd  # 또\nsk2-0123abcd\t#\n";
        let entries = parse(text);
        assert_eq!(
            entries,
            vec![
                Entry {
                    id: "sk1abc-0123abcd".into(),
                    hint: "이름 힌트".into()
                },
                Entry {
                    id: "ap9-deadbeef".into(),
                    hint: String::new()
                },
                Entry {
                    id: "sk2-0123abcd".into(),
                    hint: String::new()
                },
            ]
        );
    }
}
