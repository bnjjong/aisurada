package withlog.text;

import java.util.regex.Pattern;

/**
 * 설명서 1.4의 식별자 모양. 노트 id와 하위 노트 id 규칙을 이 한 곳에만 둔다.
 * 명령마다 사본을 두면 규칙이 바뀌는 날 어느 명령만 옛 모양을 받기 때문이다.
 */
public final class IdShape {

    private IdShape() {}

    /** 노트 id 모양 {@code ^nd[0-9a-z]+-[0-9a-f]{8}$}. 다듬지 않은 글에 댄다. */
    public static final Pattern NOTE = Pattern.compile("^nd[0-9a-z]+-[0-9a-f]{8}$");

    /** 하위 노트 id 모양 {@code ^sn[0-9a-z]+-[0-9a-f]{8}$}. */
    public static final Pattern SUB_NOTE = Pattern.compile("^sn[0-9a-z]+-[0-9a-f]{8}$");
}
