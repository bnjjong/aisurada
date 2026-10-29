package withlog.out;

import withlog.vault.ProjectReader;

import java.util.LinkedHashMap;
import java.util.Map;

/**
 * 설명서 21.6의 프로젝트 객체. list-projects, create-project, update-project가 같은 모양을 낸다.
 */
public final class ProjectRow {

    private ProjectRow() {}

    /**
     * 프로젝트 1개를 21.6 표의 키 차례(id, slug, title, hue, summary)로 짓습니다. summary는 빈 글이어도 싣는다.
     * deepLink가 없다. 프로젝트를 여는 화면과 라우트가 아직 없어서 눌러도 아무 일이 안 나는 링크를 싣지 않는다.
     * 속한 아젠다도 안 싣는다. 묶는 일은 부르는 쪽이 아젠다 응답의 project와 이 slug를 맞대어 한다.
     *
     * @param info 프로젝트 값
     * @return 프로젝트 객체
     */
    public static Map<String, Object> toMap(ProjectReader.ProjectInfo info) {
        Map<String, Object> row = new LinkedHashMap<>();
        row.put("id", info.id());
        row.put("slug", info.slug());
        row.put("title", info.title());
        row.put("hue", info.hue());
        row.put("summary", info.summary() == null ? "" : info.summary());
        return row;
    }
}
