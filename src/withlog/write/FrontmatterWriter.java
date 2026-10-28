package withlog.write;

import withlog.cli.WlException;

import java.util.*;
import java.util.regex.Pattern;

/**
 * 설명서 1.9 프론트매터 쓰기 규격 직렬화기.
 * 키 차례(1.7), 따옴표 규칙, 큰따옴표 탈출, 접는 블록/그대로 블록 및 1.10 B 재직렬화를 구현합니다.
 */
public final class FrontmatterWriter {

    private static final Set<String> BOOLEAN_KEYWORDS = Set.of(
            "y", "Y", "yes", "Yes", "YES", "on", "On", "ON",
            "n", "N", "no", "No", "NO", "off", "Off", "OFF"
    );

    private static final Pattern BASE60_PATTERN = Pattern.compile("^[-+]?[0-9_]+(:[0-9_]+)+(\\.[0-9_]*)?$");
    private static final Pattern INTEGER_PATTERN = Pattern.compile("^[+-]?(?:0|0b[01]+|0x[0-9a-fA-F]+|0o[0-7]+|[0-9]+)$");
    private static final Pattern FLOAT_PATTERN = Pattern.compile("^(?:[-+]?(?:[0-9]+)(?:\\.[0-9]*)?(?:[eE][-+]?[0-9]+)?|\\.[0-9]+(?:[eE][-+]?[0-9]+)?|[-+]?\\.(?:inf|Inf|INF)|\\.(?:nan|NaN|NAN))$");
    private static final Pattern DATE_PATTERN = Pattern.compile("^[0-9]{4}-[0-9]{2}-[0-9]{2}$");
    private static final Pattern TIMESTAMP_PATTERN = Pattern.compile("^[0-9]{4}-[0-9][0-9]?-[0-9][0-9]?(?:[Tt]|[ \t]+)[0-9][0-9]?:[0-9]{2}:[0-9]{2}(?:\\.[0-9]*)?(?:[ \t]*(?:Z|[-+][0-9][0-9]?(?::[0-9]{2})?))?$");

    private static final Set<String> NULL_LITERALS = Set.of("~", "null", "Null", "NULL");
    private static final Set<String> BOOLEAN_LITERALS = Set.of("true", "True", "TRUE", "false", "False", "FALSE");

    private static final String PLAIN_FORBIDDEN_START = " -?:,[]{}#&*!|=>'\"%@`";

    private FrontmatterWriter() {}

