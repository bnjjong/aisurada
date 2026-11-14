//! 쓰기 명령 한 번의 길. 잠금을 잡고, 워크스페이스 표식을 확인하고, 색인을 맞추고(문제가 있으면 일을 하지 않는다),
//! 쓰기 앞 커밋 여부를 판단한 뒤 명령이 일하게 한다. 범위를 벗어날 때(성공이든 실패든) 쓴 쪽을 자동 커밋 대기 기록에 남기고 잠금을 푼다.

use std::path::Path;
use std::sync::Arc;

use super::autocommit::{AutoCommit, ensure_ticker};
use super::lock::{self, WriteGuard};
use super::marker::check_workspace_marker;
use crate::commands::Caller;
use crate::error::EngineError;
use crate::vault::index::NoteIndex;

/// 쓰기 요청이 누가 보냈는지. AI 도구(`tool`)와 앱은 자동 커밋에서 따로 센다.
#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct WriteContext {
    pub caller: Caller,
    /// AI 도구가 보낸 세션 id
    pub session: Option<String>,
    /// 세션을 연 에이전트 이름(`codex` 따위). 없으면 claude로 본다. 노트에 적는 편집 도장이 읽는다.
    pub session_agent: Option<String>,
}

pub struct WriteScope {
    /// 쓰기용으로 맞춘 색인
    pub index: NoteIndex,
    auto: Arc<AutoCommit>,
    tool: bool,
    session: Option<String>,
    // 맨 마지막에 놓이도록 마지막 필드로 둔다(쓴 쪽 기록이 먼저, 잠금이 나중에 풀린다)
    _guard: WriteGuard,
}

impl WriteScope {
    /// `init_workspace`는 표식이 아직 없는 폴더에서 부르므로 표식 검사를 건너뛴다.
    pub fn begin(
        vault: &Path,
        command: &str,
        context: &WriteContext,
    ) -> Result<WriteScope, EngineError> {
        let guard = lock::acquire(vault, &lock::default_lock_dir())?;
        if command != "init_workspace" {
            check_workspace_marker(vault)?;
        }
        let index = NoteIndex::reconciled(vault)?;
        let auto = AutoCommit::serving(vault);
        ensure_ticker(vault);
        let tool = context.caller == Caller::Ai;
        auto.before_write(tool, context.session.as_deref());
        Ok(WriteScope {
            index,
            auto,
            tool,
            session: context.session.clone(),
            _guard: guard,
        })
    }
}

impl Drop for WriteScope {
    fn drop(&mut self) {
        self.auto.after_write(self.tool, self.session.as_deref());
    }
}
