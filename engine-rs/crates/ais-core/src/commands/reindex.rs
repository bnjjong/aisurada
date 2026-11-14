//! `reindex`: vault를 한 번 훑어 요약과, 읽지 못한 파일·충돌 사본 목록을 돌려줍니다.
//!
//! 앱이 새로고침 때 부르고, 응답의 `skipped`와 `conflicts`를 화면에 보여 줍니다. 건너뜀 목록의 차례는
//! 아젠다, 노트, 문서·회고 템플릿, 휴지통입니다.

use std::path::Path;
use std::time::Instant;

use crate::model::ScanSummary;
use crate::vault::Skipped;
use crate::vault::index::NoteIndex;
use crate::vault::template::{Category, scan_templates};
use crate::vault::trash::scan_trash;

pub fn execute(vault_root: &Path) -> ScanSummary {
    let started = Instant::now();
    let index = NoteIndex::fresh(vault_root);

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
    ScanSummary {
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
    }
}
