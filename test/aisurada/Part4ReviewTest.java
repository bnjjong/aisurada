package aisurada;

import aisurada.engine.EngineExecutor;
import aisurada.engine.EngineResult;

import java.io.ByteArrayOutputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;

import static aisurada.ChapterTestSupport.*;

/**
 * 「4부 다시 짓기 리뷰」(2026-11-04)의 엔진 몫 23줄(H1, M1~M4, L1~L18)을 잰다.
 * 줄 이름은 리뷰의 번호를 그대로 쓴다.
 */
public final class Part4ReviewTest {
    private static final String SESSION = "f1a50000-0000-4000-8000-000000000001";
    private static final String NOTE = "ndfixture-00000001";

    public static void main(String[] args) throws Exception {
        String oldHome = System.getProperty("aisurada.claude.home");
        try (var t = new ChapterTestSupport("part4-review")) {
            Path home = Files.createDirectory(t.temp.resolve("home"));
            System.setProperty("aisurada.claude.home", home.toString());
            Path projects = Files.createDirectories(home.resolve(".claude/projects/-Users-me-proj"));

            engineSwitch(t);
            System.setProperty("aisurada.enable.part4", "true");
            argumentOrderAndRanges(t);
            askFileReading(t);
            chatFiles(t);
            sessionArguments(t);
            transcriptReading(projects);
            titleWindows(projects);
            t.done();
        } finally {
            System.clearProperty("aisurada.enable.part4");
            if (oldHome == null) System.clearProperty("aisurada.claude.home"); else System.setProperty("aisurada.claude.home", oldHome);
        }
        System.out.println("Part4ReviewTest: all checks passed!");
    }

    /** H1. 4부를 켠 jar는 스위치를 안 줘도 4부 명령 7개가 돈다. 속성이나 환경 변수는 빌드 스위치를 바꾸지 못한다. */
    private static void engineSwitch(ChapterTestSupport t) throws Exception {
        Path v = t.vault();
        System.clearProperty("aisurada.enable.part4");
        check(EngineExecutor.isPart4Enabled(), "H1: part4 jar is on without any switch");
        eq(error(v, 400, "get-ask").errorCode(), 400, "H1: get-ask runs instead of AIS-409");
        System.setProperty("aisurada.enable.part4", "false");
        check(EngineExecutor.isPart4Enabled(), "N1: property false cannot disable the enabled build");
        error(v, 400, "get-ask");
        System.clearProperty("aisurada.enable.part4");
    }

    /** L1, L2, L3, L17. 이름 인자를 먼저 재고, 정수는 32비트이고, 빈 session-lines 는 안 준 것이다. */
    private static void argumentOrderAndRanges(ChapterTestSupport t) throws Exception {
        Path v = t.vault();
        root(v, "mynote", 1, "2026-06-16", "", "# T\nbody\n");

        // L1. 없는 노트여도 이름 인자가 틀렸으면 AIS-400 이다
        String[] bad = {"append-ask-turn", "ndfixture-99999999", "session-id=s", "turn-id=t", "mark-id=m", "asked-at=a", "question=q", "answer=a", "model=m", "effort=e", "elapsed-ms=x"};
        error(v, 400, bad);
        error(v, 400, "append-chat-turn", "ndfixture-99999999", "chat-id=c1", "source-session=s", "turn-id=t", "asked-at=a", "question=q", "answer=a", "model=m", "effort=e", "elapsed-ms=x");
        // 인자가 맞으면 그제야 노트를 찾아 404 다
        error(v, 404, "append-ask-turn", "ndfixture-99999999", "session-id=s", "turn-id=t", "mark-id=m", "asked-at=a", "question=q", "answer=a", "model=m", "effort=e", "elapsed-ms=1");

        // L2. 32비트를 넘는 elapsed-ms 는 AIS-400 이다
        error(v, 400, "append-ask-turn", NOTE, "session-id=s", "turn-id=t", "mark-id=m", "asked-at=a", "question=q", "answer=a", "model=m", "effort=e", "elapsed-ms=3000000000");
        error(v, 400, "append-chat-turn", NOTE, "chat-id=c1", "source-session=s", "turn-id=t", "asked-at=a", "question=q", "answer=a", "model=m", "effort=e", "elapsed-ms=3000000000");

        // L3. 걷어서 빈 session-lines 는 안 준 인자다
        Map<String, Object> saved = ok(v, "append-ask-turn", NOTE, "session-id=s", "turn-id=t1", "mark-id=m", "asked-at=a", "question=q", "answer=a", "model=m", "effort=e", "elapsed-ms=5", "session-lines=");
        check(!turn(saved, 0).containsKey("sessionLines"), "L3: empty session-lines is absent");
        error(v, 400, "append-ask-turn", NOTE, "session-id=s", "turn-id=t2", "mark-id=m", "asked-at=a", "question=q", "answer=a", "model=m", "effort=e", "elapsed-ms=5", "session-lines=0");

        // L17. U+3000 한 글자는 Kotlin trim 처럼 걷혀서 빈 값이다
        error(v, 400, "append-ask-turn", NOTE, "session-id=s", "turn-id=t2", "mark-id=m", "asked-at=a", "question=q", "answer=a", "model=　", "effort=e", "elapsed-ms=5");
    }

