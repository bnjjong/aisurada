package aisurada.session;

import aisurada.args.JsonReader;
import aisurada.args.JsonValue;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.net.URI;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Instant;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.concurrent.TimeUnit;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

import static aisurada.session.AgentTranscripts.addText;
import static aisurada.session.AgentTranscripts.addTool;

/**
 * Antigravity(agy) 대화 기록 읽기. 파일은 {@code ~/.gemini/antigravity-cli/conversations/<대화 id>.db} 하나이고,
 * 안에 든 SQLite의 {@code steps} 표에 단계가 한 줄씩 있습니다. 단계의 내용은 protobuf라서 {@link Protobuf}로 필요한 필드만 읽습니다.
 *
 * <p>읽는 단계는 셋뿐입니다. 사용자 입력(step_type 14), 모델 답(15), 도구 호출(132). 시스템 알림(101), 이어쓰기 요약(23),
 * API 오류 알림(17)은 대화 내용이 아니라서 건너뜁니다. 오래된 단계는 CLI가 내용을 비워 둔 채 남기므로(status 5) 글이 없는 단계는 회차로 세지 않습니다.
 *
 * <p>엔진에는 SQLite 라이브러리가 없어서 시스템의 {@code sqlite3}(macOS는 {@code /usr/bin/sqlite3})를 읽기 전용으로 부릅니다.
 * 큰 도구 결과까지 가져오지 않도록, 단계마다 필요한 앞부분만 SQL로 잘라서 받습니다.
 */
final class AntigravityTranscript {

    /** 대화 id는 UUID다. 파일 이름으로 쓰므로 이 모양만 받는다. */
    private static final Pattern CONVERSATION_ID = Pattern.compile("^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$");

    /** 도구 호출 머리(시각, 호출 id, 이름, 인자 앞부분)를 가져올 바이트 수. 인자 전체는 수십 KB일 수 있다. */
    private static final int META_PREFIX_BYTES = 1200;
    /** MCP 호출의 서버·도구 이름은 인자 JSON 맨 끝에 있어서, 그 자리를 찾아 이만큼만 따로 가져온다. */
    private static final int MCP_TAIL_BYTES = 160;
    /** 본문이 긴 호출은 인자 앞부분에 id가 안 들어오므로, {@code "id":"} 자리를 찾아 이만큼만 따로 가져온다. */
    private static final int ID_TAIL_BYTES = 60;
    private static final long SQLITE_TIMEOUT_SECONDS = 60;

    private static final String STEPS_SQL =
            "select idx, step_type, status, ifnull(length(error_details),0), hex(substr(metadata,1," + META_PREFIX_BYTES + ")),"
                    + " case when step_type=132 and instr(metadata, cast('\"ServerName\"' as blob))>0"
                    + " then hex(substr(metadata, instr(metadata, cast('\"ServerName\"' as blob)), " + MCP_TAIL_BYTES + ")) else '' end,"
                    + " case when step_type=132 and instr(metadata, cast('\"id\":\"' as blob))>0"
                    + " then hex(substr(metadata, instr(metadata, cast('\"id\":\"' as blob)), " + ID_TAIL_BYTES + ")) else '' end,"
                    + " case when step_type in (14,15) then hex(step_payload) else '' end"
                    + " from steps order by idx";

    private static final String META_SQL =
            "select 'T', hex(substr(data,1,1500)) from trajectory_metadata_blob limit 1;"
                    + " select 'E', hex(data) from executor_metadata order by idx desc limit 1;"
                    + " select 'G', hex(data) from gen_metadata order by idx desc limit 1;";

    private AntigravityTranscript() {}

    /** 대화 id로 기록 파일을 찾는다. 폴더 두 곳(antigravity-cli, antigravity)을 차례로 본다. 없거나 id 모양이 틀리면 null. */
    static Path find(List<Path> directories, String sessionId) {
        if (sessionId == null || !CONVERSATION_ID.matcher(sessionId).matches()) return null;
        String fileName = sessionId.toLowerCase(Locale.ROOT) + ".db";
        for (Path directory : directories) {
            if (directory == null) continue;
            Path file = directory.resolve(fileName);
            if (Files.isRegularFile(file)) return file;
        }
        return null;
    }

    static boolean isAntigravityFile(Path file) {
        return file.getFileName().toString().endsWith(".db");
    }

