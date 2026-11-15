//! 명령 표면(`list-docs limit=5`)의 인자 해석. Java `ArgsParser.parseCommand`와 같은 규칙이다.
//!
//! `--`로 시작하는 칸은 버리고, 둘째 글자부터 `=`가 나오는 칸은 이름 인자, 나머지는 자리 인자다.
//! 이름 인자는 명세(`spec_table.rs`)에 있는 옵션만 받고, 자리 인자는 명세의 상한을 넘으면 사용법과 함께 거절한다.

use ais_core::text::trim;

use crate::error::CliError;
use crate::registry::Spec;

/// 해석을 마친 명령 표면 인자
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedCommand {
    /// 하이픈 이름
    pub name: String,
    pub positional: Vec<String>,
    /// 적힌 차례의 이름 인자. 같은 이름을 또 주면 첫 자리에 값만 바뀐다.
    pub options: Vec<(String, String)>,
}

impl ParsedCommand {
    pub fn option(&self, key: &str) -> Option<&str> {
        self.options
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    }
}

pub fn parse_command(spec: &Spec, raw: &[String]) -> Result<ParsedCommand, CliError> {
    let mut positional = Vec::new();
    let mut options: Vec<(String, String)> = Vec::new();

    for arg in raw {
        if arg.starts_with("--") {
            continue;
        }
        // 처음 나오는 `=`가 이름 인자의 경계다. 그것이 첫 글자면 자리 인자다.
        match arg.find('=').filter(|at| *at >= 1) {
            Some(at) => {
                let name = trim(&arg[..at]).to_string();
                let value = arg[at + 1..].to_string();
                if !spec.allowed.contains(&name.as_str()) {
                    return Err(CliError::new(
                        400,
                        format!(
                            "{} 이 모르는 옵션이다: {name}. 쓸 수 있는 것은 [{}] 뿐이다.",
                            spec.name,
                            spec.allowed.join(", ")
                        ),
                    ));
                }
                match options.iter_mut().find(|(existing, _)| *existing == name) {
                    Some(slot) => slot.1 = value,
                    None => options.push((name, value)),
                }
            }
            None => positional.push(arg.clone()),
        }
    }

    if let Some(max) = spec.max
        && positional.len() > max
    {
        return Err(CliError::new(
            400,
            format!(
                "{} 이 받는 자리 인자는 {max} 개인데 {} 개를 받았다: {}",
                spec.name,
                positional.len(),
                spec.usage
            ),
        ));
    }

    Ok(ParsedCommand {
        name: spec.name.to_string(),
        positional,
        options,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry;

    fn parse(name: &str, args: &[&str]) -> Result<ParsedCommand, CliError> {
        let spec = registry::command(name).expect("명세");
        parse_command(spec, &args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn splits_options_and_positionals() {
        let parsed = parse("list-docs", &["limit=5", "--json", "tag=a,b"]).unwrap();
        assert!(parsed.positional.is_empty());
        assert_eq!(parsed.option("limit"), Some("5"));
        assert_eq!(parsed.option("tag"), Some("a,b"));
    }

    #[test]
    fn leading_equals_is_positional() {
        let parsed = parse("get-doc", &["=x"]).unwrap();
        assert_eq!(parsed.positional, vec!["=x"]);
    }

    #[test]
    fn equals_after_a_leading_equals_stays_positional() {
        let parsed = parse("get-doc", &["=a=b"]).unwrap();
        assert_eq!(parsed.positional, vec!["=a=b"]);
    }

    #[test]
    fn unknown_option_lists_the_allowed_ones() {
        let error = parse("get-doc", &["a=b"]).unwrap_err();
        assert_eq!(
            error.message,
            "get-doc 이 모르는 옵션이다: a. 쓸 수 있는 것은 [] 뿐이다."
        );
    }

    #[test]
    fn too_many_positionals_show_the_usage() {
        let error = parse("get-doc", &["a", "b"]).unwrap_err();
        assert_eq!(
            error.message,
            "get-doc 이 받는 자리 인자는 1 개인데 2 개를 받았다: get-doc <노트 id>"
        );
    }

    #[test]
    fn repeated_option_keeps_the_first_slot_and_the_last_value() {
        let parsed = parse("list-docs", &["limit=1", "type=plan", "limit=2"]).unwrap();
        assert_eq!(
            parsed.options,
            vec![("limit".into(), "2".into()), ("type".into(), "plan".into())]
        );
    }
}
