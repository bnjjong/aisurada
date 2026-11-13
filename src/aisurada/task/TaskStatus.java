package aisurada.task;

import java.util.Locale;

/**
 * 태스크 상태 8개. 파일과 명령에는 {@link #label()}의 영문 소문자 이름을 쓰고, 화면은 {@link #title()}을 보인다.
 */
public enum TaskStatus {
    DRAFT("draft", "Draft"),
    READY("ready-for-develop", "Ready For Develop"),
    IN_PROGRESS("in-progress", "In Progress"),
    WAITING("waiting-answer", "Waiting Answer"),
    IN_REVIEW("in-review", "In Review"),
    DONE("done", "Done"),
    FAILED("failed", "Failed"),
    CANCELLED("cancelled", "Cancelled");

    private final String label;
    private final String title;

    TaskStatus(String label, String title) {
        this.label = label;
        this.title = title;
    }

    /** 파일과 명령에 쓰는 이름({@code ready-for-develop}). */
    public String label() {
        return label;
    }

    /** 화면에 보이는 이름({@code Ready For Develop}). */
    public String title() {
        return title;
    }

    /** 끝난 상태(Done, Failed, Cancelled)면 true. Failed와 Cancelled는 Draft로 되돌릴 수 있다. */
    public boolean isFinished() {
        return this == DONE || this == FAILED || this == CANCELLED;
    }

    /** 이름이나 화면 이름을 상태로 읽는다. 대소문자와 공백·밑줄·하이픈 차이는 무시한다. 모르는 글이면 null. */
    public static TaskStatus parse(String raw) {
        if (raw == null) return null;
        String v = raw.trim().toLowerCase(Locale.ROOT).replace(' ', '-').replace('_', '-');
        for (TaskStatus s : values()) {
            if (s.label.equals(v)) return s;
        }
        return null;
    }
}
