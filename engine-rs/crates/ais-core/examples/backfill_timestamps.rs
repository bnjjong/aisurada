//! 프로젝트, 아젠다, 메인·하위 노트에서 비어 있는 시각을 채운다.
//! `backfill_timestamps <vault> [새 백업 폴더]`
//! 백업 폴더를 생략하면 저장 예정 건수만 확인한다.

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};

use ais_core::frontmatter::{Value, dump_string, parse};
use ais_core::ports::fs::check_confinement;
use ais_core::vault::{
    index::NoteIndex,
    note_file::{MAX_NOTE_FILE_BYTES, split_fence},
};
use ais_core::write::{
    apparatus::{format_utc_millis, now_millis},
    atomic::write_atomic,
    pipeline::acquire_lock,
};

const KEYS: [&str; 2] = ["createdAt", "editedAt"];

struct Change {
    path: PathBuf,
    before: Vec<u8>,
    after: String,
    missing: [bool; 2],
    permissions: fs::Permissions,
}

fn add_catalog_files(
    root: &Path,
    directory: &str,
    files: &mut BTreeSet<PathBuf>,
) -> Result<(), Box<dyn Error>> {
    let path = root.join(directory);
    let entries = match fs::read_dir(&path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    check_confinement(root, &path)?;
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_file() && entry.path().extension().is_some_and(|ext| ext == "md") {
            files.insert(entry.path());
        }
    }
    Ok(())
}

fn backfilled(text: &str, at: &str) -> Result<Option<(String, [bool; 2])>, Box<dyn Error>> {
    let source = text.strip_prefix('\u{feff}').unwrap_or(text);
    let (head, tail) = split_fence(source).ok_or("파일 머리를 찾을 수 없다")?;
    let metadata = parse(head).ok_or("파일 머리를 읽을 수 없다")?;
    let missing = KEYS.map(|key| {
        !metadata.iter().any(|(name, value)| {
            matches!(name, Value::Str(name) if name == key)
                && matches!(value, Value::Str(value) if !value.trim().is_empty())
        })
    });
    if !missing.iter().any(|value| *value) {
        return Ok(None);
    }

    let newline = if text
        .find('\n')
        .is_some_and(|at| text.as_bytes().get(at.wrapping_sub(1)) == Some(&b'\r'))
    {
        "\r\n"
    } else {
        "\n"
    };
    let mut remove = BTreeSet::new();
    for (key, needs_value) in KEYS.into_iter().zip(missing) {
        if !needs_value {
            continue;
        }
        let exists = metadata
            .iter()
            .any(|(name, _)| matches!(name, Value::Str(name) if name == key));
        if exists {
            let previous = metadata
                .iter()
                .find(|(name, _)| matches!(name, Value::Str(name) if name == key))
                .map(|(_, value)| value);
            if !matches!(previous, Some(Value::Null | Value::Str(_))) {
                return Err(format!("{key} 값이 비어 있거나 글이어야 한다").into());
            }
            let original = head
                .split_inclusive('\n')
                .find(|line| {
                    line.strip_prefix(key)
                        .is_some_and(|rest| rest.starts_with(':'))
                })
                .ok_or_else(|| format!("{key} 머리의 위치를 찾을 수 없다"))?;
            if parse(original)
                .as_ref()
                .and_then(|map| map.first().map(|(_, value)| value))
                != metadata
                    .iter()
                    .find(|(name, _)| matches!(name, Value::Str(name) if name == key))
                    .map(|(_, value)| value)
            {
                return Err(format!("{key} 값을 한 줄에서 바꿀 수 없다").into());
            }
            remove.insert(original);
        }
    }

    let start = head.as_ptr().addr() - text.as_ptr().addr();
    let end = tail.as_ptr().addr() - text.as_ptr().addr();
    let head_end = start + head.len();
    let mut out = text[..start].to_owned();
    for line in head
        .split_inclusive('\n')
        .filter(|line| !remove.contains(line))
    {
        out.push_str(line);
    }
    if out.ends_with('\r') {
        out.push('\n');
    } else if !out.ends_with('\n') {
        out.push_str(newline);
    }
    for (key, needs_value) in KEYS.into_iter().zip(missing) {
        if needs_value {
            out.push_str(&format!("{key}: {}{newline}", dump_string(at, 1)));
        }
    }
    if source.starts_with("<!--aisurada") {
        // HTML 주석의 닫는 울타리 앞 줄바꿈은 원본 구분자를 그대로 쓴다.
        out.truncate(out.len() - newline.len());
    }
    out.push_str(&text[head_end..end]);
    out.push_str(tail);
    let (head, _) = split_fence(out.strip_prefix('\u{feff}').unwrap_or(&out))
        .ok_or("바꾼 파일의 머리를 읽을 수 없다")?;
    let updated = parse(head).ok_or("바꾼 파일의 YAML을 읽을 수 없다")?;
    let rest = |map: Vec<(Value, Value)>| {
        map.into_iter()
            .filter(|(key, _)| !matches!(key, Value::Str(key) if KEYS.contains(&key.as_str())))
            .collect::<Vec<_>>()
    };
    if rest(metadata) != rest(updated.clone())
        || KEYS.iter().any(|key| {
            !updated.iter().any(|(name, value)| {
                matches!(name, Value::Str(name) if name == key)
                    && matches!(value, Value::Str(value) if !value.trim().is_empty())
            })
        })
    {
        return Err("다른 머리 값이 바뀌거나 시각이 빠졌다".into());
    }
    Ok(Some((out, missing)))
}

