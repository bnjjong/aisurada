//! C4 아키텍처 모델의 규칙. 모양(`Model`, `Views`), 검사(`validate`), 변경 적용(`apply`), 파일 글(`format`)이다.
//! 파일도 시계도 모르는 순수 코드다. vault에서 읽고 쓰는 곳(`c4::store`)은 `ais-core`에 있다.
//!
//! 한 아키텍처(다이어그램)의 모델은 두 파일이다.
//!
//! ```text
//! .aisurada/c4/<프로젝트 slug>/model.yaml   요소와 관계
//! .aisurada/c4/<프로젝트 slug>/views.yaml   뷰별 요소 위치(끌 때마다 바뀌는 좌표를 모델 이력과 떼어 둔다)
//! ```
//!
//! 프로젝트는 아키텍처를 여러 개 가질 수 있다. 위 파일이 `main`이고, 새로 만든 것은
//! `.aisurada/c4/<프로젝트 slug>/diagrams/<id>/` 아래에 같은 두 파일과 이름 파일(`diagram.yaml`)을 둔다(`diagram`).
//!
//! 요소의 단계(레벨)는 종류에서 정한다: 사람·시스템·외부 시스템은 1, 컨테이너는 2, 컴포넌트는 3이다.
//! 코드(4단계)는 직접 그리지 않는다(요소의 `codeRef` 파일 경로만 둔다).

pub mod apply;
pub mod diagram;
pub mod format;
pub mod history;

use std::collections::{BTreeMap, HashSet};

use crate::error::EngineError;
use crate::frontmatter::Value;

/// 요소 개수 상한
pub const MAX_ELEMENTS: usize = 2000;
/// 관계 개수 상한
pub const MAX_RELATIONSHIPS: usize = 5000;
/// 한 번에 적용하는 변경 개수 상한
pub const MAX_OPERATIONS: usize = 200;
/// 모델 파일 한 개의 크기 상한(바이트)
pub const MAX_FILE_BYTES: u64 = 1_000_000;
/// 위치 좌표의 절댓값 상한
pub const MAX_COORDINATE: i64 = 1_000_000;

const MAX_ID: usize = 64;
const MAX_NAME: usize = 120;
const MAX_TECHNOLOGY: usize = 120;
const MAX_LABEL: usize = 200;
const MAX_DESCRIPTION: usize = 2000;
const MAX_CODE_REF: usize = 300;

/// 요소 종류
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Person,
    System,
    External,
    Container,
    Component,
}

impl Kind {
    pub const ALL: [Kind; 5] = [
        Kind::Person,
        Kind::System,
        Kind::External,
        Kind::Container,
        Kind::Component,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Kind::Person => "person",
            Kind::System => "system",
            Kind::External => "external",
            Kind::Container => "container",
            Kind::Component => "component",
        }
    }

    /// 파일과 응답에 적는 이름 그대로여야 한다(대소문자도 구분한다).
    pub fn parse(raw: &str) -> Option<Kind> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == raw)
    }

    /// C4의 단계: 1 시스템 컨텍스트, 2 컨테이너, 3 컴포넌트
    pub const fn level(self) -> u8 {
        match self {
            Kind::Person | Kind::System | Kind::External => 1,
            Kind::Container => 2,
            Kind::Component => 3,
        }
    }
}

/// 요소 하나. 모르는 칸은 `extra`에 그대로 보존한다(손으로 고친 파일을 다시 쓸 때 지우지 않는다).
#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub id: String,
    pub kind: Kind,
    pub name: String,
    pub description: String,
    pub technology: String,
    /// 컨테이너는 시스템, 컴포넌트는 컨테이너의 id. 그 밖의 종류는 없다.
    pub parent: Option<String>,
    /// 코드 위치를 가리키는 파일 경로(선택)
    pub code_ref: Option<String>,
    pub extra: Vec<(String, Value)>,
}

/// 요소 사이의 관계 하나(방향이 있다)
#[derive(Clone, Debug, PartialEq)]
pub struct Relationship {
    pub id: String,
    pub source: String,
    pub target: String,
    pub label: String,
    pub technology: String,
    /// 선이 출발 요소의 어느 연결 지점에 붙는지(`PORT_IDS` 중 하나). 비어 있으면 자동 배치가 정한다.
    pub source_port: String,
    /// 선이 도착 요소의 어느 연결 지점에 붙는지. 비어 있으면 자동 배치가 정한다.
    pub target_port: String,
    pub extra: Vec<(String, Value)>,
}

