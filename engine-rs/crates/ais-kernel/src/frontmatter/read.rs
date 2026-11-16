//! 노트 머리 읽기. 파서가 주는 이벤트를 SnakeYAML 2.4(안전 생성자, 시각 해석 끔)와 같은 규칙으로 값으로 바꿉니다.
//!
//! 파서는 YAML 문법만 다루고 `yes`·`0x1F` 같은 글의 형은 정하지 않습니다.
//! 형을 정하는 규칙(불리언, 정수, 실수, null, 병합 키)은 Java 엔진이 쓰는 SnakeYAML의 정규식을 그대로 옮겼습니다.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;

use super::events::{Raw, Style, raw_events};
use super::value::Value;

/// 뿌리 노드의 깊이를 1로 셀 때 깊이 52인 노드에서 서도록 맞춘 상한입니다(SnakeYAML `nestingDepthLimit` 50과 같은 결과).
const MAX_DEPTH: usize = 51;
/// 맵이나 목록을 가리키는 별칭의 상한입니다(SnakeYAML `maxAliasesForCollections`).
const MAX_COLLECTION_ALIASES: usize = 50;

// 아래 정규식은 SnakeYAML 2.4 `Resolver`의 값을 그대로 옮겼다. TIMESTAMP는 일부러 뺐다(시각도 문자열로 읽는다).
static BOOL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:yes|Yes|YES|no|No|NO|true|True|TRUE|false|False|FALSE|on|On|ON|off|Off|OFF)$")
        .unwrap()
});
static INT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?:[-+]?0b_*[0-1][0-1_]*|[-+]?0_*[0-7][0-7_]*|[-+]?(?:0|[1-9][0-9_]*)|[-+]?0x_*[0-9a-fA-F][0-9a-fA-F_]*|[-+]?[1-9][0-9_]*(?::[0-5]?[0-9])+)$",
    )
    .unwrap()
});
static FLOAT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?:[-+]?(?:[0-9][0-9_]*)\.[0-9_]*(?:[eE][-+]?[0-9]+)?|[-+]?(?:[0-9][0-9_]*)(?:[eE][-+]?[0-9]+)|[-+]?\.[0-9_]+(?:[eE][-+]?[0-9]+)?|[-+]?[0-9][0-9_]*(?::[0-5]?[0-9])+\.[0-9_]*|[-+]?\.(?:inf|Inf|INF)|\.(?:nan|NaN|NAN))$",
    )
    .unwrap()
});
static NULL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(?:~|null|Null|NULL| )$").unwrap());

/// 노트 머리 YAML 조각을 읽어 최상위 맵의 `(키, 값)` 목록을 돌려줍니다.
///
/// 머리가 없는 것으로 보는 경우(Java 엔진이 `null`을 돌려주는 경우)는 모두 `None`입니다: 빈 글, 문법 오류,
/// 문서가 둘 이상, 깊이·별칭 상한 초과, 모르는 태그, 정의되지 않은 별칭, 최상위가 맵이 아님.
/// 자기 자신을 가리키는 구조(`a: &x [*x]`)도 `None`입니다. Java 엔진은 순환 구조를 그대로 만들지만 노트 머리에 쓸 일이 없습니다.
pub fn parse(yaml: &str) -> Option<Vec<(Value, Value)>> {
    match parse_value(yaml)? {
        Value::Map(pairs) => Some(pairs),
        _ => None,
    }
}

/// YAML(또는 JSON) 글을 읽어 최상위 값을 그대로 돌려줍니다. 최상위가 맵이 아니어도 됩니다(휴지통 목록처럼 배열인 파일용).
/// 못 읽는 경우(빈 글, 문법 오류, 상한 초과 등)는 `None`입니다.
pub fn parse_value(yaml: &str) -> Option<Value> {
    if yaml.is_empty() {
        return None;
    }
    let tree = Tree::build(yaml)?;
    let mut constructor = Constructor {
        tree: &tree,
        cache: HashMap::new(),
        in_progress: HashSet::new(),
        flattening: HashSet::new(),
        merge_depth: 0,
    };
    constructor.construct(tree.root?)
}

enum NodeKind {
    Scalar {
        text: String,
        style: Style,
    },
    Seq(Vec<usize>),
    Map(Vec<(usize, usize)>),
    /// 앵커 이름을 가리키는 별칭
    Alias(String),
}