fn run() -> Result<(), Box<dyn Error>> {
    let args = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let [vault, backup @ ..] = args.as_slice() else {
        return Err("backfill_timestamps <vault> [새 백업 폴더]".into());
    };
    if backup.len() > 1 {
        return Err("백업 폴더는 하나만 지정한다".into());
    }
    let root = vault.canonicalize()?;
    let _lock = acquire_lock(&root)?;
    if !root.join(".aisurada/workspace.json").is_file() {
        return Err("워크스페이스 표식이 없다".into());
    }
    let index = NoteIndex::reconciled(&root)?;
    let mut files = index
        .root_notes()
        .flat_map(|root| std::iter::once(root).chain(root.sub_notes.iter()))
        .map(|note| note.content_file.clone())
        .collect::<BTreeSet<_>>();
    for directory in [".aisurada/agendas", ".aisurada/projects"] {
        add_catalog_files(&root, directory, &mut files)?;
    }
    let at = format_utc_millis(now_millis());
    let mut changes = Vec::new();
    for path in &files {
        check_confinement(&root, path)?;
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.is_file() {
            return Err(format!("일반 파일만 바꿀 수 있다: {}", path.display()).into());
        }
        let before = fs::read(path)?;
        if let Some((after, missing)) = backfilled(std::str::from_utf8(&before)?, &at)
            .map_err(|error| format!("{}: {error}", path.display()))?
        {
            if path.starts_with(root.join("notes")) && after.len() as u64 > MAX_NOTE_FILE_BYTES {
                return Err(
                    format!("시각을 채우면 파일 크기 상한을 넘는다: {}", path.display()).into(),
                );
            }
            changes.push(Change {
                path: path.clone(),
                before,
                after,
                missing,
                permissions: metadata.permissions(),
            });
        }
    }
    if let [backup] = backup
        && !changes.is_empty()
    {
        let parent = backup
            .parent()
            .ok_or("백업 폴더의 부모 경로가 필요하다")?
            .canonicalize()?;
        if parent.starts_with(&root) {
            return Err("백업 폴더는 vault 밖에 둔다".into());
        }
        fs::DirBuilder::new().mode(0o700).create(backup)?;
        for change in &changes {
            let destination = backup.join(change.path.strip_prefix(&root)?);
            fs::create_dir_all(destination.parent().unwrap())?;
            fs::write(&destination, &change.before)?;
            fs::set_permissions(destination, change.permissions.clone())?;
        }
        for change in &changes {
            if fs::read(&change.path)? != change.before {
                return Err(format!("읽은 뒤 내용이 바뀌었다: {}", change.path.display()).into());
            }
            write_atomic(&change.path, change.after.as_bytes())?;
            fs::set_permissions(&change.path, change.permissions.clone())?;
        }
    }
    println!(
        "{}",
        serde_json::json!({
            "at": at,
            "mode": if backup.is_empty() { "preview" } else { "apply" },
            "files": files.len(),
            "changed": changes.len(),
            "createdAt": changes.iter().filter(|file| file.missing[0]).count(),
            "editedAt": changes.iter().filter(|file| file.missing[1]).count(),
            "skipped": index.scan.skipped.iter().filter(|item| item.kind == "note" || item.kind == "subNote").count(),
        })
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
