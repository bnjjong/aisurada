//! 쓰기 명령 한 번의 길. 디스패처(`engine.rs`)가 **한 곳에서** 범위를 열고, 명령은 열린 범위(`&WriteScope`)를 받아 일한다.
//!
//! 범위를 여는 일은 세 조각이다. 이 차례를 지킨다(실패하면 앞 조각까지만 되돌린다).
//! 1. **잠금** ([`acquire_lock`]): vault별 쓰기 잠금을 잡는다.
//! 2. **색인 맞추기** ([`reconcile_index`]): 워크스페이스 표식을 확인하고(표식이 아직 없는 폴더에서 일하는 `init_workspace`만 건너뛴다) 색인을 맞춘다.
//!    맞추기를 세우는 문제(제목이 빈 아젠다, 같은 slug에 다른 id, 코멘트 id가 파일 사이에서 겹침)가 있으면 일을 하지 않는다.
//! 3. **자동 커밋 기록** ([`CommitRecord`]): 쓰기 앞 커밋 여부를 판단하고, 범위를 벗어날 때(성공이든 실패든, 패닉이든) 쓴 쪽을 자동 커밋 대기 기록에 남긴다.
//!
//! 쓴 쪽 기록이 먼저, 잠금이 나중에 풀린다(필드 차례가 곧 놓는 차례다).

use std::path::Path;
use std::sync::Arc;

use super::autocommit::{AutoCommit, ensure_ticker};
use super::confine::absolute_normalized;
use crate::error::EngineError;
use crate::kernel::actor::{CallContext, Caller};
use crate::kernel::token::compute_token;
use crate::ports::LockHandle;
use crate::vault::index::NoteIndex;
use crate::vault::scanner::NoteRecord;

/// 본문 파일 크기 상한
const MAX_NOTE_FILE_BYTES: u64 = 1_000_000;

/// `vault_root`에 표식이 있는지 본다(끊긴 링크는 없는 것으로 본다). 없으면 `AIS-400`이다.
pub fn check_workspace_marker(vault_root: &Path) -> Result<(), EngineError> {
    if crate::ports::fs::exists(&vault_root.join(".aisurada/workspace.json")) {
        return Ok(());
    }
    Err(EngineError::invalid_argument(format!(
        "이 폴더는 아직 워크스페이스가 아니다: {}. .aisurada/workspace.json 이 없다. 경로가 맞는지 보고, 새로 만들 자리면 init-workspace 를 먼저 부른다.",
        absolute_normalized(vault_root).display()
    )))
}

