//! 태스크 애그리거트: 태스크의 규칙을 한 곳에 모은 도메인 코드. 파일을 읽고 쓰지 않는다.
//!
//! 명령(`create`, `edit`, `transition_to`, `link`, `comment`)은 **값과 사건**을 돌려준다. 새 상태의 [`Task`]와 일어난 일([`TaskEvent`])이다.
//! 그 값을 파일에 쓰고 사건을 `activity.jsonl`에 붙이는 일은 저장소(`task/store.rs`)가 한다. 그래서 규칙은 디스크 없이 시험할 수 있다.
//!
//! 규칙(바뀌지 않는 조건):
//! - 새 태스크는 요약(title), 저장소(repo), 완료 기준 하나 이상이 있어야 만들 수 있고 Draft로 시작한다.
//! - 고칠 때는 직전에 읽은 확인 값(`f1`)이 지금 파일과 맞아야 한다. 달라지는 것이 없으면 아무것도 하지 않는다.
//! - 상태 전이는 `transitions`의 규칙과 조건을 지키고, 이 태스크를 막고 있는 미완료 태스크가 없어야 시작한다.
//! - 연결은 자기 자신에게 못 걸고, 서로를 막는 순환(`blocks`)을 못 만들며, 태스크마다 `MAX_LINKS`개까지다.

use super::format::{self, DEFAULT_PRIORITY, DEFAULT_TYPE, MAX_LINKS};
use super::transitions;
use super::{Criterion, Link, Ref, Task, TaskStatus};
use crate::error::EngineError;
use crate::json::JsonValue;
use crate::kernel::stamp::Stamp;
use crate::kernel::task_link::TaskLinkKind;
use crate::text::compare_utf16;

/// 고칠 값. `None`은 「안 줌(그대로 둔다)」이다. `repo`, `project`, `base_branch`는 빈 글이 「지운다」이고,
/// 목록 칸(`labels`, `criteria`, `refs`)은 빈 목록이 「비운다」다.
#[derive(Default)]
pub struct Fields {
    pub title: Option<String>,
    pub task_type: Option<String>,
    pub priority: Option<String>,
    pub repo: Option<String>,
    pub project: Option<String>,
    pub base_branch: Option<String>,
    pub body: Option<String>,
    pub labels: Option<Vec<String>>,
    pub criteria: Option<Vec<Criterion>>,
    pub refs: Option<Vec<Ref>>,
}

/// 훑은 결과. `skipped`는 못 읽어 건너뛴 파일과 이유다.
pub struct Scan {
    pub items: Vec<Task>,
    pub skipped: Vec<String>,
}

impl Scan {
    pub fn by_key(&self, key: &str) -> Option<&Task> {
        self.items.iter().find(|t| t.key == key)
    }

    pub fn by_id(&self, id: &str) -> Option<&Task> {
        self.items.iter().find(|t| t.id == id)
    }
}

/// 이 태스크에 걸린 연결 한 줄(건 쪽과 받은 쪽을 모두 담는다)
pub struct Related {
    pub kind: String,
    pub key: String,
    pub title: String,
    pub status: TaskStatus,
}

// ---------------------------------------------------------------- 사건

/// 태스크에서 일어난 일. 활동 기록(`activity.jsonl`)에 한 줄씩 붙는다.
#[derive(Clone, Debug)]
pub struct TaskEvent {
    stamp: Stamp,
    by: String,
    kind: EventKind,
}

#[derive(Clone, Debug)]
pub enum EventKind {
    Created,
    /// 고친 칸 이름들(`title`, `type`, `priority`, `repo`, `project`, `baseBranch`, `body`, `labels`, `criteria`, `refs`)
    Edited {
        fields: Vec<&'static str>,
    },
    Status {
        from: TaskStatus,
        to: TaskStatus,
    },
    Linked(LinkEvent),
    Unlinked(LinkEvent),
    Comment {
        body: String,
    },
}

/// 연결을 걸거나 푼 일. `outgoing`은 이 태스크가 건 쪽(`out`)이면 참, 받은 쪽(`in`)이면 거짓이다.
#[derive(Clone, Debug)]
pub struct LinkEvent {
    pub kind: TaskLinkKind,
    pub with: String,
    pub outgoing: bool,
}

