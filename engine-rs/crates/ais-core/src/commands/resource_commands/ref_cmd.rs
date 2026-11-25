use super::*;

const REF_USAGE: &str = "set-resource-ref <프로젝트 slug> <글로벌 자원 id> [remove=true] [with-skills=true. 에이전트를 참조할 때 그 스킬도 함께]";
const USABLE_USAGE: &str = "usable-resources <프로젝트 slug> [bodies=true]";
const VERSIONS_USAGE: &str = "list-resource-versions <자원 id>";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SetResourceRefRequest {
    pub project: Option<Json>,
    pub id: Option<Json>,
    pub remove: Option<Json>,
    #[serde(rename = "with-skills", alias = "withSkills")]
    pub with_skills: Option<Json>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct UsableResourcesRequest {
    pub project: String,
    pub bodies: Option<Json>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ResourceVersionsRequest {
    pub id: String,
}

pub fn set_resource_ref(
    vault: &Path,
    request: &SetResourceRefRequest,
    context: &CallContext,
) -> Result<Json, EngineError> {
    let (Some(project), Some(id)) = (text_of(&request.project), text_of(&request.id)) else {
        return Err(EngineError::invalid_argument(format!(
            "set-resource-ref 는 프로젝트 slug 와 글로벌 자원 id 를 받는다: {REF_USAGE}"
        )));
    };
    let opts = collect(&[
        ("remove", &request.remove),
        ("with-skills", &request.with_skills),
    ]);
    ref_impl(&run(vault, context), &project, &id, &opts)
}

pub(super) fn ref_impl(run: &Run, project: &str, id: &str, opts: &Opts) -> Result<Json, EngineError> {
    let vault = run.vault;
    let remove = is_true(opts.get("remove"));
    let with_skills = is_true(opts.get("with-skills"));
    store::require_project(vault, project)?;
    if !ResourceKind::is_id(id) {
        return Err(EngineError::invalid_argument(format!(
            "자원 id 모양이 아니다(sk… 또는 ap…): {id}"
        )));
    }

    // 바탕은 승인한 참조 목록과 승인한 자원이다(직접 고쳐진 refs.md의 내용은 이어받지 않는다)
    let served = store::scan_served(vault)?;
    let entries = refs::read_approved(vault, project)?;
    let RefsChange {
        next,
        added,
        removed,
    } = change_refs(entries, id, remove, with_skills, |rid| served.by_id(rid))?;

    let cats = aggregate::classify_refs(&added, &removed);
    if !cats.is_empty() {
        let info = RefInfo {
            project: project.to_string(),
            added: added.clone(),
            removed: removed.clone(),
        };
        if let Some(queued) = queue_if_needed(
            run,
            "set-resource-ref",
            opts,
            &[project.to_string(), id.to_string()],
            None,
            None,
            None,
            &cats,
            Some(&info),
        )? {
            return Ok(queued);
        }
    }
    refs::write(vault, project, &next)?;
    approvals::record(
        vault,
        &approvals::refs_key(project),
        &refs::live_bytes(vault, project),
        run.by,
    )?;

    let rows: Vec<Json> = next
        .iter()
        .map(|e| {
            let found = served.by_id(&e.id).filter(|r| r.is_global());
            json!({
                "id": e.id,
                "status": if found.is_some() { "ok" } else { "missing" },
                "kind": ResourceKind::of_id(&e.id).map_or("", ResourceKind::label),
                "name": found.map_or(e.hint.clone(), |r| r.name.clone()),
            })
        })
        .collect();
    Ok(json!({"project": project, "added": added, "removed": removed, "refs": rows}))
}

pub fn usable_resources(
    vault: &Path,
    request: &UsableResourcesRequest,
) -> Result<Json, EngineError> {
    begin_read(vault)?;
    let project = only_id(&request.project, USABLE_USAGE)?;
    let bodies = is_true(text_of(&request.bodies).as_ref());
    let u = resolver::resolve(vault, project)?;
    let item_rows = |items: &[resolver::Item]| -> Vec<Json> {
        items
            .iter()
            .map(|it| {
                let mut m = row(&it.resource, bodies, false);
                m.insert("source".into(), json!(it.source));
                Json::Object(m)
            })
            .collect()
    };
    let broken: Vec<Json> = u
        .broken
        .iter()
        .map(|b| json!({"id": b.id, "hint": b.hint}))
        .collect();
    let missing: Vec<Json> = u
        .missing_skills
        .iter()
        .map(|mi| {
            let mut m = Map::new();
            m.insert(
                "agent".into(),
                json!({"id": mi.agent.id, "name": mi.agent.name}),
            );
            m.insert("skillId".into(), json!(mi.skill_id));
            if let Some(skill) = &mi.skill {
                m.insert(
                    "skill".into(),
                    json!({"name": skill.name, "scope": skill.scope()}),
                );
            }
            m.insert("canReference".into(), json!(mi.can_reference()));
            Json::Object(m)
        })
        .collect();
    Ok(json!({
        "project": u.project,
        "skills": item_rows(&u.skills),
        "agents": item_rows(&u.agents),
        "broken": broken,
        "missingSkills": missing,
    }))
}

pub fn list_resource_versions(
    vault: &Path,
    request: &ResourceVersionsRequest,
) -> Result<Json, EngineError> {
    begin_read(vault)?;
    let id = only_id(&request.id, VERSIONS_USAGE)?;
    let cur = store::find(vault, id)?;
    let rows: Vec<Json> = store::versions(vault, &cur)
        .into_iter()
        .map(|v| {
            let mut m = Map::new();
            m.insert("version".into(), json!(v.version));
            m.insert("savedAt".into(), json!(v.saved_at));
            m.insert("bytes".into(), json!(v.bytes));
            m.insert("editedAt".into(), json!(v.edited_at));
            if let Some(session) = v.edited_session {
                m.insert("editedSession".into(), json!(session));
            }
            Json::Object(m)
        })
        .collect();
    Ok(json!({ "id": cur.id, "versions": rows }))
}
