package withlog.args;

import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.text.Utf16;

import java.util.*;

/**
 * 도구 표면 및 명령 표면 인자 파서.
 * 설명서 5.1, 5.2, 5.4, 11.3, 11.8에 따라 인자를 검증하고 파싱합니다.
 */
public final class ArgsParser {
    private ArgsParser() {}

    /**
     * 명령어 인자 목록을 도구 또는 명령 표면 규칙에 따라 파싱합니다.
     * @param toolOrCmdName 첫 번째 argv 칸 (도구명 또는 명령명)
     * @param rawArgs 명령명 뒤의 argv 칸 목록
     * @return 파싱된 ParsedArgs 객체
     * @throws WlException WL-400 문법 오류 발생 시
     */
    public static ParsedArgs parse(String toolOrCmdName, List<String> rawArgs) {
        // 도구 이름인지 판별
        boolean isTool = toolOrCmdName.contains("_");

        if (isTool) {
            return parseTool(toolOrCmdName, rawArgs);
        } else {
            return parseCommand(toolOrCmdName, rawArgs);
        }
    }

    private static ParsedArgs parseTool(String toolName, List<String> rawArgs) {
        // 1. --로 시작하는 칸 제외
        List<String> filtered = new ArrayList<>();
        for (String arg : rawArgs) {
            if (!arg.startsWith("--")) {
                filtered.add(arg);
            }
        }

        // 남은 칸이 2개 이상이면 에러
        if (filtered.size() >= 2) {
            throw new WlException(400, toolName + " 이 받는 인자는 JSON 한 덩어리인데 " + filtered.size() +
                    " 개를 받았다. 셸에서 따옴표로 감쌌는지 본다: " + toolName + " '{\"키\":\"값\"}'");
        }

        String rawJson = filtered.isEmpty() ? "{}" : filtered.get(0);
        String trimmed = Trim.trim(rawJson);
        if (trimmed.isEmpty()) {
            trimmed = "{}";
        }

        JsonValue jsonVal;
        try {
            jsonVal = JsonReader.parse(trimmed);
        } catch (Exception e) {
            throw new WlException(400, "인자 JSON 파싱 실패(" + toolName + "): " + trimmed);
        }

        if (!(jsonVal instanceof JsonValue.JsonObject jsonObj)) {
            throw new WlException(400, "인자 검증 실패(" + toolName + "): 인자는 JSON 객체여야 한다: " + trimmed);
        }

        Map<String, JsonValue> props = jsonObj.properties();

        if ("get_doc".equals(toolName)) {
            // 허용 키: id
            validateKeys(toolName, props.keySet(), List.of("id"));
            if (!props.containsKey("id")) {
                throw new WlException(400, "인자 검증 실패(get_doc): id 가 필요하다.");
            }
            JsonValue idVal = props.get("id");
            if (!(idVal instanceof JsonValue.JsonString strVal)) {
                throw new WlException(400, "인자 검증 실패(get_doc): id 는 문자열 여야 한다: " + JsonCompact.compact(idVal));
            }
            String id = strVal.value(); // 안 다듬음
            return new ParsedArgs(true, "get-doc", List.of(id), Collections.emptyMap(), null);
        } else if ("get_sub_note".equals(toolName)) {
            // 허용 키: parent, id (스키마 순서: parent, id)
            validateKeys(toolName, props.keySet(), List.of("parent", "id"));
            if (!props.containsKey("parent")) {
                throw new WlException(400, "인자 검증 실패(get_sub_note): parent 가 필요하다.");
            }
            JsonValue parentVal = props.get("parent");
            if (!(parentVal instanceof JsonValue.JsonString strParent)) {
                throw new WlException(400, "인자 검증 실패(get_sub_note): parent 는 문자열 여야 한다: " + JsonCompact.compact(parentVal));
            }
            if (!props.containsKey("id")) {
                throw new WlException(400, "인자 검증 실패(get_sub_note): id 가 필요하다.");
            }
            JsonValue idVal = props.get("id");
            if (!(idVal instanceof JsonValue.JsonString strId)) {
                throw new WlException(400, "인자 검증 실패(get_sub_note): id 는 문자열 여야 한다: " + JsonCompact.compact(idVal));
            }
            String rawParent = strParent.value();
            String rawId = strId.value();
            return new ParsedArgs(true, "get-sub-note", List.of(rawParent, rawId), Collections.emptyMap(), rawParent);
        } else if ("list_attachments".equals(toolName)) {
            validateKeys(toolName, props.keySet(), List.of("parent"));
            if (!props.containsKey("parent")) {
                throw new WlException(400, "인자 검증 실패(list_attachments): parent 가 필요하다.");
            }
            JsonValue parentVal = props.get("parent");
            if (!(parentVal instanceof JsonValue.JsonString strParent)) {
                throw new WlException(400, "인자 검증 실패(list_attachments): parent 는 문자열 여야 한다: " + JsonCompact.compact(parentVal));
            }
            String rawParent = strParent.value();
            String[] split = rawParent.split("/");
            List<String> segments = new ArrayList<>();
            for (String s : split) {
                if (!Trim.isBlank(s)) {
                    segments.add(s);
                }
            }
            if (segments.isEmpty() || segments.size() >= 3) {
                throw new WlException(400, "인자 검증 실패(list_attachments): parent 는 `노트id` 이거나 `노트id/하위노트id` 여야 한다: " + rawParent);
            }
            return new ParsedArgs(true, "list-attachments", segments, Collections.emptyMap(), rawParent);
        } else if ("list_sub_notes".equals(toolName)) {
            validateKeys(toolName, props.keySet(), List.of("parent"));
            if (!props.containsKey("parent")) {
                throw new WlException(400, "인자 검증 실패(list_sub_notes): parent 가 필요하다.");
            }
            JsonValue parentVal = props.get("parent");
            if (!(parentVal instanceof JsonValue.JsonString strParent)) {
                throw new WlException(400, "인자 검증 실패(list_sub_notes): parent 는 문자열 여야 한다: " + JsonCompact.compact(parentVal));
            }
            String rawParent = strParent.value();
            return new ParsedArgs(true, "list-sub-notes", List.of(rawParent), Collections.emptyMap(), rawParent);
        } else if ("list_docs".equals(toolName)) {
            List<String> allowed = List.of("agenda", "date", "dateFrom", "dateTo", "type", "tag", "favorite", "favoriteCategory");
            validateKeys(toolName, props.keySet(), allowed);

            Map<String, Object> toolOpts = new LinkedHashMap<>();
            // 9.3 표 순서대로 타입 검증
            for (String key : allowed) {
                if (props.containsKey(key)) {
                    JsonValue v = props.get(key);
                    if ("favorite".equals(key)) {
                        if (v instanceof JsonValue.JsonLiteral lit && ("true".equals(lit.raw()) || "false".equals(lit.raw()))) {
                            toolOpts.put("favorite", Boolean.valueOf(lit.raw()));
                        } else {
                            throw new WlException(400, "인자 검증 실패(list_docs): favorite 는 true 나 false 여야 한다: " + JsonCompact.compact(v));
                        }
                    } else {
                        if (!(v instanceof JsonValue.JsonString strVal)) {
                            throw new WlException(400, "인자 검증 실패(list_docs): " + key + " 는 문자열 여야 한다: " + JsonCompact.compact(v));
                        }
                        toolOpts.put(key, strVal.value());
                    }
                }
            }
            return new ParsedArgs(true, "list-docs", Collections.emptyList(), Collections.emptyMap(), null, toolOpts);
        } else if ("list_tags".equals(toolName)) {
            List<String> allowed = List.of("query", "limit");
            validateKeys(toolName, props.keySet(), allowed);

            Map<String, Object> toolOpts = new LinkedHashMap<>();
            if (props.containsKey("query")) {
                JsonValue qVal = props.get("query");
                if (!(qVal instanceof JsonValue.JsonString strVal)) {
                    throw new WlException(400, "인자 검증 실패(list_tags): query 는 문자열 여야 한다: " + JsonCompact.compact(qVal));
                }
                toolOpts.put("query", strVal.value());
            }
            if (props.containsKey("limit")) {
                JsonValue lVal = props.get("limit");
                toolOpts.put("limit", parseToolLimit(lVal, "list_tags"));
            }
            return new ParsedArgs(true, "list-tags", Collections.emptyList(), Collections.emptyMap(), null, toolOpts);
        } else if ("search_docs".equals(toolName)) {
            List<String> allowed = List.of("query", "limit", "agenda", "date", "dateFrom", "dateTo", "type", "tag", "favorite", "favoriteCategory");
            validateKeys(toolName, props.keySet(), allowed);

            Map<String, Object> toolOpts = new LinkedHashMap<>();
            // 1. query (필수)
            if (!props.containsKey("query")) {
                throw new WlException(400, "인자 검증 실패(search_docs): query 가 필요하다.");
            }
            JsonValue qVal = props.get("query");
            if (!(qVal instanceof JsonValue.JsonString strVal)) {
                throw new WlException(400, "인자 검증 실패(search_docs): query 는 문자열 여야 한다: " + JsonCompact.compact(qVal));
            }
            toolOpts.put("query", strVal.value());

            // 2. limit (선택)
            if (props.containsKey("limit")) {
                JsonValue lVal = props.get("limit");
                toolOpts.put("limit", parseToolLimit(lVal, "search_docs"));
            }

            // 3. 필터 8개 (9.3 차례)
            List<String> filterKeys = List.of("agenda", "date", "dateFrom", "dateTo", "type", "tag", "favorite", "favoriteCategory");
            for (String key : filterKeys) {
                if (props.containsKey(key)) {
                    JsonValue v = props.get(key);
                    if ("favorite".equals(key)) {
                        if (v instanceof JsonValue.JsonLiteral lit && ("true".equals(lit.raw()) || "false".equals(lit.raw()))) {
                            toolOpts.put("favorite", Boolean.valueOf(lit.raw()));
                        } else {
                            throw new WlException(400, "인자 검증 실패(search_docs): favorite 는 true 나 false 여야 한다: " + JsonCompact.compact(v));
                        }
                    } else {
                        if (!(v instanceof JsonValue.JsonString sVal)) {
                            throw new WlException(400, "인자 검증 실패(search_docs): " + key + " 는 문자열 여야 한다: " + JsonCompact.compact(v));
                        }
                        toolOpts.put(key, sVal.value());
                    }
                }
            }
            return new ParsedArgs(true, "search-docs", Collections.emptyList(), Collections.emptyMap(), null, toolOpts);
        } else if ("list_templates".equals(toolName)) {
            List<String> allowed = List.of("category", "type");
            validateKeys(toolName, props.keySet(), allowed);

            Map<String, Object> toolOpts = new LinkedHashMap<>();
            if (props.containsKey("category")) {
                JsonValue cVal = props.get("category");
                if (!(cVal instanceof JsonValue.JsonString sVal)) {
                    throw new WlException(400, "인자 검증 실패(list_templates): category 는 문자열 여야 한다: " + JsonCompact.compact(cVal));
                }
                toolOpts.put("category", sVal.value());
            }
            if (props.containsKey("type")) {
                JsonValue tVal = props.get("type");
                if (!(tVal instanceof JsonValue.JsonString sVal)) {
                    throw new WlException(400, "인자 검증 실패(list_templates): type 는 문자열 여야 한다: " + JsonCompact.compact(tVal));
                }
                toolOpts.put("type", sVal.value());
            }
            return new ParsedArgs(true, "list-templates", Collections.emptyList(), Collections.emptyMap(), null, toolOpts);
        } else if ("get_template".equals(toolName)) {
            List<String> allowed = List.of("id");
            validateKeys(toolName, props.keySet(), allowed);

            if (!props.containsKey("id")) {
                throw new WlException(400, "인자 검증 실패(get_template): id 가 필요하다.");
            }
            JsonValue idVal = props.get("id");
            if (!(idVal instanceof JsonValue.JsonString sVal)) {
                throw new WlException(400, "인자 검증 실패(get_template): id 는 문자열 여야 한다: " + JsonCompact.compact(idVal));
            }
            Map<String, Object> toolOpts = Map.of("id", sVal.value());
            return new ParsedArgs(true, "get-template", Collections.emptyList(), Collections.emptyMap(), null, toolOpts);
        } else if ("list_reports".equals(toolName)) {
            List<String> allowed = List.of("kind", "dateFrom", "dateTo");
            validateKeys(toolName, props.keySet(), allowed);

            Map<String, Object> toolOpts = new LinkedHashMap<>();
            if (props.containsKey("kind")) {
                JsonValue kVal = props.get("kind");
                if (!(kVal instanceof JsonValue.JsonString sVal)) {
                    throw new WlException(400, "인자 검증 실패(list_reports): kind 는 문자열 여야 한다: " + JsonCompact.compact(kVal));
                }
                toolOpts.put("kind", sVal.value());
            }
            if (props.containsKey("dateFrom")) {
                JsonValue dfVal = props.get("dateFrom");
                if (!(dfVal instanceof JsonValue.JsonString sVal)) {
                    throw new WlException(400, "인자 검증 실패(list_reports): dateFrom 는 문자열 여야 한다: " + JsonCompact.compact(dfVal));
                }
                toolOpts.put("dateFrom", sVal.value());
            }
            if (props.containsKey("dateTo")) {
                JsonValue dtVal = props.get("dateTo");
                if (!(dtVal instanceof JsonValue.JsonString sVal)) {
                    throw new WlException(400, "인자 검증 실패(list_reports): dateTo 는 문자열 여야 한다: " + JsonCompact.compact(dtVal));
                }
                toolOpts.put("dateTo", sVal.value());
            }
            return new ParsedArgs(true, "list-reports", Collections.emptyList(), Collections.emptyMap(), null, toolOpts);
        } else if ("get_report".equals(toolName)) {
            List<String> allowed = List.of("id");
            validateKeys(toolName, props.keySet(), allowed);

            if (!props.containsKey("id")) {
                throw new WlException(400, "인자 검증 실패(get_report): id 가 필요하다.");
            }
            JsonValue idVal = props.get("id");
            if (!(idVal instanceof JsonValue.JsonString sVal)) {
                throw new WlException(400, "인자 검증 실패(get_report): id 는 문자열 여야 한다: " + JsonCompact.compact(idVal));
            }
            Map<String, Object> toolOpts = Map.of("id", sVal.value());
            return new ParsedArgs(true, "get-report", Collections.emptyList(), Collections.emptyMap(), null, toolOpts);
        } else if ("list_trash".equals(toolName)) {
            List<String> allowed = Collections.emptyList();
            validateKeys(toolName, props.keySet(), allowed);
            return new ParsedArgs(true, "list-trash", Collections.emptyList(), Collections.emptyMap(), null, Collections.emptyMap());
        } else {
            throw new WlException(400, "미지원 도구: " + toolName);
        }
    }