    /**
     * 설명서 1.7 키 차례(17개 키) 전체를 지원하는 프론트매터 YAML 문자열을 생성합니다.
     *
     * @param id 노트 id (1)
     * @param agenda 아젠다 slug (2)
     * @param type 정식 소문자 노트 유형 (3)
     * @param title 노트 제목 (4)
     * @param created 시각 문자열 (5)
     * @param sessionId 세션 식별자 (6, 있을 때만)
     * @param mins 소요 시간 (7)
     * @param tier 층 (8, record일 때만)
     * @param editedBy 마지막 편집자 (9, 도장 있을 때만)
     * @param editedAt 마지막 편집 시각 (10, 도장 있을 때만)
     * @param editedSessionId 마지막 편집 세션 (11, 도장 및 세션 있을 때만)
     * @param tags 태그 목록 (12, 비어있지 않을 때만)
     * @param favorite 즐겨찾기 여부 (13, 참일 때만)
     * @param favoriteCategory 즐겨찾기 라벨 (14, 있을 때만)
     * @param template 템플릿 id (15, 있을 때만)
     * @param evaluation 평가 정보 (16, 있을 때만: score, note, at)
     * @param links 연결 목록 (17, 비어있지 않을 때만)
     * @return YAML 내용 (양쪽 울타리 제외)
     */
    public static String buildFullNoteFrontmatter(
            String id,
            String agenda,
            String type,
            String title,
            String created,
            String sessionId,
            int mins,
            String tier,
            String editedBy,
            String editedAt,
            String editedSessionId,
            List<String> tags,
            boolean favorite,
            String favoriteCategory,
            String template,
            Map<String, Object> evaluation,
            List<Map<String, String>> links
    ) {
        StringBuilder sb = new StringBuilder();

        // 1. id
        appendKeyVal(sb, "id", id, 1);

        // 2. agenda
        appendKeyVal(sb, "agenda", agenda, 1);

        // 3. type
        appendKeyVal(sb, "type", type.toLowerCase(Locale.ROOT), 1);

        // 4. title
        appendKeyVal(sb, "title", title, 1);

        // 5. created
        appendKeyVal(sb, "created", created, 1);

        // 6. sessionId. 13.9의 6번: 새 노트는 빈 표식도 '' 로 적는다. 메타 쓰기는 다듬어 빈 값을 이미 null로 넘긴다
        if (sessionId != null) {
            appendKeyVal(sb, "sessionId", sessionId, 1);
        }

        // 7. mins
        sb.append("mins: ").append(mins).append("\n");

        // 8. tier
        if ("record".equalsIgnoreCase(tier)) {
            sb.append("tier: record\n");
        }

        // 9. editedBy, 10. editedAt, 11. editedSessionId
        if (editedBy != null && editedAt != null) {
            appendKeyVal(sb, "editedBy", editedBy, 1);
            appendKeyVal(sb, "editedAt", editedAt, 1);
            // 14.9의 17번: 새 도장은 빈 표식도 '' 로 적는다. 읽은 옛 값은 1.8에서 이미 빈 글이 없음이 됐다
            if (editedSessionId != null) {
                appendKeyVal(sb, "editedSessionId", editedSessionId, 1);
            }
        }

        // 12. tags
        if (tags != null && !tags.isEmpty()) {
            sb.append("tags:\n");
            for (String tag : tags) {
                sb.append("  - ");
                sb.append(dumpString(tag, 2)).append("\n");
            }
        }

        // 13. favorite
        if (favorite) {
            sb.append("favorite: true\n");
        }

        // 14. favoriteCategory
        if (favoriteCategory != null && !favoriteCategory.isEmpty()) {
            appendKeyVal(sb, "favoriteCategory", favoriteCategory, 1);
        }

        // 15. template
        if (template != null && !template.isEmpty()) {
            appendKeyVal(sb, "template", template, 1);
        }

        // 16. evaluation
        if (evaluation != null) {
            sb.append("evaluation:\n");
            Object scoreObj = evaluation.get("score");
            int score = scoreObj instanceof Number n ? n.intValue() : 0;
            sb.append("  score: ").append(score).append("\n");

            Object noteObj = evaluation.get("note");
            String note = noteObj instanceof String s ? s : "";
            sb.append("  note: ");
            String dumpedNote = dumpString(note, 2);
            if (dumpedNote.startsWith("|") || dumpedNote.startsWith(">")) {
                sb.append(dumpedNote).append("\n");
            } else {
                sb.append(dumpedNote).append("\n");
            }

            Object atObj = evaluation.get("at");
            String at = atObj instanceof String s ? s : "";
            sb.append("  at: ");
            String dumpedAt = dumpString(at, 2);
            if (dumpedAt.startsWith("|") || dumpedAt.startsWith(">")) {
                sb.append(dumpedAt).append("\n");
            } else {
                sb.append(dumpedAt).append("\n");
            }
        }

        // 17. links
        if (links != null && !links.isEmpty()) {
            sb.append("links:\n");
            for (Map<String, String> link : links) {
                String to = link.get("to");
                String kind = link.get("kind");
                sb.append("  - to: ").append(dumpString(to, 3)).append("\n");
                sb.append("    kind: ").append(dumpString(kind.toLowerCase(Locale.ROOT), 3)).append("\n");
            }
        }

        return sb.toString();
    }

    /**
     * 하위 호환성을 위한 기본 노트 생성 프론트매터 YAML 빌더.
     */
    public static String buildNoteFrontmatter(
            String id,
            String agenda,
            String type,
            String title,
            String created,
            String sessionId,
            int mins,
            String tier,
            List<String> tags,
            String template,
            List<Map<String, String>> links
    ) {
        return buildFullNoteFrontmatter(
                id, agenda, type, title, created, sessionId, mins, tier,
                null, null, null, tags, false, null, template, null, links
        );
    }

