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
        } else if ("list_attachments".equals(toolName) || "list_comments".equals(toolName)) {
            validateKeys(toolName, props.keySet(), List.of("parent"));
            if (!props.containsKey("parent")) {
                throw new WlException(400, "인자 검증 실패(" + toolName + "): parent 가 필요하다.");
            }
            JsonValue parentVal = props.get("parent");
            if (!(parentVal instanceof JsonValue.JsonString strParent)) {
                throw new WlException(400, "인자 검증 실패(" + toolName + "): parent 는 문자열 여야 한다: " + JsonCompact.compact(parentVal));
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
                String shape = "list_comments".equals(toolName) ? "'노트id' 이거나 '노트id/하위노트id'" : "`노트id` 이거나 `노트id/하위노트id`";
                throw new WlException(400, "인자 검증 실패(" + toolName + "): parent 는 " + shape + " 여야 한다: " + rawParent);
            }
            return new ParsedArgs(true, toolName.replace('_', '-'), segments, Collections.emptyMap(), rawParent);
        } else if ("write_attachment".equals(toolName)) {
            // 18.8의 1번: 칸을 parent, name, content 차례로 1개씩 끝까지 본다. parent는 그 자리에서 자른다
            validateKeys(toolName, props.keySet(), List.of("parent", "name", "content"));
            if (!props.containsKey("parent")) {
                throw new WlException(400, "인자 검증 실패(write_attachment): parent 가 필요하다.");
            }
            JsonValue parentVal = props.get("parent");
            if (!(parentVal instanceof JsonValue.JsonString strParent)) {
                throw new WlException(400, "인자 검증 실패(write_attachment): parent 는 문자열 여야 한다: " + JsonCompact.compact(parentVal));
            }
            String rawParent = strParent.value();
            List<String> segments = new ArrayList<>();
            for (String s : rawParent.split("/")) {
                if (!Trim.isBlank(s)) {
                    segments.add(s);
                }
            }
            if (segments.isEmpty() || segments.size() >= 3) {
                throw new WlException(400, "인자 검증 실패(write_attachment): parent 는 `노트id` 이거나 `노트id/하위노트id` 여야 한다: " + rawParent);
            }
            Map<String, Object> toolOpts = new LinkedHashMap<>();
            for (String key : List.of("name", "content")) {
                if (!props.containsKey(key)) {
                    throw new WlException(400, "인자 검증 실패(write_attachment): " + key + " 가 필요하다.");
                }
                JsonValue v = props.get(key);
                if (!(v instanceof JsonValue.JsonString sv)) {
                    throw new WlException(400, "인자 검증 실패(write_attachment): " + key + " 는 문자열 여야 한다: " + JsonCompact.compact(v));
                }
                if ("content".equals(key) && sv.value().length() > 200000) {
                    throw new WlException(400, "인자 검증 실패(write_attachment): content 가 너무 길다(최대 200000자): " + sv.value().length() + "자");
                }
                toolOpts.put(key, sv.value());
            }
            return new ParsedArgs(true, "write-attachment", segments, Collections.emptyMap(), rawParent, toolOpts);
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
        } else if ("create_doc".equals(toolName)) {
            List<String> allowed = List.of("agenda", "type", "title", "body", "date", "time", "format", "links", "tags", "template");
            validateKeys(toolName, props.keySet(), allowed);

            Map<String, Object> toolOpts = new LinkedHashMap<>();

            // 1. agenda
            if (!props.containsKey("agenda")) {
                throw new WlException(400, "인자 검증 실패(create_doc): agenda 가 필요하다.");
            }
            JsonValue agendaVal = props.get("agenda");
            if (!(agendaVal instanceof JsonValue.JsonString sAgenda)) {
                throw new WlException(400, "인자 검증 실패(create_doc): agenda 는 문자열 여야 한다: " + JsonCompact.compact(agendaVal));
            }
            toolOpts.put("agenda", sAgenda.value());

            // 2. type
            if (!props.containsKey("type")) {
                throw new WlException(400, "인자 검증 실패(create_doc): type 가 필요하다.");
            }
            JsonValue typeVal = props.get("type");
            if (!(typeVal instanceof JsonValue.JsonString sType)) {
                throw new WlException(400, "인자 검증 실패(create_doc): type 는 문자열 여야 한다: " + JsonCompact.compact(typeVal));
            }
            toolOpts.put("type", sType.value());

            // 3. title
            if (!props.containsKey("title")) {
                throw new WlException(400, "인자 검증 실패(create_doc): title 가 필요하다.");
            }
            JsonValue titleVal = props.get("title");
            if (!(titleVal instanceof JsonValue.JsonString sTitle)) {
                throw new WlException(400, "인자 검증 실패(create_doc): title 는 문자열 여야 한다: " + JsonCompact.compact(titleVal));
            }
            toolOpts.put("title", sTitle.value());

            // 4. body
            if (!props.containsKey("body")) {
                throw new WlException(400, "인자 검증 실패(create_doc): body 가 필요하다.");
            }
            JsonValue bodyVal = props.get("body");
            if (!(bodyVal instanceof JsonValue.JsonString sBody)) {
                throw new WlException(400, "인자 검증 실패(create_doc): body 는 문자열 여야 한다: " + JsonCompact.compact(bodyVal));
            }
            if (sBody.value().length() > 200_000) {
                throw new WlException(400, "인자 검증 실패(create_doc): body 가 너무 길다(최대 200000자): " + sBody.value().length() + "자");
            }
            toolOpts.put("body", sBody.value());

            // 5. date
            if (props.containsKey("date")) {
                JsonValue dateVal = props.get("date");
                if (!(dateVal instanceof JsonValue.JsonString sDate)) {
                    throw new WlException(400, "인자 검증 실패(create_doc): date 는 문자열 여야 한다: " + JsonCompact.compact(dateVal));
                }
                toolOpts.put("date", sDate.value());
            }

            // 6. time
            if (props.containsKey("time")) {
                JsonValue timeVal = props.get("time");
                if (!(timeVal instanceof JsonValue.JsonString sTime)) {
                    throw new WlException(400, "인자 검증 실패(create_doc): time 는 문자열 여야 한다: " + JsonCompact.compact(timeVal));
                }
                toolOpts.put("time", sTime.value());
            }

            // 7. format
            if (props.containsKey("format")) {
                JsonValue formatVal = props.get("format");
                if (!(formatVal instanceof JsonValue.JsonString sFormat)) {
                    throw new WlException(400, "인자 검증 실패(create_doc): format 는 문자열 여야 한다: " + JsonCompact.compact(formatVal));
                }
                toolOpts.put("format", sFormat.value());
            }

            // 8. links
            if (props.containsKey("links")) {
                JsonValue linksVal = props.get("links");
                if (!(linksVal instanceof JsonValue.JsonArray arr)) {
                    throw new WlException(400, "인자 검증 실패(create_doc): links 는 객체 배열 여야 한다: " + JsonCompact.compact(linksVal));
                }
                for (JsonValue el : arr.elements()) {
                    if (!(el instanceof JsonValue.JsonObject)) {
                        throw new WlException(400, "인자 검증 실패(create_doc): links 는 객체 배열 여야 한다: " + JsonCompact.compact(linksVal));
                    }
                }
                if (arr.elements().size() > 200) {
                    throw new WlException(400, "인자 검증 실패(create_doc): links 항목이 너무 많다(최대 200개): " + arr.elements().size() + "개");
                }
                List<Object> rawLinks = new ArrayList<>();
                for (JsonValue el : arr.elements()) {
                    JsonValue.JsonObject obj = (JsonValue.JsonObject) el;
                    Map<String, Object> map = new LinkedHashMap<>();
                    for (Map.Entry<String, JsonValue> entry : obj.properties().entrySet()) {
                        if (entry.getValue() instanceof JsonValue.JsonString str) {
                            map.put(entry.getKey(), str.value());
                        } else {
                            map.put(entry.getKey(), entry.getValue());
                        }
                    }
                    rawLinks.add(map);
                }
                toolOpts.put("links", rawLinks);
            }

            // 9. tags
            if (props.containsKey("tags")) {
                JsonValue tagsVal = props.get("tags");
                if (!(tagsVal instanceof JsonValue.JsonArray arr)) {
                    throw new WlException(400, "인자 검증 실패(create_doc): tags 는 문자열 배열 여야 한다: " + JsonCompact.compact(tagsVal));
                }
                for (JsonValue el : arr.elements()) {
                    if (!(el instanceof JsonValue.JsonString)) {
                        throw new WlException(400, "인자 검증 실패(create_doc): tags 는 문자열 배열 여야 한다: " + JsonCompact.compact(tagsVal));
                    }
                }
                if (arr.elements().size() > 50) {
                    throw new WlException(400, "인자 검증 실패(create_doc): tags 항목이 너무 많다(최대 50개): " + arr.elements().size() + "개");
                }
                List<String> rawTags = new ArrayList<>();
                for (JsonValue el : arr.elements()) {
                    rawTags.add(((JsonValue.JsonString) el).value());
                }
                toolOpts.put("tags", rawTags);
            }

            // 10. template
            if (props.containsKey("template")) {
                JsonValue templateVal = props.get("template");
                if (!(templateVal instanceof JsonValue.JsonString sTemplate)) {
                    throw new WlException(400, "인자 검증 실패(create_doc): template 는 문자열 여야 한다: " + JsonCompact.compact(templateVal));
                }
                if (sTemplate.value().length() > 200) {
                    throw new WlException(400, "인자 검증 실패(create_doc): template 가 너무 길다(최대 200자): " + sTemplate.value().length() + "자");
                }
                toolOpts.put("template", sTemplate.value());
            }

            return new ParsedArgs(true, "create-doc", Collections.emptyList(), Collections.emptyMap(), null, toolOpts);
        } else if ("update_doc".equals(toolName)) {
            // 14.3의 2번: 칸은 id, body, title, tags, template, f1 차례로 본다
            List<String> allowed = List.of("id", "body", "title", "tags", "template", "f1");
            validateKeys(toolName, props.keySet(), allowed);

            if (!props.containsKey("id")) {
                throw new WlException(400, "인자 검증 실패(update_doc): id 가 필요하다.");
            }
            JsonValue idVal = props.get("id");
            if (!(idVal instanceof JsonValue.JsonString sId)) {
                throw new WlException(400, "인자 검증 실패(update_doc): id 는 문자열 여야 한다: " + JsonCompact.compact(idVal));
            }
            Map<String, Object> toolOpts = new LinkedHashMap<>();
            toolOpts.put("id", sId.value());

            if (props.containsKey("body")) {
                JsonValue bodyVal = props.get("body");
                if (!(bodyVal instanceof JsonValue.JsonString sBody)) {
                    throw new WlException(400, "인자 검증 실패(update_doc): body 는 문자열 여야 한다: " + JsonCompact.compact(bodyVal));
                }
                if (sBody.value().length() > 200000) {
                    throw new WlException(400, "인자 검증 실패(update_doc): body 가 너무 길다(최대 200000자): " + sBody.value().length() + "자");
                }
                toolOpts.put("body", sBody.value());
            }
            if (props.containsKey("title")) {
                JsonValue titleVal = props.get("title");
                if (!(titleVal instanceof JsonValue.JsonString sTitle)) {
                    throw new WlException(400, "인자 검증 실패(update_doc): title 는 문자열 여야 한다: " + JsonCompact.compact(titleVal));
                }
                toolOpts.put("title", sTitle.value());
            }
            if (props.containsKey("tags")) {
                JsonValue tagsVal = props.get("tags");
                if (!(tagsVal instanceof JsonValue.JsonArray arr)) {
                    throw new WlException(400, "인자 검증 실패(update_doc): tags 는 문자열 배열 여야 한다: " + JsonCompact.compact(tagsVal));
                }
                for (JsonValue el : arr.elements()) {
                    if (!(el instanceof JsonValue.JsonString)) {
                        throw new WlException(400, "인자 검증 실패(update_doc): tags 는 문자열 배열 여야 한다: " + JsonCompact.compact(tagsVal));
                    }
                }
                if (arr.elements().size() > 50) {
                    throw new WlException(400, "인자 검증 실패(update_doc): tags 항목이 너무 많다(최대 50개): " + arr.elements().size() + "개");
                }
                List<String> rawTags = new ArrayList<>();
                for (JsonValue el : arr.elements()) {
                    rawTags.add(((JsonValue.JsonString) el).value());
                }
                toolOpts.put("tags", rawTags);
            }
            if (props.containsKey("template")) {
                JsonValue templateVal = props.get("template");
                if (!(templateVal instanceof JsonValue.JsonString sTemplate)) {
                    throw new WlException(400, "인자 검증 실패(update_doc): template 는 문자열 여야 한다: " + JsonCompact.compact(templateVal));
                }
                if (sTemplate.value().length() > 200) {
                    throw new WlException(400, "인자 검증 실패(update_doc): template 가 너무 길다(최대 200자): " + sTemplate.value().length() + "자");
                }
                toolOpts.put("template", sTemplate.value());
            }
            if (props.containsKey("f1")) {
                JsonValue f1Val = props.get("f1");
                if (!(f1Val instanceof JsonValue.JsonString sF1)) {
                    throw new WlException(400, "인자 검증 실패(update_doc): f1 는 문자열 여야 한다: " + JsonCompact.compact(f1Val));
                }
                toolOpts.put("f1", sF1.value());
            }
            return new ParsedArgs(true, "update-doc", List.of(sId.value()), Collections.emptyMap(), null, toolOpts);
        } else if ("set_favorite".equals(toolName)) {
            List<String> allowed = List.of("id", "favorite", "category", "f1");
            validateKeys(toolName, props.keySet(), allowed);

            if (!props.containsKey("id")) {
                throw new WlException(400, "인자 검증 실패(set_favorite): id 가 필요하다.");
            }
            JsonValue idVal = props.get("id");
            if (!(idVal instanceof JsonValue.JsonString sId)) {
                throw new WlException(400, "인자 검증 실패(set_favorite): id 는 문자열 여야 한다: " + JsonCompact.compact(idVal));
            }

            Map<String, Object> toolOpts = new LinkedHashMap<>();
            toolOpts.put("id", sId.value());

            if (props.containsKey("favorite")) {
                JsonValue favVal = props.get("favorite");
                if (favVal instanceof JsonValue.JsonLiteral lit && ("true".equals(lit.raw()) || "false".equals(lit.raw()))) {
                    toolOpts.put("favorite", Boolean.valueOf(lit.raw()));
                } else {
                    throw new WlException(400, "인자 검증 실패(set_favorite): favorite 는 true 나 false 여야 한다: " + JsonCompact.compact(favVal));
                }
            }

            if (props.containsKey("category")) {
                JsonValue catVal = props.get("category");
                if (!(catVal instanceof JsonValue.JsonString sCat)) {
                    throw new WlException(400, "인자 검증 실패(set_favorite): category 는 문자열 여야 한다: " + JsonCompact.compact(catVal));
                }
                toolOpts.put("category", sCat.value());
            }

            if (props.containsKey("f1")) {
                JsonValue f1Val = props.get("f1");
                if (!(f1Val instanceof JsonValue.JsonString sF1)) {
                    throw new WlException(400, "인자 검증 실패(set_favorite): f1 는 문자열 여야 한다: " + JsonCompact.compact(f1Val));
                }
                toolOpts.put("f1", sF1.value());
            }

            return new ParsedArgs(true, "set-favorite", List.of(sId.value()), Collections.emptyMap(), null, toolOpts);
        } else if ("evaluate_doc".equals(toolName)) {
            List<String> allowed = List.of("id", "score", "note", "clear", "f1");
            validateKeys(toolName, props.keySet(), allowed);

            if (!props.containsKey("id")) {
                throw new WlException(400, "인자 검증 실패(evaluate_doc): id 가 필요하다.");
            }
            JsonValue idVal = props.get("id");
            if (!(idVal instanceof JsonValue.JsonString sId)) {
                throw new WlException(400, "인자 검증 실패(evaluate_doc): id 는 문자열 여야 한다: " + JsonCompact.compact(idVal));
            }

            Map<String, Object> toolOpts = new LinkedHashMap<>();
            toolOpts.put("id", sId.value());

            if (props.containsKey("score")) {
                JsonValue scoreVal = props.get("score");
                if (scoreVal instanceof JsonValue.JsonLiteral lit &&
                        !"true".equals(lit.raw()) && !"false".equals(lit.raw()) && !"null".equals(lit.raw())) {
                    try {
                        double d = Double.parseDouble(lit.raw());
                        toolOpts.put("score", d);
                    } catch (NumberFormatException e) {
                        throw new WlException(400, "인자 검증 실패(evaluate_doc): score 는 숫자 여야 한다: " + JsonCompact.compact(scoreVal));
                    }
                } else {
                    throw new WlException(400, "인자 검증 실패(evaluate_doc): score 는 숫자 여야 한다: " + JsonCompact.compact(scoreVal));
                }
            }

            if (props.containsKey("note")) {
                JsonValue noteVal = props.get("note");
                if (!(noteVal instanceof JsonValue.JsonString sNote)) {
                    throw new WlException(400, "인자 검증 실패(evaluate_doc): note 는 문자열 여야 한다: " + JsonCompact.compact(noteVal));
                }
                toolOpts.put("note", sNote.value());
            }

            if (props.containsKey("clear")) {
                JsonValue clearVal = props.get("clear");
                if (clearVal instanceof JsonValue.JsonLiteral lit && ("true".equals(lit.raw()) || "false".equals(lit.raw()))) {
                    toolOpts.put("clear", Boolean.valueOf(lit.raw()));
                } else {
                    throw new WlException(400, "인자 검증 실패(evaluate_doc): clear 는 true 나 false 여야 한다: " + JsonCompact.compact(clearVal));
                }
            }

            if (props.containsKey("f1")) {
                JsonValue f1Val = props.get("f1");
                if (!(f1Val instanceof JsonValue.JsonString sF1)) {
                    throw new WlException(400, "인자 검증 실패(evaluate_doc): f1 는 문자열 여야 한다: " + JsonCompact.compact(f1Val));
                }
                toolOpts.put("f1", sF1.value());
            }

            return new ParsedArgs(true, "evaluate-doc", List.of(sId.value()), Collections.emptyMap(), null, toolOpts);
        } else if ("link_docs".equals(toolName)) {
            List<String> allowed = List.of("from", "to", "kind", "remove", "f1");
            validateKeys(toolName, props.keySet(), allowed);

            if (!props.containsKey("from")) {
                throw new WlException(400, "인자 검증 실패(link_docs): from 가 필요하다.");
            }
            JsonValue fromVal = props.get("from");
            if (!(fromVal instanceof JsonValue.JsonString sFrom)) {
                throw new WlException(400, "인자 검증 실패(link_docs): from 는 문자열 여야 한다: " + JsonCompact.compact(fromVal));
            }

            if (!props.containsKey("to")) {
                throw new WlException(400, "인자 검증 실패(link_docs): to 가 필요하다.");
            }
            JsonValue toVal = props.get("to");
            if (!(toVal instanceof JsonValue.JsonString sTo)) {
                throw new WlException(400, "인자 검증 실패(link_docs): to 는 문자열 여야 한다: " + JsonCompact.compact(toVal));
            }

            Map<String, Object> toolOpts = new LinkedHashMap<>();
            toolOpts.put("from", sFrom.value());
            toolOpts.put("to", sTo.value());

            if (props.containsKey("kind")) {
                JsonValue kindVal = props.get("kind");
                if (!(kindVal instanceof JsonValue.JsonString sKind)) {
                    throw new WlException(400, "인자 검증 실패(link_docs): kind 는 문자열 여야 한다: " + JsonCompact.compact(kindVal));
                }
                toolOpts.put("kind", sKind.value());
            }

            if (props.containsKey("remove")) {
                JsonValue removeVal = props.get("remove");
                if (removeVal instanceof JsonValue.JsonLiteral lit && ("true".equals(lit.raw()) || "false".equals(lit.raw()))) {
                    toolOpts.put("remove", Boolean.valueOf(lit.raw()));
                } else {
                    throw new WlException(400, "인자 검증 실패(link_docs): remove 는 true 나 false 여야 한다: " + JsonCompact.compact(removeVal));
                }
            }

            if (props.containsKey("f1")) {
                JsonValue f1Val = props.get("f1");
                if (!(f1Val instanceof JsonValue.JsonString sF1)) {
                    throw new WlException(400, "인자 검증 실패(link_docs): f1 는 문자열 여야 한다: " + JsonCompact.compact(f1Val));
                }
                toolOpts.put("f1", sF1.value());
            }

            return new ParsedArgs(true, "link-docs", List.of(sFrom.value(), sTo.value()), Collections.emptyMap(), null, toolOpts);
        } else if ("create_sub_note".equals(toolName)) {
            // 17.3: 칸은 parent, title, body, type 차례로 본다. parent는 사슬째 자리 0으로 간다(B.6)
            validateKeys(toolName, props.keySet(), List.of("parent", "title", "body", "type"));
            String parent = requiredString(toolName, props, "parent");
            Map<String, String> named = new LinkedHashMap<>();
            named.put("title", requiredString(toolName, props, "title"));
            named.put("body", withinBodyLimit(toolName, requiredString(toolName, props, "body")));
            if (props.containsKey("type")) {
                named.put("type", optionalString(toolName, props, "type"));
            }
            return new ParsedArgs(true, "create-sub-note", List.of(parent), named, parent);
        } else if ("update_sub_note".equals(toolName)) {
            // 17.3: 칸은 parent, id, body, title, f1 차례로 본다. tags 칸은 없다(17.9의 11번)
            validateKeys(toolName, props.keySet(), List.of("parent", "id", "body", "title", "f1"));
            String parent = requiredString(toolName, props, "parent");
            String id = requiredString(toolName, props, "id");
            Map<String, String> named = new LinkedHashMap<>();
            if (props.containsKey("body")) {
                named.put("body", withinBodyLimit(toolName, optionalString(toolName, props, "body")));
            }
            for (String key : List.of("title", "f1")) {
                if (props.containsKey(key)) {
                    named.put(key, optionalString(toolName, props, key));
                }
            }
            return new ParsedArgs(true, "update-sub-note", List.of(parent, id), named, parent);
        } else if ("save_template".equals(toolName)) {
            List<String> keys = List.of("id", "name", "type", "desc", "body");
            validateKeys(toolName, props.keySet(), keys);
            Map<String, String> named = new LinkedHashMap<>();
            for (String key : keys) {
                if (!props.containsKey(key)) continue;
                String value = optionalString(toolName, props, key);
                named.put(key, key.equals("body") ? withinBodyLimit(toolName, value) : value);
            }
            return new ParsedArgs(true, "save-template", List.of(), named, null);
        } else if ("save_report".equals(toolName)) {
            validateKeys(toolName, props.keySet(), List.of("kind", "date", "body", "memo"));
            Map<String, String> named = new LinkedHashMap<>();
            for (String key : List.of("kind", "date", "body", "memo")) {
                if (key.equals("kind") || key.equals("date")) named.put(key, requiredString(toolName, props, key));
                else if (props.containsKey(key)) {
                    String value = optionalString(toolName, props, key);
                    if (value.length() > 200_000) throw new WlException(400, "인자 검증 실패(" + toolName + "): " + key + " 가 너무 길다(최대 200000자): " + value.length() + "자");
                    named.put(key, value);
                }
            }
            return new ParsedArgs(true, "save-report", List.of(), named, null);
        } else if (Set.of("delete_doc", "restore_doc", "purge_doc").contains(toolName)) {
            String key = toolName.equals("delete_doc") ? "id" : "trashId";
            validateKeys(toolName, props.keySet(), List.of(key));
            return new ParsedArgs(true, toolName.replace('_', '-'), List.of(requiredString(toolName, props, key)), Map.of(), null);
        } else if ("export_markdown".equals(toolName)) {
            validateKeys(toolName, props.keySet(), List.of());
            return new ParsedArgs(true, "export-markdown", List.of(), Map.of(), null);
        } else if ("plan_day".equals(toolName)) {
            validateKeys(toolName, props.keySet(), List.of("date", "agenda", "dryRun"));
            Map<String, String> named = new LinkedHashMap<>();
            for (String key : List.of("date", "agenda", "dryRun")) {
                if (!props.containsKey(key)) continue;
                if (key.equals("dryRun")) {
                    JsonValue v = props.get(key);
                    if (!(v instanceof JsonValue.JsonLiteral l) || !(l.raw().equals("true") || l.raw().equals("false")))
                        throw new WlException(400, "인자 검증 실패(plan_day): dryRun 는 true 나 false 여야 한다: " + JsonCompact.compact(v));
                    named.put(key, ((JsonValue.JsonLiteral) v).raw());
                } else named.put(key, optionalString(toolName, props, key));
            }
            return new ParsedArgs(true, "plan-day", List.of(), named, null);
        } else if ("delete_sub_note".equals(toolName)) {
            validateKeys(toolName, props.keySet(), List.of("parent", "id"));
            String parent = requiredString(toolName, props, "parent");
            String id = requiredString(toolName, props, "id");
            return new ParsedArgs(true, "delete-sub-note", List.of(parent, id), Collections.emptyMap(), parent);
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

    /** 필수 문자열 칸입니다. 없으면 `<키> 가 필요하다.`, 문자열이 아니면(JSON null 포함) 갈래 실패입니다(5.4의 6번). */
    private static String requiredString(String toolName, Map<String, JsonValue> props, String key) {
        if (!props.containsKey(key)) {
            throw new WlException(400, "인자 검증 실패(" + toolName + "): " + key + " 가 필요하다.");
        }
        return optionalString(toolName, props, key);
    }

    /** 있는 문자열 칸의 값입니다. 탈출을 푼 글자 그대로이고 안 다듬습니다. */
    private static String optionalString(String toolName, Map<String, JsonValue> props, String key) {
        JsonValue v = props.get(key);
        if (!(v instanceof JsonValue.JsonString s)) {
            throw new WlException(400, "인자 검증 실패(" + toolName + "): " + key + " 는 문자열 여야 한다: " + JsonCompact.compact(v));
        }
        return s.value();
    }

    /** 본문 칸의 글자 수 상한 200,000(B.8)을 UTF-16 코드 단위로 봅니다. */
    private static String withinBodyLimit(String toolName, String body) {
        if (body.length() > 200_000) {
            throw new WlException(400, "인자 검증 실패(" + toolName + "): body 가 너무 길다(최대 200000자): " + body.length() + "자");
        }
        return body;
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
            case "list-comments" -> {
                maxPositional = 2;
                usage = withlog.cmd.ListComments.USAGE;
            }
            case "create-comment" -> {
                allowedOptions = List.of("anchor_quote", "body", "comment_id");
                maxPositional = 2;
                usage = withlog.cmd.CommentCommands.CREATE_USAGE;
            }
            case "update-comment" -> {
                allowedOptions = List.of("body", "comment_id");
                maxPositional = 2;
                usage = withlog.cmd.CommentCommands.UPDATE_USAGE;
            }
            case "delete-comment" -> {
                allowedOptions = List.of("comment_id");
                maxPositional = 2;
                usage = withlog.cmd.CommentCommands.DELETE_USAGE;
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
            case "save-template" -> {
                allowedOptions = List.of("id", "name", "type", "desc", "body"); maxPositional = 0;
                usage = withlog.cmd.TemplateCommands.SAVE_USAGE;
            }
            case "delete-doc-template" -> {
                maxPositional = 1; usage = withlog.cmd.TemplateCommands.DELETE_USAGE;
            }
            case "save-ritual-template" -> {
                allowedOptions = List.of("name", "desc", "body"); maxPositional = 1;
                usage = withlog.cmd.TemplateCommands.RITUAL_USAGE;
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
            case "create-doc" -> {
                allowedOptions = List.of(
                        "agenda", "body", "date", "format", "links", "mins", "tags", "template", "tier", "time", "title", "type"
                );
                maxPositional = 0;
                usage = withlog.cmd.CreateDoc.USAGE;
            }
            case "write-attachment" -> {
                allowedOptions = List.of("content", "name");
                maxPositional = 2;
                usage = withlog.cmd.WriteAttachment.USAGE;
            }
            case "update-doc" -> {
                allowedOptions = List.of("body", "date", "f1", "mins", "tags", "template", "time", "title");
                maxPositional = 1;
                usage = withlog.cmd.UpdateDoc.USAGE;
            }
            case "set-favorite" -> {
                allowedOptions = List.of("category", "f1", "favorite");
                maxPositional = 1;
                usage = withlog.cmd.SetFavorite.USAGE;
            }
            case "evaluate-doc" -> {
                allowedOptions = List.of("clear", "f1", "note", "score");
                maxPositional = 1;
                usage = withlog.cmd.EvaluateDoc.USAGE;
            }
            case "link-docs" -> {
                allowedOptions = List.of("f1", "kind", "remove");
                maxPositional = 2;
                usage = withlog.cmd.LinkDocs.USAGE;
            }
            case "create-sub-note" -> {
                allowedOptions = List.of("body", "title", "type");
                maxPositional = 1;
                usage = withlog.cmd.CreateSubNote.USAGE;
            }
            case "update-sub-note" -> {
                allowedOptions = List.of("body", "f1", "tags", "title");
                maxPositional = 2;
                usage = withlog.cmd.UpdateSubNote.USAGE;
            }
            case "delete-sub-note" -> {
                maxPositional = 2;
                usage = withlog.cmd.DeleteSubNote.USAGE;
            }
            case "init-workspace" -> {
                allowedOptions = List.of("name", "templates");
                maxPositional = 0;
                usage = withlog.cmd.InitWorkspace.USAGE;
            }
            case "list-projects" -> {
                maxPositional = 0;
                usage = withlog.cmd.ListProjects.USAGE;
            }
            case "create-project" -> {
                allowedOptions = List.of("hue", "summary", "title");
                maxPositional = 0;
                usage = withlog.cmd.CreateProject.USAGE;
            }
            case "update-project" -> {
                allowedOptions = List.of("hue", "summary", "title");
                maxPositional = 1;
                usage = withlog.cmd.UpdateProject.USAGE;
            }
            case "delete-project" -> {
                maxPositional = 1;
                usage = withlog.cmd.DeleteProject.USAGE;
            }
            case "list-agendas" -> {
                maxPositional = 0;
                usage = withlog.cmd.ListAgendas.USAGE;
            }
            case "create-agenda" -> {
                allowedOptions = List.of("hue", "project", "repo", "summary", "title");
                maxPositional = 0;
                usage = withlog.cmd.CreateAgenda.USAGE;
            }
            case "update-agenda" -> {
                allowedOptions = List.of("hue", "project", "repo", "summary", "title");
                maxPositional = 1;
                usage = withlog.cmd.UpdateAgenda.USAGE;
            }
            case "delete-agenda" -> {
                maxPositional = 1;
                usage = withlog.cmd.DeleteAgenda.USAGE;
            }
            case "get-agenda-facts" -> {
                maxPositional = 1;
                usage = withlog.cmd.AgendaFacts.GET_USAGE;
            }
            case "update-agenda-facts" -> {
                allowedOptions = List.of("facts");
                maxPositional = 1;
                usage = withlog.cmd.AgendaFacts.UPDATE_USAGE;
            }
            case "facts-brief" -> {
                allowedOptions = List.of("from", "min-new-file", "to");
                maxPositional = 0;
                usage = withlog.cmd.FactsBrief.USAGE;
            }
            case "reindex" -> {
                maxPositional = 0;
                usage = "reindex";
            }
            case "save-report" -> {
                allowedOptions = List.of("kind", "date", "body", "memo"); maxPositional = 0;
                usage = withlog.cmd.SaveReport.USAGE;
            }
            case "delete-doc", "restore-doc", "purge-doc" -> {
                maxPositional = 1;
                usage = cmdName.equals("delete-doc") ? "delete-doc <노트 id>" : cmdName + " <휴지통 항목 id. list-trash 로 조회한다>";
            }
            case "empty-trash", "export-markdown" -> { maxPositional = 0; usage = cmdName; }
            case "shard-notes" -> { allowedOptions = List.of("apply"); maxPositional = 0; usage = "shard-notes [apply=true]"; }
            case "verify-shard" -> { allowedOptions = List.of("file"); maxPositional = 0; usage = "verify-shard [file=<매핑 파일 이름>]"; }
            case "plan-day" -> {
                allowedOptions = List.of("date", "agenda", "dryRun"); maxPositional = 0;
                usage = "plan-day [date=<YYYY-MM-DD>] [agenda=<slug 또는 이름. 주면 저장한다>] [dryRun=true]";
            }
            case "check-notes" -> { allowedOptions = List.of("date"); maxPositional = 0; usage = "check-notes [date=<YYYY-MM-DD>]"; }
            case "reindex-paths" -> {
                maxPositional = Integer.MAX_VALUE;
                usage = "reindex-paths <vault 상대 경로> [경로...]";
            }
            case "get-ask" -> {
                maxPositional = 2;
                usage = "get-ask <노트 id> [하위 노트 id]";
            }
            case "append-ask-turn" -> {
                allowedOptions = List.of(
                        "answer", "asked-at", "effort", "elapsed-ms", "error",
                        "images", "mark-id", "model", "question", "quote",
                        "session-id", "session-lines", "turn-id"
                );
                maxPositional = 2;
                usage = "append-ask-turn <노트 id> [하위 노트 id] session-id=<대화 id> turn-id=<회차 id> mark-id=<밑줄 id> asked-at=<시각> [quote=<물은 대목>] question=<질문> answer=<답> [error=<실패 까닭>] model=<모델> effort=<노력> elapsed-ms=<밀리초> [session-lines=<줄 수>] [images=<attach/이름,…>]";
            }
            case "list-chats" -> {
                maxPositional = 2;
                usage = "list-chats <노트 id> [하위 노트 id]";
            }
            case "append-chat-turn" -> {
                allowedOptions = List.of(
                        "answer", "asked-at", "chat-id", "effort", "elapsed-ms",
                        "error", "images", "model", "question", "source-session",
                        "turn-id"
                );
                maxPositional = 2;
                usage = "append-chat-turn <노트 id> [하위 노트 id] chat-id=<대화 id> source-session=<세션 id> turn-id=<회차 id> asked-at=<시각> question=<질문> answer=<답> [error=<실패 까닭>] model=<모델> effort=<노력> elapsed-ms=<밀리초> [images=<attach/이름,…>]";
            }
            case "read-session" -> {
                allowedOptions = List.of("author", "before", "limit", "note", "urls");
                maxPositional = 1;
                usage = "read-session <세션 id> [limit=<개수>] [before=<순번>] [urls=true] [note=<노트 id>] [author=<노트 id>,...]";
            }
            case "session-titles" -> {
                maxPositional = Integer.MAX_VALUE;
                usage = "session-titles <세션 id> [세션 id...]";
            }
            case "recent-sessions" -> {
                allowedOptions = List.of("days");
                maxPositional = 0;
                usage = "recent-sessions [days=<일 수>]";
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

        if (maxPositional != Integer.MAX_VALUE && positional.size() > maxPositional) {
            throw new WlException(400, cmdName + " 이 받는 자리 인자는 " + maxPositional +
                    " 개인데 " + positional.size() + " 개를 받았다: " + usage);
        }

        String rawParent = ("get-sub-note".equals(cmdName) && !positional.isEmpty()) ? positional.get(0) : null;
        return new ParsedArgs(false, cmdName, positional, options, rawParent);
    }
}
