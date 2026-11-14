//! 쓰기 명령이 함께 쓰는 공통 기반입니다.
//!
//! - [`apparatus`]: 시험용 시계와 난수, id 발급
//! - [`confine`]: 경로 조각 검사와 기준 폴더 가두기
//! - [`atomic`]: 임시 파일에 쓰고 이름을 바꿔 한 번에 교체하는 쓰기
//! - [`lock`]: vault별 쓰기 잠금(프로세스 안 줄 세우기와 프로세스 간 파일 잠금)
//! - [`precondition`]: 고치기 전 확인 값(`f1`) 검사
//! - [`marker`]: 워크스페이스 표식 검사
//! - [`autocommit`]: git 자동 커밋
//! - [`pipeline`]: 쓰기 명령 한 번의 길(잠금 → 표식 → 색인 맞추기 → 자동 커밋 기록)
//! - [`slug`]: 제목에서 폴더·파일 이름(slug) 만들기
//! - [`agenda_file`]: 아젠다 파일 만들기
//! - [`project_file`]: 프로젝트 파일 만들기와 저장소 목록 검사
//! - [`template_file`]: 템플릿 파일 만들기
//! - [`warnings`]: 쓰기 응답의 경고 글
//! - [`note_front`]: 노트 머리(YAML) 만들기
//! - [`meta_writer`]: 노트 머리만 고쳐 쓰기(편집 도장 판정 포함)
//! - [`doc_place`]: 날짜·시각 인자, 부모 폴더, 폴더 이름, 폴더 옮기기
//! - [`first_heading`]: 본문 첫 헤딩 글
//! - [`local_time`]: 이 프로세스의 시간대로 본 날짜와 시각
//! - [`snake_dump`]: SnakeYAML과 같은 YAML 방출기(태스크·자원 파일)
//! - [`json_out`]: 파일에 쓰는 JSON 글(들여쓰기 두 칸)
//! - [`trash_file`]: 휴지통 목록 파일과 보관 폴더 쓰기
//! - [`comments_file`]: 코멘트 파일 쓰기와 앵커 표시 걷어내기
//! - [`sub_note`]: 후속 노트 쓰기의 공통 부품(파일 글, 폴더 옮기기, 하위 페이지 카드 걷어내기, 폴더 지우기)

pub mod agenda_file;
pub mod apparatus;
pub mod atomic;
pub mod autocommit;
pub mod comments_file;
pub mod confine;
pub mod doc_place;
pub mod first_heading;
pub mod json_out;
pub mod local_time;
pub mod lock;
pub mod marker;
pub mod meta_writer;
pub mod note_front;
pub mod pipeline;
pub mod precondition;
pub mod project_file;
pub mod slug;
pub mod snake_dump;
pub mod stamp;
pub mod sub_note;
pub mod template_file;
pub mod trash_file;
pub mod warnings;
