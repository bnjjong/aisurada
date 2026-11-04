package aisurada;

import aisurada.engine.EngineExecutor;
import aisurada.engine.EngineResult;
import aisurada.out.JsonWriter;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Map;

import static aisurada.ChapterTestSupport.*;

/**
 * 2장 문답 표식과 문답 파일 (get-ask, append-ask-turn) 계약 검증 테스트.
 */
public final class AskCommandsTest {
    public static void main(String[] args) throws Exception {
        System.setProperty("aisurada.enable.part4", "true");
        try (var t = new ChapterTestSupport("ask-commands")) {
            testTargetValidation(t);
            testGetAskNonExistent(t);
            testAppendAskTurnAndRead(t);
            testSizeLimitRejection(t);
            t.done();
        } finally {
            System.clearProperty("aisurada.enable.part4");
        }
        System.out.println("AskCommandsTest: all checks passed!");
    }

    private static void testTargetValidation(ChapterTestSupport t) throws Exception {
        Path v = t.vault();

        // 인자 없음
        var r1 = error(v, 400, "get-ask");
        check(stderr(r1).contains("인자가 하나 필요하다: get-ask <노트 id> [하위 노트 id]"), "get-ask missing positional");

        var r2 = error(v, 400, "append-ask-turn");
        check(stderr(r2).contains("인자가 하나 필요하다: append-ask-turn <노트 id> [하위 노트 id]"), "append-ask-turn missing positional");

        // 노트 id 모양 아님
        var r3 = error(v, 400, "get-ask", "invalid_id!");
        check(stderr(r3).contains("노트 id 모양이 아니다"), "invalid root id format");

        // 존재하지 않는 노트
        var r4 = error(v, 404, "get-ask", "ndfixture-99999999");
        check(stderr(r4).contains("Note not found: ndfixture-99999999"), "note not found 404");
    }

    private static void testGetAskNonExistent(ChapterTestSupport t) throws Exception {
        Path v = t.vault();
        // fixture 폴더 노트 생성
        root(v, "mynote", 1, "2026-06-16", "", "# Title\nbody\n");
        String noteId = "ndfixture-00000001";
        var res = run(v, "get-ask", noteId);
        if (res.status() != 0) {
            System.err.println("GET-ASK ERROR: status=" + res.status() + " stderr=" + stderr(res));
        }
        eq(res.status(), 0, "get-ask on empty thread returns 0");
        eq(stdout(res).trim(), "null", "get-ask on empty thread stdout is null");
    }

    private static void testAppendAskTurnAndRead(ChapterTestSupport t) throws Exception {
        Path v = t.vault();
        root(v, "mynote", 1, "2026-06-16", "", "# Title\nbody\n");
        String noteId = "ndfixture-00000001";
        String sess1 = "3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b";
        String turn1 = "akmhj0q2rs-9c1e5a7b";
        String mark1 = "akmhj0q2rs-9c1e5a7b";

        // 1. 필수 인자 누락 검사
        var errRes = error(v, 400, "append-ask-turn", noteId, "session-id=" + sess1);
        check(stderr(errRes).contains("turn-id 이 필요하다"), "missing turn-id error");

        // 2. 첫 턴 추가 (새 밑줄 물음)
        var addRes1 = run(v, "append-ask-turn", noteId,
                "session-id=" + sess1,
                "turn-id=" + turn1,
                "mark-id=" + mark1,
                "asked-at=2026-11-03T02:15:40.512Z",
                "quote=노트를 다시 열 때 비운다",
                "question=언제 비우나?",
                "answer=노트를 다시 열 때 **캐시**를 비운다.",
                "model=claude-sonnet-5-5",
                "effort=low",
                "elapsed-ms=7600",
                "session-lines=21"
        );
        eq(addRes1.status(), 0, "append-ask-turn 1st turn status 0");
        String out1 = stdout(addRes1);
        check(out1.contains("\"noteId\": \"ndfixture-00000001\""), "resp has noteId");
        check(out1.contains("\"sessionId\": \"" + sess1 + "\""), "resp has sessionId");
        check(out1.contains("\"path\": "), "resp has path");

        // 3. get-ask로 방금 쓴 ask.json 읽기
        var getRes1 = run(v, "get-ask", noteId);
        if (getRes1.status() != 0) {
            System.err.println("GET-ASK1 ERROR: status=" + getRes1.status() + " code=" + getRes1.errorCode() + " stderr=" + stderr(getRes1));
        }
        eq(getRes1.status(), 0, "get-ask read success");
        String getOut1 = stdout(getRes1);
        check(getOut1.contains("\"question\": \"언제 비우나?\""), "get-ask contains question");
        check(getOut1.contains("\"sessionLines\": 21"), "get-ask contains sessionLines");
        check(!getOut1.contains("\"path\": "), "get-ask does not have path");
        check(!getOut1.contains("\"v\": "), "get-ask does not have v");

        // 4. 둘째 턴 추가 (이어 묻기 - 다른 session-id를 주어도 기존 session-id가 이김!)
        String turn2 = "akmhj0w8tu-0d4b6f2a";
        var addRes2 = run(v, "append-ask-turn", noteId,
                "session-id=different-session-uuid",
                "turn-id=" + turn2,
                "mark-id=" + mark1,
                "asked-at=2026-11-03T02:17:02.003Z",
                "question=더 자세히 설명해줘",
                "answer=캐시 적중률에 따라 다릅니다.",
                "model=claude-opus-5-5",
                "effort=high",
                "elapsed-ms=42000"
        );
        eq(addRes2.status(), 0, "append-ask-turn 2nd turn status 0");
        String out2 = stdout(addRes2);
        check(out2.contains("\"sessionId\": \"" + sess1 + "\""), "existing sessionId preserved");
        check(out2.contains(turn2), "second turn id present");

        // 5. get-ask 재확인 (턴이 2개여야 함)
        var getRes2 = run(v, "get-ask", noteId);
        String getOut2 = stdout(getRes2);
        check(getOut2.contains(turn1) && getOut2.contains(turn2), "both turns present");
    }

