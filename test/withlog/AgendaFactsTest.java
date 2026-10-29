package withlog;

import withlog.args.JsonReader;
import withlog.args.JsonValue;
import withlog.engine.EngineExecutor;
import withlog.engine.EngineResult;
import withlog.vault.FactsReader;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Instant;
import java.util.*;
import java.util.concurrent.TimeUnit;
import java.util.logging.Handler;
import java.util.logging.LogRecord;
import java.util.logging.Logger;

/** 22장: 실제 파일, JDK 21 읽기 버퍼, 엔진 JSON 바이트와 wl-cli 왕복을 검증한다. */
public final class AgendaFactsTest {
    private static final String AGENDA = "agfixture-0000000a", NOTE = "ndfixture-00000001";
    private static final String PATH = ".withlog/facts/serving.md";
    private static final String TEXT = "# serving - 지금도 참인 것\n\n- 색인은 vault 밖에 둔다.\n- 아젠다 slug는 안 바뀐다.";
    private static final String GET_USAGE = "get-agenda-facts <아젠다 id>";
    private static final String UPDATE_USAGE = "update-agenda-facts <아젠다 id> facts=<지금도 참인 것. 빈 값이면 지운다>";
    private static final String BRIEF_USAGE = "facts-brief [from=<YYYY-MM-DD>] [to=<YYYY-MM-DD>] [min-new-file=<노트 수>]";
    private static final String ABSENT = """
            {
              "id": "agfixture-0000000a",
              "slug": "serving",
              "path": ".withlog/facts/serving.md",
              "limit": 2000,
              "editLimit": 10000
            }
            """;
    private static Path temp;
    private static int checks, warnings;

    public static void main(String[] args) throws Exception {
        temp = Files.createTempDirectory("aisurada-facts-test-");
        TimeZone zone = TimeZone.getDefault();
        String now = System.getProperty("withlog.test.now");
        Logger logger = Logger.getLogger(FactsReader.class.getName());
        Handler handler = new Handler() {
            public void publish(LogRecord record) { warnings++; }
            public void flush() {}
            public void close() {}
        };
        boolean parents = logger.getUseParentHandlers();
        logger.setUseParentHandlers(false); logger.addHandler(handler);
        try {
            TimeZone.setDefault(TimeZone.getTimeZone("Asia/Seoul"));
            System.setProperty("withlog.test.now", Long.toString(Instant.parse("2026-06-25T01:00:00Z").toEpochMilli()));
            validation(); mutations(); fileShapes(); briefing(); datesAndNumbers(); readingBoundaries(); serveAndCli();
            check(warnings > 0, "malformed UTF-8 is logged");
            System.out.println("chapter22: " + checks + " checks passed (isolated Java engine/files/JDK buffers)");
        } finally {
            logger.removeHandler(handler); logger.setUseParentHandlers(parents);
            TimeZone.setDefault(zone);
            if (now == null) System.clearProperty("withlog.test.now"); else System.setProperty("withlog.test.now", now);
            try (var paths = Files.walk(temp)) {
                for (Path p : paths.sorted(Comparator.reverseOrder()).toList()) Files.deleteIfExists(p);
            }
        }
    }

