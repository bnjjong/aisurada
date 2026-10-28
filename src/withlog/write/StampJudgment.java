package withlog.write;

import java.time.Instant;
import java.time.ZoneId;
import java.time.ZonedDateTime;
import java.util.List;
import java.util.Map;

/**
 * 설명서 1.10 편집 도장 판정기.
 * Form A(옛 도장을 포함한 정규 직렬화)와 Form B(옛 파일 YAML 맵 전체의 1.9 재직렬화)를
 * 파일 글 전체(울타리, 프론트매터, 본문)로 비교하여,
 * 같으면 옛 도장을 보존하고 다르면 새 편집 도장(tool/app, 분 단위 +09:00, 세션 식별자)을 찍습니다.
 */
public final class StampJudgment {

    private StampJudgment() {}

    public record JudgmentResult(
            String fileText,
            boolean stampChanged,
            String finalEditedBy,
            String finalEditedAt,
            String finalEditedSessionId
    ) {}

    /**
     * 편집 도장 판정을 수행하고 최종 파일 텍스트를 반환합니다.
     */
    public static JudgmentResult judge(
            String format,
            String body,
            Map<?, ?> rawYamlMapFromFile,
            String id,
            String agenda,
            String type,
            String title,
            String created,
            String sessionId,
            int mins,
            String tier,
            String oldEditedBy,
            String oldEditedAt,
            String oldEditedSessionId,
            List<String> tags,
            boolean favorite,
            String favoriteCategory,
            String template,
            Map<String, Object> evaluation,
            List<Map<String, String>> links,
            boolean isToolSurface,
            String currentSessionId
    ) {
        String openFence;
        String closeFence;
        if ("html".equalsIgnoreCase(format)) {
            openFence = "<!--withlog\n";
            closeFence = "-->\n";
        } else {
            openFence = "---\n";
            closeFence = "---\n";
        }

        // 1. Form A: 옛 도장을 넣은 새 파일 글
        String frontmatterA = FrontmatterWriter.buildFullNoteFrontmatter(
                id, agenda, type, title, created, sessionId, mins, tier,
                oldEditedBy, oldEditedAt, oldEditedSessionId,
                tags, favorite, favoriteCategory, template, evaluation, links
        );
        String formA = openFence + frontmatterA + closeFence + body;

        // 2. Form B: 옛 파일의 YAML 맵 전체를 1.9로 다시 찍고, 울타리로 감싸고, 옛 본문을 붙임
        String frontmatterB = FrontmatterWriter.dumpGeneralMap(rawYamlMapFromFile);
        String formB = openFence + frontmatterB + closeFence + body;

        // 3. A와 B 맞대기
        if (formA.equals(formB)) {
            // 같으면 A를 쓰고 옛 도장이 남음
            return new JudgmentResult(formA, false, oldEditedBy, oldEditedAt, oldEditedSessionId);
        }

        // 4. 다르면 새 도장으로 다시 지어 씀
        String newEditedBy = isToolSurface ? "tool" : "app";
        Instant now = TestApparatus.nowInstant();
        ZonedDateTime kst = now.atZone(ZoneId.of("Asia/Seoul"));
        String newEditedAt = String.format("%04d-%02d-%02dT%02d:%02d:00+09:00",
                kst.getYear(), kst.getMonthValue(), kst.getDayOfMonth(), kst.getHour(), kst.getMinute());
        // 14.9의 17번: 표식 값은 안 다듬고 안 자른다. 빈 글이어도 그대로 적는다
        String newEditedSessionId = (isToolSurface && currentSessionId != null) ? currentSessionId : null;

        String frontmatterC = FrontmatterWriter.buildFullNoteFrontmatter(
                id, agenda, type, title, created, sessionId, mins, tier,
                newEditedBy, newEditedAt, newEditedSessionId,
                tags, favorite, favoriteCategory, template, evaluation, links
        );
        String formC = openFence + frontmatterC + closeFence + body;

        return new JudgmentResult(formC, true, newEditedBy, newEditedAt, newEditedSessionId);
    }
}
