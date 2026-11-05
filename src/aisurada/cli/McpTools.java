package aisurada.cli;

import aisurada.args.JsonCompact;
import aisurada.cmd.CommandRegistry;

import java.util.ArrayList;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * MCP 클라이언트(Claude 등)에 내놓는 도구의 이름, 설명, 입력 스키마입니다.
 * 이름은 엔진 도구 29개와 같고, 입력 스키마의 키는 {@code ArgsParser}가 받는 키와 같아야 합니다(시험이 잰다).
 *
 * <p>읽기와 쓰기는 엔진의 {@link CommandRegistry#WRITE_COMMANDS}에서 갈라 냅니다. 기본은 읽기 도구만 내놓고,
 * 쓰기 도구는 {@code mcp --write}로 뜰 때만 내놓습니다. 영구 삭제({@code purge_doc})는 어느 쪽으로도 내놓지 않습니다.
 * 모델이 되돌릴 수 없는 삭제를 하지 못하게 하려는 것이고, 휴지통 비우기는 앱에서 사람이 합니다.
 */
final class McpTools {

    /** 모델에게 절대 내놓지 않는 도구. 되돌릴 수 없는 삭제입니다. */
    static final Set<String> NEVER_EXPOSED = Set.of("purge_doc");

    /**
     * 도구 하나의 MCP 정의입니다.
     *
     * @param inputSchema 한 줄로 줄인 JSON 스키마 글
     * @param write 노트나 vault를 바꾸는 도구인지 (엔진의 쓰기 깃발과 같다)
     * @param destructive 있던 내용을 지우거나 덮어쓸 수 있는 도구인지
     */
    record Tool(String name, String description, String inputSchema, boolean write, boolean destructive) {}

    private static final Map<String, Tool> BY_NAME;
    private static final List<Tool> ALL;

    private McpTools() {}

    /** 정의된 도구 29개 전부(내놓지 않는 것 포함)를 엔진 도구 차례로 돌려줍니다. */
    static List<Tool> all() {
        return ALL;
    }

    /** 이름으로 정의를 찾습니다. 없으면 null입니다. */
    static Tool find(String name) {
        return BY_NAME.get(name);
    }

    /** 이 모드에서 목록에 내놓는 도구입니다. 읽기 모드는 읽기 도구만, 쓰기 모드는 영구 삭제만 뺀 전부입니다. */
    static List<Tool> exposed(boolean allowWrite) {
        List<Tool> out = new ArrayList<>();
        for (Tool t : ALL) {
            if (isExposed(t, allowWrite)) {
                out.add(t);
            }
        }
        return out;
    }

    static boolean isExposed(Tool tool, boolean allowWrite) {
        if (NEVER_EXPOSED.contains(tool.name())) {
            return false;
        }
        return allowWrite || !tool.write();
    }

    // ---- 스키마 만들기 ----

    /** 스키마 한 장을 짓는 작은 도구입니다. 엔진은 모르는 키를 거절하므로 additionalProperties는 항상 false입니다. */
    private static final class Schema {
        private final StringBuilder props = new StringBuilder();
        private final List<String> required = new ArrayList<>();

        private Schema add(String name, String type, String description, boolean isRequired, String extra) {
            if (props.length() > 0) {
                props.append(',');
            }
            JsonCompact.appendString(props, name);
            props.append(":{\"type\":\"").append(type).append('"');
            if (extra != null) {
                props.append(',').append(extra);
            }
            props.append(",\"description\":");
            JsonCompact.appendString(props, description);
            props.append('}');
            if (isRequired) {
                required.add(name);
            }
            return this;
        }

        Schema str(String name, String description, boolean isRequired) {
            return add(name, "string", description, isRequired, null);
        }

        Schema bool(String name, String description) {
            return add(name, "boolean", description, false, null);
        }

        Schema integer(String name, String description) {
            return add(name, "integer", description, false, "\"minimum\":1");
        }

        Schema number(String name, String description) {
            return add(name, "number", description, false, "\"minimum\":0,\"maximum\":100");
        }

        Schema strings(String name, String description) {
            return add(name, "array", description, false, "\"items\":{\"type\":\"string\"}");
        }

        Schema links(String name, String description) {
            return add(name, "array", description, false,
                    "\"items\":{\"type\":\"object\",\"properties\":{\"to\":{\"type\":\"string\"},\"kind\":{\"type\":\"string\",\"enum\":[\"direct\",\"indirect\",\"attach\"]}}}");
        }

        String build() {
            StringBuilder sb = new StringBuilder("{\"type\":\"object\",\"properties\":{").append(props).append('}');
            if (!required.isEmpty()) {
                sb.append(",\"required\":[");
                for (int i = 0; i < required.size(); i++) {
                    if (i > 0) {
                        sb.append(',');
                    }
                    JsonCompact.appendString(sb, required.get(i));
                }
                sb.append(']');
            }
            return sb.append(",\"additionalProperties\":false}").toString();
        }
    }

    private static Schema schema() {
        return new Schema();
    }

    private static final String F1 = "직전에 get_doc 등으로 읽은 노트의 precondition 값(f1:…). 주면 그 사이에 다른 곳에서 고쳐진 노트를 덮어쓰지 않고 멈춘다.";

    /** list_docs와 search_docs가 같이 받는 거름 조건 8개를 붙입니다. */
    private static Schema filters(Schema s) {
        return s.str("agenda", "아젠다의 slug나 이름", false)
                .str("date", "그 날짜(YYYY-MM-DD)의 노트만", false)
                .str("dateFrom", "이 날짜(YYYY-MM-DD)부터", false)
                .str("dateTo", "이 날짜(YYYY-MM-DD)까지", false)
                .str("type", "노트 유형", false)
                .str("tag", "태그. 쉼표로 여럿을 줄 수 있다", false)
                .bool("favorite", "true면 즐겨찾기한 노트만")
                .str("favoriteCategory", "즐겨찾기 라벨", false);
    }

    private static Tool tool(String name, String description, Schema schema) {
        boolean write = CommandRegistry.WRITE_COMMANDS.contains(name.replace('_', '-'));
        boolean destructive = write && (name.startsWith("delete_") || name.startsWith("update_")
                || name.startsWith("save_") || name.equals("link_docs") || name.equals("purge_doc"));
        return new Tool(name, description, schema.build(), write, destructive);
    }

    static {
        List<Tool> t = new ArrayList<>();
        t.add(tool("create_doc",
                "새 노트 1건을 만든다. 응답에 id와 쓰기 전 토큰(precondition)이 들어 있다.",
                schema().str("agenda", "노트를 둘 아젠다의 slug나 이름", true)
                        .str("type", "노트 유형(예: free)", true)
                        .str("title", "제목", true)
                        .str("body", "본문(마크다운)", true)
                        .str("date", "노트 날짜(YYYY-MM-DD). 안 주면 오늘", false)
                        .str("time", "노트 시각(HH:MM)", false)
                        .str("format", "본문 형식. 기본은 markdown", false)
                        .links("links", "다른 노트로의 연결 목록. 항목은 {to: 노트 id, kind: direct|indirect|attach}")
                        .strings("tags", "태그 목록")
                        .str("template", "쓸 템플릿 이름", false)));
        t.add(tool("update_doc",
                "노트의 제목, 본문, 태그를 고친다. 준 칸만 바뀐다. 본문을 주면 통째로 바뀌므로 먼저 get_doc으로 읽고 고친 전체를 보낸다.",
                schema().str("id", "고칠 노트의 id", true)
                        .str("body", "새 본문 전체", false)
                        .str("title", "새 제목", false)
                        .strings("tags", "새 태그 목록(통째로 바뀐다)")
                        .str("template", "템플릿 이름", false)
                        .str("f1", F1, false)));
        t.add(tool("set_favorite",
                "노트의 즐겨찾기 별과 라벨을 고친다. favorite을 안 주면 지금 값을 뒤집는다.",
                schema().str("id", "노트 id", true)
                        .bool("favorite", "true나 false")
                        .str("category", "즐겨찾기 라벨. 빈 값이면 지운다", false)
                        .str("f1", F1, false)));
        t.add(tool("evaluate_doc",
                "노트의 평가 점수(0~100)와 메모를 새기거나 지운다.",
                schema().str("id", "노트 id", true)
                        .number("score", "점수 0~100")
                        .str("note", "평가 메모", false)
                        .bool("clear", "true면 평가를 지운다")
                        .str("f1", F1, false)));
        t.add(tool("link_docs",
                "노트에서 다른 노트로 가는 연결 1건을 잇거나(기본) 끊는다(remove).",
                schema().str("from", "출발 노트 id", true)
                        .str("to", "대상 노트 id", true)
                        .str("kind", "연결 종류: direct, indirect, attach. 기본은 direct", false)
                        .bool("remove", "true면 연결을 끊는다")
                        .str("f1", F1, false)));
        t.add(tool("list_docs",
                "조건에 맞는 노트의 목록(메타만)을 만든 시각 차례로 낸다. 본문은 get_doc으로 읽는다.",
                filters(schema())));
        t.add(tool("search_docs",
                "제목, 메타, 본문, 첨부 글에서 찾아 일치한 대목과 함께 낸다.",
                filters(schema().str("query", "찾을 말", true)
                        .integer("limit", "받을 건수. 기본 20, 최대 200"))));
        t.add(tool("list_tags",
                "태그와 그 태그가 쓰인 횟수를 낸다.",
                schema().str("query", "태그 이름의 일부", false)
                        .integer("limit", "받을 태그 수")));
        t.add(tool("get_doc",
                "노트 1건을 본문, 도장, 코멘트와 함께 읽는다. 응답의 precondition은 이 노트를 고칠 때 f1에 넣는다.",
                schema().str("id", "노트 id", true)));
        t.add(tool("delete_doc",
                "노트를 휴지통으로 보낸다. restore_doc으로 되살릴 수 있다.",
                schema().str("id", "노트 id", true)));
        t.add(tool("list_trash",
                "휴지통에 있는 노트의 목록을 낸다. 각 항목의 trashId를 restore_doc에 쓴다.",
                schema()));
        t.add(tool("restore_doc",
                "휴지통의 노트를 되살린다.",
                schema().str("trashId", "list_trash가 준 휴지통 id", true)));
        t.add(tool("purge_doc",
                "휴지통의 노트를 영구히 지운다. MCP로는 열지 않는다.",
                schema().str("trashId", "휴지통 id", true)));
        t.add(tool("plan_day",
                "하루 계획 초안을 짓는다. dryRun이 true면 노트로 저장하지 않고 초안만 낸다.",
                schema().str("date", "날짜(YYYY-MM-DD). 안 주면 오늘", false)
                        .str("agenda", "아젠다 slug나 이름", false)
                        .bool("dryRun", "true면 저장하지 않는다")));
        t.add(tool("list_reports",
                "회고(아침, 저녁, 주간)의 목록을 낸다.",
                schema().str("kind", "morning, evening, weekly 중 하나", false)
                        .str("dateFrom", "이 날짜(YYYY-MM-DD)부터", false)
                        .str("dateTo", "이 날짜(YYYY-MM-DD)까지", false)));
        t.add(tool("get_report",
                "회고 1건을 읽는다.",
                schema().str("id", "회고 id. 예: rv-evening-2026-06-04", true)));
        t.add(tool("list_templates",
                "문서 템플릿과 리추얼 템플릿의 목록을 낸다.",
                schema().str("category", "doc이나 ritual", false)
                        .str("type", "노트 유형. 주면 리추얼은 빠진다", false)));
        t.add(tool("get_template",
                "템플릿 1건을 읽는다.",
                schema().str("id", "템플릿 id", true)));
        t.add(tool("save_report",
                "회고를 저장한다. 같은 종류와 날짜가 있으면 덮어쓴다.",
                schema().str("kind", "morning, evening, weekly 중 하나", true)
                        .str("date", "날짜(YYYY-MM-DD)", true)
                        .str("body", "다듬은 본문", false)
                        .str("memo", "개인 메모", false)));
        t.add(tool("save_template",
                "문서 템플릿을 저장한다. id를 주면 그 템플릿을 고친다.",
                schema().str("id", "고칠 템플릿 id", false)
                        .str("name", "이름", false)
                        .str("type", "노트 유형", false)
                        .str("desc", "설명", false)
                        .str("body", "본문", false)));
        t.add(tool("create_sub_note",
                "노트 아래에 하위 노트 1건을 만든다.",
                schema().str("parent", "부모 노트 id", true)
                        .str("title", "제목", true)
                        .str("body", "본문", true)
                        .str("type", "하위 노트 유형", false)));
        t.add(tool("list_sub_notes",
                "노트 바로 아래의 하위 노트 목록(메타만)을 낸다.",
                schema().str("parent", "부모 노트 id", true)));
        t.add(tool("get_sub_note",
                "하위 노트 1건을 읽는다.",
                schema().str("parent", "부모 노트 id", true)
                        .str("id", "하위 노트 id", true)));
        t.add(tool("update_sub_note",
                "하위 노트의 제목과 본문을 고친다. 준 칸만 바뀐다.",
                schema().str("parent", "부모 노트 id", true)
                        .str("id", "하위 노트 id", true)
                        .str("body", "새 본문 전체", false)
                        .str("title", "새 제목", false)
                        .str("f1", F1, false)));
        t.add(tool("delete_sub_note",
                "하위 노트를 지운다.",
                schema().str("parent", "부모 노트 id", true)
                        .str("id", "하위 노트 id", true)));
        t.add(tool("write_attachment",
                "노트나 하위 노트에 글 첨부 1개를 쓴다.",
                schema().str("parent", "노트 id, 또는 하위 노트면 `노트id/하위노트id`", true)
                        .str("name", "파일명.확장자", true)
                        .str("content", "내용", true)));
        t.add(tool("list_attachments",
                "첨부 목록과 본문에 붙일 참조 조각을 낸다.",
                schema().str("parent", "노트 id, 또는 하위 노트면 `노트id/하위노트id`", true)));
        t.add(tool("list_comments",
                "코멘트 목록을 낸다.",
                schema().str("parent", "노트 id, 또는 하위 노트면 `노트id/하위노트id`", true)));
        t.add(tool("export_markdown",
                "노트를 마크다운 사본으로 내보낸다.",
                schema()));

        Map<String, Tool> byName = new LinkedHashMap<>();
        for (Tool tool : t) {
            byName.put(tool.name(), tool);
        }
        BY_NAME = Collections.unmodifiableMap(byName);
        ALL = Collections.unmodifiableList(t);
    }
}