    private static Path vault() throws Exception {
        Path v = Files.createTempDirectory(temp, "vault-");
        write(v, ".withlog/workspace.json", "{}\n");
        agenda(v, "serving", AGENDA);
        note(v, "notes/serving/root/note.md", NOTE, "serving", "2026-06-16T23:30:00-10:00", "reading");
        return v;
    }
    private static void agenda(Path v, String slug, String id) throws Exception {
        write(v, ".withlog/agendas/" + slug + ".md", "---\nid: " + id + "\nslug: " + slug + "\ntitle: " + slug + "\n---\n");
    }
    private static void note(Path v, String path, String id, String slug, String created, String tier) throws Exception {
        write(v, path, "---\nid: " + id + "\nagenda: " + slug + "\ntitle: 검사\ntype: free\ncreated: '" + created + "'\ntier: " + tier + "\n---\n본문\n");
    }
    private static void write(Path v, String path, String text) throws Exception {
        Path p = v.resolve(path); Files.createDirectories(p.getParent()); Files.writeString(p, text);
    }
    private static EngineResult run(Path v, String... args) { return EngineExecutor.execute(v, args, false, System.currentTimeMillis()); }
    private static Map<String, Object> ok(Path v, String... args) {
        EngineResult r = run(v, args);
        eq(r.status(), 0, "success " + args[0] + ": " + new String(r.stderrBytes(), StandardCharsets.UTF_8));
        eq(r.stderrBytes().length, 0, "success stderr");
        return object(r.stdoutBytes());
    }
    private static void error(Path v, int code, String message, String... args) {
        EngineResult r = run(v, args);
        eq(r.status(), 3, "error status " + args[0]); eq(r.errorCode(), code, "WL code");
        eq(r.stdoutBytes().length, 0, "failure stdout");
        eq(new String(r.stderrBytes(), StandardCharsets.UTF_8), "WL-" + code + " " + message + "\n", "error bytes");
    }
    @SuppressWarnings("unchecked")
    private static Map<String, Object> object(byte[] bytes) {
        return (Map<String, Object>) value(JsonReader.parseFile(new String(bytes, StandardCharsets.UTF_8)));
    }
    private static Object value(JsonValue v) {
        if (v instanceof JsonValue.JsonString s) return s.value();
        if (v instanceof JsonValue.JsonArray a) return a.elements().stream().map(AgendaFactsTest::value).toList();
        if (v instanceof JsonValue.JsonObject o) {
            Map<String, Object> m = new LinkedHashMap<>(); o.properties().forEach((k, x) -> m.put(k, value(x))); return m;
        }
        return switch (((JsonValue.JsonLiteral) v).raw()) {
            case "true" -> true; case "false" -> false; case "null" -> null;
            default -> Integer.valueOf(((JsonValue.JsonLiteral) v).raw());
        };
    }
    @SuppressWarnings("unchecked")
    private static List<Map<String, Object>> items(Map<String, Object> r) { return (List<Map<String, Object>>) r.get("items"); }
    private static Map<String, Object> brief(Path v, String... extra) {
        List<String> args = new ArrayList<>(List.of("facts-brief", "from=2026-06-16", "to=2026-06-25"));
        args.addAll(List.of(extra)); return ok(v, args.toArray(String[]::new));
    }
    private static void eq(Object actual, Object expected, String label) {
        checks++; if (!Objects.equals(actual, expected)) throw new AssertionError(label + "\nexpected=" + expected + "\nactual=" + actual);
    }
    private static void check(boolean condition, String label) { eq(condition, true, label); }

    private static void validation() throws Exception {
        Path v = vault();
        for (String name : List.of("get_agenda_facts", "update_agenda_facts", "facts_brief")) {
            EngineResult r = run(v, name, "{}"); eq(r.status(), 2, "command-only " + name);
            check(new String(r.stderrBytes(), StandardCharsets.UTF_8).startsWith("모르는 명령이다: " + name + "\n"), "unknown name text");
        }
        error(v, 400, "get-agenda-facts 이 모르는 옵션이다: x. 쓸 수 있는 것은 [] 뿐이다.", "get-agenda-facts", "one", "two", " x =z");
        error(v, 400, "update-agenda-facts 이 모르는 옵션이다: f1. 쓸 수 있는 것은 [facts] 뿐이다.", "update-agenda-facts", "f1=");
        error(v, 400, "facts-brief 이 모르는 옵션이다: x. 쓸 수 있는 것은 [from, min-new-file, to] 뿐이다.", "facts-brief", "x=");
        error(v, 400, "get-agenda-facts 이 받는 자리 인자는 1 개인데 2 개를 받았다: " + GET_USAGE, "get-agenda-facts", "a", "b");
        error(v, 400, "update-agenda-facts 이 받는 자리 인자는 1 개인데 2 개를 받았다: " + UPDATE_USAGE, "update-agenda-facts", "a", "b");
        error(v, 400, "facts-brief 이 받는 자리 인자는 0 개인데 1 개를 받았다: " + BRIEF_USAGE, "facts-brief", "=x");
        for (String id : List.of("", "\u3000\u00a0")) {
            error(v, 400, "인자가 하나 필요하다: " + GET_USAGE, "get-agenda-facts", id);
            error(v, 400, "인자가 하나 필요하다: " + UPDATE_USAGE, "update-agenda-facts", id);
        }
        error(v, 400, "인자가 하나 필요하다: " + GET_USAGE, "get-agenda-facts");
        error(v, 400, "인자가 하나 필요하다: " + UPDATE_USAGE, "update-agenda-facts");
        error(v, 400, "아젠다 id 모양이 아니다: " + AGENDA + " ", "update-agenda-facts", AGENDA + " ");
        error(v, 400, "facts 가 필요하다. 사실층을 지우려면 빈 값을 준다.", "update-agenda-facts", "agmissing-00000001");
        error(v, 404, "Agenda not found: agmissing-00000001", "get-agenda-facts", "agmissing-00000001");
        error(v, 404, "Agenda not found: ag-recovered", "update-agenda-facts", "ag-recovered", "facts=x");
        write(v, "notes/serving/poison/note.md", "---\nmins: !!int abc\n---\n");
        eq(run(v, "update-agenda-facts", "bad").status(), 1, "reconcile precedes id validation");
        eq(run(v, "facts-brief", "from=bad").status(), 1, "reconcile precedes dates");
        ok(v, "get-agenda-facts", AGENDA); // 노트를 훑지 않는다.
        Files.delete(v.resolve(".withlog/workspace.json"));
        String workspace = "이 폴더는 아직 워크스페이스가 아니다: " + v +
                ". .withlog/workspace.json 이 없다. 경로가 맞는지 보고, 새로 만들 자리면 init-workspace 를 먼저 부른다.";
        error(v, 400, workspace, "update-agenda-facts", "bad");
        error(v, 400, workspace, "facts-brief", "from=bad");
        ok(v, "get-agenda-facts", AGENDA);
    }

