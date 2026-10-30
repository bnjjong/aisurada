//! Chapter 9: path classification and deliberately narrow, per-file asset scopes.
use serde::Serialize;
use std::{
    collections::HashSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
};
use tauri::{Manager, Runtime};

#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Target {
    Note { ws: String, chain: Vec<String> },
    External { path: String },
}
fn md_path(path: &str) -> Result<PathBuf, String> {
    let p = Path::new(path)
        .canonicalize()
        .map_err(|_| "파일을 찾을 수 없어요")?;
    if !p.is_file()
        || !matches!(
            p.extension()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str(),
            "md" | "markdown"
        )
    {
        return Err("md 파일이 아니에요".into());
    }
    Ok(p)
}
fn head_id(path: &Path) -> Option<String> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(8192)
        .read_to_end(&mut bytes)
        .ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines = text.split('\n').map(|l| l.trim_end_matches('\r'));
    if lines.next()?.trim_start_matches('\u{feff}').trim_end() != "---" {
        return None;
    }
    for line in lines {
        if line.trim_end() == "---" {
            break;
        }
        if let Some(id) = line.strip_prefix("id:") {
            let id = id.trim().trim_matches(['\'', '"']);
            return (!id.is_empty()).then(|| id.to_owned());
        }
    }
    None
}
#[tauri::command]
pub fn resolve_md_target(path: String, known_roots: Vec<String>) -> Result<Target, String> {
    let path = md_path(&path)?;
    let external = || Target::External {
        path: path.to_string_lossy().into_owned(),
    };
    let root = known_roots
        .iter()
        .find_map(|r| {
            let p = Path::new(r).canonicalize().ok()?;
            (path.starts_with(&p) && p.join(".withlog/workspace.json").is_file())
                .then(|| (p, r.trim_end_matches('/').to_owned()))
        })
        .or_else(|| {
            path.parent()?
                .ancestors()
                .find(|p| p.join(".withlog/workspace.json").is_file())
                .map(|p| (p.to_path_buf(), p.to_string_lossy().into_owned()))
        });
    let Some((root, ws)) = root else {
        return Ok(external());
    };
    if path
        .strip_prefix(&root)
        .ok()
        .and_then(|p| p.components().next())
        .map(|c| c.as_os_str() != "notes")
        .unwrap_or(true)
        || path.file_name().unwrap() != "note.md"
    {
        return Ok(external());
    }
    let mut folder = path.parent().unwrap();
    let mut chain = Vec::new();
    loop {
        let Some(id) = head_id(&folder.join("note.md")) else {
            return Ok(external());
        };
        chain.push(id);
        let Some(parent) = folder
            .parent()
            .filter(|p| p.file_name().is_some_and(|n| n == "sub"))
        else {
            break;
        };
        let Some(ancestor) = parent.parent() else {
            return Ok(external());
        };
        folder = ancestor;
    }
    chain.reverse();
    Ok(Target::Note { ws, chain })
}
#[tauri::command]
pub fn read_external_md(path: String) -> Result<String, String> {
    let p = md_path(&path)?;
    let size = fs::metadata(&p)
        .map_err(|e| format!("파일 정보를 못 읽었어요: {e}"))?
        .len();
    if size > 2_097_152 {
        return Err(format!(
            "파일이 너무 커요({size}바이트, 상한 2097152바이트)"
        ));
    }
    String::from_utf8(fs::read(p).map_err(|e| format!("파일을 못 읽었어요: {e}"))?)
        .map_err(|_| "읽을 수 없는 파일이에요(UTF-8 글이 아니에요)".into())
}
pub fn image_candidates(md_path: &str, srcs: &[String]) -> Vec<PathBuf> {
    let Ok(md) = Path::new(md_path).canonicalize() else {
        return vec![];
    };
    let Some(base) = md.parent() else {
        return vec![];
    };
    let mut seen = HashSet::new();
    srcs.iter()
        .filter_map(|src| {
            let src = src.split(['?', '#']).next().unwrap_or("");
            if src.is_empty()
                || src.starts_with('/')
                || src.split('/').next().unwrap_or("").contains(':')
            {
                return None;
            }
            let p = base.join(src).canonicalize().ok()?;
            let ext = p.extension()?.to_str()?.to_ascii_lowercase();
            (p.is_file()
                && p.starts_with(base)
                && ["svg", "png", "jpg", "jpeg", "gif", "webp", "avif", "bmp"]
                    .contains(&ext.as_str())
                && seen.insert(p.clone()))
            .then_some(p)
        })
        .collect()
}
#[tauri::command]
pub fn allow_external_images<R: Runtime>(
    app: tauri::AppHandle<R>,
    md_path: String,
    srcs: Vec<String>,
) -> Result<usize, String> {
    let paths = image_candidates(&md_path, &srcs);
    for p in &paths {
        app.asset_protocol_scope()
            .allow_file(p)
            .map_err(|e| e.to_string())?;
    }
    Ok(paths.len())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn classify_subnotes_links_raw_roots_and_external() {
        let t = tempfile::tempdir().unwrap();
        let r = t.path();
        fs::create_dir_all(r.join(".withlog")).unwrap();
        fs::write(r.join(".withlog/workspace.json"), "{}").unwrap();
        let p = r.join("notes/a/sub/b");
        fs::create_dir_all(&p).unwrap();
        fs::write(
            r.join("notes/a/note.md"),
            "\u{feff}---\r\nid: 'root'\r\n---\r\n",
        )
        .unwrap();
        let f = p.join("note.md");
        fs::write(&f, "---\nid: child\n---\n").unwrap();
        assert_eq!(
            resolve_md_target(
                f.to_str().unwrap().into(),
                vec![format!("{}///", r.display())]
            )
            .unwrap(),
            Target::Note {
                ws: r.to_str().unwrap().into(),
                chain: vec!["root".into(), "child".into()]
            }
        );
        fs::write(&f, "---\n id: child\n---").unwrap();
        assert!(matches!(
            resolve_md_target(f.to_str().unwrap().into(), vec![]).unwrap(),
            Target::External { .. }
        ));
        assert!(resolve_md_target("/missing".into(), vec![]).is_err());
    }
    #[test]
    fn strict_text_and_scoped_image_candidates() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("README.MD");
        fs::write(&p, "\u{feff}text\r\n").unwrap();
        assert_eq!(
            read_external_md(p.to_str().unwrap().into()).unwrap(),
            "\u{feff}text\r\n"
        );
        fs::write(t.path().join("a.png"), b"img").unwrap();
        let srcs = [
            "a.png?x",
            "a.png#x",
            "https://x/a.png",
            "/a.png",
            "../a.png",
            "missing.png",
        ]
        .map(str::to_owned);
        assert_eq!(image_candidates(p.to_str().unwrap(), &srcs).len(), 1);
        fs::write(&p, [255]).unwrap();
        assert!(read_external_md(p.to_str().unwrap().into()).is_err());
        fs::write(&p, vec![0; 2_097_153]).unwrap();
        assert!(read_external_md(p.to_str().unwrap().into()).is_err());
    }
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    #[test]
    fn head_is_narrow_bounded_and_does_not_parse_yaml() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("note.md");
        for (text, expected) in [
            ("---\nid : spaced\n---", None),
            (" ---\nid: leading\n---", None),
            ("---\n---\nid: after", None),
            ("---\nid: \"'quoted'\"\n---", Some("quoted")),
            ("---\nid: x # not yaml\n---", Some("x # not yaml")),
        ] {
            fs::write(&p, text).unwrap();
            assert_eq!(head_id(&p).as_deref(), expected);
        }
        fs::write(&p, format!("---\n{}\nid: late\n---", "#".repeat(8190))).unwrap();
        assert!(head_id(&p).is_none());
    }
    #[test]
    fn image_symlink_escape_is_not_opened() {
        let t = tempfile::tempdir().unwrap();
        let base = t.path().join("base");
        fs::create_dir(&base).unwrap();
        fs::write(base.join("r.md"), "x").unwrap();
        fs::write(t.path().join("outside.png"), "img").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(t.path().join("outside.png"), base.join("alias.png")).unwrap();
        assert!(image_candidates(
            base.join("r.md").to_str().unwrap(),
            &["alias.png".into(), "../outside.png".into()]
        )
        .is_empty());
    }
}
