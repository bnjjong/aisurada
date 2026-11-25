//! 아젠다 파일(`.aisurada/agendas/<slug>.md`) 만들기: 이름 고르기, 색 고르기, 파일 내용.

use std::collections::HashSet;

use crate::frontmatter::dump_string;
use crate::kernel::hue::{HUES, position_or_default};
use crate::vault::agenda::AgendaInfo;

use super::apparatus::{format_utc_millis, now_millis};
use super::atomic::EngineError;
use super::confine::has_unusable_path_characters;
use super::slug;

/// 같은 이름의 아젠다·프로젝트를 `이름-2`부터 이 번호까지 센다.
pub const MAX_SLUG_SUFFIX: usize = 100;

pub fn rel_path(slug: &str) -> String {
    format!(".aisurada/agendas/{slug}.md")
}

/// 머리(frontmatter)에 YAML 줄 하나를 씁니다: `{key}: {dump_string(value, 1)}\n`.
pub fn push_yaml_line(out: &mut String, key: &str, value: &str) {
    out.push_str(&format!("{key}: {}\n", dump_string(value, 1)));
}

/// 값이 있을 때만 머리 YAML 줄을 씁니다.
pub fn push_yaml_opt(out: &mut String, key: &str, value: Option<&str>) {
    if let Some(v) = value {
        push_yaml_line(out, key, v);
    }
}

/// 엔티티 수정 시 타임스탬프를 갱신합니다. 변경이 있으면 editedAt을 지금 시각으로 갱신하고, 없으면 기존 값을 유지합니다.
pub fn resolve_edit_timestamps(
    changed: bool,
    created_at: Option<String>,
    edited_at: Option<String>,
) -> (String, String) {
    let now = format_utc_millis(now_millis());
    let created = created_at.unwrap_or_else(|| now.clone());
    let edited = if changed {
        now
    } else {
        edited_at.unwrap_or(now)
    };
    (created, edited)
}

/// base 슬러그를 기반으로 미사용 슬러그를 찾고 파일명 검증까지 마쳐 돌려줍니다.
pub fn allocate_entity_slug(
    entity_name: &str,
    title: &str,
    taken: &HashSet<String>,
) -> Result<String, EngineError> {
    let base = slug::build(title, 50);
    let Some(slug) = pick_free_slug(&base, taken) else {
        return Err(EngineError::invalid_argument(format!(
            "같은 이름의 {entity_name} {MAX_SLUG_SUFFIX} 개까지 차 있어 새로 만들 수 없다: {title}"
        )));
    };
    let file_name = format!("{slug}.md");
    if has_unusable_path_characters(&file_name) {
        return Err(EngineError::invalid_argument(format!(
            "경로로 쓸 수 없는 값이다: {file_name}"
        )));
    }
    Ok(slug)
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
pub fn render(info: &AgendaInfo, created_at: &str, edited_at: &str) -> Vec<u8> {
    let mut out = String::from("---\n");
    push_yaml_line(&mut out, "id", &info.id);
    push_yaml_line(&mut out, "slug", &info.slug);
    push_yaml_line(&mut out, "title", &info.title);
    push_yaml_line(&mut out, "hue", &info.hue);
    push_yaml_line(&mut out, "createdAt", created_at);
    push_yaml_line(&mut out, "editedAt", edited_at);
    push_yaml_opt(&mut out, "repo", info.repo.as_deref());
    push_yaml_opt(&mut out, "project", info.project.as_deref());
    push_yaml_opt(&mut out, "projectRepo", info.project_repo.as_deref());
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

        let slug = allocate_entity_slug("테스트", "새 아젠다", &taken).unwrap();
        assert_eq!(slug, "새-아젠다");
        let err = allocate_entity_slug("테스트", "a", &all).unwrap_err();
        assert!(err.message.contains("100 개까지 차 있어"));

        let (c1, e1) = resolve_edit_timestamps(true, Some("2026-01-01T00:00:00.000Z".into()), None);
        assert_eq!(c1, "2026-01-01T00:00:00.000Z");
        assert_ne!(e1, "2026-01-01T00:00:00.000Z");

        let (c2, e2) = resolve_edit_timestamps(false, Some("c".into()), Some("e".into()));
        assert_eq!(c2, "c");
        assert_eq!(e2, "e");
    }
}
