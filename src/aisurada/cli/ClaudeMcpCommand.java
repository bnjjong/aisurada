package aisurada.cli;

import aisurada.args.JsonCompact;
import aisurada.text.Trim;
import aisurada.vault.VaultPath;

import java.io.IOException;
import java.io.PrintStream;
import java.net.URISyntaxException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;

/**
 * {@code ais-cli claude-mcp <status|add|remove|config> [--write]}: Claude Code에 이 앱의 MCP 서버({@code aisurada})를
 * 등록하는 일을 대신 합니다. 앱의 설정 화면 버튼과 터미널이 같은 길을 씁니다. {@code config}는 등록하지 않고,
 * Claude Desktop 설정 파일에 붙여 넣을 {@code mcpServers} 조각만 출력합니다.
 *
 * <p>Claude의 설정 파일을 직접 고치지 않고 {@code claude mcp add-json --scope user}만 부릅니다. 등록하는 명령은
 * 이 명령을 돌린 바로 그 Java와 그 {@code ais-cli.jar}라서, 앱에서 부르면 앱에 든 Java와 jar가 등록됩니다.
 * 등록은 지금 {@code AISURADA_VAULT}가 가리키는 vault 하나에 묶입니다. 다시 {@code add}하면 바꿔 끼웁니다.
 */
final class ClaudeMcpCommand {

    static final String SERVER_NAME = "aisurada";
    /** 홈 아래 {@code .local/bin} 다음으로 보는 고정 자리. 호스트(ask.rs)와 같다. */
    private static final List<Path> FIXED_CLAUDE_PATHS = List.of(Paths.get("/opt/homebrew/bin/claude"), Paths.get("/usr/local/bin/claude"));
    private static final long CLAUDE_TIMEOUT_SECONDS = 40;
    private static final String USAGE = "사용법: ais-cli claude-mcp <status|add|remove|config> [--write]";

    private ClaudeMcpCommand() {}

    static void run(String[] args) {
        System.exit(execute(args, System.getenv(), System.out, System.err));
    }

    /** 종료 코드를 돌려줍니다. 0은 성공, 1은 claude를 못 돌렸거나 실패, 2는 쓰는 법이 틀린 것입니다. */
    static int execute(String[] args, Map<String, String> env, PrintStream out, PrintStream err) {
        String action = null;
        boolean write = false;
        for (String arg : args) {
            if ("--write".equals(arg)) {
                write = true;
            } else if (action == null && ("status".equals(arg) || "add".equals(arg) || "remove".equals(arg) || "config".equals(arg))) {
                action = arg;
            } else {
                err.println(USAGE);
                return 2;
            }
        }
        if (action == null) {
            err.println(USAGE);
            return 2;
        }
        if (write && !"add".equals(action) && !"config".equals(action)) {
            err.println("--write는 add와 config에서만 쓴다");
            return 2;
        }
        if ("config".equals(action)) {
            return config(env, write, out, err);
        }

        Path claude = resolveClaude(env, FIXED_CLAUDE_PATHS);
        if (claude == null) {
            if ("status".equals(action)) {
                out.println("{\"claudeFound\":false,\"registered\":false,\"write\":false,\"detail\":\"\"}");
                return 0;
            }
            err.println("claude 실행 파일을 못 찾았다. Claude Code를 먼저 깔거나 AISURADA_CLAUDE_BIN으로 자리를 알려 준다.");
            return 1;
        }

        try {
            switch (action) {
                case "status": {
                    Result get = claude(claude, env, List.of("mcp", "get", SERVER_NAME));
                    boolean registered = get.status == 0;
                    out.println("{\"claudeFound\":true,\"registered\":" + registered + ",\"write\":"
                            + (registered && get.text.contains("--write")) + ",\"detail\":" + quote(registered ? get.text.strip() : "") + "}");
                    return 0;
                }
                case "remove": {
                    if (claude(claude, env, List.of("mcp", "get", SERVER_NAME)).status == 0) {
                        Result removed = claude(claude, env, List.of("mcp", "remove", SERVER_NAME, "--scope", "user"));
                        if (removed.status != 0) {
                            err.println("연결을 못 끊었다: " + removed.text.strip());
                            return 1;
                        }
                    }
                    out.println("{\"ok\":true,\"registered\":false}");
                    return 0;
                }
                default:
                    return add(claude, env, write, out, err);
            }
        } catch (IOException | InterruptedException | ExecutionException | TimeoutException e) {
            err.println("claude를 못 돌렸다: " + e.getMessage());
            return 1;
        }
    }

    private static int add(Path claude, Map<String, String> env, boolean write, PrintStream out, PrintStream err)
            throws IOException, InterruptedException, ExecutionException, TimeoutException {
        Registration registration;
        try {
            registration = registration(env, write);
        } catch (Failure f) {
            err.println(f.getMessage());
            return f.code;
        }
        String normalizedVault = registration.vault;
        String java = registration.java;
        String cliJar = registration.jar;
        String payload = registration.payload;

        // 같은 이름이 이미 있으면 바꿔 끼운다. 없어서 실패하는 것은 괜찮다.
        claude(claude, env, List.of("mcp", "remove", SERVER_NAME, "--scope", "user"));
        Result added = claude(claude, env, List.of("mcp", "add-json", SERVER_NAME, payload, "--scope", "user"));
        if (added.status != 0) {
            err.println("연결을 못 했다: " + added.text.strip());
            return 1;
        }
        out.println("{\"ok\":true,\"registered\":true,\"write\":" + write + ",\"vault\":" + quote(normalizedVault)
                + ",\"command\":" + quote(java) + ",\"jar\":" + quote(cliJar) + "}");
        return 0;
    }

