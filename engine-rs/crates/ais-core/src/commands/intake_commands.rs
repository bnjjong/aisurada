//! 매니저 제보 원장과 Slack 채널 커서를 vault에 저장한다.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

use super::task_commands::{self, AddTaskCommentRequest, CreateTaskRequest};
use crate::error::EngineError;
use crate::kernel::{
    actor::{CallContext, Caller, ManagerContext},
    time::{LocalClock, days_from_civil},
};
use crate::ports::{clock, fs, ids};
use crate::task::{RefKind, Task, format, store};
use crate::vault::check_manager_project;
use crate::write::{marker::check_workspace_marker, write_atomic};

const MINUTE: i64 = 60_000;
const DAY_MINUTES: u16 = 24 * 60;
const QUESTION_LIMIT: usize = 2;
const ANSWER_WAIT: i64 = 3 * 24 * 60 * MINUTE;
const WEEK_SECONDS: u64 = 7 * 24 * 60 * 60;
const LEDGER_PATH: &str = ".aisurada/intake/ledger.json";

/// 일반 노트 MCP에 공개하지 않는 매니저 접수 명령이다.
#[derive(Clone, Copy)]
pub enum IntakeOperation {
    Get,
    Submit,
    Complete,
    Record,
    Move,
    Configure,
    Rewind,
    Review,
    Sources,
    Questions,
}

#[derive(Default, Deserialize, Serialize)]
struct Ledger {
    channels: BTreeMap<String, Channel>,
    entries: BTreeMap<String, Entry>,
    #[serde(skip)]
    task_sources: BTreeMap<String, Vec<String>>,
    #[serde(skip)]
    repeat_counts: BTreeMap<String, usize>,
    #[serde(skip)]
    notice_entries: BTreeSet<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Channel {
    id: String,
    label: String,
    #[serde(default, skip_serializing_if = "ChannelVisibility::is_unknown")]
    visibility: ChannelVisibility,
    emoji: String,
    mark_processed: bool,
    paused: bool,
    interval_minutes: u16,
    enabled_at: i64,
    cursor: String,
    revision: String,
    last_check_at: Option<i64>,
    last_check_via: Option<ProcessVia>,
    last_scheduled_at: Option<i64>,
    #[serde(default)]
    schedule: Option<Schedule>,
}

#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum ChannelVisibility {
    #[default]
    Unknown,
    Public,
    Private,
}

impl ChannelVisibility {
    fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Schedule {
    days: Vec<u8>,
    start_minute: u16,
    end_minute: u16,
}

impl Schedule {
    fn validate(&mut self) -> Result<(), EngineError> {
        self.days.sort_unstable();
        self.days.dedup();
        if self.days.is_empty()
            || self.days.iter().any(|day| !(1..=7).contains(day))
            || self.start_minute >= self.end_minute
            || self.end_minute > DAY_MINUTES
        {
            return Err(invalid("예약 확인 요일과 시작·종료 시각을 확인해 주세요"));
        }
        Ok(())
    }

    fn includes_day(&self, day: i64) -> bool {
        // 1970-01-01은 목요일이고, 설정은 월요일을 1로 센다.
        self.days.contains(&((day + 3).rem_euclid(7) as u8 + 1))
    }

    fn window(&self, day: i64, offset: i64) -> (i64, i64) {
        let at = |minute| {
            let wall = (day * i64::from(DAY_MINUTES) + i64::from(minute)) * MINUTE;
            let guess = wall - offset * 1000;
            wall - clock().local_clock(guess).offset_seconds * 1000
        };
        (at(self.start_minute), at(self.end_minute))
    }
}

#[derive(Default)]
enum ScheduleUpdate {
    #[default]
    Keep,
    Set(Option<Schedule>),
}

impl<'de> Deserialize<'de> for ScheduleUpdate {
    fn deserialize<D: Deserializer<'de>>(reader: D) -> Result<Self, D::Error> {
        Option::<Schedule>::deserialize(reader).map(Self::Set)
    }
}

fn local_day(local: LocalClock) -> i64 {
    let date = local.date;
    days_from_civil(date.year, i64::from(date.month), i64::from(date.day))
}

impl Channel {
    fn stalled_minutes(&self, now: i64, local: LocalClock) -> Option<i64> {
        let schedule = self.schedule.as_ref().filter(|_| !self.paused)?;
        let mut day = local_day(local);
        let minute = local.hour * 60 + local.minute;
        if !schedule.includes_day(day)
            || minute < u32::from(schedule.start_minute)
            || minute >= u32::from(schedule.end_minute)
        {
            return None;
        }
        // 자정을 넘어 연속으로 켠 예약은 같은 확인 구간으로 본다.
        let all_day = schedule.start_minute == 0 && schedule.end_minute == DAY_MINUTES;
        let window_start = if all_day && schedule.days.len() == 7 {
            self.enabled_at
        } else {
            while all_day && schedule.includes_day(day - 1) {
                day -= 1;
            }
            schedule.window(day, local.offset_seconds).0
        };
        let checked = self.last_check_at.unwrap_or(self.enabled_at);
        let elapsed = (now - checked.max(self.enabled_at).max(window_start)) / MINUTE;
        (elapsed >= (i64::from(self.interval_minutes) * 3).max(30)).then_some(elapsed)
    }