impl TaskEvent {
    fn new(stamp: &Stamp, actor: &str, kind: EventKind) -> Self {
        TaskEvent {
            stamp: stamp.clone(),
            by: actor.to_string(),
            kind,
        }
    }

    /// 활동 파일에 붙는 모양. 칸 차례는 `at`, `kind`, `by`, (`session`, `agent`), 사건마다 다른 칸이다.
    pub fn to_json_value(&self) -> JsonValue {
        let name = match &self.kind {
            EventKind::Created => "created",
            EventKind::Edited { .. } => "edited",
            EventKind::Status { .. } => "status",
            EventKind::Linked(_) => "linked",
            EventKind::Unlinked(_) => "unlinked",
            EventKind::Comment { .. } => "comment",
        };
        let text = |s: &str| JsonValue::Str(s.to_string());
        let mut entries = vec![
            ("at".to_string(), text(&self.stamp.at)),
            ("kind".to_string(), text(name)),
            ("by".to_string(), text(&self.by)),
        ];
        if let Some(session) = &self.stamp.session {
            entries.push(("session".to_string(), text(session)));
        }
        if let Some(agent) = &self.stamp.agent {
            entries.push(("agent".to_string(), text(agent)));
        }
        match &self.kind {
            EventKind::Created => {}
            EventKind::Edited { fields } => entries.push((
                "fields".to_string(),
                JsonValue::Array(fields.iter().map(|f| text(f)).collect()),
            )),
            EventKind::Status { from, to } => {
                entries.push(("from".to_string(), text(from.label())));
                entries.push(("to".to_string(), text(to.label())));
            }
            EventKind::Linked(link) | EventKind::Unlinked(link) => {
                entries.push(("type".to_string(), text(link.kind.as_str())));
                entries.push(("with".to_string(), text(&link.with)));
                entries.push((
                    "direction".to_string(),
                    text(if link.outgoing { "out" } else { "in" }),
                ));
            }
            EventKind::Comment { body } => entries.push(("body".to_string(), text(body))),
        }
        JsonValue::Object(entries)
    }
}

// ---------------------------------------------------------------- 만들기

/// 검사를 마친 새 태스크의 재료. 번호·id·파일 자리는 저장소가 정해 [`NewTask::into_task`]에 넘긴다.
pub struct NewTask {
    title: String,
    task_type: Option<String>,
    priority: Option<String>,
    repo: String,
    pub project: Option<String>,
    base_branch: Option<String>,
    body: Option<String>,
    labels: Option<Vec<String>>,
    criteria: Vec<Criterion>,
    refs: Option<Vec<Ref>>,
}

impl NewTask {
    /// 요약(title), 저장소(repo), 완료 기준(criteria 1개 이상)이 필수다. 검사 차례가 곧 오류가 나오는 차례다.
    pub fn from_fields(f: Fields) -> Result<NewTask, EngineError> {
        let Some(title) = f.title else {
            return Err(EngineError::invalid_argument("title 이 필요하다"));
        };
        let repo = match f.repo {
            Some(r) if !r.is_empty() => r,
            _ => {
                return Err(EngineError::invalid_argument(
                    "repo 가 필요하다(owner/name)",
                ));
            }
        };
        let criteria = match f.criteria {
            Some(c) if !c.is_empty() => c,
            _ => {
                return Err(EngineError::invalid_argument(
                    "완료 기준(criteria)을 하나 이상 써야 만들 수 있다. 일이 끝났는지 이 기준으로 확인한다",
                ));
            }
        };
        Ok(NewTask {
            title,
            task_type: f.task_type,
            priority: f.priority,
            repo,
            project: f.project.filter(|p| !p.is_empty()),
            base_branch: f.base_branch.filter(|b| !b.is_empty()),
            body: f.body,
            labels: f.labels,
            criteria,
            refs: f.refs,
        })
    }

