package aisurada.task;

import java.util.List;
import java.util.Map;

/**
 * 태스크(티켓) 하나. <b>외부 설명서에 없는 확장</b>입니다. 사람이 일을 적어 두면 나중에 러너가 집어 에이전트에게 맡기는 단위입니다.
 *
 * @param key           사람이 부르는 키({@code TSK-14}). 엔진이 번호를 매기고 만든 뒤 바뀌지 않는다
 * @param id            고유 ID({@code tk…}). 키와 달리 파일 안에 있어 다른 곳에서 참조할 때 쓴다
 * @param title         한 줄 요약
 * @param type          유형({@code feature}, {@code bug}, {@code chore})
 * @param priority      우선순위({@code high}, {@code medium}, {@code low})
 * @param status        상태
 * @param repo          GitHub 저장소({@code owner/name})
 * @param project       프로젝트 slug. 없으면 null
 * @param baseBranch    기준 브랜치. 없으면 null(저장소의 기본 브랜치)
 * @param labels        라벨
 * @param criteria      완료 기준. 일이 끝났는지 이 항목으로 확인한다
 * @param links         이 태스크가 거는 연결. 반대쪽 모양(막혀 있음 따위)은 읽을 때 계산한다
 * @param refs          연결한 자원(노트, GitHub 글). 에이전트는 여기 적힌 것을 읽는다
 * @param body          설명(마크다운)
 * @param extra         모르는 머리말 칸. 읽고 다시 쓸 때 지우지 않고 보존한다
 * @param createdAt     만든 시각
 * @param editedAt      마지막으로 고친 시각
 * @param editedSession 마지막으로 고친 AI 세션 ID. 사람이 고쳤으면 null
 * @param editedAgent   그 세션의 CLI 이름. Claude이거나 모르면 null
 * @param relPath       vault 기준 파일 경로
 * @param token         파일 바이트의 {@code f1} 토큰. 새로 만들기 전이면 null
 */
public record Task(
        String key,
        String id,
        String title,
        String type,
        String priority,
        TaskStatus status,
        String repo,
        String project,
        String baseBranch,
        List<String> labels,
        List<Criterion> criteria,
        List<Link> links,
        List<Ref> refs,
        String body,
        Map<String, Object> extra,
        String createdAt,
        String editedAt,
        String editedSession,
        String editedAgent,
        String relPath,
        String token) {

    /** 완료 기준 한 줄. 확인한 쪽(QA 에이전트나 사람)과 시각은 확인했을 때만 있다. */
    public record Criterion(String text, boolean done, String checkedAt, String checkedBy) {}

    /** 연결 한 줄. {@code type}은 {@code blocks}(이 태스크가 {@code to}를 막고 있다), {@code relates}, {@code duplicates}(이 태스크가 {@code to}의 중복이다). */
    public record Link(String type, String to) {}

    /** 연결한 자원 한 줄. {@code kind}는 {@code note}(노트 id)나 {@code github}(이슈·PR 주소 또는 {@code owner/name#번호}). */
    public record Ref(String kind, String target, String title) {}

    /** 번호 부분. {@code TSK-14}면 14. */
    public int number() {
        return TaskFormat.numberOf(key);
    }

    /** 토큰만 바꾼 사본. */
    public Task withToken(String newToken) {
        return new Task(key, id, title, type, priority, status, repo, project, baseBranch, labels, criteria, links, refs, body, extra,
                createdAt, editedAt, editedSession, editedAgent, relPath, newToken);
    }
}
