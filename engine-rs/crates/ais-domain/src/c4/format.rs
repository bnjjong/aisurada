//! `model.yaml`과 `views.yaml`의 글을 읽고 쓰는 곳, 그리고 응답 JSON을 짓는 곳.
//!
//! 읽을 때는 규칙에 맞는지(`Model::validate`, `Views::validate`)까지 본다. 손으로 고친 파일이 깨졌으면 조용히 덮어쓰지 않고
//! 이유를 적은 오류로 멈춘다. 모르는 칸은 `extra`에 보존했다가 다시 쓴다.

use serde_json::{Value as Json, json};

use super::{Element, Kind, Model, Position, Relationship, Views, invalid};
use crate::error::EngineError;
use crate::frontmatter::{self, Value};

const VERSION: i64 = 1;

type Pairs = [(Value, Value)];

/// `model.yaml` 글을 읽는다. 빈 글은 빈 모델이다.
pub fn parse_model(text: &str) -> Result<Model, EngineError> {
    if text.trim().is_empty() {
        return Ok(Model::default());
    }
    let top = parse_top("model.yaml", text)?;
    let mut model = Model::default();
    for (key, value) in top {
        match key.as_str() {
            "version" => check_version("model.yaml", &value)?,
            "elements" => {
                for (index, item) in list_of("model.yaml의 elements", value)?
                    .into_iter()
                    .enumerate()
                {
                    model.elements.push(element_of(item, index)?);
                }
            }
            "relationships" => {
                for (index, item) in list_of("model.yaml의 relationships", value)?
                    .into_iter()
                    .enumerate()
                {
                    model.relationships.push(relationship_of(item, index)?);
                }
            }
            _ => model.extra.push((key, value)),
        }
    }
    model.validate()?;
    Ok(model)
}

/// `views.yaml` 글을 읽는다. 빈 글은 빈 뷰 목록이다.
pub fn parse_views(text: &str, model: &Model) -> Result<Views, EngineError> {
    if text.trim().is_empty() {
        return Ok(Views::default());
    }
    let top = parse_top("views.yaml", text)?;
    let mut views = Views::default();
    for (key, value) in top {
        match key.as_str() {
            "version" => check_version("views.yaml", &value)?,
            "views" => {
                let Value::Map(by_view) = value else {
                    return Err(invalid("views.yaml의 views는 맵이어야 한다"));
                };
                for (view, positions) in by_view {
                    let view = string_key("views.yaml의 뷰 이름", view)?;
                    let Value::Map(positions) = positions else {
                        return Err(invalid(format!("views.yaml의 {view}는 맵이어야 한다")));
                    };
                    let mut placed = std::collections::BTreeMap::new();
                    for (id, point) in positions {
                        let id = string_key(&format!("views.yaml의 {view} 안 요소 id"), id)?;
                        placed.insert(id.clone(), position_of(&view, &id, &point)?);
                    }
                    views.views.insert(view, placed);
                }
            }
            _ => views.extra.push((key, value)),
        }
    }
    // 모델과 뷰는 따로 쓰므로(그 사이 전원이 끊기거나 손으로 모델만 고쳤을 수 있다) 없어진 요소의 좌표와 범위를 잃은 뷰는
    // 오류로 막지 않고 무시한다. 좌표 값 자체가 틀린 것만 거절한다.
    views.prune(model);
    views.validate(model)?;
    Ok(views)
}

/// `model.yaml` 글을 만든다.
pub fn render_model(model: &Model) -> Result<String, EngineError> {
    let mut top: Vec<(Value, Value)> = vec![
        (text("version"), Value::Int(VERSION.to_string())),
        (
            text("elements"),
            Value::List(model.elements.iter().map(element_value).collect()),
        ),
        (
            text("relationships"),
            Value::List(model.relationships.iter().map(relationship_value).collect()),
        ),
    ];
    top.extend(model.extra.iter().map(|(k, v)| (text(k), v.clone())));
    dump(&top)
}

/// `views.yaml` 글을 만든다.
pub fn render_views(views: &Views) -> Result<String, EngineError> {
    let by_view = views
        .views
        .iter()
        .map(|(view, positions)| {
            let placed = positions
                .iter()
                .map(|(id, position)| {
                    (
                        text(id),
                        Value::Map(vec![
                            (text("x"), Value::Int(position.x.to_string())),
                            (text("y"), Value::Int(position.y.to_string())),
                        ]),
                    )
                })
                .collect();
            (text(view), Value::Map(placed))
        })
        .collect();
    let mut top: Vec<(Value, Value)> = vec![
        (text("version"), Value::Int(VERSION.to_string())),
        (text("views"), Value::Map(by_view)),
    ];
    top.extend(views.extra.iter().map(|(k, v)| (text(k), v.clone())));
    dump(&top)
}

