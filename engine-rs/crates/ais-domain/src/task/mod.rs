//! 태스크(티켓) 파일의 규칙: 모양(`format`), 상태 전이(`transitions`), (vault에서 읽고 쓰는 곳 `store`는 `ais-core`에 있다.)
//! 외부 설명서에 없는 확장이다. 파일은 `.aisurada/tasks/TSK-14.md`(머리말 YAML + 설명)와 `TSK-14/activity.jsonl`(활동)이다.

pub mod aggregate;
pub mod format;
pub mod transitions;

use crate::frontmatter::Value;
use crate::kernel::task_link::TaskLinkKind;

/// 태스크 상태
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TaskStatus {
    Draft,
    Ready,
    InProgress,
    Waiting,
    InReview,
    Done,
    Failed,
    Cancelled,
}

impl TaskStatus {
    pub const ALL: [TaskStatus; 8] = [
        TaskStatus::Draft,
        TaskStatus::Ready,
        TaskStatus::InProgress,
        TaskStatus::Waiting,
        TaskStatus::InReview,
        TaskStatus::Done,
        TaskStatus::Failed,
        TaskStatus::Cancelled,
    ];

    /// 파일과 응답에 적는 이름(`ready-for-develop` 따위)
    pub fn label(self) -> &'static str {
        match self {
            TaskStatus::Draft => "draft",
            TaskStatus::Ready => "ready-for-develop",
            TaskStatus::InProgress => "in-progress",
            TaskStatus::Waiting => "waiting-answer",
            TaskStatus::InReview => "in-review",
            TaskStatus::Done => "done",
            TaskStatus::Failed => "failed",
            TaskStatus::Cancelled => "cancelled",
        }
    }

    /// 사람이 읽는 이름(`Ready For Develop` 따위)
    pub fn title(self) -> &'static str {
        match self {
            TaskStatus::Draft => "Draft",
            TaskStatus::Ready => "Ready For Develop",
            TaskStatus::InProgress => "In Progress",
            TaskStatus::Waiting => "Waiting Answer",
            TaskStatus::InReview => "In Review",
            TaskStatus::Done => "Done",
            TaskStatus::Failed => "Failed",
            TaskStatus::Cancelled => "Cancelled",
        }
    }

    /// 앞뒤 공백(U+0020 이하)을 떼고 소문자로 바꾼 뒤 공백과 밑줄을 `-`로 바꿔 이름을 찾는다. 못 찾으면 `None`이다.
    pub fn parse(raw: &str) -> Option<TaskStatus> {
        let value = raw
            .trim_matches(|c: char| c <= ' ')
            .to_lowercase()
            .replace([' ', '_'], "-");
        TaskStatus::ALL.into_iter().find(|s| s.label() == value)
    }
}

/// 완료 기준 한 줄
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Criterion {
    pub text: String,
    pub done: bool,
    pub checked_at: Option<String>,
    pub checked_by: Option<String>,
}

/// 태스크끼리의 연결(건 쪽 파일에만 적는다). `kind`는 `blocks`, `relates`, `duplicates`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    pub kind: TaskLinkKind,
    pub to: String,
}

/// 태스크가 가리키는 자원(노트, GitHub 이슈·PR 또는 Slack 제보)
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ref {
    pub kind: String,
    pub target: String,
    pub title: Option<String>,
    pub reporter: Option<String>,
    pub excerpt: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Task {
    pub key: String,
    pub id: String,
    pub title: String,
    pub task_type: String,
    pub priority: String,
    pub status: TaskStatus,
    pub repo: Option<String>,
    pub project: Option<String>,
    pub base_branch: Option<String>,
    pub manager: Option<String>,
    pub labels: Vec<String>,
    pub criteria: Vec<Criterion>,
    pub links: Vec<Link>,
    pub refs: Vec<Ref>,
    pub body: String,
    /// 머리말의 모르는 칸(그대로 보존한다)
    pub extra: Vec<(String, Value)>,
    pub created_at: String,
    pub edited_at: String,
    pub edited_session: Option<String>,
    pub edited_agent: Option<String>,
    /// vault 기준 상대 경로
    pub rel_path: String,
    /// 파일 바이트의 확인 값(`f1:` + 해시). 읽을 때 붙는다
    pub token: Option<String>,
}

impl Task {
    /// `TSK-14`의 번호. 키 모양이 아니면 0이다.
    pub fn number(&self) -> u32 {
        format::number_of(&self.key)
    }
}