struct Node {
    kind: NodeKind,
    tag: Option<String>,
}

/// 맵 틀의 `pending`은 키만 읽고 값을 기다리는 키 노드입니다.
struct Frame {
    node: usize,
    pending: Option<usize>,
}

struct Tree {
    nodes: Vec<Node>,
    /// 앵커 이름이 가장 나중에 가리킨 노드. 같은 이름을 다시 쓰면 뒤의 것이 이긴다.
    anchors: HashMap<String, usize>,
    root: Option<usize>,
}

impl Tree {
    /// 이벤트를 모아 노드 나무로 만듭니다. 재귀를 쓰지 않아서 깊은 입력에도 스택이 넘치지 않습니다.
    fn build(yaml: &str) -> Option<Tree> {
        let mut tree = Tree {
            nodes: Vec::new(),
            anchors: HashMap::new(),
            root: None,
        };
        let mut stack: Vec<Frame> = Vec::new();
        let mut documents = 0;
        let mut collection_aliases = 0;

        for event in raw_events(yaml)? {
            let depth = stack.len() + 1;
            match event {
                Raw::DocumentStart => {
                    documents += 1;
                    if documents > 1 {
                        return None;
                    }
                }
                Raw::Scalar {
                    text,
                    style,
                    anchor,
                    tag,
                } => {
                    tree.add(
                        &mut stack,
                        NodeKind::Scalar { text, style },
                        tag,
                        anchor,
                        depth,
                    )?;
                }
                Raw::SeqStart { anchor, tag } => {
                    let idx =
                        tree.add(&mut stack, NodeKind::Seq(Vec::new()), tag, anchor, depth)?;
                    stack.push(Frame {
                        node: idx,
                        pending: None,
                    });
                }
                Raw::MapStart { anchor, tag } => {
                    let idx =
                        tree.add(&mut stack, NodeKind::Map(Vec::new()), tag, anchor, depth)?;
                    stack.push(Frame {
                        node: idx,
                        pending: None,
                    });
                }
                Raw::SeqEnd | Raw::MapEnd => {
                    stack.pop();
                }
                Raw::Alias(name) => {
                    let target = *tree.anchors.get(&name)?;
                    if matches!(tree.nodes[target].kind, NodeKind::Seq(_) | NodeKind::Map(_)) {
                        collection_aliases += 1;
                        if collection_aliases > MAX_COLLECTION_ALIASES {
                            return None;
                        }
                    }
                    tree.add(&mut stack, NodeKind::Alias(name), None, None, depth)?;
                }
            }
        }
        Some(tree)
    }

    /// 노드를 만들어 부모에 붙이고 번호를 돌려줍니다. 깊이 상한을 넘으면 `None`입니다.
    fn add(
        &mut self,
        stack: &mut [Frame],
        kind: NodeKind,
        tag: Option<String>,
        anchor: Option<String>,
        depth: usize,
    ) -> Option<usize> {
        if depth > MAX_DEPTH {
            return None;
        }
        let idx = self.nodes.len();
        self.nodes.push(Node { kind, tag });
        if let Some(name) = anchor {
            self.anchors.insert(name, idx);
        }
        match stack.last_mut() {
            None => self.root = Some(idx),
            Some(frame) => match &mut self.nodes[frame.node].kind {
                NodeKind::Seq(items) => items.push(idx),
                NodeKind::Map(pairs) => match frame.pending.take() {
                    Some(key) => pairs.push((key, idx)),
                    None => frame.pending = Some(idx),
                },
                _ => return None,
            },
        }
        Some(idx)
    }
}

/// 노드에 붙은 태그를 생성 규칙에 쓰는 분류로 줄입니다.
enum TagClass<'a> {
    /// 태그가 없거나 비특정 태그(`!`)
    Implicit,
    /// `!!str` 같은 YAML 핵심 태그(접미사만)
    Core(&'a str),
    /// 그 밖의 태그. 안전 생성자가 만들 수 없어서 머리 전체를 못 읽은 것으로 본다.
    Unknown,
}

fn classify(tag: Option<&str>) -> TagClass<'_> {
    match tag {
        None | Some("!") => TagClass::Implicit,
        Some(t) => match t.strip_prefix("tag:yaml.org,2002:") {
            Some(suffix) => TagClass::Core(suffix),
            None => TagClass::Unknown,
        },
    }
}