    /// Draft 상태의 새 태스크와 「만들었다」 사건을 돌려준다. `token`은 저장소가 파일을 쓴 뒤에 붙인다.
    pub fn into_task(
        self,
        key: String,
        id: String,
        rel_path: String,
        stamp: &Stamp,
        actor: &str,
    ) -> (Task, TaskEvent) {
        let task = Task {
            key,
            id,
            title: self.title,
            task_type: self.task_type.unwrap_or_else(|| DEFAULT_TYPE.to_string()),
            priority: self
                .priority
                .unwrap_or_else(|| DEFAULT_PRIORITY.to_string()),
            status: TaskStatus::Draft,
            repo: Some(self.repo),
            project: self.project,
            base_branch: self.base_branch,
            labels: self.labels.unwrap_or_default(),
            criteria: normalize_criteria(&[], &self.criteria, stamp, actor),
            links: Vec::new(),
            refs: self.refs.unwrap_or_default(),
            body: self.body.unwrap_or_default(),
            extra: Vec::new(),
            created_at: stamp.at.clone(),
            edited_at: stamp.at.clone(),
            edited_session: stamp.session.clone(),
            edited_agent: stamp.agent.clone(),
            rel_path,
            token: None,
        };
        (task, TaskEvent::new(stamp, actor, EventKind::Created))
    }
}

// ---------------------------------------------------------------- 고치기·상태·연결·댓글

/// 고친 결과: 새 상태(확인 값은 아직 없다)와 일어난 일
pub struct Changed {
    pub next: Task,
    pub event: TaskEvent,
}

/// 연결을 걸거나 푼 결과. 파일은 `next`가 담긴 쪽(건 쪽 또는 반대쪽)에 쓰고, 사건은 두 태스크의 활동에 각각 붙인다(`events`의 차례대로).
pub struct LinkChange {
    pub next: Task,
    pub events: Vec<(String, TaskEvent)>,
}

impl Task {
    /// 태스크의 내용을 고친다. 직전에 읽은 `f1` 토큰이 지금 파일과 맞아야 한다(맞지 않으면 `AIS-412`). 실제로 달라지는 게 없으면 `None`이다.
    /// 상태와 연결은 여기서 못 바꾼다(`transition_to`, `link`). `check_project`는 프로젝트를 바꿀 때만 불리며 그 프로젝트가 있는지 본다
    /// (프로젝트는 다른 컨텍스트라 규칙이 아니라 질의로 받는다).
    pub fn edit(
        &self,
        f: Fields,
        f1: Option<&str>,
        stamp: &Stamp,
        actor: &str,
        check_project: impl FnOnce(&str) -> Result<(), EngineError>,
    ) -> Result<Option<Changed>, EngineError> {
        let cur = self;
        let Some(f1) = f1.filter(|t| !format::java_blank(t)) else {
            return Err(EngineError::precondition_failed(format!(
                "f1 이 필요하다. get-task 로 읽은 precondition 을 f1= 으로 넘긴다: {}",
                cur.rel_path
            )));
        };
        if Some(format::java_trim(f1)) != cur.token.as_deref() {
            return Err(EngineError::precondition_failed(format!(
                "읽은 뒤에 파일이 바뀌었다. 다시 읽고 고친다: {}",
                cur.rel_path
            )));
        }
        let title = f.title.unwrap_or_else(|| cur.title.clone());
        let task_type = f.task_type.unwrap_or_else(|| cur.task_type.clone());
        let priority = f.priority.unwrap_or_else(|| cur.priority.clone());
        let repo = match f.repo {
            Some(r) => Some(r).filter(|r| !r.is_empty()),
            None => cur.repo.clone(),
        };
        let project = match f.project {
            Some(p) => Some(p).filter(|p| !p.is_empty()),
            None => cur.project.clone(),
        };
        let base = match f.base_branch {
            Some(b) => Some(b).filter(|b| !b.is_empty()),
            None => cur.base_branch.clone(),
        };
        let body = f.body.unwrap_or_else(|| cur.body.clone());
        let labels = f.labels.unwrap_or_else(|| cur.labels.clone());
        let criteria = match &f.criteria {
            Some(given) => normalize_criteria(&cur.criteria, given, stamp, actor),
            None => cur.criteria.clone(),
        };
        let refs = f.refs.unwrap_or_else(|| cur.refs.clone());
        if let Some(project) = &project
            && cur.project.as_ref() != Some(project)
        {
            check_project(project)?;
        }

        let same = Task {
            title: title.clone(),
            task_type: task_type.clone(),
            priority: priority.clone(),
            repo: repo.clone(),
            project: project.clone(),
            base_branch: base.clone(),
            labels: labels.clone(),
            criteria: criteria.clone(),
            refs: refs.clone(),
            body: body.clone(),
            ..cur.clone()
        };
        if format::render(&same)? == format::render(cur)? {
            return Ok(None);
        }
        let next = Task {
            edited_at: stamp.at.clone(),
            edited_session: stamp.session.clone(),
            edited_agent: stamp.agent.clone(),
            token: None,
            ..same
        };

        let mut fields: Vec<&'static str> = Vec::new();
        if title != cur.title {
            fields.push("title");
        }
        if task_type != cur.task_type {
            fields.push("type");
        }
        if priority != cur.priority {
            fields.push("priority");
        }
        if repo != cur.repo {
            fields.push("repo");
        }
        if project != cur.project {
            fields.push("project");
        }
        if base != cur.base_branch {
            fields.push("baseBranch");
        }
        if body != cur.body {
            fields.push("body");
        }
        if labels != cur.labels {
            fields.push("labels");
        }
        if criteria != cur.criteria {
            fields.push("criteria");
        }
        if refs != cur.refs {
            fields.push("refs");
        }
        Ok(Some(Changed {
            next,
            event: TaskEvent::new(stamp, actor, EventKind::Edited { fields }),
        }))
    }

