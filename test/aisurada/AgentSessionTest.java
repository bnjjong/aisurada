package aisurada;

import aisurada.session.SessionAgents;

import java.nio.file.*;
import java.util.*;

import static aisurada.ChapterTestSupport.*;

/**
 * 세션 id와 함께 세션을 연 에이전트(claude, codex, gemini)를 노트에 적는 확장의 시험입니다.
 * 파일 형식, 편집 도장, 환경변수 해석, MCP 요청의 세션 읽기를 본다.
 */
public final class AgentSessionTest {
    private static final String SID = "\u0000ais-session-id";
    private static final String AGENT = "\u0000ais-session-agent";

    public static void main(String[] args) throws Exception {
        try (var t = new ChapterTestSupport("agent-session")) {
            Path v = t.vault();
            random("0c1d2e3f,1c2d3e4f,2c3d4e5f,3c4d5e6f,4c5d6e7f,5c6d7e8f,6c7d8e9f,7c8d9eaf");

            // 1. codex 세션으로 만들면 sessionId 뒤에 sessionAgent 줄이 붙는다
            var codex = ok(v, "create_doc", doc("코덱스 노트"), SID, "codex-thread-1", AGENT, "codex");
            String codexFile = Files.readString(v.resolve(codex.get("path") + "/note.md"));
            check(codexFile.contains("sessionId: codex-thread-1\nsessionAgent: codex\n"), "codex 노트는 sessionId 바로 뒤에 sessionAgent 줄");
            var codexRead = ok(v, "get_doc", "{\"id\":\"" + codex.get("id") + "\"}");
            eq(codexRead.get("sessionId"), "codex-thread-1", "get_doc 세션 id");
            eq(codexRead.get("sessionAgent"), "codex", "get_doc 세션 에이전트");

            // 2. 에이전트 표식이 없으면 claude다: 줄도 응답 키도 없다. 이전 파일과 바이트가 같다
            var claude = ok(v, "create_doc", doc("클로드 노트"), SID, "claude-session-1");
            String claudeFile = Files.readString(v.resolve(claude.get("path") + "/note.md"));
            check(claudeFile.contains("sessionId: claude-session-1\n") && !claudeFile.contains("sessionAgent"), "표식이 없으면 sessionAgent 줄이 없다");
            check(!ok(v, "get_doc", "{\"id\":\"" + claude.get("id") + "\"}").containsKey("sessionAgent"), "claude 응답에는 sessionAgent 키가 없다");

            // 3. 에이전트가 claude라고 와도 줄을 쓰지 않는다
            var explicitClaude = ok(v, "create_doc", doc("명시한 클로드"), SID, "claude-session-2", AGENT, "claude");
            check(!Files.readString(v.resolve(explicitClaude.get("path") + "/note.md")).contains("sessionAgent"), "claude는 줄을 쓰지 않는다");

            // 4. 세션 표식 없이 에이전트 표식만 오면 둘 다 버린다
            var orphan = ok(v, "create_doc", doc("에이전트만"), AGENT, "codex");
            String orphanFile = Files.readString(v.resolve(orphan.get("path") + "/note.md"));
            check(!orphanFile.contains("sessionId") && !orphanFile.contains("sessionAgent"), "세션 id 없이는 에이전트도 적지 않는다");

            // 5. 모양이 틀린 에이전트 이름은 줄로 쓰지 않는다
            var bad = ok(v, "create_doc", doc("이상한 이름"), SID, "bad-session", AGENT, "Co dex!");
            check(!Files.readString(v.resolve(bad.get("path") + "/note.md")).contains("sessionAgent"), "모양이 틀린 이름은 버린다");

            // 6. 다른 에이전트가 고치면 편집 도장에 그 에이전트가 붙고, 만든 세션은 그대로다
            String codexId = (String) codex.get("id");
            var got = ok(v, "get_doc", "{\"id\":\"" + codexId + "\"}");
            ok(v, "update_doc", "{\"id\":\"" + codexId + "\",\"f1\":\"" + got.get("precondition") + "\",\"body\":\"# 고친 본문\\n\"}",
                    SID, "gemini-session-9", AGENT, "gemini");
            String edited = Files.readString(v.resolve(codex.get("path") + "/note.md"));
            check(edited.contains("sessionId: codex-thread-1\nsessionAgent: codex\n"), "만든 세션과 에이전트는 그대로");
            check(edited.contains("editedBy: tool\n"), "도구 표면 편집");
            check(edited.contains("editedSessionId: gemini-session-9\neditedSessionAgent: gemini\n"), "편집 세션 뒤에 편집 에이전트");
            var editedRead = ok(v, "get_doc", "{\"id\":\"" + codexId + "\"}");
            eq(editedRead.get("editedSessionAgent"), "gemini", "get_doc 편집 에이전트");

            // 7. claude가 고치면 편집 에이전트 줄이 사라진다(없으면 claude)
            ok(v, "update_doc", "{\"id\":\"" + codexId + "\",\"f1\":\"" + editedRead.get("precondition") + "\",\"body\":\"# 또 고침\\n\"}",
                    SID, "claude-session-3");
            String editedAgain = Files.readString(v.resolve(codex.get("path") + "/note.md"));
            check(editedAgain.contains("editedSessionId: claude-session-3\n") && !editedAgain.contains("editedSessionAgent"), "claude 편집에는 편집 에이전트 줄이 없다");
            check(editedAgain.contains("sessionAgent: codex\n"), "만든 에이전트는 유지");

            // 8. 본문을 안 바꾸는 쓰기(즐겨찾기)도 두 에이전트 줄을 지우지 않는다
            var again = ok(v, "get_doc", "{\"id\":\"" + codexId + "\"}");
            ok(v, "set_favorite", "{\"id\":\"" + codexId + "\",\"f1\":\"" + again.get("precondition") + "\",\"favorite\":true}", SID, "gemini-session-7", AGENT, "gemini");
            String favorited = Files.readString(v.resolve(codex.get("path") + "/note.md"));
            check(favorited.contains("sessionAgent: codex\n") && favorited.contains("editedSessionAgent: gemini\n"), "즐겨찾기 쓰기도 에이전트 줄을 보존");

            // 9. 환경변수 해석
            Map<String, String> env = new HashMap<>();
            eq(SessionAgents.fromEnvironment(env), null, "변수가 없으면 출처도 없다");
            env.put("CLAUDE_CODE_SESSION_ID", " abc ");
            eq(SessionAgents.fromEnvironment(env), new SessionAgents.Source("claude", "abc"), "claude 변수는 다듬어서 쓴다");
            env.put("CODEX_THREAD_ID", "thread-1");
            eq(SessionAgents.fromEnvironment(env), null, "두 에이전트 변수가 함께 보이면 기록하지 않는다");
            env.put("AISURADA_SESSION_ID", "pinned-1");
            env.put("AISURADA_SESSION_AGENT", "gemini");
            eq(SessionAgents.fromEnvironment(env), new SessionAgents.Source("gemini", "pinned-1"), "지정한 쌍이 고유 변수보다 먼저");
            env.put("AISURADA_SESSION_AGENT", "Bad Name");
            eq(SessionAgents.fromEnvironment(env), null, "지정한 에이전트 이름이 틀리면 쌍을 버리고 고유 변수를 본다(여기서는 둘이라 없음)");
            env.remove("CLAUDE_CODE_SESSION_ID");
            eq(SessionAgents.fromEnvironment(env), new SessionAgents.Source("codex", "thread-1"), "고유 변수가 하나뿐이면 쓴다");
            env.remove("CODEX_THREAD_ID");
            env.put("CODEX_SESSION_ID", "sess-9");
            eq(SessionAgents.fromEnvironment(env), new SessionAgents.Source("codex", "sess-9"), "CODEX_SESSION_ID도 읽는다");

            // 10. MCP 요청의 세션 읽기는 codex 클라이언트일 때만
            eq(SessionAgents.fromMcpRequest("codex-mcp-client", "t-1"), new SessionAgents.Source("codex", "t-1"), "codex 클라이언트");
            eq(SessionAgents.fromMcpRequest("claude-code", "t-1"), null, "다른 클라이언트의 _meta는 믿지 않는다");
            eq(SessionAgents.fromMcpRequest("codex-mcp-client", "  "), null, "빈 값");
            eq(SessionAgents.forFile("Gemini"), "gemini", "대소문자는 소문자로");
            eq(SessionAgents.forFile("claude"), null, "claude는 줄이 없다");
            eq(SessionAgents.effective(null), "claude", "줄이 없으면 claude");

            t.done();
        }
    }

    private static String doc(String title) {
        return "{\"agenda\":\"demo\",\"type\":\"plan\",\"title\":\"" + title + "\",\"body\":\"# " + title + "\\n\"}";
    }
}
