//! 템플릿 쓰기: `save_template`(문서 템플릿 만들기·고치기), `save_ritual_template`(회고 템플릿 고치기),
//! `delete_doc_template`(문서 템플릿 지우기). 인자 이름과 응답 필드는 Java 엔진과 같다.

use crate::kernel::doc_type::DocType;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::error::EngineError;
use crate::text::{is_blank, trim};
use crate::vault::template::{Category, TemplateRecord, scan_templates};
use crate::write::apparatus::next_id;
use crate::write::atomic::write_atomic;
use crate::write::pipeline::WriteContext;
use crate::write::template_file::{doc_rel_path, render_doc, render_ritual, ritual_rel_path};
use crate::write::warnings::detect_indented_blocks_warning;

const SAVE_USAGE: &str = "save-template [id=<고칠 템플릿 id. 없으면 새로 만든다>] [name=<이름. 새로 만들 때 필수>] [type=<plan|build|report|research|meeting|free. 새로 만들 때 기본 plan>] [desc=<언제 쓰는 템플릿인지>] [body=<본문. 새로 만들 때 필수>]";
const DELETE_USAGE: &str = "delete-doc-template <문서 템플릿 id>";
const RITUAL_USAGE: &str = "save-ritual-template <리추얼 템플릿 id> [name=<이름>] [desc=<언제 쓰는 양식인지>] [body=<본문>]";

static ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9._-]{1,128}$").unwrap());

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SaveTemplateRequest {
    /// 고칠 템플릿 id. 없거나 비면 새로 만든다.
    pub id: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub doc_type: Option<String>,
    pub desc: Option<String>,
    pub body: Option<String>,
    #[serde(flatten)]
    pub context: WriteContext,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SaveRitualTemplateRequest {
    pub id: String,
    pub name: Option<String>,
    pub desc: Option<String>,
    pub body: Option<String>,
    #[serde(flatten)]
    pub context: WriteContext,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DeleteDocTemplateRequest {
    pub id: String,
    #[serde(flatten)]
    pub context: WriteContext,
}

#[derive(Debug, Serialize)]
pub struct SaveTemplateResponse {
    pub id: String,
    /// `created` 또는 `updated`
    pub action: &'static str,
    pub builtin: bool,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DeleteDocTemplateResponse {
    pub id: String,
    pub removed: Vec<String>,
}

fn validate_id(id: &str) -> Result<(), EngineError> {
    if is_blank(id) || !ID.is_match(id) {
        return Err(EngineError::invalid_argument(format!(
            "템플릿 id 모양이 아니다: {id}"
        )));
    }
    Ok(())
}

fn positional_id(id: &str, usage: &str) -> Result<(), EngineError> {
    if is_blank(id) {
        return Err(EngineError::invalid_argument(format!(
            "인자가 하나 필요하다: {usage}"
        )));
    }
    validate_id(id)
}

fn validate_name(name: &Option<String>) -> Result<(), EngineError> {
    if name.as_deref().is_some_and(is_blank) {
        return Err(EngineError::invalid_argument("템플릿 이름이 비었다"));
    }
    Ok(())
}

/// 유형 인자: 없으면 `None`, 소문자로 바꿔 목록에 있어야 한다
fn type_arg(raw: &Option<String>) -> Result<Option<String>, EngineError> {
    let Some(raw) = raw else { return Ok(None) };
    let lowered = raw.to_lowercase();
    if DocType::parse(&lowered).is_none() {
        return Err(EngineError::invalid_argument(format!(
            "모르는 템플릿 유형 이다: {raw}. 쓸 수 있는 것은 [{}] 뿐이다.",
            DocType::NAMES_TEXT
        )));
    }
    Ok(Some(lowered))
}

fn find(vault: &Path, id: &str) -> Option<TemplateRecord> {
    scan_templates(vault)
        .templates
        .into_iter()
        .find(|t| t.id == id)
}

fn not_found(id: &str) -> EngineError {
    EngineError::not_found(format!("Template not found: {id}"))
}

fn saved(
    id: &str,
    action: &'static str,
    builtin: bool,
    path: String,
    received_body: Option<&str>,
) -> SaveTemplateResponse {
    SaveTemplateResponse {
        id: id.to_string(),
        action,
        builtin,
        path,
        warning: detect_indented_blocks_warning(received_body),
    }
}

fn remove(vault: &Path, relative: &str) -> Result<bool, EngineError> {
    let file = vault.join(relative);
    match fs::remove_file(&file) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(EngineError::internal(format!(
            "파일을 지우지 못했다: {}",
            crate::write::confine::absolute_normalized(&file).display()
        ))),
    }
}

pub fn save_document(
    vault: &Path,
    request: &SaveTemplateRequest,
) -> Result<SaveTemplateResponse, EngineError> {
    let id = request.id.clone().filter(|i| !i.is_empty());
    let Some(id) = id else {
        // 새로 만든다
        if request.body.as_deref().is_none_or(is_blank) {
            return Err(EngineError::invalid_argument(format!(
                "새 템플릿은 body 이 필요하다: {SAVE_USAGE}"
            )));
        }
        let name = trim(request.name.as_deref().unwrap_or(""));
        if name.is_empty() {
            return Err(EngineError::invalid_argument(format!(
                "새 템플릿은 name 이 필요하다: {SAVE_USAGE}"
            )));
        }
        let doc_type = type_arg(&request.doc_type)?.unwrap_or_else(|| "plan".to_string());
        let id = next_id("tpl-");
        let desc = request.desc.clone().unwrap_or_default();
        let body = request.body.clone().unwrap_or_default();
        let path = doc_rel_path(&id, "markdown");
        write_atomic(
            &vault.join(&path),
            &render_doc(&id, name, &doc_type, "markdown", &desc, false, &body)?,
        )?;
        return Ok(saved(&id, "created", false, path, request.body.as_deref()));
    };
    validate_id(&id)?;
    validate_name(&request.name)?;
    let doc_type = type_arg(&request.doc_type)?;
    let existing = find(vault, &id)
        .filter(|t| t.category == Category::Doc)
        .ok_or_else(|| not_found(&id))?;
    let name = request
        .name
        .clone()
        .unwrap_or_else(|| existing.name.clone());
    let doc_type = doc_type
        .or_else(|| existing.doc_type.clone())
        .unwrap_or_default();
    let desc = request
        .desc
        .clone()
        .unwrap_or_else(|| existing.desc.clone());
    let body = request
        .body
        .clone()
        .unwrap_or_else(|| existing.body.clone());
    let builtin = existing.builtin.unwrap_or(false);
    let path = doc_rel_path(&id, "markdown");
    write_atomic(
        &vault.join(&path),
        &render_doc(&id, &name, &doc_type, "markdown", &desc, builtin, &body)?,
    )?;
    if existing.format != "markdown" {
        remove(
            vault,
            &format!(".aisurada/templates/doc/{id}.{}", existing.format),
        )?;
    }
    Ok(saved(
        &id,
        "updated",
        builtin,
        path,
        request.body.as_deref(),
    ))
}

pub fn save_ritual(
    vault: &Path,
    request: &SaveRitualTemplateRequest,
) -> Result<SaveTemplateResponse, EngineError> {
    positional_id(&request.id, RITUAL_USAGE)?;
    validate_name(&request.name)?;
    let templates = scan_templates(vault).templates;
    let existing = templates
        .iter()
        .find(|t| t.id == request.id)
        .filter(|t| t.category == Category::Ritual)
        .ok_or_else(|| not_found(&request.id))?;
    let rev = existing.rev.clone().unwrap_or_default();
    let same_rev = templates
        .iter()
        .filter(|t| t.category == Category::Ritual && t.rev.as_deref().unwrap_or("") == rev)
        .count();
    if same_rev != 1 {
        return Err(EngineError::invalid_argument(format!(
            "같은 rev 를 든 리추얼 템플릿이 {same_rev}건이라 저장하지 않는다. 파일 이름이 rev 라서 서로를 덮는다: rev={rev}"
        )));
    }
    let path = ritual_rel_path(&rev);
    let content = render_ritual(
        &request.id,
        &rev,
        request.name.as_deref().unwrap_or(&existing.name),
        existing.icon.as_deref().unwrap_or(""),
        existing.format,
        request.desc.as_deref().unwrap_or(&existing.desc),
        request.body.as_deref().unwrap_or(&existing.body),
    )?;
    write_atomic(&vault.join(&path), &content)?;
    Ok(saved(
        &request.id,
        "updated",
        false,
        path,
        request.body.as_deref(),
    ))
}

pub fn delete_document(
    vault: &Path,
    request: &DeleteDocTemplateRequest,
) -> Result<DeleteDocTemplateResponse, EngineError> {
    positional_id(&request.id, DELETE_USAGE)?;
    let mut removed = Vec::new();
    if let Some(existing) = find(vault, &request.id) {
        if existing.category == Category::Ritual {
            return Err(EngineError::invalid_argument(format!(
                "리추얼 템플릿은 지울 수 없다. 지우는 길이 앱에도 없다: {}",
                request.id
            )));
        }
        if existing.builtin == Some(true) {
            return Err(EngineError::invalid_argument(format!(
                "빌트인 템플릿은 지울 수 없다. 고친 것을 되돌리려면 기본값으로 복원한다: {}",
                request.id
            )));
        }
        for extension in ["md", "html", "json"] {
            let path = format!(".aisurada/templates/doc/{}.{extension}", request.id);
            if remove(vault, &path)? {
                removed.push(path);
            }
        }
    }
    Ok(DeleteDocTemplateResponse {
        id: request.id.clone(),
        removed,
    })
}
