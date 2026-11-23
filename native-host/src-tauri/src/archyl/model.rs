use super::{mcp, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashSet;

#[derive(Serialize)]
pub struct Project {
    pub id: String,
    pub name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Model {
    pub elements: Vec<Element>,
    pub relationships: Vec<Relationship>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Element {
    pub id: String,
    pub name: String,
    pub description: String,
    pub technology: String,
    pub level: u8,
    pub parent_id: Option<String>,
    pub kind: String,
    pub position: Option<Position>,
    pub icon: Option<String>,
    pub color: Option<String>,
}

#[derive(Serialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Serialize)]
pub struct Relationship {
    pub id: Option<String>,
    pub source: String,
    pub target: String,
    pub label: String,
    pub technology: String,
}

pub fn id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub fn data(value: &Value) -> &Value {
    value.get("data").filter(|v| !v.is_null()).unwrap_or(value)
}

fn text<'a>(value: &'a Value, keys: &[&str]) -> &'a str {
    keys.iter()
        .find_map(|key| value[*key].as_str().filter(|s| !s.trim().is_empty()))
        .unwrap_or("")
}

fn array<'a>(value: &'a Value, keys: &[&str]) -> &'a [Value] {
    keys.iter()
        .find_map(|key| value[*key].as_array())
        .map_or(&[], Vec::as_slice)
}

pub fn projects(value: &Value) -> Result<Vec<Project>> {
    let root = data(value);
    let items = root
        .as_array()
        .or_else(|| root["projects"].as_array())
        .or_else(|| root["items"].as_array())
        .ok_or("Archyl 프로젝트 목록의 형식을 확인하지 못했어요.")?;
    let mut seen = HashSet::new();
    Ok(items
        .iter()
        .filter_map(|row| {
            let key = text(row, &["id", "projectId"]);
            if !id(key) || !seen.insert(key) {
                return None;
            }
            let name = text(row, &["name", "title"]);
            Some(Project {
                id: key.into(),
                name: if name.is_empty() { key } else { name }.into(),
            })
        })
        .collect())
}

pub fn fetch(client: &mut mcp::Mcp<'_>, project: &str) -> Result<Model> {
    if !id(project) {
        return Err("Archyl 프로젝트 주소를 확인해 주세요.".into());
    }
    parse(&client.call("get_project_c4_model", json!({ "projectId": project }))?)
}

const GROUPS: [&str; 4] = ["systems", "containers", "components", "codeElements"];

fn add_elements(
    model: &mut Vec<Element>,
    seen: &mut HashSet<String>,
    rows: &[Value],
    level: u8,
    parent: Option<&str>,
) {
    for row in rows {
        let key = text(row, &["id", "elementId"]);
        if !id(key) {
            continue;
        }
        let parent = match level {
            1 => None,
            2 => Some(text(row, &["parentId", "systemId"])),
            3 => Some(text(row, &["parentId", "containerId"])),
            _ => Some(text(row, &["parentId", "componentId"])),
        }
        .filter(|key| id(key))
        .or(parent);
        let node_type = text(row, &["type", "systemType"]).to_ascii_lowercase();
        let kind = match (level, node_type.as_str(), row["isExternal"] == true) {
            (1, "person", _) => "person",
            (1, "external" | "external_system", _) | (1, _, true) => "external",
            (1, _, _) => "system",
            (2, _, _) => "container",
            (3, _, _) => "component",
            _ => "code",
        };
        if seen.insert(key.into()) {
            let name = text(row, &["name", "title"]);
            let x = row["position"]["x"]
                .as_f64()
                .or_else(|| row["positionX"].as_f64());
            let y = row["position"]["y"]
                .as_f64()
                .or_else(|| row["positionY"].as_f64());
            let position = x
                .zip(y)
                .filter(|(x, y)| x.abs() <= 1_000_000. && y.abs() <= 1_000_000.)
                .map(|(x, y)| Position { x, y });
            let style = |key: &str| {
                row[key]
                    .as_str()
                    .or_else(|| row["style"][key].as_str())
                    .filter(|value| value.len() <= 64)
                    .map(str::to_owned)
            };
            model.push(Element {
                id: key.into(),
                name: if name.is_empty() { key } else { name }.into(),
                description: text(row, &["description", "summary"]).into(),
                technology: text(row, &["technology", "technologies"]).into(),
                level,
                parent_id: parent.map(str::to_owned),
                kind: kind.into(),
                position,
                icon: style("icon"),
                color: style("color"),
            });
        }
        if let Some(group) = GROUPS.get(level as usize) {
            add_elements(model, seen, array(row, &[*group]), level + 1, Some(key));
        }
    }
}

