//! C4 아키텍처의 변경 이력(타임 트래블)을 vault에 쌓고 읽는 곳. 점의 모양과 요약은 도메인(`c4::history`)이 정한다.
//!
//! ```text
//! <아키텍처 폴더>/history/000001.json   점 하나(모델과 위치 전체). 번호는 오름차순이고 지워져도 다시 쓰지 않는다.
//! ```
//!
//! 이력은 모델과 위치의 확인 값(`f1`)에 들어가지 않는다. 이력을 쓰다 실패해도 이미 저장한 변경은 그대로 둔다(변경이 먼저다).

use std::path::{Path, PathBuf};

use super::history::{Kind, Snapshot, overflow, summarize};
use super::store::{self, Stored};
use super::{Model, Views};
use crate::error::EngineError;
use crate::ports::fs;
use crate::write::atomic::write_atomic;

/// 아키텍처 폴더 안의 이력 폴더 이름
pub const DIR: &str = "history";

fn folder(vault: &Path, project: &str, diagram: &str) -> PathBuf {
    store::dir(vault, project, diagram).join(DIR)
}

fn file_of(folder: &Path, seq: u32) -> PathBuf {
    folder.join(format!("{seq:06}.json"))
}

/// 폴더의 점 번호들(오름차순). 폴더가 없으면 비어 있다. `000001.json` 모양이 아닌 파일은 무시한다.
fn seqs(folder: &Path) -> Vec<u32> {
    let Ok(entries) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<u32> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let digits = name.strip_suffix(".json")?;
            (digits.len() == 6 && digits.bytes().all(|b| b.is_ascii_digit()))
                .then(|| digits.parse().ok())?
        })
        .collect();
    found.sort_unstable();
    found
}

fn read(folder: &Path, seq: u32) -> Result<Snapshot, EngineError> {
    let file = file_of(folder, seq);
    let bytes = store::read_bytes(&file)?;
    if bytes.is_empty() {
        return Err(EngineError::not_found(format!(
            "이력의 점을 못 찾았다: {seq}"
        )));
    }
    Snapshot::parse(&store::utf8(&bytes, "이력 파일")?)
}

fn write(folder: &Path, snapshot: &Snapshot) -> Result<(), EngineError> {
    write_atomic(
        &file_of(folder, snapshot.seq),
        snapshot.render()?.as_bytes(),
    )
}

/// 이력의 점 전부(오래된 것부터). 읽지 못하는 점은 건너뛴다(깨진 점 하나가 타임 트래블 전체를 막지 않게).
pub fn list(vault: &Path, project: &str, diagram: &str) -> Result<Vec<Snapshot>, EngineError> {
    store::require_project(vault, project)?;
    let folder = folder(vault, project, diagram);
    Ok(seqs(&folder)
        .into_iter()
        .filter_map(|seq| read(&folder, seq).ok())
        .collect())
}

/// 점 하나. 없으면 `not_found`다.
pub fn get(
    vault: &Path,
    project: &str,
    diagram: &str,
    seq: u32,
) -> Result<Snapshot, EngineError> {
    store::require_project(vault, project)?;
    read(&folder(vault, project, diagram), seq)
}

/// 마지막 점이 지금 파일의 모양과 같은가. 점이 하나도 없으면 모델이 비어 있을 때만 같다.
pub fn latest_is_current(snapshots: &[Snapshot], stored: &Stored) -> bool {
    match snapshots.last() {
        Some(last) => last.model == stored.model && last.views == stored.views,
        None => stored.model == Model::default(),
    }
}

/// 변경 하나가 저장된 뒤에 이력을 잇는다. `before`는 쓰기 전에 읽은 모양, `model`과 `views`는 쓴 뒤의 모양이다.
///
/// - 이력이 비어 있는데 쓰기 전 모델에 요소가 있으면 먼저 「시작 상태」 점을 남긴다.
/// - 마지막 점의 모델이 쓰기 전 모델과 다르면(손으로 고침, 다른 도구) 먼저 「외부 변경」 점을 남긴다.
/// - 모델이 그대로면(위치만 바뀜) 마지막 점의 위치에 합치고 새 점은 만들지 않는다.
/// - 그 밖에는 새 점을 잇고, 상한을 넘은 오래된 점을 지운다.
#[allow(clippy::too_many_arguments)]
pub fn record(
    vault: &Path,
    project: &str,
    diagram: &str,
    before: &Stored,
    model: &Model,
    views: &Views,
    by: &str,
    now_ms: i64,
) -> Result<(), EngineError> {
    let folder = folder(vault, project, diagram);
    let mut numbers = seqs(&folder);
    let mut next = numbers.last().copied().unwrap_or(0) + 1;
    let mut latest = numbers.last().and_then(|&seq| read(&folder, seq).ok());

    let mut append = |kind: Kind,
                      by: &str,
                      summary: String,
                      model: &Model,
                      views: &Views|
     -> Result<Snapshot, EngineError> {
        let snapshot = Snapshot {
            seq: next,
            at: now_ms,
            by: by.to_string(),
            kind,
            summary,
            model: model.clone(),
            views: views.clone(),
        };
        write(&folder, &snapshot)?;
        numbers.push(next);
        next += 1;
        Ok(snapshot)
    };

    match &latest {
        None if before.model != Model::default() => {
            latest = Some(append(
                Kind::Baseline,
                "",
                "시작 상태".to_string(),
                &before.model,
                &before.views,
            )?);
        }
        Some(last) if last.model != before.model => {
            let summary = summarize(&last.model, &before.model);
            latest = Some(append(
                Kind::External,
                "",
                if summary.is_empty() {
                    "외부 변경".to_string()
                } else {
                    format!("외부 변경 · {summary}")
                },
                &before.model,
                &before.views,
            )?);
        }
        _ => {}
    }

    if *model == before.model {
        if let Some(mut last) = latest
            && last.views != *views
        {
            last.views = views.clone();
            write(&folder, &last)?;
        }
        return Ok(());
    }

    append(
        Kind::Change,
        by,
        summarize(&before.model, model),
        model,
        views,
    )?;
    for seq in overflow(&numbers) {
        let _ = fs::remove_file(file_of(&folder, *seq));
    }
    Ok(())
}