    private static void mutations() throws Exception {
        Path v = vault(); Path file = v.resolve(PATH);
        eq(new String(run(v, "get-agenda-facts", AGENDA).stdoutBytes(), StandardCharsets.UTF_8), ABSENT, "absent exact bytes");
        Map<String, Object> saved = ok(v, "update-agenda-facts", AGENDA, " facts =\u00a0\n" + TEXT + "\n\u3000");
        eq(saved.keySet().stream().toList(), List.of("id", "slug", "path", "limit", "editLimit", "facts"), "ordered edit envelope");
        eq(saved.get("facts"), TEXT, "trimmed response"); eq(Files.readString(file), TEXT + "\n", "one final LF");
        eq(ok(v, "get-agenda-facts", AGENDA), saved, "read/write same shape");
        ok(v, "update-agenda-facts", AGENDA, "facts=" + TEXT); eq(Files.readString(file), TEXT + "\n", "repeat stable bytes");
        ok(v, "update-agenda-facts", AGENDA, "facts=first", "facts=a=b"); eq(Files.readString(file), "a=b\n", "last option and equals in value");
        for (String facts : List.of("a".repeat(2001), "😀".repeat(1000) + "x")) {
            for (String id : List.of(AGENDA, "agmissing-00000001"))
                error(v, 400, "사실층이 상한을 넘었다: 2001 자. 상한은 2000 자다.", "update-agenda-facts", id, "facts=" + facts);
            eq(Files.readString(file), "a=b\n", "oversize preserves bytes");
        }
        ok(v, "update-agenda-facts", AGENDA, "facts=\u3000" + "😀".repeat(1000) + "\n");
        eq(Files.readString(file), "😀".repeat(1000) + "\n", "2000 UTF-16 units accepted");
        Map<String, Object> deleted = ok(v, "update-agenda-facts", AGENDA, "facts= \u00a0\u3000");
        eq(deleted.keySet().stream().toList(), List.of("id", "slug", "path", "limit", "editLimit"), "delete omits facts");
        check(!Files.exists(file), "empty facts delete"); check(Files.isDirectory(file.getParent()), "delete leaves directory");
        Files.delete(file.getParent()); ok(v, "update-agenda-facts", AGENDA, "facts=");
        check(!Files.exists(file.getParent()), "absent delete creates no directory");
        Files.delete(v.resolve(".withlog/workspace.json")); ok(v, "get-agenda-facts", AGENDA);
    }