    /**
     * 아젠다 파일 생성을 위한 프론트매터 YAML을 생성합니다.
     */
    public static String buildAgendaFrontmatter(String id, String slug, String title, String hue) {
        StringBuilder sb = new StringBuilder();
        appendKeyVal(sb, "id", id, 1);
        appendKeyVal(sb, "slug", slug, 1);
        appendKeyVal(sb, "title", title, 1);
        appendKeyVal(sb, "hue", hue, 1);
        return sb.toString();
    }

    private static void appendKeyVal(StringBuilder sb, String key, String val, int depth) {
        sb.append(key).append(": ");
        String dumped = dumpString(val, depth);
        if (dumped.startsWith("|") || dumped.startsWith(">")) {
            sb.append(dumped).append("\n");
        } else {
            sb.append(dumped).append("\n");
        }
    }

    /**
     * 설명서 1.10 Form B를 위해 임의의 YAML 맵을 1.9의 규격에 맞춰 직렬화합니다.
     * 잘못된 타입(Double, BigInteger, Date 등)이나 비문자열 내부 맵 키가 있으면 WlException을 발생시킵니다.
     *
     * @param rawMap 파싱된 원본 YAML 맵
     * @return 직렬화된 YAML 문자열
     */
    public static String dumpGeneralMap(Map<?, ?> rawMap) {
        if (rawMap == null || rawMap.isEmpty()) {
            return "{}\n";
        }
        StringBuilder sb = new StringBuilder();
        for (Map.Entry<?, ?> entry : rawMap.entrySet()) {
            Object keyObj = entry.getKey();
            // 최상위 키가 문자열이 아니면 1.8에서 이미 빠져 B에도 없음 (1.10 1074행)
            if (!(keyObj instanceof String key)) {
                continue;
            }
            Object val = entry.getValue();
            checkValidValue(val);

            sb.append(dumpKey(key)).append(":");
            appendGeneralValue(sb, val, 1);
        }
        return sb.toString();
    }

    private static void checkValidValue(Object val) {
        if (val == null || val instanceof String || val instanceof Boolean ||
                val instanceof Long || val instanceof Integer || val instanceof Short || val instanceof Byte ||
                val instanceof Map || val instanceof List) {
            return;
        }
        if (val instanceof Double || val instanceof Float) {
            throw new WlException(400, "frontmatter 에 쓸 수 없는 값이다: Double (" + val + ")");
        }
        if (val instanceof java.math.BigInteger || val instanceof java.math.BigDecimal) {
            throw new WlException(400, "frontmatter 에 쓸 수 없는 값이다: BigInteger (" + val + ")");
        }
        if (val instanceof java.util.Date) {
            throw new WlException(400, "frontmatter 에 쓸 수 없는 값이다: Date (" + val + ")");
        }
        if (val instanceof byte[]) {
            throw new WlException(400, "frontmatter 에 쓸 수 없는 값이다: ByteArray (" + Arrays.toString((byte[]) val) + ")");
        }
        if (val instanceof Set) {
            throw new WlException(400, "frontmatter 에 쓸 수 없는 값이다: " + val.getClass().getSimpleName() + " (" + val + ")");
        }
        if (val.getClass().isArray()) {
            throw new WlException(400, "frontmatter 에 쓸 수 없는 값이다: Array (" + val + ")");
        }
        throw new WlException(400, "frontmatter 에 쓸 수 없는 값이다: " + val.getClass().getSimpleName() + " (" + val + ")");
    }

