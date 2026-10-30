//! Shared chapter 3 transport. Used by attachment writes; PTY commands are added in chapter 14.
#![allow(dead_code)]
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use tauri::{
    ipc::{Channel, InvokeBody, Request},
    Emitter, EventTarget, Runtime,
};

pub const MENU: &str = "wl://menu";
pub const NAVIGATE: &str = "wl://navigate";
pub const NOTE_WINDOWS: &str = "wl://note-windows";
pub const SELF_WRITE: &str = "wl://self-write";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Navigate {
    pub path: String,
    pub rail: Option<String>,
    pub sub: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SelfWrite {
    pub root: String,
    pub rel_path: String,
    pub from: String,
}

pub fn emit_window<R: Runtime, T: Serialize + Clone>(
    app: &tauri::AppHandle<R>,
    label: &str,
    event: &str,
    payload: T,
) -> tauri::Result<()> {
    app.emit_to(EventTarget::webview_window(label), event, payload)
}

pub fn header(request: &Request<'_>, key: &str) -> Result<String, String> {
    decode_header(
        request
            .headers()
            .get(format!("x-wl-{key}"))
            .map(|v| v.as_bytes()),
        key,
    )
}
fn decode_header(value: Option<&[u8]>, key: &str) -> Result<String, String> {
    let name = format!("x-wl-{key}");
    let bytes = value.ok_or_else(|| format!("요청에 {name} 이 없어요"))?;
    if !bytes.is_ascii() {
        return Err(format!("{name} 이 ASCII 가 아니에요"));
    }
    percent_encoding::percent_decode(bytes)
        .decode_utf8()
        .map(|v| v.into_owned())
        .map_err(|_| format!("{name} 을 UTF-8 로 못 읽었어요"))
}

pub fn body(request: &Request<'_>) -> Result<Vec<u8>, String> {
    decode_body(request.body())
}
fn decode_body(value: &InvokeBody) -> Result<Vec<u8>, String> {
    match value {
        InvokeBody::Raw(bytes) => Ok(bytes.clone()),
        InvokeBody::Json(json) => {
            serde_json::from_value(json.clone()).map_err(|_| "본문이 바이트가 아니에요".into())
        }
    }
}
pub fn send_bytes(channel: &Channel<String>, bytes: &[u8]) -> tauri::Result<()> {
    channel.send(STANDARD.encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn window_events_only_reach_the_selected_webview() {
        use tauri::{
            test::{mock_builder, mock_context, noop_assets},
            Listener,
        };
        let app = mock_builder().build(mock_context(noop_assets())).unwrap();
        let main = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let note = tauri::WebviewWindowBuilder::new(&app, "note-test", Default::default())
            .build()
            .unwrap();
        let main_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let note_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let copy = main_count.clone();
        main.listen(MENU, move |_| {
            copy.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });
        let copy = note_count.clone();
        note.listen(MENU, move |_| {
            copy.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        });
        emit_window(app.handle(), "note-test", MENU, "wl_close_tab").unwrap();
        assert_eq!(main_count.load(std::sync::atomic::Ordering::SeqCst), 0);
        assert_eq!(note_count.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
    #[test]
    fn raw_and_json_fallback_keep_exact_bytes() {
        assert_eq!(
            decode_body(&InvokeBody::Raw(vec![0, 255, 7])).unwrap(),
            vec![0, 255, 7]
        );
        assert_eq!(
            decode_body(&InvokeBody::Json(json!([0, 255, 7]))).unwrap(),
            vec![0, 255, 7]
        );
        assert!(decode_body(&InvokeBody::Raw(vec![])).unwrap().is_empty());
        assert!(decode_body(&InvokeBody::Json(json!([])))
            .unwrap()
            .is_empty());
        for bad in [
            json!([-1]),
            json!([256]),
            json!([1.5]),
            json!([true]),
            json!([[1]]),
            json!({}),
            json!(null),
        ] {
            assert!(decode_body(&InvokeBody::Json(bad)).is_err());
        }
    }
    #[test]
    fn header_decoding_keeps_plus_and_requires_ascii_and_utf8() {
        assert_eq!(
            decode_header(Some(b"%ED%95%9C%EA%B8%80+file%2F"), "name").unwrap(),
            "한글+file/"
        );
        assert!(decode_header(None, "name").unwrap_err().contains("없어요"));
        assert!(decode_header(Some("한글".as_bytes()), "name")
            .unwrap_err()
            .contains("ASCII"));
        assert!(decode_header(Some(b"%FF"), "name")
            .unwrap_err()
            .contains("UTF-8"));
    }
    #[test]
    fn channels_encode_arbitrary_chunks_without_decoding_utf8() {
        let received = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let copy = received.clone();
        let channel = Channel::<String>::new(move |body| {
            copy.lock()
                .unwrap()
                .push(body.deserialize::<String>().unwrap());
            Ok(())
        });
        send_bytes(&channel, &[0xED, 0x95]).unwrap();
        send_bytes(&channel, &[0x9C, 0xFF]).unwrap();
        assert_eq!(*received.lock().unwrap(), vec!["7ZU=", "nP8="]);
    }
    #[test]
    fn event_payloads_use_camel_case_and_explicit_nulls() {
        assert_eq!(
            serde_json::to_value(Navigate {
                path: "note/id".into(),
                rail: None,
                sub: None
            })
            .unwrap(),
            json!({"path":"note/id","rail":null,"sub":null})
        );
        assert_eq!(
            serde_json::to_value(SelfWrite {
                root: "/v".into(),
                rel_path: "n".into(),
                from: "main".into()
            })
            .unwrap(),
            json!({"root":"/v","relPath":"n","from":"main"})
        );
    }
}
