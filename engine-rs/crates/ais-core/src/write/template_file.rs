//! 템플릿 파일(`.aisurada/templates/doc/<id>.md`, `.aisurada/templates/ritual/<rev>.md`) 만들기.

use crate::error::EngineError;
use crate::frontmatter::{Value, dump_general_map};

pub fn doc_rel_path(id: &str, format: &str) -> String {
    let extension = if format == "markdown" { "md" } else { format };
    format!(".aisurada/templates/doc/{id}.{extension}")
}

pub fn ritual_rel_path(rev: &str) -> String {
    format!(".aisurada/templates/ritual/{rev}.md")
}

fn text(key: &str, value: &str) -> (Value, Value) {
    (Value::Str(key.into()), Value::Str(value.into()))
}

fn render(pairs: Vec<(Value, Value)>, body: &str) -> Result<Vec<u8>, EngineError> {
    let head = dump_general_map(&pairs).map_err(|e| EngineError::invalid_argument(e.0))?;
    Ok(format!("---\n{head}---\n{body}").into_bytes())
}

/// 문서 템플릿 파일 내용: 머리(`id`, `name`, `type`, `format`, `desc`, `builtin`)와 본문
pub fn render_doc(
    id: &str,
    name: &str,
    doc_type: &str,
    format: &str,
    desc: &str,
    builtin: bool,
    body: &str,
) -> Result<Vec<u8>, EngineError> {
    render(
        vec![
            text("id", id),
            text("name", name),
            text("type", doc_type),
            text("format", format),
            text("desc", desc),
            (Value::Str("builtin".into()), Value::Bool(builtin)),
        ],
        body,
    )
}

/// 회고(리추얼) 템플릿 파일 내용: 머리(`id`, `rev`, `name`, `icon`, `format`, `desc`)와 본문
pub fn render_ritual(
    id: &str,
    rev: &str,
    name: &str,
    icon: &str,
    format: &str,
    desc: &str,
    body: &str,
) -> Result<Vec<u8>, EngineError> {
    render(
        vec![
            text("id", id),
            text("rev", rev),
            text("name", name),
            text("icon", icon),
            text("format", format),
            text("desc", desc),
        ],
        body,
    )
}
