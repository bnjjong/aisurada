package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.engine.WritePipeline;
import withlog.vault.AgendaTable;
import withlog.vault.NoteIndex;
import withlog.vault.ProjectReader;
import withlog.write.ProjectFile;
import withlog.write.TrashArchive;

import java.nio.file.Path;
import java.util.LinkedHashMap;
import java.util.Map;

/**
 * 설명서 21장 delete-project 명령. 아젠다가 없는 프로젝트의 파일을 휴지통 폴더로 옮깁니다.
 * 프로젝트는 노트를 직접 안 들어서 아젠다의 「노트 0건」 가드 대신 속한 아젠다 수로 막는다.
 * 속한 아젠다를 함께 지우거나 미분류로 옮기지 않는다. 프로젝트에는 휴지통 기록이 없어 되돌릴 길이 없기 때문이다.
 */
public final class DeleteProject {

    public static final String USAGE = "delete-project <프로젝트 id>";

    private DeleteProject() {}

    /**
     * delete-project 명령을 실행합니다. 검사 차례는 21.3, 일하는 차례는 21.5의 지우기 1번부터 5번입니다.
     *
     * @param vaultRoot vault 루트 경로
     * @param parsed    파싱된 인자
     * @return 21.6의 지우기 응답(id, slug, archivedPath)
     */
    public static Map<String, Object> execute(Path vaultRoot, ParsedArgs parsed) {
        // 3. 워크스페이스 표식, 4. 색인 맞추기 (21.3). 아젠다 수를 셀 행을 이 맞추기가 새로 고친다
        NoteIndex noteIndex = WritePipeline.index(vaultRoot);

        // 5. 칸 검사: id 있음, id 모양
        String id = ProjectArgs.requireId(parsed, USAGE);

        // 1. id로 찾는다
        ProjectReader.ProjectInfo project = ProjectArgs.find(ProjectReader.scanProjects(vaultRoot), id);
        String slug = project.slug();

        // 2, 3. 그 slug를 가리키는 아젠다를 센다
        int agendas = countAgendas(noteIndex, slug);
        if (agendas > 0) {
            throw new WlException(400, "아젠다가 " + agendas + "건 있는 프로젝트는 지울 수 없다. 먼저 그 아젠다를 옮긴다: " + slug);
        }

        // 4. 색인 행 지우기는 색인이 없는 판이라 할 일이 없다
        // 5. 파일 옮기기. 걸음은 아젠다와 같다(20.5의 5-1부터 5-5)
        String archivedRel = ProjectFile.archivedRelPath(slug, id);
        TrashArchive.move(vaultRoot.resolve(ProjectFile.relPath(slug)), vaultRoot.resolve(archivedRel), "프로젝트");

        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("id", id);
        resp.put("slug", slug);
        // 옮길 파일이 없었어도 같은 글을 싣는다. 지운 그 자리에서 알려 주지 않으면 되살릴 길을 잃는다
        resp.put("archivedPath", archivedRel);
        return resp;
    }

    /**
     * 방금 맞춘 아젠다 행 가운데 프로젝트 slug가 {@code slug}와 글자 그대로 같은 것을 셉니다. 대소문자를 가린다.
     * 파일의 project가 slug 모양이 아니어서 미분류로 읽힌 아젠다는 안 센다(21.9의 6번).
     */
    private static int countAgendas(NoteIndex noteIndex, String slug) {
        int count = 0;
        for (AgendaTable.AgendaInfo agenda : noteIndex.agendaTable().idToAgenda().values()) {
            if (slug.equals(agenda.project())) {
                count++;
            }
        }
        return count;
    }
}
