//! 노트 머리만 고치는 쓰기 세 가지의 규칙: 즐겨찾기(`set_favorite`), 평가(`evaluate_doc`), 연결(`link_docs`).
//! 파일도 vault도 읽고 쓰지 않는다. 노트의 지금 값과 지금 시각을 인자로 받아 새 값을 돌려준다.
//!
//! 명령은 인자를 읽고, 노트를 찾고, 여기서 새 값을 얻고, 머리를 써 넣는 차례만 맡는다.
//! 오류가 나오는 차례를 지키려고 각 규칙을 노트를 찾기 전과 후로 나눠 두었다.

use super::meta::{Evaluation, Link, MetaData};
use crate::error::EngineError;
use crate::kernel::java::java_double_string;
use crate::kernel::link_kind::LinkKind;
use crate::kernel::time::format_utc_millis;
use crate::text::{NOTE_ID, cut_utf16, is_blank, trim};

/// 즐겨찾기를 바꾼 결과
#[derive(Debug, PartialEq, Eq)]
pub struct FavoriteChange {
    pub favorite: bool,
    pub category: Option<String>,
    /// 지금 값과 같아서 쓸 것이 없는가
    pub unchanged: bool,
}

/// 즐겨찾기 값을 정한다. `favorite`를 안 주면 지금 값을 뒤집는다(`category`만 주면 값을 그대로 둔다).
/// `category`가 빈 글이면 라벨을 지우고, 안 주면 지금 라벨을 지킨다.
pub fn decide_favorite(
    meta: &MetaData,
    favorite: Option<bool>,
    category: Option<&str>,
) -> FavoriteChange {
    let new_favorite = match (favorite, category.is_some()) {
        (Some(f), _) => f,
        (None, true) => meta.favorite,
        (None, false) => !meta.favorite,
    };
    let new_category = match category {
        Some(raw) if is_blank(raw) => None,
        Some(raw) => Some(trim(raw).to_string()),
        None => meta.favorite_category.clone(),
    };
    let unchanged = new_favorite == meta.favorite && new_category == meta.favorite_category;
    FavoriteChange {
        favorite: new_favorite,
        category: new_category,
        unchanged,
    }
}

/// 평가 점수 인자. 호출 객체에서는 숫자, 명령줄에서는 글로 온다.
#[derive(Debug, Clone, Copy)]
pub enum ScoreInput<'a> {
    Number(f64),
    Text(&'a str),
}

/// 점수 인자를 읽어 0~100 범위를 검사한다. 안 줬거나 빈 글이면 `usage`가 붙은 오류다.
pub fn parse_score(input: Option<ScoreInput>, usage: &str) -> Result<f64, EngineError> {
    let missing = || {
        EngineError::invalid_argument(format!("score 를 주거나 clear=true 를 지정한다: {usage}"))
    };
    let score = match input {
        None => return Err(missing()),
        Some(ScoreInput::Number(n)) => n,
        Some(ScoreInput::Text(text)) => {
            let trimmed = trim(text);
            if trimmed.is_empty() {
                return Err(missing());
            }
            trimmed.parse::<f64>().map_err(|_| {
                EngineError::invalid_argument(format!("score 은 숫자여야 한다: {trimmed}"))
            })?
        }
    };
    if score.is_nan() || !(0.0..=100.0).contains(&score) {
        return Err(EngineError::invalid_argument(format!(
            "평가 점수는 0 에서 100 사이여야 한다: {}",
            java_double_string(score)
        )));
    }
    Ok(score)
}

/// 점수와 메모, 지금 시각으로 평가를 짓는다. 점수는 반올림(0.5는 올림)하고 메모는 다듬어 1000자(UTF-16)까지만 남긴다.
pub fn evaluation_of(score: f64, note: Option<&str>, now_millis: i64) -> Evaluation {
    Evaluation {
        score: (score + 0.5).floor().clamp(0.0, 100.0) as u8,
        note: note.map_or_else(String::new, |n| cut_utf16(trim(n), 1000)),
        at: format_utc_millis(now_millis),
    }
}

