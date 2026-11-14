//! `history`: 노트의 판 목록과, 고른 판의 내용, 글자가 든 판을 돌려준다. 현재 색인이나 노트 파일에 기대지 않고 git만 본다.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::EngineError;
use crate::text::{find_region_match, is_blank, trim};
use crate::vault::history::{VaultHistory, VersionRow, commit_shape};

const USAGE: &str = "history <노트 id> [번호] [commit=<커밋>] [query=<글자>]";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HistoryRequest {
    pub id: String,
    /// 볼 판의 번호(1부터, 최근 것이 1번)
    pub number: Option<i64>,
    /// 볼 판의 커밋(40자리 16진수). 번호와 함께 줄 수 없다
    pub commit: Option<String>,
    /// 본문에 이 글자(영문 대소문자는 가리지 않는다)가 든 판을 `matches`로 알려 준다
    pub query: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryResponse {
    pub id: String,
    pub count: usize,
    /// 고른 판의 파일 전체 글
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// 고른 판의 본문(머리를 뗀 것)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note_body: Option<String>,
    /// `query`가 든 판의 커밋
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matches: Option<Vec<String>>,
    pub versions: Vec<VersionRow>,
}

pub fn execute(vault: &Path, request: &HistoryRequest) -> Result<HistoryResponse, EngineError> {
    let number = match request.number {
        Some(n) if !(1..=i64::from(i32::MAX)).contains(&n) => {
            return Err(EngineError::invalid_argument(format!(
                "판 번호는 1 이상의 정수여야 한다: {n}"
            )));
        }
        other => other.map(|n| n as usize),
    };
    let commit = trim(request.commit.as_deref().unwrap_or(""));
    if !commit.is_empty() && !commit_shape(commit) {
        return Err(EngineError::invalid_argument(format!(
            "커밋은 40 자리 16 진수여야 한다: {commit}"
        )));
    }
    if number.is_some() && !commit.is_empty() {
        return Err(EngineError::invalid_argument(
            "판 번호와 커밋을 함께 줄 수 없다. 하나만 준다",
        ));
    }
    let id = request.id.as_str();
    if is_blank(id) {
        return Err(EngineError::invalid_argument(format!(
            "인자가 하나 필요하다: {USAGE}"
        )));
    }
    let query = trim(request.query.as_deref().unwrap_or(""));

    let history = VaultHistory::new(vault);
    let versions = history.versions(id);
    let selected = if !commit.is_empty() {
        let at = versions
            .iter()
            .position(|v| v.commit == commit)
            .ok_or_else(|| {
                EngineError::conflict(format!("그 커밋은 이 노트의 판 목록에 없다: {commit}"))
            })?;
        Some(at)
    } else if let Some(number) = number {
        if versions.is_empty() {
            return Err(EngineError::conflict(format!(
                "그 노트에는 판이 아직 없다: {id}"
            )));
        }
        if number > versions.len() {
            return Err(EngineError::conflict(format!(
                "판 번호가 범위 밖이다(1 부터 {} 까지 있다): {number}",
                versions.len()
            )));
        }
        Some(number - 1)
    } else {
        None
    };

    let contents = history.read(&versions);
    let (body, note_body) = match selected.and_then(|at| contents[at].as_ref()) {
        Some(content) => (Some(content.full.clone()), Some(content.body.clone())),
        None => (None, None),
    };
    let matches = (!query.is_empty()).then(|| {
        let needle: Vec<u16> = query.encode_utf16().collect();
        versions
            .iter()
            .zip(&contents)
            .filter(|(_, content)| {
                content.as_ref().is_some_and(|c| {
                    let text: Vec<u16> = c.body.encode_utf16().collect();
                    text.len() >= needle.len() && find_region_match(&text, &needle).is_some()
                })
            })
            .map(|(version, _)| version.commit.clone())
            .collect()
    });
    let rows = versions
        .iter()
        .zip(&contents)
        .enumerate()
        .map(|(i, (version, content))| VaultHistory::row(version, i + 1, content.as_ref()))
        .collect();
    Ok(HistoryResponse {
        id: id.to_string(),
        count: versions.len(),
        body,
        note_body,
        matches,
        versions: rows,
    })
}
