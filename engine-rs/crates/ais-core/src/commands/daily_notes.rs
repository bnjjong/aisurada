//! 29장 `plan_day`(직전 기록일과 오늘의 초안을 짓고 `create_doc`과 같은 길로 저장한다)와 30장 `check_notes`(마지막 코멘트와 마지막 변경을
//! 각자의 오프셋으로 비교한다). 규칙과 응답은 Java 엔진의 `PlanDay`·`CheckNotes`·`DailyNotes`와 같다.

use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;
use serde_json::{Map, Value as Json, json};

use crate::kernel::time::days_from_civil;
use super::doc_create::{CreateDocRequest, create_doc_in};
use super::facts_brief::{iso_text, parse_iso_date};
use super::option_text::text_of;
use super::shard_notes::bool_option;
use crate::error::EngineError;
use crate::frontmatter::Value;
use crate::text::{compare_utf16, is_blank, trim};
use crate::vault::comments::{CommentsEnvelope, read_comments};
use crate::vault::note_file::read_note_file;
use crate::vault::scanner::NoteRecord;
use crate::write::apparatus::now_millis;
use crate::write::local_time::local_clock;
use crate::write::pipeline::{WriteContext, WriteScope};

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PlanDayRequest {
    pub date: Option<Json>,
    pub agenda: Option<Json>,
    pub dry_run: Option<Json>,
    #[serde(flatten)]
    pub context: WriteContext,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CheckNotesRequest {
    pub date: Option<Json>,
    #[serde(flatten)]
    pub context: WriteContext,
}

/// `date=<YYYY-MM-DD>` 옵션. 없으면 오늘이다. 날 수로 돌려준다.
fn date_option(raw: &Option<Json>) -> Result<i64, EngineError> {
    let value = text_of(raw)
        .map(|r| trim(&r).to_string())
        .unwrap_or_default();
    if value.is_empty() {
        let today = local_clock(now_millis()).date;
        return Ok(days_from_civil(
            today.year,
            i64::from(today.month),
            i64::from(today.day),
        ));
    }
    parse_iso_date(&value).ok_or_else(|| {
        EngineError::invalid_argument(format!("date 은 YYYY-MM-DD 모양이어야 한다: {value}"))
    })
}

/// 읽기 층 메인 노트를 만든 시각 내림차순(같으면 경로 차례)으로
fn reading_notes(index: &crate::vault::index::NoteIndex) -> Vec<&NoteRecord> {
    let mut notes: Vec<&NoteRecord> = Vec::new();
    for note in index.root_notes() {
        if !note.meta.tier.is_reading() {
            continue;
        }
        notes.push(note);
    }
    notes.sort_by(|a, b| {
        b.meta
            .created
            .as_bytes()
            .cmp(a.meta.created.as_bytes())
            .then_with(|| {
                a.rel_path
                    .replace('/', "\u{1}")
                    .as_bytes()
                    .cmp(b.rel_path.replace('/', "\u{1}").as_bytes())
            })
    });
    notes
}

fn append(
    body: &mut String,
    notes: &[&NoteRecord],
    index: &crate::vault::index::NoteIndex,
    empty: &str,
) {
    if notes.is_empty() {
        body.push_str(empty);
        body.push('\n');
        return;
    }
    for note in notes {
        body.push_str(&format!(
            "- {} · {} ({})\n",
            note.meta.title,
            index
                .agendas
                .table
                .by_slug(&note.meta.agenda_slug)
                .map_or("", |a| a.title.as_str()),
            note.meta.doc_type
        ));
    }
}

pub fn plan_day(vault: &Path, request: &PlanDayRequest) -> Result<Json, EngineError> {
    let scope = WriteScope::begin(vault, "plan_day", &request.context)?;
    let date = date_option(&request.date)?;
    let dry_run = bool_option(&request.dry_run, "dryRun")?;
    let index = &scope.index;
    let notes = reading_notes(index);
    let date_text = iso_text(date);
    let previous_limit = iso_text(date - 1);
    let previous = notes
        .iter()
        .map(|n| n.meta.date.as_str())
        .filter(|d| d.as_bytes() <= previous_limit.as_bytes())
        .max_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
    let mut yesterday: Vec<&NoteRecord> = Vec::new();
    let mut today: Vec<&NoteRecord> = Vec::new();
    for note in &notes {
        if Some(note.meta.date.as_str()) == previous {
            yesterday.push(note);
        }
        if note.meta.date == date_text {
            today.push(note);
        }
    }
    yesterday.sort_by(|a, b| {
        compare_utf16(&a.meta.agenda_slug, &b.meta.agenda_slug)
            .then_with(|| compare_utf16(&a.meta.time, &b.meta.time))
            .then_with(|| compare_utf16(&a.meta.title, &b.meta.title))
    });
    today.sort_by(|a, b| {
        compare_utf16(&a.meta.time, &b.meta.time)
            .then_with(|| compare_utf16(&a.meta.agenda_slug, &b.meta.agenda_slug))
            .then_with(|| compare_utf16(&a.meta.title, &b.meta.title))
    });
    let title = format!("{date_text} 데일리 플랜");
    let mut body = format!(
        "# {title}\n\n## 어제 한 일{}\n",
        previous.map_or(String::new(), |p| format!(" ({p})"))
    );
    append(&mut body, &yesterday, index, "- (기록 없음)");
    body.push_str("\n## 오늘\n");
    append(&mut body, &today, index, "- (아직 예정 없음)");
    let agenda = text_of(&request.agenda)
        .map(|a| trim(&a).to_string())
        .unwrap_or_default();
    let mut result = Map::new();
    if agenda.is_empty() || dry_run {
        result.insert("saved".into(), json!(false));
        result.insert("date".into(), json!(date_text));
    } else {
        let create = CreateDocRequest {
            agenda: Some(agenda),
            doc_type: Some("plan".to_string()),
            title: Some(title),
            body: Some(body.clone()),
            date: Some(date_text),
            time: Some("09:00".to_string()),
            context: request.context.clone(),
            ..CreateDocRequest::default()
        };
        let saved = create_doc_in(vault, &scope, &create)?;
        result.insert("saved".into(), json!(true));
        for key in ["id", "agenda", "agendaCreated", "path"] {
            result.insert(key.into(), saved.get(key).cloned().unwrap_or(Json::Null));
        }
    }
    result.insert("body".into(), json!(body));
    Ok(Json::Object(result))
}

// ---------------------------------------------------------------- check_notes

static OFFSET_DATE_TIME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(?::(\d{2})(?:\.(\d{1,9}))?)?(Z|[+-]\d{2}:\d{2}(?::\d{2})?)$").unwrap()
});

