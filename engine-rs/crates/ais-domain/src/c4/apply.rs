//! 변경 묶음 적용. 화면, 챗, AI 에이전트가 모두 같은 변경 목록(`operations`)을 보낸다.
//!
//! 변경은 순서대로 메모리의 사본에 적용하고, 하나라도 틀리면 아무것도 바꾸지 않은 채 몇 번째 변경이 왜 틀렸는지 알린다.
//! 끝나면 모델 전체를 다시 검사한다(부모 종류, 끊긴 관계 따위). 없어진 요소의 좌표는 뷰에서 지운다.
//!
//! | `op` | 칸 | 뜻 |
//! |---|---|---|
//! | `put_element` | `id`, `kind`, `name`, `description`, `technology`, `parent`, `codeRef` | 없는 id면 만들고(`kind`, `name` 필요), 있으면 준 칸만 고친다. 빈 글은 칸을 비운다 |
//! | `delete_element` | `id` | 요소와 그 자식, 그 요소들에 닿은 관계를 모두 지운다 |
//! | `put_relationship` | `id`(선택), `source`, `target`, `label`, `technology` | id가 없으면 새 id(`r1`, `r2`…)를 붙인다. 있는 id면 준 칸만 고친다 |
//! | `delete_relationship` | `id` | 관계를 지운다 |
//! | `set_positions` | `view`, `positions` | 뷰 안 요소의 좌표를 정한다 |

use std::collections::{BTreeMap, HashSet};

use serde::Deserialize;
use serde_json::Value as Json;

use super::{
    Element, Kind, MAX_COORDINATE, MAX_OPERATIONS, Model, Position, Relationship, Views, invalid,
    is_valid_id,
};
use crate::error::EngineError;

/// 변경 하나
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    #[serde(rename_all = "camelCase")]
    PutElement {
        id: String,
        kind: Option<String>,
        name: Option<String>,
        description: Option<String>,
        technology: Option<String>,
        parent: Option<String>,
        code_ref: Option<String>,
    },
    DeleteElement {
        id: String,
    },
    #[serde(rename_all = "camelCase")]
    PutRelationship {
        id: Option<String>,
        source: Option<String>,
        target: Option<String>,
        label: Option<String>,
        technology: Option<String>,
        source_port: Option<String>,
        target_port: Option<String>,
    },
    DeleteRelationship {
        id: String,
    },
    SetPositions {
        view: String,
        positions: BTreeMap<String, PointInput>,
    },
}

/// 요청으로 받은 좌표. 정수로 반올림해서 저장한다.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointInput {
    pub x: f64,
    pub y: f64,
}

/// 적용 결과
#[derive(Debug)]
pub struct Outcome {
    pub model: Model,
    pub views: Views,
    /// 새로 붙인 관계 id(변경 차례대로)
    pub created_relationships: Vec<String>,
    pub removed_elements: usize,
    pub removed_relationships: usize,
}

/// `operations` 값(JSON 배열, 또는 배열을 담은 JSON 글)을 변경 목록으로 읽는다.
pub fn parse_operations(raw: &Json) -> Result<Vec<Operation>, EngineError> {
    let parsed;
    let array = match raw {
        Json::String(text) => {
            parsed = serde_json::from_str::<Json>(text)
                .map_err(|_| invalid("operations는 JSON 배열이어야 한다"))?;
            &parsed
        }
        other => other,
    };
    let Json::Array(items) = array else {
        return Err(invalid("operations는 JSON 배열이어야 한다"));
    };
    if items.is_empty() {
        return Err(invalid("operations에 변경이 하나 이상 있어야 한다"));
    }
    if items.len() > MAX_OPERATIONS {
        return Err(invalid(format!(
            "변경은 한 번에 {MAX_OPERATIONS}개까지 보낼 수 있다"
        )));
    }
    items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            Operation::deserialize(item)
                .map_err(|error| invalid(format!("operations[{index}]: {error}")))
        })
        .collect()
}

