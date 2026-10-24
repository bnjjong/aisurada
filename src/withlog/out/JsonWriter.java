package withlog.out;

import withlog.text.Utf16;

import java.util.List;
import java.util.Map;

/**
 * 규격화된 stdout JSON 직렬화기.
 * 설명서 6.1에 따라 2칸 들여쓰기, LF 줄바꿈, 빈 컬렉션 한 줄 표기,
 * 특수문자 탈출, 끝 LF 1개를 만족하는 UTF-8 바이트 스트림을 생성합니다.
 */
public final class JsonWriter {
    private JsonWriter() {}

    /**
     * 임의의 Java 객체(Map, List, String, Number, Boolean, null)를 규격 JSON 문자열로 직렬화합니다.
     * 결과는 마지막에 LF 1개가 포함됩니다.
     * @param root 직렬화할 루트 객체
     * @return 직렬화된 JSON 문자열 (마지막 LF 포함)
     */
    public static String stringify(Object root) {
        StringBuilder sb = new StringBuilder();
        appendValue(sb, root, 0);
        sb.append('\n');
        return sb.toString();
    }

    /**
     * 임의의 Java 객체를 규격 JSON의 UTF-8 바이트 배열로 변환합니다.
     * 짝 없는 서로게이트는 '?' (0x3F) 1바이트로 치환됩니다.
     * @param root 직렬화할 루트 객체
     * @return 규격 stdout 바이트 배열
     */
    public static byte[] toBytes(Object root) {
        String json = stringify(root);
        return Utf16.toUtf8BytesWithQuestionForUnpaired(json);
    }

    @SuppressWarnings("unchecked")
    private static void appendValue(StringBuilder sb, Object val, int depth) {
        if (val == null) {
            sb.append("null");
        } else if (val instanceof Boolean b) {
            sb.append(b ? "true" : "false");
        } else if (val instanceof Number n) {
            sb.append(n.toString());
        } else if (val instanceof String s) {
            appendString(sb, s);
        } else if (val instanceof Map<?, ?> map) {
            appendObject(sb, (Map<String, Object>) map, depth);
        } else if (val instanceof List<?> list) {
            appendArray(sb, list, depth);
        } else {
            appendString(sb, val.toString());
        }
    }

    private static void appendObject(StringBuilder sb, Map<String, Object> map, int depth) {
        if (map.isEmpty()) {
            sb.append("{}");
            return;
        }
        sb.append("{\n");
        int indent = (depth + 1) * 2;
        boolean first = true;
        for (Map.Entry<String, Object> entry : map.entrySet()) {
            if (!first) {
                sb.append(",\n");
            }
            first = false;
            indent(sb, indent);
            appendString(sb, entry.getKey());
            sb.append(": ");
            appendValue(sb, entry.getValue(), depth + 1);
        }
        sb.append('\n');
        indent(sb, depth * 2);
        sb.append('}');
    }

    private static void appendArray(StringBuilder sb, List<?> list, int depth) {
        if (list.isEmpty()) {
            sb.append("[]");
            return;
        }
        sb.append("[\n");
        int indent = (depth + 1) * 2;
        boolean first = true;
        for (Object item : list) {
            if (!first) {
                sb.append(",\n");
            }
            first = false;
            indent(sb, indent);
            appendValue(sb, item, depth + 1);
        }
        sb.append('\n');
        indent(sb, depth * 2);
        sb.append(']');
    }

    private static void indent(StringBuilder sb, int spaces) {
        for (int i = 0; i < spaces; i++) {
            sb.append(' ');
        }
    }

    private static void appendString(StringBuilder sb, String s) {
        sb.append('"');
        if (s != null) {
            for (int i = 0; i < s.length(); i++) {
                char c = s.charAt(i);
                switch (c) {
                    case '"' -> sb.append("\\\"");
                    case '\\' -> sb.append("\\\\");
                    case '\b' -> sb.append("\\b");
                    case '\t' -> sb.append("\\t");
                    case '\n' -> sb.append("\\n");
                    case '\f' -> sb.append("\\f");
                    case '\r' -> sb.append("\\r");
                    default -> {
                        if (c <= 0x001F) {
                            sb.append(String.format("\\u00%02x", (int) c));
                        } else {
                            sb.append(c);
                        }
                    }
                }
            }
        }
        sb.append('"');
    }
}