/// 카드 가장자리의 연결 지점 이름. 상자 카드는 네 변마다 3개씩 12개를 쓰고, 알약 모양인 사람은 네 변 가운데 4개만 쓴다.
/// 예전 이름인 `left`, `right`는 `left-2`, `right-2`와 같게 다룬다.
pub const PORT_IDS: [&str; 14] = [
    "top-1", "top-2", "top-3", "bottom-1", "bottom-2", "bottom-3", "left-1", "left-2", "left-3",
    "right-1", "right-2", "right-3", "left", "right",
];

/// 요소와 관계 전체(`model.yaml`)
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    pub elements: Vec<Element>,
    pub relationships: Vec<Relationship>,
    /// 최상위의 모르는 칸
    pub extra: Vec<(String, Value)>,
}

/// 뷰 안의 요소 위치
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub x: i64,
    pub y: i64,
}

/// 뷰별 위치(`views.yaml`). 뷰 이름은 `context`, `container:<시스템 id>`, `component:<컨테이너 id>`다.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Views {
    pub views: BTreeMap<String, BTreeMap<String, Position>>,
    pub extra: Vec<(String, Value)>,
}

/// 요소·관계·뷰의 id 모양: 영문자나 숫자로 시작하고 영문자, 숫자, `-`, `_`로 이어지는 64자 이하
pub fn is_valid_id(id: &str) -> bool {
    let mut bytes = id.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    id.len() <= MAX_ID
        && first.is_ascii_alphanumeric()
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

impl Model {
    pub fn element(&self, id: &str) -> Option<&Element> {
        self.elements.iter().find(|element| element.id == id)
    }

    pub fn relationship(&self, id: &str) -> Option<&Relationship> {
        self.relationships
            .iter()
            .find(|relationship| relationship.id == id)
    }

    /// 모델이 규칙에 맞는지 검사한다. 파일을 읽을 때와 변경을 적용한 뒤에 부른다.
    pub fn validate(&self) -> Result<(), EngineError> {
        if self.elements.len() > MAX_ELEMENTS {
            return Err(invalid(format!("요소는 {MAX_ELEMENTS}개까지 둘 수 있다")));
        }
        if self.relationships.len() > MAX_RELATIONSHIPS {
            return Err(invalid(format!(
                "관계는 {MAX_RELATIONSHIPS}개까지 둘 수 있다"
            )));
        }
        let mut seen = HashSet::new();
        for element in &self.elements {
            if !is_valid_id(&element.id) {
                return Err(invalid(format!(
                    "요소 id 모양이 아니다: {:?}. 영문자나 숫자로 시작하고 영문자, 숫자, -, _ 만 쓰는 {MAX_ID}자 이하여야 한다",
                    element.id
                )));
            }
            if !seen.insert(element.id.as_str()) {
                return Err(invalid(format!("요소 id가 겹친다: {}", element.id)));
            }
        }
        for element in &self.elements {
            self.validate_element(element)?;
        }
        let mut seen = HashSet::new();
        for relation in &self.relationships {
            if !is_valid_id(&relation.id) {
                return Err(invalid(format!("관계 id 모양이 아니다: {:?}", relation.id)));
            }
            if !seen.insert(relation.id.as_str()) {
                return Err(invalid(format!("관계 id가 겹친다: {}", relation.id)));
            }
            for (side, end) in [("source", &relation.source), ("target", &relation.target)] {
                if self.element(end).is_none() {
                    return Err(invalid(format!(
                        "관계 {}의 {side} 요소가 없다: {end}",
                        relation.id
                    )));
                }
            }
            for (side, port) in [
                ("sourcePort", &relation.source_port),
                ("targetPort", &relation.target_port),
            ] {
                if !port.is_empty() && !PORT_IDS.contains(&port.as_str()) {
                    return Err(invalid(format!(
                        "관계 {}의 {side}는 {} 중 하나여야 한다: {port:?}",
                        relation.id,
                        PORT_IDS.join(", ")
                    )));
                }
            }
            single_line("관계 label", &relation.label, MAX_LABEL, false)?;
            single_line(
                "관계 technology",
                &relation.technology,
                MAX_TECHNOLOGY,
                false,
            )?;
        }
        Ok(())
    }

    fn validate_element(&self, element: &Element) -> Result<(), EngineError> {
        let id = &element.id;
        single_line(&format!("요소 {id}의 이름"), &element.name, MAX_NAME, true)?;
        single_line(
            &format!("요소 {id}의 technology"),
            &element.technology,
            MAX_TECHNOLOGY,
            false,
        )?;
        if element.description.chars().count() > MAX_DESCRIPTION
            || element.description.contains('\0')
        {
            return Err(invalid(format!(
                "요소 {id}의 설명은 {MAX_DESCRIPTION}자 이하여야 한다"
            )));
        }
        if let Some(code_ref) = &element.code_ref {
            single_line(
                &format!("요소 {id}의 codeRef"),
                code_ref,
                MAX_CODE_REF,
                true,
            )?;
        }
        let parent_kind = element
            .parent
            .as_deref()
            .map(|parent| {
                self.element(parent)
                    .map(|p| p.kind)
                    .ok_or_else(|| invalid(format!("요소 {id}의 부모가 없다: {parent}")))
            })
            .transpose()?;
        let required = match element.kind {
            Kind::Person | Kind::System | Kind::External => None,
            Kind::Container => Some(Kind::System),
            Kind::Component => Some(Kind::Container),
        };
        match (required, parent_kind) {
            (None, None) => Ok(()),
            (None, Some(_)) => Err(invalid(format!(
                "{} 요소 {id}에는 부모를 둘 수 없다",
                element.kind.as_str()
            ))),
            (Some(want), Some(got)) if want == got => Ok(()),
            (Some(want), _) => Err(invalid(format!(
                "{} 요소 {id}의 부모는 {} 요소여야 한다",
                element.kind.as_str(),
                want.as_str()
            ))),
        }
    }
}

impl Views {
    /// 뷰 이름이 모델 안의 어떤 범위를 가리키는가. 가리키는 요소가 없거나 종류가 안 맞으면 `false`다.
    pub fn scope_exists(view: &str, model: &Model) -> bool {
        if view == "context" {
            return true;
        }
        let Some((prefix, id)) = view.split_once(':') else {
            return false;
        };
        let want = match prefix {
            "container" => Kind::System,
            "component" => Kind::Container,
            _ => return false,
        };
        model
            .element(id)
            .is_some_and(|element| element.kind == want)
    }

    /// 없어진 요소·관계의 좌표와 범위를 잃은 뷰를 지운다.
    pub fn prune(&mut self, model: &Model) {
        self.views.retain(|view, _| Self::scope_exists(view, model));
        for positions in self.views.values_mut() {
            positions
                .retain(|id, _| model.element(id).is_some() || model.relationship(id).is_some());
        }
        self.views.retain(|_, positions| !positions.is_empty());
    }

    /// 파일에서 읽은 뷰가 모델과 맞는지 본다.
    pub fn validate(&self, model: &Model) -> Result<(), EngineError> {
        for (view, positions) in &self.views {
            if !Self::scope_exists(view, model) {
                return Err(invalid(format!("뷰가 가리키는 범위가 없다: {view}")));
            }
            for (id, position) in positions {
                if model.element(id).is_none() && model.relationship(id).is_none() {
                    return Err(invalid(format!("뷰 {view}의 요소나 관계가 없다: {id}")));
                }
                if position.x.abs() > MAX_COORDINATE || position.y.abs() > MAX_COORDINATE {
                    return Err(invalid(format!("뷰 {view}의 {id} 좌표가 범위를 넘는다")));
                }
            }
        }
        Ok(())
    }
}

pub(crate) fn invalid(message: impl Into<String>) -> EngineError {
    EngineError::invalid_argument(message)
}

/// 한 줄 글 검사: 길이 상한(글자 수), 줄바꿈과 제어 문자 금지, `required`면 공백만으로는 못 둔다.
fn single_line(label: &str, value: &str, max: usize, required: bool) -> Result<(), EngineError> {
    if required && value.trim().is_empty() {
        return Err(invalid(format!("{label}이(가) 비어 있다")));
    }
    if value.chars().count() > max {
        return Err(invalid(format!("{label}은(는) {max}자 이하여야 한다")));
    }
    if value.chars().any(char::is_control) {
        return Err(invalid(format!(
            "{label}에 줄바꿈이나 제어 문자를 넣을 수 없다"
        )));
    }
    Ok(())
}