/// 변경 묶음을 모델과 뷰의 사본에 적용한다. 틀리면 원본은 그대로다.
pub fn apply(
    model: &Model,
    views: &Views,
    operations: &[Operation],
) -> Result<Outcome, EngineError> {
    let mut model = model.clone();
    let mut views = views.clone();
    let mut outcome = Outcome {
        model: Model::default(),
        views: Views::default(),
        created_relationships: Vec::new(),
        removed_elements: 0,
        removed_relationships: 0,
    };
    for (index, operation) in operations.iter().enumerate() {
        apply_one(&mut model, &mut views, operation, &mut outcome).map_err(|error| {
            EngineError {
                message: format!("operations[{index}]: {}", error.message),
                ..error
            }
        })?;
    }
    model.validate()?;
    views.prune(&model);
    outcome.model = model;
    outcome.views = views;
    Ok(outcome)
}

fn apply_one(
    model: &mut Model,
    views: &mut Views,
    operation: &Operation,
    outcome: &mut Outcome,
) -> Result<(), EngineError> {
    match operation {
        Operation::PutElement {
            id,
            kind,
            name,
            description,
            technology,
            parent,
            code_ref,
        } => put_element(
            model,
            id,
            kind.as_deref(),
            name.as_deref(),
            description.as_deref(),
            technology.as_deref(),
            parent.as_deref(),
            code_ref.as_deref(),
        ),
        Operation::DeleteElement { id } => delete_element(model, id, outcome),
        Operation::PutRelationship {
            id,
            source,
            target,
            label,
            technology,
            source_port,
            target_port,
        } => {
            let created = put_relationship(
                model,
                id.as_deref(),
                source.as_deref(),
                target.as_deref(),
                label.as_deref(),
                technology.as_deref(),
                (source_port.as_deref(), target_port.as_deref()),
            )?;
            outcome.created_relationships.extend(created);
            Ok(())
        }
        Operation::DeleteRelationship { id } => {
            let before = model.relationships.len();
            model.relationships.retain(|relation| &relation.id != id);
            if model.relationships.len() == before {
                return Err(EngineError::not_found(format!("관계를 못 찾았다: {id}")));
            }
            outcome.removed_relationships += 1;
            Ok(())
        }
        Operation::SetPositions { view, positions } => set_positions(model, views, view, positions),
    }
}

#[allow(clippy::too_many_arguments)]
fn put_element(
    model: &mut Model,
    id: &str,
    kind: Option<&str>,
    name: Option<&str>,
    description: Option<&str>,
    technology: Option<&str>,
    parent: Option<&str>,
    code_ref: Option<&str>,
) -> Result<(), EngineError> {
    if !is_valid_id(id) {
        return Err(invalid(format!(
            "요소 id 모양이 아니다: {id:?}. 영문자나 숫자로 시작하고 영문자, 숫자, -, _ 만 쓰는 64자 이하여야 한다"
        )));
    }
    let kind = kind
        .map(|raw| {
            Kind::parse(raw).ok_or_else(|| {
                invalid(format!(
                    "kind를 모른다: {raw} (person, system, external, container, component)"
                ))
            })
        })
        .transpose()?;
    match model.elements.iter_mut().find(|element| element.id == id) {
        Some(element) => {
            if let Some(kind) = kind {
                element.kind = kind;
            }
            if let Some(name) = name {
                element.name = name.trim().to_string();
            }
            if let Some(description) = description {
                element.description = description.trim().to_string();
            }
            if let Some(technology) = technology {
                element.technology = technology.trim().to_string();
            }
            if let Some(parent) = parent {
                element.parent = non_blank(Some(parent)).map(|p| p.trim().to_string());
            }
            if let Some(code_ref) = code_ref {
                element.code_ref = non_blank(Some(code_ref)).map(|c| c.trim().to_string());
            }
        }
        None => {
            let kind = kind.ok_or_else(|| invalid(format!("새 요소 {id}에는 kind가 필요하다")))?;
            let name = non_blank(name)
                .ok_or_else(|| invalid(format!("새 요소 {id}에는 name이 필요하다")))?;
            model.elements.push(Element {
                id: id.to_string(),
                kind,
                name: name.trim().to_string(),
                description: description.unwrap_or_default().trim().to_string(),
                technology: technology.unwrap_or_default().trim().to_string(),
                parent: non_blank(parent).map(|p| p.trim().to_string()),
                code_ref: non_blank(code_ref).map(|c| c.trim().to_string()),
                extra: Vec::new(),
            });
        }
    }
    Ok(())
}

