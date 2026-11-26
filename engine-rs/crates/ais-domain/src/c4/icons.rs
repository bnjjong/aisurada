//! 요소에 고를 수 있는 아이콘 이름의 닫힌 목록. 화면의 아이콘 모음(`native-host/src/ui/architecture/icons.ts`)과 같은 이름이어야
//! 하고, 두 목록이 같은지는 화면 시험(`architecture-icons.test.mjs`)이 이 파일을 읽어 맞춘다.
//!
//! 이름은 두 갈래다. 앞쪽은 앱이 직접 그린 선 아이콘(`person`, `git-branch` …)이고, 뒤쪽은 서비스 로고다.
//! 로고 이름은 [Simple Icons](https://simpleicons.org/)의 slug 그대로다(`github`, `googlegemini` …).
//! 이름을 고르지 않은 요소는 `icon`이 없고, 화면이 종류에 맞는 기본 아이콘을 쓴다.

/// 앱이 직접 그린 선 아이콘
pub const BUILTIN: [&str; 34] = [
    "bell",
    "book",
    "bot",
    "box",
    "bug",
    "calendar",
    "chart",
    "chat",
    "cloud",
    "code",
    "cpu",
    "database",
    "folder",
    "git-branch",
    "globe",
    "grid",
    "image",
    "key",
    "link",
    "lock",
    "mail",
    "note",
    "person",
    "plug",
    "queue",
    "server",
    "settings",
    "shield",
    "sparkle",
    "tab",
    "terminal",
    "tree",
    "window",
    "zap",
];

/// 서비스 로고(Simple Icons의 slug)
pub const LOGOS: [&str; 76] = [
    "anthropic",
    "apple",
    "asana",
    "bitbucket",
    "claude",
    "cloudflare",
    "confluence",
    "datadog",
    "discord",
    "docker",
    "dropbox",
    "figma",
    "firebase",
    "git",
    "github",
    "githubactions",
    "githubcopilot",
    "gitlab",
    "gmail",
    "go",
    "google",
    "googlecalendar",
    "googlecloud",
    "googledrive",
    "googlegemini",
    "grafana",
    "huggingface",
    "javascript",
    "jenkins",
    "jira",
    "kakaotalk",
    "kotlin",
    "kubernetes",
    "line",
    "linear",
    "linux",
    "mistralai",
    "mongodb",
    "mysql",
    "naver",
    "nextdotjs",
    "nginx",
    "nodedotjs",
    "notion",
    "npm",
    "obsidian",
    "ollama",
    "openjdk",
    "perplexity",
    "pnpm",
    "postgresql",
    "prometheus",
    "python",
    "react",
    "redis",
    "rust",
    "sentry",
    "spring",
    "sqlite",
    "stripe",
    "supabase",
    "swift",
    "tailwindcss",
    "tauri",
    "telegram",
    "terraform",
    "trello",
    "typescript",
    "vercel",
    "vite",
    "vuedotjs",
    "whatsapp",
    "x",
    "youtube",
    "zapier",
    "zoom",
];

/// 고를 수 있는 이름인가. 대소문자까지 그대로 맞아야 한다.
pub fn is_known(name: &str) -> bool {
    BUILTIN.contains(&name) || LOGOS.contains(&name)
}

/// 모르는 이름을 거절할 때 보여 주는 전체 목록(한 줄)
pub fn catalog_line() -> String {
    format!(
        "선 아이콘: {} / 서비스 로고: {}",
        BUILTIN.join(", "),
        LOGOS.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_sorted_and_file_safe() {
        for list in [&BUILTIN[..], &LOGOS[..]] {
            assert!(
                list.windows(2).all(|pair| pair[0] < pair[1]),
                "정렬·중복: {list:?}"
            );
            assert!(list.iter().all(|name| {
                !name.is_empty()
                    && name.len() <= 40
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            }));
        }
        assert!(
            BUILTIN.iter().all(|name| !LOGOS.contains(name)),
            "두 갈래의 이름이 겹친다"
        );
    }

    #[test]
    fn only_listed_names_are_known() {
        assert!(is_known("git-branch") && is_known("github"));
        assert!(!is_known("GitHub") && !is_known("githab") && !is_known(""));
        assert!(catalog_line().contains("서비스 로고: anthropic"));
    }
}
