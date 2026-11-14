//! vault 훑기 결과를 한데 모은 색인. 요청마다 새로 훑습니다(캐시가 없으므로 디스크가 바뀌면 다음 요청에 보입니다).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::agenda::{AgendaScan, scan_agendas};
use super::scanner::{NoteRecord, NoteScan, scan_notes};

pub struct NoteIndex {
    pub vault_root: PathBuf,
    pub agendas: AgendaScan,
    pub scan: NoteScan,
    /// `scan.notes`를 만든 시각 차례(같으면 경로 차례)로 늘어놓은 번호
    order: Vec<usize>,
    /// 메인 노트 id → `scan.notes`의 번호
    by_id: HashMap<String, usize>,
}

impl NoteIndex {
    /// 디스크를 새로 훑어 색인을 만듭니다.
    pub fn fresh(vault_root: &Path) -> NoteIndex {
        let vault_root =
            std::path::absolute(vault_root).unwrap_or_else(|_| vault_root.to_path_buf());
        let agendas = scan_agendas(&vault_root);
        let scan = scan_notes(&vault_root, &agendas.table);

        let mut order: Vec<usize> = (0..scan.notes.len()).collect();
        order.sort_by(|&a, &b| compare_created_then_path(&scan.notes[a], &scan.notes[b]));
        let by_id = scan
            .notes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();
        NoteIndex {
            vault_root,
            agendas,
            scan,
            order,
            by_id,
        }
    }

    /// id로 메인 노트를 찾습니다.
    pub fn note(&self, id: &str) -> Option<&NoteRecord> {
        self.by_id.get(id).map(|&i| &self.scan.notes[i])
    }

    /// 후속 노트 id로 그 노트와 속한 메인 노트를 찾습니다. 같은 id가 여러 메인 노트에 있으면 만든 시각이 빠른 쪽입니다.
    pub fn find_sub(&self, sub_id: &str) -> Option<(&NoteRecord, &NoteRecord)> {
        self.root_notes().find_map(|root| {
            root.sub_notes
                .iter()
                .find(|s| s.id == sub_id)
                .map(|sub| (root, sub))
        })
    }

    /// 메인 노트를 `created` 오름차순(같으면 경로 차례)으로 돌려줍니다.
    pub fn root_notes(&self) -> impl Iterator<Item = &NoteRecord> {
        self.order.iter().map(|&i| &self.scan.notes[i])
    }
}

/// `created` 글의 UTF-8 바이트 차례, 같으면 경로에서 `/`를 U+0001로 바꾼 글의 바이트 차례
fn compare_created_then_path(a: &NoteRecord, b: &NoteRecord) -> std::cmp::Ordering {
    a.meta
        .created
        .as_bytes()
        .cmp(b.meta.created.as_bytes())
        .then_with(|| {
            a.rel_path
                .replace('/', "\u{1}")
                .as_bytes()
                .cmp(b.rel_path.replace('/', "\u{1}").as_bytes())
        })
}
