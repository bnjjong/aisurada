package withlog.write;

import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.regex.Pattern;

/**
 * 설명서 1.9 프론트매터 쓰기 규격 직렬화기.
 * 키 차례(1.7), 따옴표 규칙, 큰따옴표 탈출, 접는 블록/그대로 블록을 구현합니다.
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
     * 노트 생성에 사용되는 프론트매터 YAML을 생성합니다.
     * 키 차례(1.7): id, agenda, type, title, created, sessionId, mins, tier, tags, template, links
     *
     * @param id 노트 id
     * @param agenda 아젠다 slug
     * @param type 정식 소문자 노트 유형
     * @param title 노트 제목 (원본 그대로)
     * @param created 시각 문자열 (yyyy-MM-ddTHH:mm:00+09:00)
     * @param sessionId 세션 표식 (있을 때만)
     * @param mins 소요 시간
     * @param tier 층 (record일 때만 기록)
     * @param tags 태그 목록 (비어있지 않을 때만)
     * @param template 템플릿 id (있을 때만)
     * @param links 연결 목록 (비어있지 않을 때만)
     * @return YAML 내용 (양쪽 --- 울타리 제외)
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

        // 6. sessionId
        if (sessionId != null) {
            appendKeyVal(sb, "sessionId", sessionId, 1);
        }

        // 7. mins
        sb.append("mins: ").append(mins).append("\n");

        // 8. tier
        if ("record".equalsIgnoreCase(tier)) {
            sb.append("tier: record\n");
        }

        // 12. tags
        if (tags != null && !tags.isEmpty()) {
            sb.append("tags:\n");
            for (String tag : tags) {
                sb.append("  - ");
                sb.append(dumpString(tag, 2)).append("\n");
            }
        }

        // 15. template
        if (template != null && !template.isEmpty()) {
            appendKeyVal(sb, "template", template, 1);
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
     * 아젠다 파일 생성을 위한 프론트매터 YAML을 생성합니다.
     *
     * @param id 아젠다 id
     * @param slug 아젠다 slug
     * @param title 아젠다 제목
     * @param hue 색상
     * @return YAML 내용 (양쪽 --- 울타리 제외)
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
                // cp >= 0x10000
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
        // 1.9: 들여쓰기 표시 뒤에 끝 표시를 붙인다(|2-)
        StringBuilder header = new StringBuilder("|");
        if (startsWithSpaceAfterNewlines(s)) header.append('2');
        char chomping = getChompingIndicator(s);
        if (chomping != ' ') header.append(chomping);

        StringBuilder sb = new StringBuilder();
        sb.append(header).append("\n");

        int indent = 2 * depth;
        String indentStr = " ".repeat(indent);

        // 마지막 줄바꿈 1개는 뗀 내용
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
        // 1.9: 들여쓰기 표시 뒤에 끝 표시를 붙인다(|2-)
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
                // 접기 적용
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
            // start부터 start + w 범위 내 마지막 공백(' ') 위치 탐색
            int breakPos = -1;
            for (int i = start; i <= start + w && i < len; i++) {
                if (line.charAt(i) == ' ' && i + 1 < len && line.charAt(i + 1) != ' ') {
                    breakPos = i;
                }
            }
            if (breakPos == -1) {
                // 범위 내 적절한 공백이 없으면 다음 공백까지 확장
                for (int i = start + w + 1; i < len; i++) {
                    if (line.charAt(i) == ' ' && i + 1 < len && line.charAt(i + 1) != ' ') {
                        breakPos = i;
                        break;
                    }
                }
            }
            if (breakPos == -1 || breakPos <= start) {
                // 더 이상 나눌 공백이 없으면 나머지 전체
                if (!firstSubLine) res.append("\n");
                res.append(indentStr).append(line, start, len);
                break;
            }

            if (!firstSubLine) res.append("\n");
            res.append(indentStr).append(line, start, breakPos);
            firstSubLine = false;
            start = breakPos + 1; // 공백 건너뜀
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
        return ' '; // 1개의 줄바꿈으로 끝남
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
