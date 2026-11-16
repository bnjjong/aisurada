package aisurada.cli;

import aisurada.args.JsonReader;
import aisurada.args.JsonValue;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Map;
import java.util.function.Function;

/** 이슈 #81 대조용: Java McpServer 대신 진짜 Rust `ais-cli mcp` 프로세스에 줄을 보내고 답을 받는다. 가짜 호출기는 쓰지 않는다. */
final class McpServer {
    static final List<String> PROTOCOL_VERSIONS = List.of("2025-06-18", "2025-03-26", "2024-11-05");
    private final Process process;
    private final BufferedReader out;
    private final OutputStream in;

    McpServer(boolean allowWrite, Function<String[], AisCliMain.Call> ignored) {
        try {
            Path home = Files.createTempDirectory("aisurada-mcpshim-");
            Path vault = Files.createDirectories(home.resolve("vault/.aisurada"));
            Files.writeString(vault.resolve("workspace.json"), "{}\n");
            ProcessBuilder pb = new ProcessBuilder(allowWrite ? List.of(System.getProperty("shim.cli"), "mcp", "--write") : List.of(System.getProperty("shim.cli"), "mcp"));
            pb.environment().clear();
            pb.environment().putAll(Map.of("HOME", home.toString(), "PATH", "/usr/bin:/bin", "AISURADA_VAULT", home.resolve("vault").toString(),
                    "AISURADA_LOCK_DIR", home.resolve("locks").toString(), "AISURADA_APPROVALS_DIR", home.resolve("approvals").toString()));
            pb.redirectError(ProcessBuilder.Redirect.DISCARD);
            process = pb.start();
            out = new BufferedReader(new InputStreamReader(process.getInputStream(), StandardCharsets.UTF_8));
            in = process.getOutputStream();
        } catch (Exception e) {
            throw new RuntimeException(e);
        }
    }

    /** 답을 기대하는 줄이면 한 줄을 읽어 돌려주고, 알림(id 없는 객체)이면 null이다. */
    String handle(String line) {
        try {
            boolean expectReply = true;
            try {
                JsonValue parsed = JsonReader.parse(line);
                if (parsed instanceof JsonValue.JsonObject o && !o.properties().containsKey("id")) expectReply = false;
            } catch (Exception notJson) {
                expectReply = true;
            }
            in.write((line + "\n").getBytes(StandardCharsets.UTF_8));
            in.flush();
            if (!expectReply) {
                // 알림에는 답이 없어야 한다: 바로 뒤의 ping에 대한 답이 첫 줄로 와야 한다
                in.write("{\"jsonrpc\":\"2.0\",\"id\":\"__sync\",\"method\":\"ping\"}\n".getBytes(StandardCharsets.UTF_8));
                in.flush();
                String next = out.readLine();
                return next != null && next.contains("\"__sync\"") ? null : "UNEXPECTED_REPLY " + next;
            }
            return out.readLine();
        } catch (Exception e) {
            throw new RuntimeException(e);
        }
    }
}