/// 연결의 두 끝 id를 검사한다: 출발이 비면 사용법, 대상이 비면 대상 안내, 둘 다 노트 id 모양이어야 한다.
pub fn check_link_ends(from: &str, to: &str, usage: &str) -> Result<(), EngineError> {
    if is_blank(from) {
        return Err(EngineError::invalid_argument(format!(
            "인자가 하나 필요하다: {usage}"
        )));
    }
    if is_blank(to) {
        return Err(EngineError::invalid_argument(format!(
            "대상 노트 id 가 필요하다: {usage}"
        )));
    }
    for id in [from, to] {
        if !NOTE_ID.is_match(id) {
            return Err(EngineError::invalid_argument(format!(
                "노트 id 모양이 아니다: {id}"
            )));
        }
    }
    Ok(())
}

/// 관계 종류를 읽는다. 안 주거나 비면 `direct`, 모르는 값이면 오류. 소문자로 맞춰 돌려준다.
pub fn parse_link_kind(raw: Option<&str>) -> Result<String, EngineError> {
    let Some(raw) = raw.filter(|r| !trim(r).is_empty()) else {
        return Ok(LinkKind::Direct.as_str().to_string());
    };
    let lower = trim(raw).to_lowercase();
    if LinkKind::parse(&lower).is_none() {
        return Err(EngineError::invalid_argument(format!(
            "모르는 관계 종류 이다: {}. 쓸 수 있는 것은 [{}] 뿐이다.",
            trim(raw),
            LinkKind::NAMES_TEXT
        )));
    }
    Ok(lower)
}

/// 자기 자신과는 연결할 수 없다.
pub fn check_not_self(from: &str, to: &str) -> Result<(), EngineError> {
    if from == to {
        return Err(EngineError::invalid_argument(format!(
            "자기 자신과는 연결할 수 없다: {from}"
        )));
    }
    Ok(())
}