    /// 상태를 바꾼다. 규칙과 조건은 `transitions`가 검사한다. `expected_from`을 주면 지금 상태가 그것과 같을 때만 바꾼다(그 사이에
    /// 러너나 다른 창이 먼저 바꿨으면 `AIS-409`). `blockers`는 이 태스크를 막고 있는 미완료 태스크 키를 구하는 질의이고,
    /// 기대한 상태를 검사한 **뒤에** 부른다(다른 태스크를 읽어야 해서다).
    pub fn transition_to(
        &self,
        to: TaskStatus,
        actor: &str,
        expected_from: Option<&str>,
        blockers: impl FnOnce() -> Vec<String>,
        stamp: &Stamp,
    ) -> Result<Changed, EngineError> {
        let cur = self;
        if let Some(expected) = expected_from {
            let Some(exp) = TaskStatus::parse(expected) else {
                return Err(EngineError::invalid_argument(format!(
                    "from 은 상태 이름이어야 한다: {expected}"
                )));
            };
            if exp != cur.status {
                return Err(EngineError::conflict(format!(
                    "상태가 이미 바뀌었다: 지금 {}, 기대한 것 {}",
                    cur.status.label(),
                    exp.label()
                )));
            }
        }
        transitions::require(cur, to, actor, &blockers())?;
        let next = Task {
            status: to,
            edited_at: stamp.at.clone(),
            edited_session: stamp.session.clone(),
            edited_agent: stamp.agent.clone(),
            token: None,
            ..cur.clone()
        };
        Ok(Changed {
            next,
            event: TaskEvent::new(
                stamp,
                actor,
                EventKind::Status {
                    from: cur.status,
                    to,
                },
            ),
        })
    }

