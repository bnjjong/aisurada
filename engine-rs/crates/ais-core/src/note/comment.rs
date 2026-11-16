//! 코멘트 쓰기의 인자 규칙: 어느 노트의 코멘트인지, 코멘트 식별자, 본문과 앵커 인용문의 길이 상한.
//! 파일도 vault도 읽지 않는다. 코멘트 파일을 읽고 식별자가 겹치는지 보는 일은 명령이 한다.

use crate::error::EngineError;
use crate::text::{NOTE_ID, SUB_NOTE_ID, is_blank, trim, utf16_len};

/// 코멘트 쓰기의 세 갈래
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CommentOp {
    Create,
    Update,
    Delete,
}

/// 코멘트 본문 상한(UTF-16 단위)
pub const MAX_BODY_UNITS: usize = 20_000;
/// 앵커 인용문 상한(UTF-16 단위)
pub const MAX_QUOTE_UNITS: usize = 2_000;

/// `create`·`update`·`delete`에 받은 값(검사 전)
pub struct CommentInput<'a> {
    pub note_id: &'a str,
    pub sub_id: Option<&'a str>,
    pub comment_id: Option<&'a str>,
    pub body: Option<&'a str>,
    pub anchor_quote: Option<&'a str>,
}

/// 검사를 마친 코멘트 쓰기 인자
#[derive(Debug)]
pub struct ParsedComment<'a> {
    pub note_id: &'a str,
    /// 비어 있지 않은 후속 노트 id
    pub sub_id: Option<&'a str>,
    /// 다듬은 코멘트 식별자. 만들 때만 비어 있을 수 있다(발급한다)
    pub comment_id: String,
    pub body: Option<&'a str>,
    /// 앵커 인용문(만들 때만 쓴다)
    pub quote: Option<&'a str>,
}

/// 인자를 검사한다. 오류가 나오는 차례: 노트 id 없음 → 노트 id 모양 → 후속 노트 id 모양 → 코멘트 식별자 없음(고치기·지우기) → 본문(만들기·고치기) → 앵커 인용문 길이(만들기).
pub fn parse<'a>(
    op: CommentOp,
    input: CommentInput<'a>,
    usage: &str,
) -> Result<ParsedComment<'a>, EngineError> {
    if is_blank(input.note_id) {
        return Err(EngineError::invalid_argument(format!(
            "인자가 하나 필요하다: {usage}"
        )));
    }
    let sub_id = input.sub_id.filter(|s| !is_blank(s));
    if !NOTE_ID.is_match(input.note_id) {
        return Err(EngineError::invalid_argument(format!(
            "노트 id 모양이 아니다: {}",
            input.note_id
        )));
    }
    if let Some(sub) = sub_id
        && !SUB_NOTE_ID.is_match(sub)
    {
        return Err(EngineError::invalid_argument(format!(
            "하위 노트 id 모양이 아니다: {sub}"
        )));
    }
    let comment_id = input
        .comment_id
        .map(|c| trim(c).to_string())
        .unwrap_or_default();
    if op != CommentOp::Create && comment_id.is_empty() {
        return Err(EngineError::invalid_argument(format!(
            "comment_id 이 필요하다: {usage}"
        )));
    }
    if op != CommentOp::Delete {
        check_body(input.body, usage)?;
    }
    if op == CommentOp::Create
        && input
            .anchor_quote
            .is_some_and(|q| utf16_len(q) > MAX_QUOTE_UNITS)
    {
        return Err(EngineError::invalid_argument(
            "앵커 인용문이 너무 길다. 상한은 2000자다.",
        ));
    }
    Ok(ParsedComment {
        note_id: input.note_id,
        sub_id,
        comment_id,
        body: input.body,
        quote: input.anchor_quote,
    })
}

fn check_body(body: Option<&str>, usage: &str) -> Result<(), EngineError> {
    let body =
        body.ok_or_else(|| EngineError::invalid_argument(format!("body 이 필요하다: {usage}")))?;
    if is_blank(body) {
        return Err(EngineError::invalid_argument("코멘트 본문이 비었다."));
    }
    let length = utf16_len(body);
    if length > MAX_BODY_UNITS {
        return Err(EngineError::invalid_argument(format!(
            "코멘트 본문이 너무 길다({length}자). 상한은 20000자다."
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "ndzz0001-0000aaaa";

    fn input<'a>() -> CommentInput<'a> {
        CommentInput {
            note_id: ID,
            sub_id: None,
            comment_id: None,
            body: Some("본문"),
            anchor_quote: None,
        }
    }

    fn err(op: CommentOp, i: CommentInput) -> String {
        parse(op, i, "U").err().unwrap().message
    }

    #[test]
    fn ids_are_checked_before_the_body() {
        let mut i = input();
        i.note_id = " ";
        i.body = None;
        assert_eq!(err(CommentOp::Create, i), "인자가 하나 필요하다: U");
        let mut i = input();
        i.note_id = "bad";
        assert_eq!(err(CommentOp::Create, i), "노트 id 모양이 아니다: bad");
        let mut i = input();
        i.sub_id = Some("bad");
        assert_eq!(err(CommentOp::Create, i), "하위 노트 id 모양이 아니다: bad");
        // 빈 후속 노트 id는 없는 것으로 본다
        let mut i = input();
        i.sub_id = Some("  ");
        assert_eq!(parse(CommentOp::Create, i, "U").unwrap().sub_id, None);
    }

    #[test]
    fn update_and_delete_need_a_comment_id_but_create_does_not() {
        let mut i = input();
        i.body = None;
        assert_eq!(err(CommentOp::Update, i), "comment_id 이 필요하다: U");
        assert_eq!(err(CommentOp::Delete, input()), "comment_id 이 필요하다: U");
        let parsed = parse(CommentOp::Create, input(), "U").unwrap();
        assert_eq!(parsed.comment_id, "");
        let mut i = input();
        i.comment_id = Some(" cm-1 ");
        assert_eq!(parse(CommentOp::Update, i, "U").unwrap().comment_id, "cm-1");
    }

    #[test]
    fn the_body_is_required_and_capped_except_for_delete() {
        let mut i = input();
        i.body = None;
        assert_eq!(err(CommentOp::Create, i), "body 이 필요하다: U");
        let mut i = input();
        i.body = Some(" \n");
        assert_eq!(err(CommentOp::Create, i), "코멘트 본문이 비었다.");
        let long = "가".repeat(20_001);
        let mut i = input();
        i.body = Some(&long);
        assert_eq!(
            err(CommentOp::Create, i),
            "코멘트 본문이 너무 길다(20001자). 상한은 20000자다."
        );
        // 지우기는 본문을 보지 않는다
        let mut i = input();
        i.body = None;
        i.comment_id = Some("cm-1");
        assert!(parse(CommentOp::Delete, i, "U").is_ok());
    }

    #[test]
    fn only_create_caps_the_anchor_quote() {
        let long = "가".repeat(2_001);
        let mut i = input();
        i.anchor_quote = Some(&long);
        assert_eq!(
            err(CommentOp::Create, i),
            "앵커 인용문이 너무 길다. 상한은 2000자다."
        );
        let mut i = input();
        i.anchor_quote = Some(&long);
        i.comment_id = Some("cm-1");
        assert!(parse(CommentOp::Update, i, "U").is_ok());
    }
}