    private static void fileShapes() throws Exception {
        Path v = vault(); Path file = v.resolve(PATH); Files.createDirectories(file.getParent());
        Files.createDirectory(file);
        check(!ok(v, "get-agenda-facts", AGENDA).containsKey("facts"), "directory reads absent");
        error(v, 500, "파일을 쓰지 못했다: " + file, "update-agenda-facts", AGENDA, "facts=x");
        ok(v, "update-agenda-facts", AGENDA, "facts="); check(!Files.exists(file), "empty directory deleted");
        Files.createDirectory(file); Files.writeString(file.resolve("child"), "keep");
        error(v, 500, "파일을 지우지 못했다: " + file, "update-agenda-facts", AGENDA, "facts=");
        eq(Files.readString(file.resolve("child")), "keep", "nonempty directory preserved");
        Files.delete(file.resolve("child")); Files.delete(file);
        Path outside = temp.resolve("link-target.md"); Files.writeString(outside, "external");
        Files.createSymbolicLink(file, outside);
        eq(ok(v, "get-agenda-facts", AGENDA).get("facts"), "external", "read follows symlink");
        ok(v, "update-agenda-facts", AGENDA, "facts=replacement");
        check(!Files.isSymbolicLink(file), "atomic write replaces link"); eq(Files.readString(outside), "external", "target untouched");
        Files.delete(file); Files.createSymbolicLink(file, outside);
        ok(v, "update-agenda-facts", AGENDA, "facts="); eq(Files.readString(outside), "external", "delete leaves target");
        Files.createSymbolicLink(file, temp.resolve("missing"));
        check(!ok(v, "get-agenda-facts", AGENDA).containsKey("facts"), "broken link absent");
        ok(v, "update-agenda-facts", AGENDA, "facts="); check(!Files.isSymbolicLink(file), "broken link deleted");
        Files.delete(file.getParent()); Files.writeString(file.getParent(), "blocking parent");
        check(!ok(v, "get-agenda-facts", AGENDA).containsKey("facts"), "regular parent reads absent");
        error(v, 500, "폴더를 만들지 못했다: " + file.getParent(), "update-agenda-facts", AGENDA, "facts=x");
        error(v, 500, "파일을 지우지 못했다: " + file, "update-agenda-facts", AGENDA, "facts=");
        eq(Files.readString(file.getParent()), "blocking parent", "blocked parent preserved");
    }

    private static void briefing() throws Exception {
        Path v = vault();
        for (int i = 2; i <= 9; i++) note(v, "notes/serving/n" + i + "/note.md", "ndfixture-" + String.format("%08x", i), "serving", "2026-06-25", "reading");
        agenda(v, "bulk", "agfixture-0000000b"); agenda(v, "kotlin-ref", "agfixture-0000000c");
        for (int i = 1; i <= 101; i++) note(v, "notes/bulk/n" + i + "/note.md", "ndfixture-" + String.format("%08x", 100 + i), "bulk", "2026-07-01", "reading");
        Map<String, String> before = snapshot(v);
        EngineResult r = run(v, "facts-brief", "from=2026-06-16", "to=2026-06-25");
        eq(r.status(), 0, "manual brief success");
        eq(new String(r.stdoutBytes(), StandardCharsets.UTF_8), """
                {
                  "from": "2026-06-16",
                  "to": "2026-06-25",
                  "count": 1,
                  "section": "### serving — 노트 9 건, 사실층 파일이 아직 없다",
                  "items": [
                    {
                      "agenda": "serving",
                      "notes": 9
                    }
                  ]
                }
                """, "manual brief exact bytes");
        eq(brief(v, "min-new-file=10").get("section"), "", "threshold 10 empty");
        eq(snapshot(v), before, "brief changes no vault bytes");
        Path threshold = vault();
        for (int i = 2; i <= 4; i++) note(threshold, "notes/serving/n" + i + "/note.md", "ndfixture-" + String.format("%08x", i), "serving", "2026-06-16", "reading");
        eq(brief(threshold).get("count"), 0, "four notes below default threshold");
        note(threshold, "notes/serving/n5/note.md", "ndfixture-00000005", "serving", "2026-06-16", "reading");
        eq(items(brief(threshold)), List.of(Map.of("agenda", "serving", "notes", 5)), "five notes reach default threshold");
        ok(v, "update-agenda-facts", AGENDA, "facts=" + TEXT);
        Map<String, Object> full = brief(v, "to=2026-07-01");
        eq(full.keySet().stream().toList(), List.of("from", "to", "count", "section", "items"), "brief key order");
        eq(full.get("count"), 2, "manual full count");
        eq(full.get("section"), "### bulk — 노트 101 건, 사실층 파일이 아직 없다\n\n### serving — 노트 9 건\n\n- 색인은 vault 밖에 둔다.\n- 아젠다 slug는 안 바뀐다.", "manual full section");
        eq(items(full), List.of(Map.of("agenda", "bulk", "notes", 101), Map.of("agenda", "serving", "notes", 9, "facts", TEXT)), "manual items retain title");
        eq(items(full).get(1).keySet().stream().toList(), List.of("agenda", "notes", "facts"), "item key order");
        String json = new String(run(v, "facts-brief", "from=2026-06-16", "to=2026-07-01").stdoutBytes(), StandardCharsets.UTF_8);
        check(json.lines().anyMatch(line -> line.equals("  \"count\": 2,")), "count on own line for shell reader");
        eq(json.lines().filter(line -> line.startsWith("  \"section\": ")).count(), 1L, "section escaped onto one line");

        Path small = vault();
        note(small, "notes/serving/record/note.md", "ndfixture-00000002", "serving", "2026-06-16", "record");
        note(small, "notes/serving/root/sub/child/note.md", "snfixture-00000001", "serving", "2026-06-16", "reading");
        note(small, ".withlog/trash/note/note.md", "ndfixture-00000003", "serving", "2026-06-16", "reading");
        note(small, "notes/serving/orphan/note.md", "ndfixture-00000004", "missing", "2026-06-16", "reading");
        eq(brief(small).get("count"), 0, "one note without facts below default threshold");
        eq(items(brief(small, "min-new-file=1")), List.of(Map.of("agenda", "serving", "notes", 1)), "record/sub/trash/orphan excluded");
        write(small, PATH, "a".repeat(2001));
        eq(items(brief(small)).getFirst().get("facts"), FactsReader.OVER_LIMIT_MESSAGE, "oversize facts qualify with one note");
        agenda(small, "quiet", "agfixture-0000000d"); write(small, ".withlog/facts/quiet.md", "quiet facts");
        eq(brief(small).get("count"), 1, "inactive agenda excluded despite facts");
        for (String text : List.of("", "\u3000\u00a0")) {
            write(small, PATH, text); eq(brief(small).get("count"), 0, "blank facts use no-file threshold");
        }
        Files.write(small.resolve(PATH), new byte[]{(byte) 0xff});
        eq(brief(small).get("count"), 0, "unreadable facts use no-file threshold");
        String[][] bodies = {
                {"# title\r\n\u3000\r\n\u00a0\r\n- first\r\n  second  \r\n", "- first\n  second"},
                {"# title", ""}, {"## title\rbody", "## title\nbody"},
                {"#\ttitle\nbody", "#\ttitle\nbody"}, {"\ufeff# title\r\nbody", "\ufeff# title\nbody"},
                {"---\nkey: value\n---\nbody", "---\nkey: value\n---\nbody"},
                {"# title\n\n  indented\n\n# later title", "  indented\n\n# later title"}
        };
        for (String[] pair : bodies) {
            write(small, PATH, pair[0]); Map<String, Object> response = brief(small);
            eq(response.get("section"), "### serving — 노트 1 건\n\n" + pair[1], "brief body normalization");
            eq(items(response).getFirst().get("facts"), withlog.text.Trim.trim(pair[0]), "item retains raw line endings/title/BOM");
        }
        for (String slug : List.of("beta", "alpha", "😀", "\ue000")) {
            String suffix = String.format("%08x", (int) slug.charAt(0));
            agenda(small, slug, "agtie-" + suffix);
            note(small, "notes/" + slug + "/root/note.md", "ndtie-" + suffix, slug, "2026-06-16", "reading");
            write(small, ".withlog/facts/" + slug + ".md", "fact");
        }
        eq(items(brief(small)).stream().map(item -> item.get("agenda")).toList(), List.of("alpha", "beta", "serving", "😀", "\ue000"), "tie uses UTF-16 slug order");
    }

