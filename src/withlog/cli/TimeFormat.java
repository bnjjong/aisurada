package withlog.cli;

import java.time.Duration;
import java.time.Instant;

/**
 * 설명서 3.5 규격에 따른 "얼마 전" 시간 서식 유틸리티.
 * 상한, 하루 청소, doctor 등에서 공통으로 사용합니다.
 */
public final class TimeFormat {

    private TimeFormat() {}

    /**
     * 특정 과거 순간과 현재 시각(now)을 비교하여 규격화된 "얼마 전" 문자열을 반환합니다.
     */
    public static String formatAgo(Instant past, Instant now) {
        if (past == null) {
            return "(모름)";
        }
        if (past.isAfter(now)) {
            return "(시각이 미래다: " + past.toString() + ")";
        }

        long seconds = Duration.between(past, now).toSeconds();
        if (seconds < 60) {
            return seconds + "초 전";
        }
        long minutes = seconds / 60;
        if (minutes < 60) {
            return minutes + "분 전";
        }
        long hours = minutes / 60;
        if (hours < 24) {
            return hours + "시간 전";
        }
        long days = hours / 24;
        return days + "일 전";
    }

    public static String formatAgo(Instant past) {
        return formatAgo(past, Instant.now());
    }
}
