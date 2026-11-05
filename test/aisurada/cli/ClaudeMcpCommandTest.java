package aisurada.cli;

import aisurada.args.JsonReader;
import aisurada.args.JsonValue;

import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.attribute.PosixFilePermissions;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.TimeUnit;

/**
 * {@code ais-cli claude-mcp}의 시험. 진짜 {@code ais-cli.jar}를 돌리고, 진짜 claude 대신 등록 상태를 파일에 두는
 * 가짜 claude를 쓴다. 진짜 Claude 설정이나 HOME은 건드리지 않는다.
 */
public final class ClaudeMcpCommandTest {

    private static int checks;

    private static void check(boolean ok, String what) {
        checks++;
        if (!ok) {
            throw new AssertionError(what);
        }
    }

    private static void eq(Object actual, Object expected, String what) {
        check(java.util.Objects.equals(actual, expected), what + ": 기대 " + expected + ", 실제 " + actual);
    }

    private static final String FAKE_CLAUDE = """
            #!/bin/sh
            echo "$*" >> "$FAKE_LOG"
            case "$1 $2" in
              "mcp get")
                if [ -f "$FAKE_STATE" ]; then echo "aisurada:"; echo "  Args: $(cat "$FAKE_STATE")"; exit 0; fi
                echo "No MCP server found with name: aisurada" >&2; exit 1 ;;
              "mcp remove")
                if [ -f "$FAKE_STATE" ]; then rm -f "$FAKE_STATE"; exit 0; fi
                echo "No MCP server found" >&2; exit 1 ;;
              "mcp add-json")
                if [ -n "$FAKE_FAIL_ADD" ]; then echo "boom: $FAKE_FAIL_ADD" >&2; exit 1; fi
                printf '%s' "$4" > "$FAKE_STATE"; exit 0 ;;
            esac
            exit 9
            """;

    private record Run(int status, String out, String err) {}

    private static Map<String, String> noVault(Map<String, String> env) {
        Map<String, String> copy = new HashMap<>(env);
        copy.remove("AISURADA_VAULT");
        return copy;
    }

    private static JsonValue field(JsonValue v, String key) {
        return v instanceof JsonValue.JsonObject o ? o.properties().get(key) : null;
    }

    private static String text(JsonValue v) {
        return v instanceof JsonValue.JsonString s ? s.value() : v instanceof JsonValue.JsonLiteral l ? l.raw() : null;
    }

    private static Run cli(Path home, Map<String, String> extra, String... args) throws Exception {
        List<String> command = new ArrayList<>(List.of(Path.of(System.getProperty("java.home"), "bin/java").toString(),
                "-Duser.home=" + home, "-jar", Path.of("dist/ais-cli.jar").toAbsolutePath().toString(), "claude-mcp"));
        command.addAll(List.of(args));
        ProcessBuilder builder = new ProcessBuilder(command);
        builder.environment().clear();
        builder.environment().putAll(extra);
        builder.redirectOutput(home.resolve("cli.out").toFile());
        builder.redirectError(home.resolve("cli.err").toFile());
        Process process = builder.start();
        check(process.waitFor(60, TimeUnit.SECONDS), "claude-mcp이 끝난다");
        return new Run(process.exitValue(), Files.readString(home.resolve("cli.out")), Files.readString(home.resolve("cli.err")));
    }

