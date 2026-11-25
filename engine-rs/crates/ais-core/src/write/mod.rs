//! 쓰기 명령이 함께 쓰는 공통 기반입니다.
//!
//! - [`apparatus`]: 시험용 시계와 난수, id 발급, 시간대별 시계 및 고친 기록 도장
//! - [`confine`]: 경로 조각 검사와 기준 폴더 가두기
//! - [`atomic`]: 임시 파일에 쓰고 이름을 바꿔 한 번에 교체하는 쓰기
//! - 쓰기 잠금: `ports::VaultLock`(어댑터 `adapters/flock_lock.rs`: 프로세스 안 줄 세우기와 프로세스 간 파일 잠금)
//! - [`pipeline`]: 쓰기 명령 한 번의 길(잠금 → 표식 검사 → 선결 조건 검사 → 색인 맞추기 → 자동 커밋 기록)
//! - [`autocommit`]: git 자동 커밋
//! - [`slug`]: 제목에서 폴더·파일 이름(slug) 만들기
//! - [`agenda_file`]: 아젠다 파일 만들기
//! - [`project_file`]: 프로젝트 파일 만들기와 저장소 목록 검사
//! - [`template_file`]: 템플릿 파일 만들기
//! - [`warnings`]: 쓰기 응답의 경고 글
//! - [`note_front`]: 노트 머리(YAML) 만들기
//! - [`meta_writer`]: 노트 머리만 고쳐 쓰기(편집 도장 판정 포함)
//! - [`doc_place`]: 날짜·시각 인자, 부모 폴더, 폴더 이름, 폴더 옮기기
//! - [`first_heading`]: 본문 첫 헤딩 글
//! - [`snake_dump`]: SnakeYAML과 같은 YAML 방출기(태스크·자원 파일)
//! - [`json_out`]: 파일에 쓰는 JSON 글(들여쓰기 두 칸)
//! - [`trash_file`]: 휴지통 목록 파일과 보관 폴더 쓰기
//! - [`comments_file`]: 코멘트 파일 쓰기와 앵커 표시 걷어내기
//! - [`sub_note`]: 후속 노트 쓰기의 공통 부품(파일 글, 폴더 옮기기, 하위 페이지 카드 걷어내기, 폴더 지우기)

internal_mods!(
    agenda_file,
    apparatus,
    atomic,
    comments_file,
    doc_place,
    json_out,
    meta_writer,
    pipeline,
    project_file,
    sub_note,
    template_file,
    trash_file,
    warnings,
);
pub mod autocommit;

pub mod marker {
    pub use super::pipeline::check_workspace_marker;
}
pub mod precondition {
    pub use super::pipeline::check_precondition as check;
}
pub mod local_time {
    pub use super::apparatus::{LocalClock, iso_offset_date_time, local_clock};
}
pub mod stamp {
    pub use super::apparatus::{Stamp, stamp_of};
}

#[cfg(feature = "test-support")]
pub use crate::kernel::confine;
#[cfg(not(feature = "test-support"))]
pub(crate) use crate::kernel::confine;
#[cfg(feature = "test-support")]
pub use crate::note::front as note_front;
#[cfg(not(feature = "test-support"))]
pub(crate) use crate::note::front as note_front;
#[cfg(feature = "test-support")]
pub use crate::note::heading as first_heading;
#[cfg(not(feature = "test-support"))]
pub(crate) use crate::note::heading as first_heading;
#[cfg(feature = "test-support")]
pub use crate::note::slug;
#[cfg(not(feature = "test-support"))]
pub(crate) use crate::note::slug;
#[cfg(feature = "test-support")]
pub use ais_kernel::snake_dump;
#[cfg(not(feature = "test-support"))]
pub(crate) use ais_kernel::snake_dump;
pub use atomic::write_atomic;
pub use pipeline::acquire_lock;
