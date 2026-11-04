package withlog;

import withlog.engine.EngineExecutor;
import withlog.engine.EngineResult;
import withlog.out.JsonWriter;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Map;

import static withlog.ChapterTestSupport.*;

/**
 * 8장 노트 옆 세션 레일과 레일 대화 (list-chats, append-chat-turn) 계약 검증 테스트.
 */
public final class ChatCommandsTest {
    public static void main(String[] args) throws Exception {
        System.setProperty("withlog.enable.part4", "true");
        try (var t = new ChapterTestSupport("chat-commands")) {
            testTargetValidation(t);
            testListChatsEmpty(t);
            testAppendChatTurnAndList(t);
            testChatSortingAndUnreadable(t);
            testD19SizeLimitRejection(t);
            t.done();
        } finally {
            System.clearProperty("withlog.enable.part4");
        }
        System.out.println("ChatCommandsTest: all checks passed!");
    }

    private static void testTargetValidation(ChapterTestSupport t) throws Exception {
        Path v = t.vault();

        // 인자 없음 (400)
        var r1 = error(v, 400, "list-chats");
        check(stderr(r1).contains("인자가 하나 필요하다: list-chats <노트 id> [하위 노트 id]"), "list-chats missing positional");

        var r2 = error(v, 400, "append-chat-turn");
        check(stderr(r2).contains("인자가 하나 필요하다: append-chat-turn <노트 id> [하위 노트 id]"), "append-chat-turn missing positional");

        // 노트 id 모양 아님 (400)
        var r3 = error(v, 400, "list-chats", "invalid_id!");
        check(stderr(r3).contains("노트 id 모양이 아니다"), "invalid root id format");

        // 존재하지 않는 노트 (404)
        var r4 = error(v, 404, "list-chats", "ndfixture-99999999");
        check(stderr(r4).contains("Note not found: ndfixture-99999999"), "note not found 404");
    }

    private static void testListChatsEmpty(ChapterTestSupport t) throws Exception {
        Path v = t.vault();
        // fixture 폴더 노트 생성
        root(v, "chatnote1", 1, "2026-06-16", "", "# Chat Note 1\nbody\n");
        String noteId = "ndfixture-00000001";

        // chats/ 폴더가 없을 때 -> chats: [], unreadable: [] (정상 종료 0)
        var res = run(v, "list-chats", noteId);
        check(res.status() == 0, "list-chats exit code 0 when chats/ absent");
        String out = stdout(res);
        check(out.contains("\"chats\":[]") || out.contains("\"chats\": []"), "empty chats array");
        check(out.contains("\"unreadable\":[]") || out.contains("\"unreadable\": []"), "empty unreadable array");
    }