pub fn parse(value: &Value) -> Result<Model> {
    let root = data(value);
    let model = ["model", "c4Model", "architecture"]
        .iter()
        .find_map(|key| root.get(*key).filter(|v| v.is_object()))
        .unwrap_or(root);
    let systems = model["systems"]
        .as_array()
        .or_else(|| root["systems"].as_array())
        .ok_or("Archyl C4 모델의 시스템 목록을 읽지 못했어요.")?;
    let mut elements = Vec::new();
    let mut seen = HashSet::new();
    add_elements(&mut elements, &mut seen, systems, 1, None);
    for (index, group) in GROUPS.iter().enumerate().skip(1) {
        add_elements(
            &mut elements,
            &mut seen,
            array(model, &[*group]),
            (index + 1) as u8,
            None,
        );
        if !std::ptr::eq(model, root) {
            add_elements(
                &mut elements,
                &mut seen,
                array(root, &[*group]),
                (index + 1) as u8,
                None,
            );
        }
    }
    let relations = model["relationships"]
        .as_array()
        .or_else(|| root["relationships"].as_array());
    let relationships = relations
        .into_iter()
        .flatten()
        .filter_map(|row| {
            let endpoint = |keys: &[&str], object: &str| {
                let key = text(row, keys);
                if key.is_empty() {
                    text(&row[object], &["id", "elementId"])
                } else {
                    key
                }
            };
            let source = endpoint(&["sourceId", "sourceElementId", "source", "from"], "source");
            let target = endpoint(&["targetId", "targetElementId", "target", "to"], "target");
            if !seen.contains(source) || !seen.contains(target) {
                return None;
            }
            let key = text(row, &["id", "relationshipId"]);
            Some(Relationship {
                id: id(key).then(|| key.to_owned()),
                source: source.into(),
                target: target.into(),
                label: text(row, &["description", "label", "name", "type"]).into(),
                technology: text(row, &["technology"]).into(),
            })
        })
        .collect();
    Ok(Model {
        elements,
        relationships,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archyl_model_keeps_saved_positions_relations_and_nested_parents() {
        let model = parse(&json!({"data": {"c4Model": {
            "systems": [
                {"id":"client","name":"Client","type":"person","position":{"x":-30,"y":45}},
                {"id":"server","name":"Server","containers":[
                    {"id":"api","name":"API","components":[{"id":"router","name":"Router"}]}
                ]}
            ],
            "relationships":[
                {"id":"link","sourceId":"client","targetId":"server","description":"Calls"},
                {"id":"dangling","sourceId":"foreign","targetId":"server"}
            ]
        }}}))
        .unwrap();
        assert_eq!(model.elements.len(), 4);
        let client = model
            .elements
            .iter()
            .find(|item| item.id == "client")
            .unwrap();
        assert_eq!(client.kind, "person");
        assert_eq!(client.position.as_ref().unwrap().x, -30.);
        let router = model
            .elements
            .iter()
            .find(|item| item.id == "router")
            .unwrap();
        assert_eq!(router.parent_id.as_deref(), Some("api"));
        assert_eq!(model.relationships.len(), 1);
        assert_eq!(model.relationships[0].label, "Calls");
    }
}
