package withlog.cmd;

import withlog.out.ProjectRow;
import withlog.text.Utf16;
import withlog.vault.ProjectReader;

import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/**
 * 설명서 21장 list-projects 명령. 프로젝트 전부를 냅니다.
 * 읽기라 워크스페이스 표식을 안 보고 색인도 안 맞춘다.
 * 색인이 없는 판이라 부를 때마다 프로젝트 파일을 2.2로 읽는다(8.6). 그래서 손으로 넣은 파일도 나온다.
 */
public final class ListProjects {

    public static final String USAGE = "list-projects";

    private ListProjects() {}

    /**
     * list-projects 명령을 실행합니다. 차례는 id 차례다.
     * 지금 엔진은 이 색인 DB에서 처음 만든 차례로 내는데(21.6), id 앞부분이 발급 순간의 36진수 밀리초라(1.4)
     * 엔진이 발급한 id끼리는 id 차례가 만든 차례와 같다. 고쳐도 자리가 안 바뀌고, 지운 뒤 새로 만든 프로젝트는 끝에 붙는다.
     *
     * @param vaultRoot vault 루트 경로
     * @return 21.6의 프로젝트 객체 배열. 프로젝트가 없으면 빈 배열
     */
    public static List<Map<String, Object>> execute(Path vaultRoot) {
        List<ProjectReader.ProjectInfo> rows = new ArrayList<>(ProjectReader.scanProjects(vaultRoot).values());
        rows.sort((a, b) -> Utf16.compare(a.id(), b.id()));

        List<Map<String, Object>> result = new ArrayList<>(rows.size());
        for (ProjectReader.ProjectInfo row : rows) {
            result.add(ProjectRow.toMap(row));
        }
        return result;
    }
}