/// 연결을 더하거나 뺀 결과
#[derive(Debug, PartialEq, Eq)]
pub enum LinkOutcome {
    /// 이미 있어서 더할 것이 없거나, 없어서 뺄 것이 없다. 응답에 붙일 안내 글
    Unchanged(&'static str),
    /// 새 연결 목록
    Changed(Vec<Link>),
}

/// 같은 대상(`to`)과 종류(`kind`, 대소문자 무시)의 연결을 더하거나 뺀다. 더할 때는 목록 끝에 붙는다.
pub fn apply_link(links: &[Link], to: &str, kind: &str, remove: bool) -> LinkOutcome {
    let same = |l: &Link| l.to == to && l.kind.eq_ignore_ascii_case(kind);
    let exists = links.iter().any(same);
    if !remove && exists {
        return LinkOutcome::Unchanged("이미 있는 연결이에요.");
    }
    if remove && !exists {
        return LinkOutcome::Unchanged("제거할 연결이 없어요.");
    }
    let mut next: Vec<Link> = links.iter().filter(|l| !same(l)).cloned().collect();
    if !remove {
        next.push(Link {
            to: to.to_string(),
            kind: kind.to_string(),
        });
    }
    LinkOutcome::Changed(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::tier::Tier;

    fn meta(favorite: bool, category: Option<&str>) -> MetaData {
        MetaData {
            id: "ndzz0001-0000aaaa".into(),
            agenda_slug: "demo".into(),
            doc_type: "free".into(),
            title: "t".into(),
            created: "2026-11-12T09:00:00+09:00".into(),
            date: "2026-11-12".into(),
            time: "09:00".into(),
            session_id: None,
            session_agent: None,
            tier: Tier::Reading,
            decision_confirmation: None,
            edited_by: None,
            created_at: None,
            edited_at: None,
            edited_session_id: None,
            edited_session_agent: None,
            tags: Vec::new(),
            sub_agenda_ids: Vec::new(),
            links: Vec::new(),
            favorite,
            favorite_category: category.map(str::to_string),
            template: None,
            evaluation: None,
        }
    }

    fn link(to: &str, kind: &str) -> Link {
        Link {
            to: to.into(),
            kind: kind.into(),
        }
    }

    #[test]
    fn favorite_flips_by_default_and_keeps_the_value_when_only_a_label_comes() {
        let m = meta(false, None);
        assert_eq!(
            decide_favorite(&m, None, None),
            FavoriteChange {
                favorite: true,
                category: None,
                unchanged: false
            }
        );
        // 라벨만 주면 값은 그대로, 라벨이 바뀐다
        let c = decide_favorite(&m, None, Some(" 읽을 것 "));
        assert_eq!(
            (c.favorite, c.category.as_deref(), c.unchanged),
            (false, Some("읽을 것"), false)
        );
        // 빈 라벨은 지운다. 이미 없으면 바뀐 것이 없다
        assert!(decide_favorite(&m, None, Some("  ")).unchanged);
        let with = meta(true, Some("a"));
        let c = decide_favorite(&with, Some(true), Some(""));
        assert_eq!((c.favorite, c.category, c.unchanged), (true, None, false));
        assert!(decide_favorite(&with, Some(true), None).unchanged);
    }

    #[test]
    fn score_is_read_from_a_number_or_text_and_range_checked() {
        let usage = "U";
        let err = |i| parse_score(i, usage).err().unwrap().message;
        assert_eq!(err(None), "score 를 주거나 clear=true 를 지정한다: U");
        assert_eq!(
            err(Some(ScoreInput::Text("  "))),
            "score 를 주거나 clear=true 를 지정한다: U"
        );
        assert_eq!(
            err(Some(ScoreInput::Text("abc"))),
            "score 은 숫자여야 한다: abc"
        );
        assert_eq!(
            err(Some(ScoreInput::Number(100.5))),
            "평가 점수는 0 에서 100 사이여야 한다: 100.5"
        );
        assert_eq!(
            err(Some(ScoreInput::Number(f64::NAN))),
            "평가 점수는 0 에서 100 사이여야 한다: NaN"
        );
        assert_eq!(
            parse_score(Some(ScoreInput::Text(" 87.5 ")), usage).unwrap(),
            87.5
        );
    }

    #[test]
    fn evaluation_rounds_half_up_trims_the_note_and_uses_the_given_clock() {
        let at = 1_794_700_000_000;
        let e = evaluation_of(87.5, Some("  좋음  "), at);
        assert_eq!((e.score, e.note.as_str()), (88, "좋음"));
        assert_eq!(e.at, format_utc_millis(at));
        assert_eq!(evaluation_of(0.4, None, at).score, 0);
        assert_eq!(evaluation_of(0.4, None, at).note, "");
        // 메모는 UTF-16 1000자까지
        let long = "가".repeat(1500);
        assert_eq!(
            evaluation_of(1.0, Some(&long), at).note.chars().count(),
            1000
        );
    }

    #[test]
    fn link_ends_are_checked_blank_first_then_shape() {
        let id = "ndzz0001-0000aaaa";
        let msg = |f: &str, t: &str| check_link_ends(f, t, "U").err().unwrap().message;
        assert_eq!(msg(" ", " "), "인자가 하나 필요하다: U");
        assert_eq!(msg(id, " "), "대상 노트 id 가 필요하다: U");
        assert_eq!(msg("bad", id), "노트 id 모양이 아니다: bad");
        assert!(check_link_ends(id, id, "U").is_ok());
        assert_eq!(
            check_not_self(id, id).err().unwrap().message,
            format!("자기 자신과는 연결할 수 없다: {id}")
        );
    }

    #[test]
    fn link_kind_defaults_to_direct_and_lowercases() {
        assert_eq!(parse_link_kind(None).unwrap(), "direct");
        assert_eq!(parse_link_kind(Some("  ")).unwrap(), "direct");
        assert_eq!(parse_link_kind(Some(" Attach ")).unwrap(), "attach");
        assert!(
            parse_link_kind(Some("zzz"))
                .err()
                .unwrap()
                .message
                .starts_with("모르는 관계 종류 이다: zzz")
        );
    }

    #[test]
    fn links_are_added_at_the_end_and_removed_by_target_and_kind() {
        let a = link("nda-1", "direct");
        let b = link("ndb-2", "attach");
        let both = [a.clone(), b.clone()];
        assert_eq!(
            apply_link(&both, "nda-1", "direct", false),
            LinkOutcome::Unchanged("이미 있는 연결이에요.")
        );
        assert_eq!(
            apply_link(&both, "ndc-3", "direct", true),
            LinkOutcome::Unchanged("제거할 연결이 없어요.")
        );
        assert_eq!(
            apply_link(&[a.clone()], "ndb-2", "attach", false),
            LinkOutcome::Changed(vec![a.clone(), b.clone()])
        );
        // 종류 비교는 대소문자를 무시한다
        assert_eq!(
            apply_link(&both, "ndb-2", "ATTACH", true),
            LinkOutcome::Changed(vec![a])
        );
    }
}
