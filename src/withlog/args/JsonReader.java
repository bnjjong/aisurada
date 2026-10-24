package withlog.args;

import withlog.text.Trim;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * withlog 규격의 kotlinx.serialization 1.6.3 호환 JSON 파서.
 * 설명서 5.4에 명시된 규칙(따옴표 없는 리터럴, 날 제어문자 허용, 중복 키 마지막 값 유지, 쉼표 엄격성 등)을 준수합니다.
 */
public final class JsonReader {
    private final String src;
    private final int len;
    private int pos = 0;

    private JsonReader(String src) {
        this.src = src;
        this.len = src.length();
    }

    /**
     * 입력 문자열을 다듬은 후 JsonValue로 파싱합니다.
     * @param raw 입력 원문
     * @return 파싱된 JsonValue
     * @throws IllegalArgumentException 파싱 문법 오류 시
     */
    public static JsonValue parse(String raw) {
        String trimmed = Trim.trim(raw);
        if (trimmed.isEmpty()) {
            trimmed = "{}";
        }
        JsonReader reader = new JsonReader(trimmed);
        reader.skipSpaces();
        JsonValue val = reader.readValue();
        reader.skipSpaces();
        if (reader.pos < reader.len) {
            throw new IllegalArgumentException("Unexpected trailing character at pos " + reader.pos);
        }
        return val;
    }

    private void skipSpaces() {
        while (pos < len) {
            char c = src.charAt(pos);
            if (c == ' ' || c == '\t' || c == '\n' || c == '\r') {
                pos++;
            } else {
                break;
            }
        }
    }

    private JsonValue readValue() {
        if (pos >= len) {
            throw new IllegalArgumentException("Unexpected end of input");
        }
        char c = src.charAt(pos);
        if (c == '"') {
            return new JsonValue.JsonString(readString());
        } else if (c == '{') {
            return readObject();
        } else if (c == '[') {
            return readArray();
        } else {
            return readLiteral();
        }
    }

    private String readString() {
        pos++; // skip opening '"'
        StringBuilder sb = new StringBuilder();
        while (pos < len) {
            char c = src.charAt(pos++);
            if (c == '"') {
                return sb.toString();
            } else if (c == '\\') {
                if (pos >= len) {
                    throw new IllegalArgumentException("Unterminated escape sequence in string");
                }
                char esc = src.charAt(pos++);
                switch (esc) {
                    case '"' -> sb.append('"');
                    case '\\' -> sb.append('\\');
                    case '/' -> sb.append('/');
                    case 'b' -> sb.append('\b');
                    case 'f' -> sb.append('\f');
                    case 'n' -> sb.append('\n');
                    case 'r' -> sb.append('\r');
                    case 't' -> sb.append('\t');
                    case 'u' -> {
                        if (pos + 4 > len) {
                            throw new IllegalArgumentException("Invalid unicode escape");
                        }
                        String hex = src.substring(pos, pos + 4);
                        pos += 4;
                        try {
                            int code = Integer.parseInt(hex, 16);
                            sb.append((char) code);
                        } catch (NumberFormatException e) {
                            throw new IllegalArgumentException("Invalid hex in unicode escape: " + hex, e);
                        }
                    }
                    default -> throw new IllegalArgumentException("Unsupported escape: \\" + esc);
                }
            } else {
                // 탈출하지 않은 제어 문자(U+0000~U+001F)도 그대로 허용
                sb.append(c);
            }
        }
        throw new IllegalArgumentException("Unterminated string");
    }

    private JsonValue.JsonObject readObject() {
        pos++; // skip '{'
        skipSpaces();
        Map<String, JsonValue> map = new LinkedHashMap<>();
        if (pos < len && src.charAt(pos) == '}') {
            pos++;
            return new JsonValue.JsonObject(map);
        }

        while (pos < len) {
            skipSpaces();
            if (pos >= len) {
                throw new IllegalArgumentException("Unterminated object");
            }
            if (src.charAt(pos) == ',') {
                throw new IllegalArgumentException("Leading or unexpected comma in object");
            }
            if (src.charAt(pos) != '"') {
                throw new IllegalArgumentException("Object keys must be quoted strings");
            }
            String key = readString();
            skipSpaces();
            if (pos >= len || src.charAt(pos) != ':') {
                throw new IllegalArgumentException("Expected ':' after key");
            }
            pos++; // skip ':'
            skipSpaces();
            JsonValue val = readValue();
            map.put(key, val);

            skipSpaces();
            if (pos < len && src.charAt(pos) == ',') {
                pos++;
                skipSpaces();
                if (pos < len && src.charAt(pos) == '}') {
                    throw new IllegalArgumentException("Trailing comma in object");
                }
            } else if (pos < len && src.charAt(pos) == '}') {
                pos++;
                return new JsonValue.JsonObject(map);
            } else {
                throw new IllegalArgumentException("Expected ',' or '}' in object");
            }
        }
        throw new IllegalArgumentException("Unterminated object");
    }

    private JsonValue.JsonArray readArray() {
        pos++; // skip '['
        skipSpaces();
        List<JsonValue> list = new ArrayList<>();
        if (pos < len && src.charAt(pos) == ']') {
            pos++;
            return new JsonValue.JsonArray(list);
        }

        while (pos < len) {
            skipSpaces();
            if (pos >= len) {
                throw new IllegalArgumentException("Unterminated array");
            }
            if (src.charAt(pos) == ',') {
                throw new IllegalArgumentException("Leading or unexpected comma in array");
            }
            JsonValue val = readValue();
            list.add(val);

            skipSpaces();
            if (pos < len && src.charAt(pos) == ',') {
                pos++;
                skipSpaces();
                if (pos < len && src.charAt(pos) == ']') {
                    throw new IllegalArgumentException("Trailing comma in array");
                }
            } else if (pos < len && src.charAt(pos) == ']') {
                pos++;
                return new JsonValue.JsonArray(list);
            } else {
                throw new IllegalArgumentException("Expected ',' or ']' in array");
            }
        }
        throw new IllegalArgumentException("Unterminated array");
    }

    private JsonValue.JsonLiteral readLiteral() {
        int start = pos;
        while (pos < len) {
            char c = src.charAt(pos);
            if (c == ' ' || c == '\t' || c == '\n' || c == '\r' ||
                c == ',' || c == ':' || c == '{' || c == '}' ||
                c == '[' || c == ']' || c == '"' || c == '\\') {
                break;
            }
            if (c <= 0x001F) {
                break;
            }
            pos++;
        }
        if (pos == start) {
            throw new IllegalArgumentException("Unexpected character for literal at pos " + pos);
        }
        String raw = src.substring(start, pos);
        return new JsonValue.JsonLiteral(raw);
    }
}
