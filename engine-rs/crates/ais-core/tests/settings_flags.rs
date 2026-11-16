//! 환경 변수로 받는 켜고 끄는 설정의 철자. 켜기는 `true`·`on`·`yes`·`1`, 끄기는 `false`·`off`·`no`·`0`이고
//! 영문 대소문자와 앞뒤 공백은 가리지 않는다(Java 엔진이 쓰던 철자 집합, [#56](https://github.com/bnjjong/aisurada/issues/56)).
//! 그 밖의 값은 엔진이 멈추지 않고 기본값을 쓴다.
//!
//! 환경 변수를 바꾸는 시험은 `autocommit_switch_reads_every_spelling` 하나뿐이다.

use ais_core::text::parse_flag;
use ais_core::write::autocommit::AutoCommitConfig;

#[test]
fn flag_spellings() {
    for raw in ["true", "TRUE", " On ", "yes", "YES", "1", "\ttrue\n"] {
        assert_eq!(parse_flag(raw), Some(true), "{raw:?}");
    }
    for raw in ["false", "False", "off", "OFF", "no", "No", "0", "  false  "] {
        assert_eq!(parse_flag(raw), Some(false), "{raw:?}");
    }
    for raw in ["", "  ", "flase", "2", "y", "n", "enabled", "참", "true!"] {
        assert_eq!(parse_flag(raw), None, "{raw:?}");
    }
}

#[test]
fn autocommit_switch_reads_every_spelling() {
    const KEY: &str = "AISURADA_VAULT_AUTOCOMMIT_ENABLED";
    let enabled = |value: Option<&str>| {
        // SAFETY: 이 파일에서 환경 변수를 바꾸는 시험은 이것 하나뿐이다.
        unsafe {
            match value {
                Some(v) => std::env::set_var(KEY, v),
                None => std::env::remove_var(KEY),
            }
        }
        AutoCommitConfig::from_env().enabled
    };

    assert!(enabled(None), "정하지 않으면 켜진다");
    for off in ["false", "FALSE", "off", "Off", "no", "0", " false "] {
        assert!(!enabled(Some(off)), "{off:?}는 꺼야 한다");
    }
    for on in ["true", "TRUE", "on", "yes", "1"] {
        assert!(enabled(Some(on)), "{on:?}는 켠다");
    }
    for unknown in ["", "flase", "2", "disabled"] {
        assert!(
            enabled(Some(unknown)),
            "{unknown:?}는 모르는 값이라 기본값(켜짐)"
        );
    }
}
