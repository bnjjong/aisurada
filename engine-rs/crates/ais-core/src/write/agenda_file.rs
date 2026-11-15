//! 아젠다 파일(`.aisurada/agendas/<slug>.md`) 만들기: 이름 고르기, 색 고르기, 파일 내용.

use std::collections::HashSet;

use crate::frontmatter::dump_string;
use crate::kernel::hue::{HUES, position_or_default};
use crate::vault::agenda::AgendaInfo;

/// 같은 이름의 아젠다·프로젝트를 `이름-2`부터 이 번호까지 센다.
pub const MAX_SLUG_SUFFIX: usize = 100;

pub fn rel_path(slug: &str) -> String {
    format!(".aisurada/agendas/{slug}.md")
}

/// `base`가 비어 있으면 그것을, 아니면 `base-2`부터 `base-100`까지 비어 있는 첫 이름을 돌려준다. 다 차 있으면 `None`이다.
pub fn pick_free_slug(base: &str, taken: &HashSet<String>) -> Option<String> {
    if !taken.contains(base) {
        return Some(base.to_string());
    }
    (2..=MAX_SLUG_SUFFIX)
        .map(|n| format!("{base}-{n}"))
        .find(|candidate| !taken.contains(candidate))
}

/// 가장 적게 쓰인 색(같으면 앞의 것). 모르는 색은 `blue`로 센다.
pub fn least_used_hue<'a>(used: impl IntoIterator<Item = &'a str>) -> &'static str {
    let mut counts = [0usize; HUES.len()];
    for hue in used {
        counts[position_or_default(hue)] += 1;
    }
    let mut chosen = 0;
    for (i, count) in counts.iter().enumerate() {
        if *count < counts[chosen] {
            chosen = i;
        }
    }
    HUES[chosen]
}

/// 아젠다 파일 전체 내용: 머리(`id`, `slug`, `title`, `hue`와 있는 것만 `repo`, `project`, `projectRepo`)와 요약 본문
pub fn render(info: &AgendaInfo) -> Vec<u8> {
    let mut out = String::from("---\n");
    let mut line =
        |key: &str, value: &str| out.push_str(&format!("{key}: {}\n", dump_string(value, 1)));
    line("id", &info.id);
    line("slug", &info.slug);
    line("title", &info.title);
    line("hue", &info.hue);
    if let Some(repo) = &info.repo {
        line("repo", repo);
    }
    if let Some(project) = &info.project {
        line("project", project);
    }
    if let Some(project_repo) = &info.project_repo {
        line("projectRepo", project_repo);
    }
    out.push_str("---\n");
    out.push_str(&info.summary);
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_free_names_and_least_used_hues() {
        let taken: HashSet<String> = ["a", "a-2"].iter().map(|s| s.to_string()).collect();
        assert_eq!(pick_free_slug("b", &taken).as_deref(), Some("b"));
        assert_eq!(pick_free_slug("a", &taken).as_deref(), Some("a-3"));
        let all: HashSet<String> = std::iter::once("a".to_string())
            .chain((2..=100).map(|n| format!("a-{n}")))
            .collect();
        assert_eq!(pick_free_slug("a", &all), None);
        assert_eq!(least_used_hue(std::iter::empty()), HUES[0]);
        assert_eq!(least_used_hue([HUES[0]]), HUES[1]);
    }
}
