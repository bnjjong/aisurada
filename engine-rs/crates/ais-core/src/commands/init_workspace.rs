//! `init_workspace`: vault 폴더를 워크스페이스로 갖춘다. 뼈대 폴더를 만들고, 받은 빌트인 템플릿을 없는 것만 심고,
//! 표식(`.aisurada/workspace.json`)이 없으면 만든다. 이미 있는 것은 건드리지 않아서 여러 번 불러도 된다.
//! 인자 이름, 응답 필드, 검사 순서와 오류 문구는 Java 엔진과 같다.

use crate::kernel::doc_type::DocType;
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::error::EngineError;
use crate::json::{self, JsonValue, append_string};
use crate::kernel::time::format_utc_millis;
use crate::text::{compare_utf16, is_blank};
use crate::vault::agenda::is_valid_slug_segment;
use crate::write::apparatus::now_millis;
use crate::write::atomic::write_atomic;
use crate::write::pipeline::WriteContext;
use crate::write::template_file::{doc_rel_path, render_doc, render_ritual, ritual_rel_path};

const MAX_TEMPLATES: usize = 64;
const SKELETON: [&str; 4] = [
    ".aisurada/agendas",
    ".aisurada/templates/doc",
    ".aisurada/templates/ritual",
    "notes",
];
const DOC_FIELDS: [&str; 7] = ["id", "name", "format", "desc", "body", "type", "builtin"];
const RITUAL_FIELDS: [&str; 7] = ["id", "name", "format", "desc", "body", "rev", "icon"];
const FORMATS: [&str; 3] = ["markdown", "html", "json"];
const REVS: [&str; 3] = ["morning", "evening", "weekly"];

static ID_SHAPE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9._-]{1,128}$").unwrap());
static ICON_SHAPE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z][A-Za-z0-9]{0,63}$").unwrap());

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct InitWorkspaceRequest {
    /// 워크스페이스 이름. 없으면 폴더 이름을 쓴다. 있는데 공백뿐이면 오류다.
    pub name: Option<String>,
    /// 심을 빌트인 템플릿 목록(JSON 배열 글)
    pub templates: Option<String>,
    #[serde(flatten)]
    pub context: WriteContext,
}

#[derive(Debug, Serialize)]
pub struct InitWorkspaceResponse {
    /// 표식을 새로 만들었는가
    pub created: bool,
    /// 새로 심은 템플릿 파일(vault 상대 경로)
    pub seeded: Vec<String>,
    /// 이미 있어서 건드리지 않은 템플릿 파일
    pub skipped: Vec<String>,
}

struct Seed {
    doc: bool,
    id: String,
    name: String,
    format: String,
    desc: String,
    body: String,
    doc_type: String,
    builtin: bool,
    rev: String,
    icon: String,
}

fn bad(message: String) -> EngineError {
    EngineError::invalid_argument(message)
}

