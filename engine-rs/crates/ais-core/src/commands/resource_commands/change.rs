use super::*;

const CHANGE_GET_USAGE: &str = "get-resource-change <변경 id. list-resource-changes 로 조회한다>";
const APPROVE_USAGE: &str = "approve-resource-change <변경 id>. 사람만 한다";
const REJECT_USAGE: &str = "reject-resource-change <변경 id> [reason=<거절 이유>]. 사람만 한다";
const DEFAULT_DECIDED: i32 = 20;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ListResourceChangesRequest {
    pub decided: Option<Json>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ResourceChangeRequest {
    pub id: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ChangeDecisionRequest {
    pub id: String,
    pub reason: Option<Json>,
}

fn strings(items: &[String]) -> JsonValue {
    JsonValue::Array(items.iter().map(string).collect())
}

/// 변경이 승인을 기다려야 하면 대기에 올리고 응답을 돌려준다. 바로 적용해도 되면 `None`이다.
/// 같은 변경이 이미 대기 중이면 새로 올리지 않고 그것을 알려 준다(AI가 같은 요청을 되풀이해도 쌓이지 않게).
#[allow(clippy::too_many_arguments)]
pub(super) fn queue_if_needed(
    run: &Run,
    command: &str,
    args: &Opts,
    positional: &[String],
    planned_id: Option<&str>,
    before: Option<&Resource>,
    after: Option<&Resource>,
    cats: &[Category],
    ref_info: Option<&RefInfo>,
) -> Result<Option<Json>, EngineError> {
    if run.apply {
        return Ok(None);
    }
    let vault = run.vault;
    let decision = aggregate::decide(&approvals::policy(vault)?, cats, run.ai);
    if !decision.queue {
        return Ok(None);
    }

    for m in changes::pending(vault) {
        if changes::get_str(&m, "command") == Some(command)
            && same_args(m.get("args"), args)
            && same_positional(m.get("positional"), positional)
        {
            return Ok(Some(queued_response(&m, true)));
        }
    }
    let any = after.or(before);
    let mut proposer: Vec<(String, JsonValue)> = Vec::new();
    if let Some(session) = &run.stamp.session {
        proposer.push(("session".into(), string(session)));
    }
    if let Some(agent) = &run.stamp.agent {
        proposer.push(("agent".into(), string(agent)));
    }
    proposer.push(("channel".into(), string(&run.channel)));
    let fixed = decision.reasons.iter().any(|c| c.fixed());
    let mut target: Vec<(String, JsonValue)> = Vec::new();
    if let Some(any) = any {
        target.push(("kind".into(), string(any.kind.label())));
        target.push(("scope".into(), string(any.scope())));
        if let Some(project) = &any.project {
            target.push(("project".into(), string(project)));
        }
        target.push(("id".into(), string(&any.id)));
        target.push(("name".into(), string(&any.name)));
    } else if let Some(info) = ref_info {
        target.push(("scope".into(), string("project")));
        target.push(("project".into(), string(&info.project)));
    }
    let mut c: Vec<(String, JsonValue)> = vec![
        ("id".into(), string(changes::new_id())),
        ("status".into(), string(changes::PENDING)),
        ("command".into(), string(command)),
        ("createdAt".into(), string(&run.stamp.at)),
        ("proposer".into(), JsonValue::Object(proposer)),
        (
            "categories".into(),
            JsonValue::Array(cats.iter().map(|x| string(x.code())).collect()),
        ),
        (
            "reasons".into(),
            JsonValue::Array(decision.reasons.iter().map(|x| string(x.code())).collect()),
        ),
        ("fixed".into(), literal(fixed.to_string())),
        ("target".into(), JsonValue::Object(target)),
        (
            "args".into(),
            JsonValue::Object(args.iter().map(|(k, v)| (k.clone(), string(v))).collect()),
        ),
        (
            "positional".into(),
            JsonValue::Array(positional.iter().map(string).collect()),
        ),
    ];
    if let Some(planned) = planned_id {
        c.push(("forcedId".into(), string(planned)));
    }
    if let Some(before) = before {
        c.push(("before".into(), changes::view(before)));
    }
    if let Some(after) = after {
        c.push(("after".into(), changes::view(after)));
    }
    if let Some(info) = ref_info {
        c.push((
            "ref".into(),
            JsonValue::Object(vec![
                ("project".into(), string(&info.project)),
                ("added".into(), strings(&info.added)),
                ("removed".into(), strings(&info.removed)),
            ]),
        ));
    }
    c.push((
        "summary".into(),
        string(summarize_change(command, before, after, ref_info)),
    ));
    let change = JsonValue::Object(c);
    changes::write(vault, &change)?;
    Ok(Some(queued_response(&change, false)))
}

/// 저장된 `args`가 지금 옵션과 같은가(`Map.equals`: 값은 글일 때만 같다)
fn same_args(stored: Option<&JsonValue>, args: &Opts) -> bool {
    let Some(JsonValue::Object(entries)) = stored else {
        return false;
    };
    entries.len() == args.len()
        && entries
            .iter()
            .all(|(k, v)| matches!(v, JsonValue::Str(s) if args.get(k) == Some(s)))
}

fn same_positional(stored: Option<&JsonValue>, positional: &[String]) -> bool {
    let Some(JsonValue::Array(items)) = stored else {
        return false;
    };
    items.len() == positional.len()
        && items
            .iter()
            .zip(positional)
            .all(|(a, b)| matches!(a, JsonValue::Str(s) if s == b))
}

fn queued_response(c: &changes::Change, duplicate: bool) -> Json {
    let mut m = Map::new();
    m.insert("queued".into(), json!(true));
    m.insert(
        "changeId".into(),
        c.get("id").map_or(Json::Null, json_to_serde),
    );
    m.insert("status".into(), json!(changes::PENDING));
    if duplicate {
        m.insert("duplicate".into(), json!(true));
    }
    m.insert(
        "summary".into(),
        c.get("summary").map_or(Json::Null, json_to_serde),
    );
    m.insert("reasons".into(), Json::Array(reason_rows(c)));
    m.insert(
        "message".into(),
        json!("변경을 적용하지 않고 승인 대기에 올렸다. 사람이 앱의 스킬·에이전트 화면에서 승인해야 적용되고, 그때까지 목록과 조회에는 이전 판이 그대로 보인다. 처리 결과는 get-resource-change 로 본다."),
    );
    Json::Object(m)
}

fn category_row(cat: Category) -> Json {
    json!({"category": cat.code(), "label": cat.label(), "fixed": cat.fixed()})
}

/// 칸에 적힌 갈래 코드들을 응답 행으로(모르는 코드는 건너뛴다)
fn code_rows(c: &changes::Change, key: &str) -> Vec<Json> {
    match c.get(key) {
        Some(JsonValue::Array(codes)) => codes
            .iter()
            .filter_map(|code| Category::of_code(&java_text(code)))
            .map(category_row)
            .collect(),
        _ => Vec::new(),
    }
}

fn reason_rows(c: &changes::Change) -> Vec<Json> {
    code_rows(c, "reasons")
}

/// `String.valueOf`: 글은 그대로, 따옴표 없는 값은 읽은 모양(정수 `5`, 실수 `5.0`)으로 쓴다.
fn java_text(value: &JsonValue) -> String {
    match value {
        JsonValue::Str(s) => s.clone(),
        JsonValue::Literal(raw) => match raw.as_str() {
            "true" | "false" | "null" => raw.clone(),
            text => text.parse::<i64>().map_or_else(
                |_| {
                    text.parse::<f64>().map_or_else(
                        |_| text.to_string(),
                        crate::kernel::java::java_double_string,
                    )
                },
                |n| n.to_string(),
            ),
        },
        other => json::compact(other),
    }
}

pub(super) fn approval_counts(vault: &Path) -> Result<Json, EngineError> {
    Ok(json!({
        "pending": changes::pending(vault).len(),
        "unapproved": unapproved_rows(vault)?.len(),
    }))
}

pub fn list_resource_changes(
    vault: &Path,
    request: &ListResourceChangesRequest,
) -> Result<Json, EngineError> {
    begin_read(vault)?;
    let mut decided_limit = DEFAULT_DECIDED;
    if let Some(raw) = text_of(&request.decided).filter(|r| !is_blank(r)) {
        decided_limit = java_trim(&raw).parse::<i32>().unwrap_or(-1);
        if decided_limit < 0 {
            return Err(EngineError::invalid_argument(format!(
                "decided 는 0 이상의 정수여야 한다: {raw}"
            )));
        }
    }
    let mut pending: Vec<Json> = Vec::new();
    let mut decided: Vec<Json> = Vec::new();
    for c in changes::list(vault) {
        if changes::get_str(&c, "status") == Some(changes::PENDING) {
            pending.push(change_row(&c, false));
        } else if (decided.len() as i32) < decided_limit {
            decided.push(change_row(&c, false));
        }
    }
    let unapproved = unapproved_rows(vault)?;
    Ok(json!({
        "counts": {"pending": pending.len(), "unapproved": unapproved.len()},
        "pending": pending,
        "unapproved": unapproved,
        "decided": decided,
        "policy": approvals::policy(vault)?.to_json(),
    }))
}

pub fn get_resource_change(
    vault: &Path,
    request: &ResourceChangeRequest,
) -> Result<Json, EngineError> {
    begin_read(vault)?;
    let id = only_id(&request.id, CHANGE_GET_USAGE)?;
    Ok(change_row(&changes::read(vault, id)?, true))
}

/// `Objects.equals`로 본 같음: 맵은 차례를 안 따지고, 정수와 실수는 다른 값이다.
fn same_value(a: Option<&JsonValue>, b: Option<&JsonValue>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(JsonValue::Literal(x)), None) | (None, Some(JsonValue::Literal(x))) => x == "null",
        (Some(x), Some(y)) => same_json(x, y),
        _ => false,
    }
}