struct Constructor<'a> {
    tree: &'a Tree,
    cache: HashMap<usize, Value>,
    in_progress: HashSet<usize>,
    /// 지금 쌍을 정리(중복·병합)하는 중인 맵 노드. 병합 키가 자기 자신이나 조상을 가리킬 때 끝없이 도는 것을 막는다.
    flattening: HashSet<usize>,
    /// 별칭으로 병합 원본을 정리하며 들어간 깊이. 노드가 한 방향으로만 이어져 끝나지만 만일을 위한 상한이다.
    merge_depth: usize,
}

impl Constructor<'_> {
    /// 노드를 값으로 바꿉니다. `None`은 안전 생성자가 실패하는 경우입니다.
    fn construct(&mut self, idx: usize) -> Option<Value> {
        let tree = self.tree;
        if let NodeKind::Alias(name) = &tree.nodes[idx].kind {
            return self.construct(tree.anchors.get(name).copied()?);
        }
        if let Some(done) = self.cache.get(&idx) {
            return Some(done.clone());
        }
        if !self.in_progress.insert(idx) {
            return None; // 자기 자신을 가리키는 구조
        }
        let node = &tree.nodes[idx];
        let value = match &node.kind {
            NodeKind::Scalar { text, style } => construct_scalar(text, *style, node.tag.as_deref()),
            NodeKind::Seq(items) => self.construct_seq(items, node.tag.as_deref()),
            NodeKind::Map(pairs) => self.construct_map(idx, pairs, node.tag.as_deref()),
            NodeKind::Alias(_) => None,
        };
        self.in_progress.remove(&idx);
        let value = value?;
        self.cache.insert(idx, value.clone());
        Some(value)
    }

    fn construct_seq(&mut self, items: &[usize], tag: Option<&str>) -> Option<Value> {
        match classify(tag) {
            TagClass::Implicit | TagClass::Core("seq") => {}
            TagClass::Core("omap" | "pairs") => return Some(Value::Other),
            _ => return None,
        }
        let mut list = Vec::with_capacity(items.len());
        for &item in items {
            list.push(self.construct(item)?);
        }
        Some(Value::List(list))
    }

    fn construct_map(
        &mut self,
        idx: usize,
        pairs: &[(usize, usize)],
        tag: Option<&str>,
    ) -> Option<Value> {
        match classify(tag) {
            TagClass::Implicit | TagClass::Core("map") => {}
            TagClass::Core("set") => return Some(Value::Other),
            _ => return None,
        }

        self.flattening.insert(idx);
        let tuples = self.flatten(pairs);
        self.flattening.remove(&idx);

        let mut result = Vec::new();
        for (key, value) in tuples? {
            result.push((key, self.construct(value)?));
        }
        Some(Value::Map(result))
    }

    /// 맵의 쌍을 중복 키와 병합 키를 정리한 `(키 값, 값 노드)` 목록으로 만듭니다.
    ///
    /// 같은 키가 여러 번 나오면 앞의 쌍을 지우고 마지막 쌍의 자리에 둡니다(SnakeYAML 2.x `processDuplicateKeys`).
    /// 병합 키(`<<`)가 있으면 병합 원본의 쌍을 나온 차례로 모으고, 먼저 나온 쌍이 이기며,
    /// 이 맵이 직접 적은 쌍만 이미 모인 같은 키를 제자리에서 덮어씁니다(`mergeNode`).
    fn flatten(&mut self, pairs: &[(usize, usize)]) -> Option<Vec<(Value, usize)>> {
        let pairs = self.drop_earlier_duplicates(pairs)?;
        let mut tuples = Vec::new();
        self.merge_tuples(&pairs, true, &mut tuples)?;
        Some(tuples)
    }

    /// 병합 키가 아닌 쌍 가운데 같은 키가 또 나오는 앞쪽 쌍을 뺍니다.
    fn drop_earlier_duplicates(&mut self, pairs: &[(usize, usize)]) -> Option<Vec<(usize, usize)>> {
        let mut last_index: Vec<(Value, usize)> = Vec::new();
        let mut removed: HashSet<usize> = HashSet::new();
        for (i, &(key, _)) in pairs.iter().enumerate() {
            if self.is_merge_key(key) {
                continue;
            }
            let key = self.construct(key)?;
            match last_index.iter_mut().find(|(seen, _)| *seen == key) {
                Some(slot) => {
                    removed.insert(slot.1);
                    slot.1 = i;
                }
                None => last_index.push((key, i)),
            }
        }
        Some(
            pairs
                .iter()
                .enumerate()
                .filter(|(i, _)| !removed.contains(i))
                .map(|(_, pair)| *pair)
                .collect(),
        )
    }

    /// `pairs`를 차례로 `tuples`에 모읍니다. `preferred`는 이 맵이 직접 적은 쌍이 이미 모인 같은 키를 덮어쓸지입니다.
    fn merge_tuples(
        &mut self,
        pairs: &[(usize, usize)],
        preferred: bool,
        tuples: &mut Vec<(Value, usize)>,
    ) -> Option<()> {
        for &(key, value) in pairs {
            if !self.is_merge_key(key) {
                let key = self.construct(key)?;
                insert_tuple(tuples, key, value, preferred);
                continue;
            }
            let source = self.deref_alias(value)?;
            match &self.tree.nodes[source].kind {
                NodeKind::Map(inner) => {
                    let inner = inner.clone();
                    self.merge_map(source, &inner, source != value, tuples)?;
                }
                NodeKind::Seq(items) => {
                    for item in items.clone() {
                        let source = self.deref_alias(item)?;
                        let NodeKind::Map(inner) = &self.tree.nodes[source].kind else {
                            return None;
                        };
                        let inner = inner.clone();
                        self.merge_map(source, &inner, source != item, tuples)?;
                    }
                }
                _ => return None,
            }
        }
        Some(())
    }

    /// 병합 원본 맵 하나를 모읍니다. 앵커로 이미 만든 맵(`by_alias`)은 SnakeYAML이 쌍 목록을 정리해 둔 뒤라서
    /// 정리한 목록을, 그 자리에 적은 맵은 적힌 그대로를 모읍니다.
    fn merge_map(
        &mut self,
        source: usize,
        inner: &[(usize, usize)],
        by_alias: bool,
        tuples: &mut Vec<(Value, usize)>,
    ) -> Option<()> {
        if self.flattening.contains(&source) {
            // 쌍을 정리하는 중인 맵(자기 자신 포함)을 병합하는 경우다. SnakeYAML은 병합 키 쌍을 목록에서 이미 지운 뒤라서
            // 병합 키를 뺀 나머지 쌍만 모은다.
            for &(key, value) in inner {
                if !self.is_merge_key(key) {
                    let key = self.construct(key)?;
                    insert_tuple(tuples, key, value, false);
                }
            }
            return Some(());
        }
        if by_alias {
            self.merge_depth += 1;
            let flattened = if self.merge_depth > 64 {
                None
            } else {
                self.flatten(inner)
            };
            self.merge_depth -= 1;
            for (key, value) in flattened? {
                insert_tuple(tuples, key, value, false);
            }
            Some(())
        } else {
            self.merge_tuples(inner, false, tuples)
        }
    }

    /// 별칭이면 가리키는 앵커 노드의 번호를, 아니면 그대로 돌려줍니다.
    fn deref_alias(&self, idx: usize) -> Option<usize> {
        match &self.tree.nodes[idx].kind {
            NodeKind::Alias(name) => self.tree.anchors.get(name).copied(),
            _ => Some(idx),
        }
    }

    fn is_merge_key(&self, idx: usize) -> bool {
        let node = &self.tree.nodes[idx];
        matches!(&node.kind, NodeKind::Scalar { text, style: Style::Plain } if text == "<<")
            && matches!(classify(node.tag.as_deref()), TagClass::Implicit)
    }
}

