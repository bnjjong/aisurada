package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.engine.WritePipeline;
import withlog.out.ProjectRow;
import withlog.text.Trim;
import withlog.vault.ProjectReader;
import withlog.write.AgendaFile;
import withlog.write.AtomicWriter;
import withlog.write.IdGenerator;
import withlog.write.PathConfinement;
import withlog.write.ProjectFile;
import withlog.write.SlugBuilder;

import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * 설명서 21장 create-project 명령. 프로젝트 1개를 새로 만듭니다.
 * create-agenda(20.5)와 같은 차례로 일하고, 이름으로 먼저 찾아보는 단계가 없어서 같은 이름으로 2번 만들면
 * 2번째가 slug에 -2를 단다.
 */
public final class CreateProject {

    public static final String USAGE = "create-project title=<이름> [hue=<색>] [summary=<한 문단 설명>]";

    private CreateProject() {}

    /**
     * create-project 명령을 실행합니다. 검사 차례는 21.3, 일하는 차례는 21.5입니다.
     *
     * @param vaultRoot vault 루트 경로
     * @param parsed    파싱된 인자
     * @return 21.6의 프로젝트 객체
     */
    public static Map<String, Object> execute(Path vaultRoot, ParsedArgs parsed) {
        // 3. 워크스페이스 표식, 4. 색인 맞추기 (21.3). 맞추기는 프로젝트를 안 다루고 표식 확인이 깃발의 주된 값이다
        WritePipeline.index(vaultRoot);

        // 5. 칸 검사: title, hue 차례. 이름에 길이 상한은 없다
        Map<String, String> opts = parsed.namedOptions();
        String title = Trim.trim(opts.get("title"));
        if (title.isEmpty()) {
            throw new WlException(400, "title 이 필요하다");
        }
        String hue = AgendaArgs.hue(opts.get("hue"));
        String summary = opts.getOrDefault("summary", "");

        // 21.5: 프로젝트 행 전부를 1번 읽어 slug 비켜서기와 색 고르기에 쓴다
        Map<String, ProjectReader.ProjectInfo> projects = ProjectReader.scanProjects(vaultRoot);

        // create-agenda처럼 slug보다 먼저 id를 발급한다(20.5의 5번)
        String id = IdGenerator.nextProjectId();

        // 7. 명령의 규칙: slug 비켜서기. 빈자리는 프로젝트 행으로 보고 파일 이름은 안 본다
        Set<String> taken = new HashSet<>();
        List<String> usedHues = new ArrayList<>(projects.size());
        for (ProjectReader.ProjectInfo row : projects.values()) {
            taken.add(row.slug());
            usedHues.add(row.hue());
        }
        String slug = AgendaFile.pickFreeSlug(SlugBuilder.build(title, 50), taken);
        if (slug == null) {
            throw new WlException(400, "같은 이름의 프로젝트가 " + AgendaFile.MAX_SLUG_SUFFIX +
                    " 개까지 차 있어 새로 만들 수 없다: " + title);
        }
        if (hue == null) {
            hue = AgendaFile.leastUsedHueOf(usedHues);
        }

        // 8. 파일 쓰기. 50단위 자르기가 서로게이트 쌍을 가른 slug는 경로를 못 지어 아무것도 안 쓴다(21.9의 8번)
        String fileName = slug + ".md";
        if (PathConfinement.hasUnusablePathCharacters(fileName)) {
            throw new WlException(400, "경로로 쓸 수 없는 값이다: " + fileName);
        }
        AtomicWriter.writeAtomic(vaultRoot.resolve(ProjectFile.relPath(slug)),
                ProjectFile.render(id, slug, title, hue, summary));

        // 색인 행 넣기는 색인이 없는 판이라 할 일이 없다. 다음 호출이 이 파일을 읽는다(8.6)
        // 응답은 방금 쓴 값에서 짓는다. 그래서 summary는 받은 글 그대로다(21.6)
        return ProjectRow.toMap(new ProjectReader.ProjectInfo(id, slug, title, hue, summary));
    }
}
