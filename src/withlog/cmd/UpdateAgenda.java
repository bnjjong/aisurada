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
import withlog.engine.WritePipeline;

import java.nio.file.Path;
import java.util.Map;

/**
 * 설명서 20장 update-agenda 명령. 준 칸만 바꾸고 아젠다 파일을 통째로 다시 씁니다.
 * 안 준 칸을 기본값으로 채우지 않는 것이 계약이다. 채우면 이 명령이 안 받는 값이 조용히 초기화된다.
 * slug와 id는 그대로 나른다. 노트가 아젠다에 slug로 묶이는 자리가 여럿이라 이름을 바꿔도 파일 이름은 그대로다.
 */
public final class UpdateAgenda {

    public static final String USAGE = "update-agenda <아젠다 id> [title=<이름>] [hue=<색>] " +
            "[repo=<저장소. 빈 값이면 지운다>] [summary=<한 문단 설명>] [project=<프로젝트 slug. 빈 값이면 미분류로>]";

    private UpdateAgenda() {}

    /**
     * update-agenda 명령을 실행합니다. 검사 차례는 20.3, 바탕 값은 방금 맞춘 색인(부르기 직전의 파일을 2.1로 읽은 값)입니다.
     *
     * @param vaultRoot vault 루트 경로
     * @param parsed    파싱된 인자
     * @return 20.6의 아젠다 객체
     */
    public static Map<String, Object> execute(Path vaultRoot, ParsedArgs parsed) {
        // 3. 워크스페이스 표식, 4. 색인 맞추기 (20.3)

        NoteIndex noteIndex = WritePipeline.index(vaultRoot);

        // 5. 칸 검사: id 있음, id 모양, hue, project 차례
        Map<String, String> opts = parsed.namedOptions();
        String id = AgendaArgs.requireId(parsed, USAGE);
        String hue = AgendaArgs.hue(opts.get("hue"));
        // project는 3갈래다. 안 줌은 지금 소속을 두고, 공백뿐이면 미분류로 옮긴다(20.9의 7번)
        boolean projectGiven = opts.containsKey("project");
        String project = null;
        if (projectGiven) {
            project = Trim.trim(opts.get("project"));
            if (project.isEmpty()) {
                project = null;
            } else if (!AgendaReader.isValidSlugSegment(project)) {
                throw new WlException(400, "프로젝트 slug 가 안전한 경로 세그먼트가 아니다: " + project);
            }
        }

        // 6. id로 아젠다 찾기
        AgendaTable.AgendaInfo current = AgendaArgs.find(noteIndex, id);

        // 준 칸만 지금 값에 얹는다. title은 공백뿐이면 안 준 것이라 이름을 비우는 길이 없다
        String title = Trim.trim(opts.get("title"));
        if (title.isEmpty()) {
            title = current.title();
        }
        if (hue == null) {
            hue = current.hue();
        }
        // repo도 3갈래다. 지울 수 있는 값이고 지우는 길이 빈 값을 주는 것뿐이기 때문이다
        String repo = current.repo();
        if (opts.containsKey("repo")) {
            repo = Trim.trim(opts.get("repo"));
            if (repo.isEmpty()) {
                repo = null;
            }
        }
        if (!projectGiven) {
            project = current.project();
        }
        // summary는 다듬지 않는다. 아젠다 파일의 본문이라 사용자가 넣은 끝 줄바꿈까지 그 사람의 글이다
        String summary = opts.containsKey("summary") ? opts.get("summary") : current.summary();

        // 8. 파일을 통째로 다시 쓴다. 칸을 1개도 안 줘도 쓴다. 모르는 키(옛 status: 같은 것)는 여기서 사라진다
        String slug = current.slug();
        AtomicWriter.writeAtomic(vaultRoot.resolve(AgendaFile.relPath(slug)),
                AgendaFile.render(id, slug, title, hue, repo, project, summary));

        return AgendaRow.toMap(new AgendaTable.AgendaInfo(id, slug, title, hue, repo, project, summary), vaultRoot);
    }
}