    /** 키의 `:` 뒤에 값을 잇습니다. 비지 않은 맵과 목록은 다음 줄부터 적고, 나머지는 공백 1개 뒤에 적습니다(1.9). */
    private static void appendGeneralValue(StringBuilder sb, Object val, int depth) {
        checkValidValue(val);
        boolean block = (val instanceof Map<?, ?> bm && !bm.isEmpty()) || (val instanceof List<?> bl && !bl.isEmpty());
        if (!block) {
            sb.append(' ');
        }
        if (val == null) {
            sb.append("null\n");
        } else if (val instanceof Boolean b) {
            sb.append(b ? "true\n" : "false\n");
        } else if (val instanceof Number) {
            sb.append(val.toString()).append("\n");
        } else if (val instanceof String s) {
            String dumped = dumpString(s, depth);
            sb.append(dumped).append("\n");
        } else if (val instanceof Map<?, ?> m) {
            if (m.isEmpty()) {
                sb.append("{}\n");
            } else {
                sb.append("\n");
                String indent = "  ".repeat(depth);
                for (Map.Entry<?, ?> entry : m.entrySet()) {
                    if (!(entry.getKey() instanceof String k)) {
                        throw new WlException(400, "frontmatter 의 키는 문자열이어야 한다: " + entry.getKey());
                    }
                    sb.append(indent).append(dumpKey(k)).append(":");
                    appendGeneralValue(sb, entry.getValue(), depth + 1);
                }
            }
        } else if (val instanceof List<?> list) {
            if (list.isEmpty()) {
                sb.append("[]\n");
            } else {
                sb.append("\n");
                String indent = "  ".repeat(depth);
                for (Object item : list) {
                    checkValidValue(item);
                    if (item instanceof Map<?, ?> mapItem) {
                        if (mapItem.isEmpty()) {
                            sb.append(indent).append("- {}\n");
                        } else {
                            boolean first = true;
                            for (Map.Entry<?, ?> entry : mapItem.entrySet()) {
                                if (!(entry.getKey() instanceof String k)) {
                                    throw new WlException(400, "frontmatter 의 키는 문자열이어야 한다: " + entry.getKey());
                                }
                                if (first) {
                                    sb.append(indent).append("- ").append(dumpKey(k)).append(":");
                                    appendGeneralValue(sb, entry.getValue(), depth + 2);
                                    first = false;
                                } else {
                                    sb.append(indent).append("  ").append(dumpKey(k)).append(":");
                                    appendGeneralValue(sb, entry.getValue(), depth + 2);
                                }
                            }
                        }
                    } else if (item instanceof List<?> listItem) {
                        sb.append(indent).append("- ");
                        appendNestedList(sb, listItem, depth + 1);
                    } else {
                        sb.append(indent).append("- ");
                        if (item == null) {
                            sb.append("null\n");
                        } else if (item instanceof Boolean b) {
                            sb.append(b ? "true\n" : "false\n");
                        } else if (item instanceof Number) {
                            sb.append(item.toString()).append("\n");
                        } else if (item instanceof String s) {
                            String dumped = dumpString(s, depth + 1);
                            sb.append(dumped).append("\n");
                        }
                    }
                }
            }
        }
    }

    private static void appendNestedList(StringBuilder sb, List<?> list, int depth) {
        if (list.isEmpty()) {
            sb.append("[]\n");
            return;
        }
        sb.append("- ");
        Object first = list.get(0);
        checkValidValue(first);
        if (first instanceof List<?> subList) {
            appendNestedList(sb, subList, depth + 1);
        } else if (first instanceof Map<?, ?> map) {
            sb.append("\n");
            String indent = "  ".repeat(depth + 1);
            for (Map.Entry<?, ?> entry : map.entrySet()) {
                if (!(entry.getKey() instanceof String k)) {
                    throw new WlException(400, "frontmatter 의 키는 문자열이어야 한다: " + entry.getKey());
                }
                sb.append(indent).append(dumpKey(k)).append(":");
                appendGeneralValue(sb, entry.getValue(), depth + 2);
            }
        } else {
            if (first == null) sb.append("null\n");
            else if (first instanceof Boolean b) sb.append(b ? "true\n" : "false\n");
            else if (first instanceof Number) sb.append(first.toString()).append("\n");
            else if (first instanceof String s) sb.append(dumpString(s, depth + 1)).append("\n");
        }

        String indent = "  ".repeat(depth);
        for (int i = 1; i < list.size(); i++) {
            Object item = list.get(i);
            checkValidValue(item);
            sb.append(indent).append("- ");
            if (item instanceof List<?> subList) {
                appendNestedList(sb, subList, depth + 1);
            } else if (item instanceof Map<?, ?> map) {
                sb.append("\n");
                String mapIndent = "  ".repeat(depth + 1);
                for (Map.Entry<?, ?> entry : map.entrySet()) {
                    if (!(entry.getKey() instanceof String k)) {
                        throw new WlException(400, "frontmatter 의 키는 문자열이어야 한다: " + entry.getKey());
                    }
                    sb.append(mapIndent).append(dumpKey(k)).append(":");
                    appendGeneralValue(sb, entry.getValue(), depth + 2);
                }
            } else {
                if (item == null) sb.append("null\n");
                else if (item instanceof Boolean b) sb.append(b ? "true\n" : "false\n");
                else if (item instanceof Number) sb.append(item.toString()).append("\n");
                else if (item instanceof String s) sb.append(dumpString(s, depth + 1)).append("\n");
            }
        }
    }