    /** L4, L5, L6. ask.json 읽기. */
    private static void askFileReading(ChapterTestSupport t) throws Exception {
        Path v = t.vault();
        root(v, "mynote", 1, "2026-06-16", "", "# T\nbody\n");
        Path askFile = v.resolve("notes/demo/mynote/ask.json");
        String turns = "[{\"id\":\"a\",\"question\":\"q\",\"answer\":\"x\",\"model\":\"m\",\"effort\":\"e\",\"elapsedMs\":7600.5,\"sessionLines\":21.0},"
                + "{\"id\":\"b\",\"question\":\"q\",\"answer\":\"x\",\"model\":\"m\",\"effort\":\"e\",\"elapsedMs\":1e3,\"sessionLines\":\"21\"},"
                + "{\"id\":\"c\",\"question\":\"q\",\"answer\":\"x\",\"model\":\"m\",\"effort\":\"e\",\"elapsedMs\":\"5\",\"sessionLines\":21,\"images\":[\"attach/a\u007F.png\"]}]";
        Files.writeString(askFile, "{\"v\":1,\"noteId\":\"" + NOTE + "\",\"sessionId\":\"s\",\"turns\":" + turns + "}\n");
        Map<String, Object> got = ok(v, "get-ask", NOTE);
        eq(turn(got, 0).get("elapsedMs"), 0L, "L4: 7600.5 is not an integer");
        check(!turn(got, 0).containsKey("sessionLines"), "L4: 21.0 is no integer line count");
        eq(turn(got, 1).get("elapsedMs"), 0L, "L4: 1e3 is not an integer");
        eq(turn(got, 1).get("sessionLines"), 21L, "L4: quoted line count reads as Int");
        eq(turn(got, 2).get("elapsedMs"), 5L, "L4: quoted integer elapsedMs still reads");
        eq(turn(got, 2).get("sessionLines"), 21L, "L4: integer sessionLines reads");
        eq(turn(got, 2).get("images"), List.of("attach/a\u007F.png"), "L6: U+007F is accepted in attachment names");

        // Second review L4: Int overflow, non-positive values and floating point never become line counts.
        for (String count : List.of("3000000000", "\"3000000000\"", "0", "-1", "21.0")) {
            String row = "{\"id\":\"range\",\"question\":\"q\",\"answer\":\"a\",\"sessionLines\":" + count + "}";
            Files.writeString(askFile, "{\"v\":1,\"noteId\":\"" + NOTE + "\",\"sessionId\":\"s\",\"turns\":[" + row + "]}");
            check(!turn(ok(v, "get-ask", NOTE), 0).containsKey("sessionLines"), "second review: invalid line count " + count);
        }

        // L5. 깨진 UTF-8 바이트는 U+FFFD 로 읽는다
        ByteArrayOutputStream broken = new ByteArrayOutputStream();
        broken.write("{\"v\":1,\"noteId\":\"".getBytes(StandardCharsets.UTF_8));
        broken.write(NOTE.getBytes(StandardCharsets.UTF_8));
        broken.write("\",\"sessionId\":\"s\",\"turns\":[{\"id\":\"a\",\"question\":\"깨짐 ".getBytes(StandardCharsets.UTF_8));
        broken.write(new byte[]{(byte) 0xEA, (byte) 0xB0});
        broken.write("\",\"answer\":\"x\",\"model\":\"m\",\"effort\":\"e\",\"elapsedMs\":1}]}\n".getBytes(StandardCharsets.UTF_8));
        Files.write(askFile, broken.toByteArray());
        got = ok(v, "get-ask", NOTE);
        check(String.valueOf(turn(got, 0).get("question")).contains("�"), "L5: broken bytes become U+FFFD");
    }

