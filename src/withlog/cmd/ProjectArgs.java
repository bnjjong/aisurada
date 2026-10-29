package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.vault.ProjectReader;
import withlog.write.IdGenerator;

import java.util.List;
import java.util.Map;

/**
 * 설명서 21.3의 칸 검사 가운데 프로젝트 명령 여럿이 함께 쓰는 것. 프로젝트 id 칸과 id로 찾기다.
 * hue 칸은 아젠다와 같은 글이라 {@link AgendaArgs#hue}를 쓴다.
 */
final class ProjectArgs {

    private ProjectArgs() {}

    /**
     * 첫 자리 인자에서 프로젝트 id를 꺼냅니다. 판정은 아젠다 id(20.3)와 같다.
     * 없거나 공백뿐이면 「인자가 하나 필요하다」이고, 공백뿐이 아닌 id는 다듬지 않아서 앞뒤 공백도 모양 오류다.
     *
     * @param parsed 파싱된 인자
     * @param usage  그 명령의 사용법 1줄
     * @return 모양 검사를 지난 프로젝트 id
     */
    static String requireId(ParsedArgs parsed, String usage) {
        List<String> pos = parsed.positionalArgs();
        if (pos.isEmpty() || Trim.isBlank(pos.get(0))) {
            throw new WlException(400, "인자가 하나 필요하다: " + usage);
        }
        String id = pos.get(0);
        if (!IdGenerator.isValidProjectId(id)) {
            throw new WlException(400, "프로젝트 id 모양이 아니다: " + id);
        }
        return id;
    }

    /**
     * 프로젝트 행에서 id로 찾습니다. 없으면 WL-404입니다.
     */
    static ProjectReader.ProjectInfo find(Map<String, ProjectReader.ProjectInfo> projects, String id) {
        ProjectReader.ProjectInfo info = projects.get(id);
        if (info == null) {
            throw new WlException(404, "Project not found: " + id);
        }
        return info;
    }
}