/// 확인 값과 본문 파일을 검사하고 파일 전체 바이트를 돌려준다.
///
/// `provided_token`이 없거나 비면 없는 것으로 본다(JSON 노트는 면제). `action`은 실패 문구에 들어갈 동작 조각이다
/// (예: `"메타를 저장하지"`, `"고치지"`).
pub fn check_precondition(
    vault_root: &Path,
    note: &NoteRecord,
    provided_token: Option<&str>,
    action: &str,
) -> Result<Vec<u8>, EngineError> {
    let path = &note.content_file;
    let rel = path
        .strip_prefix(vault_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let unreadable =
        || EngineError::internal(format!("노트 본문 파일을 못 읽어 {action} 않았다: {rel}"));

    // 없음, 1,000,000바이트 초과, 엄격한 UTF-8이 아님, 입출력 오류
    let metadata = crate::ports::fs::metadata(path).map_err(|_| unreadable())?;
    if !metadata.is_file() || metadata.len() > MAX_NOTE_FILE_BYTES {
        return Err(unreadable());
    }
    let bytes = crate::ports::fs::read(path).map_err(|_| unreadable())?;
    if std::str::from_utf8(&bytes).is_err() {
        return Err(unreadable());
    }

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let is_json = name == "note.json"
        || (name != "comments.json" && name != "ask.json" && name.ends_with(".json"));

    let Some(token) = provided_token.filter(|t| !t.is_empty()) else {
        if is_json {
            return Ok(bytes);
        }
        return Err(EngineError::precondition_failed(format!(
            "선결 조건 토큰이 안 왔어요. 아무것도 쓰지 않았습니다: {rel}\nget_doc 으로 이 노트를 읽고, 그 응답의 precondition 을 f1 인자에 그대로 실어 다시 부릅니다.\n방금 만들거나 방금 고친 노트면 그 쓰기 응답이 이미 다음 토큰을 줍니다. 다시 읽지 말고 그 값을 씁니다."
        )));
    };
    if !token.starts_with(crate::kernel::token::PREFIX) {
        return Err(EngineError::precondition_failed(format!(
            "선결 조건 토큰의 모양이 아니에요. 아무것도 쓰지 않았습니다: {rel}\nget_doc 응답의 precondition 을 글자 그대로 f1 인자에 되돌립니다. 계산해서 만드는 값이 아닙니다.\n해시를 직접 구해 넣지 않습니다. 이 대조가 묻는 것은 그 파일을 방금 읽었는가 입니다."
        )));
    }
    if token != compute_token(&bytes) {
        return Err(EngineError::precondition_failed(format!(
            "다른 쪽이 이 노트를 먼저 고쳤어요. 아무것도 쓰지 않았습니다: {rel}\nget_doc 으로 다시 읽고, 그 새 본문 위에 자기 변경을 다시 얹은 뒤, 그 응답의 precondition 을 f1 로 실어 보냅니다.\nf1 만 갈아 같은 본문으로 다시 부르지 않습니다. 그러면 남이 쓴 글이 사라집니다."
        )));
    }
    Ok(bytes)
}

/// 열린 쓰기 범위. 명령이 일하는 동안 쥐고 있다.
pub struct WriteScope {
    /// 쓰기용으로 맞춘 색인
    pub index: NoteIndex,
    // 아래 두 필드는 차례가 중요하다: 쓴 쪽 기록이 먼저 남고, 잠금이 나중에 풀린다.
    _record: CommitRecord,
    _lock: LockHandle,
}

/// 쓰기 범위를 연다(디스패처만 부른다). `marker_required`가 거짓이면 워크스페이스 표식 검사를 건너뛴다.
pub(crate) fn open(
    vault: &Path,
    marker_required: bool,
    context: &CallContext,
) -> Result<WriteScope, EngineError> {
    let lock = acquire_lock(vault)?;
    let index = if let Some(manager) = &context.manager {
        check_workspace_marker(vault)?;
        NoteIndex::for_manager_write(vault, manager)?
    } else {
        reconcile_index(vault, marker_required)?
    };
    let record = CommitRecord::open(vault, context);
    Ok(WriteScope {
        index,
        _record: record,
        _lock: lock,
    })
}

/// 1. 잠금: 쓰기 잠금을 잡을 때까지 기다린다.
pub fn acquire_lock(vault: &Path) -> Result<LockHandle, EngineError> {
    crate::ports::lock().acquire(vault)
}

/// 2. 색인 맞추기: 표식을 확인하고 쓰기용으로 vault를 훑는다.
pub fn reconcile_index(vault: &Path, marker_required: bool) -> Result<NoteIndex, EngineError> {
    if marker_required {
        check_workspace_marker(vault)?;
    }
    NoteIndex::reconciled(vault)
}

/// 3. 자동 커밋 기록: 쓰기 앞 커밋 여부를 판단하고, 놓일 때 쓴 쪽을 대기 기록에 남긴다.
/// 실패한 명령에서도, 패닉으로 풀리는 중에도 기록을 남긴다(부분으로 쓴 파일이 있을 수 있어서다).
pub struct CommitRecord {
    auto: Arc<AutoCommit>,
    tool: bool,
    session: Option<String>,
}

impl CommitRecord {
    pub fn open(vault: &Path, context: &CallContext) -> CommitRecord {
        let auto = AutoCommit::serving(vault);
        ensure_ticker(vault);
        let tool = context.caller == Caller::Ai;
        auto.before_write(tool, context.session.as_deref());
        CommitRecord {
            auto,
            tool,
            session: context.session.clone(),
        }
    }
}

impl Drop for CommitRecord {
    fn drop(&mut self) {
        self.auto.after_write(self.tool, self.session.as_deref());
    }
}