    /** Claude Desktop 설정 파일에 붙여 넣을 {@code mcpServers} 조각을 출력한다. 아무것도 등록하지 않는다. */
    private static int config(Map<String, String> env, boolean write, PrintStream out, PrintStream err) {
        try {
            Registration registration = registration(env, write);
            out.println("{\"mcpServers\":{" + quote(SERVER_NAME) + ":" + registration.payload + "}}");
            return 0;
        } catch (Failure f) {
            err.println(f.getMessage());
            return f.code;
        }
    }

    /** 등록할 값 하나치. 이 명령을 돌린 Java와 ais-cli.jar, 지금 vault와 엔진 jar를 모은 것이다. */
    private record Registration(String vault, String java, String jar, String payload) {}

    /** 쓰는 법이나 환경이 틀려서 멈춘다. 종료 코드를 같이 실어 나른다. */
    private static final class Failure extends RuntimeException {
        final int code;

        Failure(int code, String message) {
            super(message);
            this.code = code;
        }
    }

    private static Registration registration(Map<String, String> env, boolean write) {
        String vault = env.get("AISURADA_VAULT");
        if (vault == null || Trim.isBlank(vault)) {
            throw new Failure(2, "AIS-400 AISURADA_VAULT 환경변수가 없다. 연결할 vault 폴더의 절대 경로를 넣고 다시 실행한다.");
        }
        String normalizedVault;
        try {
            normalizedVault = VaultPath.normalize(Trim.trim(vault));
        } catch (RuntimeException e) {
            normalizedVault = Trim.trim(vault);
        }
        String java = PlistGenerator.resolveSidecarJava();
        String cliJar = runningJar();
        if (cliJar == null) {
            throw new Failure(1, "ais-cli.jar의 자리를 알 수 없다. jar로 돌린 ais-cli에서만 연결할 수 있다.");
        }
        String sidecarJar = PlistGenerator.resolveSidecarJar(System.getProperty("user.home"));

        StringBuilder payload = new StringBuilder("{\"type\":\"stdio\",\"command\":");
        JsonCompact.appendString(payload, java);
        payload.append(",\"args\":[\"-jar\",");
        JsonCompact.appendString(payload, cliJar);
        payload.append(",\"mcp\"").append(write ? ",\"--write\"" : "").append("],\"env\":{\"AISURADA_VAULT\":");
        JsonCompact.appendString(payload, normalizedVault);
        if (sidecarJar != null) {
            payload.append(",\"AISURADA_SIDECAR_JAR\":");
            JsonCompact.appendString(payload, sidecarJar);
        }
        payload.append("}}");
        return new Registration(normalizedVault, java, cliJar, payload.toString());
    }

    /** claude 실행 파일을 찾는 차례는 호스트(ask.rs의 resolve_claude)와 같다. */
    static Path resolveClaude(Map<String, String> env, List<Path> fixedPaths) {
        List<Path> candidates = new ArrayList<>();
        String bin = env.get("AISURADA_CLAUDE_BIN");
        if (bin != null && !bin.isEmpty()) {
            candidates.add(Paths.get(bin));
        }
        String home = env.get("HOME");
        if (home != null && !home.isEmpty()) {
            candidates.add(Paths.get(home, ".local/bin/claude"));
        }
        candidates.addAll(fixedPaths);
        String path = env.get("PATH");
        if (path != null) {
            for (String dir : path.split(":")) {
                if (!dir.isEmpty()) {
                    candidates.add(Paths.get(dir, "claude"));
                }
            }
        }
        for (Path candidate : candidates) {
            if (Files.isRegularFile(candidate)) {
                return candidate;
            }
        }
        return null;
    }

    /** 지금 돌고 있는 ais-cli.jar의 절대 경로. jar로 돌지 않았으면 null이다. */
    private static String runningJar() {
        try {
            Path location = Paths.get(ClaudeMcpCommand.class.getProtectionDomain().getCodeSource().getLocation().toURI());
            return Files.isRegularFile(location) && location.toString().endsWith(".jar") ? location.toAbsolutePath().normalize().toString() : null;
        } catch (URISyntaxException | RuntimeException e) {
            return null;
        }
    }

    private record Result(int status, String text) {}

    /** claude를 홈 폴더에서 돌려 종료 코드와 출력을 모은다. 프로젝트 설정(.mcp.json)이 끼어들지 않게 하려는 것이다. */
    private static Result claude(Path claude, Map<String, String> env, List<String> args)
            throws IOException, InterruptedException, ExecutionException, TimeoutException {
        List<String> command = new ArrayList<>();
        command.add(claude.toString());
        command.addAll(args);
        ProcessBuilder builder = new ProcessBuilder(command).redirectErrorStream(true);
        String home = env.get("HOME");
        if (home != null && Files.isDirectory(Paths.get(home))) {
            builder.directory(Paths.get(home).toFile());
        }
        builder.environment().clear();
        builder.environment().putAll(env);
        Process process = builder.start();
        process.getOutputStream().close();
        CompletableFuture<String> output = CompletableFuture.supplyAsync(() -> {
            try {
                return new String(process.getInputStream().readAllBytes(), StandardCharsets.UTF_8);
            } catch (IOException e) {
                return "";
            }
        });
        if (!process.waitFor(CLAUDE_TIMEOUT_SECONDS, TimeUnit.SECONDS)) {
            process.destroyForcibly();
            throw new TimeoutException("claude " + String.join(" ", args.subList(0, Math.min(2, args.size()))) + " 가 " + CLAUDE_TIMEOUT_SECONDS + "초 안에 안 끝났다");
        }
        return new Result(process.exitValue(), output.get(5, TimeUnit.SECONDS));
    }

    private static String quote(String s) {
        StringBuilder sb = new StringBuilder();
        JsonCompact.appendString(sb, s);
        return sb.toString();
    }
}
