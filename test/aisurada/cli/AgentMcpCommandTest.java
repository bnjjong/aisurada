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
 * {@code ais-cli codex-mcp}와 {@code gemini-mcp}의 시험. 진짜 {@code ais-cli.jar}를 돌리고, 진짜 CLI 대신 등록 상태를 파일에 두는
 * 가짜 CLI를 쓴다. Codex, Antigravity(agy), Gemini CLI의 {@code mcp add} 인자 모양이 각각 맞는지, 등록에 vault를 안 박는지 잰다.
 * 진짜 CLI로는 임시 HOME에서 따로 확인했고, 이 시험은 진짜 설정이나 HOME을 건드리지 않는다.
 */
public final class AgentMcpCommandTest {

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

    /** 세 가짜가 같은 모양이다: mcp add는 받은 인자를 줄마다 상태 파일에 적고, get/list는 그것을 보여 준다. */
    private static String fake(String showCommand) {
        return """
                #!/bin/sh
                echo "$*" >> "$FAKE_LOG"
                case "$1 $2" in
                  "mcp get"|"mcp list")
                    %s ;;
                  "mcp remove")
                    if [ -f "$FAKE_STATE" ]; then rm -f "$FAKE_STATE"; exit 0; fi
                    echo "not found" >&2; exit 1 ;;
                  "mcp add")
                    if [ -n "$FAKE_FAIL_ADD" ]; then echo "boom: $FAKE_FAIL_ADD" >&2; exit 1; fi
                    shift 2; printf '%%s\\n' "$@" > "$FAKE_STATE"; exit 0 ;;
                esac
                exit 9
                """.formatted(showCommand);
    }

    private static final String FAKE_CODEX = fake(
            "if [ -f \"$FAKE_STATE\" ]; then echo aisurada; echo \"  args: $(tr '\\n' ' ' < \"$FAKE_STATE\")\"; exit 0; fi; echo \"No MCP server found\" >&2; exit 1");
    private static final String FAKE_AGY = fake(
            "if [ -f \"$FAKE_STATE\" ]; then echo 'NAME  TYPE  STATUS  COMMAND/URL'; echo \"aisurada  stdio  enabled  $(tr '\\n' ' ' < \"$FAKE_STATE\")\"; echo 'notion  stdio  enabled  npx x'; else echo 'No MCP servers configured.'; fi; exit 0");
    private static final String FAKE_GEMINI = fake(
            "if [ -f \"$FAKE_STATE\" ]; then echo \"✓ aisurada: $(tr '\\n' ' ' < \"$FAKE_STATE\") (stdio) - Connected\"; else echo 'No MCP servers configured.'; fi; exit 0");

    private record Run(int status, String out, String err) {}

    private static JsonValue field(JsonValue v, String key) {
        return v instanceof JsonValue.JsonObject o ? o.properties().get(key) : null;
    }

    private static String text(JsonValue v) {
        return v instanceof JsonValue.JsonString s ? s.value() : v instanceof JsonValue.JsonLiteral l ? l.raw() : null;
    }

    private static Run cli(Path home, Map<String, String> env, String kind, String... args) throws Exception {
        List<String> command = new ArrayList<>(List.of(Path.of(System.getProperty("java.home"), "bin/java").toString(),
                "-Duser.home=" + home, "-jar", Path.of("dist/ais-cli.jar").toAbsolutePath().toString(), kind));
        command.addAll(List.of(args));
        ProcessBuilder builder = new ProcessBuilder(command);
        builder.environment().clear();
        builder.environment().putAll(env);
        builder.redirectOutput(home.resolve("cli.out").toFile());
        builder.redirectError(home.resolve("cli.err").toFile());
        Process process = builder.start();
        check(process.waitFor(60, TimeUnit.SECONDS), kind + "이 끝난다");
        return new Run(process.exitValue(), Files.readString(home.resolve("cli.out")), Files.readString(home.resolve("cli.err")));
    }

    private static Path script(Path dir, String name, String body) throws Exception {
        Path file = dir.resolve(name);
        Files.writeString(file, body);
        Files.setPosixFilePermissions(file, PosixFilePermissions.fromString("rwxr-xr-x"));
        return file;
    }