    fn next_check_at(&self, now: i64, local: LocalClock) -> Option<i64> {
        let schedule = self.schedule.as_ref().filter(|_| !self.paused)?;
        let today = local_day(local);
        let interval = i64::from(self.interval_minutes.max(1)) * MINUTE;
        (today..=today + 7).filter(|day| schedule.includes_day(*day)).find_map(|day| {
            let (start, end) = schedule.window(day, local.offset_seconds);
            let base = self.last_scheduled_at.filter(|time| *time >= start && *time < end)
                .map_or(start, |time| time + interval);
            let next = if base > now { base } else { base + ((now - base) / interval + 1) * interval };
            (next < end).then_some(next)
        })
    }
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ProcessVia {
    Desktop,
    Scheduled,
    Manual,
    Direct,
}

#[derive(Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ResultKind {
    Created,
    Commented,
    Asked,
    Ignored,
    Pending,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<DirectSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    destination: Option<SourceDestination>,
    channel: String,
    message_ts: String,
    url: String,
    reporter: String,
    excerpt: String,
    manager: String,
    project_id: String,
    received_at: i64,
    task_key: Option<String>,
    task_id: Option<String>,
    task_origin: Option<ResultKind>,
    reaction_added: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    reactions: Vec<SlackReaction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    report_reaction: Option<ReportReaction>,
    #[serde(default)]
    mark_requested: bool,
    reply_ts: Option<String>,
    history: Vec<Processing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    work: Option<DirectWork>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct DirectWork {
    result: ResultKind,
    task_id: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct DirectSource {
    kind: String,
    key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    posted_at: Option<i64>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SourceDestination {
    conversation: String,
    message: String,
    thread: Option<String>,
}

impl SourceDestination {
    fn valid(&self) -> bool {
        let valid_id = |id: &str| !id.is_empty() && id.len() <= 128
            && id.bytes().all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b));
        valid_id(&self.conversation) && valid_id(&self.message) && self.thread.as_deref().is_none_or(valid_id)
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SlackReaction {
    name: String,
    count: u32,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReportReaction {
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    at: Option<i64>,
}

#[derive(Clone, Copy, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
enum AttentionReason {
    QuestionLimit,
    NoAnswer,
}

#[derive(Clone, Copy, Serialize)]
struct Attention {
    reason: AttentionReason,
    at: i64,
}

fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Processing {
    revision: String,
    result: ResultKind,
    at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    checked_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    searched_at: Option<i64>,
    via: ProcessVia,
    decision: String,
    question: String,
    #[serde(default, skip_serializing_if = "is_false")]
    question_limit: bool,
}

impl Entry {
    fn source_time(&self) -> i64 {
        let posted_at = match &self.source {
            Some(source) => source.posted_at,
            None => timestamp(&self.message_ts).ok().map(|at| (at / 1000) as i64),
        };
        posted_at.unwrap_or(self.received_at)
    }

    fn current(&self) -> Result<&Processing, EngineError> {
        self.history
            .last()
            .ok_or_else(|| EngineError::internal("접수 항목에 처리 이력이 없다"))
    }

    fn view(&self) -> Result<Value, EngineError> {
        let current = self.current()?;
        let mut view = json!({ "entry": self, "current": current });
        if let Some(attention) = current.attention(clock().now_millis()) {
            view["attention"] = json!(attention);
        }
        Ok(view)
    }
}

impl Processing {
    fn attention(&self, now: i64) -> Option<Attention> {
        if self.question_limit {
            return Some(Attention { reason: AttentionReason::QuestionLimit, at: self.at });
        }
        let deadline = self.at.saturating_add(ANSWER_WAIT);
        (self.result == ResultKind::Asked && now >= deadline).then_some(Attention {
            reason: AttentionReason::NoAnswer,
            at: deadline,
        })
    }
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
struct Query {
    channel: Option<String>,
    message_ts: Option<String>,
    entry_id: Option<String>,
    summary: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Submit {
    source: SubmittedSource,
    reporter: String,
    excerpt: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SubmittedSource {
    kind: String,
    key: String,
    posted_at: Option<i64>,
    url: Option<String>,
    destination: Option<SourceDestination>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Complete {
    entry_id: String,
    prior_revision: String,
    outcome: Outcome,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Outcome {
    result: ResultKind,
    #[serde(default)]
    decision: String,
    #[serde(default)]
    question: String,
    task: Option<CreateTaskRequest>,
    task_key: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Record {
    channel: String,
    message_ts: String,
    url: String,
    reporter: String,
    excerpt: String,
    result: ResultKind,
    via: ProcessVia,
    checked_at: Option<i64>,
    searched_at: Option<i64>,
    reactions: Option<Vec<SlackReaction>>,
    report_reaction: Option<ReportReaction>,
    #[serde(default)]
    decision: String,
    #[serde(default)]
    question: String,
    task: Option<CreateTaskRequest>,
    task_key: Option<String>,
    prior_revision: Option<String>,
    reply_ts: Option<String>,
    reaction_added: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Move {
    channel: String,
    revision: String,
    message_ts: Option<String>,
    via: ProcessVia,
    #[serde(default)]
    report_timestamps: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Configure {
    channel: String,
    label: Option<String>,
    visibility: Option<ChannelVisibility>,
    emoji: Option<String>,
    mark_processed: Option<bool>,
    paused: Option<bool>,
    interval_minutes: Option<u16>,
    #[serde(default)]
    create_only: bool,
    #[serde(default)]
    schedule: ScheduleUpdate,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Rewind {
    channel: String,
    message_ts: String,
    revision: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Review {
    id: String,
    revision: String,
    result: ResultKind,
    task: Option<CreateTaskRequest>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SourceQuery {
    task_key: String,
}

impl IntakeOperation {
    pub(crate) fn is_write(self) -> bool {
        !matches!(self, Self::Get | Self::Sources | Self::Questions)
    }
}

pub(crate) fn authorize(
    vault: &Path,
    operation: IntakeOperation,
    context: &CallContext,
) -> Result<(), EngineError> {
    check_workspace_marker(vault)?;
    if matches!(operation, IntakeOperation::Questions) && context.caller != Caller::App {
        return Err(invalid("되물음 알림은 앱에서 조회한다"));
    }
    if matches!(operation, IntakeOperation::Sources) {
        if context.caller != Caller::App || context.manager.is_some() {
            return Err(invalid("태스크 출처는 앱에서 조회한다"));
        }
        return Ok(());
    }
    let manager = context.manager.as_ref().ok_or_else(|| invalid("등록한 매니저가 필요하다"))?;
    check_manager_project(vault, manager)?;
    if operation.is_write() {
        let manual = matches!(operation, IntakeOperation::Configure | IntakeOperation::Rewind | IntakeOperation::Review);
        if manual != (context.caller == Caller::App) {
            return Err(invalid("이 접수 명령을 현재 호출자가 쓸 수 없다"));
        }
    }
    Ok(())
}

pub(crate) fn execute(
    vault: &Path,
    operation: IntakeOperation,
    context: &CallContext,
    allowed_channels: &[String],
    args: Value,
) -> Result<Value, EngineError> {
    if matches!(operation, IntakeOperation::Sources) {
        let query: SourceQuery = parse("intake_sources", args)?;
        let scan = store::scan(vault);
        let task = store::find(&scan, &query.task_key)?;
        return sources_for_task(vault, &task.id);
    }
    let path = ledger_path(vault)?;
    let manager = context.manager.as_ref().ok_or_else(|| invalid("등록한 매니저가 필요하다"))?;
    if matches!(operation, IntakeOperation::Get) {
        return Ledger::read(&path)?.get(vault, manager, allowed_channels, parse("get_intake", args)?, context.caller);
    }
    if matches!(operation, IntakeOperation::Questions) {
        return Ledger::read(&path)?.questions(manager, allowed_channels);
    }
    let mut ledger = Ledger::load(&path)?;
    let (response, changed) = match operation {
        IntakeOperation::Submit => ledger.submit(manager, parse("submit_intake", args)?)?,
        IntakeOperation::Complete => ledger.complete(
            vault,
            &path,
            context,
            manager,
            parse("complete_intake", args)?,
        )?,
        IntakeOperation::Record => ledger.record(
            vault,
            context,
            manager,
            allowed_channels,
            parse("record_intake", args)?,
        )?,
        IntakeOperation::Move => {
            ledger.advance(manager, allowed_channels, parse("move_cursor", args)?)?
        }
        IntakeOperation::Configure => {
            ledger.configure(manager, allowed_channels, parse("configure_intake", args)?)?
        }
        IntakeOperation::Rewind => {
            ledger.rewind(manager, allowed_channels, parse("rewind_cursor", args)?)?
        }
        IntakeOperation::Review => ledger.review(vault, context, manager, parse("review_intake", args)?)?,
        IntakeOperation::Get | IntakeOperation::Sources | IntakeOperation::Questions => unreachable!(),
    };
    if changed {
        ledger.save(&path)?;
    }
    Ok(response)
}

fn parse<T: DeserializeOwned>(name: &str, args: Value) -> Result<T, EngineError> {
    serde_json::from_value(args)
        .map_err(|error| EngineError::invalid_argument(format!("{name} 인자가 잘못됐다: {error}")))
}

fn invalid(message: &str) -> EngineError {
    EngineError::invalid_argument(message)
}

fn ledger_path(vault: &Path) -> Result<PathBuf, EngineError> {
    let path = vault.join(LEDGER_PATH);
    fs::check_confinement(vault, &path)?;
    Ok(path)
}

fn timestamp(value: &str) -> Result<u64, EngineError> {
    let (seconds, micros) = value
        .split_once('.')
        .ok_or_else(|| invalid("Slack 메시지 시각(초.마이크로초)이 필요하다"))?;
    if seconds.len() != 10
        || micros.len() != 6
        || !seconds
            .bytes()
            .chain(micros.bytes())
            .all(|byte| byte.is_ascii_digit())
    {
        return Err(invalid(
            "Slack 메시지 시각은 초 10자리와 마이크로초 6자리여야 한다",
        ));
    }
    Ok(seconds.parse::<u64>().unwrap() * 1_000_000 + micros.parse::<u64>().unwrap())
}

fn slack_channel(value: &str) -> bool {
    (3..=80).contains(&value.len())
        && value.starts_with(['C', 'G', 'D'])
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

fn slack_emoji(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 80
        && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"_+-".contains(&byte))
}

fn slack_reaction_name(name: &str) -> bool {
    name.len() <= 80 && name.split("::").all(slack_emoji)
}

fn check_channel(allowed: &[String], channel: &str) -> Result<(), EngineError> {
    if !slack_channel(channel) || !allowed.iter().any(|value| value == channel) {
        return Err(invalid("매니저에 등록된 Slack 채널 ID가 필요하다"));
    }
    Ok(())
}

fn scoped_key(project_id: &str, manager: &str, id: &str) -> String {
    format!("{project_id}/{manager}/{id}")
}

fn manager_key(manager: &ManagerContext, id: &str) -> String {
    scoped_key(&manager.project_id, &manager.name, id)
}

fn revision() -> String {
    format!("{:016x}", ids().next_random_u64())
}

fn check_revision(actual: &str, expected: &str) -> Result<(), EngineError> {
    if actual != expected {
        return Err(EngineError::precondition_failed(
            "접수 기록이 바뀌었다. 다시 읽어 주세요",
        ));
    }
    Ok(())
}

fn agent_via(via: ProcessVia) -> Result<(), EngineError> {
    if !matches!(via, ProcessVia::Desktop | ProcessVia::Scheduled) {
        return Err(invalid("처리 경로는 desktop 또는 scheduled여야 한다"));
    }
    Ok(())
}

fn short(value: &str, limit: usize) -> String {
    value
        .trim()
        .chars()
        .filter(|character| !character.is_control() || *character == '\n')
        .take(limit)
        .collect()
}

fn source_matches(url: &str, channel: &str, message_ts: &str) -> bool {
    let bare = url.split('?').next().unwrap_or_default();
    format::slack_thread(bare).is_some_and(|(id, ts)| id == channel && ts == message_ts)
        && format::slack_thread(url).is_some_and(|(_, ts)| ts == message_ts)
}

fn direct_source_url(url: &str) -> bool {
    let Some(address) = url.strip_prefix("https://") else {
        return false;
    };
    let authority = address.split(['/', '?', '#']).next().unwrap_or_default();
    url.len() <= 2048
        && !authority.is_empty()
        && !authority.contains('@')
        && !url.chars().any(|c| c.is_whitespace() || c.is_control() || c == '\\')
}

struct CachedLedger {
    path: PathBuf,
    stamp: (SystemTime, u64),
    ledger: Arc<Ledger>,
}

// 앱의 여러 조회가 같은 파일을 읽는다. 마지막으로 연 원장 하나만 재사용한다.
static READ_LEDGER: Mutex<Option<CachedLedger>> = Mutex::new(None);

fn file_stamp(path: &Path) -> Option<(SystemTime, u64)> {
    let meta = fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

pub(super) fn sources_for_task(vault: &Path, task_id: &str) -> Result<Value, EngineError> {
    let ledger = Ledger::read(&ledger_path(vault)?)?;
    let Some(keys) = ledger.task_sources.get(task_id) else {
        return Ok(json!([]));
    };
    let mut rows: Vec<_> = keys
        .iter()
        .filter_map(|key| ledger.entries.get(key))
        .collect();
    rows.sort_by_key(|entry| (entry.task_origin != Some(ResultKind::Created), entry.source_time()));
    // 같은 프로젝트의 매니저가 같은 Slack 출처를 연결하면 화면에는 한 번만 싣는다.
    let mut shown = HashSet::new();
    rows.retain(|entry| shown.insert(&entry.id));
    Ok(json!(rows))
}

impl Ledger {
    fn read(path: &Path) -> Result<Arc<Self>, EngineError> {
        let mut cache = READ_LEDGER.lock().unwrap_or_else(|error| error.into_inner());
        let stamp = file_stamp(path);
        if let Some(saved) = cache.as_ref()
            && saved.path == path && stamp == Some(saved.stamp)
        {
            return Ok(Arc::clone(&saved.ledger));
        }
        let mut ledger = Self::load(path)?;
        for (key, entry) in &ledger.entries {
            if entry.history.last().is_some_and(|event| event.result == ResultKind::Asked || event.question_limit) {
                ledger.notice_entries.insert(key.clone());
            }
            if let Some(task) = &entry.task_id {
                ledger.task_sources.entry(task.clone()).or_default().push(key.clone());
            }
        }
        for (task, keys) in &ledger.task_sources {
            let reports: HashSet<_> = keys.iter().filter_map(|key| {
                let entry = &ledger.entries[key];
                (entry.task_origin == Some(ResultKind::Commented)).then_some(&entry.id)
            }).collect();
            ledger.repeat_counts.insert(task.clone(), reports.len());
        }
        let ledger = Arc::new(ledger);
        if let Some(stamp) = stamp
            && file_stamp(path) == Some(stamp)
        {
            *cache = Some(CachedLedger { path: path.to_owned(), stamp, ledger: Arc::clone(&ledger) });
        }
        Ok(ledger)
    }

    fn load(path: &Path) -> Result<Self, EngineError> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Self::default()),
            Err(error) => return Err(EngineError::internal(format!("접수 원장을 읽지 못했다: {error}"))),
        };
        let mut ledger: Self = serde_json::from_slice(&bytes).map_err(|error|
            EngineError::internal(format!("접수 원장을 읽지 못했다: {error}")))?;
        // 이전 원장은 채널과 시각만 키로 썼다. 원글 ID는 알림과 화면에서 계속 쓴다.
        let mut legacy = Vec::new();
        for (key, entry) in &ledger.entries {
            let scoped = scoped_key(&entry.project_id, &entry.manager, &entry.id);
            if *key == entry.id {
                legacy.push((key.clone(), scoped));
            } else if *key != scoped {
                return Err(invalid("접수 항목의 저장 위치와 매니저가 일치하지 않는다"));
            }
        }
        for (key, scoped) in legacy {
            let entry = ledger.entries.remove(&key).unwrap();
            ledger.entries.entry(scoped).or_insert(entry);
        }
        for channel in ledger.channels.values_mut() {
            if let Some(schedule) = &mut channel.schedule {
                schedule.validate()?;
            }
        }
        Ok(ledger)
    }

    fn save(&self, path: &Path) -> Result<(), EngineError> {
        let mut bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| EngineError::internal(error.to_string()))?;
        bytes.push(b'\n');
        write_atomic(path, &bytes)
    }

    fn channel(
        &self,
        manager: &ManagerContext,
        allowed: &[String],
        id: &str,
    ) -> Result<&Channel, EngineError> {
        check_channel(allowed, id)?;
        self.channels
            .get(&manager_key(manager, id))
            .ok_or_else(|| invalid("연결 탭에서 제보 채널을 먼저 설정해 주세요"))
    }

    fn questions(&self, manager: &ManagerContext, allowed: &[String]) -> Result<Value, EngineError> {
        let prefix = manager_key(manager, "");
        let now = clock().now_millis();
        let mut notices = Vec::new();
        for key in self.notice_entries.range(prefix.clone()..).take_while(|key| key.starts_with(&prefix)) {
            let entry = &self.entries[key];
            let event = entry.current()?;
            let attention = event.attention(now);
            if attention.is_none() && entry.source.is_none() && !allowed.contains(&entry.channel) {
                continue;
            }
            let (id, at) = match attention {
                Some(item) => (format!("intake-review-{}-{}", manager.name, event.revision), item.at),
                None => (format!("intake-question-{}", event.revision), event.at),
            };
            let mut notice = json!({
                "id": id,
                "entry": entry.id,
                "at": at,
                "excerpt": short(&entry.excerpt, 100),
                "url": entry.source.is_none().then_some(entry.url.as_str()),
            });
            if let Some(item) = attention {
                notice["reviewReason"] = json!(item.reason);
            } else {
                notice["question"] = json!(short(&event.question, 140));
            }
            notices.push(notice);
        }
        Ok(json!(notices))
    }

    fn get(
        &self,
        vault: &Path,
        manager: &ManagerContext,
        allowed: &[String],
        query: Query,
        caller: Caller,
    ) -> Result<Value, EngineError> {
        if let Some(entry_id) = &query.entry_id {
            if query.channel.is_some() || query.message_ts.is_some() || query.summary {
                return Err(invalid("접수 번호만 지정해 주세요"));
            }
            let existing = self
                .entries
                .get(&manager_key(manager, entry_id))
                .filter(|entry| entry.source.is_some());
            return Ok(json!({
                "recorded": existing.is_some(),
                "item": existing.map(Entry::view).transpose()?
            }));
        }
        if let Some(id) = &query.channel {
            check_channel(allowed, id)?;
        }
        if let Some(ts) = &query.message_ts {
            timestamp(ts)?;
            let id = query
                .channel
                .as_deref()
                .ok_or_else(|| invalid("메시지 시각과 채널을 함께 지정한다"))?;
            self.channel(manager, allowed, id)?;
            let existing = self.entries.get(&manager_key(manager, &format!("{id}:{ts}")));
            return Ok(json!({
                "recorded": existing.is_some(),
                "item": existing.map(Entry::view).transpose()?
            }));
        }
        let now = clock().now_millis();
        let local = clock().local_clock(now);
        let channels: Vec<_> = allowed
            .iter()
            .filter(|id| query.channel.as_ref().is_none_or(|channel| channel == *id))
            .filter_map(|id| self.channels.get(&manager_key(manager, id)))
            .collect();
        let mut views = Vec::new();
        let mut pending = Vec::new();
        let mut direct_pending = Vec::new();
        let mut unmarked = Vec::new();
        let mut asked = Vec::new();
        let mut attention_items = Vec::new();
        let mut counts = [0_u64; 5];
        let mut waiting = 0;
        let mut rows_revision = DefaultHasher::new();
        let mut repos = BTreeMap::new();
        let prefix = manager_key(manager, "");
        let entries = self.entries.range(prefix.clone()..)
            .take_while(|(key, _)| key.starts_with(&prefix))
            .map(|(_, entry)| entry);
        for entry in entries.filter(|entry| {
            (entry.source.is_some() || caller == Caller::App || allowed.contains(&entry.channel))
                && query.channel.as_ref().is_none_or(|channel| channel == &entry.channel)
        }) {
            let event = entry.current()?;
            let attention = event.attention(now);
            attention.map(|item| item.reason).hash(&mut rows_revision);
            entry.id.hash(&mut rows_revision);
            event.revision.hash(&mut rows_revision);
            entry.reaction_added.hash(&mut rows_revision);
            let repeat_count = entry.task_id.as_ref().and_then(|id| self.repeat_counts.get(id));
            repeat_count.hash(&mut rows_revision);
            if entry.received_at >= now - 48 * 60 * MINUTE
                && clock().local_clock(entry.received_at).date == local.date
            {
                counts[0] += 1;
                match event.result {
                    ResultKind::Created => counts[1] += 1,
                    ResultKind::Commented => counts[2] += 1,
                    ResultKind::Asked if attention.is_none() => counts[3] += 1,
                    ResultKind::Asked => {},
                    ResultKind::Ignored => counts[4] += 1,
                    ResultKind::Pending => {}
                }
            }
            if let Some(attention) = attention {
                attention_items.push(json!({
                    "id": entry.id, "at": attention.at, "reason": attention.reason,
                    "revision": event.revision, "url": entry.url,
                    "excerpt": (caller == Caller::App).then(|| short(&entry.excerpt, 100)),
                }));
            } else if event.result == ResultKind::Asked {
                waiting += 1;
                if allowed.contains(&entry.channel) {
                    let mut question = json!({
                        "id": entry.id, "at": event.at, "revision": event.revision, "url": entry.url
                    });
                    if caller == Caller::App {
                        question["excerpt"] = json!(short(&entry.excerpt, 100));
                        question["question"] = json!(short(&event.question, 140));
                    }
                    asked.push(question);
                    if !query.summary {
                        pending.push(entry.view()?);
                    }
                }
            } else if event.result == ResultKind::Pending && !query.summary {
                if entry.source.is_some() {
                    direct_pending.push(entry.view()?);
                } else if allowed.contains(&entry.channel) {
                    pending.push(entry.view()?);
                }
            }
            if caller == Caller::Ai
                && !query.summary
                && !entry.reaction_added
                && entry.mark_requested
                && matches!(
                    event.result,
                    ResultKind::Created | ResultKind::Commented | ResultKind::Ignored
                )
                && channels.iter().any(|channel| {
                    channel.id == entry.channel && !channel.paused && channel.mark_processed
                })
            {
                unmarked.push(json!({
                    "channel": entry.channel, "messageTs": entry.message_ts, "url": entry.url,
                    "revision": event.revision, "reporter": entry.reporter, "excerpt": entry.excerpt,
                    "result": event.result
                }));
            }
            if caller == Caller::App && !query.summary {
                let mut view = entry.view()?;
                if let Some((key, id)) = entry.task_key.as_deref().zip(entry.task_id.as_deref()) {
                    let repo = repos.entry(key).or_insert_with(|| {
                        store::read_by_key(vault, key)
                            .filter(|task| task.id == id)
                            .and_then(|task| task.repo)
                    });
                    view["taskRepo"] = json!(repo);
                }
                if event.result == ResultKind::Commented {
                    view["repeatCount"] = json!(repeat_count);
                }
                views.push((event.at, view));
            }
        }
        views.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
        let stalled_minutes = channels.iter().filter_map(|channel| channel.stalled_minutes(now, local)).max();
        let channels: Vec<_> = channels.iter().map(|channel| {
            let mut view = json!(channel);
            view["stalledMinutes"] = json!(channel.stalled_minutes(now, local));
            view["nextCheckAt"] = json!(channel.next_check_at(now, local));
            view
        }).collect();
        Ok(json!({
            "checkedAt": now, "channels": channels, "rows": views.into_iter().map(|(_, view)| view).collect::<Vec<_>>(),
            "rowsRevision": format!("{:016x}", rows_revision.finish()),
            "pending": pending, "directPending": direct_pending, "unmarked": unmarked,
            "asked": asked, "waiting": waiting, "counts": counts,
            "attention": attention_items,
            "stalledMinutes": stalled_minutes
        }))
    }

    fn configure(
        &mut self,
        manager: &ManagerContext,
        allowed: &[String],
        mut request: Configure,
    ) -> Result<(Value, bool), EngineError> {
        check_channel(allowed, &request.channel)?;
        let emoji = request.emoji.as_deref().map(|value| value.trim().trim_matches(':'));
        if emoji.is_some_and(|name| !slack_emoji(name))
            || request.interval_minutes.is_some_and(|minutes| !(1..=1440).contains(&minutes))
        {
            return Err(invalid("제보 기준 이모지 이름과 확인 간격(1~1440분)을 확인해 주세요"));
        }
        if let ScheduleUpdate::Set(Some(schedule)) = &mut request.schedule {
            schedule.validate()?;
        }
        let now = clock().now_millis();
        let key = manager_key(manager, &request.channel);
        let channel = match self.channels.entry(key) {
            std::collections::btree_map::Entry::Occupied(_) if request.create_only => {
                return Err(invalid("이미 설정한 제보 채널이다"));
            }
            std::collections::btree_map::Entry::Occupied(existing) => existing.into_mut(),
            std::collections::btree_map::Entry::Vacant(new) => {
                if request.label.is_none() || emoji.is_none() || request.mark_processed.is_none()
                    || request.paused.is_none() || request.interval_minutes.is_none()
                {
                    return Err(invalid("새 제보 채널에는 채널 이름, 이모지, 처리 표시, 정지 여부와 간격을 지정한다"));
                }
                new.insert(Channel {
                    id: request.channel.clone(),
                    label: String::new(),
                    visibility: ChannelVisibility::Unknown,
                    emoji: String::new(),
                    mark_processed: true,
                    paused: false,
                    interval_minutes: 10,
                    enabled_at: now,
                    cursor: format!("{:010}.{:06}", now / 1000, (now % 1000) * 1000),
                    revision: String::new(),
                    last_check_at: None,
                    last_check_via: None,
                    last_scheduled_at: None,
                    schedule: None,
                })
            }
        };
        let paused = request.paused.unwrap_or(channel.paused);
        let mut restart_monitoring = channel.paused && !paused;
        if let ScheduleUpdate::Set(schedule) = request.schedule {
            restart_monitoring |= channel.schedule != schedule;
            channel.schedule = schedule;
        }
        if restart_monitoring {
            channel.enabled_at = now;
            channel.last_scheduled_at = None;
        }
        if let Some(label) = request.label {
            channel.label = short(&label, 80);
        }
        if let Some(visibility) = request.visibility {
            channel.visibility = visibility;
        }
        if let Some(emoji) = emoji {
            channel.emoji = emoji.into();
        }
        channel.mark_processed = request.mark_processed.unwrap_or(channel.mark_processed);
        channel.paused = paused;
        channel.interval_minutes = request.interval_minutes.unwrap_or(channel.interval_minutes);
        channel.revision = revision();
        Ok((json!(channel), true))
    }

    fn advance(
        &mut self,
        manager: &ManagerContext,
        allowed: &[String],
        request: Move,
    ) -> Result<(Value, bool), EngineError> {
        agent_via(request.via)?;
        let channel = self.channel(manager, allowed, &request.channel)?;
        check_revision(&channel.revision, &request.revision)?;
        if channel.paused {
            return Err(invalid("이 제보 채널은 멈춰 있다"));
        }
        let next = request.message_ts.as_deref().unwrap_or(&channel.cursor);
        let next_ts = timestamp(next)?;
        let now = clock().now_millis();
        if next_ts < timestamp(&channel.cursor)? || next_ts > now.max(0) as u64 * 1000 + 1_000_000 {
            return Err(invalid(
                "커서는 현재 위치부터 지금까지의 범위에서만 앞으로 옮긴다",
            ));
        }
        for ts in &request.report_timestamps {
            if timestamp(ts)? > next_ts
                || !self
                    .entries
                    .contains_key(&manager_key(manager, &format!("{}:{ts}", request.channel)))
            {
                return Err(invalid(
                    "커서를 옮기기 전에 확인한 제보를 모두 원장에 기록한다",
                ));
            }
        }
        let channel = self
            .channels
            .get_mut(&manager_key(manager, &request.channel))
            .unwrap();
        if let Some(ts) = request.message_ts {
            channel.cursor = ts;
        }
        channel.revision = revision();
        channel.last_check_at = Some(now);
        channel.last_check_via = Some(request.via);
        if request.via == ProcessVia::Scheduled {
            channel.last_scheduled_at = Some(now);
        }
        Ok((json!(channel), true))
    }

    fn rewind(
        &mut self,
        manager: &ManagerContext,
        allowed: &[String],
        request: Rewind,
    ) -> Result<(Value, bool), EngineError> {
        let channel = self.channel(manager, allowed, &request.channel)?;
        check_revision(&channel.revision, &request.revision)?;
        let target = timestamp(&request.message_ts)?;
        let now_seconds = (clock().now_millis().max(0) / 1000) as u64;
        if target >= timestamp(&channel.cursor)?
            || target / 1_000_000 < now_seconds.saturating_sub(WEEK_SECONDS)
        {
            return Err(invalid(
                "되감을 위치는 기존 커서보다 앞이고 최근 7일 안이어야 한다",
            ));
        }
        let channel = self
            .channels
            .get_mut(&manager_key(manager, &request.channel))
            .unwrap();
        channel.cursor = request.message_ts;
        channel.revision = revision();
        Ok((json!(channel), true))
    }

    fn review(
        &mut self,
        vault: &Path,
        context: &CallContext,
        manager: &ManagerContext,
        request: Review,
    ) -> Result<(Value, bool), EngineError> {
        if !matches!(request.result, ResultKind::Pending | ResultKind::Ignored | ResultKind::Created)
            || (request.result == ResultKind::Created) != request.task.is_some()
        {
            return Err(invalid("다시 처리, 무시 또는 새 태스크 내용이 있는 접수를 지정한다"));
        }
        let key = manager_key(manager, &request.id);
        let entry = self.entries.get_mut(&key)
            .ok_or_else(|| EngineError::not_found("접수 항목을 찾을 수 없다"))?;
        let current = entry.current()?;
        if request.result == ResultKind::Created
            && current.result == ResultKind::Created
            && current.via == ProcessVia::Manual
            && entry.history.iter().rev().nth(1).is_some_and(|previous| previous.revision == request.revision)
        {
            return Ok((entry.view()?, false));
        }
        check_revision(&current.revision, &request.revision)?;
        if entry.work.is_some() {
            return Err(invalid("매니저가 시작한 태스크 작업을 먼저 마쳐 주세요"));
        }
        let now = clock().now_millis();
        let attention = current.attention(now).is_some();
        if request.result == ResultKind::Created && !attention {
            return Err(invalid("사람이 확인할 제보만 원장에서 바로 접수할 수 있다"));
        }
        let decision = match request.result {
            ResultKind::Created => "사람이 초안으로 접수",
            ResultKind::Pending => "다시 처리 요청",
            _ => "무시로 표시",
        };
        let mut event = Processing {
            revision: revision(),
            result: request.result,
            at: now,
            checked_at: None,
            searched_at: None,
            via: ProcessVia::Manual,
            decision: decision.into(),
            question: String::new(),
            question_limit: false,
        };
        if request.result == ResultKind::Created {
            let outcome = Outcome {
                result: ResultKind::Created,
                decision: decision.into(),
                question: String::new(),
                task: request.task,
                task_key: None,
            };
            resolve(vault, context, manager, entry, &outcome, &mut event)?;
        }
        if attention && request.result != ResultKind::Pending {
            entry.mark_requested = self.channels.get(&manager_key(manager, &entry.channel))
                .is_some_and(|channel| channel.mark_processed);
        }
        entry.history.push(event);
        Ok((entry.view()?, true))
    }
}

impl Ledger {
    fn submit(
        &mut self,
        manager: &ManagerContext,
        request: Submit,
    ) -> Result<(Value, bool), EngineError> {
        let now = clock().now_millis();
        let SubmittedSource { kind, key, posted_at, url, destination } = request.source;
        let source = DirectSource { kind, key, posted_at };
        if source.kind == "slack"
            || source.kind.len() > 32
            || !source.kind.starts_with(|c: char| c.is_ascii_lowercase())
            || !source
                .kind
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            || source.key.is_empty()
            || source.key.len() > 256
            || source
                .key
                .chars()
                .any(|c| c.is_whitespace() || c.is_control())
            || source
                .posted_at
                .is_some_and(|at| at <= 0 || at > now + 1000)
        {
            return Err(invalid(
                "직접 제보의 출처 종류, 메시지 키, 원글 시각을 확인해 주세요",
            ));
        }
        if url.as_deref().is_some_and(|url| !direct_source_url(url)) {
            return Err(invalid("원글 주소는 사용자 정보나 공백이 없는 HTTPS 주소여야 한다"));
        }
        if destination.as_ref().is_some_and(|target| !target.valid()) {
            return Err(invalid("제보의 대화, 메시지, 주제 식별자를 확인해 주세요"));
        }
        let reporter = short(&request.reporter, 100);
        let excerpt = short(&request.excerpt, 200);
        if reporter.is_empty() || excerpt.is_empty() {
            return Err(invalid("제보자와 제보 내용을 입력해 주세요"));
        }
        let prefix = manager_key(manager, "intake-");
        let existing = self
            .entries
            .range(prefix.clone()..)
            .take_while(|(key, _)| key.starts_with(&prefix))
            .find_map(|(_, entry)| {
                entry
                    .source
                    .as_ref()
                    .filter(|stored| stored.kind == source.kind && stored.key == source.key)
                    .map(|_| entry)
            });
        if let Some(existing) = existing {
            return Ok((json!({"recorded": true, "item": existing.view()?}), false));
        }
        let id = loop {
            let candidate = format!("intake-{}", revision());
            if !self.entries.contains_key(&manager_key(manager, &candidate)) {
                break candidate;
            }
        };
        let entry = Entry {
            id: id.clone(),
            source: Some(source),
            destination,
            channel: String::new(),
            message_ts: String::new(),
            url: url.unwrap_or_default(),
            reporter,
            excerpt,
            manager: manager.name.clone(),
            project_id: manager.project_id.clone(),
            received_at: now,
            task_key: None,
            task_id: None,
            task_origin: None,
            reaction_added: false,
            reactions: Vec::new(),
            report_reaction: None,
            mark_requested: false,
            reply_ts: None,
            history: vec![Processing {
                revision: revision(),
                result: ResultKind::Pending,
                at: now,
                checked_at: None,
                searched_at: None,
                via: ProcessVia::Direct,
                decision: String::new(),
                question: String::new(),
                question_limit: false,
            }],
            work: None,
        };
        let response = json!({"recorded": false, "item": entry.view()?});
        self.entries.insert(manager_key(manager, &id), entry);
        Ok((response, true))
    }

    fn complete(
        &mut self,
        vault: &Path,
        path: &Path,
        context: &CallContext,
        manager: &ManagerContext,
        request: Complete,
    ) -> Result<(Value, bool), EngineError> {
        let key = manager_key(manager, &request.entry_id);
        let mut entry = self
            .entries
            .get(&key)
            .filter(|entry| entry.source.is_some())
            .cloned()
            .ok_or_else(|| invalid("이 매니저의 직접 제보 접수 번호가 필요하다"))?;
        let current = entry.current()?;
        if current.result != ResultKind::Pending || current.attention(clock().now_millis()).is_some() {
            return Ok((json!({"recorded": true, "item": entry.view()?}), false));
        }
        check_revision(&current.revision, &request.prior_revision)?;
        let outcome = request.outcome;
        if let Some(work) = &entry.work {
            if work.result != outcome.result {
                return Err(invalid("이미 시작한 태스크 작업의 결과를 다시 보내 주세요"));
            }
        } else if let Some(work) = prepare_direct_work(vault, context, manager, &outcome)? {
            // 결과를 쓰기 전에 중단돼도 원래 정한 태스크 작업을 이어 간다.
            entry.work = Some(work);
            self.entries.insert(key.clone(), entry.clone());
            self.save(path)?;
        }
        let mut event = Processing {
            revision: revision(),
            result: outcome.result,
            at: clock().now_millis(),
            checked_at: None,
            searched_at: None,
            via: ProcessVia::Direct,
            decision: String::new(),
            question: String::new(),
            question_limit: false,
        };
        resolve(vault, context, manager, &mut entry, &outcome, &mut event)?;
        entry.work = None;
        entry.history.push(event);
        let mut response = json!({"recorded": false, "item": entry.view()?});
        if entry.current()?.question_limit {
            response["questionAccepted"] = json!(false);
        }
        self.entries.insert(key, entry);
        Ok((response, true))
    }

    fn record(
        &mut self,
        vault: &Path,
        context: &CallContext,
        manager: &ManagerContext,
        allowed: &[String],
        mut request: Record,
    ) -> Result<(Value, bool), EngineError> {
        agent_via(request.via)?;
        let channel = self.channel(manager, allowed, &request.channel)?;
        if channel.paused {
            return Err(invalid("이 제보 채널은 멈춰 있다"));
        }
        let source_ts = timestamp(&request.message_ts)?;
        let now = clock().now_millis();
        if source_ts > now.max(0) as u64 * 1000 + 1_000_000 {
            return Err(invalid("현재보다 뒤의 메시지는 접수할 수 없다"));
        }
        if let Some(ts) = &request.reply_ts {
            let reply = timestamp(ts)?;
            if reply <= source_ts || reply > now.max(0) as u64 * 1000 + 1_000_000 {
                return Err(invalid(
                    "스레드 답변 시각은 원글 이후부터 지금까지의 범위여야 한다",
                ));
            }
        }
        let id = format!("{}:{}", request.channel, request.message_ts);
        let key = manager_key(manager, &id);
        let mut entry = if let Some(previous) = self.entries.get(&key) {
            let event = previous.current()?;
            let new_reply = request
                .reply_ts
                .as_ref()
                .is_some_and(|ts| previous.reply_ts.as_ref().is_none_or(|last| ts > last));
            let reopen = event.attention(now).is_none()
                && (event.result == ResultKind::Pending || (event.result == ResultKind::Asked && new_reply));
            if !reopen || request.prior_revision.is_none() {
                if request.reaction_added == Some(true)
                    && channel.mark_processed
                    && previous.mark_requested
                    && !previous.reaction_added
                    && matches!(
                        event.result,
                        ResultKind::Created | ResultKind::Commented | ResultKind::Ignored
                    )
                {
                    check_revision(
                        &event.revision,
                        request.prior_revision.as_deref().unwrap_or_default(),
                    )?;
                    let stored = self.entries.get_mut(&key).unwrap();
                    stored.reaction_added = true;
                    return Ok((json!({"recorded": true, "item": stored.view()?}), true));
                }
                return Ok((json!({"recorded": true, "item": previous.view()?}), false));
            }
            check_revision(
                &event.revision,
                request.prior_revision.as_deref().unwrap_or_default(),
            )?;
            previous.clone()
        } else {
            if request.prior_revision.is_some()
                || request.reply_ts.is_some()
                || request.reaction_added.is_some()
            {
                return Err(invalid("새 제보에는 원글 처리 결과만 기록한다"));
            }
            if !source_matches(&request.url, &request.channel, &request.message_ts)
                || request.reporter.trim().is_empty()
            {
                return Err(invalid("등록 채널의 Slack 원글 주소와 제보자가 필요하다"));
            }
            Entry {
                id: id.clone(),
                source: None,
                destination: None,
                channel: request.channel.clone(),
                message_ts: request.message_ts.clone(),
                url: request.url.split('?').next().unwrap_or_default().into(),
                reporter: short(&request.reporter, 100),
                excerpt: short(&request.excerpt, 200),
                manager: manager.name.clone(),
                project_id: manager.project_id.clone(),
                received_at: now,
                task_key: None,
                task_id: None,
                task_origin: None,
                reaction_added: false,
                reactions: Vec::new(),
                report_reaction: None,
                mark_requested: false,
                reply_ts: None,
                history: Vec::new(),
                work: None,
            }
        };

        if request.checked_at.is_some_and(|at| at <= 0 || at > now) {
            return Err(invalid("확인을 시작한 시각은 현재보다 뒤일 수 없다"));
        }
        let posted_at = (source_ts / 1000) as i64;
        if request.searched_at.is_some_and(|at| at < posted_at || at > now
            || request.checked_at.is_some_and(|started| at < started)) {
            return Err(invalid("검색 시각은 제보 이후부터 현재까지의 범위여야 한다"));
        }
        if let Some(reactions) = request.reactions.take() {
            let mut names = HashSet::new();
            if reactions.len() > 30 || reactions.iter().any(|reaction| {
                !slack_reaction_name(&reaction.name) || reaction.count == 0 || !names.insert(&reaction.name)
            }) {
                return Err(invalid("Slack 반응 이름과 개수를 확인해 주세요"));
            }
            entry.reactions = reactions;
        }
        if let Some(mut reaction) = request.report_reaction.take() {
            if !slack_reaction_name(&reaction.name)
                || reaction.at.is_some_and(|at| at < posted_at || at > now)
            {
                return Err(invalid("제보 반응의 이름과 시각을 확인해 주세요"));
            }
            reaction.by = reaction.by.map(|by| short(&by, 100)).filter(|by| !by.is_empty());
            if reaction.at.is_some() || reaction.by.is_some() {
                entry.report_reaction = Some(reaction);
            }
        }
        if request.reaction_added.is_some() {
            return Err(invalid("Slack 표시 여부는 접수한 항목에 따로 기록한다"));
        }
        if let Some(reply_ts) = request.reply_ts {
            if entry.reply_ts.as_ref().is_none_or(|last| &reply_ts > last) {
                entry.reply_ts = Some(reply_ts);
            }
        }
        entry.mark_requested = channel.mark_processed
            && matches!(
                request.result,
                ResultKind::Created | ResultKind::Commented | ResultKind::Ignored
            );
        let mut event = Processing {
            revision: revision(),
            result: request.result,
            at: now,
            checked_at: request.checked_at,
            searched_at: request.searched_at,
            via: request.via,
            decision: String::new(),
            question: String::new(),
            question_limit: false,
        };
        let outcome = Outcome {
            result: request.result,
            decision: request.decision,
            question: request.question,
            task: request.task,
            task_key: request.task_key,
        };
        resolve(vault, context, manager, &mut entry, &outcome, &mut event)?;
        entry.history.push(event);
        let mut response = json!({"recorded": false, "item": entry.view()?});
        if entry.current()?.question_limit {
            response["questionAccepted"] = json!(false);
        }
        self.entries.insert(key, entry);
        Ok((response, true))
    }
}

fn prepare_direct_work(
    vault: &Path,
    context: &CallContext,
    manager: &ManagerContext,
    outcome: &Outcome,
) -> Result<Option<DirectWork>, EngineError> {
    let task_id = match outcome.result {
        ResultKind::Created => {
            if outcome.task_key.is_some() {
                return Err(invalid("새 태스크에는 태스크 내용을 지정한다"));
            }
            let draft = outcome
                .task
                .as_ref()
                .ok_or_else(|| invalid("새 태스크 내용을 지정한다"))?;
            task_commands::prepare_task(vault, draft, context)?;
            None
        }
        ResultKind::Commented => {
            if outcome.task.is_some() {
                return Err(invalid("재제보에는 기존 태스크 키를 지정한다"));
            }
            let reference = outcome
                .task_key
                .as_deref()
                .ok_or_else(|| invalid("재제보를 남길 태스크 키를 지정한다"))?;
            let scan = store::scan(vault);
            let task = find_intake_task(vault, manager, &scan, reference)?;
            Some(task.id.clone())
        }
        _ => return Ok(None),
    };
    Ok(Some(DirectWork {
        result: outcome.result,
        task_id,
    }))
}

fn resolve(
    vault: &Path,
    context: &CallContext,
    manager: &ManagerContext,
    entry: &mut Entry,
    outcome: &Outcome,
    event: &mut Processing,
) -> Result<(), EngineError> {
    event.decision = short(&outcome.decision, 500);
    event.question = short(&outcome.question, 500);
    match outcome.result {
        ResultKind::Pending => return Err(invalid("다시 처리 대기는 앱에서 지정한다")),
        ResultKind::Asked if event.question.is_empty() => {
            return Err(invalid("되물음에는 질문을 적는다"));
        }
        ResultKind::Created | ResultKind::Commented => {
            let (key, task_id) = apply_task(vault, context, manager, entry, outcome)?;
            entry.task_key = Some(key);
            entry.task_id = Some(task_id);
            entry.task_origin = Some(outcome.result);
        }
        _ if outcome.task_key.is_some() || outcome.task.is_some() => {
            return Err(invalid("태스크는 새 태스크 또는 재제보 댓글에만 지정한다"));
        }
        _ => {}
    }
    if outcome.result == ResultKind::Asked
        && entry.history.iter().filter(|event| event.result == ResultKind::Asked).count() >= QUESTION_LIMIT
    {
        event.result = ResultKind::Pending;
        event.question_limit = true;
        event.question.clear();
        event.decision = "질문을 두 번 남겨 사람이 검토하도록 넘겼어요.".into();
    }
    Ok(())
}

fn apply_task(
    vault: &Path,
    context: &CallContext,
    manager: &ManagerContext,
    entry: &Entry,
    request: &Outcome,
) -> Result<(String, String), EngineError> {
    let scan = store::scan(vault);
    if let Some(id) = &entry.task_id {
        let task = find_intake_task(vault, manager, &scan, id)?;
        if entry.task_origin != Some(request.result)
            || request
                .task_key
                .as_ref()
                .is_some_and(|key| key != &task.key && key != &task.id)
        {
            return Err(invalid("이 제보에는 이미 처리한 태스크가 연결되어 있다"));
        }
        return Ok((task.key.clone(), task.id.clone()));
    }
    if request.result == ResultKind::Created {
        // 원장 저장 전에 작업이 끊겨도 이미 만든 태스크를 찾는다.
        let receipt = entry.source.as_ref().map(|_| entry.id.as_str());
        let matches_source = |task: &Task| {
            if let Some(receipt) = receipt {
                return task.intake_receipt() == Some(receipt);
            }
            task.refs.iter().any(|item| {
                item.kind == RefKind::Slack
                    && source_matches(&item.target, &entry.channel, &entry.message_ts)
            })
        };
        if let Some(reference) = &request.task_key {
            if request.task.is_some() {
                return Err(invalid(
                    "새 태스크 내용과 이미 만든 태스크 키 중 하나만 지정한다",
                ));
            }
            let task = find_intake_task(vault, manager, &scan, reference)?;
            if !matches_source(task) {
                return Err(invalid(if receipt.is_some() {
                    "태스크에 같은 접수 번호가 연결되어 있지 않다"
                } else {
                    "태스크의 Slack 출처가 접수하려는 원글과 다르다"
                }));
            }
            return Ok((task.key.clone(), task.id.clone()));
        }
        if let Some(task) = scan
            .items
            .iter()
            .find(|task| task.project.as_deref() == Some(&manager.project) && matches_source(task))
        {
            task_commands::check_manager_repo(vault, manager, task.repo.as_deref())?;
            return Ok((task.key.clone(), task.id.clone()));
        }
        let mut draft = request
            .task
            .clone()
            .ok_or_else(|| invalid("새 태스크 내용을 지정한다"))?;
        if let Some(receipt) = receipt {
            let task = task_commands::create_intake_task(vault, &draft, context, Some(receipt))?;
            return Ok((task.key, task.id));
        }
        let mut refs = draft
            .refs
            .take()
            .as_ref()
            .map(format::refs)
            .transpose()?
            .unwrap_or_default();
        refs.retain(|item| item.kind != RefKind::Slack);
        let mut values: Vec<_> = refs
            .into_iter()
            .map(|item| json!({"kind": item.kind.as_str(), "target": item.target, "title": item.title}))
            .collect();
        values.insert(
            0,
            json!({"kind": "slack", "target": entry.url, "title": "Slack 원글",
                   "reporter": entry.reporter, "excerpt": entry.excerpt}),
        );
        draft.refs = Some(json!(values));
        let task = task_commands::create_intake_task(vault, &draft, context, None)?;
        return Ok((task.key, task.id));
    }
    if request.task.is_some() {
        return Err(invalid("재제보에는 기존 태스크 키를 지정한다"));
    }
    let reference = request
        .task_key
        .as_deref()
        .ok_or_else(|| invalid("재제보를 남길 태스크 키를 지정한다"))?;
    let task = find_intake_task(vault, manager, &scan, reference)?;
    if entry
        .work
        .as_ref()
        .and_then(|work| work.task_id.as_ref())
        .is_some_and(|id| id != &task.id)
    {
        return Err(invalid("이미 시작한 재제보 댓글의 태스크 키를 보내 주세요"));
    }
    let comment = if entry.source.is_some() {
        format!("재제보를 접수했어요. 접수 번호는 {}예요.", entry.id)
    } else {
        format!("Slack 재제보를 접수했어요. [원글 열기]({})", entry.url)
    };
    let already_commented = store::activity(vault, &task.key, 0)?
        .iter()
        .any(|event| event["kind"] == "comment" && event["body"] == comment);
    if !already_commented {
        task_commands::add_task_comment(
            vault,
            &AddTaskCommentRequest {
                id: task.id.clone(),
                body: Some(comment),
                actor: None,
            },
            context,
        )?;
    }
    Ok((task.key.clone(), task.id.clone()))
}

fn find_intake_task<'a>(
    vault: &Path,
    manager: &ManagerContext,
    scan: &'a ais_domain::task::aggregate::Scan,
    reference: &str,
) -> Result<&'a Task, EngineError> {
    let task = store::find(scan, reference)?;
    if task.project.as_deref() != Some(&manager.project) {
        return Err(store::task_not_found(reference));
    }
    task_commands::check_manager_repo(vault, manager, task.repo.as_deref())?;
    Ok(task)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_time(day: i64, hour: i64, minute: i64) -> (i64, LocalClock) {
        let wall = (days_from_civil(2026, 11, day) * i64::from(DAY_MINUTES) + hour * 60 + minute) * MINUTE;
        let approximate = wall - clock().local_clock(wall).offset_seconds * 1000;
        let at = wall - clock().local_clock(approximate).offset_seconds * 1000;
        (at, clock().local_clock(at))
    }

    #[test]
    fn intake_schedule_waits_for_work_hours_and_clears_after_a_desktop_check() {
        let mut channel = Channel {
            id: "CTEST123".into(), label: "가상 채널".into(), emoji: "ticket".into(),
            visibility: ChannelVisibility::Unknown,
            mark_processed: true, paused: false, interval_minutes: 10,
            enabled_at: local_time(23, 9, 0).0,
            cursor: "1795500000.000000".into(), revision: "original".into(),
            last_check_at: None, last_check_via: None, last_scheduled_at: None, schedule: None,
        };
        let (monday, monday_local) = local_time(30, 9, 31);
        assert_eq!(channel.stalled_minutes(monday, monday_local), None);
        assert_eq!(channel.next_check_at(monday, monday_local), None);

        channel.schedule = Some(Schedule { days: vec![1, 2, 3, 4, 5], start_minute: 540, end_minute: 1080 });
        assert_eq!(channel.stalled_minutes(monday, monday_local), Some(31));
        channel.last_check_at = Some(local_time(27, 17, 0).0);
        channel.last_scheduled_at = channel.last_check_at;
        let (weekend, weekend_local) = local_time(28, 13, 0);
        assert_eq!(channel.stalled_minutes(weekend, weekend_local), None);
        assert_eq!(channel.next_check_at(weekend, weekend_local), Some(local_time(30, 9, 0).0));
        let (opening, opening_local) = local_time(30, 9, 15);
        assert_eq!(channel.stalled_minutes(opening, opening_local), None);
        assert_eq!(channel.stalled_minutes(monday, monday_local), Some(31));
        channel.last_check_at = Some(monday - MINUTE);
        channel.last_check_via = Some(ProcessVia::Desktop);
        assert_eq!(channel.stalled_minutes(monday, monday_local), None);
        let (after_hours, after_hours_local) = local_time(30, 18, 1);
        assert_eq!(channel.stalled_minutes(after_hours, after_hours_local), None);
    }
}