    static AgentTranscripts.Parsed parse(Path db) throws IOException {
        List<AgentTranscripts.Turn> turns = new ArrayList<>();
        int lines = 0;
        int skipped = 0;
        for (String[] row : query(db, STEPS_SQL, 8)) {
            lines++;
            try {
                step(row, turns);
            } catch (RuntimeException e) {
                skipped++;
            }
        }
        for (int i = 0; i < turns.size(); i++) turns.get(i).index = i;
        String cwd = null;
        String model = null;
        String generatedModel = null;
        for (String[] row : query(db, META_SQL, 2)) {
            byte[] data = unhex(row[1]);
            if ("T".equals(row[0])) {
                cwd = workingDirectory(Protobuf.parse(data, true));
            } else if ("E".equals(row[0])) {
                model = executorModel(Protobuf.parse(data, true));
            } else if ("G".equals(row[0])) {
                generatedModel = generatedModel(Protobuf.parse(data, true));
            }
        }
        return new AgentTranscripts.Parsed(turns, lines, skipped, cwd, model != null ? model : generatedModel);
    }

    // ---- 단계 하나 ----

    private static void step(String[] row, List<AgentTranscripts.Turn> turns) {
        int type = Integer.parseInt(row[1]);
        if (type != 14 && type != 15 && type != 132) return;
        List<Protobuf.Field> meta = Protobuf.parse(unhex(row[4]), true);
        String at = timestamp(Protobuf.message(meta, 1, true));
        if (type == 132) {
            tool(row, meta, at, turns);
            return;
        }
        List<Protobuf.Field> payload = Protobuf.parse(unhex(row[7]), false);
        if (type == 14) {
            // 사용자 입력: 19번 메시지의 2번이 글이다
            addText(turns, "user", at, Protobuf.text(Protobuf.message(payload, 19, false), 2));
        } else {
            // 모델 답: 20번 메시지의 1번이 사용자에게 보이는 글, 3번은 생각이라 뺀다
            addText(turns, "assistant", at, Protobuf.text(Protobuf.message(payload, 20, false), 1));
        }
    }

    private static void tool(String[] row, List<Protobuf.Field> meta, String at, List<AgentTranscripts.Turn> turns) {
        List<Protobuf.Field> call = Protobuf.message(meta, 4, true);
        String name = Protobuf.text(call, 2);
        if (name == null) return;
        String args = Protobuf.text(call, 3);
        boolean failed = "7".equals(row[2]) || !"0".equals(row[3]);
        if ("call_mcp_tool".equals(name)) {
            String tail = new String(unhex(row[5]), StandardCharsets.UTF_8);
            String server = jsonString(tail, "ServerName");
            String tool = jsonString(tail, "ToolName");
            name = (server == null ? "" : server + "/") + (tool == null ? "mcp" : tool);
        }
        String idTail = new String(unhex(row[6]), StandardCharsets.UTF_8);
        addTool(turns, at, name, summarize(args == null ? idTail : args + " " + idTail), null, failed);
    }

    /** 인자 가운데 사람이 읽을 만한 것 하나. Gemini CLI 쪽 요약과 같은 생각이고, 키 이름이 대문자로 시작하는 것을 함께 본다. */
    private static String summarize(String args) {
        if (args == null) return null;
        for (String key : new String[] {"description", "file_path", "path", "absolutepath", "commandline", "command", "query", "url", "id", "title"}) {
            String value = jsonString(args, key);
            if (value != null && !value.isBlank()) return value;
        }
        return null;
    }

    /** 잘린 JSON에서도 {@code "키":"값"}을 정규식으로 꺼낸다. 키는 대소문자를 가리지 않는다. */
    private static String jsonString(String json, String key) {
        Matcher m = Pattern.compile("\"" + Pattern.quote(key) + "\"\\s*:\\s*\"((?:[^\"\\\\]|\\\\.)*)\"", Pattern.CASE_INSENSITIVE).matcher(json);
        if (!m.find()) return null;
        try {
            return JsonReader.parse("\"" + m.group(1) + "\"") instanceof JsonValue.JsonString s ? s.value() : m.group(1);
        } catch (RuntimeException e) {
            return m.group(1);
        }
    }

    private static String timestamp(List<Protobuf.Field> time) {
        Long seconds = Protobuf.number(time, 1);
        if (seconds == null) return null;
        Long nanos = Protobuf.number(time, 2);
        return Instant.ofEpochSecond(seconds, nanos == null ? 0 : nanos).toString();
    }

    // ---- 대화 전체 정보 ----

    /** trajectory_metadata_blob의 7번(작업 폴더 file: 주소), 없으면 1번 안의 1번. */
    private static String workingDirectory(List<Protobuf.Field> metadata) {
        String url = Protobuf.text(metadata, 7);
        if (url == null) url = Protobuf.text(Protobuf.message(metadata, 1, true), 1);
        if (url == null || !url.startsWith("file:")) return null;
        try {
            return URI.create(url).getPath();
        } catch (IllegalArgumentException e) {
            return null;
        }
    }

