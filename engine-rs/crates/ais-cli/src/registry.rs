//! `ais-cli`가 아는 이름의 표. Java `AisCliMain`과 `CommandRegistry`의 목록을 그대로 옮겼다.
//!
//! 이름은 세 갈래다. 도구(밑줄, JSON 한 덩이를 받는다), 명령(하이픈, `key=value`를 받는다), 직접 하는 명령(진단·MCP 등).
//! 도움말과 「알 수 없는 이름」 글이 이 순서와 문구를 그대로 찍으므로 목록의 차례를 바꾸지 않는다.

use crate::spec_table::SPECS;

/// 명령 하나의 명세. `spec_table.rs`가 Java 엔진에서 뽑아 둔 값이다.
#[derive(Debug)]
pub struct Spec {
    /// 하이픈 이름(`list-docs`)
    pub name: &'static str,
    /// 도구 이름(`list_docs`). 도구 표면이 없는 명령은 `None`
    pub tool: Option<&'static str>,
    pub write: bool,
    pub part4: bool,
    /// 잘못 부르면 오류 글 끝에 붙이는 사용법
    pub usage: &'static str,
    /// 쓸 수 있는 `key=value` 옵션(정렬된 차례)
    pub allowed: &'static [&'static str],
    /// 자리 인자의 최대 개수. 상한이 없으면 `None`
    pub max: Option<usize>,
}

/// 하이픈 이름으로 명령 명세를 찾는다.
pub fn command(name: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|spec| spec.name == name)
}

/// 도구 이름(`get_doc`)으로 명세를 찾는다.
pub fn tool(name: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|spec| spec.tool == Some(name))
}

/// `ais-cli`가 도구로 받는 이름 40개
pub const CLI_TOOL_NAMES: &[&str] = &[
    "create_doc",
    "update_doc",
    "set_favorite",
    "evaluate_doc",
    "link_docs",
    "list_docs",
    "search_docs",
    "list_tags",
    "get_doc",
    "delete_doc",
    "list_trash",
    "restore_doc",
    "purge_doc",
    "plan_day",
    "list_templates",
    "get_template",
    "save_template",
    "create_sub_note",
    "list_sub_notes",
    "get_sub_note",
    "update_sub_note",
    "delete_sub_note",
    "write_attachment",
    "list_attachments",
    "list_comments",
    "export_markdown",
    "list_resources",
    "get_resource",
    "save_resource",
    "delete_resource",
    "restore_resource",
    "set_resource_ref",
    "usable_resources",
    "list_resource_versions",
    "list_resource_changes",
    "get_resource_change",
    "list_tasks",
    "get_task",
    "create_task",
    "list_decisions",
];

/// `--list`가 도구 이름 옆에 적는 한 줄 설명
pub fn tool_description(name: &str) -> &'static str {
    match name {
        "create_doc" => "새 노트 1건을 만든다",
        "update_doc" => "노트의 제목, 본문, 태그, 날짜, 시각을 고치고 자리를 다시 정한다",
        "set_favorite" => "즐겨찾기 별과 라벨을 고친다",
        "evaluate_doc" => "평가 점수와 메모를 새기거나 지운다",
        "link_docs" => "다른 노트로의 연결 1건을 잇거나 끊는다",
        "list_docs" => "조건에 맞는 노트 목록을 만든 시각 차례로 낸다",
        "search_docs" => "제목, 메타, 본문, 첨부 글에서 찾아 대목과 함께 낸다",
        "list_tags" => "태그와 쓰인 횟수를 낸다",
        "get_doc" => "노트 1건을 본문, 도장, 코멘트와 함께 낸다",
        "delete_doc" => "노트를 휴지통으로 보낸다",
        "list_trash" => "휴지통 목록을 낸다",
        "restore_doc" => "휴지통의 노트를 되살린다",
        "purge_doc" => "휴지통의 노트를 영구히 지운다",
        "plan_day" => "하루 계획 초안을 짓고, 원하면 노트로 저장한다",
        "list_templates" => "문서와 리추얼 템플릿 목록을 낸다",
        "get_template" => "템플릿 1건을 낸다",
        "save_template" => "문서 템플릿을 저장한다",
        "create_sub_note" => "하위 노트 1건을 만든다",
        "list_sub_notes" => "바로 아래 하위 노트 목록을 낸다",
        "get_sub_note" => "하위 노트 1건을 낸다",
        "update_sub_note" => "하위 노트의 제목과 본문을 고친다",
        "delete_sub_note" => "하위 노트를 지운다",
        "write_attachment" => "노트나 하위 노트에 글 첨부 1개를 쓴다",
        "list_attachments" => "첨부 목록과 본문에 붙일 참조 조각을 낸다",
        "list_comments" => "코멘트 목록을 낸다",
        "export_markdown" => "노트를 마크다운 사본으로 내보낸다",
        "list_resources" => "스킬과 에이전트 정의의 관리 목록을 낸다",
        "get_resource" => "스킬이나 에이전트 정의 1건을 지침과 함께 낸다",
        "save_resource" => "스킬이나 에이전트 정의를 만들거나 고친다",
        "delete_resource" => "스킬이나 에이전트 정의를 휴지통으로 보낸다",
        "restore_resource" => "휴지통의 스킬이나 에이전트 정의를 되살린다",
        "set_resource_ref" => "프로젝트가 글로벌 자원을 참조하는 것을 켜거나 끈다",
        "usable_resources" => "프로젝트가 쓸 수 있는 스킬과 에이전트 정의를 낸다",
        "list_resource_versions" => "스킬이나 에이전트 정의의 고치기 전 판 목록을 낸다",
        "list_resource_changes" => {
            "승인을 기다리는 스킬·에이전트 변경과 승인 안 된 직접 수정을 낸다"
        }
        "get_resource_change" => "승인 대기 변경 1건을 이전 판과 새 판과 함께 낸다",
        "list_tasks" => "태스크(티켓) 목록을 상태, 유형, 저장소, 라벨로 거른다",
        "get_task" => "태스크 1건을 설명, 완료 기준, 연결, 활동과 함께 낸다",
        "create_task" => "새 태스크(티켓) 1건을 만든다",
        "list_decisions" => "노트의 결정 블록(정할 것)과 사람이 남긴 답을 낸다",
        _ => "",
    }
}

