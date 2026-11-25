//! 저장된 되물음을 현재 vault의 알림 센터로 전달한다.

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
struct Question {
    id: String,
    entry: String,
    at: u64,
    excerpt: String,
    question: String,
    manager: String,
    label: String,
    url: Option<String>,
}

fn items_from_reply(stdout: &str) -> Vec<Item> {
    let Ok(questions) = serde_json::from_str::<Vec<Question>>(stdout) else {
        return Vec::new();
    };
    questions
        .into_iter()
        .filter_map(|question| {
            let mut body = Vec::new();
            let excerpt = question
                .excerpt
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if !excerpt.is_empty() {
                body.push(format!("「{excerpt}」"));
            }
            body.push(
                question
                    .question
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            body.push("원장에서 답글 초안을 확인해 주세요.".into());
            notifications::build_item(
                NewItem {
                    kind: "intake-question".into(),
                    title: format!("{}가 제보에 물어볼 내용을 남겼어요", question.label),
                    body: body.join(" · "),
                    task_key: None,
                    note_ref: None,
                    intake: Some(IntakeTarget {
                        manager: question.manager,
                        entry: question.entry,
                        url: question.url,
                    }),
                    id: Some(question.id),
                },
                question.at,
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
          {"id":"intake-question-slack","entry":"CDEMO123:1700000000.000000","at":1700000000000,"excerpt":"화면 오류","question":"어느 화면인가요?","manager":"demo","label":"Hermes","url":"https://demo.slack.com/archives/CDEMO123/p1700000000000000"}
        ]"#,
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].at, 1_700_000_000_000);
        assert!(rows[0].body.contains("어느 화면인가요?"));
        assert_eq!(rows[0].intake.as_ref().unwrap().url, None);
        assert!(rows[1].intake.as_ref().unwrap().url.is_some());
    }
}