/// 화면과 AI에 내보내는 모델 JSON. 요소에는 종류에서 계산한 `level`을 붙인다.
pub fn model_json(model: &Model) -> Json {
    let elements: Vec<Json> = model
        .elements
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "kind": e.kind.as_str(),
                "level": e.kind.level(),
                "name": e.name,
                "description": e.description,
                "technology": e.technology,
                "parentId": e.parent,
                "codeRef": e.code_ref,
            })
        })
        .collect();
    let relationships: Vec<Json> = model
        .relationships
        .iter()
        .map(|r| {
            json!({
                "id": r.id,
                "source": r.source,
                "target": r.target,
                "label": r.label,
                "technology": r.technology,
            })
        })
        .collect();
    json!({ "elements": elements, "relationships": relationships })
}

/// 뷰별 위치 JSON: `{ "context": { "<요소 id>": {"x":..,"y":..} } }`
pub fn views_json(views: &Views) -> Json {
    let mut out = serde_json::Map::new();
    for (view, positions) in &views.views {
        let mut placed = serde_json::Map::new();
        for (id, position) in positions {
            placed.insert(id.clone(), json!({ "x": position.x, "y": position.y }));
        }
        out.insert(view.clone(), Json::Object(placed));
    }
    Json::Object(out)
}

fn dump(top: &Pairs) -> Result<String, EngineError> {
    frontmatter::dump_general_map(top)
        .map_err(|_| EngineError::internal("C4 모델을 파일 글로 쓰지 못했다"))
}

fn text(value: &str) -> Value {
    Value::Str(value.to_string())
}

fn parse_top(file: &str, text: &str) -> Result<Vec<(String, Value)>, EngineError> {
    let pairs = frontmatter::parse(text)
        .ok_or_else(|| invalid(format!("{file}을 읽지 못했다. YAML 맵이어야 한다")))?;
    pairs
        .into_iter()
        .map(|(key, value)| Ok((string_key(&format!("{file}의 최상위 칸 이름"), key)?, value)))
        .collect()
}

fn string_key(what: &str, key: Value) -> Result<String, EngineError> {
    match key {
        Value::Str(key) => Ok(key),
        _ => Err(invalid(format!("{what}은(는) 글이어야 한다"))),
    }
}

fn check_version(file: &str, value: &Value) -> Result<(), EngineError> {
    match value {
        Value::Int(raw) if raw == &VERSION.to_string() => Ok(()),
        _ => Err(EngineError::unsupported(format!(
            "{file}의 version을 지원하지 않는다(지원: {VERSION})"
        ))),
    }
}

fn list_of(what: &str, value: Value) -> Result<Vec<Value>, EngineError> {
    match value {
        Value::List(items) => Ok(items),
        Value::Null => Ok(Vec::new()),
        _ => Err(invalid(format!("{what}는 목록이어야 한다"))),
    }
}

fn map_of(what: &str, value: Value) -> Result<Vec<(Value, Value)>, EngineError> {
    match value {
        Value::Map(pairs) => Ok(pairs),
        _ => Err(invalid(format!("{what}는 맵이어야 한다"))),
    }
}

/// 맵에서 글 칸을 꺼낸다. 없거나 `null`이면 `None`, 글이 아니면 오류다.
fn take_str(
    what: &str,
    fields: &mut Vec<(String, Value)>,
    key: &str,
) -> Result<Option<String>, EngineError> {
    let Some(at) = fields.iter().position(|(name, _)| name == key) else {
        return Ok(None);
    };
    match fields.remove(at).1 {
        Value::Null => Ok(None),
        Value::Str(value) => Ok(Some(value)),
        _ => Err(invalid(format!("{what}의 {key}는 글이어야 한다"))),
    }
}

fn fields_of(what: &str, value: Value) -> Result<Vec<(String, Value)>, EngineError> {
    map_of(what, value)?
        .into_iter()
        .map(|(key, value)| Ok((string_key(&format!("{what}의 칸 이름"), key)?, value)))
        .collect()
}

