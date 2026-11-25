//! 한 프로젝트가 가진 아키텍처(다이어그램) 여러 개를 가리는 규칙: 식별자, 이름, 이름 파일(`diagram.yaml`).
//! 파일도 시계도 모르는 순수 코드다. 폴더 위치와 읽고 쓰기는 `ais-core`의 `c4::store`가 한다.
//!
//! 기존 파일(`<프로젝트>/model.yaml`)은 그대로 `main`이다. 새로 만드는 것은 `<프로젝트>/diagrams/<id>/` 아래에 둔다.
//! 이름은 `diagram.yaml`의 `name:` 한 칸이다. 이 파일은 모델과 위치의 확인 값(`f1`)에 넣지 않아서, 이름을 바꿔도
//! 다른 쪽에서 읽고 있던 모델이 낡았다고 거절되지 않는다.

use super::{invalid, single_line};
use crate::error::EngineError;
use crate::frontmatter::{self, Value};

/// 기존 파일이 가리키는 기본 아키텍처의 식별자
pub const MAIN: &str = "main";
/// 이름 파일이 없는 `main`이 화면에 보이는 이름
pub const MAIN_NAME: &str = "기본";
/// 아키텍처 개수 상한(`main` 포함)
pub const MAX_DIAGRAMS: usize = 50;
/// 새로 만드는 아키텍처의 식별자 접두. `arch-2`부터 붙는다(`main`이 첫 번째다).
pub const ID_PREFIX: &str = "arch-";

const MAX_DIAGRAM_NAME: usize = 60;
const MAX_ID: usize = 32;
const VERSION: i64 = 1;

/// 식별자가 쓸 수 있는 모양인지 본다: 소문자·숫자로 시작하고 소문자·숫자·`-`만, 32자 이하.
/// 폴더 이름으로 쓰므로 경로 장난(`..`, `/`)이 들어올 틈을 닫는 검사이기도 하다.
pub fn check_id(id: &str) -> Result<(), EngineError> {
    let mut chars = id.chars();
    let first_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    let rest_ok = chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if first_ok && rest_ok && id.chars().count() <= MAX_ID {
        Ok(())
    } else {
        Err(invalid(format!(
            "아키텍처 id가 올바르지 않다: {id} (소문자·숫자로 시작하고 소문자, 숫자, -만 쓰는 {MAX_ID}자 이하)"
        )))
    }
}

/// 이름을 다듬어(앞뒤 공백 제거) 돌려준다. 비었거나 길거나 줄바꿈이 있으면 오류다.
pub fn clean_name(raw: &str) -> Result<String, EngineError> {
    let name = raw.trim();
    single_line("아키텍처 이름", name, MAX_DIAGRAM_NAME, true)?;
    Ok(name.to_string())
}

/// 이미 있는 식별자들 다음에 붙일 새 식별자. `arch-N`의 N은 가장 큰 번호 + 1이고 2부터다.
pub fn next_id<'a>(existing: impl IntoIterator<Item = &'a str>) -> String {
    let highest = existing
        .into_iter()
        .filter_map(|id| id.strip_prefix(ID_PREFIX)?.parse::<u32>().ok())
        .max()
        .unwrap_or(1);
    format!("{ID_PREFIX}{}", highest.max(1) + 1)
}

/// 목록에서 보여 줄 차례: `main`이 맨 앞이고, 나머지는 `arch-N`의 N 순서, 그 밖의 식별자는 글자 순서로 뒤에 둔다.
pub fn order_key(id: &str) -> (u8, u32, String) {
    if id == MAIN {
        return (0, 0, String::new());
    }
    match id
        .strip_prefix(ID_PREFIX)
        .and_then(|n| n.parse::<u32>().ok())
    {
        Some(number) => (1, number, String::new()),
        None => (2, 0, id.to_string()),
    }
}

/// `diagram.yaml` 글에서 이름을 읽는다. 빈 글이면 `None`이다. 모르는 칸은 무시한다.
pub fn parse_name(text: &str) -> Result<Option<String>, EngineError> {
    if text.trim().is_empty() {
        return Ok(None);
    }
    let pairs = frontmatter::parse(text)
        .ok_or_else(|| invalid("diagram.yaml을 읽지 못했다. YAML 맵이어야 한다"))?;
    let mut name = None;
    for (key, value) in pairs {
        match (key, value) {
            (Value::Str(key), Value::Int(raw)) if key == "version" => {
                if raw != VERSION.to_string() {
                    return Err(EngineError::unsupported(format!(
                        "diagram.yaml의 version을 지원하지 않는다(지원: {VERSION})"
                    )));
                }
            }
            (Value::Str(key), Value::Str(value)) if key == "name" => {
                name = Some(clean_name(&value)?);
            }
            (Value::Str(key), _) if key == "name" => {
                return Err(invalid("diagram.yaml의 name은 글이어야 한다"));
            }
            _ => {}
        }
    }
    Ok(name)
}

/// `diagram.yaml` 글을 만든다.
pub fn render_name(name: &str) -> Result<String, EngineError> {
    let top = vec![
        (
            Value::Str("version".into()),
            Value::Int(VERSION.to_string()),
        ),
        (Value::Str("name".into()), Value::Str(name.to_string())),
    ];
    frontmatter::dump_general_map(&top)
        .map_err(|_| EngineError::internal("아키텍처 이름을 파일 글로 쓰지 못했다"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_folder_safe() {
        for good in ["main", "arch-2", "a1", "0x"] {
            assert!(check_id(good).is_ok(), "{good}");
        }
        for bad in ["", "..", "a/b", "A", "-a", "a b", &"a".repeat(33), "한글"] {
            assert!(check_id(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn names_are_trimmed_and_checked() {
        assert_eq!(clean_name("  결제 흐름 ").unwrap(), "결제 흐름");
        assert!(clean_name("   ").is_err());
        assert!(clean_name("줄\n바꿈").is_err());
        assert!(clean_name(&"가".repeat(61)).is_err());
        assert!(clean_name(&"가".repeat(60)).is_ok());
    }

    #[test]
    fn the_next_id_follows_the_highest_number() {
        assert_eq!(next_id(["main"]), "arch-2");
        assert_eq!(next_id(["main", "arch-2", "arch-5"]), "arch-6");
        assert_eq!(next_id(["main", "other"]), "arch-2");
    }

    #[test]
    fn the_list_order_puts_main_first_then_by_number() {
        let mut ids = vec!["zeta", "arch-10", "arch-2", "main", "alpha"];
        ids.sort_by_key(|id| order_key(id));
        assert_eq!(ids, ["main", "arch-2", "arch-10", "alpha", "zeta"]);
    }

    #[test]
    fn the_name_file_round_trips_and_ignores_unknown_fields() {
        let text = render_name("결제 흐름").unwrap();
        assert_eq!(parse_name(&text).unwrap().as_deref(), Some("결제 흐름"));
        assert_eq!(parse_name("").unwrap(), None);
        assert_eq!(
            parse_name("version: 1\nname: 가\nnote: 손으로\n")
                .unwrap()
                .as_deref(),
            Some("가")
        );
        assert!(parse_name("version: 2\nname: 가\n").is_err());
        assert!(parse_name("name: [1]\n").is_err());
    }
}
