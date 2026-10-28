package withlog.write;

import java.util.Locale;
import java.util.regex.Pattern;

/**
 * 식별자 발급 및 유효성 검증기.
 * 설명서 1.4에 따라 머리 + 36진수 유닉스 밀리초 + '-' + 16진수 8자리 무작위 값으로 id를 발급합니다.
 */
public final class IdGenerator {

    private static final Pattern NOTE_ID_PATTERN = withlog.text.IdShape.NOTE;
    private static final Pattern SUB_NOTE_ID_PATTERN = withlog.text.IdShape.SUB_NOTE;
    private static final Pattern AGENDA_ID_PATTERN = Pattern.compile("^(ag[0-9a-z]+-[0-9a-f]{8}|ag-recovered)$");

    private IdGenerator() {}

    /**
     * 지정한 접두어로 새 식별자를 발급합니다.
     *
     * @param prefix 머리 접두어 (nd, ag, sn, pj, tr 등)
     * @return 발급된 식별자 문자열
     */
    public static String nextId(String prefix) {
        long millis = TestApparatus.nowMillis();
        String timeBase36 = Long.toString(millis, 36).toLowerCase(Locale.ROOT);
        String randomHex = TestApparatus.nextRandomHex();
        return prefix + timeBase36 + "-" + randomHex;
    }

    /**
     * 새 노트 id를 발급합니다 (nd 접두어).
     *
     * @return nd... 식별자
     */
    public static String nextNoteId() {
        return nextId("nd");
    }

    /**
     * 새 아젠다 id를 발급합니다 (ag 접두어).
     *
     * @return ag... 식별자
     */
    public static String nextAgendaId() {
        return nextId("ag");
    }

    /**
     * 새 하위 노트 id를 발급합니다 (sn 접두어).
     *
     * @return sn... 식별자
     */
    public static String nextSubNoteId() {
        return nextId("sn");
    }

    /**
     * 문자열이 올바른 노트 식별자 모양인지 검사합니다.
     *
     * @param id 검사할 식별자
     * @return 올바른 모양이면 true
     */
    public static boolean isValidNoteId(String id) {
        if (id == null) return false;
        return NOTE_ID_PATTERN.matcher(id).matches();
    }

    /**
     * 문자열이 올바른 하위 노트 식별자 모양인지 검사합니다.
     *
     * @param id 검사할 식별자
     * @return 올바른 모양이면 true
     */
    public static boolean isValidSubNoteId(String id) {
        if (id == null) return false;
        return SUB_NOTE_ID_PATTERN.matcher(id).matches();
    }

    /**
     * 문자열이 올바른 아젠다 식별자 모양인지 검사합니다.
     *
     * @param id 검사할 식별자
     * @return 올바른 모양이면 true
     */
    public static boolean isValidAgendaId(String id) {
        if (id == null) return false;
        return AGENDA_ID_PATTERN.matcher(id).matches();
    }
}
