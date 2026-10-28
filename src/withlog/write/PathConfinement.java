package withlog.write;

import withlog.cli.WlException;
import withlog.text.Trim;

import java.nio.file.Path;

/**
 * 기준 폴더 가둠 검사 및 경로 조각 안전성 검증기.
 * 설명서 7.4에 따라 상위 폴더 탈출, 빈 경로 조각, 경로로 쓸 수 없는 문자열을 거절합니다.
 */
public final class PathConfinement {

    private PathConfinement() {}

    /**
     * 경로 조각(segment)의 유효성을 검사합니다.
     * 빈 글자이거나 공백뿐인 조각, NUL 문자(\u0000) 또는 짝 없는 서로게이트가 포함된 조각을 거절합니다.
     *
     * @param base 기준 폴더 경로
     * @param segment 결합할 경로 조각
     * @throws WlException 유효하지 않은 조각일 때 WL-400
     */
    public static void checkSegment(Path base, String segment) {
        if (segment == null || Trim.isBlank(segment)) {
            throw new WlException(400, "빈 경로 조각은 결합할 수 없다: 기준=" + base.toAbsolutePath().normalize());
        }
        if (hasUnusablePathCharacters(segment)) {
            throw new WlException(400, "경로로 쓸 수 없는 값이다: " + segment);
        }
    }

    /**
     * 결합된 경로가 기준 폴더를 벗어나는지 검사합니다.
     *
     * @param base 기준 폴더 경로
     * @param target 결합된 결과 경로
     * @throws WlException 기준 폴더를 벗어날 때 WL-400
     */
    public static void checkConfinement(Path base, Path target) {
        Path normBase = base.toAbsolutePath().normalize();
        Path normTarget = target.toAbsolutePath().normalize();
        if (!normTarget.startsWith(normBase)) {
            throw new WlException(400, "기준 폴더 밖을 가리키는 경로다: " + normTarget);
        }
    }

    /**
     * 문자열에 NUL 문자 또는 짝 없는 서로게이트가 포함되어 있는지 검사합니다.
     *
     * @param s 검사할 문자열
     * @return 경로로 쓸 수 없는 문자가 포함되어 있으면 true
     */
    public static boolean hasUnusablePathCharacters(CharSequence s) {
        int len = s.length();
        for (int i = 0; i < len; i++) {
            char c = s.charAt(i);
            if (c == '\0') {
                return true;
            }
            if (Character.isHighSurrogate(c)) {
                if (i + 1 < len && Character.isLowSurrogate(s.charAt(i + 1))) {
                    i++; // 정상 서로게이트 쌍
                } else {
                    return true; // 짝 없는 상위 서로게이트
                }
            } else if (Character.isLowSurrogate(c)) {
                return true; // 짝 없는 하위 서로게이트
            }
        }
        return false;
    }
}