    private static void validateKeys(String toolName, Set<String> givenKeys, List<String> allowedSchemaKeys) {
        List<String> unknown = new ArrayList<>();
        for (String k : givenKeys) {
            if (!allowedSchemaKeys.contains(k)) {
                unknown.add(k);
            }
        }
        if (!unknown.isEmpty()) {
            unknown.sort(Utf16::compare);
            String unknownJoined = String.join(", ", unknown);
            String allowedJoined = String.join(", ", allowedSchemaKeys);
            throw new WlException(400, "인자 검증 실패(" + toolName + "): 모르는 키다: [" + unknownJoined +
                    "]. 쓸 수 있는 것은 [" + allowedJoined + "] 뿐이다.");
        }
    }

    public static long parseToolLimit(JsonValue value, String toolName) {
        if (!(value instanceof JsonValue.JsonLiteral lit)) {
            throw new WlException(400, "인자 검증 실패(" + toolName + "): limit 는 1 이상의 정수 여야 한다: " + JsonCompact.compact(value));
        }
        String raw = lit.raw();
        if ("true".equals(raw) || "false".equals(raw) || "null".equals(raw)) {
            throw new WlException(400, "인자 검증 실패(" + toolName + "): limit 는 1 이상의 정수 여야 한다: " + JsonCompact.compact(value));
        }
        double x;
        try {
            x = Double.parseDouble(raw);
        } catch (NumberFormatException e) {
            throw new WlException(400, "인자 검증 실패(" + toolName + "): limit 는 1 이상의 정수 여야 한다: " + JsonCompact.compact(value));
        }
        if (Double.isNaN(x) || Double.isInfinite(x)) {
            throw new WlException(400, "인자 검증 실패(" + toolName + "): limit 는 1 이상의 정수 여야 한다: " + JsonCompact.compact(value));
        }
        long asLong = (long) x;
        if ((double) asLong != x || asLong < 1) {
            throw new WlException(400, "인자 검증 실패(" + toolName + "): limit 는 1 이상의 정수 여야 한다: " + JsonCompact.compact(value));
        }
        return asLong;
    }

