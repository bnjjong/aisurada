package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.vault.AgendaTable;
import withlog.vault.NoteIndex;
import withlog.vault.NoteScanner;
import withlog.write.AgendaFile;
import withlog.write.WorkspaceMarker;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.util.LinkedHashMap;
import java.util.Map;

/**
 * 설명서 20장 delete-agenda 명령. 노트가 없는 아젠다의 파일을 지우지 않고 휴지통 폴더로 옮깁니다.
 * 옮긴 이름에 아젠다 id를 붙여서, 같은 이름의 아젠다를 지우고 다시 만들어 또 지워도 옮긴 이름이 안 겹친다.
 * notes/&lt;slug&gt;/ 폴더와 사실층 파일은 비어 있어도 안 지운다(20.9의 9번).
 */
public final class DeleteAgenda {

    public static final String USAGE = "delete-agenda <아젠다 id>";

    private DeleteAgenda() {}

    /**
     * delete-agenda 명령을 실행합니다. 검사 차례는 20.3, 일하는 차례는 20.5의 지우기 1번부터 5번입니다.
     *
     * @param vaultRoot vault 루트 경로
     * @param parsed    파싱된 인자
     * @return 20.6의 지우기 응답(id, slug, archivedPath, notesDir)
     */
    public static Map<String, Object> execute(Path vaultRoot, ParsedArgs parsed) {
        // 3. 워크스페이스 표식, 4. 색인 맞추기 (20.3)
        WorkspaceMarker.check(vaultRoot);
        NoteIndex noteIndex = NoteIndex.reconciled(vaultRoot);

        // 5. 칸 검사: id 있음, id 모양
        String id = AgendaArgs.requireId(parsed, USAGE);

        // 1. id로 찾는다. 없는 id면 노트를 세기 전에 끝낸다
        AgendaTable.AgendaInfo agenda = AgendaArgs.find(noteIndex, id);

        // 2, 3. 그 아젠다를 가리키는 노트를 센다. 층을 안 가리고, 하위 노트는 루트에 딸려서 따로 안 센다
        int notes = countNotes(noteIndex, id);
        if (notes > 0) {
            throw new WlException(400, "노트가 " + notes + "건 있는 아젠다는 지울 수 없다. 먼저 그 노트를 옮기거나 버린다: "
                    + agenda.slug());
        }

        // 4. 색인 행 지우기는 색인이 없는 판이라 할 일이 없다
        // 5. 파일 옮기기
        String slug = agenda.slug();
        String archivedRel = ".withlog/trash/agendas/" + slug + "-" + id + ".md";
        moveToTrash(vaultRoot.resolve(AgendaFile.relPath(slug)), vaultRoot.resolve(archivedRel));

        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("id", id);
        resp.put("slug", slug);
        // 옮길 파일이 없었어도 같은 글을 싣는다. 되살릴 자리를 알리는 것은 이 칸 1개뿐이다
        resp.put("archivedPath", archivedRel);
        // 엔진이 그 폴더를 안 지워서 부르는 쪽이 정리할지 정하려면 자리를 알아야 한다. 있는지는 안 본다
        resp.put("notesDir", "notes/" + slug);
        return resp;
    }

    /**
     * 이번에 훑은 루트 노트 가운데 아젠다가 {@code agendaId}인 것을 셉니다. 같은 id의 아젠다 파일 2개가
     * 서로 다른 slug를 들면 그 폴더 2곳의 노트가 모두 그 아젠다에 속한다(20.7).
     */
    private static int countNotes(NoteIndex noteIndex, String agendaId) {
        Map<String, String> slugToId = noteIndex.agendaTable().slugToId();
        int count = 0;
        for (NoteScanner.NoteRecord note : noteIndex.notesById().values()) {
            if (note.meta() != null && agendaId.equals(slugToId.get(note.meta().agendaSlug()))) {
                count++;
            }
        }
        return count;
    }

    /**
     * 20.5 지우기 5번의 걸음 5-1부터 5-5입니다. 원래 자리가 비었으면 경고 로그만 남기고 성공한다.
     * 사람이 파일을 손으로 지웠어도 그 아젠다를 지울 길이 있어야 하기 때문이다.
     */
    private static void moveToTrash(Path source, Path target) {
        // 5-1, 5-2: 링크는 따라가서 본다. 끊어진 링크는 「없음」이라 그 자리에 남는다
        if (!Files.exists(source)) {
            System.err.println("Warning: 옮길 아젠다 파일이 없어 옮기지 않았다: " + source);
            return;
        }
        // 5-3: 옮길 자리가 차 있으면 덮지 않는다
        if (Files.exists(target)) {
            throw new WlException(500, "옮길 자리에 이미 무엇이 있어 옮기지 않았다: " + target);
        }
        // 5-4: 휴지통 폴더를 위쪽까지 만든다. 실패한 조각이 위여도 만들려던 폴더 전체를 찍는다
        Path trashDir = target.getParent();
        try {
            Files.createDirectories(trashDir);
        } catch (IOException e) {
            throw new WlException(500, "폴더를 만들지 못했다: " + trashDir);
        }
        // 5-5: 이름 바꾸기 1번. 폴더는 폴더째, 심볼릭 링크는 링크 자신을 옮긴다
        try {
            Files.move(source, target, StandardCopyOption.ATOMIC_MOVE);
        } catch (IOException e) {
            throw new WlException(500, "옮기지 못했다: " + source + " -> " + target);
        }
    }
}
