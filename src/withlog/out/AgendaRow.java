package withlog.out;

import withlog.text.Uri;
import withlog.vault.AgendaTable;

import java.nio.file.Path;
import java.util.LinkedHashMap;
import java.util.Map;

/**
 * 설명서 20.6의 아젠다 객체.
 * list-agendas, create-agenda, update-agenda가 같은 모양을 내서 부르는 쪽이 파서 1개로 읽는다.
 */
public final class AgendaRow {

    private AgendaRow() {}

    /**
     * 아젠다 1개를 20.6 표의 키 차례(id, slug, title, hue, repo, project, summary, deepLink)로 짓습니다.
     * repo와 project는 없으면 null이 아니라 키째 뺀다. 대부분의 아젠다에 없는 값이라 null로 실으면
     * 값이 있는 아젠다가 묻히기 때문이다.
     *
     * @param info      아젠다 값
     * @param vaultRoot 1.1에서 정리한 vault 뿌리
     * @return 아젠다 객체
     */
    public static Map<String, Object> toMap(AgendaTable.AgendaInfo info, Path vaultRoot) {
        Map<String, Object> row = new LinkedHashMap<>();
        row.put("id", info.id());
        row.put("slug", info.slug());
        row.put("title", info.title());
        row.put("hue", info.hue());
        if (info.repo() != null) {
            row.put("repo", info.repo());
        }
        if (info.project() != null) {
            row.put("project", info.project());
        }
        row.put("summary", info.summary() == null ? "" : info.summary());
        // 노트 링크와 달리 id가 아니라 slug를 싣는다. 링크를 만드는 쪽이 아는 값이 slug이기 때문이다
        row.put("deepLink", "withlog://agenda/" + Uri.encodeURIComponent(info.slug()) +
                "?ws=" + Uri.encodeURIComponent(vaultRoot.toString()));
        return row;
    }
}
