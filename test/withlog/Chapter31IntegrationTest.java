package withlog;

import withlog.args.JsonReader;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.TimeUnit;
import static withlog.ChapterTestSupport.*;

/** 격리된 HOME에서 serve 시작/작성자 전환/정비/종료와 wl-cli의 명령 왕복을 검증한다. */
public final class Chapter31IntegrationTest {
    private static List<String> prefix;
    private static Map<String, String> env;
    private static Path vault;
    private static String git(String... args) throws Exception {
        List<String> cmd = new ArrayList<>(List.of("git", "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgsign=false")); cmd.addAll(List.of(args));
        ProcessBuilder builder = new ProcessBuilder(cmd).directory(vault.toFile()).redirectErrorStream(true);
        builder.environment().remove("GIT_INDEX_FILE"); builder.environment().put("GIT_DIR", vault.resolve(".git").toString()); builder.environment().put("GIT_WORK_TREE", vault.toString());
        Process child = builder.start(); String output = new String(child.getInputStream().readAllBytes(), StandardCharsets.UTF_8);
        if (child.waitFor() != 0) throw new AssertionError("git " + List.of(args) + ": " + output);
        return output;
    }
    private static String message() throws Exception { return git("show", "-s", "--format=%B", "HEAD"); }
    @SuppressWarnings("unchecked") private static Map<String, Object> request(String session, String... args) throws Exception {
        List<String> cmd = new ArrayList<>(prefix); cmd.add(Path.of("dist/wl-cli.jar").toAbsolutePath().toString()); cmd.addAll(List.of(args));
        ProcessBuilder builder = new ProcessBuilder(cmd); builder.environment().clear(); builder.environment().putAll(env);
        if (session != null) builder.environment().put("CLAUDE_CODE_SESSION_ID", session);
        Process child = builder.start();
        try {
            check(child.waitFor(12, TimeUnit.SECONDS), "cli completes");
            String out = new String(child.getInputStream().readAllBytes(), StandardCharsets.UTF_8), err = new String(child.getErrorStream().readAllBytes(), StandardCharsets.UTF_8);
            eq(child.exitValue(), 0, "cli " + List.of(args) + ": " + err);
            return (Map<String, Object>) value(JsonReader.parseFile(out));
        } finally { if (child.isAlive()) child.destroyForcibly().waitFor(); }
    }
    public static void main(String[] args) throws Exception {
        try (var t = new ChapterTestSupport("chapter31-socket")) {
            vault = t.vault(); Path home = Files.createDirectory(t.temp.resolve("home"));
            root(vault, "root", 1, "2026-06-16T10:00:00+09:00", "", "body\n"); git("init", "-b", "main");
            Path socket = Path.of("/tmp", "wl-ch31-" + UUID.randomUUID().toString().substring(0, 8) + ".sock");
            prefix = List.of(Path.of(System.getProperty("java.home"), "bin/java").toString(), "-Duser.home=" + home, "-jar");
            env = Map.of("HOME", home.toString(), "PATH", "/usr/bin:/bin", "WITHLOG_VAULT", vault.toString(), "WITHLOG_SIDECAR_SOCKET", socket.toString(), "WITHLOG_VAULT_AUTOCOMMIT_IDLE_SECONDS", "0");
            List<String> command = new ArrayList<>(prefix); command.add(Path.of("dist/wl-engine.jar").toAbsolutePath().toString()); command.add("serve");
            ProcessBuilder builder = new ProcessBuilder(command); builder.environment().clear(); builder.environment().putAll(env);
            builder.redirectOutput(home.resolve("server.out").toFile()); builder.redirectError(home.resolve("server.err").toFile()); Process server = builder.start();
            try {
                long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(12);
                while (!Files.exists(socket) && server.isAlive() && System.nanoTime() < deadline) Thread.sleep(20);
                check(Files.exists(socket), "server ready: " + Files.readString(home.resolve("server.err")));
                check(message().contains("Withlog-Reason: startup"), "startup committed before socket opened");
                check(message().startsWith("wl(outside)"), "startup outside writer");
                eq(request(null, "history", "ndfixture-00000001").get("count"), 1L, "history socket response");
                request(null, "update-agenda-facts", "agfixture-00000001", "facts=approved");
                request("session-A", "save_report", "{\"kind\":\"evening\",\"date\":\"2026-10-21\",\"body\":\"first tool\"}");
                String message = message(); check(message.startsWith("wl(app)"), "tool request commits previous app"); check(message.contains("Withlog-Reason: writer"), "writer reason through real request");
                request("session-B", "save_report", "{\"kind\":\"evening\",\"date\":\"2026-10-21\",\"body\":\"second tool\"}");
                message = message(); check(message.contains("Withlog-Session: session-A"), "new tool session commits previous session");
                deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(12);
                while (System.nanoTime() < deadline && !message().contains("Withlog-Reason: idle")) Thread.sleep(100);
                message = message(); check(message.contains("Withlog-Reason: idle"), "zero idle commits at maintenance tick"); check(message.contains("Withlog-Session: session-B"), "idle session retained");
                eq(request(null, "commit-vault").get("outcome"), "nothing", "explicit command over socket");
                request(null, "update-agenda-facts", "agfixture-00000001", "facts=shutdown facts");
                server.destroy(); check(server.waitFor(12, TimeUnit.SECONDS), "server shutdown completes");
                check(message().contains("Withlog-Reason: shutdown"), "shutdown commit before lock release");
                eq(git("show", "HEAD:.withlog/facts/demo.md"), "shutdown facts\n", "shutdown includes last write");
                check(!Files.exists(socket), "socket removed after teardown");
                // startup와 단발 호출 모두 설정 오류는 명령 처리 전에 종료 코드 1이다.
                for (String option : List.of("--withlog.vault.autocommit.enabled=", "--withlog.vault.autocommit.enabled=maybe", "--withlog.vault.snapshot.enabled=maybe", "--withlog.vault.autocommit.idle.seconds=", "--withlog.vault.autocommit.idle.seconds=9223372036854775808")) {
                    List<String> invalid = new ArrayList<>(prefix); invalid.add(Path.of("dist/wl-engine.jar").toAbsolutePath().toString()); invalid.addAll(List.of("history", "missing", option));
                    ProcessBuilder bad = new ProcessBuilder(invalid); bad.environment().clear(); bad.environment().putAll(env);
                    Process child = bad.start(); check(child.waitFor(10, TimeUnit.SECONDS), "invalid config exits"); eq(child.exitValue(), 1, "strict config " + option);
                }
                t.done();
            } finally { if (server.isAlive()) { server.destroyForcibly(); server.waitFor(); } Files.deleteIfExists(socket); }
        }
    }
}
