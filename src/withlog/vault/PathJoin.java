package withlog.vault;

import withlog.cli.WlException;
import withlog.text.Trim;

import java.nio.file.Path;

/**
 * 경로 조각 검사 및 결합 유틸리티.
 * 설명서 7.4 및 11.4에 따라 경로 조각 중 공백뿐인 조각이 있을 경우 WL-400 오류를 발생시킵니다.
 */
public final class PathJoin {
    private PathJoin() {}

    /**
     * 기준 경로에 하위 조각들을 차례로 결합하며 공백 조각을 검사합니다.
     * @param base 기준 Path
     * @param segments 추가할 경로 조각들
     * @return 결합된 Path
     * @throws WlException 조각 중 공백뿐인 조각이 있을 때
     */
    public static Path join(Path base, String... segments) {
        Path current = base;
        for (String seg : segments) {
            if (seg == null || Trim.isBlank(seg)) {
                throw new WlException(400, "빈 경로 조각은 결합할 수 없다: 기준=" + current.toAbsolutePath().normalize());
            }
            current = current.resolve(seg);
        }
        return current;
    }
}
