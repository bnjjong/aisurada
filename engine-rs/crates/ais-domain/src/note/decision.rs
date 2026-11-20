//! 결정 블록: AI가 노트에 쓴 「정할 것」과 사람이 남긴 답을 본문 글에서 뽑는다.
//!
//! AI가 사람에게 고르게 할 것을 노트에 적고, 사람이 편집기에서 답하면 AI가 그 답을 읽고 이어 가게 하려는 것이다. 형식은
//! 2026-11-20에 사용자와 정했다.
//!
//! ```markdown
//! > [!decision] 결정 1 · 명령을 실행해도 되나?
//! > - [ ] 가(추천): 읽기만 한다.
//! > - [ ] 나: 안 바꾸는 명령만 돌린다.
//! >
//! > 답:
//!
//! > [!decided] 답을 다 적었으면 체크하세요
//! > - [ ] 답 다 했음
//! ```
//!
//! - 체크와 `답:` 줄이 둘 다 있으면 `답:` 줄을 따른다(`choice`).
//! - 사람이 「답 다 했음」을 체크해 저장하면 끝난 것으로 본다(`finished`). 그 블록이 없으면 모든 결정에 답이 차야 한다.
//! - 코드 펜스 안의 블록은 보기 글이라 세지 않는다. `[!question]` 같은 다른 콜아웃도 세지 않는다.
//!
//! 파일도 vault도 모른다. 누가 마지막으로 저장했는지는 인자로 받는다.

use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use super::meta::MetaData;
use crate::kernel::token::compute_token;