    /** 한 CLI에 대해 status, add, 교체, remove, 실패, 사용법, CLI 없음을 모두 잰다. */
    private static void exercise(Path temp, String label, String kind, String foundKey, String binEnv, Path fakeCli, List<String> expectedAddArgs, String cliName) throws Exception {
        Path home = Files.createDirectories(temp.resolve("home-" + label));
        Path log = Files.writeString(temp.resolve(label + ".log"), "");
        Path state = temp.resolve(label + ".state");
        Path vault = Files.createDirectories(temp.resolve("vault-" + label));

        Map<String, String> env = new HashMap<>();
        env.put("HOME", home.toString());
        env.put("PATH", "/usr/bin:/bin");
        env.put(binEnv, fakeCli.toString());
        env.put("AISURADA_VAULT", vault.toString());
        env.put("AISURADA_SIDECAR_JAR", "/opt/app/ais-sidecar.jar");
        env.put("FAKE_LOG", log.toString());
        env.put("FAKE_STATE", state.toString());

        Run status = cli(home, env, kind, "status");
        eq(status.status, 0, label + " status 종료 코드: " + status.err);
        JsonValue parsed = JsonReader.parse(status.out);
        eq(text(field(parsed, foundKey)), "true", label + ": CLI를 찾았다");
        eq(text(field(parsed, "cli")), cliName, label + ": 찾은 CLI 이름");
        eq(text(field(parsed, "registered")), "false", label + ": 처음엔 등록 안 됨");

        // 읽기 전용으로 등록: 인자 모양이 CLI마다 다르다. 환경에 AISURADA_VAULT가 있어도 등록에 박지 않는다
        Run added = cli(home, env, kind, "add");
        eq(added.status, 0, label + " add 종료 코드: " + added.err);
        check(Files.exists(state), label + ": mcp add가 불렸다");
        String java = Path.of(System.getProperty("java.home"), "bin/java").toString();
        String jar = Path.of("dist/ais-cli.jar").toAbsolutePath().normalize().toString();
        List<String> expected = new ArrayList<>();
        for (String arg : expectedAddArgs) {
            expected.add(arg.replace("<java>", java).replace("<jar>", jar));
        }
        eq(Files.readAllLines(state), expected, label + ": 등록 인자");
        check(!Files.readString(state).contains("AISURADA_VAULT"), label + ": vault를 등록에 박지 않는다");
        eq(text(field(JsonReader.parse(added.out), "followsAppVault")), "true", label + ": add 응답은 앱이 연 vault를 따른다");
        eq(text(field(JsonReader.parse(added.out), "write")), "false", label + ": add 응답: 쓰기 없음");

        status = cli(home, env, kind, "status");
        eq(text(field(JsonReader.parse(status.out), "registered")), "true", label + ": 등록 뒤 status");
        eq(text(field(JsonReader.parse(status.out), "write")), "false", label + ": 읽기 전용으로 보인다");

        // 쓰기를 켜서 다시 등록하면 바꿔 끼운다
        Files.writeString(log, "");
        Run again = cli(home, env, kind, "add", "--write");
        eq(again.status, 0, label + " add --write 종료 코드: " + again.err);
        String calls = Files.readString(log);
        check(calls.indexOf("mcp remove") >= 0 && calls.indexOf("mcp remove") < calls.indexOf("mcp add"), label + ": 지우고 다시 더한다: " + calls);
        check(Files.readString(state).contains("--write"), label + ": 쓰기 인자가 들어갔다");
        eq(text(field(JsonReader.parse(cli(home, env, kind, "status").out), "write")), "true", label + ": status가 쓰기를 알려 준다");

        // 끊기
        Run removed = cli(home, env, kind, "remove");
        eq(removed.status, 0, label + " remove 종료 코드: " + removed.err);
        check(!Files.exists(state), label + ": 끊었다");
        Files.writeString(log, "");
        eq(cli(home, env, kind, "remove").status, 0, label + ": 이미 없어도 remove는 성공");
        check(!Files.readString(log).contains("mcp remove"), label + ": 없는 것은 지우려 하지 않는다");

        // 실패와 사용법
        Map<String, String> failing = new HashMap<>(env);
        failing.put("FAKE_FAIL_ADD", "denied");
        Run failed = cli(home, failing, kind, "add");
        eq(failed.status, 1, label + ": CLI가 실패하면 종료 코드 1");
        check(failed.err.contains("denied"), label + ": CLI의 오류 글을 알려 준다: " + failed.err);
        eq(cli(home, env, kind, "bogus").status, 2, label + ": 모르는 동작은 사용법");
        eq(cli(home, env, kind).status, 2, label + ": 동작이 없으면 사용법");
        eq(cli(home, env, kind, "status", "--write").status, 2, label + ": --write는 add에서만");
        eq(cli(home, env, kind, "config").status, 2, label + ": config는 없다");

        // vault가 없어도 등록은 되고, 앱이 연 vault가 없다는 것만 알린다
        Map<String, String> noVault = new HashMap<>(env);
        noVault.remove("AISURADA_VAULT");
        Run bare = cli(home, noVault, kind, "add");
        eq(bare.status, 0, label + ": vault 없이도 등록: " + bare.err);
        check(bare.err.contains("앱을 열어"), label + ": 앱을 열라고 알린다: " + bare.err);
        cli(home, env, kind, "remove");

        // CLI가 없을 때: status는 알려 주고, add는 실패한다
        Map<String, String> none = new HashMap<>(env);
        none.remove(binEnv);
        none.put("PATH", "/nonexistent");
        Run absent = cli(home, none, kind, "status");
        eq(absent.status, 0, label + ": CLI가 없어도 status는 성공");
        eq(text(field(JsonReader.parse(absent.out), foundKey)), "false", label + ": CLI 없음을 알린다");
        eq(cli(home, none, kind, "add").status, 1, label + ": CLI가 없으면 add는 실패");
    }

