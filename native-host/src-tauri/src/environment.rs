use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
};

const CLAUDE_NAMES: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_PID",
    "CLAUDE_EFFORT",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_TMPDIR",
    "CLAUDE_CODE_INVOKED_SKILLS",
    "CLAUDE_PROJECT_DIR",
    "CLAUDE_ENV_FILE",
    "GIT_EDITOR",
    "GIT_CONFIG_PARAMETERS",
    "AI_AGENT",
    "COREPACK_ENABLE_AUTO_PIN",
    "NoDefaultCurrentDirectoryInExePath",
    "TRACEPARENT",
    "BUN_OPTIONS",
];

pub fn cleaned(mut env: BTreeMap<OsString, OsString>) -> BTreeMap<OsString, OsString> {
    if env.contains_key(OsStr::new("CLAUDECODE"))
        || env.contains_key(OsStr::new("CLAUDE_CODE_SESSION_ID"))
    {
        env.retain(|name, _| match name.to_str() {
            Some(name) => {
                !CLAUDE_NAMES.contains(&name)
                    && !name.starts_with("CLAUDE_CODE_MESSAGING_")
                    && !name.starts_with("CLAUDE_PLUGIN_")
            }
            None => true,
        });
    }
    if env
        .keys()
        .any(|name| name.to_str().is_some_and(|name| name.starts_with("CMUX_")))
    {
        if env
            .get(OsStr::new("NODE_OPTIONS"))
            .and_then(|v| v.to_str())
            .is_some_and(|v| v.contains("cmux-claude-node-options"))
        {
            let original = if env
                .get(OsStr::new("CMUX_ORIGINAL_NODE_OPTIONS_PRESENT"))
                .is_some_and(|v| v == "1")
            {
                env.get(OsStr::new("CMUX_ORIGINAL_NODE_OPTIONS")).cloned()
            } else {
                None
            };
            if let Some(value) = original {
                env.insert("NODE_OPTIONS".into(), value);
            } else {
                env.remove(OsStr::new("NODE_OPTIONS"));
            }
        }
        env.retain(|name, _| !name.to_str().is_some_and(|name| name.starts_with("CMUX_")));
    }
    env
}

// Must run before Tauri or any worker thread is created. Non-UTF8 names/values survive.
pub fn sanitize_startup() {
    let old: BTreeMap<OsString, OsString> = std::env::vars_os().collect();
    let new = cleaned(old.clone());
    for name in old.keys() {
        if !new.contains_key(name) {
            std::env::remove_var(name);
        }
    }
    for (name, value) in new {
        std::env::set_var(name, value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn map(pairs: &[(&str, &str)]) -> BTreeMap<OsString, OsString> {
        pairs
            .iter()
            .map(|(k, v)| (OsString::from(k), OsString::from(v)))
            .collect()
    }
    #[test]
    fn preserves_unmarked_user_environment() {
        let env = map(&[
            ("GIT_EDITOR", "vim"),
            ("CLAUDE_CODE_OAUTH_TOKEN", "user"),
            ("NODE_OPTIONS", "custom"),
        ]);
        assert_eq!(cleaned(env.clone()), env);
    }
    #[test]
    fn removes_only_injected_claude_variables() {
        let result = cleaned(map(&[
            ("CLAUDECODE", ""),
            ("CLAUDE_PLUGIN_X", "x"),
            ("CLAUDE_CODE_MESSAGING_X", "x"),
            ("GIT_EDITOR", "x"),
            ("CLAUDE_CONFIG_DIR", "keep"),
            ("CLAUDE_CODE_OAUTH_TOKEN", "keep"),
        ]));
        assert_eq!(
            result,
            map(&[
                ("CLAUDE_CONFIG_DIR", "keep"),
                ("CLAUDE_CODE_OAUTH_TOKEN", "keep")
            ])
        );
    }
    #[test]
    fn restores_cmux_node_options_even_when_empty() {
        let result = cleaned(map(&[
            ("CMUX_ORIGINAL_NODE_OPTIONS_PRESENT", "1"),
            ("CMUX_ORIGINAL_NODE_OPTIONS", ""),
            ("NODE_OPTIONS", "cmux-claude-node-options"),
        ]));
        assert_eq!(result, map(&[("NODE_OPTIONS", "")]));
        assert!(cleaned(map(&[
            ("CMUX_X", "1"),
            ("NODE_OPTIONS", "cmux-claude-node-options")
        ]))
        .is_empty());
    }

    #[test]
    fn all_injected_names_are_removed_but_user_and_terminal_settings_survive() {
        let mut values = map(&[
            ("CLAUDE_CODE_SESSION_ID", ""),
            ("CLAUDE_CONFIG_DIR", "keep"),
            ("CLAUDE_CODE_USE_BEDROCK", "keep"),
            ("CLAUDE_CODE_OAUTH_TOKEN", "keep"),
            ("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS", "keep"),
            ("TERM_PROGRAM", "keep"),
            ("GHOSTTY_X", "keep"),
            ("TERMINFO", "keep"),
        ]);
        for name in CLAUDE_NAMES {
            values.insert(OsString::from(name), OsString::from("injected"));
        }
        let result = cleaned(values);
        assert_eq!(result.len(), 7);
        assert!(result.values().all(|v| v == "keep"));
    }
    #[test]
    fn cmux_leaves_unrelated_node_options_and_restores_original() {
        let result = cleaned(map(&[("CMUX_X", "x"), ("NODE_OPTIONS", "--user-setting")]));
        assert_eq!(result, map(&[("NODE_OPTIONS", "--user-setting")]));
        let result = cleaned(map(&[
            ("CMUX_ORIGINAL_NODE_OPTIONS_PRESENT", "1"),
            ("CMUX_ORIGINAL_NODE_OPTIONS", "original"),
            ("NODE_OPTIONS", "cmux-claude-node-options"),
        ]));
        assert_eq!(result, map(&[("NODE_OPTIONS", "original")]));
    }
    #[test]
    #[cfg(unix)]
    fn non_utf8_names_and_values_are_preserved() {
        use std::os::unix::ffi::OsStringExt;
        let raw = OsString::from_vec(vec![0xff]);
        let mut values = map(&[("CLAUDECODE", "1")]);
        values.insert(raw.clone(), raw.clone());
        values.insert("USER_SETTING".into(), raw.clone());
        let result = cleaned(values);
        assert_eq!(result.get(&raw), Some(&raw));
        assert_eq!(result.get(OsStr::new("USER_SETTING")), Some(&raw));
    }
}