static BLOCK_START: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^ {0,3}> ?\[!(decision|decided)\][ \t]*(.*)$").unwrap());
static QUOTE_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^ {0,3}>(?: ?)(.*)$").unwrap());
static OPTION_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*[-*+]\s+\[([ xX/\-])\]\s*(.*)$").unwrap());
static ANSWER_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:\*\*)?답\s*[:：](?:\*\*)?(.*)$").unwrap());
static NUMBERED_TITLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^결정\s*(\d+)\s*[·.:\-–—]?\s*(.*)$").unwrap());
static FENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^ {0,3}(`{3,}|~{3,})").unwrap());

/// 결정 하나의 선택지
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionOption {
    /// 선택지 이름(`가`). `가(추천):`처럼 쌍점 앞에 적은 글에서 `(추천)`을 뺀 것이다. 쌍점이 없으면 빈 글이다.
    pub label: String,
    /// 쌍점 뒤의 설명
    pub text: String,
    /// 이름 자리에 `추천`이 들어 있는가
    pub recommended: bool,
    /// 체크했는가(`[x]`)
    pub checked: bool,
}

/// 결정 블록 하나
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    /// `결정 N · 제목`의 N. 번호가 없으면 노트 안의 차례(1부터)다.
    pub number: u32,
    /// 질문
    pub title: String,
    pub options: Vec<DecisionOption>,
    /// 체크한 선택지 이름들
    pub checked: Vec<String>,
    /// `답:` 줄의 글(앞뒤 공백을 다듬음). 비어 있을 수 있다.
    pub answer: String,
    /// 고른 선택지. 답 줄의 첫 낱말이 선택지 이름이면 그것, 아니면 체크가 하나뿐일 때 그것이다.
    pub choice: Option<String>,
    /// 답 줄에 글이 있거나 체크가 하나라도 있다
    pub answered: bool,
}

/// 노트 하나의 결정 블록 전부와 「답 다 했음」 블록의 상태
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DecisionSheet {
    pub decisions: Vec<Decision>,
    /// 「답 다 했음」 블록(`[!decided]`)이 있는가
    pub done_block: bool,
    /// 그 블록에서 체크한 것이 있는가
    pub done_checked: bool,
}

impl DecisionSheet {
    /// 답이 찬 결정 수
    pub fn answered_count(&self) -> usize {
        self.decisions.iter().filter(|d| d.answered).count()
    }

    /// 사람이 답을 끝냈는가. AI가 쓴 체크로 끝나지 않게, 마지막 저장이 사람(앱)일 때만 본다([`edited_by_human`]).
    /// 「답 다 했음」 블록이 있으면 그 체크를, 없으면 모든 결정에 답이 찼는지를 본다.
    pub fn finished(&self, edited_by_human: bool) -> bool {
        if self.decisions.is_empty() || !edited_by_human {
            return false;
        }
        if self.done_block {
            self.done_checked
        } else {
            self.decisions.iter().all(|d| d.answered)
        }
    }

    /// 편집 도장만 있는 옛 노트가 사람의 답을 기다리는가.
    pub fn waiting(&self, edited_by_human: bool) -> bool {
        !self.decisions.is_empty() && !self.finished(edited_by_human)
    }

    fn confirmation_token(&self) -> String {
        // 알림 지문과 달리 선택지와 답까지 묶어 이전 확인을 다른 결정에 쓰지 않게 한다.
        compute_token(&serde_json::to_vec(self).expect("결정 블록은 JSON으로 표현할 수 있다"))
    }

    /// 옛 노트는 당시의 편집 도장으로, 새 노트는 이번 결정에 남긴 확인 값으로 판정한다.
    pub fn confirmed(&self, meta: &MetaData) -> bool {
        match meta.decision_confirmation.as_deref() {
            None => self.finished(edited_by_human(meta.edited_by.as_deref())),
            Some("pending") => false,
            Some(token) => self.finished(true) && token == self.confirmation_token(),
        }
    }

    /// 묻는 결정들의 지문. 같은 묶음을 두 번 알리지 않으려고 알림 id에 붙인다. 번호와 질문만 보고 답은 보지 않는다.
    pub fn signature(&self) -> String {
        // FNV-1a 64비트. 암호용이 아니라 묶음이 바뀌었는지만 가른다.
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for decision in &self.decisions {
            for byte in decision
                .number
                .to_string()
                .bytes()
                .chain([0x1f])
                .chain(decision.title.bytes())
                .chain([0x1e])
            {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        format!("{hash:016x}")
    }
}

/// 마지막 저장이 사람(앱)인가. 노트 머리의 `editedBy`가 `app`일 때다. 도장이 없으면(만든 뒤 고친 적 없음) 사람으로 보지 않는다.
pub fn edited_by_human(edited_by: Option<&str>) -> bool {
    edited_by == Some("app")
}

/// 대부분의 노트에는 결정이 없어 본문을 줄별로 해석할 필요가 없다.
pub fn mentions_decision(body: &str) -> bool {
    body.as_bytes()
        .windows(b"[!decision".len())
        .any(|w| w.eq_ignore_ascii_case(b"[!decision"))
}

/// 질문과 답이 같으면 기존 확인을 보존한다. 사람은 결정 블록을 편집해 답을 확정한다.
pub fn confirmation_after_edit(
    meta: &MetaData,
    old_body: &str,
    body: &str,
    tool: bool,
) -> Option<String> {
    if !mentions_decision(body) {
        return None;
    }
    let current = parse(body);
    if current.decisions.is_empty() {
        return None;
    }
    let old = parse(old_body);
    let confirmed = if current == old {
        old.confirmed(meta)
    } else {
        // 이전 묶음의 완료 체크를 남기고 새 질문을 추가한 경우 다시 확인을 기다린다.
        let stale_check =
            current.done_checked && old.done_checked && current.signature() != old.signature();
        !tool && !stale_check && current.finished(true)
    };
    Some(if confirmed {
        current.confirmation_token()
    } else {
        "pending".to_owned()
    })
}

/// 본문에서 결정 블록과 「답 다 했음」 블록을 뽑는다.
pub fn parse(body: &str) -> DecisionSheet {
    let mut sheet = DecisionSheet::default();
    let lines: Vec<&str> = body.lines().collect();
    let mut fence: Option<(char, usize)> = None;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if let Some(found) = FENCE.captures(line) {
            let run = found.get(1).unwrap().as_str();
            let (mark, len) = (run.chars().next().unwrap(), run.chars().count());
            fence = match fence {
                None => Some((mark, len)),
                Some((open, open_len)) if open == mark && len >= open_len => None,
                still => still,
            };
            i += 1;
            continue;
        }
        if fence.is_some() {
            i += 1;
            continue;
        }
        let Some(start) = BLOCK_START.captures(line) else {
            i += 1;
            continue;
        };
        let kind = start.get(1).unwrap().as_str().to_ascii_lowercase();
        let first = start.get(2).map_or("", |m| m.as_str()).trim().to_string();
        let mut inner = Vec::new();
        i += 1;
        while i < lines.len() {
            let Some(quoted) = QUOTE_LINE.captures(lines[i]) else {
                break;
            };
            if BLOCK_START.is_match(lines[i]) {
                break;
            }
            inner.push(quoted.get(1).map_or("", |m| m.as_str()));
            i += 1;
        }
        if kind == "decided" {
            sheet.done_block = true;
            if inner
                .iter()
                .any(|l| option_line(l).is_some_and(|(checked, _)| checked))
            {
                sheet.done_checked = true;
            }
        } else {
            let order = sheet.decisions.len() as u32 + 1;
            sheet.decisions.push(decision(order, &first, &inner));
        }
    }
    sheet
}

/// 체크리스트 줄이면 (체크했는가, 줄 글)
fn option_line(line: &str) -> Option<(bool, &str)> {
    let found = OPTION_LINE.captures(line)?;
    let mark = found.get(1).unwrap().as_str();
    Some((
        mark.eq_ignore_ascii_case("x"),
        found.get(2).map_or("", |m| m.as_str()),
    ))
}

fn decision(order: u32, first: &str, inner: &[&str]) -> Decision {
    let mut title = first.to_string();
    let mut options = Vec::new();
    let mut answer: Option<Vec<String>> = None;
    for line in inner {
        if let Some((checked, text)) = option_line(line) {
            options.push(option(checked, text));
            continue;
        }
        if let Some(found) = ANSWER_LINE.captures(line) {
            answer
                .get_or_insert_with(Vec::new)
                .push(found.get(1).map_or("", |m| m.as_str()).trim().to_string());
            continue;
        }
        if let Some(lines) = answer.as_mut() {
            // 답 줄 뒤에 이어 쓴 줄은 답이다
            lines.push(line.trim().to_string());
        } else if title.is_empty() && !line.trim().is_empty() {
            // 편집기가 제목을 다음 줄로 내린 경우
            title = line.trim().to_string();
        }
    }
    let (number, title) = match NUMBERED_TITLE.captures(&title) {
        Some(found) => (
            found.get(1).unwrap().as_str().parse().unwrap_or(order),
            found.get(2).map_or("", |m| m.as_str()).trim().to_string(),
        ),
        None => (order, title),
    };
    let answer = answer
        .map(|lines| lines.join("\n").trim().to_string())
        .unwrap_or_default();
    let checked: Vec<String> = options
        .iter()
        .filter(|o| o.checked)
        .map(|o| o.label.clone())
        .collect();
    let choice = leading_label(&answer, &options)
        .or_else(|| (checked.len() == 1).then(|| checked[0].clone()));
    let answered = !answer.is_empty() || !checked.is_empty();
    Decision {
        number,
        title,
        options,
        checked,
        answer,
        choice,
        answered,
    }
}

fn option(checked: bool, text: &str) -> DecisionOption {
    let text = text.trim();
    let split = text.char_indices().find(|&(_, c)| c == ':' || c == '：');
    let Some((at, colon)) = split else {
        return DecisionOption {
            label: String::new(),
            text: text.trim_matches('*').trim().to_string(),
            recommended: false,
            checked,
        };
    };
    let head = text[..at].trim().trim_matches('*').trim();
    let rest = text[at + colon.len_utf8()..].trim_start_matches('*').trim();
    let recommended = head.contains("추천");
    let label = head
        .replace("(추천)", "")
        .replace("（추천）", "")
        .trim()
        .to_string();
    DecisionOption {
        label,
        text: rest.to_string(),
        recommended,
        checked,
    }
}

/// 답 글의 첫 낱말이 선택지 이름이면 그 이름
fn leading_label(answer: &str, options: &[DecisionOption]) -> Option<String> {
    let word: String = answer
        .trim_start_matches(['*', ' '])
        .chars()
        .take_while(|c| {
            !c.is_whitespace()
                && !matches!(
                    c,
                    '.' | ',' | ':' | ';' | '(' | ')' | '·' | '/' | '*' | '。' | '，'
                )
        })
        .collect();
    if word.is_empty() {
        return None;
    }
    options
        .iter()
        .find(|o| !o.label.is_empty() && o.label == word)
        .map(|o| o.label.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ASKED: &str = "# 노트\n\n## 정할 것\n\n> [!decision] 결정 1 · 명령을 실행해도 되나?\n> - [ ] 가(추천): 읽기만 한다.\n> - [ ] 나: 안 바꾸는 명령만.\n>\n> 답:\n\n> [!decision] 결정 2 · 판정 칸은?\n> - [ ] **가(추천):** 넷.\n> - [ ] 나: 셋.\n>\n> 답:\n\n> [!decided] 답을 다 적었으면 체크하세요\n> - [ ] 답 다 했음\n";

    #[test]
    fn reads_questions_options_and_the_done_block() {
        let sheet = parse(ASKED);
        assert_eq!(sheet.decisions.len(), 2);
        let first = &sheet.decisions[0];
        assert_eq!(
            (first.number, first.title.as_str()),
            (1, "명령을 실행해도 되나?")
        );
        assert_eq!(
            first.options[0],
            DecisionOption {
                label: "가".into(),
                text: "읽기만 한다.".into(),
                recommended: true,
                checked: false
            }
        );
        assert_eq!(first.options[1].label, "나");
        assert!(!first.answered && first.choice.is_none());
        // 굵게 쓴 이름도 읽는다
        assert_eq!(
            (
                sheet.decisions[1].options[0].label.as_str(),
                sheet.decisions[1].options[0].text.as_str()
            ),
            ("가", "넷.")
        );
        assert!(sheet.done_block && !sheet.done_checked);
        assert!(sheet.waiting(true) && !sheet.finished(true));
    }

    #[test]
    fn the_answer_line_wins_over_the_checkbox_and_spaces_are_trimmed() {
        let body = ASKED
            .replacen("> 답:\n", "> 답:  나. 다만 --help는 허용\n", 1)
            .replacen("> - [ ] 가(추천): 읽기만", "> - [x] 가(추천): 읽기만", 1);
        let first = &parse(&body).decisions[0];
        assert_eq!(first.answer, "나. 다만 --help는 허용");
        assert_eq!(first.checked, vec!["가".to_string()]);
        assert_eq!(first.choice.as_deref(), Some("나"));
        assert!(first.answered);
        // 체크만 있으면 체크한 것
        let only_check = ASKED.replacen("> - [ ] 나: 셋.", "> - [x] 나: 셋.", 1);
        assert_eq!(
            parse(&only_check).decisions[1].choice.as_deref(),
            Some("나")
        );
    }

    #[test]
    fn only_a_human_check_on_the_done_block_finishes() {
        let done = ASKED.replace("> - [ ] 답 다 했음", "> - [x] 답 다 했음");
        let sheet = parse(&done);
        assert!(sheet.done_checked);
        assert!(sheet.finished(true), "사람이 체크해 저장했다");
        assert!(!sheet.finished(false), "AI가 쓴 체크는 세지 않는다");
        assert_eq!(sheet.answered_count(), 0, "일부만 답하고 넘길 수 있다");
    }

    #[test]
    fn without_a_done_block_every_decision_needs_an_answer() {
        let body = "> [!decision] 하나\n> - [ ] 가: a\n>\n> 답: 가\n\n> [!decision] 둘\n> - [ ] 가: a\n>\n> 답:\n";
        let sheet = parse(body);
        assert_eq!(
            (sheet.decisions[0].number, sheet.decisions[1].number),
            (1, 2),
            "번호가 없으면 차례"
        );
        assert!(!sheet.finished(true));
        assert!(parse(&body.replace("> 답:\n", "> 답: 가\n")).finished(true));
    }

    #[test]
    fn fenced_examples_and_other_callouts_are_not_decisions() {
        let body = "```markdown\n> [!decision] 보기\n> 답:\n> [!decided] 보기\n> - [ ] 답 다 했음\n```\n\n> [!question] 결정 1 · 옛 형식\n> 답: 가\n";
        assert_eq!(parse(body), DecisionSheet::default());
    }

    #[test]
    fn a_title_pushed_to_the_next_line_and_multi_line_answers_are_read() {
        let body = "> [!decision]\n> 결정 3 · 어디에 둘까?\n>\n> - [ ] 가: 여기\n>\n> 답: 다\n> 이유는 이렇다\n";
        let only = &parse(body).decisions[0];
        assert_eq!((only.number, only.title.as_str()), (3, "어디에 둘까?"));
        assert_eq!(only.answer, "다\n이유는 이렇다");
        assert_eq!(only.choice, None, "선택지에 없는 이름이면 고른 것이 없다");
    }

    #[test]
    fn the_signature_follows_the_questions_not_the_answers() {
        let asked = parse(ASKED).signature();
        let answered = parse(&ASKED.replacen("> 답:\n", "> 답: 가\n", 1)).signature();
        assert_eq!(asked, answered);
        assert_ne!(
            asked,
            parse(&ASKED.replace("판정 칸은?", "판정 칸 수는?")).signature()
        );
        assert!(
            edited_by_human(Some("app"))
                && !edited_by_human(Some("tool"))
                && !edited_by_human(None)
        );
    }
}