fn same_json(a: &JsonValue, b: &JsonValue) -> bool {
    match (a, b) {
        (JsonValue::Str(x), JsonValue::Str(y)) => x == y,
        (JsonValue::Array(x), JsonValue::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same_json(p, q))
        }
        (JsonValue::Object(x), JsonValue::Object(y)) => {
            x.len() == y.len()
                && x.iter().all(|(k, v)| {
                    y.iter()
                        .find(|(k2, _)| k2 == k)
                        .is_some_and(|(_, v2)| same_json(v, v2))
                })
        }
        (JsonValue::Literal(x), JsonValue::Literal(y)) => {
            match (x.parse::<i64>(), y.parse::<i64>()) {
                (Ok(p), Ok(q)) => p == q,
                (Ok(_), Err(_)) | (Err(_), Ok(_)) => false,
                (Err(_), Err(_)) => match (x.parse::<f64>(), y.parse::<f64>()) {
                    (Ok(p), Ok(q)) => p.to_bits() == q.to_bits() || (p.is_nan() && q.is_nan()),
                    _ => x == y,
                },
            }
        }
        _ => false,
    }
}

/// 변경 한 줄. 목록에서는 비교용 칸(before, after, args)을 빼고, 하나를 볼 때만 싣는다.
fn change_row(c: &changes::Change, detail: bool) -> Json {
    let mut m = Map::new();
    for key in [
        "id",
        "status",
        "command",
        "createdAt",
        "summary",
        "target",
        "proposer",
        "fixed",
    ] {
        if let Some(v) = c.get(key) {
            m.insert(key.into(), json_to_serde(v));
        }
    }
    m.insert("reasons".into(), Json::Array(reason_rows(c)));
    m.insert("categories".into(), Json::Array(code_rows(c, "categories")));
    for key in ["decidedAt", "decidedBy", "reason"] {
        if let Some(v) = c.get(key) {
            m.insert(key.into(), json_to_serde(v));
        }
    }
    if detail {
        for key in ["before", "after", "ref", "args", "positional", "forcedId"] {
            if let Some(v) = c.get(key) {
                m.insert(key.into(), json_to_serde(v));
            }
        }
        if let (Some(b @ JsonValue::Object(_)), Some(a @ JsonValue::Object(_))) =
            (c.get("before"), c.get("after"))
        {
            let changed: Vec<&str> = [
                "name",
                "description",
                "permission",
                "skills",
                "repo",
                "tools",
                "body",
            ]
            .into_iter()
            .filter(|key| !same_value(b.get(key), a.get(key)))
            .collect();
            m.insert("changedFields".into(), json!(changed));
        }
    }
    Json::Object(m)
}

