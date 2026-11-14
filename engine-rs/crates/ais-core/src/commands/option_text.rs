//! 명령줄 옵션처럼 글로 다루는 인자 도우미. 호출 객체의 값(글, 참거짓, 숫자, 배열·객체)을 명령줄에서 오는 글로 바꾼다.

use serde_json::Value as Json;

/// 호출 객체의 값을 명령줄 옵션 글로 바꾼다. 글은 그대로, 참거짓·숫자는 글자로, 배열·객체는 압축 JSON 글로 만든다. `null`은 안 준 것이다.
pub fn text_of(value: &Option<Json>) -> Option<String> {
    match value {
        None | Some(Json::Null) => None,
        Some(Json::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}
