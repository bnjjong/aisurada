//! AI가 노트에 쓴 「정할 것」(결정 블록)이 사람의 답을 기다리면 알린다.
//!
//! AI는 결정 블록을 쓴 뒤 `ais-cli wait-decisions`로 사람의 답을 기다리며 멈춰 있다. 사람이 채팅을 안 보고 있어도 알 수 있게, 앱이
//! 켜져 있는 동안 몇십 초마다 엔진에 `pending-decisions`를 묻고 아직 알리지 않은 묶음마다 알림 센터에 「AI가 답을 기다려요」를
//! 쌓는다(설정에 따라 macOS 배너와 토스트도 뜬다, 2026-11-20 사용자 결정). 알림 id에 묻는 결정들의 지문을 넣어
//! 같은 묶음은 한 번만, AI가 새 묶음을 물으면 다시 알린다. 레일 배지는 화면이 따로 읽는다(`ui/decisionsStore.ts`).
use crate::{
    lifecycle::HostState,
    notifications::{self, NewItem},
    sidecar,
};
use std::{thread, time::Duration};
use tauri::{AppHandle, Manager, Runtime};

/// 엔진에 묻는 간격
pub const POLL: Duration = Duration::from_secs(30);
/// 앱이 뜬 직후 첫 확인까지 기다리는 시간(승인 알림과 겹치지 않게 조금 늦춘다)
const FIRST: Duration = Duration::from_secs(12);
/// 한 번에 쌓는 알림의 상한
const MAX_PER_TICK: usize = 5;

pub const TITLE: &str = "AI가 노트에서 답을 기다려요";

/// `pending-decisions` 응답에서 기다리는 노트를 알림으로 바꾼다. 응답이 JSON이 아니거나 비었으면 빈 목록이다.
pub fn items_from_reply(stdout: &str) -> Vec<NewItem> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(stdout) else {
        return Vec::new();
    };
    let Some(notes) = value.get("notes").and_then(|n| n.as_array()) else {
        return Vec::new();
    };
    notes
        .iter()
        .filter_map(|note| {
            let id = note.get("id")?.as_str().filter(|s| !s.is_empty())?;
            let sub = note
                .get("subId")
                .and_then(|s| s.as_str())
                .filter(|s| !s.is_empty());
            let signature = note.get("signature").and_then(|s| s.as_str()).unwrap_or("");
            let title = note
                .get("title")
                .and_then(|t| t.as_str())
                .filter(|t| !t.trim().is_empty())
                .unwrap_or("제목 없음");
            let total = note.get("total").and_then(|t| t.as_u64()).unwrap_or(0);
            let answered = note.get("answered").and_then(|a| a.as_u64()).unwrap_or(0);
            let body = if answered == 0 {
                format!("{title} · 정할 것 {total}개")
            } else {
                format!("{title} · 정할 것 {total}개 가운데 {answered}개 답함")
            };
            let note_ref = match sub {
                Some(sub) => format!("{id}/{sub}"),
                None => id.to_string(),
            };
            Some(NewItem {
                kind: "decision".to_string(),
                title: TITLE.to_string(),
                body,
                task_key: None,
                id: Some(format!(
                    "decision-{}-{signature}",
                    note_ref.replace('/', "-")
                )),
                note_ref: Some(note_ref),
                intake: None,
            })
        })
        .collect()
}

fn check<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<HostState>() else {
        return;
    };
    let Some(vault) = state.current_vault() else {
        return;
    };
    let argv = ["pending-decisions".to_string()];
    let Ok(reply) = sidecar::call_blocking(app, &vault, &argv) else {
        return;
    };
    if reply.status != 0 {
        return;
    }
    for item in items_from_reply(&reply.stdout)
        .into_iter()
        .take(MAX_PER_TICK)
    {
        let _ = notifications::push(app, &notifications::default_dir(), &vault, item);
    }
}

/// 확인을 되풀이하는 스레드를 띄운다. 앱이 끝나면 프로세스와 함께 끝난다.
pub fn install<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    thread::Builder::new()
        .name("decisions-notify".into())
        .spawn(move || {
            thread::sleep(FIRST);
            loop {
                check(&app);
                thread::sleep(POLL);
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waiting_notes_become_one_notification_per_round() {
        let reply = r#"{"count":2,"notes":[
            {"id":"ndmw1-aaaaaaaa","title":"보기 노트","answered":0,"total":6,"signature":"0123456789abcdef"},
            {"id":"ndmw1-aaaaaaaa","subId":"snmw2-bbbbbbbb","title":"보기 후속 노트","answered":3,"total":8,"signature":"fedcba9876543210"}]}"#;
        let items = items_from_reply(reply);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind, "decision");
        assert_eq!(items[0].title, TITLE);
        assert_eq!(items[0].body, "보기 노트 · 정할 것 6개");
        assert_eq!(items[0].note_ref.as_deref(), Some("ndmw1-aaaaaaaa"));
        assert_eq!(
            items[0].id.as_deref(),
            Some("decision-ndmw1-aaaaaaaa-0123456789abcdef"),
            "same round, same notification id"
        );
        assert_eq!(
            items[1].note_ref.as_deref(),
            Some("ndmw1-aaaaaaaa/snmw2-bbbbbbbb")
        );
        assert_eq!(
            items[1].body,
            "보기 후속 노트 · 정할 것 8개 가운데 3개 답함"
        );
        assert!(items.iter().all(|i| i.task_key.is_none()));
    }

    #[test]
    fn nothing_waiting_or_a_broken_reply_yields_nothing() {
        assert!(items_from_reply(r#"{"count":0,"notes":[]}"#).is_empty());
        assert!(items_from_reply("not json").is_empty());
        assert!(items_from_reply(r#"{"notes":[{"title":"id 없음"}]}"#).is_empty());
        assert_eq!(
            items_from_reply(r#"{"notes":[{"id":"nd1-a","title":" ","total":1,"signature":"s"}]}"#)
                [0]
            .body,
            "제목 없음 · 정할 것 1개"
        );
    }

    #[test]
    fn the_built_item_keeps_the_note_reference() {
        let item = items_from_reply(
            r#"{"notes":[{"id":"nd1-a","subId":"sn2-b","title":"t","total":1,"signature":"s"}]}"#,
        )
        .remove(0);
        let built = notifications::build_item(item, 1, 0).expect("valid");
        assert_eq!(built.note_ref.as_deref(), Some("nd1-a/sn2-b"));
        assert!(
            notifications::build_item(
                NewItem {
                    note_ref: Some("../etc".into()),
                    intake: None,
                    ..items_from_reply(r#"{"notes":[{"id":"nd1-a","total":1}]}"#).remove(0)
                },
                1,
                0
            )
            .is_err(),
            "a note reference outside the id shape is refused"
        );
    }
}
