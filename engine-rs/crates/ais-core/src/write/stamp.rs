//! 고친 기록(누가 언제). 태스크와 자원 파일의 `edited*` 칸과 활동에 쓴다.

use super::apparatus::now_millis;
use super::local_time::iso_offset_date_time;
use super::pipeline::WriteContext;

pub use crate::kernel::stamp::Stamp;

/// 지금의 고친 기록. AI 세션이 고쳤으면 세션 id와(claude가 아니면) CLI 이름이 남고, 사람이 고쳤으면 시각만 남는다.
pub fn stamp_of(context: &WriteContext) -> Stamp {
    let session = context
        .session
        .as_deref()
        .filter(|s| !crate::kernel::java::java_blank(s))
        .map(str::to_string);
    let agent = session.as_ref().and_then(|_| {
        context
            .session_agent
            .as_deref()
            .filter(|a| *a != "claude")
            .map(str::to_string)
    });
    Stamp {
        at: iso_offset_date_time(now_millis()),
        session,
        agent,
    }
}