    public static void main(String[] args) throws Exception {
        Path temp = Files.createTempDirectory("aisurada-agent-mcp-");
        Path bin = Files.createDirectory(temp.resolve("bin"));
        Path codexDir = Files.createDirectory(bin.resolve("c"));
        Path agyDir = Files.createDirectory(bin.resolve("a"));
        Path geminiDir = Files.createDirectory(bin.resolve("g"));

        List<String> codexLike = List.of("--env", "AISURADA_SIDECAR_JAR=/opt/app/ais-sidecar.jar", "aisurada", "--", "<java>", "-jar", "<jar>", "mcp");
        exercise(temp, "codex", "codex-mcp", "codexFound", "AISURADA_CODEX_BIN", script(codexDir, "codex", FAKE_CODEX), codexLike, "codex");
        exercise(temp, "agy", "gemini-mcp", "geminiFound", "AISURADA_GEMINI_BIN", script(agyDir, "agy", FAKE_AGY), codexLike, "agy");
        List<String> geminiCli = List.of("-s", "user", "--env=AISURADA_SIDECAR_JAR=/opt/app/ais-sidecar.jar", "aisurada", "<java>", "--", "-jar", "<jar>", "mcp");
        exercise(temp, "gemini", "gemini-mcp", "geminiFound", "AISURADA_GEMINI_BIN", script(geminiDir, "gemini", FAKE_GEMINI), geminiCli, "gemini");

        // 찾는 차례: 환경변수로 준 자리, 홈 아래 .local/bin, Homebrew와 /usr/local/bin, PATH. 이름 차례는 antigravity, agy, gemini
        Path home = Files.createDirectory(temp.resolve("search-home"));
        Path pathDir = Files.createDirectory(temp.resolve("on-path"));
        Path pathGemini = Files.writeString(pathDir.resolve("gemini"), "x");
        Path pathAgy = Files.writeString(pathDir.resolve("agy"), "x");
        List<String> names = List.of("antigravity", "agy", "gemini");
        Map<String, String> search = new HashMap<>();
        search.put("PATH", pathDir.toString());
        eq(AgentCli.resolve(search, List.of("AISURADA_GEMINI_BIN"), names), pathAgy, "같은 폴더에서는 agy가 gemini보다 먼저");
        Path localBin = Files.createDirectories(home.resolve(".local/bin"));
        Path homeGemini = Files.writeString(localBin.resolve("gemini"), "x");
        search.put("HOME", home.toString());
        eq(AgentCli.resolve(search, List.of("AISURADA_GEMINI_BIN"), names), homeGemini, "홈 아래 .local/bin이 PATH보다 먼저");
        Path given = Files.writeString(temp.resolve("given-agy"), "x");
        search.put("AISURADA_GEMINI_BIN", given.toString());
        eq(AgentCli.resolve(search, List.of("AISURADA_ANTIGRAVITY_BIN", "AISURADA_GEMINI_BIN"), names), given, "환경변수로 준 자리가 가장 먼저");
        search.put("AISURADA_ANTIGRAVITY_BIN", pathGemini.toString());
        eq(AgentCli.resolve(search, List.of("AISURADA_ANTIGRAVITY_BIN", "AISURADA_GEMINI_BIN"), names), pathGemini, "환경변수끼리는 앞의 것이 먼저");
        search.put("AISURADA_ANTIGRAVITY_BIN", temp.resolve("missing").toString());
        eq(AgentCli.resolve(search, List.of("AISURADA_ANTIGRAVITY_BIN", "AISURADA_GEMINI_BIN"), names), given, "가리킨 파일이 없으면 다음 후보로 간다");
        eq(AgentCli.resolve(Map.of("PATH", "/nonexistent"), List.of(), List.of("codex")), null, "어디에도 없으면 null");

        System.out.println("AgentMcpCommandTest: " + checks + "건 통과");
    }
}
