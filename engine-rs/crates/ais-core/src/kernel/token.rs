//! 내용 확인 값(content token). 고치기 전에 읽은 파일 바이트의 지문으로, `f1:` 뒤에 SHA-256 16진수 64자가 붙는다.
//!
//! 같은 값이 코드 곳곳에서 세 이름으로 불렸다: 명령 인자 `f1`, 응답의 `precondition`(「이 값을 f1에 넣어라」), 기록·대기 파일의 `token`.
//! 모두 이 값이다. 읽은 뒤 파일이 바뀌었는지(낙관적 잠금)를 이 값으로 가려서, 노트·태스크·자원이 같은 계산을 쓴다.
//! 계산은 한때 `vault/note_file.rs`에 있어서 태스크·자원·쓰기가 노트 읽기 모듈을 거쳐 가져왔다.

use sha2::{Digest, Sha256};

/// 확인 값의 접두어. 이 글자로 시작하지 않는 `f1`은 값 모양이 틀린 것이다.
pub const PREFIX: &str = "f1:";

/// 파일 전체 바이트의 SHA-256으로 고치기 전 확인 값을 만듭니다.
pub fn compute_token(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut token = String::with_capacity(PREFIX.len() + 64);
    token.push_str(PREFIX);
    for byte in digest {
        token.push_str(&format!("{byte:02x}"));
    }
    token
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_prefix_plus_sha256_hex() {
        // SHA-256("") = e3b0c442...
        assert_eq!(
            compute_token(b""),
            "f1:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert!(compute_token(b"x").starts_with(PREFIX));
        assert_eq!(compute_token(b"x").len(), PREFIX.len() + 64);
    }
}
