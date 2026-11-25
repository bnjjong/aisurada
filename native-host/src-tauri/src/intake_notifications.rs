//! 저장된 되물음과 사람이 검토할 제보를 열린 vault의 알림 센터에 전달한다.

use crate::{
    lifecycle::HostState,
    notifications::{self, IntakeTarget, Item, NewItem},
    sidecar,
};
use serde::Deserialize;
use std::{thread, time::Duration};
use tauri::{AppHandle, Manager, Runtime};

const POLL: Duration = Duration::from_secs(5);

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReviewReason {
    QuestionLimit,
    NoAnswer,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Notice {
    id: String,
    entry: String,
    at: u64,
    excerpt: String,
    #[serde(default)]
    question: String,
    review_reason: Option<ReviewReason>,
    manager: String,
    label: String,
    url: Option<String>,
}

fn items_from_reply(stdout: &str) -> Vec<Item> {
    let Ok(notices) = serde_json::from_str::<Vec<Notice>>(stdout) else {
        return Vec::new();
    };
    notices
        .into_iter()
        .filter_map(|notice| {
            let excerpt = notice
                .excerpt
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            let (kind, title, body) = match notice.review_reason {
                Some(reason) => (
                    "intake-review",
                    match reason {
                        ReviewReason::NoAnswer => "3일 동안 답이 없는 제보가 있어요",
                        ReviewReason::QuestionLimit => "두 번 질문한 제보를 검토해 주세요",
                    }
                    .into(),
                    if excerpt.is_empty() {
                        notice.label
                    } else {
                        excerpt
                    },
                ),
                None => {
                    let mut body = Vec::new();
                    if !excerpt.is_empty() {
                        body.push(format!("「{excerpt}」"));
                    }
                    body.push(
                        notice
                            .question
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join(" "),
                    );
                    body.push("원장에서 답글 초안을 확인해 주세요.".into());
                    (
                        "intake-question",
                        format!("{}가 제보에 물어볼 내용을 남겼어요", notice.label),
                        body.join(" · "),
                    )
                }
            };
            notifications::build_item(
                NewItem {
                    kind: kind.into(),
                    title,
                    body,
                    task_key: None,
                    note_ref: None,
                    intake: Some(IntakeTarget {
                        manager: notice.manager,
                        entry: notice.entry,
                        url: notice.url,
                    }),
                    id: Some(notice.id),
                },
                notice.at,
                0,
            )
            .ok()
        })
        .collect()
}

fn check<R: Runtime>(app: &AppHandle<R>) {
    let host = app.state::<HostState>();
    let Some(vault) = host.current_vault() else {
        return;
    };
    let argv = ["manager".to_string(), "intake-questions".to_string()];
    let Ok(reply) = sidecar::call_blocking(app, &vault, &argv) else {
        return;
    };
    if reply.status != 0 {
        return;
    }
    let items = items_from_reply(&reply.stdout);
    let _ = notifications::deliver_intake_questions(app, &host, &vault, items);
}

pub fn install<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    let _ = thread::Builder::new()
        .name("intake-notify".into())
        .spawn(move || loop {
            check(&app);
            thread::sleep(POLL);
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direct_and_slack_questions_keep_the_saved_timestamp_and_original_link() {
        let rows = items_from_reply(
            r#"[
          {"id":"intake-question-direct","entry":"intake-0123456789abcdef","at":1700000000000,"excerpt":"화면 오류","question":"어느 화면인가요?","manager":"demo","label":"Hermes"},
          {"id":"intake-question-slack","entry":"CDEMO123:1700000000.000000","at":1700000000000,"excerpt":"화면 오류","question":"어느 화면인가요?","manager":"demo","label":"Hermes","url":"https://demo.slack.com/archives/CDEMO123/p1700000000000000"},
          {"id":"intake-review-demo","entry":"intake-0123456789abcdef","at":1700259200000,"excerpt":"화면 오류","reviewReason":"no_answer","manager":"demo","label":"Hermes"}
        ]"#,
        );
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].at, 1_700_000_000_000);
        assert!(rows[0].body.contains("어느 화면인가요?"));
        assert_eq!(rows[0].intake.as_ref().unwrap().url, None);
        assert!(rows[1].intake.as_ref().unwrap().url.is_some());
        assert_eq!(rows[2].kind, "intake-review");
        assert_eq!(rows[2].at, 1_700_259_200_000);
        assert_eq!(rows[2].body, "화면 오류");
    }
}
