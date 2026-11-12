package aisurada.cli;

import aisurada.args.JsonCompact;
import aisurada.vault.VaultPointer;

import java.io.IOException;
import java.io.PrintStream;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.TimeoutException;

/**
 * {@code ais-cli codex-mcp|gemini-mcp <status|add|remove> [--write]}: Codex나 Gemini 계열 CLI에 이 앱의 MCP 서버를 등록합니다.
 * {@link ClaudeMcpCommand}와 같은 방식입니다. 설정 파일을 직접 고치지 않고 각 CLI의 {@code mcp} 하위 명령만 부르고,
 * 등록에는 vault 경로를 박지 않습니다(서버가 호출마다 앱이 열어 둔 vault를 읽습니다).
 *
 * <p>Gemini 쪽은 둘 중 먼저 찾은 것을 씁니다. Antigravity({@code agy}, {@code antigravity})와 Gemini CLI({@code gemini})는
 * 등록 명령의 모양이 달라서 실행 파일 이름으로 가립니다. 찾는 차례는 호스트(ask.rs의 resolve_gemini)와 같습니다.
 */
final class AgentMcpCommand {

    static final String SERVER_NAME = "aisurada";

    enum Kind { CODEX, GEMINI }

    private AgentMcpCommand() {}

    static void run(Kind kind, String[] args) {
        System.exit(execute(kind, args, System.getenv(), System.out, System.err));
    }

    private static String commandName(Kind kind) {
        return kind == Kind.CODEX ? "codex-mcp" : "gemini-mcp";
    }

    private static String usage(Kind kind) {
        return "사용법: ais-cli " + commandName(kind) + " <status|add|remove> [--write]";
    }

    /** 종료 코드를 돌려줍니다. 0은 성공, 1은 CLI를 못 돌렸거나 실패, 2는 쓰는 법이 틀린 것입니다. */
    static int execute(Kind kind, String[] args, Map<String, String> env, PrintStream out, PrintStream err) {
        String action = null;
        boolean write = false;
        for (String arg : args) {
            if ("--write".equals(arg)) {
                write = true;
            } else if (action == null && ("status".equals(arg) || "add".equals(arg) || "remove".equals(arg))) {
                action = arg;
            } else {
                err.println(usage(kind));
                return 2;
            }
        }
        if (action == null) {
            err.println(usage(kind));
            return 2;
        }
        if (write && !"add".equals(action)) {
            err.println("--write는 add에서만 쓴다");
            return 2;
        }

        String label = kind == Kind.CODEX ? "Codex" : "Gemini";
        String foundKey = kind == Kind.CODEX ? "codexFound" : "geminiFound";
        Path cli = resolve(kind, env);
        if (cli == null) {
            if ("status".equals(action)) {
                out.println("{\"" + foundKey + "\":false,\"registered\":false,\"write\":false,\"detail\":\"\"}");
                return 0;
            }
            err.println(label + " 실행 파일을 못 찾았다. " + (kind == Kind.CODEX
                    ? "Codex를 먼저 깔거나 AISURADA_CODEX_BIN으로 자리를 알려 준다."
                    : "Antigravity(agy)나 Gemini CLI를 먼저 깔거나 AISURADA_GEMINI_BIN으로 자리를 알려 준다."));
            return 1;
        }

        boolean geminiCli = kind == Kind.GEMINI && "gemini".equals(cli.getFileName().toString());
        try {
            switch (action) {
                case "status": {
                    Registered registered = registered(kind, geminiCli, cli, env);
                    out.println("{\"" + foundKey + "\":true,\"cli\":" + quote(cli.getFileName().toString())
                            + ",\"registered\":" + registered.registered + ",\"write\":" + registered.write
                            + ",\"detail\":" + quote(registered.registered ? registered.detail : "") + "}");
                    return 0;
                }
                case "remove": {
                    if (registered(kind, geminiCli, cli, env).registered) {
                        AgentCli.Result removed = AgentCli.run(label, cli, env, removeArgs(kind, geminiCli));
                        if (removed.status() != 0) {
                            err.println("연결을 못 끊었다: " + removed.text().strip());
                            return 1;
                        }
                    }
                    out.println("{\"ok\":true,\"registered\":false}");
                    return 0;
                }
                default:
                    return add(kind, geminiCli, cli, env, write, out, err);
            }
        } catch (IOException | InterruptedException | ExecutionException | TimeoutException e) {
            err.println(label + "를 못 돌렸다: " + e.getMessage());
            return 1;
        }
    }