/// `OffsetDateTime`: 그 오프셋의 달력 시각과 UTC 순간
#[derive(Clone, Copy)]
struct Odt {
    local_days: i64,
    hour: i64,
    minute: i64,
    second: i64,
    nanos: i64,
    offset_seconds: i64,
}

impl Odt {
    /// UTC 순간(나노초)
    fn instant(&self) -> i128 {
        let seconds = self.local_days * 86_400 + self.hour * 3600 + self.minute * 60 + self.second
            - self.offset_seconds;
        i128::from(seconds) * 1_000_000_000 + i128::from(self.nanos)
    }

    /// 분 아래를 버린 UTC 순간
    fn instant_minutes(&self) -> i128 {
        let seconds =
            self.local_days * 86_400 + self.hour * 3600 + self.minute * 60 - self.offset_seconds;
        i128::from(seconds)
    }

    /// `OffsetDateTime.toString()`: 초와 나노가 0이면 생략한다
    fn text(&self) -> String {
        let mut text = format!(
            "{}T{:02}:{:02}",
            iso_text(self.local_days),
            self.hour,
            self.minute
        );
        if self.second != 0 || self.nanos != 0 {
            text.push_str(&format!(":{:02}", self.second));
            if self.nanos != 0 {
                if self.nanos % 1_000_000 == 0 {
                    text.push_str(&format!(".{:03}", self.nanos / 1_000_000));
                } else if self.nanos % 1000 == 0 {
                    text.push_str(&format!(".{:06}", self.nanos / 1000));
                } else {
                    text.push_str(&format!(".{:09}", self.nanos));
                }
            }
        }
        if self.offset_seconds == 0 {
            text.push('Z');
        } else {
            let abs = self.offset_seconds.abs();
            text.push_str(&format!(
                "{}{:02}:{:02}",
                if self.offset_seconds < 0 { '-' } else { '+' },
                abs / 3600,
                abs % 3600 / 60
            ));
            if abs % 60 != 0 {
                text.push_str(&format!(":{:02}", abs % 60));
            }
        }
        text
    }

    /// `yyyy-MM-dd HH:mm`
    fn display(&self) -> String {
        format!(
            "{} {:02}:{:02}",
            iso_text(self.local_days),
            self.hour,
            self.minute
        )
    }
}