    /**
     * 설명서 1.9에 따라 YAML 맵 키 문자열을 직렬화합니다.
     */
    public static String dumpKey(String key) {
        if (key == null || key.isEmpty()) {
            return "''";
        }
        if (BOOLEAN_KEYWORDS.contains(key) || BASE60_PATTERN.matcher(key).matches()) {
            return "'" + key + "'";
        }
        if (hasNonPrintableCharactersForKey(key)) {
            return dumpDoubleQuoted(key);
        }
        if (isPlainSafe(key) && !isOtherType(key)) {
            return key;
        }
        return "'" + key.replace("'", "''") + "'";
    }

    private static boolean hasNonPrintableCharactersForKey(String s) {
        int len = s.length();
        for (int i = 0; i < len; ) {
            int cp = s.codePointAt(i);
            int charCount = Character.charCount(cp);

            if (cp == '\n' || cp == '\t' || cp == '\r') {
                return true; // 키에서는 LF, 탭, CR도 큰따옴표로 이스케이프 (1.9 995행)
            }
            if (cp == 0x2028 || cp == 0x2029 || cp == 0xFEFF) {
                return true;
            }
            if (charCount == 1) {
                char c = s.charAt(i);
                if (Character.isSurrogate(c)) {
                    return true;
                }
                boolean printable = (c >= 0x20 && c <= 0x7E) ||
                        (c >= 0xA1 && c <= 0xD7FF) ||
                        (c >= 0xE000 && c <= 0xFFFD);
                if (!printable) {
                    return true;
                }
            } else {
                if (cp > 0x10FFFF) {
                    return true;
                }
            }
            i += charCount;
        }
        return false;
    }

    /**
     * 설명서 1.9에 따라 문자열을 YAML 스칼라 형식으로 직렬화합니다.
     *
     * @param s 직렬화할 문자열
     * @param depth 중첩 깊이 (W = max(40, 80 - 2 * depth))
     * @return 직렬화된 문자열 (따옴표 또는 블록 스칼라 포함)
     */
    public static String dumpString(String s, int depth) {
        if (s == null || s.isEmpty()) {
            return "''";
        }

        // 2. 불리언 키워드 및 60진 모양
        if (BOOLEAN_KEYWORDS.contains(s) || BASE60_PATTERN.matcher(s).matches()) {
            return "'" + s + "'";
        }

        // 3. 인쇄 가능 범위 밖 글자 확인 (LF는 블록으로 가므로 제외)
        if (hasNonPrintableCharacters(s)) {
            return dumpDoubleQuoted(s);
        }

        int w = Math.max(40, 80 - 2 * depth);

        // 5. LF로 나눈 줄 가운데 W를 넘고 U+0020으로 시작하지 않는 줄이 있는지 확인
        boolean hasLf = s.indexOf('\n') >= 0;
        boolean hasFoldableLine = false;

        String[] lines = s.split("\n", -1);
        for (String line : lines) {
            if (line.length() > w && (line.isEmpty() || line.charAt(0) != ' ')) {
                hasFoldableLine = true;
                break;
            }
        }

        // 7. 접을 줄이 있으면 접는 블록 '>', LF만 있으면 그대로 블록 '|'
        if (hasFoldableLine) {
            return dumpFoldedBlock(s, lines, depth, w);
        } else if (hasLf) {
            return dumpLiteralBlock(s, lines, depth);
        }

        // 6. LF도 접을 줄도 없으면 plain 조건 검사
        if (isPlainSafe(s) && !isOtherType(s)) {
            return s;
        } else {
            return "'" + s.replace("'", "''") + "'";
        }
    }