    private static Map<String, String> snapshot(Path v) throws Exception {
        Map<String, String> files = new TreeMap<>();
        try (var paths = Files.walk(v)) {
            for (Path p : paths.filter(Files::isRegularFile).toList()) files.put(v.relativize(p).toString(), Base64.getEncoder().encodeToString(Files.readAllBytes(p)));
        }
        return files;
    }

    private static void datesAndNumbers() throws Exception {
        Path v = vault();
        Map<String, Object> defaults = ok(v, "facts-brief", "from=\u3000", "to=", "min-new-file=\u00a0");
        eq(defaults.get("from"), "2026-06-19", "default seven-day start"); eq(defaults.get("to"), "2026-06-25", "today process zone");
        System.setProperty("withlog.test.now", Long.toString(Instant.parse("2026-06-24T16:00:00Z").toEpochMilli()));
        eq(ok(v, "facts-brief").get("to"), "2026-06-25", "KST date");
        TimeZone.setDefault(TimeZone.getTimeZone("UTC"));
        eq(ok(v, "facts-brief").get("to"), "2026-06-24", "engine zone affects today");
        TimeZone.setDefault(TimeZone.getTimeZone("Asia/Seoul"));
        System.setProperty("withlog.test.now", Long.toString(Instant.parse("2026-06-25T01:00:00Z").toEpochMilli()));
        eq(ok(v, "facts-brief", "to=2026-06-22").get("from"), "2026-06-16", "explicit to minus six");
        Map<String, Object> zero = ok(v, "facts-brief", "to=0000-01-03");
        eq(zero, Map.of("from", "-0001-12-28", "to", "0000-01-03", "count", 0, "section", "", "items", List.of()), "year-zero example");
        for (String day : List.of("0000-02-29", "-0001-01-07", "+10000-01-07", "+00010000-01-07", "+999999999-01-07"))
            eq(ok(v, "facts-brief", "to=" + day).get("count"), 0, "valid extended calendar date " + day);
        for (String day : List.of("2026-9-7", "2026-02-30", "10000-01-01", "-0000-01-01", "+2026-01-01", "２０２６-０６-１６", "2026-06-16T00:00:00", "+1000000000-01-01"))
            error(v, 400, "from 은 YYYY-MM-DD 모양이어야 한다: " + day, "facts-brief", "from= " + day + " ", "to=bad", "min-new-file=bad");
        error(v, 400, "to 은 YYYY-MM-DD 모양이어야 한다: bad", "facts-brief", "to=bad", "min-new-file=bad");
        error(v, 400, "날짜 구간이 뒤집혔다: 2026-07-01 ~ 2026-06-16", "facts-brief", "from=2026-07-01", "to=2026-06-16");
        error(v, 400, "날짜 구간이 뒤집혔다: 2026-06-26 ~ 2026-06-25", "facts-brief", "from=2026-06-26");
        for (String num : List.of("1", "+1", "００１", "١", "2147483647"))
            eq(brief(v, "min-new-file=" + num).get("count"), num.equals("2147483647") ? 0 : 1, "integer grammar " + num);
        for (String num : List.of("0", "-0", "０", "-０００"))
            error(v, 400, "min-new-file 은 1 이상이어야 한다: 0", "facts-brief", "min-new-file=" + num);
        for (String num : List.of("-007", "-７", "-2147483648"))
            error(v, 400, "min-new-file 은 1 이상이어야 한다: " + Integer.parseInt(num), "facts-brief", "min-new-file=" + num);
        for (String num : List.of("+", "-", "1.0", "2147483648", "-2147483649", "𝟎", "−1", "1 2"))
            error(v, 400, "min-new-file 은 숫자여야 한다: " + num, "facts-brief", "min-new-file=\u3000" + num + "\u00a0");
        eq(items(brief(v, "from=2026-06-16", "to=2026-06-16", "min-new-file=1")).getFirst().get("notes"), 1, "date is note's own offset date");
        eq(brief(v, "from=2026-06-15", "to=2026-06-15", "min-new-file=1").get("count"), 0, "outside window excluded");
    }

