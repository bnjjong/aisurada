package aisurada;

import aisurada.vault.Frontmatter;
import aisurada.write.FrontmatterWriter;

import java.util.*;

/**
 * [R0] YAML 왕복(Round-trip) 시험 그물.
 * FrontmatterWriter가 쓴 YAML을 SnakeYAML(Frontmatter.parse)이 읽었을 때
 * 타입과 내용이 훼손되지 않고 원본 그대로 보존되는지 검증합니다.
 */
public final class YamlRoundTripTest {
    private static int checks = 0;

    public static void main(String[] args) {
        testB7UnderlineNumbers();
        testB2FoldedBlockNewlines();
        testB2TemplateDescMultipleRoundTrips();
        testGeneralRoundTrip();
        System.out.println("YamlRoundTripTest: " + checks + " checks passed.");
    }

    /** 템플릿 설명이 여러 번 저장-파싱되어도 끝 줄바꿈이나 내용이 매 회차 늘어나지 않는지 검증 */
    private static void testB2TemplateDescMultipleRoundTrips() {
        String originalDesc = "이것은 템플릿의 설명입니다.\n두 번째 줄 설명입니다.";
        String currentDesc = originalDesc;

        for (int cycle = 1; cycle <= 5; cycle++) {
            Map<String, Object> map = new LinkedHashMap<>();
            map.put("id", "tpl-00000001");
            map.put("desc", currentDesc);

            String dumped = FrontmatterWriter.dumpGeneralMap(map);
            Map<String, Object> loaded = Frontmatter.parse(dumped);
            check(loaded != null, "loaded must not be null at cycle " + cycle);
            Object loadedDesc = loaded.get("desc");
            check(loadedDesc instanceof String, "desc must be string at cycle " + cycle);
            currentDesc = (String) loadedDesc;
            check(originalDesc.equals(currentDesc), "desc must stay exactly identical after cycle " + cycle + ", but was: [" + currentDesc + "]");
        }
    }

    private static void check(boolean condition, String message) {
        checks++;
        if (!condition) {
            throw new AssertionError("CHECK FAILED: " + message);
        }
    }

    /** B7: 2026_11_10, 1_000 같은 밑줄 숫자가 정수로 변환되거나 태그가 버려지지 않는지 검증 */
    private static void testB7UnderlineNumbers() {
        Map<String, Object> map = new LinkedHashMap<>();
        map.put("id", "ndfixture-00000001");
        map.put("title", "2026_11_10");
        map.put("tags", List.of("2026_11", "1_000", "0b101", "0xdead_beef"));
        map.put("date_str", "2026-11-10");

        String dumped = FrontmatterWriter.dumpGeneralMap(map);
        System.out.println("--- Dumped B7 YAML ---\n" + dumped + "----------------------");

        Map<String, Object> loaded = Frontmatter.parse(dumped);
        check(loaded != null, "loaded map must not be null");

        // title 검증
        Object title = loaded.get("title");
        check(title instanceof String, "title must remain String but was: " + (title == null ? "null" : title.getClass().getName()) + " (" + title + ")");
        check("2026_11_10".equals(title), "title value mismatch: " + title);

        // tags 검증
        Object tagsObj = loaded.get("tags");
        check(tagsObj instanceof List<?>, "tags must be List");
        List<?> tags = (List<?>) tagsObj;
        check(tags.get(0) instanceof String && "2026_11".equals(tags.get(0)), "tag 0 must be String '2026_11': " + tags.get(0));
        check(tags.get(1) instanceof String && "1_000".equals(tags.get(1)), "tag 1 must be String '1_000': " + tags.get(1));
        check(tags.get(2) instanceof String && "0b101".equals(tags.get(2)), "tag 2 must be String '0b101': " + tags.get(2));
        check(tags.get(3) instanceof String && "0xdead_beef".equals(tags.get(3)), "tag 3 must be String '0xdead_beef': " + tags.get(3));
    }

    /** B2: 여러 줄 값 중 한 줄이 78자를 넘을 때 줄바꿈이 공백으로 접히지 않는지 검증 */
    private static void testB2FoldedBlockNewlines() {
        String longLine = "This is a very long line that definitely exceeds 78 characters in length so that it triggers foldLine in FrontmatterWriter";
        String multiLine = longLine + "\nsecond line\nthird line";

        Map<String, Object> map = new LinkedHashMap<>();
        map.put("id", "ndfixture-00000002");
        map.put("note", multiLine);

        String dumped = FrontmatterWriter.dumpGeneralMap(map);
        System.out.println("--- Dumped B2 YAML ---\n" + dumped + "----------------------");

        Map<String, Object> loaded = Frontmatter.parse(dumped);
        check(loaded != null, "loaded map must not be null");

        Object noteObj = loaded.get("note");
        check(noteObj instanceof String, "note must be String");
        String noteStr = (String) noteObj;

        // 원본 줄바꿈이 살아있어야 함
        check(noteStr.contains("second line"), "note must contain second line");
        check(noteStr.contains("third line"), "note must contain third line");
        check(noteStr.contains("\nsecond line"), "newline before 'second line' must be preserved, but was: " + noteStr);
    }

    /** 일반적인 Frontmatter 왕복 검증 */
    private static void testGeneralRoundTrip() {
        Map<String, Object> map = new LinkedHashMap<>();
        map.put("id", "ndfixture-00000003");
        map.put("title", "Normal Title");
        map.put("pinned", true);
        map.put("score", 100);
        map.put("tags", List.of("tag1", "tag2"));

        String dumped = FrontmatterWriter.dumpGeneralMap(map);
        Map<String, Object> loaded = Frontmatter.parse(dumped);
        check(loaded != null, "loaded must not be null");
        check("Normal Title".equals(loaded.get("title")), "title match");
        check(Boolean.TRUE.equals(loaded.get("pinned")), "pinned match");
        check(Integer.valueOf(100).equals(loaded.get("score")), "score match");
    }
}