/// 승인 안 된 직접 수정: 승인한 뒤 파일이 바뀌었거나(dirty), 승인 기록 없이 생긴(new) 자원 파일과 참조 목록.
pub(super) fn unapproved_rows(vault: &Path) -> Result<Vec<Json>, EngineError> {
    let mut rows: Vec<Json> = Vec::new();
    let served = store::scan_served(vault)?;
    let raw = store::scan(vault);
    for approved in &served.items {
        if served.state_of(&approved.id) != Some(TrustState::Dirty) {
            continue;
        }
        let mut m = unapproved_row(approved, "dirty");
        m.insert("approved".into(), view_json(approved));
        if let Some(live) = raw.by_id(&approved.id) {
            m.insert("live".into(), view_json(live));
        }
        rows.push(Json::Object(m));
    }
    for r in &served.untrusted {
        let mut m = unapproved_row(r, "new");
        m.insert("live".into(), view_json(r));
        rows.push(Json::Object(m));
    }
    let mut projects: BTreeSet<Utf16Key> = refs::projects_with_refs_file(vault)
        .into_iter()
        .map(Utf16Key)
        .collect();
    for (key, _) in &approvals::ledger(vault)?.items {
        if let Some(project) = key.strip_prefix("refs:") {
            projects.insert(Utf16Key(project.to_string()));
        }
    }
    for Utf16Key(project) in projects {
        let state = refs::state(vault, &project)?;
        if state == TrustState::Approved {
            continue;
        }
        let ids = |entries: Vec<refs::Entry>| -> Vec<String> {
            entries.into_iter().map(|e| e.id).collect()
        };
        rows.push(json!({
            "key": approvals::refs_key(&project),
            "type": "refs",
            "state": state.label(),
            "project": project,
            "approved": ids(refs::read_approved(vault, &project)?),
            "live": ids(refs::read(vault, &project)),
        }));
    }
    Ok(rows)
}

