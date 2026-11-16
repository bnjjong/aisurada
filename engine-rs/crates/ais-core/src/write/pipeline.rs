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
use super::marker::check_workspace_marker;
use crate::error::EngineError;
use crate::kernel::actor::{CallContext, Caller};
use crate::ports::LockHandle;
use crate::vault::index::NoteIndex;

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
    let index = reconcile_index(vault, marker_required)?;
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
