//! YAML 문법을 읽는 파서와 `read` 모듈 사이의 얇은 경계입니다.
//!
//! 파서가 주는 이벤트를 파서와 상관없는 [`Raw`]로 바꿔서, 파서를 바꿔도 값 해석 규칙은 그대로 둡니다.
//! 지금 쓰는 파서는 `libyaml-safer`(libyaml을 안전한 Rust로 옮긴 것)입니다. Java 엔진의 SnakeYAML도 같은 PyYAML/libyaml 계통이라
//! 문법을 받아들이는 폭이 거의 같습니다. YAML 1.2 규격을 엄격히 따르는 `saphyr-parser`는 같은 퍼징 4,096건 가운데 80건에서
//! Java와 달랐고 Java가 읽는 입력을 거부하는 경우가 섞여 있었습니다. `libyaml-safer`는 30건이었고 그런 경우가 없었습니다
//! (자세한 내용은 `docs/rust-engine-step1-yaml.md`).

use libyaml_safer::{EventData, Parser, ScalarStyle};

/// 스칼라가 적힌 모양입니다. 형을 정할 때는 따옴표 없는 글(`Plain`)인지만 구별하면 됩니다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Style {
    Plain,
    /// 따옴표나 `|`, `>` 블록
    Other,
}

/// 파서와 상관없는 이벤트입니다. 앵커는 이름으로, 태그는 펼친 전체 글(`tag:yaml.org,2002:str`, `!custom`, `!`)로 줍니다.
pub(super) enum Raw {
    DocumentStart,
    Scalar {
        text: String,
        style: Style,
        anchor: Option<String>,
        tag: Option<String>,
    },
    SeqStart {
        anchor: Option<String>,
        tag: Option<String>,
    },
    SeqEnd,
    MapStart {
        anchor: Option<String>,
        tag: Option<String>,
    },
    MapEnd,
    Alias(String),
}

/// 입력 전체를 이벤트로 바꿉니다. 문법 오류면 `None`입니다.
pub(super) fn raw_events(yaml: &str) -> Option<Vec<Raw>> {
    let mut parser = Parser::new();
    let mut input: &[u8] = yaml.as_bytes();
    parser.set_input_string(&mut input);

    let mut out = Vec::new();
    for event in parser {
        match event.ok()?.data {
            EventData::DocumentStart { .. } => out.push(Raw::DocumentStart),
            EventData::Scalar {
                anchor,
                tag,
                value,
                style,
                ..
            } => out.push(Raw::Scalar {
                text: value,
                style: if style == ScalarStyle::Plain {
                    Style::Plain
                } else {
                    Style::Other
                },
                anchor,
                tag,
            }),
            EventData::SequenceStart { anchor, tag, .. } => out.push(Raw::SeqStart { anchor, tag }),
            EventData::SequenceEnd => out.push(Raw::SeqEnd),
            EventData::MappingStart { anchor, tag, .. } => out.push(Raw::MapStart { anchor, tag }),
            EventData::MappingEnd => out.push(Raw::MapEnd),
            EventData::Alias { anchor } => out.push(Raw::Alias(anchor)),
            _ => {}
        }
    }
    Some(out)
}