    private static boolean hasNonPrintableCharacters(String s) {
        int len = s.length();
        for (int i = 0; i < len; ) {
            int cp = s.codePointAt(i);
            int charCount = Character.charCount(cp);

            if (cp == '\n') {
                i += charCount;
                continue;
            }

            if (cp == 0x2028 || cp == 0x2029 || cp == 0xFEFF) {
                return true;
            }

            if (charCount == 1) {
                char c = s.charAt(i);
                if (Character.isSurrogate(c)) {
                    return true; // 짝 없는 서로게이트
                }
                boolean printable = (c >= 0x20 && c <= 0x7E) ||
                        (c >= 0xA1 && c <= 0xD7FF) ||
                        (c >= 0xE000 && c <= 0xFFFD);
                if (!printable) {
                    return true;
                }
            } else {
                if (cp > 0x10FFFF) {
                    return true;
                }
            }
            i += charCount;
        }
        return false;
    }

    private static String dumpDoubleQuoted(String s) {
        StringBuilder sb = new StringBuilder(s.length() + 16);
        sb.append('"');
        int len = s.length();
        for (int i = 0; i < len; ) {
            int cp = s.codePointAt(i);
            int charCount = Character.charCount(cp);

            if (charCount == 1) {
                char c = s.charAt(i);
                if (Character.isSurrogate(c)) {
                    sb.append(String.format("\\u%04X", (int) c));
                    i++;
                    continue;
                }
                switch (c) {
                    case '\0' -> sb.append("\\0");
                    case 0x07 -> sb.append("\\a");
                    case '\b' -> sb.append("\\b");
                    case '\t' -> sb.append("\\t");
                    case '\n' -> sb.append("\\n");
                    case 0x0B -> sb.append("\\v");
                    case '\f' -> sb.append("\\f");
                    case '\r' -> sb.append("\\r");
                    case 0x1B -> sb.append("\\e");
                    case '"' -> sb.append("\\\"");
                    case '\\' -> sb.append("\\\\");
                    case 0x85 -> sb.append("\\N");
                    case 0xA0 -> sb.append("\\_");
                    case 0x2028 -> sb.append("\\L");
                    case 0x2029 -> sb.append("\\P");
                    default -> {
                        boolean printable = (c >= 0x20 && c <= 0x7E) ||
                                (c >= 0xA1 && c <= 0xD7FF) ||
                                (c >= 0xE000 && c <= 0xFFFD);
                        if (printable && c != 0xFEFF) {
                            sb.append(c);
                        } else if (c <= 0xFF) {
                            sb.append(String.format("\\x%02X", (int) c));
                        } else {
                            sb.append(String.format("\\u%04X", (int) c));
                        }
                    }
                }
            } else {
                if (cp <= 0xFFFF) {
                    sb.append(String.format("\\u%04X", cp));
                } else {
                    sb.append(String.format("\\U%08X", cp));
                }
            }
            i += charCount;
        }
        sb.append('"');
        return sb.toString();
    }

    private static String dumpLiteralBlock(String s, String[] lines, int depth) {
        StringBuilder header = new StringBuilder("|");
        if (startsWithSpaceAfterNewlines(s)) header.append('2');
        char chomping = getChompingIndicator(s);
        if (chomping != ' ') header.append(chomping);

        StringBuilder sb = new StringBuilder();
        sb.append(header).append("\n");

        int indent = 2 * depth;
        String indentStr = " ".repeat(indent);

        int count = lines.length;
        if (s.endsWith("\n") && count > 0 && lines[count - 1].isEmpty()) {
            count--;
        }

        for (int i = 0; i < count; i++) {
            String line = lines[i];
            if (!line.isEmpty()) {
                sb.append(indentStr).append(line);
            }
            if (i < count - 1) {
                sb.append("\n");
            }
        }
        return sb.toString();
    }

