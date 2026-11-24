//! 후속 노트 머리 아젠다 줄 교체: 아젠다 이동 시 폴더 내 후속 노트들의 frontmatter `agenda:` 동기화.

use super::*;

/// 옮긴 노트 폴더 안 후속 노트들의 머리 `agenda:` 줄을 새 `slug`로 바꾼다.
/// 색인은 후속 노트의 아젠다를 폴더 자리로 읽으므로 이 줄이 낡아도 목록은 그대로지만, 파일을 직접 읽는 사람과 도구를 위해 맞춘다.
/// 그 줄 하나만 바꾸고 나머지 바이트(편집 도장 포함)는 그대로 둔다. 폴더째 옮기기는 이미 끝났으므로
/// 한 파일을 못 고쳐도 되돌리지 않고 `warnings`에 적는다.
pub(crate) fn retag_sub_notes(
    vault: &Path,
    old_folder: &Path,
    new_rel: &str,
    subs: &[NoteRecord],
    slug: &str,
    warnings: &mut Vec<String>,
) {
    let new_folder = vault.join(new_rel);
    let line = format!("agenda: {}", dump_string(slug, 1));
    for sub in subs {
        let Ok(inside) = sub.content_file.strip_prefix(old_folder) else {
            continue;
        };
        let path = new_folder.join(inside);
        let result = fs::read(&path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| String::from_utf8(bytes).map_err(|_| "UTF-8 이 아니다".to_string()))
            .and_then(|text| match replace_agenda_line(&text, &line) {
                Some(changed) if changed != text => {
                    write_atomic(&path, changed.as_bytes()).map_err(|e| e.message)
                }
                _ => Ok(()),
            });
        if let Err(reason) = result {
            warnings.push(format!(
                "후속 노트 머리의 agenda 를 못 고쳤다: {} ({reason})",
                rel_of(vault, &path)
            ));
        }
    }
}

/// 머리 울타리 안에서 들여쓰지 않은 `agenda:` 줄을 `line`으로 바꾼 글. 머리나 그 줄이 없으면 `None`
pub(crate) fn replace_agenda_line(text: &str, line: &str) -> Option<String> {
    let (yaml, _) = split_fence(text)?;
    let mut offset = yaml.as_ptr() as usize - text.as_ptr() as usize;
    for raw in yaml.split_inclusive('\n') {
        let content = raw.trim_end_matches(['\n', '\r']);
        if content.starts_with("agenda:") {
            let end = offset + content.len();
            return Some(format!("{}{line}{}", &text[..offset], &text[end..]));
        }
        offset += raw.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::replace_agenda_line;

    #[test]
    fn replaces_only_the_agenda_line_of_the_head() {
        let text = "---\nid: sn1\nagenda: demo\ntitle: t\n---\n\nagenda: 본문은 그대로\n";
        assert_eq!(
            replace_agenda_line(text, "agenda: home").as_deref(),
            Some("---\nid: sn1\nagenda: home\ntitle: t\n---\n\nagenda: 본문은 그대로\n")
        );
        let html = "<!--aisurada\nid: sn1\nagenda: demo\n-->\n<p>x</p>";
        assert_eq!(
            replace_agenda_line(html, "agenda: home").as_deref(),
            Some("<!--aisurada\nid: sn1\nagenda: home\n-->\n<p>x</p>")
        );
        // 들여쓴 줄(다른 키의 값)은 아젠다 줄이 아니다. 머리가 없으면 손대지 않는다
        assert_eq!(
            replace_agenda_line("---\nmeta:\n  agenda: x\n---\n", "agenda: y"),
            None
        );
        assert_eq!(replace_agenda_line("agenda: x\n", "agenda: y"), None);
    }
}