/// 모은 쌍에 하나를 더합니다. 같은 키가 이미 있으면 `preferred`일 때만 그 자리를 덮어씁니다.
fn insert_tuple(tuples: &mut Vec<(Value, usize)>, key: Value, value: usize, preferred: bool) {
    match tuples.iter().position(|(seen, _)| *seen == key) {
        None => tuples.push((key, value)),
        Some(at) if preferred => tuples[at] = (key, value),
        Some(_) => {}
    }
}

fn construct_scalar(text: &str, style: Style, tag: Option<&str>) -> Option<Value> {
    match classify(tag) {
        TagClass::Unknown => None,
        TagClass::Core(suffix) => match suffix {
            "str" => Some(Value::Str(text.to_string())),
            "null" => Some(Value::Null),
            "bool" => Some(match text.to_ascii_lowercase().as_str() {
                "yes" | "true" | "on" => Value::Bool(true),
                "no" | "false" | "off" => Value::Bool(false),
                _ => Value::Null,
            }),
            "int" => construct_int(text),
            "float" => construct_float(text).map(Value::Float),
            "binary" | "timestamp" => Some(Value::Other),
            _ => None,
        },
        TagClass::Implicit => {
            if style != Style::Plain {
                return Some(Value::Str(text.to_string()));
            }
            // SnakeYAML이 암시적 형 규칙을 등록한 차례(불리언, 정수, 실수, 병합, null)대로 맞춰 본다
            if BOOL.is_match(text) {
                Some(Value::Bool(matches!(
                    text,
                    "yes" | "Yes" | "YES" | "true" | "True" | "TRUE" | "on" | "On" | "ON"
                )))
            } else if INT.is_match(text) {
                construct_int(text)
            } else if FLOAT.is_match(text) {
                construct_float(text).map(Value::Float)
            } else if text == "<<" {
                None // 병합 키가 아닌 자리의 `<<`는 안전 생성자가 만들지 못한다
            } else if text.is_empty() || NULL.is_match(text) {
                Some(Value::Null)
            } else {
                Some(Value::Str(text.to_string()))
            }
        }
    }
}

