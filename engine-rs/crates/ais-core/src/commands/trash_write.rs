//! 휴지통 쓰기: `delete_doc`(휴지통으로 보내기), `restore_doc`(되살리기), `purge_doc`(영구 삭제), `empty_trash`(비우기).
//! 인자 이름, 검사 순서, 오류 문구와 응답 필드는 Java 엔진과 같고, 목록 파일은 바이트까지 같다.

use crate::kernel::ids::TRASH_ID;
use crate::ports::fs;
use std::collections::HashSet;
use std::path::Path;

use serde::Deserialize;
use serde_json::{Value as Json, json};

use super::subnote::check_writable;
use crate::error::EngineError;
use crate::frontmatter::{Value, dump_general_map};
use crate::kernel::actor::{CallContext, Caller};
use crate::kernel::time::format_utc_millis;
use crate::text::{NOTE_ID, cut_utf16, is_blank};
use crate::vault::agenda::{AgendaInfo, is_valid_slug_segment};
use crate::vault::note_file::read_note_file;
use crate::vault::note_meta::get;
use crate::vault::scanner::{Format, is_sharded};
use crate::vault::trash::{TrashRecord, archived_tier};
use crate::write::{agenda_file, apparatus::{next_id, now_millis}};
use crate::write::atomic::write_atomic;
use crate::write::confine::{check_segment, normalize};
use crate::ports::fs::check_confinement;
use crate::write::doc_place::move_path;
use crate::write::pipeline::WriteScope;
use crate::write::sub_note::remove_tree;
use crate::write::trash_file::{archive_path, entries_in_file_order, save};

const RECOVERED_AGENDA_SUMMARY: &str = "복구함\n\n삭제됐다가 복구된 노트 중, 원래 아젠다가 사라진 것들이 모이는 곳이에요.";

