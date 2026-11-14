//! vault 훑기 결과를 한데 모은 색인. 요청마다 새로 훑습니다(캐시가 없으므로 디스크가 바뀌면 다음 요청에 보입니다).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::agenda::{AgendaScan, scan_agendas};
use super::comment_copy;
use super::scanner::{NoteRecord, NoteScan, scan_notes};
use crate::error::EngineError;

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
    /// 디스크를 새로 훑어 읽기용 색인을 만듭니다. 맞추기를 세우는 문제가 있어도 읽을 수 있는 만큼 읽습니다.
    pub fn fresh(vault_root: &Path) -> NoteIndex {
        Self::build(vault_root).0
    }

    /// 쓰기용으로 디스크를 훑습니다(색인 맞추기). 맞추기를 세우는 문제(제목이 빈 아젠다, 같은 slug에 다른 id,
    /// 코멘트 id가 파일 사이에서 겹침)를 만나면 쓰기가 일을 하지 않도록 오류로 돌려줍니다.
    pub fn reconciled(vault_root: &Path) -> Result<NoteIndex, EngineError> {
        match Self::build(vault_root) {
            (_, Some(fatal)) => Err(fatal),
            (index, None) => Ok(index),
        }
    }

    fn build(vault_root: &Path) -> (NoteIndex, Option<EngineError>) {
        let vault_root =
            std::path::absolute(vault_root).unwrap_or_else(|_| vault_root.to_path_buf());
        let mut agendas = scan_agendas(&vault_root);
        let mut scan = scan_notes(&vault_root, &agendas.table);
        let mut fatal = agendas.fatal.take();
        // 코멘트 id가 겹치는 노트부터 뒤는 읽기 목록에서 뺀다
        if let Some((at, error)) = comment_copy::first_failure(&vault_root, &scan.notes) {
            scan.notes.truncate(at);
            fatal.get_or_insert(error);
        }

        let mut order: Vec<usize> = (0..scan.notes.len()).collect();
        order.sort_by(|&a, &b| compare_created_then_path(&scan.notes[a], &scan.notes[b]));
        let by_id = scan
            .notes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();
        (
            NoteIndex {
                vault_root,
                agendas,
                scan,
                order,
                by_id,
            },
            fatal,
        )
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn scratch() -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ais-index-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".aisurada/agendas")).unwrap();
        dir
    }

    #[test]
    fn same_slug_with_another_id_fails_reconcile_but_reads_an_empty_table() {
        let vault = scratch();
        let agenda = |file: &str, id: &str| {
            fs::write(
                vault.join(".aisurada/agendas").join(file),
                format!("---\nid: {id}\nslug: same\ntitle: 같다\n---\n"),
            )
            .unwrap();
        };
        agenda("a.md", "aga00001-0000000a");
        agenda("b.md", "agb00001-0000000b");
        let error = NoteIndex::reconciled(&vault).err().expect("겹친 slug");
        assert_eq!(
            error.message,
            "Duplicate agenda slug with different id: same"
        );
        assert_eq!(NoteIndex::fresh(&vault).agendas.table.slug_count(), 0);
        fs::remove_dir_all(&vault).unwrap();
    }

    #[test]
    fn a_clean_vault_reconciles() {
        let vault = scratch();
        fs::write(
            vault.join(".aisurada/agendas/ok.md"),
            "---\nid: agok0001-0000000c\nslug: ok\ntitle: 정상\n---\n",
        )
        .unwrap();
        let index = NoteIndex::reconciled(&vault).unwrap();
        assert_eq!(index.agendas.table.slug_count(), 1);
        fs::remove_dir_all(&vault).unwrap();
    }
}
