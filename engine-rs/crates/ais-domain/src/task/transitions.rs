//! 태스크 상태 전이의 규칙과 조건. 한 곳에서 검사한다. Java 엔진의 `TaskTransitions`와 같다.

use super::format::java_blank;
use super::{Task, TaskStatus};
use crate::error::EngineError;

pub const HUMAN: &str = "human";
pub const RUNNER: &str = "runner";

/// 상태를 바꾸는 쪽. 사람(화면)이거나 러너다.
pub fn is_actor(actor: &str) -> bool {
    actor == HUMAN || actor == RUNNER
}

const BOTH: &[&str] = &[HUMAN, RUNNER];
const HUMAN_ONLY: &[&str] = &[HUMAN];
const RUNNER_ONLY: &[&str] = &[RUNNER];

pub struct Rule {
    pub from: TaskStatus,
    pub to: TaskStatus,
    pub actors: &'static [&'static str],
}

fn rules() -> Vec<Rule> {
    use TaskStatus::*;
    let mut rules = vec![
        Rule {
            from: Draft,
            to: Ready,
            actors: HUMAN_ONLY,
        },
        Rule {
            from: Ready,
            to: Draft,
            actors: HUMAN_ONLY,
        },
        Rule {
            from: Ready,
            to: InProgress,
            actors: BOTH,
        },
        Rule {
            from: InProgress,
            to: Waiting,
            actors: RUNNER_ONLY,
        },
        Rule {
            from: Waiting,
            to: InProgress,
            actors: BOTH,
        },
        Rule {
            from: InProgress,
            to: InReview,
            actors: RUNNER_ONLY,
        },
        Rule {
            from: InReview,
            to: Done,
            actors: BOTH,
        },
    ];
    for from in [Ready, InProgress, Waiting, InReview] {
        rules.push(Rule {
            from,
            to: Failed,
            actors: RUNNER_ONLY,
        });
    }
    for from in [Draft, Ready, InProgress, Waiting, InReview] {
        rules.push(Rule {
            from,
            to: Cancelled,
            actors: HUMAN_ONLY,
        });
    }
    rules.push(Rule {
        from: Failed,
        to: Draft,
        actors: HUMAN_ONLY,
    });
    rules.push(Rule {
        from: Cancelled,
        to: Draft,
        actors: HUMAN_ONLY,
    });
    rules
}

/// 이 상태에서 갈 수 있는 규칙들
pub fn from(status: TaskStatus) -> Vec<Rule> {
    rules().into_iter().filter(|r| r.from == status).collect()
}

/// Ready For Develop으로 가기 전에 채워야 할 것
pub struct Check {
    pub item: &'static str,
    pub ok: bool,
    pub label: &'static str,
}

pub fn readiness(task: &Task) -> Vec<Check> {
    vec![
        Check {
            item: "title",
            ok: !java_blank(&task.title),
            label: "요약이 있어요",
        },
        Check {
            item: "criteria",
            ok: !task.criteria.is_empty(),
            label: "완료 기준이 1개 이상이에요",
        },
        Check {
            item: "repo",
            ok: task.repo.as_deref().is_some_and(|r| !java_blank(r)),
            label: "저장소를 골랐어요",
        },
    ]
}

fn list_text(items: &[String]) -> String {
    format!("[{}]", items.join(", "))
}

/// `to`로 바꿀 수 있는지 검사한다. 안 되면 이유를 담은 409(actor 글이 틀리면 400)다. `unfinished_blockers`는 이 태스크를 막고 있는 미완료 태스크 키들이다.
pub fn require(
    task: &Task,
    to: TaskStatus,
    actor: &str,
    unfinished_blockers: &[String],
) -> Result<(), EngineError> {
    if !is_actor(actor) {
        return Err(EngineError::invalid_argument(format!(
            "actor 는 human 이나 runner 여야 한다: {actor}"
        )));
    }
    let all = rules();
    let Some(rule) = all.iter().find(|r| r.from == task.status && r.to == to) else {
        let allowed: Vec<String> = all
            .iter()
            .filter(|r| r.from == task.status)
            .map(|r| r.to.label().to_string())
            .collect();
        return Err(EngineError::conflict(format!(
            "{} 에서 {} 로는 바꿀 수 없다. 갈 수 있는 곳: {}",
            task.status.label(),
            to.label(),
            list_text(&allowed)
        )));
    };
    if !rule.actors.contains(&actor) {
        return Err(EngineError::conflict(format!(
            "{} → {} 는 {} 만 바꾼다",
            task.status.label(),
            to.label(),
            rule.actors.join("·")
        )));
    }
    if to == TaskStatus::Ready {
        let missing: Vec<String> = readiness(task)
            .into_iter()
            .filter(|c| !c.ok)
            .map(|c| c.item.to_string())
            .collect();
        if !missing.is_empty() {
            return Err(EngineError::conflict(format!(
                "Ready For Develop 으로 가려면 먼저 채워야 한다: {}",
                list_text(&missing)
            )));
        }
    }
    if task.status == TaskStatus::Ready
        && to == TaskStatus::InProgress
        && !unfinished_blockers.is_empty()
    {
        return Err(EngineError::conflict(format!(
            "다른 태스크에 막혀 있어 시작하지 못한다(그 태스크가 Done 이 되어야 한다): {}",
            list_text(unfinished_blockers)
        )));
    }
    Ok(())
}

/// 이 `actor`가 지금 갈 수 있는 곳과 막힌 까닭
pub struct Choice {
    pub to: TaskStatus,
    pub ok: bool,
    pub reason: Option<String>,
}

pub fn options(task: &Task, actor: &str, unfinished_blockers: &[String]) -> Vec<Choice> {
    from(task.status)
        .into_iter()
        .filter(|rule| rule.actors.contains(&actor))
        .map(
            |rule| match require(task, rule.to, actor, unfinished_blockers) {
                Ok(()) => Choice {
                    to: rule.to,
                    ok: true,
                    reason: None,
                },
                Err(e) => Choice {
                    to: rule.to,
                    ok: false,
                    reason: Some(e.message),
                },
            },
        )
        .collect()
}
