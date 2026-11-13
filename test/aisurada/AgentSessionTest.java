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

            // 11~. Codex·Gemini 기록 읽기 (가짜 홈에 형식대로 만든 파일)
            transcripts(t, v);

            t.done();
        }
    }

    private static void transcripts(ChapterTestSupport t, Path v) throws Exception {
        Path home = t.temp.resolve("fakehome");
        System.setProperty("aisurada.codex.home", home.toString());
        System.setProperty("aisurada.gemini.home", home.toString());
        System.setProperty("aisurada.enable.part4", "true");
        try {
            // Codex 새 형식: item_completed가 있으면 그것만 쓴다. response_item 함수 호출은 중복이라 버린다
            String cid = "01a1cce2-96c7-7560-8ab2-586887dc6fa4";
            Path day = home.resolve(".codex/sessions/2026/11/12");
            Files.createDirectories(day);
            Files.writeString(day.resolve("rollout-2026-11-12T08-12-44-" + cid + ".jsonl"), String.join("\n",
                    "{\"timestamp\":\"2026-11-11T23:12:44.000Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"" + cid + "\",\"cwd\":\"/work/proj\"}}",
                    "{\"timestamp\":\"2026-11-11T23:12:44.100Z\",\"type\":\"turn_context\",\"payload\":{\"cwd\":\"/work/proj\",\"model\":\"gpt-x\"}}",
                    "{\"timestamp\":\"2026-11-11T23:12:45.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"item_completed\",\"item\":{\"type\":\"UserMessage\",\"content\":[{\"type\":\"text\",\"text\":\"안녕 https://example.com/a ndnote0001-abcdef12\"}]}}}",
                    "{\"timestamp\":\"2026-11-11T23:12:46.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"item_completed\",\"item\":{\"type\":\"Reasoning\"}}}",
                    "{\"timestamp\":\"2026-11-11T23:12:47.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"item_completed\",\"item\":{\"type\":\"AgentMessage\",\"content\":[{\"type\":\"Text\",\"text\":\"네\"}],\"phase\":\"commentary\"}}}",
                    "{\"timestamp\":\"2026-11-11T23:12:48.000Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"shell\",\"arguments\":\"{\\\"command\\\":[\\\"bash\\\",\\\"-lc\\\",\\\"dup\\\"]}\"}}",
                    "{\"timestamp\":\"2026-11-11T23:12:49.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"item_completed\",\"item\":{\"type\":\"CommandExecution\",\"command\":[\"/bin/zsh\",\"-lc\",\"ls -la\"],\"exit_code\":1,\"duration\":{\"secs\":2,\"nanos\":500000000}}}}",
                    "{\"timestamp\":\"2026-11-11T23:12:50.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"item_completed\",\"item\":{\"type\":\"McpToolCall\",\"server\":\"aisurada\",\"tool\":\"create_doc\",\"status\":\"completed\"}}}",
                    "이건 JSON이 아닌 \"item_completed\" 줄", "") );
            var located = aisurada.session.AgentTranscripts.locate(cid);
            eq(located == null ? null : located.agent(), "codex", "codex 기록을 id로 찾는다");
            var info = aisurada.session.AgentTranscripts.info(located, cid);
            eq(info.cwd(), "/work/proj", "codex 작업 폴더");
            eq(info.title(), "안녕 https://example.com/a ndnote0001-abcdef12", "제목은 첫 사용자 글");
            var read = ok(v, "read-session", cid, "limit=10", "urls=true", "note=ndnote0001-abcdef12");
            eq(read.get("agent"), "codex", "응답에 agent");
            var source = (Map<?, ?>) read.get("source");
            eq(source.get("messages"), 4L, "사용자 1, 에이전트 1, 도구 2 (생각과 중복 함수 호출은 뺀다)");
            eq(source.get("skipped"), 1L, "읽지 못한 줄은 skipped로 센다");
            var turns = (List<?>) read.get("turns");
            var shell = (Map<?, ?>) ((Map<?, ?>) turns.get(2)).get("tool");
            eq(shell.get("name"), "shell", "명령 실행은 shell 도구"); eq(shell.get("arg"), "ls -la", "-lc 뒤의 명령만");
            eq(shell.get("failed"), true, "종료 코드가 0이 아니면 실패"); eq(shell.get("seconds"), 2.5, "걸린 시간");
            eq(((Map<?, ?>) ((Map<?, ?>) turns.get(3)).get("tool")).get("name"), "aisurada/create_doc", "MCP 도구는 서버/도구");
            eq(((Map<?, ?>) read.get("runState")).get("model"), "gpt-x", "모델");
            eq(((Map<?, ?>) ((List<?>) read.get("urls")).get(0)).get("url"), "https://example.com/a", "주소 모으기");
            eq(read.get("noteMentions"), List.of(0L), "노트 id가 나온 회차");
            var paged = ok(v, "read-session", cid, "limit=1");
            eq(((List<?>) paged.get("turns")).size(), 3, "끝에서 글 회차 1개와 그 뒤 도구들"); eq(paged.get("hasMore"), true, "앞이 더 있다");

            // Codex 예전 형식
            String oldId = "019966ea-156b-7e21-bee2-e9afe891a30d";
            Path oldDay = home.resolve(".codex/sessions/2025/09/20");
            Files.createDirectories(oldDay);
            Files.writeString(oldDay.resolve("rollout-2025-09-20T20-37-21-" + oldId + ".jsonl"), String.join("\n",
                    "{\"timestamp\":\"2025-09-20T11:37:21.000Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"" + oldId + "\",\"cwd\":\"/old\"}}",
                    "{\"timestamp\":\"2025-09-20T11:37:22.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"user_message\",\"message\":\"옛 질문\"}}",
                    "{\"timestamp\":\"2025-09-20T11:37:23.000Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"shell\",\"arguments\":\"{\\\"command\\\":[\\\"bash\\\",\\\"-lc\\\",\\\"echo legacy\\\"]}\"}}",
                    "{\"timestamp\":\"2025-09-20T11:37:24.000Z\",\"type\":\"event_msg\",\"payload\":{\"type\":\"agent_message\",\"message\":\"옛 답\"}}", ""));
            var legacy = ok(v, "read-session", oldId);
            var legacyTurns = (List<?>) legacy.get("turns");
            eq(legacyTurns.size(), 3, "예전 형식도 읽는다");
            eq(((Map<?, ?>) ((Map<?, ?>) legacyTurns.get(1)).get("tool")).get("arg"), "echo legacy", "예전 형식의 함수 호출 인자");

            // Gemini: 환경 설명 줄은 버리고, 도구 호출을 풀고, 되감기를 지킨다
            String gid = "fd1924a5-3858-4789-8935-5bd929c4c49e";
            Path chats = home.resolve(".gemini/tmp/proj-a/chats");
            Files.createDirectories(chats);
            Files.writeString(home.resolve(".gemini/projects.json"), "{\"projects\":{\"/work/gem\":\"proj-a\"}}");
            Files.writeString(chats.resolve("session-2026-11-10T00-23-fd1924a5.jsonl"), String.join("\n",
                    "{\"sessionId\":\"" + gid + "\",\"projectHash\":\"h\",\"startTime\":\"2026-11-10T00:23:38.832Z\",\"kind\":\"main\"}",
                    "{\"id\":\"m1\",\"timestamp\":\"2026-11-10T00:23:39.000Z\",\"type\":\"user\",\"content\":[{\"text\":\"<session_context>\\n환경 설명\"}]}",
                    "{\"id\":\"m2\",\"timestamp\":\"2026-11-10T00:23:40.000Z\",\"type\":\"user\",\"content\":[{\"text\":\"첫 질문\"}]}",
                    "{\"$set\":{\"lastUpdated\":\"2026-11-10T00:23:41.000Z\"}}",
                    "{\"id\":\"m3\",\"timestamp\":\"2026-11-10T00:23:42.000Z\",\"type\":\"gemini\",\"content\":\"\",\"model\":\"gemini-x\",\"toolCalls\":[{\"id\":\"t1\",\"name\":\"read_file\",\"args\":{\"file_path\":\"a.md\"},\"status\":\"success\"},{\"id\":\"t2\",\"name\":\"run_shell_command\",\"args\":{\"command\":\"false\"},\"status\":\"error\"}]}",
                    "{\"id\":\"m4\",\"timestamp\":\"2026-11-10T00:23:43.000Z\",\"type\":\"gemini\",\"content\":\"답변\",\"model\":\"gemini-x\"}",
                    "{\"id\":\"m5\",\"timestamp\":\"2026-11-10T00:23:44.000Z\",\"type\":\"user\",\"content\":[{\"text\":\"되감을 질문\"}]}",
                    "{\"$rewindTo\":\"m5\"}", ""));
            var g = aisurada.session.AgentTranscripts.locate(gid);
            eq(g == null ? null : g.agent(), "gemini", "gemini 기록을 id로 찾는다");
            var ginfo = aisurada.session.AgentTranscripts.info(g, gid);
            eq(ginfo.title(), "첫 질문", "환경 설명 줄은 제목이 아니다"); eq(ginfo.cwd(), "/work/gem", "projects.json으로 작업 폴더를 거꾸로 찾는다");
            var gread = ok(v, "read-session", gid);
            var gturns = (List<?>) gread.get("turns");
            eq(gturns.size(), 4, "사용자, 도구 2, 에이전트 (환경 설명과 되감긴 질문은 없다)");
            eq(((Map<?, ?>) ((Map<?, ?>) gturns.get(1)).get("tool")).get("arg"), "a.md", "도구 인자 요약");
            eq(((Map<?, ?>) ((Map<?, ?>) gturns.get(2)).get("tool")).get("failed"), true, "상태가 success가 아니면 실패");
            eq(((Map<?, ?>) gread.get("runState")).get("model"), "gemini-x", "gemini 모델");

            // Antigravity(agy): SQLite 안의 protobuf. sqlite3가 없으면 건너뛴다
            antigravity(v, home);

            // session-titles: 모르는 id는 found=false에 agent 키가 없다
            var titles = (List<?>) ok(v, "session-titles", cid, gid, "00000000-0000-0000-0000-000000000000").get("sessions");
            eq(((Map<?, ?>) titles.get(0)).get("agent"), "codex", "titles codex"); eq(((Map<?, ?>) titles.get(1)).get("agent"), "gemini", "titles gemini");
            eq(((Map<?, ?>) titles.get(2)).get("found"), false, "모르는 id"); check(!((Map<?, ?>) titles.get(2)).containsKey("agent"), "모르는 id에는 agent 키가 없다");
        } finally {
            System.clearProperty("aisurada.codex.home");
            System.clearProperty("aisurada.gemini.home");
            System.clearProperty("aisurada.enable.part4");
        }
    }

    /** Antigravity 대화 기록을 진짜 모양으로 짓고(SQLite + protobuf) 읽는다. 가짜 기록은 sqlite3 명령으로 만든다. */
    private static void antigravity(Path v, Path home) throws Exception {
        String sqlite = Files.isExecutable(Path.of("/usr/bin/sqlite3")) ? "/usr/bin/sqlite3" : null;
        if (sqlite == null) {
            System.out.println("  (sqlite3가 없어 Antigravity 기록 시험은 건너뜀)");
            return;
        }
        String aid = "b6e7c390-5781-4dae-b98e-e17dc5202d70";
        Path conversations = home.resolve(".gemini/antigravity-cli/conversations");
        Files.createDirectories(conversations);
        Path db = conversations.resolve(aid + ".db");
        String longBody = "긴 본문 ".repeat(600); // 인자 앞부분(1200바이트)을 넘겨도 도구 이름과 id는 읽혀야 한다
        String mcpArgs = "{\"Arguments\":{\"body\":\"" + longBody + "\",\"id\":\"ndnote0001-abcdef12\"},\"ServerName\":\"aisurada\",\"ToolName\":\"update_doc\"}";
        List<String> sql = new ArrayList<>(List.of(
                "create table steps (idx integer, step_type integer not null default 0, status integer not null default 0, has_subtrajectory numeric not null default false, metadata blob, error_details blob, permissions blob, task_details blob, render_info blob, step_payload blob, step_format integer not null default 0, primary key (idx));",
                "create table trajectory_metadata_blob (id text default 'main', data blob, primary key (id));",
                "create table executor_metadata (idx integer, data blob, primary key (idx));",
                "create table gen_metadata (idx integer, data blob, size integer not null default 0, primary key (idx));",
                // 작업 폴더는 file: 주소이고 공백이 %20으로 온다
                "insert into trajectory_metadata_blob values ('main', " + hex(pbString(7, "file:///work/ag%20dir")) + ");",
                "insert into executor_metadata values (0, " + hex(pbMessage(10, pbMessage(1, pbString(28, "gemini-test-high")))) + ");",
                // 0: 사용자 질문(노트 id가 들어 있다)
                step(0, 14, 3, null, pbMessage(19, cat(pbString(2, "안녕 ndnote0001-abcdef12"), pbMessage(3, pbString(1, "안녕"))))),
                // 1: 생각만 있고 답 글이 없는 모델 단계(오래된 단계는 내용이 비워진다) → 회차가 아니다
                step(1, 15, 5, null, pbMessage(20, pbString(3, "생각만"))),
                // 2: 기본 도구
                stepTool(2, 3, null, "call_1", "read_file", "{\"EndLine\":100,\"Path\":\"/a/b.md\"}"),
                // 3: MCP 도구. 서버와 도구 이름은 인자 맨 끝에 있고 id는 긴 본문 뒤에 있다
                stepTool(3, 3, null, "call_2", "call_mcp_tool", mcpArgs),
                // 4: 실패한 도구(status 7)
                stepTool(4, 7, "file does not exist", "call_3", "run_command", "{\"CommandLine\":\"false\"}"),
                // 5: 모델 답. 3번(생각)은 뺀다
                step(5, 15, 3, null, pbMessage(20, cat(pbString(1, "답변입니다"), pbString(3, "생각")))),
                // 6, 7: 시스템 알림(101)과 이어쓰기 요약(23)은 대화가 아니다
                step(6, 101, 3, null, pbMessage(114, pbString(1, "[Message] system"))),
                step(7, 23, 3, null, pbMessage(30, pbString(5, "<summary>요약</summary>"))),
                // 8: 글이 빈 사용자 단계
                step(8, 14, 5, null, pbMessage(19, pbString(2, "")))));
        Process builder = new ProcessBuilder(sqlite, db.toString()).redirectErrorStream(true).start();
        builder.getOutputStream().write(String.join("\n", sql).getBytes(java.nio.charset.StandardCharsets.UTF_8));
        builder.getOutputStream().close();
        String builderOut = new String(builder.getInputStream().readAllBytes(), java.nio.charset.StandardCharsets.UTF_8);
        check(builder.waitFor() == 0, "가짜 Antigravity 기록을 sqlite3로 짓는다: " + builderOut);

        var located = aisurada.session.AgentTranscripts.locate(aid);
        eq(located == null ? null : located.agent(), "gemini", "Antigravity 기록도 gemini로 찾는다");
        var info = aisurada.session.AgentTranscripts.info(located, aid);
        eq(info.found(), true, "Antigravity 읽기"); eq(info.title(), "안녕 ndnote0001-abcdef12", "제목은 첫 사용자 글");
        eq(info.cwd(), "/work/ag dir", "작업 폴더는 file: 주소를 풀어 쓴다");
        var read = ok(v, "read-session", aid, "note=ndnote0001-abcdef12");
        eq(read.get("agent"), "gemini", "응답에 agent");
        var turns = (List<?>) read.get("turns");
        eq(turns.size(), 5, "사용자 1, 도구 3, 모델 답 1 (생각만 있는 단계, 시스템 알림, 요약, 빈 입력은 뺀다)");
        eq(((Map<?, ?>) turns.get(0)).get("who"), "user", "첫 회차는 사용자");
        eq(((Map<?, ?>) turns.get(0)).get("at"), "2026-11-08T10:30:01.500Z", "시각은 UTC 초와 나노초");
        var readFile = (Map<?, ?>) ((Map<?, ?>) turns.get(1)).get("tool");
        eq(readFile.get("name"), "read_file", "기본 도구 이름"); eq(readFile.get("arg"), "/a/b.md", "Path 인자 요약(대문자 키)");
        var mcp = (Map<?, ?>) ((Map<?, ?>) turns.get(2)).get("tool");
        eq(mcp.get("name"), "aisurada/update_doc", "MCP 도구는 서버/도구"); eq(mcp.get("arg"), "ndnote0001-abcdef12", "긴 본문 뒤의 id를 요약에 쓴다");
        var failed = (Map<?, ?>) ((Map<?, ?>) turns.get(3)).get("tool");
        eq(failed.get("name"), "run_command", "명령 실행"); eq(failed.get("arg"), "false", "CommandLine 인자 요약"); eq(failed.get("failed"), true, "status 7은 실패");
        eq(((Map<?, ?>) turns.get(4)).get("text"), "답변입니다", "모델 답은 1번 필드만");
        eq(((Map<?, ?>) read.get("runState")).get("model"), "gemini-test-high", "모델 id");
        eq(read.get("noteMentions"), List.of(0L), "노트 id가 나온 회차");

        // id 모양이 UUID가 아니면 파일 이름으로 쓰지 않는다
        eq(aisurada.session.AgentTranscripts.locate("../" + aid), null, "경로를 흉내 낸 id는 찾지 않는다");
        var titles = (List<?>) ok(v, "session-titles", aid).get("sessions");
        eq(((Map<?, ?>) titles.get(0)).get("agent"), "gemini", "titles에도 gemini"); eq(((Map<?, ?>) titles.get(0)).get("found"), true, "titles found");
        // Gemini CLI와 같은 agent라서, 이어 여는 명령을 가르려면 client가 필요하다. Antigravity에만 붙는다
        eq(((Map<?, ?>) titles.get(0)).get("client"), "antigravity", "Antigravity 기록에는 client가 붙는다");
        eq(read.get("client"), "antigravity", "read-session에도 client");
        var geminiCliTitle = (Map<?, ?>) ((List<?>) ok(v, "session-titles", "fd1924a5-3858-4789-8935-5bd929c4c49e").get("sessions")).get(0);
        eq(geminiCliTitle.get("agent"), "gemini", "Gemini CLI 기록도 gemini"); check(!geminiCliTitle.containsKey("client"), "Gemini CLI 기록에는 client 키가 없다");
        check(!ok(v, "read-session", "fd1924a5-3858-4789-8935-5bd929c4c49e").containsKey("client"), "Gemini CLI read-session에는 client 키가 없다");

        // sqlite3를 못 찾으면 못 읽은 것으로 보고하고 죽지 않는다
        System.setProperty("aisurada.sqlite.bin", home.resolve("no-such-sqlite3").toString());
        try {
            eq(aisurada.session.AgentTranscripts.info(located, aid).found(), false, "sqlite3가 없으면 found=false");
        } finally {
            System.clearProperty("aisurada.sqlite.bin");
        }
    }

    // ---- 가짜 Antigravity 기록을 짓는 도우미: 최소 protobuf 인코더 ----

    private static byte[] cat(byte[]... parts) {
        var out = new java.io.ByteArrayOutputStream();
        for (byte[] part : parts) out.writeBytes(part);
        return out.toByteArray();
    }

    private static byte[] varint(long value) {
        var out = new java.io.ByteArrayOutputStream();
        while ((value & ~0x7fL) != 0) { out.write((int) ((value & 0x7f) | 0x80)); value >>>= 7; }
        out.write((int) value);
        return out.toByteArray();
    }

    private static byte[] pbNumber(int field, long value) { return cat(varint((long) field << 3), varint(value)); }

    private static byte[] pbBytes(int field, byte[] value) { return cat(varint(((long) field << 3) | 2), varint(value.length), value); }

    private static byte[] pbString(int field, String value) { return pbBytes(field, value.getBytes(java.nio.charset.StandardCharsets.UTF_8)); }

    private static byte[] pbMessage(int field, byte[] message) { return pbBytes(field, message); }

    /** 단계 시각 메시지: 1번 초, 2번 나노초. */
    private static byte[] time() { return cat(pbNumber(1, 1794133801L), pbNumber(2, 500_000_000L)); }

    private static String hex(byte[] bytes) {
        var sb = new StringBuilder("X'");
        for (byte b : bytes) sb.append(String.format("%02x", b));
        return sb.append("'").toString();
    }

    private static String step(int idx, int type, int status, byte[] errors, byte[] payload) {
        byte[] metadata = pbMessage(1, time());
        byte[] full = cat(pbNumber(1, type), pbNumber(4, status), pbMessage(5, metadata), payload);
        return "insert into steps (idx, step_type, status, metadata, error_details, step_payload) values (" + idx + "," + type + "," + status + ","
                + hex(metadata) + "," + (errors == null ? "null" : hex(errors)) + "," + hex(full) + ");";
    }

    private static String stepTool(int idx, int status, String error, String callId, String name, String args) {
        byte[] metadata = cat(pbMessage(1, time()), pbMessage(4, cat(pbString(1, callId), pbString(2, name), pbString(3, args))));
        return "insert into steps (idx, step_type, status, metadata, error_details, step_payload) values (" + idx + ",132," + status + ","
                + hex(metadata) + "," + (error == null ? "null" : hex(pbString(2, error))) + "," + hex(cat(pbNumber(1, 132), pbMessage(5, metadata))) + ");";
    }

    private static String doc(String title) {
        return "{\"agenda\":\"demo\",\"type\":\"plan\",\"title\":\"" + title + "\",\"body\":\"# " + title + "\\n\"}";
    }
}