fn offset_date_time(text: &str) -> Option<Odt> {
    let c = OFFSET_DATE_TIME.captures(text)?;
    let num = |i: usize| c.get(i).and_then(|m| m.as_str().parse::<i64>().ok());
    let (year, month, day) = (num(1)?, num(2)?, num(3)?);
    let (hour, minute, second) = (num(4)?, num(5)?, num(6).unwrap_or(0));
    if !(1..=12).contains(&month) || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let local_days = days_from_civil(year, month, day);
    if crate::kernel::time::civil_from_days(local_days) != (year, month as u32, day as u32) {
        return None;
    }
    let nanos = c.get(7).map_or(0, |m| {
        m.as_str().parse::<i64>().unwrap_or(0) * 10_i64.pow(9 - m.as_str().len() as u32)
    });
    let offset = c.get(8)?.as_str();
    let offset_seconds = if offset.eq_ignore_ascii_case("z") {
        0
    } else {
        let sign = if offset.starts_with('-') { -1 } else { 1 };
        let part = |from: usize| {
            offset
                .get(from..from + 2)
                .and_then(|p| p.parse::<i64>().ok())
        };
        let (h, m, s) = (part(1)?, part(4)?, part(7).unwrap_or(0));
        if m > 59 || s > 59 || h * 3600 + m * 60 + s > 18 * 3600 {
            return None;
        }
        sign * (h * 3600 + m * 60 + s)
    };
    Some(Odt {
        local_days,
        hour,
        minute,
        second,
        nanos,
        offset_seconds,
    })
}

struct Pending {
    item: Json,
    comment: Odt,
    line: String,
}

pub fn check_notes(vault: &Path, request: &CheckNotesRequest) -> Result<Json, EngineError> {
    let scope = WriteScope::begin(vault, "check_notes", &request.context)?;
    let date = date_option(&request.date)?;
    let start = date - 14;
    let index = &scope.index;
    let notes = reading_notes(index);
    let mut stamps: HashMap<&str, Odt> = HashMap::new();
    for note in &notes {
        if note.format == crate::vault::scanner::Format::Json {
            continue;
        }
        let Ok(parsed) = read_note_file(&note.content_file) else {
            continue;
        };
        let Some(meta) = parsed.frontmatter else {
            continue;
        };
        let get = |key: &str| {
            meta.iter()
                .find(|(k, _)| matches!(k, Value::Str(s) if s == key))
                .map(|(_, v)| v)
        };
        let (Some(Value::Str(by)), Some(Value::Str(at))) = (get("editedBy"), get("editedAt"))
        else {
            continue;
        };
        if is_blank(by) {
            continue;
        }
        if let Some(stamp) = offset_date_time(trim(at)) {
            stamps.insert(note.id.as_str(), stamp);
        }
    }
    let earliest = stamps.values().map(Odt::instant).min();
    let mut pending: Vec<Pending> = Vec::new();
    for note in &notes {
        if note.is_single_file {
            continue;
        }
        let envelope = read_comments(vault, &vault.join(&note.rel_path).join("comments.json"));
        let CommentsEnvelope::Loaded { comments } = envelope else {
            continue;
        };
        let Some(comment) = comments
            .iter()
            .filter_map(|c| offset_date_time(&c.created_at))
            .max_by_key(Odt::instant)
        else {
            continue;
        };
        if comment.local_days < start {
            continue;
        }
        let created = offset_date_time(&note.meta.created);
        let (change, stamped) = match stamps.get(note.id.as_str()) {
            Some(change) => (*change, true),
            None => {
                let Some(created) = created else {
                    continue;
                };
                if earliest.is_some_and(|e| created.instant() < e) {
                    continue;
                }
                (created, false)
            }
        };
        if comment.instant_minutes() <= change.instant_minutes() {
            continue;
        }
        let agenda = index
            .agendas
            .table
            .by_slug(&note.meta.agenda_slug)
            .map(|a| a.title.clone());
        let mut item = Map::new();
        item.insert("check".into(), json!("unanswered-comments"));
        item.insert("checkDisplay".into(), json!("답 없는 코멘트"));
        item.insert("id".into(), json!(note.id));
        item.insert("title".into(), json!(note.meta.title));
        if let Some(agenda) = &agenda {
            item.insert("agenda".into(), json!(agenda));
        }
        item.insert("date".into(), json!(note.meta.date));
        item.insert("comments".into(), json!(comments.len()));
        item.insert("lastCommentAt".into(), json!(comment.text()));
        item.insert("lastChangeAt".into(), json!(change.text()));
        item.insert("stamped".into(), json!(stamped));
        let line = format!(
            "- 답 없는 코멘트: \"{}\"{}, {}. 코멘트 {}개, 마지막 {}. 그 뒤로 노트가 {}{}).",
            note.meta.title,
            agenda.map_or(String::new(), |a| format!(" ({a})")),
            note.meta.date,
            comments.len(),
            comment.display(),
            if stamped {
                "바뀌지 않았다(마지막 편집 "
            } else {
                "바뀐 기록이 없다(편집 도장 없음, 작성 "
            },
            change.display()
        );
        pending.push(Pending {
            item: Json::Object(item),
            comment,
            line,
        });
    }
    pending.sort_by_key(|p| std::cmp::Reverse(p.comment.instant()));
    Ok(json!({
        "date": iso_text(date),
        "windowDays": 14,
        "count": pending.len(),
        "section": pending.iter().map(|p| p.line.as_str()).collect::<Vec<_>>().join("\n"),
        "items": pending.into_iter().map(|p| p.item).collect::<Vec<_>>(),
    }))
}
