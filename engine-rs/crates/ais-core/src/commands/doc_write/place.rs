//! 노트 쓰기 배치: 폴더 노트 및 단일 파일 노트의 경로 검사, 원자적 쓰기, 이전 파일 정리 및 롤백.

use super::*;

/// 옛 단일 파일을 지운다. 못 지우면 응답을 실패로 바꾸지 않고 경고로 알린다.
pub(crate) fn remove_old(vault: &Path, old_rel: &str, warnings: &mut Vec<String>) {
    match fs::remove_file(vault.join(old_rel)) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => warnings.push(format!(
            "옛 노트 삭제 실패 — 같은 id가 둘 남았을 수 있어요(정리 필요): {old_rel}"
        )),
    }
}

/// 노트를 형식(폴더/단일파일)에 맞게 정식 위치에 배치하고 쓴다.
/// 쓰기에 성공하면 최종 상대 경로(`final_rel`)를 반환한다.
pub(crate) fn place_and_write(
    vault: &Path,
    note: &NoteRecord,
    parent: &str,
    folder: &str,
    text: &str,
    agenda_changed: bool,
    warnings: &mut Vec<String>,
) -> Result<String, EngineError> {
    let old_rel = note.rel_path.as_str();
    let check_text_size = |rel: &str| check_size(text, rel);

    let final_rel: String;
    if !note.is_single_file {
        // 폴더 노트: 경로 검사, 빈자리 고르기, 옮기기 뒤에 상한을 보고 쓴다
        let canonical = format!("{parent}/{folder}");
        check_path(vault, &canonical)?;
        if agenda_changed {
            check_text_size(&canonical)?;
        }
        final_rel = place_folder(vault, old_rel, &canonical)?;
        if !agenda_changed {
            check_text_size(&final_rel)?;
        }
        let content_name = note
            .content_file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Err(error) =
            write_atomic(&vault.join(&final_rel).join(&content_name), text.as_bytes())
        {
            if agenda_changed
                && final_rel != old_rel
                && let Err(rollback) = move_path(&vault.join(&final_rel), &vault.join(old_rel))
            {
                return Err(EngineError::internal(format!(
                    "{}. 노트 폴더를 원래 자리로 돌리지 못했다: {}",
                    error.message, rollback.message
                )));
            }
            return Err(error);
        }
    } else if note.format == Format::Html {
        // 옛 .html 단일 파일: 이름만 바꾼다. 상한, 경로, 쓰기 차례이고 같은 이름 파일은 덮는다
        final_rel = format!("{parent}/{folder}.html");
        check_text_size(&final_rel)?;
        check_path(vault, &final_rel)?;
        write_atomic(&vault.join(&final_rel), text.as_bytes())?;
        if final_rel != old_rel {
            remove_old(vault, old_rel, warnings);
        }
    } else {
        // 옛 .md, .json 단일 파일: 빈자리를 안 보고 새 폴더에 쓴 뒤 옛 파일을 지운다
        final_rel = format!("{parent}/{folder}");
        check_text_size(&final_rel)?;
        check_path(vault, &final_rel)?;
        let name = if note.format == Format::Json {
            "note.json"
        } else {
            "note.md"
        };
        write_atomic(&vault.join(&final_rel).join(name), text.as_bytes())?;
        remove_old(vault, old_rel, warnings);
    }

    Ok(final_rel)
}