    private static ParsedArgs parseCommand(String cmdName, List<String> rawArgs) {
        List<String> positional = new ArrayList<>();
        Map<String, String> options = new LinkedHashMap<>();

        // 허용 옵션 정의
        List<String> allowedOptions = Collections.emptyList(); // get-doc, get-sub-note, reindex 모두 옵션 없음
        int maxPositional;
        String usage;

        switch (cmdName) {
            case "get-doc" -> {
                maxPositional = 1;
                usage = "get-doc <노트 id>";
            }
            case "get-sub-note" -> {
                maxPositional = 2;
                usage = "get-sub-note <부모 노트 id> <하위 노트 id>";
            }
            case "list-attachments" -> {
                maxPositional = 2;
                usage = "list-attachments <노트 id> [하위 노트 id]";
            }
            case "list-sub-notes" -> {
                allowedOptions = List.of("bodies");
                maxPositional = 1;
                usage = "list-sub-notes <부모 노트 id> [bodies=true. 본문까지 싣는다. 기본은 메타만]";
            }
            case "list-docs" -> {
                allowedOptions = List.of("agenda", "bodies", "date", "favorite", "favoriteCategory", "from", "limit", "tag", "to", "type");
                maxPositional = 0;
                usage = "list-docs [agenda=<slug 또는 이름>] [type=<유형>] [tag=<태그,태그>] [favorite=true] [favoriteCategory=<라벨>] [date=<YYYY-MM-DD>] [from=<YYYY-MM-DD>] [to=<YYYY-MM-DD>] [limit=<건수. 0 이면 전부>] [bodies=true. 본문까지 싣는다. 기본은 메타만]";
            }
            case "list-tags" -> {
                allowedOptions = List.of("limit", "query");
                maxPositional = 0;
                usage = "list-tags [query=<태그 이름의 일부>] [limit=<받을 태그 수. 1 이상>]";
            }
            case "search-docs" -> {
                allowedOptions = List.of("agenda", "date", "favorite", "favoriteCategory", "from", "limit", "query", "tag", "to", "type");
                maxPositional = 1;
                usage = "search-docs <검색어> [agenda=<slug 또는 이름>] [type=<유형>] [tag=<태그,태그>] [favorite=true] [favoriteCategory=<라벨>] [date=<YYYY-MM-DD>] [from=<YYYY-MM-DD>] [to=<YYYY-MM-DD>] [limit=<건수. 기본 20, 최대 200>]";
            }
            case "list-templates" -> {
                allowedOptions = List.of("bodies", "category", "type");
                maxPositional = 0;
                usage = "list-templates [category=<갈래>] [type=<유형>] [bodies=true]";
            }
            case "get-template" -> {
                maxPositional = 1;
                usage = "get-template <템플릿 id>";
            }
            case "list-reports" -> {
                allowedOptions = List.of("bodies", "from", "kind", "to");
                maxPositional = 0;
                usage = "list-reports [kind=<종류>] [from=<YYYY-MM-DD>] [to=<YYYY-MM-DD>] [bodies=true]";
            }
            case "get-report" -> {
                maxPositional = 1;
                usage = "get-report <회고 id. 예: rv-evening-2026-06-04>";
            }
            case "list-trash" -> {
                maxPositional = 0;
                usage = "list-trash";
            }
            case "reindex" -> {
                maxPositional = 0;
                usage = "reindex";
            }
            default -> {
                maxPositional = 1;
                usage = cmdName;
            }
        }

        for (String arg : rawArgs) {
            if (arg.startsWith("--")) {
                // 5.2 1번: --로 시작하는 칸은 버린다
                continue;
            }
            int eqIdx = arg.indexOf('=');
            if (eqIdx >= 1) {
                // 2번째 글자 이후에 처음 나오는 = -> 이름 인자
                String optName = Trim.trim(arg.substring(0, eqIdx));
                String optVal = arg.substring(eqIdx + 1);
                if (!allowedOptions.contains(optName)) {
                    // 허용되지 않은 옵션 즉시 실패
                    List<String> sortedAllowed = new ArrayList<>(allowedOptions);
                    sortedAllowed.sort(Utf16::compare);
                    throw new WlException(400, cmdName + " 이 모르는 옵션이다: " + optName +
                            ". 쓸 수 있는 것은 [" + String.join(", ", sortedAllowed) + "] 뿐이다.");
                }
                options.put(optName, optVal);
            } else {
                // 자리 인자 (= 없거나 첫 글자가 =)
                positional.add(arg);
            }
        }

        if (positional.size() > maxPositional) {
            throw new WlException(400, cmdName + " 이 받는 자리 인자는 " + maxPositional +
                    " 개인데 " + positional.size() + " 개를 받았다: " + usage);
        }

        String rawParent = ("get-sub-note".equals(cmdName) && !positional.isEmpty()) ? positional.get(0) : null;
        return new ParsedArgs(false, cmdName, positional, options, rawParent);
    }
}
