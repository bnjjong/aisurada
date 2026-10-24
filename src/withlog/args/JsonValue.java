package withlog.args;

import java.util.List;
import java.util.Map;

/**
 * withlog 엔진의 JSON AST 인터페이스.
 */
public sealed interface JsonValue permits JsonValue.JsonString, JsonValue.JsonLiteral, JsonValue.JsonObject, JsonValue.JsonArray {

    /**
     * 큰따옴표로 감싸진 JSON 문자열 값.
     */
    record JsonString(String value) implements JsonValue {}

    /**
     * 따옴표 없는 리터럴 값 (true, false, null, 숫자, 미따옴표 식별자 등).
     */
    record JsonLiteral(String raw) implements JsonValue {
        /**
         * 이 리터럴이 JSON null인지 여부를 확인합니다.
         */
        public boolean isNull() {
            return "null".equals(raw);
        }
    }

    /**
     * JSON 객체 값.
     * 키 순서는 최초 등장 순서이며, 중복 키는 마지막 값이 반영됩니다.
     */
    record JsonObject(Map<String, JsonValue> properties) implements JsonValue {}

    /**
     * JSON 배열 값.
     */
    record JsonArray(List<JsonValue> elements) implements JsonValue {}
}