    /** M1, L5, L6, L7, L8, L17. 레일 대화 파일. */
    private static void chatFiles(ChapterTestSupport t) throws Exception {
        Path v = t.vault();
        root(v, "mynote", 1, "2026-06-16", "", "# T\nbody\n");
        String[] base = {"append-chat-turn", NOTE, "chat-id=c1", "source-session=s", "turn-id=t1", "asked-at=2026-11-03T03:00:00.000Z", "question=q", "answer=a", "model=m", "effort=e", "elapsed-ms=1"};
        // M1. 첨부 이름의 빗금, 역빗금, .., 점 시작, 제어 글자를 막는다
        for (String name : new String[]{"../../x.png", "a/b.png", "a\\b.png", ".hidden.png", "a..b.png", "a\u0001b.png"}) {
            error(v, 400, with(base, "images=attach/" + name));
        }
        // L6. U+007F 는 받는다
        ok(v, with(base, "images=attach/a\u007F.png"));
        // L8. 빈 조각은 세지 않는다
        ok(v, withId(base, "c2", "images=,,,,,,,,,attach/a.png"));
        // L7. 빈 error= 도 "error": "" 로 쓴다
        ok(v, withId(base, "c3", "error="));
        String text = Files.readString(v.resolve("notes/demo/mynote/chats/c3.json"));
        check(text.contains("\"error\": \"\""), "L7: empty error is written");
        // 필수 인자 문구는 answer 만 「가」다
        check(stderr(error(v, 400, "append-chat-turn", NOTE, "chat-id=c9", "source-session=s", "turn-id=t", "asked-at=a", "question=q", "model=m", "effort=e", "elapsed-ms=1")).contains("answer 가 필요하다"), "required answer wording");

        // L5. 깨진 UTF-8 대화 파일은 못 읽는 파일이 아니다
        ByteArrayOutputStream broken = new ByteArrayOutputStream();
        broken.write(("{\"v\":1,\"noteId\":\"" + NOTE + "\",\"chatId\":\"c4\",\"sourceSessionId\":\"s\",\"createdAt\":\"2026-11-03T03:00:00.000Z\",\"turns\":[{\"id\":\"a\",\"question\":\"깨짐 ").getBytes(StandardCharsets.UTF_8));
        broken.write(new byte[]{(byte) 0xEA, (byte) 0xB0});
        broken.write("\",\"answer\":\"x\",\"model\":\"m\",\"effort\":\"e\",\"elapsedMs\":7600.5}]}\n".getBytes(StandardCharsets.UTF_8));
        Files.write(v.resolve("notes/demo/mynote/chats/c4.json"), broken.toByteArray());
        Map<String, Object> listed = ok(v, "list-chats", NOTE);
        eq(listed.get("unreadable"), List.of(), "L5: broken UTF-8 chat is readable");
        @SuppressWarnings("unchecked") List<Map<String, Object>> chats = (List<Map<String, Object>>) listed.get("chats");
        Map<String, Object> c4 = chats.stream().filter(c -> "c4".equals(c.get("chatId"))).findFirst().orElseThrow();
        @SuppressWarnings("unchecked") Map<String, Object> first = ((List<Map<String, Object>>) c4.get("turns")).get(0);
        eq(first.get("elapsedMs"), 0L, "L4: chat elapsedMs 7600.5 reads as 0");
        check(String.valueOf(first.get("question")).contains("�"), "L5: chat question has U+FFFD");
    }

    /** L9, L18. read-session 인자. */
    private static void sessionArguments(ChapterTestSupport t) throws Exception {
        Path v = t.vault();
        error(v, 400, "read-session", SESSION, "urls=TRUE");
        error(v, 400, "read-session", " " + SESSION);
        error(v, 400, "session-titles", " " + SESSION);
        ok(v, "read-session", SESSION, "urls= true ");
        // L17. U+3000 만 든 limit 는 빈 값이라 기본값이다
        ok(v, "read-session", SESSION, "limit=　");
    }