/// 이 호출자가 휴지통 항목을 건드려도 되는가. AI는 기록 층 노트의 보관물을 되살리지도 지우지도 못한다(이슈 #80, #82).
/// 앱에는 층이 상관없으므로 보관 파일을 열지 않는다.
fn may_touch(vault: &Path, entry: &TrashRecord, context: &CallContext) -> bool {
    context.caller == Caller::App || archived_tier(vault, entry).visible_to(context.caller)
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DeleteDocRequest {
    /// 노트 id
    pub id: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TrashEntryRequest {
    /// 휴지통 항목 id(`list_trash`가 알려 준다)
    pub trash_id: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EmptyTrashRequest {}

fn require_id(id: &str, usage: &str) -> Result<(), EngineError> {
    if is_blank(id) {
        return Err(EngineError::invalid_argument(format!(
            "인자가 하나 필요하다: {usage}"
        )));
    }
    Ok(())
}

/// 휴지통 항목 id 모양을 검사하고 그 항목을 찾는다(목록 파일에 적힌 차례로 처음 것).
fn find_entry(vault: &Path, id: &str, usage: &str) -> Result<TrashRecord, EngineError> {
    require_id(id, usage)?;
    if !TRASH_ID.is_match(id) {
        return Err(EngineError::invalid_argument(format!(
            "휴지통 항목 id 모양이 아니다: {id}"
        )));
    }
    entries_in_file_order(vault)
        .0
        .into_iter()
        .find(|entry| entry.trash_id == id)
        .ok_or_else(|| EngineError::not_found(format!("TrashEntry not found: {id}")))
}

fn auto_purged_rows(purged: &[TrashRecord]) -> Vec<Json> {
    purged
        .iter()
        .map(|entry| {
            json!({"trashId": entry.trash_id, "docId": entry.doc_id, "title": entry.title, "file": entry.file})
        })
        .collect()
}

fn rel_of(vault: &Path, path: &Path) -> String {
    path.strip_prefix(vault)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// 노트를 휴지통으로 보낸다: 노트 폴더(또는 파일)를 보관 폴더로 옮기고 목록 맨 앞에 항목을 더한다.
pub fn delete_doc(
    vault: &Path,
    request: &DeleteDocRequest,
    scope: &WriteScope,
    context: &CallContext,
) -> Result<Json, EngineError> {
    require_id(&request.id, "delete-doc <노트 id>")?;
    if !NOTE_ID.is_match(&request.id) {
        return Err(EngineError::invalid_argument(format!(
            "노트 id 모양이 아니다: {}",
            request.id
        )));
    }
    let note = scope
        .index
        .note(&request.id)
        .ok_or_else(|| EngineError::not_found(format!("Note not found: {}", request.id)))?;
    check_writable(note, context.caller)?;
    let meta = &note.meta;

    let trash_id = next_id("tr");
    let path = note.rel_path.clone();
    let extension = if note.format == Format::Markdown {
        "md"
    } else {
        note.format.as_str()
    };
    let single = [".md", ".html", ".json"]
        .iter()
        .any(|suffix| path.ends_with(suffix));
    let file = if single {
        format!("{trash_id}.{extension}")
    } else {
        trash_id.clone()
    };
    // 아젠다는 파일 머리의 `agenda`(공백뿐이 아닌 글)를, 없거나 못 읽으면 경로의 둘째 조각을 쓴다
    let from_path = || path.split('/').nth(1).unwrap_or_default().to_string();
    let content = if single {
        vault.join(&path)
    } else {
        note.content_file.clone()
    };
    let slug = match read_note_file(&content)
        .ok()
        .and_then(|parsed| parsed.frontmatter)
        .and_then(|raw| match get(&raw, "agenda") {
            Some(Value::Str(s)) if !is_blank(s) => Some(s.clone()),
            _ => None,
        }) {
        Some(slug) => slug,
        None => from_path(),
    };
    let entry = TrashRecord {
        trash_id: trash_id.clone(),
        doc_id: request.id.clone(),
        title: meta.title.clone(),
        agenda_slug: slug,
        agenda_id: scope
            .index
            .agendas
            .table
            .by_slug(&meta.agenda_slug)
            .map(|a| a.id.clone())
            .unwrap_or_default(),
        doc_type: meta.doc_type.clone(),
        format: note.format.as_str(),
        original_path: path.clone(),
        file: file.clone(),
        created: meta.created.clone(),
        trashed_at: format_utc_millis(now_millis()),
        file_order: 0,
    };
    let (existing, _) = entries_in_file_order(vault);
    let source = vault.join(&path);
    if !fs::exists(&source) {
        return Err(EngineError::invalid_argument(format!(
            "옮길 노트 파일이 없다: {path}"
        )));
    }
    move_path(&source, &archive_path(vault, &file)?)?;
    let mut updated = vec![entry];
    updated.extend(existing.into_iter().filter(|e| e.trash_id != trash_id));
    let purged = save(vault, &updated)?;

    let mut response = json!({
        "trashId": trash_id,
        "id": request.id,
        "title": meta.title,
        "originalPath": path,
        "file": file,
    });
    if !purged.is_empty() {
        response["autoPurged"] = json!(auto_purged_rows(&purged));
    }
    let category = meta.favorite_category.as_deref();
    if meta.favorite {
        response["wasFavorite"] = json!(true);
    }
    if let Some(category) = category {
        response["favoriteCategory"] = json!(category);
    }
    if meta.favorite || category.is_some() {
        let label = category.map_or_else(String::new, |c| format!("(카테고리 '{c}')"));
        response["warning"] = json!(format!(
            "이 노트는 즐겨찾기였어요{label} — 즐겨찾기는 노트 id에 붙으니 같은 내용을 새 노트로 다시 만들면 별이 따라오지 않습니다. 내용을 다시 쓰려던 거면 update_doc으로 이 노트를 키우고, 되살리려면 restore_doc({{ trashId }})을 쓰세요."
        ));
    }
    Ok(response)
}

/// 휴지통 항목을 되살린다. 원래 아젠다가 없어졌으면 `recovered` 아젠다(없으면 만든다)로 보낸다.
pub fn restore_doc(
    vault: &Path,
    request: &TrashEntryRequest,
    scope: &WriteScope,
    context: &CallContext,
) -> Result<Json, EngineError> {
    let entry = find_entry(
        vault,
        &request.trash_id,
        "restore-doc <휴지통 항목 id. list-trash 로 조회한다>",
    )?;
    // AI는 기록 층 노트를 되살리지 못한다(휴지통 목록에서도 안 보인다). 목록 파일을 고치기 전에 선다.
    if !may_touch(vault, &entry, context) {
        return Err(EngineError::record_tier_write(&entry.doc_id));
    }
    let table = &scope.index.agendas.table;

    let mut slug = entry.agenda_slug.clone();
    if !is_valid_slug_segment(&slug) || !table.has_slug(&slug) {
        slug = "recovered".to_string();
        if !table.has_slug(&slug) {
            let at = format_utc_millis(now_millis());
            let recovered = AgendaInfo {
                id: "ag-recovered".into(),
                slug: slug.clone(),
                title: "복구함".into(),
                hue: "blue".into(),
                created_at: Some(at.clone()),
                edited_at: Some(at.clone()),
                repo: None,
                project: None,
                project_repo: None,
                summary: RECOVERED_AGENDA_SUMMARY.into(),
            };
            write_atomic(
                &vault.join(agenda_file::rel_path(&slug)),
                &agenda_file::render(&recovered, &at, &at),
            )?;
        }
    }
    let (entries, _) = entries_in_file_order(vault);
    let others = || -> Vec<TrashRecord> {
        entries
            .iter()
            .filter(|e| e.trash_id != entry.trash_id)
            .cloned()
            .collect()
    };
    let source = archive_path(vault, &entry.file)?;
    if !fs::exists(&source) {
        save(vault, &others())?;
        return Err(EngineError::invalid_argument(
            "보관 파일이 사라져 복구할 수 없어요(휴지통 목록 정리됨).",
        ));
    }
    let mut name = entry
        .original_path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .to_string();
    if name.is_empty() || name.contains("..") || name.starts_with('.') {
        name = entry.file.clone();
    }
    let parent = if is_sharded(vault) {
        format!("notes/{slug}/{}", cut_utf16(&entry.created, 7))
    } else {
        format!("notes/{slug}")
    };
    check_segment(vault, &parent)?;
    check_segment(vault, &name)?;
    let mut target = normalize(&vault.join(&parent).join(&name));
    check_confinement(vault, &target)?;
    if fs::exists(&target) {
        let tail = format!(
            "-restored-{}",
            &entry.trash_id[entry.trash_id.len().saturating_sub(6)..]
        );
        name = match name.rfind('.') {
            Some(dot) if dot > 0 => format!("{}{tail}{}", &name[..dot], &name[dot..]),
            _ => format!("{name}{tail}"),
        };
        target = normalize(&vault.join(&parent).join(&name));
        check_confinement(vault, &target)?;
    }
    let agenda_changed = slug != entry.agenda_slug
        || !entry.agenda_id.is_empty()
            && table
                .by_slug(&slug)
                .is_some_and(|agenda| agenda.id != entry.agenda_id);
    let mut removed_sub_agendas = false;
    if agenda_changed {
        removed_sub_agendas =
            rewrite_agenda(&source, entry.format, &slug, scope.index.sub_agendas(&slug))?;
    }
    move_path(&source, &target)?;
    save(vault, &others())?;
    let mut response = json!({
        "trashId": entry.trash_id,
        "restored": true,
        "path": rel_of(vault, &target),
    });
    if removed_sub_agendas {
        response["warning"] = json!("복원한 아젠다에 없는 서브 아젠다를 제외했다");
    }
    Ok(response)
}

/// 되살릴 보관물의 머리에 적힌 아젠다를 `slug`로 바꾼다. 못 읽거나 머리가 없으면 그대로 둔다.
fn rewrite_agenda(
    source: &Path,
    format: &str,
    slug: &str,
    sub_agendas: &[crate::vault::sub_agenda::SubAgenda],
) -> Result<bool, EngineError> {
    let extension = if format == "markdown" { "md" } else { format };
    let name = source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let single = [".md", ".html", ".json"]
        .iter()
        .any(|suffix| name.ends_with(suffix));
    let content = if single {
        source.to_path_buf()
    } else {
        source.join(format!("note.{extension}"))
    };
    let Ok(parsed) = read_note_file(&content) else {
        return Ok(false);
    };
    let Some(mut meta) = parsed.frontmatter else {
        return Ok(false);
    };
    match meta
        .iter_mut()
        .find(|(k, _)| matches!(k, Value::Str(s) if s == "agenda"))
    {
        Some((_, value)) => *value = Value::Str(slug.to_string()),
        None => meta.push((
            Value::Str("agenda".to_string()),
            Value::Str(slug.to_string()),
        )),
    }
    let mut removed_sub_agendas = false;
    meta.retain_mut(|(key, value)| {
        if !matches!(key, Value::Str(key) if key == "subAgendaIds") {
            return true;
        }
        let Value::List(ids) = value else {
            removed_sub_agendas = true;
            return false;
        };
        let before = ids.len();
        ids.retain(
            |id| matches!(id, Value::Str(id) if sub_agendas.iter().any(|item| &item.id == id)),
        );
        removed_sub_agendas |= ids.len() != before;
        !ids.is_empty()
    });
    let dumped = dump_general_map(&meta).map_err(|e| EngineError::invalid_argument(e.0))?;
    let (open, close) = if format == "html" {
        ("<!--aisurada\n", "-->\n")
    } else {
        ("---\n", "---\n")
    };
    let text = format!("{open}{dumped}{close}{}", parsed.body);
    write_atomic(&content, text.as_bytes())?;
    Ok(removed_sub_agendas)
}

/// 휴지통 항목을 영구 삭제한다(보관물과 목록의 항목). AI는 기록 층 노트의 보관물을 지우지 못한다.
pub fn purge_doc(
    vault: &Path,
    request: &TrashEntryRequest,
    context: &CallContext,
) -> Result<Json, EngineError> {
    let first = find_entry(
        vault,
        &request.trash_id,
        "purge-doc <휴지통 항목 id. list-trash 로 조회한다>",
    )?;
    // 보관물을 지우기 전에 선다(`restore_doc`과 같은 검사)
    if !may_touch(vault, &first, context) {
        return Err(EngineError::record_tier_write(&first.doc_id));
    }
    let (entries, _) = entries_in_file_order(vault);
    remove_tree(&archive_path(vault, &first.file)?)?;
    let kept: Vec<TrashRecord> = entries
        .into_iter()
        .filter(|e| e.trash_id != request.trash_id)
        .collect();
    save(vault, &kept)?;
    Ok(json!({"trashId": request.trash_id, "purged": true}))
}

/// 휴지통을 비운다. 지운 개수, 못 지운 항목, 못 읽어 건너뛴 항목 수, 목록에 없는 보관물(고아) 수를 알린다.
/// AI가 부르면 자기에게 보이는 항목만 비우고, 기록 층 노트의 보관물은 목록에 그대로 남긴다(그 항목은 AI에게 알리지도 않는다).
pub fn empty_trash(vault: &Path, context: &CallContext) -> Result<Json, EngineError> {
    let (entries, skipped) = entries_in_file_order(vault);
    let names: HashSet<&str> = entries.iter().map(|e| e.file.as_str()).collect();
    let orphans = fs::read_dir(vault.join(".aisurada/trash/files"))
        .map(|dir| {
            dir.flatten()
                .filter(|e| !names.contains(e.file_name().to_string_lossy().as_ref()))
                .count()
        })
        .unwrap_or(0);
    let mut retained: Vec<TrashRecord> = Vec::new();
    let mut failures: Vec<Json> = Vec::new();
    let mut purged = 0;
    for entry in &entries {
        if !may_touch(vault, entry, context) {
            retained.push(entry.clone());
            continue;
        }
        let removed = archive_path(vault, &entry.file).and_then(|path| remove_tree(&path));
        match removed {
            Ok(()) => purged += 1,
            Err(error) => {
                failures.push(json!({"trashId": entry.trash_id, "reason": error.message}));
                retained.push(entry.clone());
            }
        }
    }
    save(vault, &retained)?;
    Ok(json!({
        "purged": purged,
        "failed": failures,
        "skipped": skipped,
        "orphans": orphans,
    }))
}