    private static void testAppendChatTurnAndList(ChapterTestSupport t) throws Exception {
        Path v = t.vault();
        root(v, "chatnote1", 1, "2026-06-16", "", "# Chat Note 1\nbody\n");
        String noteId = "ndfixture-00000001";
        String chatId = "6b0f1e2a-3c4d-4e5f-8a9b-0c1d2e3f4a5b";
        String sourceSession = "f1a50000-0000-4000-8000-000000000001";

        // 필수 인자 누락 검사
        var r1 = error(v, 400, "append-chat-turn", noteId);
        check(stderr(r1).contains("chat-id 가 필요하다"), "missing chat-id error");

        var r2 = error(v, 400, "append-chat-turn", noteId, "chat-id=" + chatId);
        check(stderr(r2).contains("source-session 이 필요하다"), "missing source-session error");

        var r3 = error(v, 400, "append-chat-turn", noteId, "chat-id=invalid@chat!id", "source-session=" + sourceSession);
        check(stderr(r3).contains("대화 id 모양이 아니다"), "invalid chat-id format check");

        // 첫 번째 회차 추가 (새 파일 생성)
        var res1 = run(v, "append-chat-turn", noteId,
                "chat-id=" + chatId,
                "source-session=" + sourceSession,
                "turn-id=rt1",
                "asked-at=2026-11-03T03:00:00.000Z",
                "question=첫 번째 질문",
                "answer=첫 번째 답변",
                "model=claude-sonnet-5-5",
                "effort=low",
                "elapsed-ms=1500"
        );
        check(res1.status() == 0, "append-chat-turn first turn success");
        String out1 = stdout(res1);
        check(out1.contains("\"chat\""), "response wraps chat");
        check(out1.contains("\"path\""), "response wraps path");
        check(out1.contains("chats/" + chatId + ".json"), "path contains json filename");
        check(out1.contains(noteId), "noteId in chat");
        check(out1.contains(chatId), "chatId in chat");
        check(out1.contains(sourceSession), "sourceSessionId in chat");
        check(out1.contains("2026-11-03T03:00:00.000Z"), "createdAt in chat");
        check(out1.contains("1500"), "elapsedMs in turn");

        // 파일 직접 검사: v=1이 파일에 존재해야 함
        Path chatFile = v.resolve("notes/demo/chatnote1/chats/" + chatId + ".json");
        check(Files.exists(chatFile), "chat file was created");
        String fileContent = Files.readString(chatFile, StandardCharsets.UTF_8);
        check(fileContent.contains("\"v\": 1") || fileContent.contains("\"v\":1"), "file contains v: 1");

        // 두 번째 회차 추가 (기존 파일에 붙이기, 실패한 회차 예시: answer="" error="통신 오류")
        var res2 = run(v, "append-chat-turn", noteId,
                "chat-id=" + chatId,
                "source-session=should-be-ignored-by-existing-file",
                "turn-id=rt2",
                "asked-at=2026-11-03T03:05:00.000Z",
                "question=두 번째 질문",
                "answer=",
                "error=통신 오류 발생",
                "model=claude-opus-5-5",
                "effort=high",
                "elapsed-ms=2000"
        );
        check(res2.status() == 0, "append-chat-turn second turn success");
        String out2 = stdout(res2);
        check(out2.contains(sourceSession), "existing sourceSessionId wins");
        check(out2.contains("통신 오류 발생"), "error field included");

        // list-chats로 조회
        var listRes = run(v, "list-chats", noteId);
        check(listRes.status() == 0, "list-chats after append success");
        String listOut = stdout(listRes);
        check(listOut.contains(chatId), "chatId listed in chats");
        check(!listOut.contains("\"v\":"), "v excluded from list-chats element");
        check(listOut.contains("첫 번째 질문"), "turn 1 present");
        check(listOut.contains("두 번째 질문"), "turn 2 present");
    }

    private static void testChatSortingAndUnreadable(ChapterTestSupport t) throws Exception {
        Path v = t.vault();
        root(v, "chatnote1", 1, "2026-06-16", "", "# Chat Note 1\nbody\n");
        String noteId = "ndfixture-00000001";
        Path chatsDir = v.resolve("notes/demo/chatnote1/chats");
        Files.createDirectories(chatsDir);

        // 더 오래된 시각(02:00)의 대화 파일 수동 생성
        String earlierChatId = "early-chat-id-001";
        String earlierJson = """
        {
          "v": 1,
          "noteId": "ndfixture-00000001",
          "chatId": "early-chat-id-001",
          "sourceSessionId": "sess-early",
          "createdAt": "2026-11-03T02:00:00.000Z",
          "turns": []
        }
        """;
        Files.writeString(chatsDir.resolve(earlierChatId + ".json"), earlierJson, StandardCharsets.UTF_8);

        // 더 최신 시각(03:00)의 대화 파일 수동 생성
        String laterChatId = "6b0f1e2a-3c4d-4e5f-8a9b-0c1d2e3f4a5b";
        String laterJson = """
        {
          "v": 1,
          "noteId": "ndfixture-00000001",
          "chatId": "6b0f1e2a-3c4d-4e5f-8a9b-0c1d2e3f4a5b",
          "sourceSessionId": "sess-later",
          "createdAt": "2026-11-03T03:00:00.000Z",
          "turns": []
        }
        """;
        Files.writeString(chatsDir.resolve(laterChatId + ".json"), laterJson, StandardCharsets.UTF_8);

        // 깨진 JSON 파일 생성 -> unreadable에 등재되어야 함
        String brokenFilename = "broken-chat.json";
        Files.writeString(chatsDir.resolve(brokenFilename), "{ not valid json ...", StandardCharsets.UTF_8);

        // 정규식에 안 맞는 파일 -> 대화로 안 치므로 unreadable에도 안 들어가야 함
        Files.writeString(chatsDir.resolve("temp.tmp"), "temp data", StandardCharsets.UTF_8);
        Files.writeString(chatsDir.resolve("not-json.txt"), "text file", StandardCharsets.UTF_8);

        var listRes = run(v, "list-chats", noteId);
        check(listRes.status() == 0, "list-chats sorting test success");
        String out = stdout(listRes);

        // early-chat-id-001이 6b0f1e2a보다 앞에 위치해야 함
        int idxEarly = out.indexOf("early-chat-id-001");
        int idxLater = out.indexOf("6b0f1e2a-3c4d-4e5f-8a9b-0c1d2e3f4a5b");
        check(idxEarly >= 0 && idxLater >= 0 && idxEarly < idxLater, "chats sorted by createdAt ascending");

        // unreadable에 broken-chat.json이 포함되어야 함
        check(out.contains("\"unreadable\":[\"broken-chat.json\"]") || out.contains("\"unreadable\": [\"broken-chat.json\"]") || out.contains("broken-chat.json"), "broken chat in unreadable");
        check(!out.contains("temp.tmp"), "temp file excluded from unreadable");
        check(!out.contains("not-json.txt"), "txt file excluded from unreadable");
    }

