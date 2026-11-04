package aisurada;

import aisurada.cli.AisException;
import aisurada.engine.EngineExecutor;
import aisurada.engine.EngineResult;
import aisurada.session.SessionCommands;
import aisurada.session.SessionTranscript;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;

/**
 * 7장 Claude 세션 전사 읽기(read-session, session-titles, recent-sessions) 및
 * D18 결함 고침(UTF-8 대체 문자 복원) 검증 테스트.
 */
public final class SessionCommandsTest {

    private static int checks = 0;

    private static void check(boolean cond, String msg) {
        checks++;
        if (!cond) {
            throw new AssertionError("Check failed: " + msg);
        }
    }

    public static void main(String[] args) throws Exception {
        System.setProperty("aisurada.enable.part4", "true");

        Path tempDir = Files.createTempDirectory("aisurada-session-test");
        Path vaultRoot = tempDir.resolve("vault");
        Files.createDirectories(vaultRoot);
        Path homeDir = tempDir.resolve("home");
        Path projectsDir = homeDir.resolve(".claude").resolve("projects");
        Files.createDirectories(projectsDir);

        System.setProperty("aisurada.claude.home", homeDir.toString());

        try {
            testArgValidation(vaultRoot);
            testMissingSession(vaultRoot);
            testTranscriptParsingAndD18(vaultRoot, projectsDir);
            testSessionTitles(vaultRoot, projectsDir);
            testRecentSessions(vaultRoot, projectsDir);
            System.out.println("session-commands: " + checks + " checks passed");
            System.out.println("SessionCommandsTest: all checks passed!");
        } finally {
            deleteRecursively(tempDir.toFile());
        }
    }

    private static void testArgValidation(Path vaultRoot) {
        // 1. read-session 인자 없음
        EngineResult r1 = EngineExecutor.execute(vaultRoot, new String[]{"read-session"}, false, 0);
        check(r1.status() == 3, "read-session without args must fail with 3");
        check(new String(r1.stderrBytes()).contains("인자가 하나 필요하다"), "read-session usage message required");

        // 2. read-session 잘못된 세션 id
        EngineResult r2 = EngineExecutor.execute(vaultRoot, new String[]{"read-session", "../../etc/passwd"}, false, 0);
        check(r2.status() == 3, "read-session with path traversal id must fail with 3");
        check(new String(r2.stderrBytes()).contains("세션 식별자 모양이 아니에요"), "invalid session id message");

        // 3. read-session 잘못된 limit
        EngineResult r3 = EngineExecutor.execute(vaultRoot, new String[]{"read-session", "f1a50000-0000-4000-8000-000000000001", "limit=abc"}, false, 0);
        check(r3.status() == 3, "invalid limit number must fail");
        check(new String(r3.stderrBytes()).contains("limit 은 숫자여야 한다"), "limit error message");

        // 4. read-session 잘못된 before
        EngineResult r4 = EngineExecutor.execute(vaultRoot, new String[]{"read-session", "f1a50000-0000-4000-8000-000000000001", "before=xyz"}, false, 0);
        check(r4.status() == 3, "invalid before number must fail");
        check(new String(r4.stderrBytes()).contains("before 은 숫자여야 한다"), "before error message");

        // 5. read-session 잘못된 urls
        EngineResult r5 = EngineExecutor.execute(vaultRoot, new String[]{"read-session", "f1a50000-0000-4000-8000-000000000001", "urls=invalid"}, false, 0);
        check(r5.status() == 3, "invalid urls flag must fail");
        check(new String(r5.stderrBytes()).contains("urls 은 true 나 false 여야 한다"), "urls error message");

        // 6. read-session 잘못된 note id
        EngineResult r6 = EngineExecutor.execute(vaultRoot, new String[]{"read-session", "f1a50000-0000-4000-8000-000000000001", "note=short"}, false, 0);
        check(r6.status() == 3, "short note id must fail");
        check(new String(r6.stderrBytes()).contains("노트 id 모양이 아니에요"), "note id regex error");

        // 7. read-session author 개수 위반 (9개)
        String nineIds = "ndm1abcd-00000001,ndm1abcd-00000002,ndm1abcd-00000003,ndm1abcd-00000004," +
                "ndm1abcd-00000005,ndm1abcd-00000006,ndm1abcd-00000007,ndm1abcd-00000008,ndm1abcd-00000009";
        EngineResult r7 = EngineExecutor.execute(vaultRoot, new String[]{"read-session", "f1a50000-0000-4000-8000-000000000001", "author=" + nineIds}, false, 0);
        check(r7.status() == 3, "author > 8 must fail");
        check(new String(r7.stderrBytes()).contains("author 의 노트 id 는 1개에서 8개까지예요: 9"), "author count error");

        // 8. session-titles 잘못된 세션 id
        EngineResult r8 = EngineExecutor.execute(vaultRoot, new String[]{"session-titles", "bad_id"}, false, 0);
        check(r8.status() == 3, "bad session title id must fail");
        check(new String(r8.stderrBytes()).contains("세션 식별자 모양이 아니에요"), "bad id error");

        // 9. recent-sessions 잘못된 days
        EngineResult r9 = EngineExecutor.execute(vaultRoot, new String[]{"recent-sessions", "days=0"}, false, 0);
        check(r9.status() == 3, "days=0 must fail");
        check(new String(r9.stderrBytes()).contains("days 는 1 부터 30 사이여야 해요"), "days range error");

        EngineResult r10 = EngineExecutor.execute(vaultRoot, new String[]{"recent-sessions", "days=31"}, false, 0);
        check(r10.status() == 3, "days=31 must fail");

        EngineResult r11 = EngineExecutor.execute(vaultRoot, new String[]{"recent-sessions", "days=foo"}, false, 0);
        check(r11.status() == 3, "days=foo must fail");
        check(new String(r11.stderrBytes()).contains("days 은 숫자여야 한다"), "days number error");
    }

