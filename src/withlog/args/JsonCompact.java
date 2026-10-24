package withlog.args;

import java.util.Map;

/**
 * JSON 값을 공백 없이 다시 찍는 포맷터.
 * 설명서 5.4의 <값의 JSON 글자> 규칙에 따라 문자열 탈출 및 축약 포맷을 생성합니다.
 */
public final class JsonCompact {
    private JsonCompact() {}

    /**
     * JsonValue를 공백 없는 JSON 문자열로 변환합니다.
     * @param value JSON AST 노드
     * @return 압축된 JSON 문자열
     */
    public static String compact(JsonValue value) {
        StringBuilder sb = new StringBuilder();
        append(sb, value);
        return sb.toString();
    }

    private static void append(StringBuilder sb, JsonValue value) {
        if (value instanceof JsonValue.JsonString str) {
            appendString(sb, str.value());
        } else if (value instanceof JsonValue.JsonLiteral lit) {
            sb.append(lit.raw());
        } else if (value instanceof JsonValue.JsonObject obj) {
            sb.append('{');
            boolean first = true;
            for (Map.Entry<String, JsonValue> entry : obj.properties().entrySet()) {
                if (!first) {
                    sb.append(',');
                }
                first = false;
                appendString(sb, entry.getKey());
                sb.append(':');
                append(sb, entry.getValue());
            }
            sb.append('}');
        } else if (value instanceof JsonValue.JsonArray arr) {
            sb.append('[');
            boolean first = true;
            for (JsonValue item : arr.elements()) {
                if (!first) {
                    sb.append(',');
                }
                first = false;
                append(sb, item);
            }
            sb.append(']');
        }
    }

    /**
     * 문자열을 규격대로 큰따옴표 및 탈출 처리하여 버퍼에 추가합니다.
     */
    public static void appendString(StringBuilder sb, String s) {
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
