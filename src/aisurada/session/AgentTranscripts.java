package aisurada.session;

import aisurada.args.JsonReader;
import aisurada.args.JsonValue;
import aisurada.text.Trim;

import java.io.BufferedReader;
import java.io.IOException;
import java.nio.file.DirectoryStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.stream.Stream;

/**
 * Claude가 아닌 에이전트(Codex, Gemini 계열)의 세션 기록을 찾고 {@code read-session}이 내는 모양으로 바꿉니다.
 *
 * <p>Claude의 기록은 {@link SessionTranscript}가 읽습니다. 이 클래스는 세션 id로 Claude 기록을 못 찾았을 때만
 * 불립니다. 기록 형식은 {@link CodexTranscript}, {@link GeminiTranscript}(Gemini CLI), {@link AntigravityTranscript}
 * (Antigravity {@code agy})가 읽어서 같은 {@link Turn} 목록으로 만들고, 쪽 나누기와 주소 모으기는 여기서 한 번에 합니다.
 * Gemini CLI와 Antigravity는 노트에 같은 에이전트 이름({@code gemini})으로 적히므로, 둘 다 {@link SessionAgents#GEMINI}로 돌려줍니다.
 */
public final class AgentTranscripts {

    private static final int MAX_TEXT_CHARS = 2000;
    private static final int MAX_TITLE_CHARS = 60;

    private AgentTranscripts() {}

    /** 찾은 기록. [agent]는 codex 또는 gemini입니다. */
    public record Located(String agent, Path file) {}

    /** 한 회차. who는 user, assistant, tool 가운데 하나입니다. */
    static final class Turn {
        int index;
        String who;
        String at;
        String text = "";
        String toolName;
        String toolArg;
        Double toolSeconds;
        boolean toolFailed;
    }

    /** 파서가 한 기록 파일에서 읽어 낸 것. */
    record Parsed(List<Turn> turns, int lines, int skipped, String cwd, String model) {}

    /** {@code session-titles}가 내는 정보. */
    public record Info(String agent, String sessionId, boolean found, String title, String cwd) {}

    // ---- 위치 ----

    /** Codex 기록 폴더(~/.codex/sessions). 시스템 속성 aisurada.codex.home으로 홈을 바꿀 수 있습니다. */
    public static Path codexSessionsDir() {
        Path home = homeFor("aisurada.codex.home");
        return home == null ? null : home.resolve(".codex").resolve("sessions");
    }

    /** Gemini 기록 폴더(~/.gemini/tmp). 시스템 속성 aisurada.gemini.home으로 홈을 바꿀 수 있습니다. */
    public static Path geminiTmpDir() {
        Path home = homeFor("aisurada.gemini.home");
        return home == null ? null : home.resolve(".gemini").resolve("tmp");
    }

    /** Antigravity 대화 기록 폴더. 지금 쓰는 antigravity-cli를 먼저, 예전 antigravity를 그다음에 본다. 시스템 속성 aisurada.gemini.home으로 홈을 바꿀 수 있습니다. */
    public static List<Path> antigravityDirs() {
        Path home = homeFor("aisurada.gemini.home");
        if (home == null) return List.of();
        Path gemini = home.resolve(".gemini");
        return List.of(gemini.resolve("antigravity-cli").resolve("conversations"), gemini.resolve("antigravity").resolve("conversations"));
    }

    private static Path homeFor(String property) {
        String custom = System.getProperty(property);
        if (custom != null && !custom.trim().isEmpty()) return Path.of(custom.trim());
        String home = System.getProperty("user.home");
        return home == null || home.trim().isEmpty() ? null : Path.of(home);
    }

    /** 세션 id로 Codex나 Gemini 기록을 찾습니다. 없으면 null입니다. */
    public static Located locate(String sessionId) {
        Path codex = findCodex(codexSessionsDir(), sessionId);
        if (codex != null) return new Located(SessionAgents.CODEX, codex);
        Path gemini = findGemini(geminiTmpDir(), sessionId);
        if (gemini != null) return new Located(SessionAgents.GEMINI, gemini);
        Path antigravity = AntigravityTranscript.find(antigravityDirs(), sessionId);
        if (antigravity != null) return new Located(SessionAgents.GEMINI, antigravity);
        return null;
    }

