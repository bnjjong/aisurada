//! 프로세스의 환경 변수를 그대로 읽는 어댑터.

use std::ffi::OsString;

use crate::ports::EnvSource;

pub struct ProcessEnv;

impl EnvSource for ProcessEnv {
    fn var_os(&self, name: &str) -> Option<OsString> {
        std::env::var_os(name)
    }
}
