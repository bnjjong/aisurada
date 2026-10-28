package withlog.write;

import withlog.text.Trim;

import java.util.ArrayList;
import java.util.List;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * 설명서 1.11과 17.5 지우기의 4번, 하위 페이지 카드 걷기.
 * 지운 하위 노트를 가리키는 카드가 직계 컨테이너 본문에 남아 눌러도 아무 데도 못 가는 일을 막습니다.
 */
public final class ChildPageCards {

    /** 줄 전체가 `[제목](#sub:<id>)`와 ASCII 공백 0개 이상입니다. 들여쓴 줄과 문장 안 링크는 안 맞습니다. */
    private static final Pattern CARD_LINE = Pattern.compile("\\[[^\\]]+\\]\\(#sub:([^)]+)\\)[ \\t\\n\\x0B\\f\\r]*");

    private ChildPageCards() {}

    /**
     * [body]를 LF로 나눠 [subId]를 가리키는 카드 줄만 빼고 남은 줄을 LF로 다시 잇습니다.
     * 괄호 안의 id는 다듬어 견주어서 `#sub: id `와 끝에 CR이 붙은 줄도 카드입니다. 걷을 줄이 없으면 [body] 그대로입니다.
     */
    public static String strip(String body, String subId) {
        String[] lines = body.split("\n", -1);
        List<String> kept = new ArrayList<>(lines.length);
        for (String line : lines) {
            Matcher m = CARD_LINE.matcher(line);
            if (m.matches() && Trim.trim(m.group(1)).equals(subId)) {
                continue;
            }
            kept.add(line);
        }
        return kept.size() == lines.length ? body : String.join("\n", kept);
    }
}