    private static void testMissingSession(Path vaultRoot) {
        String sId = "f1a50000-0000-4000-8000-000000000099";
        EngineResult res = EngineExecutor.execute(vaultRoot, new String[]{"read-session", sId, "note=ndm1abcd-00000001", "author=ndm1abcd-00000001"}, false, 0);
        check(res.status() == 0, "missing session must return 0");
        String stdout = new String(res.stdoutBytes());
        check(stdout.contains("\"sessionId\": \"f1a50000-0000-4000-8000-000000000099\""), "stdout contains sessionId");
        check(stdout.contains("\"file\": null"), "file is null");
        check(stdout.contains("\"bytes\": 0"), "bytes is 0");
        check(stdout.contains("\"lines\": 0"), "lines is 0");
        check(stdout.contains("\"skipped\": 0"), "skipped is 0");
        check(stdout.contains("\"messages\": 0"), "messages is 0");
        check(stdout.contains("\"hasMore\": false"), "hasMore is false");
        check(stdout.contains("\"urls\": []"), "urls is empty array");
        check(stdout.contains("\"runningAgents\": []"), "runningAgents is empty");
        check(stdout.contains("\"todos\": []"), "todos is empty");
        check(stdout.contains("\"runState\": null"), "runState is null");
        check(stdout.contains("\"noteMentions\": []"), "noteMentions is empty array");
        check(stdout.contains("\"author\": null"), "author is null");
        check(stdout.contains("\"turns\": []"), "turns is empty array");
    }