/// SnakeYAML `ConstructYamlInt`와 같은 해석입니다: `_` 제거, 부호, `0b`·`0x`·앞의 0(8진수), 60진수(`1:30`).
fn construct_int(raw: &str) -> Option<Value> {
    let cleaned: String = raw.chars().filter(|c| *c != '_').collect();
    let (negative, body) = match cleaned.chars().next()? {
        '-' => (true, &cleaned[1..]),
        '+' => (false, &cleaned[1..]),
        _ => (false, &cleaned[..]),
    };
    if body == "0" {
        return Some(Value::Int("0".to_string()));
    }
    let (digits, radix) = if let Some(rest) = body.strip_prefix("0b") {
        (rest.to_string(), 2)
    } else if let Some(rest) = body.strip_prefix("0x") {
        (rest.to_string(), 16)
    } else if let Some(rest) = body.strip_prefix('0') {
        (rest.to_string(), 8)
    } else if body.contains(':') {
        let mut value: i128 = 0;
        let mut base: i128 = 1;
        for part in body.split(':').rev() {
            value += part.parse::<i128>().ok()? * base;
            base *= 60;
        }
        (value.to_string(), 10)
    } else {
        (body.to_string(), 10)
    };
    match i128::from_str_radix(&digits, radix) {
        Ok(n) => Some(Value::Int((if negative { -n } else { n }).to_string())),
        // i128을 넘는 10진수는 글 그대로 둔다(Java의 BigInteger)
        Err(_)
            if radix == 10 && !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) =>
        {
            Some(Value::Int(format!(
                "{}{}",
                if negative { "-" } else { "" },
                digits
            )))
        }
        Err(_) => None,
    }
}

/// SnakeYAML `ConstructYamlFloat`와 같은 해석입니다: `_` 제거, 부호, `.inf`, `.nan`, 60진수 실수.
fn construct_float(raw: &str) -> Option<f64> {
    let cleaned: String = raw.chars().filter(|c| *c != '_').collect();
    let (sign, body) = match cleaned.chars().next()? {
        '-' => (-1.0, &cleaned[1..]),
        '+' => (1.0, &cleaned[1..]),
        _ => (1.0, &cleaned[..]),
    };
    let lower = body.to_ascii_lowercase();
    if lower == ".inf" {
        return Some(sign * f64::INFINITY);
    }
    if lower == ".nan" {
        return Some(f64::NAN);
    }
    if body.contains(':') {
        let mut value = 0.0;
        let mut base = 1.0;
        for part in body.split(':').rev() {
            value += part.parse::<f64>().ok()? * base;
            base *= 60.0;
        }
        return Some(sign * value);
    }
    Some(body.parse::<f64>().ok()? * sign)
}