    /** M3, L10~L15, L17. 전사 훑기. */
    private static void transcriptReading(Path projects) throws Exception {
        Path v = Files.createTempDirectory("part4-review-vault");
        String sid = "f1a50000-0000-4000-8000-0000000000aa";
        List<String> lines = new ArrayList<>();
        lines.add(user("2026-09-20T01:00:00Z", "start"));
        lines.add(assistantText("2026-09-20T01:00:01Z", "one"));
        lines.add(user("2026-09-20T01:00:02Z", "two"));
        lines.add(assistantText("2026-09-20T01:00:03Z", "three"));
        // 줄 4: 글 회차(4)와 도구 회차(5)를 만들고, 원문에 노트 id 가 있다 -> 자리는 그 줄을 다 담은 뒤의 5
        lines.add("{\"type\":\"assistant\",\"timestamp\":\"2026-09-20T01:00:04Z\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"ndm1abcd-00000001 노트를 고칠게요\"},"
                + "{\"type\":\"tool_use\",\"id\":\"t1\",\"name\":\"Bash\",\"input\":{\"description\":\"\",\"command\":\"ls -la\"}}]}}");
        // 결과 줄(회차를 안 만든다)에도 같은 id -> 같은 순번이라 연달아 두 번 안 적는다. 호출보다 1초 앞선 시각
        lines.add("{\"type\":\"user\",\"timestamp\":\"2026-09-20T01:00:03Z\",\"message\":{\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"t1\",\"content\":\"ndm1abcd-00000001 ok\"}]}}");
        // L10. 수는 글 값이 아니라서 다음 키로 넘어간다
        lines.add("{\"type\":\"assistant\",\"timestamp\":\"2026-09-20T01:00:05Z\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"id\":\"t2\",\"name\":\"Bash\",\"input\":{\"description\":5,\"command\":\"ls\"}}]}}");
        // L11, L14, L15. 할 일
        lines.add(toolUse("c1", "TaskCreate", "{\"description\":\"a\"}"));
        lines.add(toolResult("c1", "\"Task #1 created successfully: x\\ry\""));
        lines.add(toolUse("c2", "TaskUpdate", "{\"status\":\"deleted\"}"));
        lines.add(toolResult("c2", "\"Updated task #1\""));
        lines.add(toolUse("c3", "TaskUpdate", "{\"status\":\"in_progress\"}"));
        lines.add(toolResult("c3", "\"Updated task #1\""));
        lines.add(toolUse("c4", "TaskCreate", "{\"description\":\"b\"}"));
        lines.add(toolResult("c4", "5"));
        lines.add(toolUse("c5", "TaskCreate", "{\"description\":\"c\"}"));
        lines.add(toolResult("c5", "\"Task #99999999999 created successfully: x\""));
        // Second review N3: DOTALL would incorrectly accept a Unicode line separator inside the task result.
        lines.add(toolUse("unicode-task", "TaskCreate", "{}"));
        lines.add(toolResult("unicode-task", "\"Task #2 created successfully: a\\u2028b\""));
        // L13. 링크 호스트
        lines.add(user("2026-09-20T01:00:09Z", "http://LOCALHOST/x http://user@localhost/x http://localhost?x=1 http://localhost:1420/x http://127.0.0.1/abc"));
        // L17. U+3000 한 글자뿐인 사람 말은 버린다
        lines.add(user("2026-09-20T01:00:10Z", "　"));
        Files.writeString(projects.resolve(sid + ".jsonl"), String.join("\n", lines) + "\n");

        Map<String, Object> res = ok(v, "read-session", sid, "urls=true", "limit=500", "author=ndm1abcd-00000001");
        @SuppressWarnings("unchecked") Map<String, Object> author = (Map<String, Object>) res.get("author");
        eq(author.get("mentions"), List.of(5L), "M3: mention index is taken after the line's turns");
        check(!author.containsKey("creation"), "M3: one mention has no creation window");
        @SuppressWarnings("unchecked") List<Map<String, Object>> turns = (List<Map<String, Object>>) res.get("turns");
        Map<String, Object> bash1 = turns.get(5), bash2 = turns.get(6);
        @SuppressWarnings("unchecked") Map<String, Object> tool1 = (Map<String, Object>) bash1.get("tool");
        @SuppressWarnings("unchecked") Map<String, Object> tool2 = (Map<String, Object>) bash2.get("tool");
        check(!tool1.containsKey("arg"), "L10: empty description is the found value so arg has no key");
        eq(tool2.get("arg"), "ls", "L10: a number is not a text value");
        eq(String.valueOf(tool1.get("seconds")), "-1.0", "L12: negative tool seconds stay");

        @SuppressWarnings("unchecked") List<Map<String, Object>> todos = (List<Map<String, Object>>) res.get("todos");
        eq(todos.size(), 1, "L14: deleted then in_progress brings the task back");
        eq(todos.get(0).get("title"), "x", "L11: first line splits at CR");
        eq(todos.get(0).get("status"), "in_progress", "L14: status after revival");
        @SuppressWarnings("unchecked") Map<String, Object> source = (Map<String, Object>) res.get("source");
        eq(source.get("skipped"), 3, "L14, L15, N3: invalid task results each skip once");

        @SuppressWarnings("unchecked") List<Map<String, Object>> urls = (List<Map<String, Object>>) res.get("urls");
        List<Object> found = urls.stream().map(u -> u.get("url")).toList();
        eq(found, List.of("http://LOCALHOST/x", "http://user@localhost/x", "http://localhost?x=1"), "L13: only exact local hosts are dropped");
        check(turns.stream().noneMatch(x -> "　".equals(x.get("text"))), "L17: U+3000 only turn is dropped");
    }