fn unapproved_row(r: &Resource, state: &str) -> Map<String, Json> {
    let mut m = Map::new();
    m.insert("key".into(), json!(r.id));
    m.insert("type".into(), json!("resource"));
    m.insert("state".into(), json!(state));
    m.insert("id".into(), json!(r.id));
    m.insert("kind".into(), json!(r.kind.label()));
    m.insert("scope".into(), json!(r.scope()));
    if let Some(project) = &r.project {
        m.insert("project".into(), json!(project));
    }
    m.insert("name".into(), json!(r.name));
    m.insert("path".into(), json!(r.rel_path));
    m
}

pub fn approve_resource_change(
    vault: &Path,
    request: &ChangeDecisionRequest,
    context: &CallContext,
) -> Result<Json, EngineError> {
    require_human(is_ai(context), "승인")?;
    let id = only_id(&request.id, APPROVE_USAGE)?;
    let mut change = pending_change(vault, id)?;
    let run = apply_run(vault, &change);
    let result = match apply_change(&run, &change) {
        Ok(result) => result,
        Err(e) => {
            if aggregate::goes_stale(&e) {
                decide(
                    vault,
                    &mut change,
                    changes::STALE,
                    Some(&format!("적용하지 못했다: {}", e.message)),
                )?;
            }
            return Err(e);
        }
    };
    decide(vault, &mut change, changes::APPROVED, None)?;
    Ok(json!({
        "approved": true,
        "changeId": change.get("id").map_or(Json::Null, json_to_serde),
        "summary": change.get("summary").map_or(Json::Null, json_to_serde),
        "result": result,
    }))
}

