package withlog;

import withlog.args.JsonReader;
import withlog.args.JsonValue;
import withlog.config.EngineConfig;
import withlog.engine.EngineExecutor;
import withlog.engine.EngineResult;
import withlog.write.TestApparatus;

import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.time.Instant;
import java.util.*;

/** 24~30장 시험용 임시 파일과 실제 엔진 호출. 외부 fixture나 사용자 HOME은 사용하지 않는다. */
final class ChapterTestSupport implements AutoCloseable {
    static int checks;
    final Path temp;
    private final String name, oldNow, oldRandom;
    private final TimeZone oldZone;
    ChapterTestSupport(String name) throws Exception {
        this.name = name; temp = Files.createTempDirectory("aisurada-" + name + "-");
        oldNow = System.getProperty("withlog.test.now"); oldRandom = System.getProperty("withlog.test.random"); oldZone = TimeZone.getDefault();
        System.setProperty("withlog.test.now", Long.toString(Instant.parse("2026-10-21T05:05:07.123Z").toEpochMilli()));
        TimeZone.setDefault(TimeZone.getTimeZone("Asia/Seoul"));
    }
    Path vault() throws Exception {
        Path v = Files.createTempDirectory(temp, "vault-");
        put(v, ".withlog/workspace.json", "{}\n"); agenda(v, "demo", "agfixture-00000001", "데모"); return v;
    }
    static void agenda(Path v, String slug, String id, String title) throws Exception {
        put(v, ".withlog/agendas/" + slug + ".md", "---\nid: " + id + "\nslug: " + slug + "\ntitle: " + title + "\n---\n");
    }
    static void note(Path v, String path, String id, String slug, String created, String extra, String body) throws Exception {
        String text = "id: " + id + "\nagenda: " + slug + "\ntype: build\ntitle: " + path.substring(path.lastIndexOf('/') + 1) + "\ncreated: '" + created + "'\n" + extra;
        put(v, path, path.endsWith(".html") ? "<!--withlog\n" + text + "-->\n" + body : "---\n" + text + "---\n" + body);
    }
    static void root(Path v, String name, int id, String created, String extra, String body) throws Exception {
        note(v, "notes/demo/" + name + "/note.md", "ndfixture-" + String.format("%08x", id), "demo", created, extra, body);
    }
    static void put(Path v, String relative, String text) throws Exception {
        Path p = v.resolve(relative); Files.createDirectories(p.getParent()); Files.writeString(p, text);
    }
    static EngineResult run(Path v, String... args) { return EngineExecutor.execute(v, args, false, System.currentTimeMillis()); }
    static Object success(Path v, String... args) {
        EngineResult result = run(v, args); eq(result.status(), 0, "success " + Arrays.toString(args) + ": " + stderr(result));
        check(result.stdoutBytes().length > 0 && result.stdoutBytes()[result.stdoutBytes().length - 1] == '\n', "JSON ends with LF");
        return value(JsonReader.parseFile(new String(result.stdoutBytes(), StandardCharsets.UTF_8)));
    }
    @SuppressWarnings("unchecked") static Map<String, Object> ok(Path v, String... args) { return (Map<String, Object>) success(v, args); }
    @SuppressWarnings("unchecked") static List<Map<String, Object>> list(Path v, String... args) { return (List<Map<String, Object>>) success(v, args); }
    static EngineResult error(Path v, int code, String... args) {
        EngineResult result = run(v, args); eq(result.status(), 3, "failure status " + args[0]); eq(result.errorCode(), code, "WL code: " + stderr(result)); eq(result.stdoutBytes().length, 0, "failure stdout"); return result;
    }
    static String stderr(EngineResult r) { return new String(r.stderrBytes(), StandardCharsets.UTF_8); }
    static String stdout(EngineResult r) { return new String(r.stdoutBytes(), StandardCharsets.UTF_8); }
    static Object value(JsonValue v) {
        if (v instanceof JsonValue.JsonString s) return s.value();
        if (v instanceof JsonValue.JsonArray a) return a.elements().stream().map(ChapterTestSupport::value).toList();
        if (v instanceof JsonValue.JsonObject o) { Map<String, Object> r = new LinkedHashMap<>(); o.properties().forEach((k, x) -> r.put(k, value(x))); return r; }
        return switch (((JsonValue.JsonLiteral) v).raw()) { case "true" -> true; case "false" -> false; case "null" -> null; default -> Long.valueOf(((JsonValue.JsonLiteral) v).raw()); };
    }
    static void check(boolean condition, String label) { checks++; if (!condition) throw new AssertionError(label); }
    static void eq(Object actual, Object expected, String label) {
        if (actual instanceof Number a && expected instanceof Number e) { check(a.longValue() == e.longValue(), label + ": " + actual + " != " + expected); return; }
        check(Objects.equals(actual, expected), label + "\nactual=" + actual + "\nexpected=" + expected);
    }
    static void random(String hex) { System.setProperty("withlog.test.random", hex); TestApparatus.resetRandomQueue(); }
    static void sharded(Path v, Boolean value) { EngineConfig.activate(new EngineConfig(v.toString(), v.toString(), null, value, 10, false, 300, true)); }
    void done() { System.out.println(name + ": " + checks + " checks passed"); }
    public void close() throws Exception {
        EngineConfig.activate(null); TimeZone.setDefault(oldZone);
        if (oldNow == null) System.clearProperty("withlog.test.now"); else System.setProperty("withlog.test.now", oldNow);
        if (oldRandom == null) System.clearProperty("withlog.test.random"); else System.setProperty("withlog.test.random", oldRandom);
        TestApparatus.resetRandomQueue();
        try (var paths = Files.walk(temp)) { for (Path p : paths.sorted(Comparator.reverseOrder()).toList()) Files.deleteIfExists(p); }
    }
}
