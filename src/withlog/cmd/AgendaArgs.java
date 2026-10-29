package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.vault.AgendaReader;
import withlog.vault.AgendaTable;
import withlog.vault.NoteIndex;
import withlog.write.IdGenerator;

import java.util.List;

/**
 * 설명서 20.3의 칸 검사 가운데 아젠다 명령 여럿이 함께 쓰는 것.
 * 아젠다 id 칸, hue 칸, 색인에서 id로 찾기다.
 */
final class AgendaArgs {

    private AgendaArgs() {}

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
            throw new WlException(400, "인자가 하나 필요하다: " + usage);
        }
        String id = pos.get(0);
        if (!IdGenerator.isValidAgendaId(id)) {
            throw new WlException(400, "아젠다 id 모양이 아니다: " + id);
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
            throw new WlException(400, "hue 값이 아니다: " + hue);
        }
        return hue;
    }

    /**
     * 방금 맞춘 색인(이번 호출이 훑은 아젠다 파일)에서 id로 아젠다를 찾습니다. 없으면 WL-404입니다.
     */
    static AgendaTable.AgendaInfo find(NoteIndex noteIndex, String id) {
        return find(noteIndex.agendaTable(), id);
    }

    static AgendaTable.AgendaInfo find(AgendaTable table, String id) {
        AgendaTable.AgendaInfo info = table.idToAgenda().get(id);
        if (info == null) {
            throw new WlException(404, "Agenda not found: " + id);
        }
        return info;
    }
}