    /** rollout-<시각>-<id>.jsonl. 연/월/일 폴더 아래에 있어서 깊이 4까지 본다. */
    static Path findCodex(Path root, String sessionId) {
        if (root == null || !Files.isDirectory(root)) return null;
        String suffix = "-" + sessionId.toLowerCase(Locale.ROOT) + ".jsonl";
        try (Stream<Path> walk = Files.walk(root, 4)) {
            return walk.filter(p -> {
                String name = p.getFileName().toString();
                return name.startsWith("rollout-") && name.toLowerCase(Locale.ROOT).endsWith(suffix) && Files.isRegularFile(p);
            }).findFirst().orElse(null);
        } catch (IOException | RuntimeException e) {
            return null;
        }
    }

    /** session-<시각>-<id 앞 8자>.jsonl. 이름만 믿지 않고 첫 줄의 sessionId를 맞대어 본다. */
    static Path findGemini(Path root, String sessionId) {
        if (root == null || !Files.isDirectory(root) || sessionId.length() < 8) return null;
        String tail = "-" + sessionId.substring(0, 8).toLowerCase(Locale.ROOT) + ".jsonl";
        try (DirectoryStream<Path> projects = Files.newDirectoryStream(root)) {
            for (Path project : projects) {
                Path chats = project.resolve("chats");
                if (!Files.isDirectory(chats)) continue;
                try (DirectoryStream<Path> files = Files.newDirectoryStream(chats, "session-*.jsonl")) {
                    for (Path file : files) {
                        if (!file.getFileName().toString().toLowerCase(Locale.ROOT).endsWith(tail)) continue;
                        if (sessionId.equalsIgnoreCase(GeminiTranscript.headerSessionId(file))) return file;
                    }
                }
            }
        } catch (IOException | RuntimeException ignored) {
            // 읽을 수 없는 폴더는 없는 것으로 본다
        }
        return null;
    }

    // ---- 읽기 ----

    private static Parsed parse(Located located) throws IOException {
        if (SessionAgents.CODEX.equals(located.agent())) return CodexTranscript.parse(located.file());
        if (AntigravityTranscript.isAntigravityFile(located.file())) return AntigravityTranscript.parse(located.file());
        return GeminiTranscript.parse(located.file(), geminiTmpDir());
    }

    /** session-titles 한 건. 읽을 수 없으면 found=false입니다. */
    public static Info info(Located located, String sessionId) {
        try {
            Parsed parsed = parse(located);
            String title = null;
            for (Turn turn : parsed.turns()) {
                if ("user".equals(turn.who)) {
                    title = firstLine(turn.text);
                    break;
                }
            }
            return new Info(located.agent(), sessionId, true, title, parsed.cwd());
        } catch (IOException | RuntimeException e) {
            return new Info(located.agent(), sessionId, false, null, null);
        }
    }

    /**
     * read-session 한 건. 쪽 나누기는 Claude와 같다: 끝 자리(before)에서 거꾸로 도구가 아닌 회차를 limit개 센다.
     * 노트 id가 들어 있는 회차 자리는 [noteId]가 있을 때만 모은다. author는 지원하지 않는다.
     */
    public static SessionTranscript.ReadSessionResult read(Located located, String sessionId, int limit, Integer before,
                                                           boolean includeUrls, String noteId) {
        Parsed parsed;
        long bytes;
        try {
            parsed = parse(located);
            bytes = Files.size(located.file());
        } catch (IOException | RuntimeException e) {
            throw new RuntimeException("사이드카 안에서 실패했다: " + e.getMessage() + ". 자세한 것은 사이드카 로그에 있다.", e);
        }
        List<Turn> turns = parsed.turns();
        int total = turns.size();
        int effectiveLimit = Math.max(1, Math.min(500, limit));
        int end = before == null ? total : Math.max(0, Math.min(total, before));

        int start = end;
        int nonTool = 0;
        while (start > 0 && nonTool < effectiveLimit) {
            start--;
            if (!"tool".equals(turns.get(start).who)) nonTool++;
        }
        List<Turn> page = new ArrayList<>(turns.subList(start, end));
        boolean hasMore = !page.isEmpty() && page.get(0).index > 0;

        List<Map<String, Object>> urls = new ArrayList<>();
        if (includeUrls) {
            Map<String, Integer> counts = new LinkedHashMap<>();
            for (Turn turn : turns) SessionTranscript.extractUrls(turn.text, counts);
            counts.entrySet().stream()
                    .sorted((a, b) -> Integer.compare(b.getValue(), a.getValue()))
                    .forEach(e -> {
                        Map<String, Object> row = new LinkedHashMap<>();
                        row.put("url", e.getKey());
                        row.put("count", e.getValue());
                        urls.add(row);
                    });
        }

        Map<String, Object> runState = null;
        if (parsed.model() != null) {
            runState = new LinkedHashMap<>();
            runState.put("model", parsed.model());
            runState.put("quotaBlocked", false);
        }

        List<Integer> mentions = null;
        if (noteId != null) {
            mentions = new ArrayList<>();
            for (Turn turn : turns) {
                if (!"tool".equals(turn.who) && turn.text.contains(noteId)) mentions.add(turn.index);
            }
        }

        return new SessionTranscript.ReadSessionResult(
                sessionId, located.file().toAbsolutePath().toString(), bytes,
                parsed.lines(), parsed.skipped(), total, hasMore,
                urls, Collections.emptyList(), Collections.emptyList(), runState, mentions, null, toMaps(page));
    }