    private static int add(Kind kind, boolean geminiCli, Path cli, Map<String, String> env, boolean write, PrintStream out, PrintStream err)
            throws IOException, InterruptedException, ExecutionException, TimeoutException {
        McpRegistration registration;
        try {
            registration = McpRegistration.current();
        } catch (IllegalStateException e) {
            err.println(e.getMessage());
            return 1;
        }
        String label = kind == Kind.CODEX ? "Codex" : "Gemini";
        // 같은 이름이 이미 있으면 바꿔 끼운다. 예전에 vault를 박아 등록한 것도 이때 사라진다. 없어서 실패하는 것은 괜찮다.
        if (registered(kind, geminiCli, cli, env).registered) {
            AgentCli.run(label, cli, env, removeArgs(kind, geminiCli));
        }
        AgentCli.Result added = AgentCli.run(label, cli, env, addArgs(kind, geminiCli, registration, write));
        if (added.status() != 0) {
            err.println("연결을 못 했다: " + added.text().strip());
            return 1;
        }
        out.println("{\"ok\":true,\"registered\":true,\"write\":" + write + ",\"followsAppVault\":true,\"cli\":" + quote(cli.getFileName().toString())
                + ",\"command\":" + quote(registration.java()) + ",\"jar\":" + quote(registration.cliJar()) + "}");
        if (VaultPointer.resolve(env.get("AISURADA_VAULT"), System.getProperty("user.home")) == null) {
            err.println("알림: 아직 vault를 정할 수 없다. 앱을 열어 vault를 열면 도구가 그 vault를 쓴다.");
        }
        return 0;
    }

    // ---- 각 CLI의 명령 모양 ----

    private static Path resolve(Kind kind, Map<String, String> env) {
        return kind == Kind.CODEX
                ? AgentCli.resolve(env, List.of("AISURADA_CODEX_BIN"), List.of("codex"))
                : AgentCli.resolve(env, List.of("AISURADA_ANTIGRAVITY_BIN", "AISURADA_GEMINI_BIN"), List.of("antigravity", "agy", "gemini"));
    }

    private record Registered(boolean registered, boolean write, String detail) {}

    /** 등록돼 있는지, 쓰기 모드인지, 그리고 CLI가 보여 주는 글. Antigravity와 Gemini CLI는 get이 없어 list 줄에서 찾는다. */
    private static Registered registered(Kind kind, boolean geminiCli, Path cli, Map<String, String> env)
            throws IOException, InterruptedException, ExecutionException, TimeoutException {
        String label = kind == Kind.CODEX ? "Codex" : "Gemini";
        if (kind == Kind.CODEX) {
            AgentCli.Result get = AgentCli.run(label, cli, env, List.of("mcp", "get", SERVER_NAME));
            return new Registered(get.status() == 0, get.status() == 0 && get.text().contains("--write"), get.text().strip());
        }
        AgentCli.Result list = AgentCli.run(label, cli, env, List.of("mcp", "list"));
        if (list.status() != 0) {
            return new Registered(false, false, "");
        }
        for (String line : list.text().split("\n")) {
            String trimmed = line.strip();
            // agy: "aisurada  stdio  enabled  <명령>", gemini: "✓ aisurada: <명령> (stdio) - Connected" 꼴이다
            boolean mine = geminiCli
                    ? trimmed.matches("^\\S{0,3}\\s*" + SERVER_NAME + ":.*")
                    : trimmed.matches("^" + SERVER_NAME + "\\s.*");
            if (mine) {
                return new Registered(true, trimmed.contains("--write"), trimmed);
            }
        }
        return new Registered(false, false, "");
    }

    private static List<String> removeArgs(Kind kind, boolean geminiCli) {
        if (kind == Kind.CODEX) {
            return List.of("mcp", "remove", SERVER_NAME);
        }
        return geminiCli ? List.of("mcp", "remove", "-s", "user", SERVER_NAME) : List.of("mcp", "remove", SERVER_NAME);
    }

    private static List<String> addArgs(Kind kind, boolean geminiCli, McpRegistration registration, boolean write) {
        List<String> args = new ArrayList<>(List.of("mcp", "add"));
        if (kind == Kind.CODEX) {
            // codex mcp add [--env K=V ...] <이름> -- <명령> <인자...>
            for (Map.Entry<String, String> e : registration.env().entrySet()) {
                args.add("--env");
                args.add(e.getKey() + "=" + e.getValue());
            }
            args.add(SERVER_NAME);
            args.add("--");
            args.add(registration.java());
            args.addAll(registration.serverArgs(write));
        } else if (geminiCli) {
            // gemini mcp add -s user --env=K=V <이름> <명령> -- <인자...>  (--env는 배열 옵션이라 =로 붙여야 이름을 안 삼킨다)
            args.add("-s");
            args.add("user");
            for (Map.Entry<String, String> e : registration.env().entrySet()) {
                args.add("--env=" + e.getKey() + "=" + e.getValue());
            }
            args.add(SERVER_NAME);
            args.add(registration.java());
            args.add("--");
            args.addAll(registration.serverArgs(write));
        } else {
            // agy mcp add [--env K=V ...] <이름> -- <명령> <인자...>
            for (Map.Entry<String, String> e : registration.env().entrySet()) {
                args.add("--env");
                args.add(e.getKey() + "=" + e.getValue());
            }
            args.add(SERVER_NAME);
            args.add("--");
            args.add(registration.java());
            args.addAll(registration.serverArgs(write));
        }
        return args;
    }

    private static String quote(String s) {
        StringBuilder sb = new StringBuilder();
        JsonCompact.appendString(sb, s);
        return sb.toString();
    }
}
