package aisurada.resource;

import java.util.List;
import java.util.Map;

/**
 * 스킬이나 에이전트 정의 하나. 어느 AI 도구에도 묶이지 않는 중립한 값이고, 도구별 차이는 선택 칸 {@code tools}에만 둡니다.
 *
 * @param id            고유 ID({@code sk…}, {@code ap…}). 참조는 이름이 아니라 이 ID로 건다
 * @param kind          종류
 * @param project       프로젝트 slug. 글로벌 자원이면 null이다(범위는 파일 위치로 정해진다)
 * @param name          이름. 범위와 종류 안에서 하나뿐이다(폴더·파일 이름의 바탕)
 * @param description   한 문단 설명
 * @param permission    에이전트의 권한({@code read}, {@code edit}, {@code publish}). 스킬은 null
 * @param skills        에이전트가 쓸 스킬의 ID 목록. 스킬은 빈 목록
 * @param repo          프로젝트 에이전트가 쓸 저장소 이름. 없으면 null
 * @param tools         도구별 선택 칸(예: {@code claude}, {@code codex}). 몰라도 무시하고, 모르는 칸도 지우지 않고 보존한다
 * @param body          지침(마크다운)
 * @param editedAt      마지막으로 고친 시각. 없으면 null
 * @param editedSession 마지막으로 고친 AI 세션 ID. 사람이 고쳤으면 null
 * @param editedAgent   그 세션의 CLI 이름. Claude이거나 모르면 null
 * @param relPath       vault 기준 파일 경로
 * @param token         파일 바이트의 {@code f1} 토큰. 고칠 때 직전에 읽은 값을 넘긴다. 새로 만들기 전이면 null
 */
public record Resource(
        String id,
        ResourceKind kind,
        String project,
        String name,
        String description,
        String permission,
        List<String> skills,
        String repo,
        Map<String, Object> tools,
        String body,
        String editedAt,
        String editedSession,
        String editedAgent,
        String relPath,
        String token) {

    /** 글로벌 자원이면 true. */
    public boolean isGlobal() {
        return project == null;
    }

    /** 응답에 쓰는 범위 이름({@code global}, {@code project}). */
    public String scope() {
        return project == null ? "global" : "project";
    }

    /** 토큰만 바꾼 사본. */
    public Resource withToken(String newToken) {
        return new Resource(id, kind, project, name, description, permission, skills, repo, tools, body,
                editedAt, editedSession, editedAgent, relPath, newToken);
    }

    /** 위치만 바꾼 사본(이름을 바꿔 파일이 옮겨졌을 때). */
    public Resource withRelPath(String newRelPath) {
        return new Resource(id, kind, project, name, description, permission, skills, repo, tools, body,
                editedAt, editedSession, editedAgent, newRelPath, token);
    }
}
