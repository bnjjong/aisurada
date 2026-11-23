//! 고친 기록(누가 언제). 태스크와 자원 파일의 `edited*` 칸과 활동에 쓴다.
//!
//! 값만 정의한다. 지금 시각과 호출 맥락에서 만드는 `stamp_of`는 쓰기 쪽(`write/stamp.rs`)에 있다.

/// 누가 언제 고쳤는지의 기록. 사람이 고쳤으면 `session`은 `None`이다.
#[derive(Clone, Debug)]
pub struct Stamp {
    pub at: String,
    pub session: Option<String>,
    pub agent: Option<String>,
    pub manager: Option<String>,
}
