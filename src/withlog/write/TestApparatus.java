package withlog.write;

import java.security.SecureRandom;
import java.time.Instant;
import java.time.LocalDate;
import java.time.LocalDateTime;
import java.time.LocalTime;
import java.time.ZoneId;
import java.util.LinkedList;
import java.util.Queue;

/**
 * 테스트 제어용 시계 및 난수 발생 장치.
 * JVM 시스템 속성 withlog.test.now 및 withlog.test.random을 통해
 * 결정론적 시각과 식별자 생성을 지원합니다.
 */
public final class TestApparatus {

    private static final SecureRandom SECURE_RANDOM = new SecureRandom();
    private static final Object RANDOM_LOCK = new Object();
    private static Queue<String> testRandomQueue = null;

    private TestApparatus() {}

    /**
     * 현재 유닉스 밀리초 시각을 반환합니다.
     * withlog.test.now 시스템 속성이 설정되어 있으면 해당 값을 우선 반환합니다.
     *
     * @return 유닉스 에포크 밀리초
     */
    public static long nowMillis() {
        String prop = System.getProperty("withlog.test.now");
        if (prop != null && !prop.trim().isEmpty()) {
            try {
                return Long.parseLong(prop.trim());
            } catch (NumberFormatException ignored) {}
        }
        return System.currentTimeMillis();
    }

    /**
     * 현재 시각의 Instant 객체를 반환합니다.
     *
     * @return 현재 Instant
     */
    public static Instant nowInstant() {
        return Instant.ofEpochMilli(nowMillis());
    }

    /**
     * 지정한 시간대 기준 현재 날짜와 시각을 반환합니다.
     *
     * @param zone 시간대
     * @return 현재 LocalDateTime
     */
    public static LocalDateTime nowLocalDateTime(ZoneId zone) {
        return LocalDateTime.ofInstant(nowInstant(), zone);
    }

    /**
     * 지정한 시간대 기준 오늘 날짜를 반환합니다.
     *
     * @param zone 시간대
     * @return 현재 LocalDate
     */
    public static LocalDate today(ZoneId zone) {
        return LocalDate.ofInstant(nowInstant(), zone);
    }

    /**
     * 지정한 시간대 기준 현재 시각(초 아래 버림)을 반환합니다.
     *
     * @param zone 시간대
     * @return 현재 LocalTime (초=0, 나노초=0)
     */
    public static LocalTime nowTime(ZoneId zone) {
        LocalTime lt = LocalTime.ofInstant(nowInstant(), zone);
        return lt.withSecond(0).withNano(0);
    }

    /**
     * 식별자 꼬리에 사용할 8자리 16진수 무작위 글자를 반환합니다.
     * withlog.test.random 시스템 속성에 쉼표로 구분된 8자리 16진수가 있으면 차례대로 소진하고,
     * 소진되었거나 설정되지 않았으면 암호학적으로 안전한 무작위 4바이트를 생성합니다.
     *
     * @return 소문자 8자리 16진수 문자열
     */
    public static String nextRandomHex() {
        synchronized (RANDOM_LOCK) {
            if (testRandomQueue == null) {
                initRandomQueue();
            }
            if (testRandomQueue != null && !testRandomQueue.isEmpty()) {
                return testRandomQueue.poll();
            }
        }
        byte[] bytes = new byte[4];
        SECURE_RANDOM.nextBytes(bytes);
        return String.format("%02x%02x%02x%02x", bytes[0], bytes[1], bytes[2], bytes[3]);
    }

    /**
     * 임시 파일 이름에 사용할 64비트 무작위 정수를 반환합니다.
     *
     * @return 부호 없는 64비트 정수로 취급할 long 값
     */
    public static long nextRandom64() {
        return SECURE_RANDOM.nextLong();
    }

    /**
     * 테스트 실행 간 큐를 초기화합니다.
     */
    public static void resetRandomQueue() {
        synchronized (RANDOM_LOCK) {
            initRandomQueue();
        }
    }

    private static void initRandomQueue() {
        String prop = System.getProperty("withlog.test.random");
        if (prop != null && !prop.trim().isEmpty()) {
            Queue<String> queue = new LinkedList<>();
            for (String part : prop.split(",")) {
                String trimmed = part.trim().toLowerCase();
                if (!trimmed.isEmpty()) {
                    queue.add(trimmed);
                }
            }
            testRandomQueue = queue;
        } else {
            testRandomQueue = null;
        }
    }
}
