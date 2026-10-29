package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.out.AgendaRow;
import withlog.text.Trim;
import withlog.vault.AgendaReader;
import withlog.vault.AgendaTable;
import withlog.vault.NoteIndex;
import withlog.write.AgendaFile;
import withlog.write.AtomicWriter;
import withlog.write.IdGenerator;
import withlog.write.PathConfinement;
import withlog.write.SlugBuilder;
import withlog.engine.WritePipeline;

import java.nio.file.Path;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * 설명서 20장 create-agenda 명령. 아젠다 1개를 새로 만듭니다.
 * 같은 이름의 아젠다가 있어도 찾아 주지 않고 늘 새로 만들고 slug만 비켜선다. 이름을 친 사람에게
 * 남의 아젠다를 돌려주면 방금 한 생성이 사라진 것처럼 보이기 때문이다. 찾아 만드는 길은 create_doc에 있다(20.8).
 */
public final class CreateAgenda {

    public static final String USAGE = "create-agenda title=<이름> [hue=<색>] [repo=<저장소>] " +
            "[summary=<한 문단 설명>] [project=<프로젝트 slug>]";

    private CreateAgenda() {}

    /**
     * create-agenda 명령을 실행합니다. 검사 차례는 20.3, 일하는 차례는 20.5입니다.
     *
     * @param vaultRoot vault 루트 경로
     * @param parsed    파싱된 인자
     * @return 20.6의 아젠다 객체
     */
    public static Map<String, Object> execute(Path vaultRoot, ParsedArgs parsed) {
        // 3. 워크스페이스 표식, 4. 색인 맞추기 (20.3)

        NoteIndex noteIndex = WritePipeline.index(vaultRoot);

        // 5. 칸 검사: title, hue, project 차례. 칸마다 앞에서 걸리면 뒤는 안 본다
        Map<String, String> opts = parsed.namedOptions();
        String title = Trim.trim(opts.get("title"));
        if (title.isEmpty()) {
            throw new WlException(400, "title 이 필요하다");
        }
        String hue = AgendaArgs.hue(opts.get("hue"));
        String project = Trim.trim(opts.get("project"));
        if (project.isEmpty()) {
            // create에서 공백뿐인 project는 안 준 것이고 미분류다(20.9의 7번)
            project = null;
        } else if (!AgendaReader.isValidSlugSegment(project)) {
            throw new WlException(400, "project 값이 안전한 이름이 아니다: " + project);
        }
        String repo = Trim.trim(opts.get("repo"));
        if (repo.isEmpty()) {
            repo = null;
        }
        String summary = opts.getOrDefault("summary", "");

        // 20.5의 1번: slug 비켜서기와 색 고르기가 같은 행 목록을 본다
        List<AgendaTable.AgendaInfo> rows = List.copyOf(noteIndex.agendaTable().idToAgenda().values());

        // 20.5의 5번: 지금 엔진은 slug보다 먼저 id를 발급한다. 실패하면 버려져서 바깥에서 안 보인다
        String id = IdGenerator.nextAgendaId();

        // 7. 명령의 규칙: slug 비켜서기 (20.5의 2번, 3번)
        Set<String> taken = new HashSet<>();
        for (AgendaTable.AgendaInfo row : rows) {
            taken.add(row.slug());
        }
        String slug = AgendaFile.pickFreeSlug(SlugBuilder.build(title, 50), taken);
        if (slug == null) {
            throw new WlException(400, "같은 이름의 아젠다가 " + AgendaFile.MAX_SLUG_SUFFIX +
                    " 개까지 차 있어 새로 만들 수 없다: " + title);
        }
        if (hue == null) {
            hue = AgendaFile.leastUsedHue(rows);
        }

        // 8. 파일 쓰기. 50단위 자르기가 서로게이트 쌍을 가른 slug는 경로를 못 지어 아무것도 안 쓴다(20.9의 11번)
        String fileName = slug + ".md";
        if (PathConfinement.hasUnusablePathCharacters(fileName)) {
            throw new WlException(400, "경로로 쓸 수 없는 값이다: " + fileName);
        }
        AtomicWriter.writeAtomic(vaultRoot.resolve(AgendaFile.relPath(slug)),
                AgendaFile.render(id, slug, title, hue, repo, project, summary));

        // 색인 행 넣기는 색인이 없는 판이라 할 일이 없다. 다음 호출이 이 파일을 읽는다(8.6)
        // 응답은 디스크를 다시 읽지 않고 방금 쓴 값에서 짓는다. 그래서 summary는 받은 글 그대로다(20.6)
        return AgendaRow.toMap(new AgendaTable.AgendaInfo(id, slug, title, hue, repo, project, summary), vaultRoot);
    }
}
