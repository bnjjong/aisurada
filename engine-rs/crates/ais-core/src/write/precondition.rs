//! 고치기 전 확인 값(`f1`) 검사. 읽은 뒤 그 파일이 그 사이 바뀌지 않았는지 가려 덮어쓰기를 막습니다.
//!
//! 검사 순서: 본문 파일을 읽을 수 있는가(500) → 확인 값이 왔는가(412) → 모양이 맞는가(412) → 지금 파일과 같은가(412).

use crate::error::EngineError;
use crate::kernel::token::compute_token;
use crate::vault::scanner::NoteRecord;

/// 본문 파일 크기 상한
const MAX_NOTE_FILE_BYTES: u64 = 1_000_000;

/// 확인 값과 본문 파일을 검사하고 파일 전체 바이트를 돌려준다.
///
/// `provided_token`이 없거나 비면 없는 것으로 본다(JSON 노트는 면제). `action`은 실패 문구에 들어갈 동작 조각이다
/// (예: `"메타를 저장하지"`, `"고치지"`).
pub fn check(
    vault_root: &std::path::Path,
    note: &NoteRecord,
    provided_token: Option<&str>,
    action: &str,
) -> Result<Vec<u8>, EngineError> {
    let path = &note.content_file;
    let rel = path
        .strip_prefix(vault_root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let unreadable =
        || EngineError::internal(format!("노트 본문 파일을 못 읽어 {action} 않았다: {rel}"));

    // 없음, 1,000,000바이트 초과, 엄격한 UTF-8이 아님, 입출력 오류
    let metadata = crate::ports::fs::metadata(path).map_err(|_| unreadable())?;
    if !metadata.is_file() || metadata.len() > MAX_NOTE_FILE_BYTES {
        return Err(unreadable());
    }
    let bytes = crate::ports::fs::read(path).map_err(|_| unreadable())?;
    if std::str::from_utf8(&bytes).is_err() {
        return Err(unreadable());
    }

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let is_json = name == "note.json"
        || (name != "comments.json" && name != "ask.json" && name.ends_with(".json"));

    let Some(token) = provided_token.filter(|t| !t.is_empty()) else {
        if is_json {
            return Ok(bytes);
        }
        return Err(EngineError::precondition_failed(format!(
            "선결 조건 토큰이 안 왔어요. 아무것도 쓰지 않았습니다: {rel}\nget_doc 으로 이 노트를 읽고, 그 응답의 precondition 을 f1 인자에 그대로 실어 다시 부릅니다.\n방금 만들거나 방금 고친 노트면 그 쓰기 응답이 이미 다음 토큰을 줍니다. 다시 읽지 말고 그 값을 씁니다."
        )));
    };
    if !token.starts_with(crate::kernel::token::PREFIX) {
        return Err(EngineError::precondition_failed(format!(
            "선결 조건 토큰의 모양이 아니에요. 아무것도 쓰지 않았습니다: {rel}\nget_doc 응답의 precondition 을 글자 그대로 f1 인자에 되돌립니다. 계산해서 만드는 값이 아닙니다.\n해시를 직접 구해 넣지 않습니다. 이 대조가 묻는 것은 그 파일을 방금 읽었는가 입니다."
        )));
    }
    if token != compute_token(&bytes) {
        return Err(EngineError::precondition_failed(format!(
            "다른 쪽이 이 노트를 먼저 고쳤어요. 아무것도 쓰지 않았습니다: {rel}\nget_doc 으로 다시 읽고, 그 새 본문 위에 자기 변경을 다시 얹은 뒤, 그 응답의 precondition 을 f1 로 실어 보냅니다.\nf1 만 갈아 같은 본문으로 다시 부르지 않습니다. 그러면 남이 쓴 글이 사라집니다."
        )));
    }
    Ok(bytes)
}
