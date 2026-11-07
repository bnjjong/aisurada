package aisurada;

import static aisurada.ChapterTestSupport.*;

/**
 * 데일리 리포트(회고) 기능 제거에 따라 본 테스트는 통과 처리합니다.
 */
public final class Chapter25Test {
    public static void main(String[] args) throws Exception {
        try (var t = new ChapterTestSupport("chapter25")) {
            t.done();
        }
    }
}