    public static void main(String[] args) throws Exception {
        Path temp = Files.createTempDirectory("aisurada-claude-mcp-");
        Path home = Files.createDirectory(temp.resolve("home"));
        Path vault = Files.createDirectory(temp.resolve("vault dir"));
        Path fake = temp.resolve("claude");
        Files.writeString(fake, FAKE_CLAUDE);
        Files.setPosixFilePermissions(fake, PosixFilePermissions.fromString("rwxr-xr-x"));
        Path log = temp.resolve("claude.log");
        Path state = temp.resolve("registered.json");
        Files.writeString(log, "");

        Map<String, String> env = new HashMap<>();
        env.put("HOME", home.toString());
        env.put("PATH", "/usr/bin:/bin");
        env.put("AISURADA_CLAUDE_BIN", fake.toString());
        env.put("AISURADA_VAULT", vault.toString());
        env.put("AISURADA_SIDECAR_JAR", "/opt/app/ais-sidecar.jar");
        env.put("FAKE_LOG", log.toString());
        env.put("FAKE_STATE", state.toString());

        // 처음엔 등록돼 있지 않다
        Run status = cli(home, env, "status");
        eq(status.status, 0, "status 종료 코드: " + status.err);
        eq(text(field(JsonReader.parse(status.out), "claudeFound")), "true", "claude를 찾았다");
        eq(text(field(JsonReader.parse(status.out), "registered")), "false", "처음엔 등록 안 됨");

        // 읽기 전용으로 등록
        Run added = cli(home, env, "add");
        eq(added.status, 0, "add 종료 코드: " + added.err);
        check(Files.exists(state), "claude mcp add-json이 불렸다");
        JsonValue payload = JsonReader.parse(Files.readString(state));
        eq(text(field(payload, "type")), "stdio", "stdio 서버");
        eq(text(field(payload, "command")), Path.of(System.getProperty("java.home"), "bin/java").toString(), "등록하는 Java는 이 명령을 돌린 Java");
        List<String> argv = new ArrayList<>();
        ((JsonValue.JsonArray) field(payload, "args")).elements().forEach(e -> argv.add(text(e)));
        eq(argv, List.of("-jar", Path.of("dist/ais-cli.jar").toAbsolutePath().normalize().toString(), "mcp"), "등록하는 인자는 이 ais-cli.jar의 mcp 모드(쓰기 없음)");
        check(text(field(field(payload, "env"), "AISURADA_VAULT")).endsWith("vault dir"), "공백이 든 vault 경로도 그대로 간다");
        eq(text(field(field(payload, "env"), "AISURADA_SIDECAR_JAR")), "/opt/app/ais-sidecar.jar", "엔진 jar 환경 변수도 함께 등록");
        eq(text(field(JsonReader.parse(added.out), "write")), "false", "add 응답: 쓰기 없음");
        check(Files.readString(log).contains("--scope user"), "사용자 범위로 등록한다");

        status = cli(home, env, "status");
        eq(text(field(JsonReader.parse(status.out), "registered")), "true", "등록 뒤 status");
        eq(text(field(JsonReader.parse(status.out), "write")), "false", "읽기 전용으로 보인다");

        // 쓰기를 켜서 다시 등록하면 바꿔 끼운다
        Files.writeString(log, "");
        Run again = cli(home, env, "add", "--write");
        eq(again.status, 0, "add --write 종료 코드: " + again.err);
        List<String> calls = Files.readAllLines(log);
        check(calls.get(0).startsWith("mcp remove aisurada") && calls.get(1).startsWith("mcp add-json aisurada"), "지우고 다시 더한다: " + calls);
        check(Files.readString(state).contains("\"mcp\",\"--write\""), "쓰기 인자가 들어갔다");
        eq(text(field(JsonReader.parse(cli(home, env, "status").out), "write")), "true", "status가 쓰기를 알려 준다");

        // 끊기
        Run removed = cli(home, env, "remove");
        eq(removed.status, 0, "remove 종료 코드: " + removed.err);
        check(!Files.exists(state), "끊었다");
        Files.writeString(log, "");
        eq(cli(home, env, "remove").status, 0, "이미 없어도 remove는 성공");
        check(!Files.readString(log).contains("mcp remove"), "없는 것은 지우려 하지 않는다");

        // 실패와 사용법
        Map<String, String> failing = new HashMap<>(env);
        failing.put("FAKE_FAIL_ADD", "denied");
        Run failed = cli(home, failing, "add");
        eq(failed.status, 1, "claude가 실패하면 종료 코드 1");
        check(failed.err.contains("denied"), "claude의 오류 글을 알려 준다: " + failed.err);
        Map<String, String> noVault = new HashMap<>(env);
        noVault.remove("AISURADA_VAULT");
        Run missingVault = cli(home, noVault, "add");
        eq(missingVault.status, 2, "vault가 없으면 종료 코드 2");
        check(missingVault.err.contains("AISURADA_VAULT"), "vault 환경 변수를 알려 준다");
        eq(cli(home, env, "bogus").status, 2, "모르는 동작은 사용법");
        eq(cli(home, env).status, 2, "동작이 없으면 사용법");
        eq(cli(home, env, "status", "--write").status, 2, "--write는 add에서만");

        // config: 등록하지 않고 Claude Desktop에 붙여 넣을 조각만 출력한다(claude가 없어도 된다)
        Map<String, String> noClaudeAtAll = new HashMap<>(env);
        noClaudeAtAll.remove("AISURADA_CLAUDE_BIN");
        noClaudeAtAll.put("PATH", "/nonexistent");
        Files.writeString(log, "");
        Run config = cli(home, noClaudeAtAll, "config", "--write");
        eq(config.status, 0, "config 종료 코드: " + config.err);
        JsonValue server = field(field(JsonReader.parse(config.out), "mcpServers"), "aisurada");
        check(server != null, "mcpServers.aisurada가 있다: " + config.out);
        eq(text(field(server, "type")), "stdio", "config: stdio");
        check(((JsonValue.JsonArray) field(server, "args")).elements().stream().map(ClaudeMcpCommandTest::text).toList().contains("--write"), "config --write는 쓰기 인자를 싣는다");
        eq(Files.readString(log), "", "config는 claude를 부르지 않는다");
        check(!Files.exists(state), "config는 아무것도 등록하지 않는다");
        eq(cli(home, noVault(env), "config").status, 2, "config도 vault가 없으면 종료 코드 2");

        // claude가 없을 때: status는 알려 주고, add는 실패한다
        Map<String, String> noClaude = new HashMap<>(env);
        noClaude.remove("AISURADA_CLAUDE_BIN");
        noClaude.put("PATH", "/nonexistent");
        Run absent = cli(home, noClaude, "status");
        eq(absent.status, 0, "claude가 없어도 status는 성공");
        eq(text(field(JsonReader.parse(absent.out), "claudeFound")), "false", "claude 없음을 알린다");
        eq(cli(home, noClaude, "add").status, 1, "claude가 없으면 add는 실패");

        // 찾는 차례: AISURADA_CLAUDE_BIN, 홈 아래 .local/bin, 고정 자리, PATH
        Path pathDir = Files.createDirectory(temp.resolve("on-path"));
        Path onPath = Files.writeString(pathDir.resolve("claude"), "x");
        Path fixed = Files.writeString(temp.resolve("fixed-claude"), "x");
        Path localBin = Files.createDirectories(home.resolve(".local/bin"));
        Path inHome = Files.writeString(localBin.resolve("claude"), "x");
        Map<String, String> search = new HashMap<>();
        search.put("PATH", pathDir.toString());
        eq(ClaudeMcpCommand.resolveClaude(search, List.of(fixed)), fixed, "고정 자리가 PATH보다 먼저");
        eq(ClaudeMcpCommand.resolveClaude(search, List.of()), onPath, "PATH에서 찾는다");
        search.put("HOME", home.toString());
        eq(ClaudeMcpCommand.resolveClaude(search, List.of(fixed)), inHome, "홈 아래 .local/bin이 고정 자리보다 먼저");
        search.put("AISURADA_CLAUDE_BIN", fake.toString());
        eq(ClaudeMcpCommand.resolveClaude(search, List.of(fixed)), fake, "AISURADA_CLAUDE_BIN이 가장 먼저");
        search.put("AISURADA_CLAUDE_BIN", temp.resolve("missing").toString());
        eq(ClaudeMcpCommand.resolveClaude(search, List.of(fixed)), inHome, "가리킨 파일이 없으면 다음 후보로 간다");
        eq(ClaudeMcpCommand.resolveClaude(Map.of("PATH", "/nonexistent"), List.of()), null, "어디에도 없으면 null");

        System.out.println("ClaudeMcpCommandTest: " + checks + "건 통과");
    }
}
