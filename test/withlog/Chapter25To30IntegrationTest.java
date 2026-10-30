package withlog;

import withlog.args.JsonReader;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.TimeUnit;
import static withlog.ChapterTestSupport.*;

/** 실제 serve와 wl-cli 프레임에서 새 명령과 성공·실패 진단을 왕복한다. */
public final class Chapter25To30IntegrationTest {
    private record Reply(int status, String out, String err) {
        @SuppressWarnings("unchecked") Map<String, Object> object() { return (Map<String, Object>) value(JsonReader.parseFile(out)); }
    }
    private static List<String> prefix;
    private static Map<String, String> environment;
    private static Path cli;
    public static void main(String[] args) throws Exception {
        try (var t = new ChapterTestSupport("chapters25-30-socket")) {
            Path v = t.vault(), home = Files.createDirectory(t.temp.resolve("home"));
            root(v, "root", 1, "2026-06-16T10:00:00+09:00", "", "body"); put(v, "notes/demo/root/attach/file.bin", "1234");
            put(v, "notes/demo/root/comments.json", "[{\"id\":\"cmfixture-00000001\",\"createdAt\":\"2026-06-18T10:00Z\",\"body\":\"reply\"}]");
            Path socket = Path.of("/tmp", "wl-ch25-30-" + UUID.randomUUID().toString().substring(0, 8) + ".sock");
            cli = Path.of("dist/wl-cli.jar").toAbsolutePath();
            prefix = List.of(Path.of(System.getProperty("java.home"), "bin/java").toString(), "-Duser.home=" + home, "-Dwithlog.test.now=" + System.getProperty("withlog.test.now"), "-Duser.timezone=Asia/Seoul", "-jar");
            environment = Map.of("HOME", home.toString(), "WITHLOG_VAULT", v.toString(), "WITHLOG_SIDECAR_SOCKET", socket.toString(), "WITHLOG_VAULT_AUTOCOMMIT_ENABLED", "false");
            List<String> command = new ArrayList<>(prefix); command.add(Path.of("dist/wl-engine.jar").toAbsolutePath().toString()); command.addAll(List.of("serve", "--withlog.notes.shard.by.month=true"));
            ProcessBuilder builder = new ProcessBuilder(command); builder.environment().clear(); builder.environment().putAll(environment);
            builder.redirectOutput(home.resolve("server.out").toFile()); builder.redirectError(home.resolve("server.err").toFile()); Process server = builder.start();
            try {
                long until = System.nanoTime() + TimeUnit.SECONDS.toNanos(10);
                while (!Files.exists(socket) && server.isAlive() && System.nanoTime() < until) Thread.sleep(25);
                check(Files.exists(socket), "server ready: " + Files.readString(home.resolve("server.err")));
                Reply absent = request(3, "get-ask");
                eq(absent.err, "WL-409 이 판에는 4부(Claude 연동)가 없다. 문답과 레일 대화와 세션 기록 명령은 wl-claude 모듈을 넣어 지은 사이드카에서만 돈다.\n", "part4 absence over socket");
                check(request(3, "read-session", "id", "unknown=x").err.startsWith("WL-400"), "part4 syntax failure over socket");
                check(request(3, "append-chat-turn").err.startsWith("WL-409"), "part4 write preparation over socket");
                request(0, "save_report", "{\"kind\":\"evening\",\"date\":\"2026-06-16\",\"body\":\"회고\\n원문\"}");
                eq(request(0, "get-report", "rv-evening-2026-06-16").object().get("body"), "회고\n원문", "socket report UTF8");
                put(v, ".withlog/reports/rv-morning-2026-06-17.json", "broken");
                check(request(0, "list-reports").err.contains("회고 1건을 못 읽어"), "incomplete diagnostic returned over frame");
                request(0, "plan-day", "date=2026-06-16", "agenda=demo");
                check(request(0, "plan_day", "{\"date\":\"2026-06-16\",\"agenda\":\"demo\"}").err.contains("제목이 같은 노트"), "plan warning returned over frame");
                var deleted = request(0, "delete_doc", "{\"id\":\"ndfixture-00000001\"}").object();
                request(0, "list_trash", "{}"); request(0, "restore_doc", "{\"trashId\":\"" + deleted.get("trashId") + "\"}");
                eq(request(0, "check-notes", "date=2026-06-20").object().get("count"), 1, "socket note check");
                request(0, "export_markdown", "{}"); request(0, "shard-notes", "apply=true");
                eq(request(0, "verify-shard").object().get("ok"), true, "socket shard baseline");
                Files.delete(v.resolve("notes/demo/2026-06/root/attach/file.bin"));
                Reply mismatch = request(0, "verify-shard"); eq(mismatch.object().get("ok"), false, "socket mismatch flag");
                check(mismatch.err.contains("크기가 갈린 것 1 건"), "verify diagnostics returned over frame");
                check(request(3, "plan-day", "dryRun=yes").err.startsWith("WL-400"), "socket semantic error code/text");
                request(0, "empty-trash"); t.done();
            } finally {
                server.destroy(); if (!server.waitFor(5, TimeUnit.SECONDS)) server.destroyForcibly().waitFor(); Files.deleteIfExists(socket);
            }
        }
    }
    private static Reply request(int status, String... args) throws Exception {
        List<String> command = new ArrayList<>(prefix); command.add(cli.toString()); command.addAll(List.of(args));
        ProcessBuilder builder = new ProcessBuilder(command); builder.environment().clear(); builder.environment().putAll(environment); Process child = builder.start();
        try {
            check(child.waitFor(10, TimeUnit.SECONDS), "client completes");
            Reply reply = new Reply(child.exitValue(), new String(child.getInputStream().readAllBytes(), StandardCharsets.UTF_8), new String(child.getErrorStream().readAllBytes(), StandardCharsets.UTF_8));
            eq(reply.status, status, "client status " + args[0] + ": " + reply.err); if (status != 0) eq(reply.out, "", "failure empty stdout"); return reply;
        } finally { if (child.isAlive()) child.destroyForcibly().waitFor(); }
    }
}
