//! C4 아키텍처의 변경 이력(타임 트래블)의 규칙. 스냅숏의 모양, 파일 글, 두 모델을 견줘 한 줄 요약을 짓는 일이다.
//! 파일도 시계도 모르는 순수 코드다. 폴더와 읽고 쓰기는 `ais-core`의 `c4::history_store`가 한다.
//!
//! 스냅숏 하나는 **한 번의 변경이 끝난 뒤의 모양** 전체다(모델과 위치). 요소나 관계가 바뀔 때만 새 점을 만들고, 위치만
//! 바뀐 쓰기는 마지막 점의 위치에 합친다(끌 때마다 점이 쌓이지 않게). 점은 아키텍처마다 `MAX_SNAPSHOTS`개까지 두고
//! 넘으면 오래된 것부터 지운다.

use serde_json::{Value as Json, json};

use super::format::{parse_model, parse_views, render_model, render_views};
use super::{Model, Views, invalid};
use crate::error::EngineError;

/// 아키텍처마다 두는 점의 상한
pub const MAX_SNAPSHOTS: usize = 200;

const MAX_SUMMARY: usize = 200;
const VERSION: i64 = 1;
/// 요약에서 이름을 늘어놓는 개수
const NAMES_SHOWN: usize = 3;

/// 점이 생긴 까닭
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// 이력을 쌓기 시작할 때 이미 있던 모양
    Baseline,
    /// 엔진을 거치지 않고 파일이 바뀐 것(손으로 고침, 다른 도구)을 다음 쓰기 때 알아챈 모양
    External,
    /// 엔진이 적용한 변경의 결과
    Change,
}

impl Kind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Kind::Baseline => "baseline",
            Kind::External => "external",
            Kind::Change => "change",
        }
    }

    pub fn parse(raw: &str) -> Option<Kind> {
        [Kind::Baseline, Kind::External, Kind::Change]
            .into_iter()
            .find(|kind| kind.as_str() == raw)
    }
}

/// 이력의 점 하나
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    /// 1부터 늘어나는 번호. 지워져도 다시 쓰지 않는다.
    pub seq: u32,
    /// 만든 시각(유닉스 밀리초)
    pub at: i64,
    /// 쓴 쪽: `app`, `ai`
    pub by: String,
    pub kind: Kind,
    pub summary: String,
    pub model: Model,
    pub views: Views,
}

impl Snapshot {
    /// 파일 글(JSON)을 만든다. 모델과 위치는 `model.yaml`, `views.yaml`과 같은 글로 넣는다.
    pub fn render(&self) -> Result<String, EngineError> {
        let value = json!({
            "version": VERSION,
            "seq": self.seq,
            "at": self.at,
            "by": self.by,
            "kind": self.kind.as_str(),
            "summary": self.summary,
            "model": render_model(&self.model)?,
            "views": render_views(&self.views)?,
        });
        serde_json::to_string(&value)
            .map_err(|_| EngineError::internal("C4 이력을 파일 글로 쓰지 못했다"))
    }

    /// 파일 글을 읽는다. 모양이 틀리면 이유를 적은 오류다.
    pub fn parse(text: &str) -> Result<Snapshot, EngineError> {
        let value: Json = serde_json::from_str(text)
            .map_err(|_| invalid("C4 이력 파일을 읽지 못했다. JSON이어야 한다"))?;
        if value["version"].as_i64() != Some(VERSION) {
            return Err(EngineError::unsupported(format!(
                "C4 이력 파일의 version을 지원하지 않는다(지원: {VERSION})"
            )));
        }
        let text_of = |key: &str| {
            value[key]
                .as_str()
                .ok_or_else(|| invalid(format!("C4 이력 파일의 {key}가 글이 아니다")))
        };
        let number_of = |key: &str| {
            value[key]
                .as_i64()
                .ok_or_else(|| invalid(format!("C4 이력 파일의 {key}가 정수가 아니다")))
        };
        let model = parse_model(text_of("model")?)?;
        let views = parse_views(text_of("views")?, &model)?;
        let kind =
            Kind::parse(text_of("kind")?).ok_or_else(|| invalid("C4 이력 파일의 kind를 모른다"))?;
        Ok(Snapshot {
            seq: u32::try_from(number_of("seq")?)
                .map_err(|_| invalid("C4 이력 파일의 seq가 범위를 넘었다"))?,
            at: number_of("at")?,
            by: text_of("by")?.to_string(),
            kind,
            summary: text_of("summary")?.to_string(),
            model,
            views,
        })
    }

    /// 응답에 싣는 이름표(모델과 위치는 빼고)
    pub fn row_json(&self) -> Json {
        json!({
            "seq": self.seq,
            "at": self.at,
            "by": self.by,
            "kind": self.kind.as_str(),
            "summary": self.summary,
            "elements": self.model.elements.len(),
            "relationships": self.model.relationships.len(),
        })
    }
}

/// 번호 목록에서 상한을 넘는 오래된 번호들(지울 것). `seqs`는 오름차순이다.
pub fn overflow(seqs: &[u32]) -> &[u32] {
    &seqs[..seqs.len().saturating_sub(MAX_SNAPSHOTS)]
}

