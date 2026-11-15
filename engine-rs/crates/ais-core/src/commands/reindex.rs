//! `reindex`: vault를 한 번 훑어 요약과, 읽지 못한 파일·충돌 사본 목록을 돌려줍니다.
//!
//! 앱이 새로고침 때 부르고, 응답의 `skipped`와 `conflicts`를 화면에 보여 줍니다. 건너뜀 목록의 차례는
//! 아젠다, 노트, 문서·회고 템플릿, 휴지통입니다.

use std::path::Path;
use std::time::Instant;

use crate::error::EngineError;
use crate::model::ScanSummary;
use crate::vault::Skipped;
use crate::vault::index::NoteIndex;
use crate::vault::template::{Category, scan_templates};
use crate::vault::trash::scan_trash;

/// 색인 맞추기로 훑는다. 맞추기를 세우는 문제(코멘트 id가 파일 사이에서 겹침 등)가 있으면 요약 대신 오류를 돌려준다.
pub fn execute(vault_root: &Path) -> Result<ScanSummary, EngineError> {
    let started = Instant::now();
    let index = NoteIndex::reconciled(vault_root)?;

    let mut skipped: Vec<Skipped> = index.agendas.skipped.clone();
    skipped.extend(index.scan.skipped.iter().cloned());
    for t in scan_templates(vault_root).skipped {
        let kind = if t.category == Category::Doc {
            "docTemplate"
        } else {
            "ritualTemplate"
        };
        skipped.push(Skipped {
            kind,
            path: t.rel_path,
            reason: t.reason,
        });
    }
    for t in scan_trash(vault_root).skipped {
        skipped.push(Skipped {
            kind: "trash",
            path: t.location,
            reason: t.reason,
        });
    }

    let indexed = index.scan.notes.len();
    Ok(ScanSummary {
        mode: "full",
        elapsed_ms: started.elapsed().as_millis(),
        agendas: index.agendas.table.slug_count(),
        scanned: indexed,
        indexed,
        unchanged: 0,
        removed: 0,
        skipped,
        conflicts: index.scan.conflicts.clone(),
        unsharded: index.scan.unsharded.clone(),
    })
}

/// `reindex_paths` 요청: 훑을 vault 상대 경로들
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default)]
pub struct ReindexPathsRequest {
    pub paths: Vec<String>,
}

/// 경로가 이 개수를 넘으면 노트 전체를 훑는 쪽으로 물러선다.
const MAX_PATHS: usize = 200;

/// 받은 경로가 가리키는 노트만 훑어 요약을 돌려줍니다. 경로가 [`MAX_PATHS`]개를 넘으면 노트 전체를 훑고 `mode`를 `full`로 적습니다.
/// 템플릿과 휴지통은 어느 쪽이든 열지 않습니다.
pub fn execute_paths(
    vault_root: &Path,
    request: &ReindexPathsRequest,
) -> Result<ScanSummary, EngineError> {
    let started = Instant::now();
    let full = request.paths.len() > MAX_PATHS;
    let index = if full {
        NoteIndex::reconciled(vault_root)?
    } else {
        NoteIndex::reconciled_paths(vault_root, &request.paths)?
    };

    let mut skipped: Vec<Skipped> = index.agendas.skipped.clone();
    skipped.extend(index.scan.skipped.iter().cloned());
    let indexed = index.scan.notes.len();
    Ok(ScanSummary {
        mode: if full { "full" } else { "paths" },
        elapsed_ms: started.elapsed().as_millis(),
        agendas: index.agendas.table.slug_count(),
        scanned: indexed,
        indexed,
        unchanged: 0,
        removed: 0,
        skipped,
        conflicts: index.scan.conflicts.clone(),
        unsharded: index.scan.unsharded.clone(),
    })
}
