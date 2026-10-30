use std::collections::BTreeMap;

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

pub fn cleaned(mut env: BTreeMap<String, String>) -> BTreeMap<String, String> {
    if env.contains_key("CLAUDECODE") || env.contains_key("CLAUDE_CODE_SESSION_ID") {
        env.retain(|name, _| {
            !CLAUDE_NAMES.contains(&name.as_str())
                && !name.starts_with("CLAUDE_CODE_MESSAGING_")
                && !name.starts_with("CLAUDE_PLUGIN_")
        });
    }
    if env.keys().any(|name| name.starts_with("CMUX_")) {
        if env
            .get("NODE_OPTIONS")
            .is_some_and(|v| v.contains("cmux-claude-node-options"))
        {
            let original = if env
                .get("CMUX_ORIGINAL_NODE_OPTIONS_PRESENT")
                .is_some_and(|v| v == "1")
            {
                env.get("CMUX_ORIGINAL_NODE_OPTIONS").cloned()
            } else {
                None
            };
            if let Some(value) = original {
                env.insert("NODE_OPTIONS".into(), value);
            } else {
                env.remove("NODE_OPTIONS");
            }
        }
        env.retain(|name, _| !name.starts_with("CMUX_"));
    }
    env
}

// Call only before Tauri or any worker thread is created.
pub fn sanitize_startup() {
    let old: BTreeMap<String, String> = std::env::vars().collect();
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
    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
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
}