    /// `from`이 `to`에 연결을 걸거나 푼다. 연결은 건 쪽 파일에만 적는다. 푼다(`remove`)면 두 방향 가운데 있는 쪽을 지운다.
    /// 새 연결은 `blocks`와 `relates`를 받는다. 기존 `duplicates`는 해제할 수 있다.
    /// 자기 자신에게 걸 수 없고, 막고 있음(`blocks`)은 서로를 막는 순환을 만들 수 없다. 이미 있는 연결을 다시 걸면 바뀐 게 없다(`None`).
    pub fn link(
        from: &Task,
        to: &Task,
        kind: &str,
        remove: bool,
        stamp: &Stamp,
        actor: &str,
    ) -> Result<Option<LinkChange>, EngineError> {
        let Some(kind) = TaskLinkKind::parse(kind) else {
            return Err(EngineError::invalid_argument(format!(
                "type 은 [{}] 가운데 하나여야 한다: {kind}",
                TaskLinkKind::NAMES_TEXT
            )));
        };
        if from.key == to.key {
            return Err(EngineError::invalid_argument(format!(
                "자기 자신에게는 연결할 수 없다: {}",
                from.key
            )));
        }
        let forward = Link {
            kind,
            to: to.key.clone(),
        };
        let backward = Link {
            kind,
            to: from.key.clone(),
        };
        let (owner, links, unlinked): (&Task, Vec<Link>, bool);
        if remove {
            let (chosen, target) = if from.links.contains(&forward) {
                (from, &forward)
            } else if to.links.contains(&backward) {
                (to, &backward)
            } else {
                return Ok(None);
            };
            let mut list = chosen.links.clone();
            if let Some(at) = list.iter().position(|l| l == target) {
                list.remove(at);
            }
            owner = chosen;
            links = list;
            unlinked = true;
        } else {
            if kind == TaskLinkKind::Duplicates {
                return Err(EngineError::invalid_argument(
                    "중복 연결은 새로 등록할 수 없다",
                ));
            }
            if from.links.contains(&forward) {
                return Ok(None);
            }
            if kind == TaskLinkKind::Relates && to.links.contains(&backward) {
                return Ok(None);
            }
            if kind == TaskLinkKind::Blocks
                && to.links.contains(&Link {
                    kind: TaskLinkKind::Blocks,
                    to: from.key.clone(),
                })
            {
                return Err(EngineError::invalid_argument(format!(
                    "{} 가 이미 {} 를 막고 있어 서로 막게 할 수 없다",
                    to.key, from.key
                )));
            }
            if from.links.len() >= MAX_LINKS {
                return Err(EngineError::invalid_argument(format!(
                    "연결은 태스크마다 {MAX_LINKS}개까지다"
                )));
            }
            let mut list = from.links.clone();
            list.push(forward);
            owner = from;
            links = list;
            unlinked = false;
        }
        let next = Task {
            links,
            edited_at: stamp.at.clone(),
            edited_session: stamp.session.clone(),
            edited_agent: stamp.agent.clone(),
            token: None,
            ..owner.clone()
        };
        let events = [from, to]
            .into_iter()
            .map(|side| {
                let other = if side.key == from.key {
                    &to.key
                } else {
                    &from.key
                };
                let link = LinkEvent {
                    kind,
                    with: other.clone(),
                    outgoing: side.key == owner.key,
                };
                let event = if unlinked {
                    EventKind::Unlinked(link)
                } else {
                    EventKind::Linked(link)
                };
                (side.key.clone(), TaskEvent::new(stamp, actor, event))
            })
            .collect();
        Ok(Some(LinkChange { next, events }))
    }

    /// 댓글 사건. 댓글은 태스크 파일을 바꾸지 않고 활동에만 남는다.
    pub fn comment(&self, body: &str, stamp: &Stamp, actor: &str) -> TaskEvent {
        TaskEvent::new(
            stamp,
            actor,
            EventKind::Comment {
                body: body.to_string(),
            },
        )
    }
}

/// 완료 기준을 다듬는다. 새로 확인된 항목에는 확인한 시각과 쪽을 적고, 확인이 풀린 항목은 그 기록을 지운다. 이미 확인된 항목은 기록을 그대로 둔다.
fn normalize_criteria(
    old: &[Criterion],
    given: &[Criterion],
    stamp: &Stamp,
    actor: &str,
) -> Vec<Criterion> {
    given
        .iter()
        .map(|c| {
            if !c.done {
                return Criterion {
                    text: c.text.clone(),
                    done: false,
                    checked_at: None,
                    checked_by: None,
                };
            }
            if let Some(before) = old.iter().find(|o| o.done && o.text == c.text) {
                return before.clone();
            }
            Criterion {
                text: c.text.clone(),
                done: true,
                checked_at: Some(c.checked_at.clone().unwrap_or_else(|| stamp.at.clone())),
                checked_by: Some(c.checked_by.clone().unwrap_or_else(|| actor.to_string())),
            }
        })
        .collect()
}

// ---------------------------------------------------------------- 도메인 서비스(여러 태스크에 걸친 규칙)

/// 이 태스크를 막고 있으면서 아직 Done이 아닌 태스크 키들. 비면 막히지 않았다.
pub fn unfinished_blockers(scan: &Scan, key: &str) -> Vec<String> {
    scan.items
        .iter()
        .filter(|t| t.status != TaskStatus::Done)
        .filter(|t| {
            t.links
                .iter()
                .any(|l| l.kind == TaskLinkKind::Blocks && l.to == key)
        })
        .map(|t| t.key.clone())
        .collect()
}