    private static List<Map<String, Object>> toMaps(List<Turn> turns) {
        List<Map<String, Object>> list = new ArrayList<>(turns.size());
        for (Turn t : turns) {
            Map<String, Object> map = new LinkedHashMap<>();
            map.put("index", t.index);
            map.put("who", t.who);
            map.put("at", t.at);
            map.put("chars", t.text.length());
            map.put("truncated", t.text.length() > MAX_TEXT_CHARS);
            if ("tool".equals(t.who)) {
                Map<String, Object> tool = new LinkedHashMap<>();
                tool.put("name", t.toolName);
                if (t.toolArg != null) tool.put("arg", t.toolArg);
                if (t.toolSeconds != null) tool.put("seconds", t.toolSeconds);
                if (t.toolFailed) tool.put("failed", true);
                map.put("tool", tool);
            }
            map.put("text", t.text.length() > MAX_TEXT_CHARS ? t.text.substring(0, MAX_TEXT_CHARS) : t.text);
            list.add(map);
        }
        return list;
    }

    // ---- 파서가 같이 쓰는 도우미 ----

    /** 줄이 아니라 회차 목록에서 공백뿐인 글을 버리고 순번을 붙여 더한다(7.4). */
    static void addText(List<Turn> turns, String who, String at, String text) {
        if (text == null || Trim.isBlank(text)) return;
        Turn turn = new Turn();
        turn.index = turns.size();
        turn.who = who;
        turn.at = at;
        turn.text = text;
        turns.add(turn);
    }

    static void addTool(List<Turn> turns, String at, String name, String arg, Double seconds, boolean failed) {
        Turn turn = new Turn();
        turn.index = turns.size();
        turn.who = "tool";
        turn.at = at;
        turn.toolName = name;
        turn.toolArg = arg == null || arg.isBlank() ? null : cut(arg, 160);
        turn.toolSeconds = seconds;
        turn.toolFailed = failed;
        turns.add(turn);
    }

    static String cut(String text, int max) {
        String oneLine = text.replace('\n', ' ').replace('\r', ' ').trim();
        return oneLine.length() <= max ? oneLine : oneLine.substring(0, max - 1) + "…";
    }

    private static String firstLine(String text) {
        for (String line : text.split("\n")) {
            String trimmed = Trim.trim(line);
            if (!trimmed.isEmpty()) return cut(trimmed, MAX_TITLE_CHARS);
        }
        return null;
    }

    /** 한 줄을 JSON 객체로 읽습니다. 객체가 아니거나 못 읽으면 null입니다. */
    static Map<String, JsonValue> readObject(String line) {
        try {
            return JsonReader.parse(line) instanceof JsonValue.JsonObject object ? object.properties() : null;
        } catch (RuntimeException e) {
            return null;
        }
    }

    static String str(JsonValue value) {
        return value instanceof JsonValue.JsonString s ? s.value() : null;
    }

    static String str(Map<String, JsonValue> object, String key) {
        return object == null ? null : str(object.get(key));
    }

    static Map<String, JsonValue> obj(JsonValue value) {
        return value instanceof JsonValue.JsonObject o ? o.properties() : null;
    }

    static Map<String, JsonValue> obj(Map<String, JsonValue> object, String key) {
        return object == null ? null : obj(object.get(key));
    }

    static List<JsonValue> arr(JsonValue value) {
        return value instanceof JsonValue.JsonArray a ? a.elements() : null;
    }

    static Double number(JsonValue value) {
        if (value instanceof JsonValue.JsonLiteral lit) {
            try {
                return Double.parseDouble(lit.raw());
            } catch (NumberFormatException ignored) {
                return null;
            }
        }
        return null;
    }

    /** 줄 단위로 읽는 리더. UTF-8이 깨져도 대체 문자로 읽는다(D18). */
    static BufferedReader reader(Path file) throws IOException {
        return SessionTranscript.newFlexibleReader(file);
    }
}