    private static void readingBoundaries() throws Exception {
        Path v = vault(); Path file = v.resolve(PATH); Files.createDirectories(file.getParent());
        for (int size : new int[]{2000, 2001, 10000, 10001}) {
            String text = "a".repeat(size); Files.writeString(file, text);
            eq(FactsReader.readFacts(v, "serving"), size > 2000 ? FactsReader.OVER_LIMIT_MESSAGE : text, "AI limit " + size);
            Map<String, Object> edit = ok(v, "get-agenda-facts", AGENDA);
            eq(edit.get("facts"), size <= 10000 ? text : null, "edit limit " + size);
            eq(edit.get("tooLargeToEdit"), size > 10000 ? true : null, "tooLarge flag " + size);
        }
        for (String text : List.of("", " \n\u00a0\u3000", "\ufeff# title\r\nbody", "---\r\nkey: value\r\n---\r\nbody", "\ufeff" + "a".repeat(2000))) {
            Files.writeString(file, text);
            String expected = withlog.text.Trim.trim(text);
            eq(ok(v, "get-agenda-facts", AGENDA).get("facts"), expected.isEmpty() ? null : expected, "edit preserves BOM/frontmatter/CRLF");
            String ai = expected.isEmpty() ? null : expected.length() > 2000 ? FactsReader.OVER_LIMIT_MESSAGE : expected;
            eq(ok(v, "get_doc", "{\"id\":\"" + NOTE + "\"}").get("agendaFacts"), ai, "get_doc shares AI reader");
            eq(ok(v, "get-doc", NOTE).get("agendaFacts"), null, "command get-doc omits facts");
        }
        Files.writeString(file, "a".repeat(2000) + "\n" + "b".repeat(500));
        eq(FactsReader.readFacts(v, "serving"), "a".repeat(2000), "frozen trim-after-cut at 2001");
        Files.writeString(file, "a".repeat(10000) + "\u3000tail");
        eq(ok(v, "get-agenda-facts", AGENDA).get("facts"), "a".repeat(10000), "frozen trim-after-cut at 10001");
        Files.writeString(file, " ".repeat(10) + "a".repeat(10000));
        eq(ok(v, "get-agenda-facts", AGENDA).get("facts"), "a".repeat(9991), "leading whitespace loses unseen tail");
        for (int limit : new int[]{2000, 10000}) {
            Files.writeString(file, " " + "a".repeat(limit - 1) + "😀tail");
            String direct = limit == 2000 ? FactsReader.readFacts(v, "serving") : FactsReader.readForEdit(v, "serving").facts();
            eq(direct.length(), limit, "split pair length");
            check(Character.isHighSurrogate(direct.charAt(limit - 1)), "split high surrogate retained");
            Map<String, Object> response = limit == 2000 ? ok(v, "get_doc", "{\"id\":\"" + NOTE + "\"}") : ok(v, "get-agenda-facts", AGENDA);
            eq(response.get(limit == 2000 ? "agendaFacts" : "facts"), "a".repeat(limit - 1) + "?", "stdout replaces unpaired unit");
        }
        raw(file, "a".repeat(2001), 0xff);
        eq(FactsReader.readFacts(v, "serving"), null, "invalid byte after capture is decoded ahead");
        eq(ok(v, "get-agenda-facts", AGENDA).get("facts"), null, "malformed file absent for editor");
        for (int[] suffix : new int[][]{{0xc2}, {0xe3, 0x81}, {0xf0, 0x9f, 0x98}}) {
            raw(file, "a".repeat(2001), suffix);
            eq(FactsReader.readFacts(v, "serving"), FactsReader.OVER_LIMIT_MESSAGE, "truncated EOF ignored after capture");
            eq(FactsReader.readForEdit(v, "serving").facts(), null, "editor confirms truncated EOF");
            raw(file, "a".repeat(2000), suffix);
            eq(FactsReader.readFacts(v, "serving"), null, "short capture confirms truncated EOF");
        }
        raw(file, "a".repeat(10001), 0xc2);
        check(FactsReader.readForEdit(v, "serving").tooLarge(), "editor ignores truncated EOF after 10001");
        raw(file, "a".repeat(10000), 0xc2);
        eq(FactsReader.readForEdit(v, "serving").facts(), null, "editor checks truncated EOF before 10001");
        raw(file, "a".repeat(8191), 0xff);
        eq(FactsReader.readFacts(v, "serving"), null, "bad byte inside first byte buffer");
        raw(file, "a".repeat(8192), 0xff);
        eq(FactsReader.readFacts(v, "serving"), FactsReader.OVER_LIMIT_MESSAGE, "bad byte outside first buffer ignored");
        raw(file, "가" + "a".repeat(8191), 0xff);
        eq(FactsReader.readFacts(v, "serving"), null, "multibyte prefix moves bad head into buffered bytes");
        String prefix = "가" + "a".repeat(8191);
        for (int[] suffix : new int[][]{{0x80}, {0xc1}, {0xf5}, {0xe0, 0x80, 0x80}, {0xf0, 0x80, 0x80, 0x80}, {0xf0, 0x90, 0xff, 0x80}}) {
            raw(file, prefix, suffix);
            eq(FactsReader.readFacts(v, "serving"), null, "overflow checks malformed next head " + Arrays.toString(suffix));
        }
        for (int[] suffix : new int[][]{{0xc2, 0xff}, {0xe1, 0x80, 0xff}, {0xf0, 0x90, 0x80, 0xff}}) {
            raw(file, prefix, suffix);
            eq(FactsReader.readFacts(v, "serving"), FactsReader.OVER_LIMIT_MESSAGE, "overflow ignores later bytes " + Arrays.toString(suffix));
        }
        raw(file, "a".repeat(10001), 0xff);
        eq(FactsReader.readForEdit(v, "serving").facts(), null, "edit next head inspected at 10001");
        raw(file, "a".repeat(10000) + "😀" + "a".repeat(100), 0xff);
        eq(FactsReader.readForEdit(v, "serving").facts(), null, "split pair requires second buffered decode");
        raw(file, "a".repeat(20000), 0xff);
        check(FactsReader.readForEdit(v, "serving").tooLarge(), "bad byte beyond edit decode ignored");
        eq(FactsReader.readFacts(v, "serving"), FactsReader.OVER_LIMIT_MESSAGE, "bad byte beyond AI decode ignored");
    }

