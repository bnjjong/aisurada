package withlog.vault;

import java.util.Map;

/**
 * 아젠다 훑기 결과 및 slug 매핑 테이블 DTO.
 * @param slugToId slug -> agendaId 매핑
 * @param slugToTitle slug -> agendaTitle 매핑
 * @param idToAgenda id -> AgendaInfo 매핑
 * @param slugToAgenda slug -> AgendaInfo 매핑
 */
public record AgendaTable(
        Map<String, String> slugToId,
        Map<String, String> slugToTitle,
        Map<String, AgendaInfo> idToAgenda,
        Map<String, AgendaInfo> slugToAgenda
) {
    /**
     * 아젠다 상세 정보 DTO.
     */
    public record AgendaInfo(
            String id,
            String slug,
            String title,
            String hue,
            String repo,
            String project,
            String summary
    ) {}
}