    private static void testSizeLimitRejection(ChapterTestSupport t) throws Exception {
        Path v = t.vault();
        root(v, "mynote", 1, "2026-06-16", "", "# Title\nbody\n");
        String noteId = "ndfixture-00000001";

        // 1. 단일 인자 상한 검사 (20,000자 상한 초과시 400)
        String oversizedText = "x".repeat(20001);
        var errTooLong = error(v, 400, "append-ask-turn", noteId,
                "session-id=sess", "turn-id=t1", "mark-id=m1", "asked-at=2026-11-03T00:00:00Z",
                "question=" + oversizedText, "answer=ok", "model=sonnet", "effort=low", "elapsed-ms=100");
        check(stderr(errTooLong).contains("질문이 상한을 넘었다"), "oversized question 400");

        // 2. D1 고침 (cc12691b): 전체 문답 파일 1,000,000 바이트 상한 초과시 WL-500 거절 및 원본 보존
        // 먼저 정상 턴 1개 작성
        run(v, "append-ask-turn", noteId,
                "session-id=sess-1234", "turn-id=t-init", "mark-id=m-init", "asked-at=2026-11-03T00:00:00Z",
                "question=첫질문", "answer=첫답변", "model=sonnet", "effort=low", "elapsed-ms=100");

        Path askFile = v.resolve("notes/demo/mynote/ask.json");
        check(Files.exists(askFile), "ask.json created");
        String originalContent = Files.readString(askFile, StandardCharsets.UTF_8);

        // 거의 1MB에 달하는 더미 턴들로 채워진 ask.json을 직접 쓰거나 준비
        // 995,000 바이트 상당의 ask.json 구성
        StringBuilder largeAnswer = new StringBuilder();
        for (int i = 0; i < 990; i++) {
            largeAnswer.append("a".repeat(1000));
        }
        String bigTurnId = "t-big";
        String rawBigJson = "{\n  \"v\": 1,\n  \"noteId\": \"" + noteId + "\",\n  \"sessionId\": \"sess-1234\",\n  \"turns\": [\n"
                + "    {\n      \"id\": \"" + bigTurnId + "\",\n      \"markId\": \"m1\",\n      \"askedAt\": \"2026-11-03T00:00:00Z\",\n"
                + "      \"question\": \"big\",\n      \"answer\": \"" + largeAnswer + "\",\n      \"model\": \"m\",\n      \"effort\": \"low\",\n      \"elapsedMs\": 1\n    }\n"
                + "  ]\n}\n";
        Files.writeString(askFile, rawBigJson, StandardCharsets.UTF_8);
        long sizeBefore = Files.size(askFile);
        check(sizeBefore > 990000, "file is close to 1MB");

        // 이제 새로운 턴(20,000 바이트)을 추가 시도 -> 1,000,000 바이트 초과로 500 실패해야 함
        String addedAnswer = "b".repeat(15000);
        var err500 = error(v, 500, "append-ask-turn", noteId,
                "session-id=sess-1234", "turn-id=t-overflow", "mark-id=m-init", "asked-at=2026-11-03T00:00:00Z",
                "question=overflow-check", "answer=" + addedAnswer, "model=sonnet", "effort=low", "elapsed-ms=100");
        check(stderr(err500).contains("문답 파일이 상한을 넘어 붙일 수 없다"), "oversized file 500");

        // 실패 후 기존 파일이 덮어써지지 않고 그대로 유지되었는지 확인 (원자적 쓰기 & 롤백 보존)
        String contentAfter = Files.readString(askFile, StandardCharsets.UTF_8);
        eq(contentAfter, rawBigJson, "ask.json preserved on 500 overflow");
    }
}
