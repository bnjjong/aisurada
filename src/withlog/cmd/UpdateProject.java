package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.engine.WritePipeline;
import withlog.out.ProjectRow;
import withlog.text.Trim;
import withlog.vault.ProjectReader;
import withlog.write.AtomicWriter;
import withlog.write.ProjectFile;

import java.nio.file.Path;
import java.util.Map;

/**
 * 설명서 21장 update-project 명령. 준 칸만 바꾸고 프로젝트 파일을 통째로 다시 씁니다.
 * slug를 바꾸는 인자는 없다. 아젠다가 그 값으로 이 프로젝트를 가리켜서, 바꾸려면 가리키는 아젠다를
 * 모두 함께 고쳐야 하고 그 일은 되돌릴 자리가 없기 때문이다(21.3).
 */
public final class UpdateProject {

    public static final String USAGE = "update-project <프로젝트 id> [title=<이름>] [hue=<색>] [summary=<한 문단 설명>]";

    private UpdateProject() {}

    /**
     * update-project 명령을 실행합니다. 검사 차례는 21.3이고, 바탕 값은 부르기 직전의 파일을 2.2로 읽은 값입니다.
     *
     * @param vaultRoot vault 루트 경로
     * @param parsed    파싱된 인자
     * @return 21.6의 프로젝트 객체
     */
    public static Map<String, Object> execute(Path vaultRoot, ParsedArgs parsed) {
        // 3. 워크스페이스 표식, 4. 색인 맞추기 (21.3)
        WritePipeline.index(vaultRoot);

        // 5. 칸 검사: id 있음, id 모양, hue 차례
        Map<String, String> opts = parsed.namedOptions();
        String id = ProjectArgs.requireId(parsed, USAGE);
        String hue = AgendaArgs.hue(opts.get("hue"));

        // 6. id로 프로젝트 찾기
        ProjectReader.ProjectInfo current = ProjectArgs.find(ProjectReader.scanProjects(vaultRoot), id);

        // 준 칸만 지금 값에 얹는다. title은 공백뿐이면 안 준 것이라 이름을 비우는 길이 없다
        String title = Trim.trim(opts.get("title"));
        if (title.isEmpty()) {
            title = current.title();
        }
        if (hue == null) {
            hue = current.hue();
        }
        // summary는 다듬지 않는다. 빈 글을 주면 본문을 비운다
        String summary = opts.containsKey("summary") ? opts.get("summary") : current.summary();

        // 8. 파일을 통째로 다시 쓴다. 칸을 1개도 안 줘도 쓴다. 모르는 키는 여기서 사라진다
        String slug = current.slug();
        AtomicWriter.writeAtomic(vaultRoot.resolve(ProjectFile.relPath(slug)),
                ProjectFile.render(id, slug, title, hue, summary));

        return ProjectRow.toMap(new ProjectReader.ProjectInfo(id, slug, title, hue, summary));
    }
}
