package withlog.write;

import withlog.text.Utf16;
import withlog.vault.AgendaReader;
import withlog.vault.AgendaTable;

import java.util.ArrayList;
import java.util.Collection;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * 설명서 20장 아젠다 파일의 쓰기 규칙.
 * create-agenda, update-agenda와 create_doc의 아젠다 만들기(20.8)가 함께 쓴다.
 * slug와 색 규칙이 2벌이면 같은 이름이 들어온 길에 따라 다른 폴더로 갈리기 때문이다.
 */
public final class AgendaFile {

    /** slug 비켜서기가 붙이는 마지막 꼬리 번호(20.5의 3번). */
    public static final int MAX_SLUG_SUFFIX = 100;

    private AgendaFile() {}

    /**
     * 아젠다 파일의 vault 상대 경로 {@code .withlog/agendas/<slug>.md}를 돌려줍니다.
     */
    public static String relPath(String slug) {
        return ".withlog/agendas/" + slug + ".md";
    }

    /**
     * 20.5의 3번 slug 비켜서기. {@code base}를 {@code taken}이 안 쓰면 그대로 돌려주고, 쓰면 {@code <base>-2}부터
     * {@code <base>-100}까지 비어 있는 첫 이름을 돌려줍니다. 다 차 있으면 null입니다.
     * 비교는 글자 그대로이고 대소문자를 가린다. 디스크의 파일 이름은 안 본다(20.9의 4번).
     */
    public static String pickFreeSlug(String base, Set<String> taken) {
        if (!taken.contains(base)) {
            return base;
        }
        for (int n = 2; n <= MAX_SLUG_SUFFIX; n++) {
            String candidate = base + "-" + n;
            if (!taken.contains(candidate)) {
                return candidate;
            }
        }
        return null;
    }

    /**
     * 20.5의 4번 색 고르기. {@code rows}의 색을 세어 가장 적게 쓰인 색을 고르고, 수가 같으면 20.3 표의 앞 차례가 이긴다.
     * 원색 6개가 먼저 다 쓰여서 아젠다가 적을 때 또렷한 색을 받는다.
     */
    public static String leastUsedHue(Collection<AgendaTable.AgendaInfo> rows) {
        List<String> used = new ArrayList<>(rows.size());
        for (AgendaTable.AgendaInfo row : rows) {
            used.add(row.hue());
        }
        return leastUsedHueOf(used);
    }

    /**
     * 색 목록 {@code usedHues}에서 가장 적게 쓰인 색을 고릅니다. 규칙은 {@link #leastUsedHue}와 같다.
     * 프로젝트(21.5)도 이 규칙으로 고르되 프로젝트 색만 센다. 프로젝트 색과 아젠다 색은 쓰는 자리가 달라 서로 독립이다.
     */
    public static String leastUsedHueOf(Collection<String> usedHues) {
        Map<String, Integer> counts = new LinkedHashMap<>();
        for (String hue : AgendaReader.HUES) {
            counts.put(hue, 0);
        }
        for (String used : usedHues) {
            // 파일에서 읽은 모르는 색은 이미 blue로 물러서 있다(2.1)
            String hue = counts.containsKey(used) ? used : "blue";
            counts.merge(hue, 1, Integer::sum);
        }
        String chosen = AgendaReader.HUES.get(0);
        int min = Integer.MAX_VALUE;
        for (String hue : AgendaReader.HUES) {
            int count = counts.get(hue);
            if (count < min) {
                min = count;
                chosen = hue;
            }
        }
        return chosen;
    }

    /**
     * 20.5의 쓰는 바이트를 짓습니다. 프론트매터 줄은 1.9의 규칙이고, {@code repo}와 {@code project}는 null이 아닐 때만 적는다.
     * 닫는 {@code ---\n} 바로 뒤에 {@code summary}를 받은 그대로 붙이고 끝에 줄바꿈을 더하지 않는다.
     * 줄바꿈을 1개 더하면 되돌리기 1번마다 파일이 1바이트씩 달라 보이기 때문이다.
     */
    public static byte[] render(String id, String slug, String title, String hue, String repo, String project, String summary) {
        String frontmatter = FrontmatterWriter.buildAgendaFrontmatter(id, slug, title, hue, repo, project);
        String content = "---\n" + frontmatter + "---\n" + (summary == null ? "" : summary);
        return Utf16.toUtf8BytesWithQuestionForUnpaired(content);
    }
}
