use super::*;

const RESOLVE_USAGE: &str = "resolve-resource-edit <자원 id 또는 refs:<프로젝트 slug>> action=approve|revert. 승인 안 된 직접 수정을 받아들이거나(approve) 승인한 판으로 되돌린다(revert. 새 파일이면 휴지통으로). 사람만 한다";
const POLICY_SET_USAGE: &str = "set-resource-policy [project=auto|approve] [global=auto|approve] [refs=auto|approve] [permission=auto|approve]. 사람만 한다";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ResolveResourceEditRequest {
    pub id: String,
    pub action: Option<Json>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SetResourcePolicyRequest {
    pub project: Option<Json>,
    pub global: Option<Json>,
    pub refs: Option<Json>,
    pub permission: Option<Json>,
}

pub fn resolve_resource_edit(
    vault: &Path,
    request: &ResolveResourceEditRequest,
    context: &CallContext,
) -> Result<Json, EngineError> {
    require_human(is_ai(context), "승인 안 된 직접 수정의 처리")?;
    let key = only_id(&request.id, RESOLVE_USAGE)?;
    let action = text_of(&request.action);
    let Some(action) = action
        .as_deref()
        .map(java_trim)
        .filter(|a| *a == "approve" || *a == "revert")
    else {
        return Err(EngineError::invalid_argument(format!(
            "action 은 approve 나 revert 여야 한다: {RESOLVE_USAGE}"
        )));
    };
    let approve = action == "approve";
    let out = json!({"key": key, "action": action});
    if let Some(project) = key.strip_prefix("refs:") {
        let state = refs::state(vault, project)?;
        if state == TrustState::Approved {
            return Err(EngineError::conflict(format!(
                "승인 안 된 변경이 없다: {key}"
            )));
        }
        if approve {
            approvals::record(vault, key, &refs::live_bytes(vault, project), "adopted")?;
        } else if state == TrustState::Dirty {
            let copy = approvals::copy(vault, key).unwrap_or_default();
            store::restore_refs_bytes(vault, project, &copy)?;
        } else {
            refs::write(vault, project, &[])?;
        }
        return Ok(out);
    }
    let live = store::find(vault, key)?;
    let state = store::approval_state(vault, &live)?;
    if state == TrustState::Approved {
        return Err(EngineError::conflict(format!(
            "승인 안 된 변경이 없다: {key}"
        )));
    }
    if approve {
        approvals::record(
            vault,
            &approvals::res_key(&live.id),
            &store::live_bytes(vault, &live)?,
            "adopted",
        )?;
    } else if state == TrustState::Dirty {
        store::revert_to_approved(vault, &live)?;
    } else {
        store::delete(vault, &live)?;
        approvals::forget(vault, &approvals::res_key(&live.id))?;
    }
    Ok(out)
}

fn policy_response(vault: &Path) -> Result<Json, EngineError> {
    Ok(json!({
        "policy": approvals::policy(vault)?.to_json(),
        "defaults": Policy::defaults().to_json(),
        "categories": aggregate::categories_json(),
    }))
}

pub fn get_resource_policy(vault: &Path) -> Result<Json, EngineError> {
    begin_read(vault)?;
    policy_response(vault)
}

pub fn set_resource_policy(
    vault: &Path,
    request: &SetResourcePolicyRequest,
    context: &CallContext,
) -> Result<Json, EngineError> {
    require_human(is_ai(context), "승인 정책 변경")?;
    let opts = collect(&[
        ("project", &request.project),
        ("global", &request.global),
        ("refs", &request.refs),
        ("permission", &request.permission),
    ]);
    let mut policy = approvals::policy(vault)?;
    let mut any = false;
    for key in POLICY_KEYS {
        let Some(value) = opts.get(key) else {
            continue;
        };
        policy = policy.with(key, trim(value))?;
        any = true;
    }
    if !any {
        return Err(EngineError::invalid_argument(format!(
            "바꿀 칸이 필요하다: {POLICY_SET_USAGE}"
        )));
    }
    approvals::set_policy(vault, &policy)?;
    policy_response(vault)
}