fn non_blank(value: Option<&str>) -> Option<&str> {
    value.filter(|v| !v.trim().is_empty())
}

fn delete_element(model: &mut Model, id: &str, outcome: &mut Outcome) -> Result<(), EngineError> {
    if model.element(id).is_none() {
        return Err(EngineError::not_found(format!("요소를 못 찾았다: {id}")));
    }
    let mut gone: HashSet<String> = HashSet::from([id.to_string()]);
    loop {
        let more: Vec<String> = model
            .elements
            .iter()
            .filter(|element| {
                !gone.contains(&element.id)
                    && element
                        .parent
                        .as_ref()
                        .is_some_and(|parent| gone.contains(parent))
            })
            .map(|element| element.id.clone())
            .collect();
        if more.is_empty() {
            break;
        }
        gone.extend(more);
    }
    let elements = model.elements.len();
    let relationships = model.relationships.len();
    model.elements.retain(|element| !gone.contains(&element.id));
    model
        .relationships
        .retain(|r| !gone.contains(&r.source) && !gone.contains(&r.target));
    outcome.removed_elements += elements - model.elements.len();
    outcome.removed_relationships += relationships - model.relationships.len();
    Ok(())
}

fn put_relationship(
    model: &mut Model,
    id: Option<&str>,
    source: Option<&str>,
    target: Option<&str>,
    label: Option<&str>,
    technology: Option<&str>,
    (source_port, target_port): (Option<&str>, Option<&str>),
) -> Result<Option<String>, EngineError> {
    if let Some(id) = id {
        if !is_valid_id(id) {
            return Err(invalid(format!("관계 id 모양이 아니다: {id:?}")));
        }
        if let Some(relation) = model.relationships.iter_mut().find(|r| r.id == id) {
            if let Some(source) = source {
                relation.source = source.to_string();
            }
            if let Some(target) = target {
                relation.target = target.to_string();
            }
            if let Some(label) = label {
                relation.label = label.trim().to_string();
            }
            if let Some(technology) = technology {
                relation.technology = technology.trim().to_string();
            }
            if let Some(port) = source_port {
                relation.source_port = port.trim().to_string();
            }
            if let Some(port) = target_port {
                relation.target_port = port.trim().to_string();
            }
            return Ok(None);
        }
    }
    let (Some(source), Some(target)) = (source, target) else {
        return Err(invalid("새 관계에는 source와 target이 필요하다"));
    };
    let (new_id, created) = match id {
        Some(id) => (id.to_string(), None),
        None => {
            let next = model
                .relationships
                .iter()
                .filter_map(|r| r.id.strip_prefix('r')?.parse::<u64>().ok())
                .max()
                .map_or(1, |n| n + 1);
            let generated = format!("r{next}");
            (generated.clone(), Some(generated))
        }
    };
    model.relationships.push(Relationship {
        id: new_id,
        source: source.to_string(),
        target: target.to_string(),
        label: label.unwrap_or_default().trim().to_string(),
        technology: technology.unwrap_or_default().trim().to_string(),
        source_port: source_port.unwrap_or_default().trim().to_string(),
        target_port: target_port.unwrap_or_default().trim().to_string(),
        extra: Vec::new(),
    });
    Ok(created)
}