/// 엔진을 거치지 않고 `ais-cli`가 직접 하는 명령
pub const DIRECT_COMMANDS: &[&str] = &[
    "doctor",
    "sweep",
    "schedule",
    "install",
    "mcp",
    "claude-mcp",
    "codex-mcp",
    "gemini-mcp",
    "wait-decisions",
];

/// 앱이 쓰는 사이드카 고유 명령 61개
pub const ENGINE_COMMANDS_61: &[&str] = &[
    "reindex",
    "reindex-paths",
    "list-docs",
    "search-docs",
    "get-doc",
    "list-sub-notes",
    "get-sub-note",
    "list-attachments",
    "list-comments",
    "list-tags",
    "list-trash",
    "list-templates",
    "get-template",
    "list-agendas",
    "list-projects",
    "create-project",
    "update-project",
    "delete-project",
    "list-resources",
    "get-resource",
    "save-resource",
    "delete-resource",
    "restore-resource",
    "set-resource-ref",
    "usable-resources",
    "list-resource-versions",
    "save-template",
    "set-favorite",
    "evaluate-doc",
    "link-docs",
    "write-attachment",
    "delete-doc",
    "restore-doc",
    "purge-doc",
    "create-sub-note",
    "update-sub-note",
    "delete-sub-note",
    "export-markdown",
    "create-doc",
    "update-doc",
    "empty-trash",
    "create-agenda",
    "update-agenda",
    "delete-agenda",
    "save-ritual-template",
    "delete-doc-template",
    "init-workspace",
    "create-comment",
    "update-comment",
    "delete-comment",
    "shard-notes",
    "verify-shard",
    "plan-day",
    "check-notes",
    "history",
    "commit-vault",
    "get-agenda-facts",
    "get-ask",
    "append-ask-turn",
    "list-chats",
    "append-chat-turn",
    "update-agenda-facts",
    "facts-brief",
    "read-session",
    "session-titles",
    "recent-sessions",
];

/// 태스크 명령 9개
pub const ENGINE_COMMANDS_TASKS: &[&str] = &[
    "create-task",
    "get-task",
    "list-tasks",
    "update-task",
    "transition-task",
    "link-tasks",
    "add-task-comment",
    "delete-task",
    "restore-task",
];

/// 승인 워크플로 명령 7개. 사람이 앱에서 쓰는 길이라 도구(MCP)로는 열지 않는다.
pub const ENGINE_COMMANDS_APPROVALS: &[&str] = &[
    "list-resource-changes",
    "get-resource-change",
    "approve-resource-change",
    "reject-resource-change",
    "resolve-resource-edit",
    "get-resource-policy",
    "set-resource-policy",
];

/// 결정 블록 명령 2개. AI가 노트에 쓴 「정할 것」과 사람의 답을 읽는다. 기다리기(`wait-decisions`)는 `ais-cli`가 직접 한다.
pub const ENGINE_COMMANDS_DECISIONS: &[&str] = &["list-decisions", "pending-decisions"];

pub const ENGINE_COMMANDS_SUB_AGENDAS: &[&str] = &["create-sub-agenda", "update-sub-agenda"];

