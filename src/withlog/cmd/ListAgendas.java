package withlog.cmd;

import withlog.out.AgendaRow;
import withlog.text.Utf16;
import withlog.vault.AgendaReader;
import withlog.vault.AgendaTable;

import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/**
 * 설명서 20장 list-agendas 명령. 아젠다 전부를 slug 차례로 냅니다.
 * 읽기라 워크스페이스 표식을 안 보고, 표식이 없는 폴더에서도 답한다.
 * 색인이 없는 판이라 부를 때마다 아젠다 파일을 2.1로 읽는다(8.6). 그래서 손으로 지운 아젠다는
 * 바로 빠지고, 손으로 넣은 아젠다는 바로 나온다.
 */
public final class ListAgendas {

    public static final String USAGE = "list-agendas";

    private ListAgendas() {}

    /**
     * list-agendas 명령을 실행합니다.
     *
     * @param vaultRoot vault 루트 경로
     * @return 20.6의 아젠다 객체 배열. 아젠다가 없으면 빈 배열
     */
    public static List<Map<String, Object>> execute(Path vaultRoot) {
        AgendaTable table = AgendaReader.scanAgendas(vaultRoot, false).table();
        // 색인 행은 id마다 1개라, 같은 id의 파일 2개는 이름 차례로 뒤 파일의 값 1개만 남는다(20.7)
        List<AgendaTable.AgendaInfo> rows = new ArrayList<>(table.idToAgenda().values());
        rows.sort((a, b) -> Utf16.compare(a.slug(), b.slug()));

        List<Map<String, Object>> result = new ArrayList<>(rows.size());
        for (AgendaTable.AgendaInfo row : rows) {
            result.add(AgendaRow.toMap(row, vaultRoot));
        }
        return result;
    }
}
