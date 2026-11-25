use super::*;

const GET_USAGE: &str = "get-resource <자원 id> [version=<list-resource-versions 의 판>]";
const DELETE_USAGE: &str = "delete-resource <자원 id>";
const RESTORE_USAGE: &str =
    "restore-resource <휴지통의 자원 id. list-resources trash=true 로 조회한다>";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ListResourcesRequest {
    pub scope: Option<Json>,
    pub project: Option<Json>,
    pub kind: Option<Json>,
    pub bodies: Option<Json>,
    pub trash: Option<Json>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GetResourceRequest {
    pub id: String,
    pub version: Option<Json>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SaveResourceRequest {
    pub id: Option<Json>,
    pub kind: Option<Json>,
    pub scope: Option<Json>,
    pub project: Option<Json>,
    pub name: Option<Json>,
    pub description: Option<Json>,
    pub body: Option<Json>,
    pub permission: Option<Json>,
    pub skills: Option<Json>,
    pub repo: Option<Json>,
    pub tools: Option<Json>,
    pub f1: Option<Json>,
}

/// 자원 id 하나만 받는 쓰기 명령(`delete_resource`, `restore_resource`)
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ResourceIdRequest {
    pub id: String,
}

pub fn list_resources(vault: &Path, request: &ListResourcesRequest) -> Result<Json, EngineError> {
    begin_read(vault)?;
    let opts = collect(&[
        ("scope", &request.scope),
        ("project", &request.project),
        ("kind", &request.kind),
        ("bodies", &request.bodies),
        ("trash", &request.trash),
    ]);
    let scope = scope_of(&opts, false)?;
    let kind = match opts.get("kind") {
        Some(raw) => Some(kind_of(Some(raw))?),
        None => None,
    };
    if let Some(project) = &scope.project {
        store::require_project(vault, project)?;
    }

    if is_true(opts.get("trash")) {
        let rows: Vec<Json> = store::trash(vault)
            .into_iter()
            .filter(|t| kind.is_none_or(|k| k == t.kind))
            .filter(|t| !scope.given || scope.project == t.project)
            .map(|t| {
                let mut m = Map::new();
                m.insert("id".into(), json!(t.id));
                m.insert("kind".into(), json!(t.kind.label()));
                m.insert(
                    "scope".into(),
                    json!(if t.project.is_none() {
                        "global"
                    } else {
                        "project"
                    }),
                );
                if let Some(project) = &t.project {
                    m.insert("project".into(), json!(project));
                }
                m.insert("name".into(), json!(t.name));
                m.insert("path".into(), json!(t.rel_path));
                m.insert("deletedAt".into(), json!(t.deleted_at));
                Json::Object(m)
            })
            .collect();
        return Ok(json!({ "trash": rows }));
    }

    let bodies = is_true(opts.get("bodies"));
    let served = store::scan_served(vault)?;
    let mut items: Vec<&Resource> = served
        .items
        .iter()
        .filter(|r| kind.is_none_or(|k| k == r.kind))
        .filter(|r| !scope.given || scope.project == r.project)
        .collect();
    items.sort_by(|a, b| {
        compare_utf16(
            a.project.as_deref().unwrap_or(""),
            b.project.as_deref().unwrap_or(""),
        )
        .then_with(|| kind_ordinal(a.kind).cmp(&kind_ordinal(b.kind)))
        .then_with(|| compare_utf16(&a.name.to_lowercase(), &b.name.to_lowercase()))
    });
    let rows: Vec<Json> = items
        .into_iter()
        .map(|r| {
            let mut m = row(r, bodies, false);
            if served.state_of(&r.id) == Some(TrustState::Dirty) {
                m.insert("approval".into(), json!("dirty"));
            }
            Json::Object(m)
        })
        .collect();
    let mut out = Map::new();
    out.insert("resources".into(), Json::Array(rows));
    if !served.skipped.is_empty() {
        out.insert("skipped".into(), json!(served.skipped));
    }
    out.insert("approvals".into(), approval_counts(vault)?);
    Ok(Json::Object(out))
}

pub fn get_resource(vault: &Path, request: &GetResourceRequest) -> Result<Json, EngineError> {
    begin_read(vault)?;
    let id = only_id(&request.id, GET_USAGE)?;
    let live = store::find(vault, id)?;
    // 읽는 것은 승인한 판이다. 승인 기록이 없는 새 파일은 보여 주지 않는다
    let served = store::scan_served(vault)?;
    let Some(cur) = served.by_id(id) else {
        return Err(EngineError::not_found(format!(
            "승인되지 않은 자원이라 조회되지 않는다(list-resource-changes 로 승인 상태를 본다): {id}"
        )));
    };
    let version = text_of(&request.version).filter(|v| !is_blank(v));
    let mut m = match &version {
        Some(version) => {
            let old = store::read_version(vault, cur, java_trim(version))?;
            let mut m = row(&Resource { token: None, ..old }, true, false);
            m.insert("version".into(), json!(java_trim(version)));
            m
        }
        None => row(cur, true, false),
    };
    if store::approval_state(vault, &live)? == TrustState::Dirty {
        m.insert("approval".into(), json!("dirty"));
    }
    // 고칠 때 넘길 토큰은 늘 지금 파일의 것이다(옛 판을 읽어도, 그 내용으로 되돌려 쓰려면 지금 파일의 토큰이 필요하다)
    m.insert("precondition".into(), json!(live.token));
    if cur.is_global() {
        m.insert(
            "referencedBy".into(),
            json!(refs::projects_referencing(vault, &cur.id)),
        );
    }
    Ok(Json::Object(m))
}

pub fn save_resource(
    vault: &Path,
    request: &SaveResourceRequest,
    context: &CallContext,
) -> Result<Json, EngineError> {
    let opts = collect(&[
        ("body", &request.body),
        ("description", &request.description),
        ("f1", &request.f1),
        ("id", &request.id),
        ("kind", &request.kind),
        ("name", &request.name),
        ("permission", &request.permission),
        ("project", &request.project),
        ("repo", &request.repo),
        ("scope", &request.scope),
        ("skills", &request.skills),
        ("tools", &request.tools),
    ]);
    save(&run(vault, context), &opts)
}

pub(super) fn save(run: &Run, opts: &Opts) -> Result<Json, EngineError> {
    let vault = run.vault;
    let id = opts.get("id").filter(|id| !is_blank(id));
    let Some(id) = id else {
        let kind = kind_of(opts.get("kind").map(String::as_str))?;
        let scope = scope_of(opts, true)?;
        reject_agent_only_for_skill(kind, |k| opts.contains_key(k))?;
        let fields = fields_of(|k| opts.get(k).map(String::as_str))?;
        let plan = store::plan_create(
            vault,
            kind,
            scope.project.as_deref(),
            &fields,
            &run.stamp,
            run.forced_id.as_deref(),
        )?;
        let cats = aggregate::classify(None, Some(&plan.after));
        if let Some(queued) = queue_if_needed(
            run,
            "save-resource",
            opts,
            &[],
            Some(&plan.after.id),
            None,
            Some(&plan.after),
            &cats,
            None,
        )? {
            return Ok(queued);
        }
        let r = store::commit_create(vault, &plan)?;
        approvals::record(
            vault,
            &approvals::res_key(&r.id),
            &store::live_bytes(vault, &r)?,
            run.by,
        )?;
        let mut m = row(&r, true, true);
        m.insert("created".into(), json!(true));
        return Ok(Json::Object(m));
    };
    let live = store::find(vault, java_trim(id))?;
    // 오류 차례는 종류 → 범위다. 범위 인자를 해석하기 전에 종류를 먼저 본다(둘 다 틀리면 종류 오류가 나온다).
    check_same_kind(&live, opts.get("kind").map(String::as_str))?;
    let scope = scope_of(opts, false)?;
    check_same_scope(&live, &scope)?;
    reject_agent_only_for_skill(live.kind, |k| opts.contains_key(k))?;
    // 고칠 바탕은 승인한 판이다. 직접 고쳐진 파일의 내용은 이어받지 않는다(받아들이려면 resolve-resource-edit)
    let base = store::base_for_edit(vault, &live)?;
    let plan = store::plan_update(
        vault,
        &base,
        &fields_of(|k| opts.get(k).map(String::as_str))?,
        opts.get("f1").map(String::as_str),
        &run.stamp,
    )?;
    if !plan.changed {
        let mut m = row(&base, true, true);
        m.insert("created".into(), json!(false));
        m.insert("changed".into(), json!(false));
        return Ok(Json::Object(m));
    }
    let cats = aggregate::classify(Some(&base), Some(&plan.after));
    if let Some(queued) = queue_if_needed(
        run,
        "save-resource",
        opts,
        &[],
        None,
        Some(&base),
        Some(&plan.after),
        &cats,
        None,
    )? {
        return Ok(queued);
    }
    let next = store::commit_update(vault, &plan)?;
    approvals::record(
        vault,
        &approvals::res_key(&next.id),
        &store::live_bytes(vault, &next)?,
        run.by,
    )?;
    let mut m = row(&next, true, true);
    m.insert("created".into(), json!(false));
    m.insert("changed".into(), json!(true));
    Ok(Json::Object(m))
}

pub fn delete_resource(
    vault: &Path,
    request: &ResourceIdRequest,
    context: &CallContext,
) -> Result<Json, EngineError> {
    let id = only_id(&request.id, DELETE_USAGE)?;
    delete(&run(vault, context), id)
}

pub(super) fn delete(run: &Run, id: &str) -> Result<Json, EngineError> {
    let vault = run.vault;
    let cur = store::find(vault, id)?;
    let cats = aggregate::classify(Some(&cur), None);
    if let Some(queued) = queue_if_needed(
        run,
        "delete-resource",
        &Opts::new(),
        &[id.to_string()],
        None,
        Some(&cur),
        None,
        &cats,
        None,
    )? {
        return Ok(queued);
    }
    let referenced_by = if cur.is_global() {
        refs::projects_referencing(vault, &cur.id)
    } else {
        Vec::new()
    };
    store::delete(vault, &cur)?;
    approvals::forget(vault, &approvals::res_key(&cur.id))?;
    let mut m = Map::new();
    m.insert("id".into(), json!(cur.id));
    m.insert("kind".into(), json!(cur.kind.label()));
    m.insert("scope".into(), json!(cur.scope()));
    if let Some(project) = &cur.project {
        m.insert("project".into(), json!(project));
    }
    m.insert("name".into(), json!(cur.name));
    m.insert("trashed".into(), json!(true));
    m.insert("referencedBy".into(), json!(referenced_by));
    Ok(Json::Object(m))
}

pub fn restore_resource(
    vault: &Path,
    request: &ResourceIdRequest,
    context: &CallContext,
) -> Result<Json, EngineError> {
    let id = only_id(&request.id, RESTORE_USAGE)?;
    restore(&run(vault, context), id)
}

pub(super) fn restore(run: &Run, id: &str) -> Result<Json, EngineError> {
    let vault = run.vault;
    let back = store::peek_trash(vault, id)?;
    let cats = aggregate::classify(None, Some(&back));
    if let Some(queued) = queue_if_needed(
        run,
        "restore-resource",
        &Opts::new(),
        &[id.to_string()],
        None,
        None,
        Some(&back),
        &cats,
        None,
    )? {
        return Ok(queued);
    }
    let r = store::restore(vault, id)?;
    approvals::record(
        vault,
        &approvals::res_key(&r.id),
        &store::live_bytes(vault, &r)?,
        run.by,
    )?;
    let mut m = row(&r, true, true);
    m.insert("restored".into(), json!(true));
    Ok(Json::Object(m))
}