fn set_positions(
    model: &Model,
    views: &mut Views,
    view: &str,
    positions: &BTreeMap<String, PointInput>,
) -> Result<(), EngineError> {
    if !Views::scope_exists(view, model) {
        return Err(invalid(format!(
            "뷰가 가리키는 범위가 없다: {view}. context, container:<시스템 id>, component:<컨테이너 id> 중 하나여야 한다"
        )));
    }
    let placed = views.views.entry(view.to_string()).or_default();
    for (id, point) in positions {
        if model.element(id).is_none() && model.relationship(id).is_none() {
            return Err(EngineError::not_found(format!("요소나 관계를 못 찾았다: {id}")));
        }
        let (x, y) = (point.x.round(), point.y.round());
        if x.abs() > MAX_COORDINATE as f64 || y.abs() > MAX_COORDINATE as f64 {
            return Err(invalid(format!(
                "{id}의 좌표는 ±{MAX_COORDINATE} 안이어야 한다"
            )));
        }
        placed.insert(
            id.clone(),
            Position {
                x: x as i64,
                y: y as i64,
            },
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::c4::format;
    use serde_json::json;

    fn run(model: &Model, views: &Views, ops: Json) -> Result<Outcome, EngineError> {
        apply(model, views, &parse_operations(&ops)?)
    }

    fn base() -> (Model, Views) {
        let out = run(
            &Model::default(),
            &Views::default(),
            json!([
                {"op":"put_element","id":"me","kind":"person","name":"개발자"},
                {"op":"put_element","id":"app","kind":"system","name":"aisurada","description":"vault 앱"},
                {"op":"put_element","id":"gh","kind":"external","name":"GitHub"},
                {"op":"put_element","id":"engine","kind":"container","name":"엔진","parent":"app","technology":"Rust"},
                {"op":"put_element","id":"store","kind":"component","name":"저장소","parent":"engine"},
                {"op":"put_relationship","source":"me","target":"app","label":"지시"},
                {"op":"put_relationship","source":"app","target":"gh","label":"push"},
                {"op":"set_positions","view":"context","positions":{"me":{"x":10.4,"y":20.6},"app":{"x":300,"y":20}}},
                {"op":"set_positions","view":"container:app","positions":{"engine":{"x":0,"y":0}}}
            ]),
        )
        .unwrap();
        (out.model, out.views)
    }

    #[test]
    fn builds_a_model_and_names_new_relationships() {
        let (model, views) = base();
        assert_eq!(model.elements.len(), 5);
        assert_eq!(
            model
                .relationships
                .iter()
                .map(|r| r.id.as_str())
                .collect::<Vec<_>>(),
            ["r1", "r2"]
        );
        assert_eq!(views.views["context"]["me"], Position { x: 10, y: 21 });
    }

    #[test]
    fn partial_edit_keeps_untouched_fields() {
        let (model, views) = base();
        let out = run(
            &model,
            &views,
            json!([{"op":"put_element","id":"app","name":"aisurada 앱","description":""}]),
        )
        .unwrap();
        let app = out.model.element("app").unwrap();
        assert_eq!(app.name, "aisurada 앱");
        assert_eq!(app.description, "");
        assert_eq!(app.kind, Kind::System);
    }

    #[test]
    fn deleting_a_parent_removes_children_relations_and_positions() {
        let (model, views) = base();
        let out = run(&model, &views, json!([{"op":"delete_element","id":"app"}])).unwrap();
        assert_eq!(out.removed_elements, 3);
        assert_eq!(out.removed_relationships, 2);
        assert!(out.model.element("engine").is_none());
        assert!(!out.views.views.contains_key("container:app"));
        assert!(!out.views.views["context"].contains_key("app"));
        assert!(out.views.views["context"].contains_key("me"));
    }

    #[test]
    fn a_bad_operation_changes_nothing_and_names_its_index() {
        let (model, views) = base();
        let error = run(
            &model,
            &views,
            json!([
                {"op":"put_element","id":"extra","kind":"system","name":"x"},
                {"op":"put_element","id":"bad","kind":"component","name":"y","parent":"app"}
            ]),
        )
        .unwrap_err();
        assert!(
            error.message.contains("부모는 container"),
            "{}",
            error.message
        );
        // 모델 검사는 끝에서 하므로 번호 없이 이유만 온다. 원본은 그대로다.
        assert_eq!(model.elements.len(), 5);
    }

    #[test]
    fn unknown_ops_fields_and_kinds_are_rejected() {
        assert!(parse_operations(&json!([{"op":"nope"}])).is_err());
        assert!(parse_operations(&json!([{"op":"delete_element","id":"a","x":1}])).is_err());
        let (model, views) = base();
        let error = run(
            &model,
            &views,
            json!([{"op":"put_element","id":"z","kind":"planet","name":"z"}]),
        )
        .unwrap_err();
        assert!(
            error.message.starts_with("operations[0]:"),
            "{}",
            error.message
        );
        assert!(parse_operations(&json!([])).is_err());
        assert!(parse_operations(&json!("not json")).is_err());
    }

    #[test]
    fn operations_may_arrive_as_a_json_string() {
        let ops = parse_operations(&json!(
            r#"[{"op":"put_element","id":"a","kind":"system","name":"A"}]"#
        ))
        .unwrap();
        assert_eq!(ops.len(), 1);
    }

    #[test]
    fn relationship_endpoints_must_exist_and_ids_stay_unique() {
        let (model, views) = base();
        assert!(
            run(
                &model,
                &views,
                json!([{"op":"put_relationship","source":"me","target":"ghost"}])
            )
            .is_err()
        );
        let out = run(
            &model,
            &views,
            json!([{"op":"put_relationship","id":"r1","label":"새 글"}]),
        )
        .unwrap();
        assert_eq!(out.model.relationships[0].label, "새 글");
        assert_eq!(out.model.relationships.len(), 2);
    }

    #[test]
    fn positions_need_a_real_view_and_known_elements() {
        let (model, views) = base();
        assert!(
            run(
                &model,
                &views,
                json!([{"op":"set_positions","view":"container:gh","positions":{}}])
            )
            .is_err()
        );
        assert!(
            run(
                &model,
                &views,
                json!([{"op":"set_positions","view":"context","positions":{"nobody":{"x":1,"y":1}}}])
            )
            .is_err()
        );
    }

    #[test]
    fn files_round_trip_and_keep_unknown_fields() {
        let (model, views) = base();
        let model_text = format::render_model(&model).unwrap();
        let views_text = format::render_views(&views).unwrap();
        let again = format::parse_model(&model_text).unwrap();
        assert_eq!(again, model);
        assert_eq!(format::parse_views(&views_text, &again).unwrap(), views);

        let hand = "version: 1\nelements:\n  - id: a\n    kind: system\n    name: A\n    owner: team-x\nrelationships: []\nnote: keep\n";
        let parsed = format::parse_model(hand).unwrap();
        let written = format::render_model(&parsed).unwrap();
        assert!(written.contains("owner: team-x"), "{written}");
        assert!(written.contains("note: keep"), "{written}");
    }

    #[test]
    fn broken_files_stop_with_a_reason() {
        assert!(format::parse_model("elements: 3").is_err());
        assert!(format::parse_model("version: 2\nelements: []").is_err());
        let dup = "version: 1\nelements:\n  - {id: a, kind: system, name: A}\n  - {id: a, kind: system, name: B}\n";
        assert!(
            format::parse_model(dup)
                .unwrap_err()
                .message
                .contains("겹친다")
        );
        assert!(format::parse_model("").unwrap().elements.is_empty());
    }

    #[test]
    fn views_of_vanished_elements_are_ignored_but_bad_coordinates_are_not() {
        let (model, _) = base();
        let stale = "version: 1\nviews:\n  context:\n    me: {x: 1, y: 2}\n    ghost: {x: 3, y: 4}\n  container:gone:\n    me: {x: 5, y: 6}\n";
        let views = format::parse_views(stale, &model).unwrap();
        assert_eq!(views.views.len(), 1);
        assert_eq!(views.views["context"].len(), 1);
        let far = "version: 1\nviews:\n  context:\n    me: {x: 99999999, y: 2}\n";
        assert!(format::parse_views(far, &model).is_err());
    }

    #[test]
    fn a_line_remembers_which_ports_it_attaches_to() {
        let (model, views) = base();
        let out = run(
            &model,
            &views,
            json!([{"op":"put_relationship","id":"r1","sourcePort":"right","targetPort":"top-2"}]),
        )
        .unwrap();
        let line = out
            .model
            .relationships
            .iter()
            .find(|r| r.id == "r1")
            .unwrap();
        assert_eq!(
            (line.source_port.as_str(), line.target_port.as_str()),
            ("right", "top-2")
        );
        assert_eq!(line.label, "지시", "고치지 않은 칸은 그대로다");
        // 한쪽만 고치면 다른 쪽은 그대로다
        let again = run(
            &out.model,
            &out.views,
            json!([{"op":"put_relationship","id":"r1","targetPort":"left"}]),
        )
        .unwrap();
        let line = again
            .model
            .relationships
            .iter()
            .find(|r| r.id == "r1")
            .unwrap();
        assert_eq!(
            (line.source_port.as_str(), line.target_port.as_str()),
            ("right", "left")
        );
        // 빈 글은 자동 배치로 되돌린다
        let cleared = run(
            &again.model,
            &again.views,
            json!([{"op":"put_relationship","id":"r1","sourcePort":"","targetPort":""}]),
        )
        .unwrap();
        let line = cleared
            .model
            .relationships
            .iter()
            .find(|r| r.id == "r1")
            .unwrap();
        assert!(line.source_port.is_empty() && line.target_port.is_empty());
        // 새 관계도 지점을 받을 수 있다
        let made = run(
            &model,
            &views,
            json!([{"op":"put_relationship","source":"me","target":"gh","sourcePort":"bottom-3","targetPort":"top-1"}]),
        )
        .unwrap();
        let line = made.model.relationships.last().unwrap();
        assert_eq!(
            (line.source_port.as_str(), line.target_port.as_str()),
            ("bottom-3", "top-1")
        );
    }

    #[test]
    fn an_unknown_port_is_refused_and_changes_nothing() {
        let (model, views) = base();
        let err = run(
            &model,
            &views,
            json!([{"op":"put_relationship","id":"r1","sourcePort":"middle"}]),
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("sourcePort") && err.contains("middle"),
            "{err}"
        );
        assert!(model.relationships.iter().all(|r| r.source_port.is_empty()));
    }

    #[test]
    fn ports_survive_the_files_and_show_in_json() {
        let (model, views) = base();
        let out = run(
            &model,
            &views,
            json!([{"op":"put_relationship","id":"r2","sourcePort":"left","targetPort":"bottom-2"}]),
        )
        .unwrap();
        let text = format::render_model(&out.model).unwrap();
        assert!(
            text.contains("sourcePort: left") && text.contains("targetPort: bottom-2"),
            "{text}"
        );
        assert_eq!(format::parse_model(&text).unwrap(), out.model);
        let json = format::model_json(&out.model);
        let line = json["relationships"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == "r2")
            .unwrap();
        assert_eq!(
            (line["sourcePort"].as_str(), line["targetPort"].as_str()),
            (Some("left"), Some("bottom-2"))
        );
        let auto = json["relationships"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == "r1")
            .unwrap();
        assert_eq!(auto["sourcePort"].as_str(), Some(""), "자동은 빈 글이다");
        // 손으로 쓴 파일의 틀린 지점은 이유를 달고 멈춘다
        let hand = "version: 1\nelements:\n  - {id: a, kind: system, name: A}\n  - {id: b, kind: system, name: B}\nrelationships:\n  - {id: r1, source: a, target: b, sourcePort: nowhere}\n";
        assert!(
            format::parse_model(hand)
                .unwrap_err()
                .to_string()
                .contains("sourcePort")
        );
    }
}