pub fn execute(
    vault: &Path,
    request: &InitWorkspaceRequest,
) -> Result<InitWorkspaceResponse, EngineError> {
    let seeds = read_templates(request.templates.as_deref())?;
    if request.name.as_deref().is_some_and(is_blank) {
        return Err(bad(
            "워크스페이스 이름이 비었다. 이름을 주거나 아예 빼서 폴더 이름을 쓰게 한다."
                .to_string(),
        ));
    }
    // 뼈대 폴더는 값 검사보다 먼저 만든다(검사에서 실패해도 폴더는 남는다)
    for dir in SKELETON {
        let path = vault.join(dir);
        fs::create_dir_all(&path).map_err(|_| {
            EngineError::internal(format!("폴더를 만들지 못했다: {}", path.display()))
        })?;
    }
    let (docs, rituals): (Vec<&Seed>, Vec<&Seed>) = seeds.iter().partition(|s| s.doc);
    for seed in docs.iter().chain(rituals.iter()) {
        check_values(seed)?;
    }
    let (mut seeded, mut skipped) = (Vec::new(), Vec::new());
    for seed in &docs {
        if !is_valid_slug_segment(&seed.id) {
            return Err(bad(format!(
                "템플릿 id 가 안전한 경로 세그먼트가 아니다: {}",
                seed.id
            )));
        }
        let content = render_doc(
            &seed.id,
            &seed.name,
            &seed.doc_type,
            &seed.format,
            &seed.desc,
            seed.builtin,
            &seed.body,
        )?;
        plant(
            vault,
            doc_rel_path(&seed.id, &seed.format),
            &content,
            &mut seeded,
            &mut skipped,
        )?;
    }
    for seed in &rituals {
        let content = render_ritual(
            &seed.id,
            &seed.rev,
            &seed.name,
            &seed.icon,
            &seed.format,
            &seed.desc,
            &seed.body,
        )?;
        plant(
            vault,
            ritual_rel_path(&seed.rev),
            &content,
            &mut seeded,
            &mut skipped,
        )?;
    }
    let marker = vault.join(".aisurada/workspace.json");
    let created = !marker.exists();
    if created {
        let name = request.name.clone().unwrap_or_else(|| {
            vault
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        write_atomic(&marker, marker_bytes(&name).as_bytes())?;
    }
    Ok(InitWorkspaceResponse {
        created,
        seeded,
        skipped,
    })
}

fn plant(
    vault: &Path,
    rel_path: String,
    content: &[u8],
    seeded: &mut Vec<String>,
    skipped: &mut Vec<String>,
) -> Result<(), EngineError> {
    if vault.join(&rel_path).exists() {
        skipped.push(rel_path);
        return Ok(());
    }
    write_atomic(&vault.join(&rel_path), content)?;
    seeded.push(rel_path);
    Ok(())
}

/// 표식 파일 내용: 들여쓰기 2칸의 JSON과 마지막 줄바꿈
fn marker_bytes(name: &str) -> String {
    let mut name_json = String::new();
    append_string(&mut name_json, name);
    format!(
        "{{\n  \"schemaVersion\": 1,\n  \"name\": {name_json},\n  \"createdAt\": \"{}\"\n}}\n",
        format_utc_millis(now_millis())
    )
}

fn read_templates(raw: Option<&str>) -> Result<Vec<Seed>, EngineError> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    let Ok(JsonValue::Array(items)) = json::parse_file(raw) else {
        return Err(bad(format!("templates 은 JSON 배열이어야 한다: {raw}")));
    };
    if items.len() > MAX_TEMPLATES {
        return Err(bad(format!(
            "templates 항목이 너무 많다(최대 {MAX_TEMPLATES}건): {}건",
            items.len()
        )));
    }
    items.iter().map(read_seed).collect()
}

fn compact_or_null(value: Option<&JsonValue>) -> String {
    value.map_or_else(|| "null".to_string(), json::compact)
}

fn read_seed(element: &JsonValue) -> Result<Seed, EngineError> {
    let JsonValue::Object(props) = element else {
        return Err(bad(format!(
            "templates 항목이 객체가 아니다: {}",
            json::compact(element)
        )));
    };
    let category_value = element.get("category");
    let Some(JsonValue::Str(category)) = category_value else {
        return Err(bad(format!(
            "templates 항목의 category 가 문자열이 아니다: {}",
            compact_or_null(category_value)
        )));
    };
    let doc = match category.as_str() {
        "doc" => true,
        "ritual" => false,
        other => {
            return Err(bad(format!(
                "templates 항목의 category 를 모른다: {other}. 쓸 수 있는 것은 [doc, ritual] 뿐이다."
            )));
        }
    };
    let fields = if doc { DOC_FIELDS } else { RITUAL_FIELDS };
    let mut allowed: Vec<&str> = fields.to_vec();
    allowed.push("category");
    allowed.sort_by(|a, b| compare_utf16(a, b));
    let mut unknown: Vec<&str> = props
        .iter()
        .map(|(k, _)| k.as_str())
        .filter(|k| !allowed.contains(k))
        .collect();
    if !unknown.is_empty() {
        unknown.sort_by(|a, b| compare_utf16(a, b));
        return Err(bad(format!(
            "templates 항목에 모르는 키가 있다: [{}]. 쓸 수 있는 것은 [{}] 뿐이다.",
            unknown.join(", "),
            allowed.join(", ")
        )));
    }
    let mut seed = Seed {
        doc,
        id: String::new(),
        name: String::new(),
        format: String::new(),
        desc: String::new(),
        body: String::new(),
        doc_type: String::new(),
        builtin: false,
        rev: String::new(),
        icon: String::new(),
    };
    for field in fields {
        let value = element.get(field);
        if field == "builtin" {
            match value {
                Some(JsonValue::Literal(raw)) if raw == "true" || raw == "false" => {
                    seed.builtin = raw == "true"
                }
                other => {
                    return Err(bad(format!(
                        "templates 항목의 builtin 가 boolean 이 아니다: {}",
                        compact_or_null(other)
                    )));
                }
            }
            continue;
        }
        let Some(JsonValue::Str(text)) = value else {
            return Err(bad(format!(
                "templates 항목의 {field} 가 문자열이 아니다: {}",
                compact_or_null(value)
            )));
        };
        let slot = match field {
            "id" => &mut seed.id,
            "name" => &mut seed.name,
            "format" => &mut seed.format,
            "desc" => &mut seed.desc,
            "body" => &mut seed.body,
            "type" => &mut seed.doc_type,
            "rev" => &mut seed.rev,
            _ => &mut seed.icon,
        };
        *slot = text.clone();
    }
    Ok(seed)
}

fn check_values(seed: &Seed) -> Result<(), EngineError> {
    if is_blank(&seed.id) || !ID_SHAPE.is_match(&seed.id) {
        return Err(bad(format!("템플릿 id 모양이 아니다: {}", seed.id)));
    }
    if is_blank(&seed.name) {
        return Err(bad("템플릿 이름이 비었다".to_string()));
    }
    if !FORMATS.contains(&seed.format.as_str()) {
        return Err(bad(format!(
            "모르는 템플릿 포맷이다: {}. 쓸 수 있는 것은 [markdown, html, json] 뿐이다.",
            seed.format
        )));
    }
    if seed.doc {
        if DocType::parse(&seed.doc_type).is_none() {
            return Err(bad(format!(
                "모르는 노트 유형이다: {}. 쓸 수 있는 것은 [{}] 뿐이다.",
                seed.doc_type,
                DocType::NAMES_TEXT
            )));
        }
        return Ok(());
    }
    if !REVS.contains(&seed.rev.as_str()) {
        return Err(bad(format!(
            "모르는 리추얼 종류다: {}. 쓸 수 있는 것은 [morning, evening, weekly] 뿐이다.",
            seed.rev
        )));
    }
    if !ICON_SHAPE.is_match(&seed.icon) {
        return Err(bad(format!("아이콘 이름 모양이 아니다: {}", seed.icon)));
    }
    Ok(())
}