pub fn reject_resource_change(
    vault: &Path,
    request: &ChangeDecisionRequest,
    context: &CallContext,
) -> Result<Json, EngineError> {
    require_human(is_ai(context), "거절")?;
    let id = only_id(&request.id, REJECT_USAGE)?;
    let mut change = pending_change(vault, id)?;
    let reason = text_of(&request.reason)
        .filter(|r| !is_blank(r))
        .map(|r| trim(&r).to_string());
    decide(vault, &mut change, changes::REJECTED, reason.as_deref())?;
    Ok(json!({
        "rejected": true,
        "changeId": change.get("id").map_or(Json::Null, json_to_serde),
        "summary": change.get("summary").map_or(Json::Null, json_to_serde),
    }))
}

fn pending_change(vault: &Path, id: &str) -> Result<changes::Change, EngineError> {
    let change = changes::read(vault, id)?;
    if changes::get_str(&change, "status") != Some(changes::PENDING) {
        let current = change
            .get("status")
            .map_or_else(|| "null".to_string(), java_text);
        return Err(aggregate::already_decided(id, &current));
    }
    Ok(change)
}

fn decide(
    vault: &Path,
    change: &mut changes::Change,
    status: &str,
    reason: Option<&str>,
) -> Result<(), EngineError> {
    changes::put(change, "status", string(status));
    changes::put(
        change,
        "decidedAt",
        string(iso_offset_date_time(now_millis())),
    );
    changes::put(change, "decidedBy", string("human"));
    if let Some(reason) = reason {
        changes::put(change, "reason", string(reason));
    }
    changes::write(vault, change)?;
    changes::prune(vault);
    Ok(())
}

/// 승인한 변경을 적용할 실행. 올린 사람의 기록으로 쓰고, 대기에 다시 올리지 않는다.
fn apply_run<'a>(vault: &'a Path, change: &changes::Change) -> Run<'a> {
    let proposer = change.get("proposer");
    let field = |key: &str| {
        proposer
            .and_then(|p| p.get(key))
            .filter(|v| !v.is_null())
            .map(java_text)
    };
    Run {
        vault,
        ai: false,
        stamp: Stamp {
            at: change
                .get("createdAt")
                .map_or_else(|| "null".to_string(), java_text),
            session: field("session"),
            agent: field("agent"),
            manager: None,
        },
        apply: true,
        by: "approved",
        forced_id: change
            .get("forcedId")
            .filter(|v| !v.is_null())
            .map(java_text),
        channel: field("channel").unwrap_or_else(|| "mcp".to_string()),
    }
}

fn apply_change(run: &Run, change: &changes::Change) -> Result<Json, EngineError> {
    let mut args = Opts::new();
    if let Some(JsonValue::Object(entries)) = change.get("args") {
        for (key, value) in entries {
            args.insert(key.clone(), java_text(value));
        }
    }
    let mut positional: Vec<String> = Vec::new();
    if let Some(JsonValue::Array(items)) = change.get("positional") {
        positional.extend(items.iter().map(java_text));
    }
    let command = change
        .get("command")
        .map_or_else(|| "null".to_string(), java_text);
    let missing = || EngineError::internal(format!("대기 변경의 자리 인자가 모자라다: {command}"));
    match command.as_str() {
        "save-resource" => save(run, &args),
        "delete-resource" => delete(run, positional.first().ok_or_else(missing)?),
        "restore-resource" => restore(run, positional.first().ok_or_else(missing)?),
        "set-resource-ref" => {
            let (project, id) = (
                positional.first().ok_or_else(missing)?,
                positional.get(1).ok_or_else(missing)?,
            );
            ref_impl(run, project, id, &args)
        }
        _ => Err(EngineError::internal(format!(
            "알 수 없는 대기 변경 명령이다: {command}"
        ))),
    }
}