fn element_of(value: Value, index: usize) -> Result<Element, EngineError> {
    let what = format!("elements[{index}]");
    let mut fields = fields_of(&what, value)?;
    let id = take_str(&what, &mut fields, "id")?
        .ok_or_else(|| invalid(format!("{what}에 id가 없다")))?;
    let what = format!("요소 {id}");
    let kind_name = take_str(&what, &mut fields, "kind")?
        .ok_or_else(|| invalid(format!("{what}에 kind가 없다")))?;
    let kind = Kind::parse(&kind_name).ok_or_else(|| {
        invalid(format!(
            "{what}의 kind를 모른다: {kind_name} (person, system, external, container, component)"
        ))
    })?;
    let name = take_str(&what, &mut fields, "name")?
        .ok_or_else(|| invalid(format!("{what}에 name이 없다")))?;
    Ok(Element {
        kind,
        name,
        description: take_str(&what, &mut fields, "description")?.unwrap_or_default(),
        technology: take_str(&what, &mut fields, "technology")?.unwrap_or_default(),
        parent: take_str(&what, &mut fields, "parent")?,
        code_ref: take_str(&what, &mut fields, "codeRef")?,
        extra: fields,
        id,
    })
}

fn relationship_of(value: Value, index: usize) -> Result<Relationship, EngineError> {
    let what = format!("relationships[{index}]");
    let mut fields = fields_of(&what, value)?;
    let id = take_str(&what, &mut fields, "id")?
        .ok_or_else(|| invalid(format!("{what}에 id가 없다")))?;
    let what = format!("관계 {id}");
    let source = take_str(&what, &mut fields, "source")?
        .ok_or_else(|| invalid(format!("{what}에 source가 없다")))?;
    let target = take_str(&what, &mut fields, "target")?
        .ok_or_else(|| invalid(format!("{what}에 target이 없다")))?;
    Ok(Relationship {
        source,
        target,
        label: take_str(&what, &mut fields, "label")?.unwrap_or_default(),
        technology: take_str(&what, &mut fields, "technology")?.unwrap_or_default(),
        extra: fields,
        id,
    })
}

fn position_of(view: &str, id: &str, value: &Value) -> Result<Position, EngineError> {
    let Value::Map(pairs) = value else {
        return Err(invalid(format!(
            "views.yaml의 {view}/{id}는 x, y 맵이어야 한다"
        )));
    };
    let coordinate = |axis: &str| -> Result<i64, EngineError> {
        let found = pairs
            .iter()
            .find(|(key, _)| matches!(key, Value::Str(name) if name == axis))
            .map(|(_, value)| value);
        match found {
            Some(Value::Int(raw)) => raw.parse::<i64>().map_err(|_| ()),
            Some(Value::Float(number)) if number.is_finite() => Ok(number.round() as i64),
            _ => Err(()),
        }
        .map_err(|()| invalid(format!("views.yaml의 {view}/{id}의 {axis}는 수여야 한다")))
    };
    Ok(Position {
        x: coordinate("x")?,
        y: coordinate("y")?,
    })
}

fn put_str(pairs: &mut Vec<(Value, Value)>, key: &str, value: &str) {
    if !value.is_empty() {
        pairs.push((text(key), text(value)));
    }
}

fn element_value(element: &Element) -> Value {
    let mut pairs = vec![
        (text("id"), text(&element.id)),
        (text("kind"), text(element.kind.as_str())),
        (text("name"), text(&element.name)),
    ];
    put_str(&mut pairs, "description", &element.description);
    put_str(&mut pairs, "technology", &element.technology);
    if let Some(parent) = &element.parent {
        pairs.push((text("parent"), text(parent)));
    }
    if let Some(code_ref) = &element.code_ref {
        pairs.push((text("codeRef"), text(code_ref)));
    }
    pairs.extend(element.extra.iter().map(|(k, v)| (text(k), v.clone())));
    Value::Map(pairs)
}

fn relationship_value(relation: &Relationship) -> Value {
    let mut pairs = vec![
        (text("id"), text(&relation.id)),
        (text("source"), text(&relation.source)),
        (text("target"), text(&relation.target)),
    ];
    put_str(&mut pairs, "label", &relation.label);
    put_str(&mut pairs, "technology", &relation.technology);
    pairs.extend(relation.extra.iter().map(|(k, v)| (text(k), v.clone())));
    Value::Map(pairs)
}
