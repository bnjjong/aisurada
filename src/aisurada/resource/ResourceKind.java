package aisurada.resource;

import java.util.regex.Pattern;

/**
 * 자원의 종류. 스킬은 폴더 하나(안에 {@code SKILL.md})이고 에이전트 정의는 {@code .md} 파일 하나입니다.
 * <b>외부 설명서에 없는 확장</b>입니다(프로젝트에서 작업을 시키기 위한 바탕).
 *
 * <p>ID는 노트·아젠다와 같은 모양(머리 + 36진수 시각 + {@code -} + 16진수 8자리)이고, 머리가 종류를 알려 줍니다.
 */
public enum ResourceKind {
    SKILL("skill", "sk", "skills"),
    AGENT("agent", "ap", "agents");

    private static final Pattern ID_SHAPE = Pattern.compile("^(sk|ap)[0-9a-z]+-[0-9a-f]{8}$");

    private final String label;
    private final String idPrefix;
    private final String dir;

    ResourceKind(String label, String idPrefix, String dir) {
        this.label = label;
        this.idPrefix = idPrefix;
        this.dir = dir;
    }

    /** 명령 인자와 응답에 쓰는 이름({@code skill}, {@code agent}). */
    public String label() {
        return label;
    }

    /** ID 머리({@code sk}, {@code ap}). */
    public String idPrefix() {
        return idPrefix;
    }

    /** 범위 폴더 아래의 종류별 폴더 이름({@code skills}, {@code agents}). */
    public String dir() {
        return dir;
    }

    /** 인자 글을 종류로 읽는다. 모르는 글이면 null. */
    public static ResourceKind parse(String raw) {
        if (raw == null) return null;
        String v = raw.trim().toLowerCase(java.util.Locale.ROOT);
        for (ResourceKind k : values()) {
            if (k.label.equals(v)) return k;
        }
        return null;
    }

    /** 자원 ID 모양이면 true. */
    public static boolean isId(String s) {
        return s != null && ID_SHAPE.matcher(s).matches();
    }

    /** ID 머리로 종류를 안다. ID 모양이 아니면 null. */
    public static ResourceKind ofId(String id) {
        if (!isId(id)) return null;
        return id.startsWith(SKILL.idPrefix) ? SKILL : AGENT;
    }
}