/// 사이드카 고유 명령 전부를 쉼표로 이은 글
fn engine_command_list() -> String {
    [
        ENGINE_COMMANDS_61.join(", "),
        ENGINE_COMMANDS_TASKS.join(", "),
        ENGINE_COMMANDS_APPROVALS.join(", "),
        ENGINE_COMMANDS_DECISIONS.join(", "),
        ENGINE_COMMANDS_SUB_AGENDAS.join(", "),
    ]
    .join(", ")
}

/// 도구 이름인가
pub fn is_tool_name(name: &str) -> bool {
    CLI_TOOL_NAMES.contains(&name)
}

/// 엔진으로 가는 명령 이름인가
pub fn is_engine_command(name: &str) -> bool {
    ENGINE_COMMANDS_61.contains(&name)
        || ENGINE_COMMANDS_TASKS.contains(&name)
        || ENGINE_COMMANDS_APPROVALS.contains(&name)
        || ENGINE_COMMANDS_DECISIONS.contains(&name)
        || ENGINE_COMMANDS_SUB_AGENDAS.contains(&name)
}

/// `--list`가 찍는 글
pub fn list_text() -> String {
    let mut text = String::new();
    for name in CLI_TOOL_NAMES {
        text.push_str(&format!("{name} — {}\n", tool_description(name)));
    }
    text.push('\n');
    for line in [
        "doctor — vault, git, MCP 등록, 남은 옛 사이드카의 상태를 진단합니다",
        "sweep — 남은 옛 사이드카(Java)를 내리고 흔적을 지웁니다",
        "schedule — 주기적 작업을 다룹니다",
        "install — 설치할 파일이 없음을 안내하고, 남은 옛 jar를 알려 줍니다",
        "mcp — Claude 같은 MCP 클라이언트가 노트를 읽도록 MCP 서버로 선다(표준 입출력). 쓰기 도구는 --write를 줄 때만 연다",
        "claude-mcp — Claude Code에 이 MCP 서버를 등록한다(status, add [--write], remove)",
        "codex-mcp — Codex에 이 MCP 서버를 등록한다(status, add [--write], remove)",
        "gemini-mcp — Antigravity(agy)나 Gemini CLI에 이 MCP 서버를 등록한다(status, add [--write], remove)",
    ] {
        text.push_str(line);
        text.push('\n');
    }
    text.push('\n');
    text.push_str(&format!("사이드카 고유 명령: {}\n", engine_command_list()));
    text.push_str("  앱이 쓰는 배선입니다. 이름이 하이픈인 것이 표시이고, 도구 이름(밑줄)과 다른 표면입니다.\n");
    text
}

/// 인자 없이 부르거나 `--help`일 때 찍는 글
pub fn usage_text() -> String {
    let mut text = String::new();
    text.push_str("사용법: ais-cli <도구 또는 명령> [인자...]\n");
    text.push_str("  도구는 JSON 한 덩이를 받습니다: ais-cli get_doc '{\"id\":\"...\"}'\n");
    text.push_str("  명령은 이름과 값을 받습니다: ais-cli list-docs limit=0\n");
    text.push('\n');
    text.push_str(&list_text());
    text
}

/// 모르는 이름을 받았을 때 표준 오류에 찍는 글
pub fn unknown_name_text(name: &str) -> String {
    format!(
        "알 수 없는 이름: {name}\n도구: {}\n명령: {}\n사이드카 고유 명령: {}\n전체 설명은 ais-cli --list 로 본다.\n",
        CLI_TOOL_NAMES.join(", "),
        DIRECT_COMMANDS.join(", "),
        engine_command_list()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 엔진 명세에 있는 모든 이름이 `ais-cli`가 받는 목록에도 있어야 한다(없으면 「알 수 없는 이름」으로 거절된다).
    #[test]
    fn every_spec_name_is_reachable() {
        for spec in SPECS {
            assert!(
                is_engine_command(spec.name),
                "{}은 엔진 명령 목록에 없다",
                spec.name
            );
            if let Some(tool) = spec.tool
                && !is_tool_name(tool)
            {
                // 엔진에는 도구 이름이 있지만 ais-cli는 하이픈 명령으로만 여는 것(태스크 쓰기 따위)은 허용한다
                assert!(
                    is_engine_command(spec.name),
                    "{tool}은 어느 쪽으로도 열리지 않는다"
                );
            }
        }
    }

    /// 목록의 모든 이름이 명세에 있어야 한다.
    #[test]
    fn every_listed_name_has_a_spec() {
        for name in CLI_TOOL_NAMES {
            assert!(tool(name).is_some(), "도구 {name}의 명세가 없다");
        }
        for name in ENGINE_COMMANDS_61
            .iter()
            .chain(ENGINE_COMMANDS_TASKS)
            .chain(ENGINE_COMMANDS_APPROVALS)
            .chain(ENGINE_COMMANDS_SUB_AGENDAS)
        {
            assert!(command(name).is_some(), "명령 {name}의 명세가 없다");
        }
    }
}