    private static void raw(Path file, String prefix, int... suffix) throws Exception {
        byte[] bytes = prefix.getBytes(StandardCharsets.UTF_8);
        byte[] all = Arrays.copyOf(bytes, bytes.length + suffix.length);
        for (int i = 0; i < suffix.length; i++) all[bytes.length + i] = (byte) suffix[i];
        Files.write(file, all);
    }

    private static void serveAndCli() throws Exception {
        Path v = vault(), home = Files.createTempDirectory(temp, "home-");
        Path socket = Path.of("/tmp", "wl-facts-" + UUID.randomUUID().toString().substring(0, 8) + ".sock");
        Path engine = Path.of("dist/wl-engine.jar").toAbsolutePath(), cli = Path.of("dist/wl-cli.jar").toAbsolutePath();
        String java = Path.of(System.getProperty("java.home"), "bin/java").toString();
        List<String> prefix = List.of(java, "-Duser.home=" + home, "-Duser.timezone=Asia/Seoul", "-jar");
        Map<String, String> environment = Map.of("HOME", home.toString(), "TZ", "Asia/Seoul", "WITHLOG_VAULT", v.toString(), "WITHLOG_SIDECAR_SOCKET", socket.toString());
        List<String> command = new ArrayList<>(prefix); command.add(engine.toString()); command.add("serve");
        ProcessBuilder builder = new ProcessBuilder(command);
        builder.environment().clear(); builder.environment().putAll(environment);
        builder.redirectOutput(home.resolve("stdout.log").toFile()); builder.redirectError(home.resolve("stderr.log").toFile());
        Process server = builder.start();
        try {
            long until = System.nanoTime() + TimeUnit.SECONDS.toNanos(10);
            while (!Files.exists(socket) && server.isAlive() && System.nanoTime() < until) Thread.sleep(25);
            check(Files.exists(socket), "serve ready: " + Files.readString(home.resolve("stderr.log")));
            for (String[] args : List.of(
                    new String[]{"update-agenda-facts", AGENDA, "facts=" + TEXT},
                    new String[]{"get-agenda-facts", AGENDA},
                    new String[]{"facts-brief", "from=2026-06-16", "to=2026-06-25"},
                    new String[]{"get_doc", "{\"id\":\"" + NOTE + "\"}"},
                    new String[]{"update-agenda-facts", AGENDA, "facts="},
                    new String[]{"get-agenda-facts", AGENDA})) {
                command = new ArrayList<>(prefix); command.add(cli.toString()); command.addAll(List.of(args));
                Map<String, Object> response = object(child(command, environment, 0));
                if (args[0].equals("facts-brief")) eq(response.get("count"), 1, "socket facts qualify below new-file threshold");
                else if (args[0].equals("get_doc")) eq(response.get("agendaFacts"), TEXT, "socket AI reads facts");
                else eq(response.get("id"), AGENDA, "socket agenda id");
            }
            check(!Files.exists(v.resolve(PATH)), "socket deletion persisted");
            command = new ArrayList<>(prefix); command.add(cli.toString()); command.addAll(List.of("facts-brief", "min-new-file=0"));
            child(command, environment, 3);
        } finally {
            server.destroy(); if (!server.waitFor(5, TimeUnit.SECONDS)) server.destroyForcibly().waitFor();
            Files.deleteIfExists(socket);
        }
        command = new ArrayList<>(prefix); command.add(engine.toString()); command.addAll(List.of("get-agenda-facts", AGENDA));
        child(command, Map.of("HOME", home.toString()), 2); // vault 안내가 명령 실행보다 먼저다.
        command = new ArrayList<>(prefix); command.add(engine.toString()); command.addAll(List.of("get-agenda-facts", AGENDA, "--withlog.vault=" + v));
        eq(new String(child(command, Map.of("HOME", home.toString()), 0), StandardCharsets.UTF_8), ABSENT, "one-shot explicit vault exact bytes");
    }

    private static byte[] child(List<String> command, Map<String, String> env, int status) throws Exception {
        ProcessBuilder builder = new ProcessBuilder(command); builder.environment().clear(); builder.environment().putAll(env);
        Process child = builder.start();
        try {
            check(child.waitFor(10, TimeUnit.SECONDS), "child completes");
            byte[] out = child.getInputStream().readAllBytes(), err = child.getErrorStream().readAllBytes();
            eq(child.exitValue(), status, "process exit: " + new String(err, StandardCharsets.UTF_8));
            if (status == 0) eq(err.length, 0, "process success stderr"); else eq(out.length, 0, "process failure stdout");
            return out;
        } finally { if (child.isAlive()) child.destroyForcibly().waitFor(); }
    }
}