    /** M2, L16. 제목 읽기의 창. */
    private static void titleWindows(Path projects) throws Exception {
        Path v = Files.createTempDirectory("part4-review-vault");
        String pad = "x".repeat(70_000);

        // M2. 머리 창 전체가 한 줄의 가운데면, 버린 조각까지 든 창에서 cwd 를 찾는다
        String headId = "f1a50000-0000-4000-8000-0000000000b1";
        Files.writeString(projects.resolve(headId + ".jsonl"),
                "{\"type\":\"user\",\"cwd\":\"/Users/me/proj\",\"pad\":\"" + pad + "\"}\n{\"type\":\"user\"}\n");
        Map<String, Object> head = titleOf(v, headId);
        eq(head.get("cwd"), "/Users/me/proj", "M2: cwd found in the whole head window");

        // M2. 끝 줄이 창보다 길면 꼬리 창의 글자에서 마지막 브랜치를 찾는다(머리의 옛 브랜치로 안 물러선다)
        String tailId = "f1a50000-0000-4000-8000-0000000000b2";
        Files.writeString(projects.resolve(tailId + ".jsonl"),
                "{\"type\":\"user\",\"gitBranch\":\"old/branch\"}\n{\"type\":\"user\",\"pad\":\"" + pad + "\",\"gitBranch\":\"new/branch\"}\n");
        eq(titleOf(v, tailId).get("branch"), "new/branch", "M2: branch found in the whole tail window");

        // L16. 글자 대체가 찾은 빈 글은 없는 것이다
        String emptyId = "f1a50000-0000-4000-8000-0000000000b3";
        Files.writeString(projects.resolve(emptyId + ".jsonl"), "{\"type\":\"user\",\"gitBranch\":\"\",\"cwd\":\"\"}\n");
        Map<String, Object> empty = titleOf(v, emptyId);
        check(empty.get("branch") == null && empty.get("cwd") == null, "L16: empty branch and cwd are null");
        String spaceId = "f1a50000-0000-4000-8000-0000000000b4";
        Files.writeString(projects.resolve(spaceId + ".jsonl"), "{\"type\":\"user\",\"gitBranch\":\" \",\"cwd\":\" \"}\n");
        Map<String, Object> spaces = titleOf(v, spaceId);
        eq(spaces.get("branch"), " ", "N2: whitespace is a non-empty branch");
        eq(spaces.get("cwd"), " ", "N2: whitespace is a non-empty cwd");
        String literalId = "f1a50000-0000-4000-8000-0000000000b5";
        Files.writeString(projects.resolve(literalId + ".jsonl"), "{\"type\":\"user\",\"gitBranch\":true,\"cwd\":5}\n");
        Map<String, Object> literals = titleOf(v, literalId);
        check(literals.get("branch") == null && literals.get("cwd") == null, "N2: branch and cwd require JSON strings");
    }

    @SuppressWarnings("unchecked")
    private static Map<String, Object> titleOf(Path v, String sid) {
        return ((List<Map<String, Object>>) ok(v, "session-titles", sid).get("sessions")).get(0);
    }

    @SuppressWarnings("unchecked")
    private static Map<String, Object> turn(Map<String, Object> thread, int index) {
        return ((List<Map<String, Object>>) thread.get("turns")).get(index);
    }

    private static String[] with(String[] base, String extra) {
        String[] out = Arrays.copyOf(base, base.length + 1);
        out[base.length] = extra;
        return out;
    }

    private static String[] withId(String[] base, String chatId, String extra) {
        String[] out = with(base, extra);
        out[2] = "chat-id=" + chatId;
        return out;
    }

    private static String user(String at, String text) {
        return "{\"type\":\"user\",\"timestamp\":\"" + at + "\",\"message\":{\"content\":\"" + text + "\"}}";
    }

    private static String assistantText(String at, String text) {
        return "{\"type\":\"assistant\",\"timestamp\":\"" + at + "\",\"message\":{\"content\":\"" + text + "\"}}";
    }

    private static String toolUse(String id, String name, String input) {
        return "{\"type\":\"assistant\",\"timestamp\":\"2026-09-20T01:00:06Z\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"id\":\"" + id + "\",\"name\":\"" + name + "\",\"input\":" + input + "}]}}";
    }

    private static String toolResult(String id, String contentJson) {
        return "{\"type\":\"user\",\"timestamp\":\"2026-09-20T01:00:07Z\",\"message\":{\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"" + id + "\",\"content\":" + contentJson + "}]}}";
    }
}
