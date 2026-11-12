package aisurada.cmd;

import aisurada.args.ParsedArgs;
import aisurada.cli.AisException;
import aisurada.text.Trim;
import aisurada.vault.AgendaReader;
import aisurada.vault.AgendaTable;
import aisurada.vault.NoteIndex;
import aisurada.vault.ProjectReader;
import aisurada.vault.ProjectRepo;
import aisurada.write.IdGenerator;

import java.nio.file.Path;
import java.util.List;

/**
 * 설명서 20.3의 칸 검사 가운데 아젠다 명령 여럿이 함께 쓰는 것.
 * 아젠다 id 칸, hue 칸, 색인에서 id로 찾기다.
 */
final class AgendaArgs {

    private AgendaArgs() {}

    /**
     * 확장: projectRepo 칸. 안 줬거나 공백뿐이면 null(가리키는 저장소 없음)이다. 이름을 주면 project가 있어야 하고, 그 프로젝트의
     * {@code repos}에 그 이름이 있어야 한다. 프로젝트 파일은 색인에 없어서 부를 때마다 파일에서 읽는다.
     *
     * @param project 이 아젠다가 속할(속한) 프로젝트 slug. 미분류면 null
     */
    static String projectRepo(Path vaultRoot, String project, String raw) {
        if (raw == null) {
            return null;
        }
        String name = Trim.trim(raw);
        if (name.isEmpty()) {
            return null;
        }
        if (project == null) {
            throw new AisException(400, "projectRepo 는 project 와 함께 줘야 한다: " + name);
        }
        ProjectReader.ProjectInfo found = null;
        for (ProjectReader.ProjectInfo info : ProjectReader.scanProjects(vaultRoot).values()) {
            if (info.slug().equals(project)) {
                found = info;
                break;
            }
        }
        if (found == null) {
            throw new AisException(400, "프로젝트를 찾지 못했다: " + project);
        }
        if (ProjectRepo.find(found.repos(), name) == null) {
            List<String> names = found.repos().stream().map(ProjectRepo::name).toList();
            throw new AisException(400, "프로젝트 " + project + " 에 저장소 " + name + " 이 없다. 있는 이름: " + names);
        }
        return name;
    }

    /**
     * 첫 자리 인자에서 아젠다 id를 꺼냅니다. id는 다듬지 않는다.
     * 없거나 공백뿐이면 「인자가 하나 필요하다」로 선다. 빈 셸 변수가 {@code ''}로 넘어올 때
     * 「id 모양이 아니다」로 답하면 사람이 원인을 인자 쪽에서 못 찾기 때문이다.
     *
     * @param parsed 파싱된 인자
     * @param usage  그 명령의 사용법 1줄
     * @return 모양 검사를 지난 아젠다 id
     */
    static String requireId(ParsedArgs parsed, String usage) {
        List<String> pos = parsed.positionalArgs();
        if (pos.isEmpty() || Trim.isBlank(pos.get(0))) {
            throw new AisException(400, "인자가 하나 필요하다: " + usage);
        }
        String id = pos.get(0);
        if (!IdGenerator.isValidAgendaId(id)) {
            throw new AisException(400, "아젠다 id 모양이 아니다: " + id);
        }
        return id;
    }

    /**
     * hue 칸을 읽습니다. 다듬고, 공백뿐이면 안 준 것(null)이다.
     * 12개 가운데 1개와 글자가 같아야 하고 대소문자를 가린다. 사람이 방금 친 값을 조용히 파랑으로
     * 바꾸면 그 사람은 고른 색이 저장된 줄 알기 때문에 파일 읽기와 달리 거절한다.
     *
     * @param raw 받은 값. 안 줬으면 null
     * @return 받은 색, 또는 안 줬으면 null
     */
    static String hue(String raw) {
        String hue = Trim.trim(raw);
        if (hue.isEmpty()) {
            return null;
        }
        if (!AgendaReader.HUES.contains(hue)) {
            throw new AisException(400, "hue 값이 아니다: " + hue);
        }
        return hue;
    }

    /**
     * 방금 맞춘 색인(이번 호출이 훑은 아젠다 파일)에서 id로 아젠다를 찾습니다. 없으면 AIS-404입니다.
     */
    static AgendaTable.AgendaInfo find(NoteIndex noteIndex, String id) {
        return find(noteIndex.agendaTable(), id);
    }

    static AgendaTable.AgendaInfo find(AgendaTable table, String id) {
        AgendaTable.AgendaInfo info = table.idToAgenda().get(id);
        if (info == null) {
            throw new AisException(404, "Agenda not found: " + id);
        }
        return info;
    }
}