    private static void testD19SizeLimitRejection(ChapterTestSupport t) throws Exception {
        Path v = t.vault();
        root(v, "chatnote1", 1, "2026-06-16", "", "# Chat Note 1\nbody\n");
        String noteId = "ndfixture-00000001";
        String largeChatId = "large-chat-test";

        // 거대한 질문(20,000자 초과) 시 400 에러
        String hugeQuestion = "a".repeat(20001);
        var r1 = error(v, 400, "append-chat-turn", noteId,
                "chat-id=" + largeChatId,
                "source-session=s1",
                "turn-id=t1",
                "asked-at=2026-11-03T00:00:00Z",
                "question=" + hugeQuestion,
                "answer=ok",
                "model=m",
                "effort=e",
                "elapsed-ms=10"
        );
        check(stderr(r1).contains("질문은 20000자 이하여야 한다"), "question 20k char limit check");

        // D19: 파일 크기가 1,000,000 바이트를 초과하도록 인위적으로 995,000 바이트짜리 파일을 두고 append 시도
        Path chatFile = v.resolve("notes/demo/chatnote1/chats/" + largeChatId + ".json");
        Files.createDirectories(chatFile.getParent());
        StringBuilder sb = new StringBuilder();
        sb.append("{\n  \"v\": 1,\n  \"noteId\": \"").append(noteId).append("\",\n  \"chatId\": \"").append(largeChatId).append("\",\n  \"sourceSessionId\": \"s1\",\n  \"createdAt\": \"2026-11-03T00:00:00Z\",\n  \"turns\": [\n");
        sb.append("    {\n      \"id\": \"t0\",\n      \"askedAt\": \"2026-11-03T00:00:00Z\",\n      \"question\": \"init\",\n      \"answer\": \"");
        // 약 995,000 바이트 채우기
        sb.append("x".repeat(995000));
        sb.append("\",\n      \"model\": \"m\",\n      \"effort\": \"low\",\n      \"elapsedMs\": 1\n    }\n  ]\n}\n");
        Files.writeString(chatFile, sb.toString(), StandardCharsets.UTF_8);

        // 여기에 10,000자짜리 새 답변을 붙이면 1,000,000바이트를 넘게 됨!
        String newAnswer = "y".repeat(10000);
        var r2 = error(v, 500, "append-chat-turn", noteId,
                "chat-id=" + largeChatId,
                "source-session=s1",
                "turn-id=t2",
                "asked-at=2026-11-03T01:00:00Z",
                "question=new question",
                "answer=" + newAnswer,
                "model=m",
                "effort=e",
                "elapsed-ms=10"
        );
        String err = stderr(r2);
        check(err.contains("레일 대화 파일이 상한을 넘어 붙일 수 없다"), "D19 rejection message keyword check");
        check(err.contains("상한은 1000000바이트다"), "D19 limit text check");
        check(err.contains("chats/" + largeChatId + ".json"), "D19 target path check");
    }
}