    /** executor_metadata의 10번 > 1번 > 28번(모델 id, 예: gemini-3.8-flash-high). */
    private static String executorModel(List<Protobuf.Field> executor) {
        List<Protobuf.Field> config = Protobuf.message(Protobuf.message(executor, 10, true), 1, true);
        return Protobuf.text(config, 28);
    }

    /** gen_metadata의 1번 > 21번(화면에 쓰는 모델 이름). executor에서 못 찾았을 때만 쓴다. */
    private static String generatedModel(List<Protobuf.Field> generated) {
        return Protobuf.text(Protobuf.message(generated, 1, true), 21);
    }

    // ---- sqlite3 ----

    /**
     * 읽기 전용으로 질의하고 줄마다 [columns]칸으로 나눠 돌려준다. 칸은 16진 글과 숫자뿐이라 '|'로 나눠도 안전하다.
     * 평소 열기가 안 되면(쓰는 쪽이 잠갔거나 읽기 전용 폴더) 같은 파일을 immutable로 한 번 더 시도한다.
     */
    private static List<String[]> query(Path db, String sql, int columns) throws IOException {
        String uri = "file:" + db.toAbsolutePath().toUri().getRawPath();
        IOException first = null;
        for (String options : new String[] {"?mode=ro", "?mode=ro&immutable=1"}) {
            try {
                return run(uri + options, sql, columns);
            } catch (IOException e) {
                if (first == null) first = e;
            }
        }
        throw first;
    }

    private static List<String[]> run(String uri, String sql, int columns) throws IOException {
        Path sqlite = sqliteBinary();
        if (sqlite == null) {
            throw new IOException("Antigravity 대화 기록은 sqlite3로 읽는데 찾지 못했다(macOS는 /usr/bin/sqlite3). AISURADA_SQLITE_BIN으로 자리를 알려 줄 수 있다.");
        }
        Process process = new ProcessBuilder(sqlite.toString(), "-batch", "-readonly", "-cmd", ".timeout 3000", "-separator", "|", uri, sql)
                .redirectError(ProcessBuilder.Redirect.DISCARD)
                .start();
        List<String[]> rows = new ArrayList<>();
        try (BufferedReader out = new BufferedReader(new InputStreamReader(process.getInputStream(), StandardCharsets.UTF_8))) {
            String line;
            while ((line = out.readLine()) != null) {
                if (line.isEmpty()) continue;
                String[] cells = line.split("\\|", -1);
                if (cells.length == columns) rows.add(cells);
            }
        }
        try {
            if (!process.waitFor(SQLITE_TIMEOUT_SECONDS, TimeUnit.SECONDS)) {
                process.destroyForcibly();
                throw new IOException("sqlite3가 " + SQLITE_TIMEOUT_SECONDS + "초 안에 끝나지 않았다");
            }
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            process.destroyForcibly();
            throw new IOException("sqlite3 실행이 끊겼다", e);
        }
        if (process.exitValue() != 0) throw new IOException("sqlite3가 대화 기록을 열지 못했다(종료 코드 " + process.exitValue() + ")");
        return rows;
    }

    /** 시험용 시스템 속성, 환경 변수, 기본 자리, PATH 차례로 sqlite3를 찾는다. 없으면 null. */
    static Path sqliteBinary() {
        String custom = System.getProperty("aisurada.sqlite.bin");
        if (custom == null || custom.isBlank()) custom = System.getenv("AISURADA_SQLITE_BIN");
        if (custom != null && !custom.isBlank()) {
            Path path = Path.of(custom.trim());
            return Files.isExecutable(path) ? path : null;
        }
        Path system = Path.of("/usr/bin/sqlite3");
        if (Files.isExecutable(system)) return system;
        String pathVariable = System.getenv("PATH");
        if (pathVariable == null) return null;
        for (String directory : pathVariable.split(java.io.File.pathSeparator)) {
            if (directory.isBlank()) continue;
            Path candidate = Path.of(directory, "sqlite3");
            if (Files.isExecutable(candidate)) return candidate;
        }
        return null;
    }

    static byte[] unhex(String hex) {
        int length = hex.length() / 2;
        byte[] out = new byte[length];
        for (int i = 0; i < length; i++) {
            out[i] = (byte) ((Character.digit(hex.charAt(2 * i), 16) << 4) | Character.digit(hex.charAt(2 * i + 1), 16));
        }
        return out;
    }
}
