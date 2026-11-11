package aisurada.session;

import aisurada.text.Trim;

import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.regex.Pattern;

/**
 * 세션을 연 에이전트(claude, codex, gemini 따위)의 이름 규칙과, 요청을 보낸 쪽의 세션 출처를 정하는 규칙입니다.
 *
 * <p>노트에 적힌 세션 id만으로는 어느 CLI의 세션인지 알 수 없어서, claude가 아닌 에이전트의 세션에는
 * {@code sessionAgent} 줄을 함께 적습니다. 줄이 없으면 claude입니다. 이렇게 하면 기존 노트와
 * Claude 호출이 쓰는 파일은 바이트 하나도 바뀌지 않습니다.
 */
public final class SessionAgents {

    public static final String CLAUDE = "claude";
    public static final String CODEX = "codex";
    public static final String GEMINI = "gemini";

    /** 에이전트를 직접 지정하는 환경변수 쌍. 실행 래퍼가 넣습니다. */
    public static final String ENV_PINNED_ID = "AISURADA_SESSION_ID";
    public static final String ENV_PINNED_AGENT = "AISURADA_SESSION_AGENT";

    private static final Pattern AGENT_PATTERN = Pattern.compile("^[a-z][a-z0-9-]{0,31}$");
    private static final ThreadLocal<Source> CALL_OVERRIDE = new ThreadLocal<>();

    private SessionAgents() {}

    /** 요청을 보낸 세션의 출처. 에이전트 이름과 그 CLI가 쓰는 세션 id입니다. */
    public record Source(String agent, String sessionId) {}

    /** 에이전트 이름으로 쓸 수 있는 모양인지 봅니다. 소문자로 시작하고 소문자·숫자·하이픈만 32자까지 허용합니다. */
    public static boolean isAgentName(String value) {
        return value != null && AGENT_PATTERN.matcher(value).matches();
    }

    /**
     * 파일에 적을 에이전트 이름을 돌려줍니다. 없음, claude, 모양이 틀린 값은 null입니다.
     * claude는 줄을 쓰지 않는다는 약속 때문에 null이고, 읽는 쪽은 null을 claude로 봅니다.
     */
    public static String forFile(String agent) {
        if (agent == null) return null;
        String lower = Trim.trim(agent).toLowerCase(Locale.ROOT);
        if (CLAUDE.equals(lower) || !isAgentName(lower)) return null;
        return lower;
    }

    /** 노트 머리말에서 읽은 값을 정리합니다. 모양이 틀리면 없음입니다. 값이 없음이면 읽는 쪽은 claude로 봅니다. */
    public static String readFromFile(Object raw) {
        if (!(raw instanceof String text)) return null;
        return forFile(text);
    }

    /** 화면과 응답이 쓰는 이름입니다. 줄이 없으면 claude입니다. */
    public static String effective(String agentOrNull) {
        return agentOrNull == null ? CLAUDE : agentOrNull;
    }

    /**
     * 환경변수로 이 요청의 세션 출처를 정합니다. 정하지 못하면 null입니다.
     *
     * <ol>
     *   <li>{@code AISURADA_SESSION_ID}와 {@code AISURADA_SESSION_AGENT}가 둘 다 쓸 수 있으면 그 쌍이 이깁니다.
     *       래퍼가 시작할 때 넣은 값이고, 부모 에이전트에서 물려받은 변수보다 믿을 수 있습니다.</li>
     *   <li>아니면 에이전트 고유 변수를 봅니다: claude는 {@code CLAUDE_CODE_SESSION_ID}, codex는
     *       {@code CODEX_SESSION_ID} 또는 {@code CODEX_THREAD_ID}.</li>
     *   <li>고유 변수가 둘 이상의 에이전트에서 동시에 보이면 어느 쪽이 안쪽인지 알 수 없으므로 기록하지 않습니다.
     *       엉뚱한 세션을 가리키는 것보다 비어 있는 편이 낫기 때문입니다.</li>
     * </ol>
     */
    public static Source fromEnvironment(Map<String, String> env) {
        String pinnedId = Trim.trim(nullToEmpty(env.get(ENV_PINNED_ID)));
        String pinnedAgent = Trim.trim(nullToEmpty(env.get(ENV_PINNED_AGENT))).toLowerCase(Locale.ROOT);
        if (!pinnedId.isEmpty() && isAgentName(pinnedAgent)) {
            return new Source(pinnedAgent, pinnedId);
        }

        List<Source> found = new ArrayList<>();
        String claude = Trim.trim(nullToEmpty(env.get("CLAUDE_CODE_SESSION_ID")));
        if (!claude.isEmpty()) found.add(new Source(CLAUDE, claude));
        String codex = Trim.trim(nullToEmpty(env.get("CODEX_SESSION_ID")));
        if (codex.isEmpty()) codex = Trim.trim(nullToEmpty(env.get("CODEX_THREAD_ID")));
        if (!codex.isEmpty()) found.add(new Source(CODEX, codex));
        return found.size() == 1 ? found.get(0) : null;
    }

    /**
     * MCP 요청에 실려 온 세션 정보로 출처를 정합니다. Codex는 도구 호출마다 {@code _meta.sessionId}를 싣고,
     * 환경변수로는 MCP 서버에 세션을 알려 주지 않습니다. 클라이언트 이름이 codex일 때만 믿습니다.
     */
    public static Source fromMcpRequest(String clientName, String metaSessionId) {
        if (clientName == null || metaSessionId == null) return null;
        String id = Trim.trim(metaSessionId);
        if (id.isEmpty()) return null;
        if (clientName.toLowerCase(Locale.ROOT).contains(CODEX)) return new Source(CODEX, id);
        return null;
    }

    /** 이 스레드의 다음 호출에 환경변수보다 먼저 쓸 출처를 정합니다. MCP 서버가 호출 직전에 부릅니다. */
    public static void setCallOverride(Source source) {
        if (source == null) CALL_OVERRIDE.remove();
        else CALL_OVERRIDE.set(source);
    }

    /** 이 스레드에 걸린 출처를 돌려줍니다. 없으면 null입니다. */
    public static Source callOverride() {
        return CALL_OVERRIDE.get();
    }

    private static String nullToEmpty(String value) {
        return value == null ? "" : value;
    }
}
