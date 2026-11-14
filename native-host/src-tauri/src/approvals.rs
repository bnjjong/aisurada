//! 승인이 필요한 스킬·에이전트 변경(#53)을 사람에게 알린다.
//!
//! AI가 쓴 변경이 승인을 기다리면 그 일이 막히므로, 앱이 켜져 있는 동안 몇십 초마다 엔진에 대기 목록을 묻고 아직 알리지 않은 변경마다
//! 알림 센터에 「승인이 필요해요」를 쌓는다(설정에 따라 배너와 토스트도 뜬다). 알림 id를 변경 id로 정해서 같은 변경은 한 번만 쌓인다.
//! 승인 화면의 배지는 화면이 따로 읽는다(`ui/approvalsStore.ts`). 여기는 창이 닫혀 있어도 알리는 쪽이다.
use crate::{
    lifecycle::HostState,
    notifications::{self, NewItem},
    sidecar,
};
use std::{thread, time::Duration};
use tauri::{AppHandle, Manager, Runtime};

/// 엔진에 대기 목록을 묻는 간격. 엔진을 한 번 부르는 데 자바를 띄우므로 너무 잦지 않게 한다.
pub const POLL: Duration = Duration::from_secs(30);
/// 앱이 뜬 직후 첫 확인까지 기다리는 시간(창과 엔진이 자리 잡도록).
const FIRST: Duration = Duration::from_secs(8);
/// 한 번에 쌓는 알림의 상한. 한꺼번에 많이 올라와도 알림 센터를 덮지 않는다.
const MAX_PER_TICK: usize = 5;

pub const TITLE: &str = "승인이 필요한 변경이 있어요";

/// `list-resource-changes` 응답에서 대기 변경을 알림으로 바꾼다. 응답이 JSON이 아니거나 대기가 없으면 빈 목록이다.
pub fn items_from_reply(stdout: &str) -> Vec<NewItem> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(stdout) else {
        return Vec::new();
    };
    let Some(pending) = value.get("pending").and_then(|p| p.as_array()) else {
        return Vec::new();
    };
    pending
        .iter()
        .filter_map(|c| {
            let id = c.get("id")?.as_str()?;
            if id.is_empty() {
                return None;
            }
            let summary = c.get("summary").and_then(|s| s.as_str()).unwrap_or("스킬·에이전트 변경");
            let fixed = c.get("fixed").and_then(|f| f.as_bool()).unwrap_or(false);
            let body = if fixed { format!("{summary} · 권한이나 참조를 바꾸는 변경이라 꼭 확인해야 해요") } else { summary.to_string() };
            Some(NewItem {
                kind: "approval".to_string(),
                title: TITLE.to_string(),
                body,
                task_key: None,
                id: Some(format!("approval-{id}")),
            })
        })
        .collect()
}

fn check<R: Runtime>(app: &AppHandle<R>) {
    let Some(state) = app.try_state::<HostState>() else { return };
    let vault = match state.vault.lock() {
        Ok(g) => g.clone(),
        Err(_) => None,
    };
    let Some(vault) = vault else { return };
    let argv = ["list-resource-changes".to_string(), "decided=0".to_string()];
    let Ok(reply) = sidecar::call_blocking(app, &vault, &argv) else { return };
    if reply.status != 0 {
        return;
    }
    for item in items_from_reply(&reply.stdout).into_iter().take(MAX_PER_TICK) {
        let _ = notifications::push(app, &notifications::default_dir(), &vault, item);
    }
}

/// 확인을 되풀이하는 스레드를 띄운다. 앱이 끝나면 프로세스와 함께 끝난다.
pub fn install<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    thread::Builder::new()
        .name("approvals-notify".into())
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
    fn pending_changes_become_one_notification_each_with_a_stable_id() {
        let reply = r#"{"counts":{"pending":2,"unapproved":0},"pending":[
            {"id":"pcmw1-aaaaaaaa","summary":"글로벌 스킬 '번역 검수' 고치기(설명)","fixed":false},
            {"id":"pcmw2-bbbbbbbb","summary":"프로젝트 에이전트 '웹' 고치기(권한 read → publish)","fixed":true}],
            "unapproved":[],"decided":[]}"#;
        let items = items_from_reply(reply);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].kind, "approval");
        assert_eq!(items[0].title, TITLE);
        assert_eq!(items[0].id.as_deref(), Some("approval-pcmw1-aaaaaaaa"), "same change, same notification id");
        assert_eq!(items[0].body, "글로벌 스킬 '번역 검수' 고치기(설명)");
        assert!(items[1].body.contains("꼭 확인해야 해요"), "a fixed category is called out: {}", items[1].body);
        assert!(items.iter().all(|i| i.task_key.is_none()));
    }

    #[test]
    fn nothing_pending_or_a_broken_reply_yields_nothing() {
        assert!(items_from_reply(r#"{"counts":{"pending":0},"pending":[],"unapproved":[]}"#).is_empty());
        assert!(items_from_reply("not json").is_empty());
        assert!(items_from_reply(r#"{"pending":[{"summary":"id 없음"},{"id":""}]}"#).is_empty(), "rows without an id are skipped");
        assert_eq!(items_from_reply(r#"{"pending":[{"id":"pcx-00000000"}]}"#)[0].body, "스킬·에이전트 변경", "a missing summary has a fallback");
    }
}