/// 이 태스크에 걸린 모든 연결. 이 태스크가 건 것과, 다른 태스크가 이 태스크에 건 것(반대쪽 이름)을 함께 낸다. 없는 태스크를 가리키면 뺀다.
pub fn related(scan: &Scan, task: &Task) -> Vec<Related> {
    let mut out: Vec<Related> = Vec::new();
    for link in &task.links {
        if let Some(other) = scan.by_key(&link.to) {
            out.push(Related {
                kind: link.kind.as_str().to_string(),
                key: other.key.clone(),
                title: other.title.clone(),
                status: other.status,
            });
        }
    }
    for other in &scan.items {
        if other.key == task.key {
            continue;
        }
        for link in &other.links {
            if link.to != task.key {
                continue;
            }
            let inverse = link.kind.inverse_label();
            if link.kind == TaskLinkKind::Relates
                && task
                    .links
                    .iter()
                    .any(|x| x.kind == TaskLinkKind::Relates && x.to == other.key)
            {
                continue;
            }
            out.push(Related {
                kind: inverse.to_string(),
                key: other.key.clone(),
                title: other.title.clone(),
                status: other.status,
            });
        }
    }
    out.sort_by(|a, b| compare_utf16(&a.kind, &b.kind).then_with(|| compare_utf16(&a.key, &b.key)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use crate::json::compact;

    fn stamp(at: &str) -> Stamp {
        Stamp {
            at: at.to_string(),
            session: None,
            agent: None,
        }
    }

    fn criteria(texts: &[(&str, bool)]) -> Vec<Criterion> {
        texts
            .iter()
            .map(|(text, done)| Criterion {
                text: text.to_string(),
                done: *done,
                checked_at: None,
                checked_by: None,
            })
            .collect()
    }

    fn new_fields() -> Fields {
        Fields {
            title: Some("제목".into()),
            repo: Some("owner/name".into()),
            criteria: Some(criteria(&[("하나", false), ("둘", true)])),
            ..Fields::default()
        }
    }

    /// 디스크 없이 만든 태스크(확인 값은 `f1:x`)
    fn task(key: &str) -> Task {
        let (mut task, _) = NewTask::from_fields(new_fields()).unwrap().into_task(
            key.to_string(),
            format!("tk{key}-00000001"),
            format!(".aisurada/tasks/{key}.md"),
            &stamp("2026-11-16T09:00:00+09:00"),
            "human",
        );
        task.token = Some("f1:x".into());
        task
    }

    #[test]
    fn a_new_task_needs_title_repo_and_criteria_in_that_order() {
        let err = |f: Fields| NewTask::from_fields(f).err().unwrap().message;
        assert_eq!(err(Fields::default()), "title 이 필요하다");
        let with_title = || Fields {
            title: Some("t".into()),
            ..Fields::default()
        };
        assert_eq!(err(with_title()), "repo 가 필요하다(owner/name)");
        let with_repo = Fields {
            repo: Some("a/b".into()),
            ..with_title()
        };
        assert!(err(with_repo).starts_with("완료 기준(criteria)을 하나 이상"));
    }

    #[test]
    fn creating_starts_in_draft_and_records_who_checked_done_criteria() {
        let (task, event) = NewTask::from_fields(new_fields()).unwrap().into_task(
            "TSK-1".into(),
            "tkaa-00000001".into(),
            ".aisurada/tasks/TSK-1.md".into(),
            &stamp("2026-11-16T09:00:00+09:00"),
            "human",
        );
        assert_eq!(task.status, TaskStatus::Draft);
        assert_eq!(task.task_type, DEFAULT_TYPE);
        assert_eq!(task.priority, DEFAULT_PRIORITY);
        assert_eq!(task.criteria[0].checked_by, None);
        assert_eq!(task.criteria[1].checked_by.as_deref(), Some("human"));
        assert_eq!(
            compact(&event.to_json_value()),
            r#"{"at":"2026-11-16T09:00:00+09:00","kind":"created","by":"human"}"#
        );
    }

    #[test]
    fn editing_needs_the_token_and_does_nothing_when_nothing_changes() {
        let cur = task("TSK-1");
        let s = stamp("2026-11-16T10:00:00+09:00");
        let none = |_: &str| Ok(());
        // f1이 없거나 틀리면 거절
        let err = cur
            .edit(Fields::default(), None, &s, "human", none)
            .err()
            .unwrap();
        assert_eq!(err.kind, ErrorKind::PreconditionFailed);
        let err = cur
            .edit(Fields::default(), Some("f1:other"), &s, "human", none)
            .err()
            .unwrap();
        assert!(err.message.starts_with("읽은 뒤에 파일이 바뀌었다"));
        // 같은 값으로 고치면 바뀐 것이 없다
        let same = Fields {
            title: Some("제목".into()),
            ..Fields::default()
        };
        assert!(
            cur.edit(same, Some("f1:x"), &s, "human", none)
                .unwrap()
                .is_none()
        );
        // 달라지면 새 상태와 고친 칸 이름이 사건에 든다
        let changed = Fields {
            title: Some("새 제목".into()),
            priority: Some("high".into()),
            ..Fields::default()
        };
        let edit = cur
            .edit(changed, Some("f1:x"), &s, "human", none)
            .unwrap()
            .unwrap();
        assert_eq!(edit.next.title, "새 제목");
        assert_eq!(
            edit.next.edited_at.as_deref(),
            Some("2026-11-16T10:00:00+09:00")
        );
        assert_eq!(edit.next.token, None);
        assert_eq!(
            compact(&edit.event.to_json_value()),
            r#"{"at":"2026-11-16T10:00:00+09:00","kind":"edited","by":"human","fields":["title","priority"]}"#
        );
    }

    #[test]
    fn the_project_is_checked_only_when_it_changes() {
        let cur = task("TSK-1");
        let s = stamp("2026-11-16T10:00:00+09:00");
        let mut asked = Vec::new();
        let set = |p: &str| Fields {
            project: Some(p.into()),
            ..Fields::default()
        };
        let edit = cur.edit(set("demo"), Some("f1:x"), &s, "human", |p| {
            asked.push(p.to_string());
            Ok(())
        });
        assert!(edit.unwrap().is_some());
        assert_eq!(asked, ["demo"]);
        // 없는 프로젝트면 그 오류가 그대로 나온다
        let missing = cur.edit(set("nope"), Some("f1:x"), &s, "human", |p| {
            Err(EngineError::not_found(format!("프로젝트를 못 찾았다: {p}")))
        });
        assert_eq!(missing.err().unwrap().kind, ErrorKind::NotFound);
    }

    #[test]
    fn a_transition_checks_the_expected_state_before_it_asks_for_blockers() {
        let cur = task("TSK-1");
        let s = stamp("2026-11-16T11:00:00+09:00");
        let mut asked = false;
        let err = cur
            .transition_to(
                TaskStatus::Ready,
                "human",
                Some("in-progress"),
                || {
                    asked = true;
                    Vec::new()
                },
                &s,
            )
            .err()
            .unwrap();
        assert_eq!(err.kind, ErrorKind::Conflict);
        assert!(!asked, "기대한 상태가 틀리면 막힘을 묻지 않는다");
        let ok = cur
            .transition_to(TaskStatus::Ready, "human", Some("draft"), Vec::new, &s)
            .unwrap();
        assert_eq!(ok.next.status, TaskStatus::Ready);
        assert_eq!(
            compact(&ok.event.to_json_value()),
            r#"{"at":"2026-11-16T11:00:00+09:00","kind":"status","by":"human","from":"draft","to":"ready-for-develop"}"#
        );
    }

    #[test]
    fn a_blocked_task_cannot_start() {
        let s = stamp("2026-11-16T11:00:00+09:00");
        let mut ready = task("TSK-2");
        ready.status = TaskStatus::Ready;
        let err = ready
            .transition_to(
                TaskStatus::InProgress,
                "runner",
                None,
                || vec!["TSK-1".to_string()],
                &s,
            )
            .err()
            .unwrap();
        assert!(
            err.message
                .starts_with("다른 태스크에 막혀 있어 시작하지 못한다")
        );
    }

    #[test]
    fn links_follow_their_rules_and_record_both_sides() {
        let s = stamp("2026-11-16T12:00:00+09:00");
        let (a, b) = (task("TSK-1"), task("TSK-2"));
        // 자기 자신에게는 못 건다
        assert!(Task::link(&a, &a, "blocks", false, &s, "human").is_err());
        // 모르는 종류
        assert!(Task::link(&a, &b, "x", false, &s, "human").is_err());
        // 걸면 건 쪽 파일이 바뀌고 두 태스크의 활동에 사건이 남는다
        let change = Task::link(&a, &b, "blocks", false, &s, "human")
            .unwrap()
            .unwrap();
        assert_eq!(change.next.key, "TSK-1");
        assert_eq!(change.next.links.len(), 1);
        let directions: Vec<String> = change
            .events
            .iter()
            .map(|(key, e)| {
                format!(
                    "{key}:{}",
                    compact(&e.to_json_value()).rsplit(',').next().unwrap()
                )
            })
            .collect();
        assert_eq!(
            directions,
            [r#"TSK-1:"direction":"out"}"#, r#"TSK-2:"direction":"in"}"#]
        );
        // 같은 연결을 다시 걸면 바뀐 게 없다
        let a2 = change.next;
        assert!(
            Task::link(&a2, &b, "blocks", false, &s, "human")
                .unwrap()
                .is_none()
        );
        // 서로를 막는 순환은 못 만든다
        let err = Task::link(&b, &a2, "blocks", false, &s, "human")
            .err()
            .unwrap();
        assert!(err.message.contains("서로 막게 할 수 없다"));
        // 푸는 것은 두 방향 가운데 있는 쪽을 지운다(받는 쪽에서 불러도 건 쪽 파일이 바뀐다)
        let removed = Task::link(&b, &a2, "blocks", true, &s, "human")
            .unwrap()
            .unwrap();
        assert_eq!(removed.next.key, "TSK-1");
        assert!(removed.next.links.is_empty());
        // 없는 연결을 풀면 바뀐 게 없다
        assert!(
            Task::link(&a, &b, "blocks", true, &s, "human")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn stored_duplicate_links_can_be_read_and_removed() {
        let stamp = stamp("2026-11-16T12:00:00+09:00");
        let (mut duplicate, original) = (task("TSK-1"), task("TSK-2"));
        duplicate.id = "tkfx0001-00000001".into();
        assert!(Task::link(&duplicate, &original, "duplicates", false, &stamp, "human").is_err());

        duplicate.links.push(Link {
            kind: TaskLinkKind::Duplicates,
            to: original.key.clone(),
        });
        let bytes = format::render(&duplicate).unwrap();
        let stored = format::parse(&bytes, &duplicate.rel_path).unwrap();
        let scan = Scan {
            items: vec![stored, original],
            skipped: Vec::new(),
        };
        let duplicate = scan.by_key("TSK-1").unwrap();
        let original = scan.by_key("TSK-2").unwrap();
        assert_eq!(related(&scan, duplicate)[0].kind, "duplicates");
        assert_eq!(related(&scan, original)[0].kind, "duplicated-by");

        let change = Task::link(original, duplicate, "duplicates", true, &stamp, "human")
            .unwrap()
            .unwrap();
        assert_eq!(change.next.key, duplicate.key);
        assert!(change.next.links.is_empty());
    }

    #[test]
    fn unfinished_blockers_ignore_done_tasks_and_related_names_the_other_side() {
        let s = stamp("2026-11-16T12:00:00+09:00");
        let (a, b) = (task("TSK-1"), task("TSK-2"));
        let a = Task::link(&a, &b, "blocks", false, &s, "human")
            .unwrap()
            .unwrap()
            .next;
        let mut scan = Scan {
            items: vec![a, b.clone()],
            skipped: Vec::new(),
        };
        assert_eq!(unfinished_blockers(&scan, "TSK-2"), ["TSK-1"]);
        let rel = related(&scan, &b);
        assert_eq!(rel.len(), 1);
        assert_eq!(
            (rel[0].kind.as_str(), rel[0].key.as_str()),
            ("blocked-by", "TSK-1")
        );
        scan.items[0].status = TaskStatus::Done;
        assert!(unfinished_blockers(&scan, "TSK-2").is_empty());
    }

    #[test]
    fn a_comment_changes_no_task_file_and_is_one_event() {
        let cur = task("TSK-1");
        let event = cur.comment("본문", &stamp("2026-11-16T13:00:00+09:00"), "human");
        assert_eq!(
            compact(&event.to_json_value()),
            r#"{"at":"2026-11-16T13:00:00+09:00","kind":"comment","by":"human","body":"본문"}"#
        );
    }
}