/// 두 모델을 견줘 무엇이 바뀌었는지 한 줄로 쓴다. 예: `요소 추가: API, CLI · 관계 삭제: 지시`.
/// 위치는 보지 않는다. 달라진 것이 없으면 빈 글이다.
pub fn summarize(before: &Model, after: &Model) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut push = |what: &str, names: Vec<String>| {
        if !names.is_empty() {
            parts.push(format!("{what}: {}", list_names(&names)));
        }
    };

    let element_name = |model: &Model, id: &str| {
        model
            .element(id)
            .map(|e| e.name.clone())
            .unwrap_or_else(|| id.to_string())
    };
    push(
        "요소 추가",
        after
            .elements
            .iter()
            .filter(|e| before.element(&e.id).is_none())
            .map(|e| e.name.clone())
            .collect(),
    );
    push(
        "요소 삭제",
        before
            .elements
            .iter()
            .filter(|e| after.element(&e.id).is_none())
            .map(|e| e.name.clone())
            .collect(),
    );
    push(
        "요소 수정",
        after
            .elements
            .iter()
            .filter(|e| before.element(&e.id).is_some_and(|old| old != *e))
            .map(|e| e.name.clone())
            .collect(),
    );

    let relation_title = |model: &Model, id: &str| {
        let Some(relation) = model.relationship(id) else {
            return id.to_string();
        };
        if relation.label.trim().is_empty() {
            format!(
                "{} → {}",
                element_name(model, &relation.source),
                element_name(model, &relation.target)
            )
        } else {
            relation.label.clone()
        }
    };
    push(
        "관계 추가",
        after
            .relationships
            .iter()
            .filter(|r| before.relationship(&r.id).is_none())
            .map(|r| relation_title(after, &r.id))
            .collect(),
    );
    push(
        "관계 삭제",
        before
            .relationships
            .iter()
            .filter(|r| after.relationship(&r.id).is_none())
            .map(|r| relation_title(before, &r.id))
            .collect(),
    );
    push(
        "관계 수정",
        after
            .relationships
            .iter()
            .filter(|r| before.relationship(&r.id).is_some_and(|old| old != *r))
            .map(|r| relation_title(after, &r.id))
            .collect(),
    );

    let line = parts.join(" · ");
    if line.chars().count() <= MAX_SUMMARY {
        return line;
    }
    let mut cut: String = line.chars().take(MAX_SUMMARY - 1).collect();
    cut.push('…');
    cut
}

fn list_names(names: &[String]) -> String {
    let shown: Vec<&str> = names.iter().take(NAMES_SHOWN).map(String::as_str).collect();
    let mut text = shown.join(", ");
    if names.len() > NAMES_SHOWN {
        text.push_str(&format!(" 외 {}개", names.len() - NAMES_SHOWN));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::c4::{Element, Kind as ElementKind, Relationship};

    fn element(id: &str, name: &str) -> Element {
        Element {
            id: id.into(),
            kind: ElementKind::System,
            name: name.into(),
            description: String::new(),
            technology: String::new(),
            parent: None,
            code_ref: None,
            extra: Vec::new(),
        }
    }

    fn relation(id: &str, source: &str, target: &str, label: &str) -> Relationship {
        Relationship {
            id: id.into(),
            source: source.into(),
            target: target.into(),
            label: label.into(),
            technology: String::new(),
            source_port: String::new(),
            target_port: String::new(),
            extra: Vec::new(),
        }
    }

    fn model(elements: Vec<Element>, relationships: Vec<Relationship>) -> Model {
        Model {
            elements,
            relationships,
            extra: Vec::new(),
        }
    }

    #[test]
    fn a_snapshot_round_trips_through_its_file_text() {
        let base = model(
            vec![element("a", "에이"), element("b", "비")],
            vec![relation("r1", "a", "b", "지시")],
        );
        let snapshot = Snapshot {
            seq: 7,
            at: 1_795_000_000_123,
            by: "ai".into(),
            kind: Kind::Change,
            summary: "요소 추가: 비".into(),
            model: base,
            views: Views::default(),
        };
        let text = snapshot.render().unwrap();
        assert_eq!(Snapshot::parse(&text).unwrap(), snapshot);
        assert_eq!(snapshot.row_json()["elements"], 2);
    }

    #[test]
    fn a_broken_snapshot_file_is_refused_with_a_reason() {
        assert!(Snapshot::parse("not json").is_err());
        assert!(Snapshot::parse(r#"{"version":2}"#).is_err());
        let ok = Snapshot {
            seq: 1,
            at: 1,
            by: "app".into(),
            kind: Kind::Baseline,
            summary: String::new(),
            model: Model::default(),
            views: Views::default(),
        }
        .render()
        .unwrap();
        let bad_kind = ok.replace("baseline", "weird");
        assert!(
            Snapshot::parse(&bad_kind)
                .unwrap_err()
                .to_string()
                .contains("kind")
        );
    }

    #[test]
    fn the_summary_names_what_changed_and_ignores_positions() {
        let before = model(
            vec![element("a", "에이"), element("b", "비")],
            vec![relation("r1", "a", "b", "")],
        );
        assert_eq!(summarize(&before, &before), "");

        let mut renamed = element("a", "에이 바뀜");
        renamed.technology = "Rust".into();
        let after = model(
            vec![
                renamed,
                element("c", "씨"),
                element("d", "디"),
                element("e", "이"),
                element("f", "에프"),
            ],
            vec![relation("r2", "a", "c", "새 관계")],
        );
        assert_eq!(
            summarize(&before, &after),
            "요소 추가: 씨, 디, 이 외 1개 · 요소 삭제: 비 · 요소 수정: 에이 바뀜 · 관계 추가: 새 관계 · 관계 삭제: 에이 → 비"
        );
    }

    #[test]
    fn the_oldest_numbers_overflow_past_the_cap() {
        let few: Vec<u32> = (1..=5).collect();
        assert!(overflow(&few).is_empty());
        let many: Vec<u32> = (1..=(MAX_SNAPSHOTS as u32 + 3)).collect();
        assert_eq!(overflow(&many), &[1, 2, 3]);
    }
}