    private static void testTranscriptParsingAndD18(Path vaultRoot, Path projectsDir) throws Exception {
        Path projDir = projectsDir.resolve("-Users-test-vault");
        Files.createDirectories(projDir);

        String sId = "f1a50000-0000-4000-8000-000000000001";
        Path sessionFile = projDir.resolve(sId + ".jsonl");

        // 합성 전사 생성
        // 줄 1: user text (ndm1abcd-00000001 언급, URL 포함)
        // 줄 2: assistant text (model 설정, effort, usage)
        // 줄 3: assistant tool_use (Agent 호출)
        // 줄 4: assistant tool_use (TaskCreate 호출)
        // 줄 5: user tool_result (TaskCreate 성공)
        // 줄 6: user tool_result (일반 Read)
        // 줄 7: assistant synthetic (quotaLimits 발생)
        // 줄 8: D18 검증용 깨진 UTF-8 바이트 시퀀스를 담은 줄
        // 줄 9: assistant 마지막 말 (ndm1abcd-00000001 언급)

        ByteArrayOutputStream baos = new ByteArrayOutputStream();

        // 줄 1
        baos.write(("{\"type\":\"user\",\"timestamp\":\"2026-09-20T01:00:00Z\",\"message\":{\"content\":\"노트 ndm1abcd-00000001 참고해줘. https://github.com/aisurada/app/issues/42 링크와 http://localhost:8080 무시\"}}\n")
                .getBytes(StandardCharsets.UTF_8));
        // 줄 2
        baos.write(("{\"type\":\"assistant\",\"timestamp\":\"2026-09-20T01:00:01Z\",\"effort\":\"low\",\"message\":{\"model\":\"claude-sonnet-5\",\"usage\":{\"input_tokens\":40,\"cache_creation_input_tokens\":10,\"cache_read_input_tokens\":5},\"content\":\"네, 확인했습니다.\"}}\n")
                .getBytes(StandardCharsets.UTF_8));
        // 줄 3
        baos.write(("{\"type\":\"assistant\",\"timestamp\":\"2026-09-20T01:00:02Z\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"id\":\"call_agent_1\",\"name\":\"Agent\",\"input\":{\"description\":\"레일폭조사\"}}]}}\n")
                .getBytes(StandardCharsets.UTF_8));
        // 줄 4
        baos.write(("{\"type\":\"assistant\",\"timestamp\":\"2026-09-20T01:00:03Z\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"id\":\"call_task_1\",\"name\":\"TaskCreate\",\"input\":{\"description\":\"할일등록\"}}]}}\n")
                .getBytes(StandardCharsets.UTF_8));
        // 줄 5 (TaskCreate 성공 결과)
        baos.write(("{\"type\":\"user\",\"timestamp\":\"2026-09-20T01:00:05Z\",\"message\":{\"content\":[{\"type\":\"tool_result\",\"tool_use_id\":\"call_task_1\",\"is_error\":false,\"content\":\"Task #1 created successfully: 레일붙이기\"}]}}\n")
                .getBytes(StandardCharsets.UTF_8));
        // 줄 6 (Agent는 결과를 안 주어서 runningAgents로 남음)
        // 줄 7 (synthetic quotaLimits)
        baos.write(("{\"type\":\"assistant\",\"message\":{\"model\":\"<synthetic>\",\"content\":[]},\"quotaLimits\":{\"resetsAt\":1789000100}}\n")
                .getBytes(StandardCharsets.UTF_8));
        // 줄 8: D18 검증용 깨진 UTF-8 줄
        // 한글 "가" (0xEA 0xB0 0x80)의 앞 2바이트만 쓴 뒤 개행
        baos.write("{\"type\":\"user\",\"message\":{\"content\":\"깨진바이트: ".getBytes(StandardCharsets.UTF_8));
        baos.write(new byte[]{(byte) 0xEA, (byte) 0xB0});
        baos.write("\"}}\n".getBytes(StandardCharsets.UTF_8));
        // 줄 9
        baos.write(("{\"type\":\"assistant\",\"timestamp\":\"2026-09-20T01:00:50Z\",\"message\":{\"content\":\"ndm1abcd-00000001 마지막 수정 완료\"}}\n")
                .getBytes(StandardCharsets.UTF_8));

        Files.write(sessionFile, baos.toByteArray());

        // read-session 실행 및 검증
        EngineResult res = EngineExecutor.execute(vaultRoot, new String[]{
                "read-session", sId, "urls=true", "note=ndm1abcd-00000001", "author=ndm1abcd-00000001"
        }, false, 0);

        check(res.status() == 0, "D18: broken UTF-8 byte must NOT fail read-session (exit code 0)");
        String stdout = new String(res.stdoutBytes());

        check(stdout.contains("\"sessionId\": \"" + sId + "\""), "contains sessionId");
        check(stdout.contains("\"file\": \"" + sessionFile.toAbsolutePath() + "\""), "contains file path");
        check(stdout.contains("\"model\": \"claude-sonnet-5\""), "runState.model is claude-sonnet-5");
        check(stdout.contains("\"effort\": \"low\""), "runState.effort is low");
        check(stdout.contains("\"contextTokens\": 55"), "runState.contextTokens is 40+10+5=55");
        check(stdout.contains("\"quotaBlocked\": true"), "runState.quotaBlocked is true");
        check(stdout.contains("\"quotaResetsAt\": 1789000100"), "runState.quotaResetsAt is 1789000100");

        // URLs 검증: localhost는 urls 배열에서 제외되고 github url만 남아야 함
        check(stdout.contains("https://github.com/aisurada/app/issues/42"), "github URL collected");
        check(!stdout.matches("(?s).*\"urls\":\\s*\\[[^\\]]*localhost.*"), "localhost URL must not be in urls array");

        // runningAgents 검증: Agent 호출이 결과가 안 와서 남아있어야 함
        check(stdout.contains("\"what\": \"레일폭조사\""), "runningAgents contains 레일폭조사");

        // todos 검증: TaskCreate 결과로 레일붙이기 등록
        check(stdout.contains("\"title\": \"레일붙이기\""), "todos contains title");
        check(stdout.contains("\"status\": \"pending\""), "todos contains status pending");

        // author & noteMentions 검증
        check(stdout.contains("\"noteMentions\": ["), "contains noteMentions");
        check(stdout.contains("\"author\": {"), "contains author object");
        check(stdout.contains("\"note\": \"ndm1abcd-00000001\""), "author note matches");

        // D18 복원 검증: 깨진 바이트가 U+FFFD로 치환되어 텍스트에 포함되었는지
        check(stdout.contains("깨진바이트: \uFFFD"), "D18 replacement char U+FFFD present");
    }

