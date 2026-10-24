package withlog.args;

import java.util.Collections;
import java.util.List;
import java.util.Map;

/**
 * 파싱된 명령줄/도구 인자 정보 DTO.
 * @param isToolSurface 도구 표면(get_doc 등)으로 호출되었는지 여부
 * @param commandName 실행 대상 정규 명령 이름 (get-doc, get-sub-note, reindex 등)
 * @param positionalArgs 위치 인자 목록 (0번, 1번 등)
 * @param namedOptions 이름 인자 맵 (이름 -> 값)
 * @param rawParentSubNote get_sub_note 호출 시 받은 원본 parent 문자열 (에러 메시지 출력용)
 */
public record ParsedArgs(
        boolean isToolSurface,
        String commandName,
        List<String> positionalArgs,
        Map<String, String> namedOptions,
        String rawParentSubNote,
        Map<String, Object> toolOptions
) {
    public ParsedArgs(
            boolean isToolSurface,
            String commandName,
            List<String> positionalArgs,
            Map<String, String> namedOptions,
            String rawParentSubNote
    ) {
        this(isToolSurface, commandName, positionalArgs, namedOptions, rawParentSubNote, Collections.emptyMap());
    }
}