    private static String dumpFoldedBlock(String s, String[] lines, int depth, int w) {
        StringBuilder header = new StringBuilder(">");
        if (startsWithSpaceAfterNewlines(s)) header.append('2');
        char chomping = getChompingIndicator(s);
        if (chomping != ' ') header.append(chomping);

        StringBuilder sb = new StringBuilder();
        sb.append(header).append("\n");

        int indent = 2 * depth;
        String indentStr = " ".repeat(indent);

        int count = lines.length;
        if (s.endsWith("\n") && count > 0 && lines[count - 1].isEmpty()) {
            count--;
        }

        for (int i = 0; i < count; i++) {
            String line = lines[i];
            if (line.isEmpty()) {
                sb.append("\n");
                continue;
            }
            if (line.startsWith(" ")) {
                sb.append(indentStr).append(line);
            } else {
                String folded = foldLine(line, w, indentStr);
                sb.append(folded);
            }
            if (i < count - 1) {
                sb.append("\n");
            }
        }
        return sb.toString();
    }

    private static String foldLine(String line, int w, String indentStr) {
        if (line.length() <= w) {
            return indentStr + line;
        }
        StringBuilder res = new StringBuilder();
        int start = 0;
        int len = line.length();
        boolean firstSubLine = true;

        while (start < len) {
            if (len - start <= w) {
                if (!firstSubLine) res.append("\n");
                res.append(indentStr).append(line, start, len);
                break;
            }
            int breakPos = -1;
            for (int i = start; i <= start + w && i < len; i++) {
                if (line.charAt(i) == ' ' && i + 1 < len && line.charAt(i + 1) != ' ') {
                    breakPos = i;
                }
            }
            if (breakPos == -1) {
                for (int i = start + w + 1; i < len; i++) {
                    if (line.charAt(i) == ' ' && i + 1 < len && line.charAt(i + 1) != ' ') {
                        breakPos = i;
                        break;
                    }
                }
            }
            if (breakPos == -1 || breakPos <= start) {
                if (!firstSubLine) res.append("\n");
                res.append(indentStr).append(line, start, len);
                break;
            }

            if (!firstSubLine) res.append("\n");
            res.append(indentStr).append(line, start, breakPos);
            firstSubLine = false;
            start = breakPos + 1;
        }
        return res.toString();
    }

    private static char getChompingIndicator(String s) {
        if (!s.endsWith("\n")) {
            return '-';
        }
        if (s.equals("\n") || s.endsWith("\n\n")) {
            return '+';
        }
        return ' ';
    }

    private static boolean startsWithSpaceAfterNewlines(String s) {
        int i = 0;
        while (i < s.length() && s.charAt(i) == '\n') {
            i++;
        }
        return i < s.length() && s.charAt(i) == ' ';
    }

    private static boolean isPlainSafe(String s) {
        if (s.isEmpty()) return false;
        char first = s.charAt(0);
        if (PLAIN_FORBIDDEN_START.indexOf(first) >= 0) return false;
        char last = s.charAt(s.length() - 1);
        if (last == ' ' || last == ':') return false;
        if (s.contains(": ")) return false;
        if (s.contains(" #")) return false;
        return true;
    }

    private static boolean isOtherType(String s) {
        if (NULL_LITERALS.contains(s)) return true;
        if (BOOLEAN_LITERALS.contains(s)) return true;
        if ("<<".equals(s)) return true;
        if (INTEGER_PATTERN.matcher(s).matches()) return true;
        if (FLOAT_PATTERN.matcher(s).matches()) return true;
        if (DATE_PATTERN.matcher(s).matches()) return true;
        if (TIMESTAMP_PATTERN.matcher(s).matches()) return true;
        return false;
    }
}
