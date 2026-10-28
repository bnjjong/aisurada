package withlog.write;

import withlog.vault.NoteScanner;

import java.time.LocalDate;
import java.time.LocalTime;
import java.time.ZoneId;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

/**
 * 설명서 13.5 시각 자동 배정 엔진 컴포넌트.
 * 날짜 및 기존 reading 층 노트의 점유 분을 확인하고 5분 단위로 밀어 시각을 배정합니다.
 */
public final class TimeScheduler {

    private static final LocalTime LIMIT_TIME = LocalTime.of(23, 55);

    private TimeScheduler() {}

    /**
     * time 인자가 주어지지 않았을 때 새 노트의 시각을 자동으로 배정합니다.
     *
     * 1. 프로세스 시간대의 현재 시각에서 분 아래를 버린 값을 후보로 설정합니다.
     * 2. 해당 날짜에 존재하는 reading 층 메인 노트들의 시분(HH:mm) 점유 집합을 수집합니다.
     * 3. 후보가 점유되어 있고 23:55보다 이른 동안 후보에 5분씩 더합니다.
     * 4. 멈춘 후보를 반환합니다.
     *
     * @param targetDate 대상 날짜
     * @param allMainNotes 색인된 모든 메인 노트 목록
     * @param zone 시간대
     * @return 배정된 LocalTime (초=0, 나노초=0)
     */
    public static LocalTime scheduleTime(LocalDate targetDate, List<NoteScanner.NoteRecord> allMainNotes, ZoneId zone) {
        LocalTime candidate = TestApparatus.nowTime(zone);

        String dateStr = targetDate.toString();
        Set<String> occupiedTimes = new HashSet<>();

        if (allMainNotes != null) {
            for (NoteScanner.NoteRecord note : allMainNotes) {
                // 하위 노트 제외, reading 층만 포함
                if (note.meta() == null) continue;
                String tier = note.meta().tier();
                if ("record".equalsIgnoreCase(tier)) continue;

                String created = note.meta().created();
                if (created == null || created.length() < 16) continue;

                // created의 자기 오프셋 기준 날짜 및 시분
                String noteDate = created.substring(0, 10);
                if (dateStr.equals(noteDate)) {
                    String noteTime = created.substring(11, 16); // HH:mm
                    occupiedTimes.add(noteTime);
                }
            }
        }

        while (occupiedTimes.contains(formatTime(candidate)) && candidate.isBefore(LIMIT_TIME)) {
            candidate = candidate.plusMinutes(5);
        }

        return candidate;
    }

    /**
     * LocalTime을 HH:mm 형식으로 포맷합니다.
     */
    public static String formatTime(LocalTime time) {
        return String.format("%02d:%02d", time.getHour(), time.getMinute());
    }
}