    private static void testSessionTitles(Path vaultRoot, Path projectsDir) throws Exception {
        Path projDir = projectsDir.resolve("-Users-test-vault");
        String sId = "f1a50000-0000-4000-8000-000000000002";
        Path sessionFile = projDir.resolve(sId + ".jsonl");

        // 세션 파일에 cwd, custom-title, gitBranch 작성
        String content = "{\"type\":\"user\",\"cwd\":\"/Users/fixture/aisurada\",\"timestamp\":\"2026-09-20T00:00:00Z\",\"message\":{\"content\":\"시작\"}}\n" +
                "{\"type\":\"custom-title\",\"customTitle\":\"유저가붙인제목\"}\n" +
                "{\"type\":\"assistant\",\"gitBranch\":\"feat/test-branch\",\"message\":{\"content\":\"끝\"}}\n";
        Files.writeString(sessionFile, content, StandardCharsets.UTF_8);

        // 1. session-titles 빈 인자
        EngineResult r0 = EngineExecutor.execute(vaultRoot, new String[]{"session-titles"}, false, 0);
        check(r0.status() == 0, "session-titles with no args must return 0");
        check(new String(r0.stdoutBytes()).contains("\"sessions\": []"), "empty sessions array");

        // 2. 단일 및 복수 조회 (존재하는 것과 존재하지 않는 것)
        EngineResult r1 = EngineExecutor.execute(vaultRoot, new String[]{
                "session-titles", sId, "f1a50000-0000-4000-8000-00000000dead"
        }, false, 0);
        check(r1.status() == 0, "session-titles must succeed");
        String stdout = new String(r1.stdoutBytes());

        check(stdout.contains("\"found\": true"), "sId found is true");
        check(stdout.contains("\"title\": \"유저가붙인제목\""), "title matches custom-title");
        check(stdout.contains("\"branch\": \"feat/test-branch\""), "branch matches");
        check(stdout.contains("\"cwd\": \"/Users/fixture/aisurada\""), "cwd matches");

        check(stdout.contains("\"found\": false"), "dead session found is false");
    }

    private static void testRecentSessions(Path vaultRoot, Path projectsDir) throws Exception {
        Path projDir = projectsDir.resolve("-Users-test-vault");

        // 1. cli 세션 (최근 수정)
        String sId1 = "f1a50000-0000-4000-8000-000000000010";
        Path file1 = projDir.resolve(sId1 + ".jsonl");
        String content1 = "{\"type\":\"user\",\"entrypoint\":\"cli\",\"timestamp\":\"2026-09-20T01:00:00Z\",\"message\":{\"content\":\"cli 세션\"}}\n" +
                "{\"type\":\"custom-title\",\"customTitle\":\"터미널대화\"}\n";
        Files.writeString(file1, content1);

        // 2. sdk-cli 세션 (걸러져야 함)
        String sId2 = "f1a50000-0000-4000-8000-000000000020";
        Path file2 = projDir.resolve(sId2 + ".jsonl");
        String content2 = "{\"type\":\"user\",\"entrypoint\":\"sdk-cli\",\"timestamp\":\"2026-09-20T01:00:00Z\",\"message\":{\"content\":\"sdk-cli 세션\"}}\n";
        Files.writeString(file2, content2);

        EngineResult res = EngineExecutor.execute(vaultRoot, new String[]{"recent-sessions", "days=7"}, false, 0);
        check(res.status() == 0, "recent-sessions must return 0");
        String stdout = new String(res.stdoutBytes());

        check(stdout.contains("\"days\": 7"), "days is 7");
        check(stdout.contains(sId1), "cli session included");
        check(!stdout.contains(sId2), "sdk-cli session must be excluded");
        check(stdout.contains("\"title\": \"터미널대화\""), "title extracted");
    }

    private static void deleteRecursively(File file) {
        if (file.isDirectory()) {
            File[] children = file.listFiles();
            if (children != null) {
                for (File child : children) {
                    deleteRecursively(child);
                }
            }
        }
        file.delete();
    }
}
