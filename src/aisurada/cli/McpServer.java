package aisurada.cli;

import aisurada.session.SessionAgents;
import aisurada.vault.VaultPointer;
import aisurada.args.JsonCompact;
import aisurada.args.JsonReader;
import aisurada.args.JsonValue;

import java.io.BufferedReader;
import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.util.List;
import java.util.Map;
import java.util.function.Function;

/**
 * {@code ais-cli mcp}: Claude 같은 MCP 클라이언트가 vault의 노트를 도구로 읽고 쓰게 하는 MCP 서버입니다.
 *
 * <p>표준 입출력으로 한 줄에 JSON-RPC 메시지 하나씩 주고받고(표준 입력이 닫히면 끝낸다), 도구 호출은 단발
 * {@code ais-cli}와 같은 길(소켓, 엔진 띄우기, 요청 왕복)로 엔진에 보냅니다. JVM을 한 번만 띄우고 호출마다
 * 재사용하므로 호출마다 JVM을 띄우는 비용이 없습니다.
 *
 * <p>vault는 {@code AISURADA_VAULT} 환경 변수가 있으면 그것으로, 없으면 앱이 열어 둔 vault({@link VaultPointer})로 정하고,
 * 도구를 부를 때마다 다시 정합니다. 그래서 등록에는 vault 경로를 박지 않아도 앱에서 vault를 바꾸면 따라갑니다.
 * 기본은 읽기 도구만 내놓고, {@code --write}를 주면 쓰기 도구도 내놓습니다({@link McpTools}).
 */
final class McpServer {

    static final String SERVER_NAME = "aisurada";
    static final String SERVER_VERSION = "0.1.0";

    /** 이 서버가 아는 MCP 규격 개정. 맨 앞이 가장 새것이고, 클라이언트가 모르는 것을 청하면 이것으로 답한다. */
    static final List<String> PROTOCOL_VERSIONS = List.of("2025-06-18", "2025-03-26", "2024-11-05");

    private static final int PARSE_ERROR = -32700;
    private static final int INVALID_REQUEST = -32600;
    private static final int METHOD_NOT_FOUND = -32601;
    private static final int INVALID_PARAMS = -32602;
    private static final int INTERNAL_ERROR = -32603;

    private static final String INSTRUCTIONS =
            "aisurada vault의 노트를 읽고 쓰는 도구들이다. 기존 노트를 고치는 도구(update_doc, set_favorite, evaluate_doc, "
                    + "link_docs, update_sub_note)는 f1이 꼭 필요하다. 고치기 전에 get_doc으로 읽어 precondition 값을 얻어 f1으로 넘긴다. "
                    + "방금 create_doc이나 update_doc을 했다면 그 응답의 precondition을 쓴다. 그 사이 다른 곳에서 고쳐졌으면 덮어쓰지 않고 멈춘다. "
                    + "id는 list_docs나 search_docs가 준 값을 그대로 쓴다.";

    private final boolean allowWrite;
    private final Function<String[], AisCliMain.Call> caller;

    McpServer(boolean allowWrite, Function<String[], AisCliMain.Call> caller) {
        this.allowWrite = allowWrite;
        this.caller = caller;
    }

    /** {@code ais-cli mcp [--write]}의 진입점입니다. 표준 입력이 닫힐 때까지 돌고 프로세스를 끝냅니다. */
    static void run(String[] args) {
        boolean write = false;
        for (String arg : args) {
            if ("--write".equals(arg)) {
                write = true;
            } else if ("--help".equals(arg) || "-h".equals(arg)) {
                System.out.println("사용법: ais-cli mcp [--write]");
                System.out.println("  MCP 서버로 선다(표준 입출력). vault는 AISURADA_VAULT로 정하고, 없으면 앱이 열어 둔 vault를 호출마다 따른다.");
                System.out.println("  기본은 읽기 도구만 내놓는다. --write를 주면 노트를 만들고 고치는 도구도 내놓는다.");
                System.out.println("  영구 삭제(purge_doc)는 어느 쪽으로도 내놓지 않는다.");
                System.exit(0);
                return;
            } else {
                System.err.println("알 수 없는 인자: " + arg + " (ais-cli mcp [--write])");
                System.exit(2);
                return;
            }
        }
        // 프로토콜 출력은 처음의 표준 출력 하나로만 내보낸다. 그 밖의 글이 섞이지 않게 System.out은 표준 오류로 돌려 둔다.
        PrintStream protocol = new PrintStream(new FileOutputStream(FileDescriptor.out), false, StandardCharsets.UTF_8);
        System.setOut(System.err);
        AisCliMain.enableEmbedded();
        // 환경변수가 없으면 호출마다 앱이 열어 둔 vault를 읽는다. 시작할 때는 안내 글을 위해 한 번만 본다
        String vault = VaultPointer.resolve(System.getenv("AISURADA_VAULT"), System.getProperty("user.home"));
        System.err.println("aisurada MCP 서버 " + SERVER_VERSION + ": " + (write ? "읽기+쓰기" : "읽기 전용")
                + (vault == null || vault.isBlank() ? " (vault를 아직 못 정했다. 앱을 열어 vault를 열면 도구 호출이 그 vault를 쓴다)" : ""));
        try {
            new McpServer(write, AisCliMain::callEmbedded).serve(
                    new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8)), protocol);
        } catch (IOException e) {
            System.err.println("MCP 입출력이 끊겼다: " + e.getMessage());
            System.exit(1);
            return;
        }
        System.exit(0);
    }

    /** 한 줄에 메시지 하나씩 읽어 답을 쓰고, 입력이 닫히면 돌아옵니다. */
    void serve(BufferedReader in, PrintStream out) throws IOException {
        String line;
        while ((line = in.readLine()) != null) {
            if (line.isBlank()) {
                continue;
            }
            String response = handle(line);
            if (response != null) {
                out.print(response);
                out.print('\n');
                out.flush();
            }
        }
    }

    /** 메시지 한 줄을 처리해 답 한 줄을 돌려줍니다. 알림이라 답이 없으면 null입니다. */
    String handle(String line) {
        JsonValue parsed;
        try {
            parsed = JsonReader.parse(line);
        } catch (RuntimeException e) {
            return error("null", PARSE_ERROR, "JSON을 못 읽었다");
        }
        if (!(parsed instanceof JsonValue.JsonObject message)) {
            return error("null", INVALID_REQUEST, "요청은 JSON 객체여야 한다");
        }
        Map<String, JsonValue> fields = message.properties();
        JsonValue idValue = fields.get("id");
        boolean hasId = idValue != null && !(idValue instanceof JsonValue.JsonLiteral l && l.isNull());
        String id = hasId ? JsonCompact.compact(idValue) : "null";
        if (!(fields.get("method") instanceof JsonValue.JsonString methodValue)) {
            // method가 없는 것은 우리가 보낸 요청의 답이거나 깨진 메시지다. 답할 id가 있을 때만 거절한다.
            return hasId ? error(id, INVALID_REQUEST, "method가 없다") : null;
        }
        String method = methodValue.value();
        if (!hasId) {
            return null; // 알림(notifications/initialized 등)에는 답하지 않는다
        }
        try {
            return switch (method) {
                case "initialize" -> result(id, initializeResult(fields.get("params")));
                case "ping" -> result(id, "{}");
                case "tools/list" -> result(id, toolsListResult());
                case "tools/call" -> callTool(id, fields.get("params"));
                default -> error(id, METHOD_NOT_FOUND, "지원하지 않는 메서드다: " + method);
            };
        } catch (RuntimeException e) {
            return error(id, INTERNAL_ERROR, "서버 안에서 예외가 났다: " + e);
        }
    }

    /** initialize 요청의 clientInfo.name입니다. 어느 에이전트가 붙었는지 가릴 때 씁니다. */
    private volatile String clientName;

    private String initializeResult(JsonValue params) {
        if (params instanceof JsonValue.JsonObject p0 && p0.properties().get("clientInfo") instanceof JsonValue.JsonObject info
                && info.properties().get("name") instanceof JsonValue.JsonString nameValue) {
            clientName = nameValue.value();
        }
        String requested = null;
        if (params instanceof JsonValue.JsonObject p && p.properties().get("protocolVersion") instanceof JsonValue.JsonString v) {
            requested = v.value();
        }
        String version = requested != null && PROTOCOL_VERSIONS.contains(requested) ? requested : PROTOCOL_VERSIONS.get(0);
        StringBuilder sb = new StringBuilder("{\"protocolVersion\":");
        JsonCompact.appendString(sb, version);
        sb.append(",\"capabilities\":{\"tools\":{\"listChanged\":false}},\"serverInfo\":{\"name\":");
        JsonCompact.appendString(sb, SERVER_NAME);
        sb.append(",\"version\":");
        JsonCompact.appendString(sb, SERVER_VERSION);
        sb.append("},\"instructions\":");
        JsonCompact.appendString(sb, INSTRUCTIONS);
        return sb.append('}').toString();
    }

    private String toolsListResult() {
        StringBuilder sb = new StringBuilder("{\"tools\":[");
        boolean first = true;
        for (McpTools.Tool tool : McpTools.exposed(allowWrite)) {
            if (!first) {
                sb.append(',');
            }
            first = false;
            sb.append("{\"name\":");
            JsonCompact.appendString(sb, tool.name());
            sb.append(",\"description\":");
            JsonCompact.appendString(sb, tool.description());
            sb.append(",\"inputSchema\":").append(tool.inputSchema());
            sb.append(",\"annotations\":{\"readOnlyHint\":").append(!tool.write());
            if (tool.write()) {
                sb.append(",\"destructiveHint\":").append(tool.destructive());
            }
            sb.append("}}");
        }
        return sb.append("]}").toString();
    }

    private String callTool(String id, JsonValue params) {
        if (!(params instanceof JsonValue.JsonObject p) || !(p.properties().get("name") instanceof JsonValue.JsonString nameValue)) {
            return error(id, INVALID_PARAMS, "params.name이 필요하다");
        }
        String name = nameValue.value();
        McpTools.Tool tool = McpTools.find(name);
        if (tool == null) {
            return error(id, INVALID_PARAMS, "없는 도구다: " + name);
        }
        JsonValue arguments = p.properties().get("arguments");
        if (arguments == null || (arguments instanceof JsonValue.JsonLiteral l && l.isNull())) {
            arguments = new JsonValue.JsonObject(Map.of());
        }
        if (!(arguments instanceof JsonValue.JsonObject)) {
            return error(id, INVALID_PARAMS, "arguments는 객체여야 한다");
        }
        if (McpTools.NEVER_EXPOSED.contains(name)) {
            return result(id, toolResult("이 도구는 MCP로 열지 않는다. 영구 삭제는 앱에서 사람이 한다.", true));
        }
        if (!McpTools.isExposed(tool, allowWrite)) {
            return result(id, toolResult("이 서버는 읽기 전용으로 떠 있어 " + name + " 을 쓸 수 없다. "
                    + "쓰려면 서버를 `ais-cli mcp --write`로 다시 띄워야 한다.", true));
        }
        // Codex는 도구 호출마다 _meta에 세션 id를 싣는다. 환경변수로는 MCP 서버에 세션이 전해지지 않는다
        AisCliMain.Call call;
        SessionAgents.setCallOverride(SessionAgents.fromMcpRequest(clientName, metaSessionId(p.properties().get("_meta"))));
        try {
            call = caller.apply(new String[] {name, JsonCompact.compact(arguments)});
        } finally {
            SessionAgents.setCallOverride(null);
        }
        if (call.status() == 0) {
            String text = call.stdout().isEmpty() ? "(출력 없음)" : call.stdout();
            return result(id, toolResult(text, false));
        }
        StringBuilder text = new StringBuilder();
        text.append(call.stderr().isBlank() ? call.stdout() : call.stderr().strip());
        if (text.length() == 0) {
            text.append("(오류 글 없음)");
        }
        text.append("\n(종료 코드 ").append(call.status()).append(')');
        return result(id, toolResult(text.toString(), true));
    }

    /** 호출 요청의 _meta에서 세션 id를 꺼냅니다. sessionId를 먼저 보고 없으면 threadId를 봅니다. 없으면 null입니다. */
    private static String metaSessionId(JsonValue meta) {
        if (!(meta instanceof JsonValue.JsonObject m)) {
            return null;
        }
        for (String key : new String[] {"sessionId", "threadId"}) {
            if (m.properties().get(key) instanceof JsonValue.JsonString v && !v.value().isBlank()) {
                return v.value();
            }
        }
        return null;
    }

    private static String toolResult(String text, boolean isError) {
        StringBuilder sb = new StringBuilder("{\"content\":[{\"type\":\"text\",\"text\":");
        JsonCompact.appendString(sb, text);
        return sb.append("}],\"isError\":").append(isError).append('}').toString();
    }

    private static String result(String id, String resultJson) {
        return "{\"jsonrpc\":\"2.0\",\"id\":" + id + ",\"result\":" + resultJson + "}";
    }

    private static String error(String id, int code, String message) {
        StringBuilder sb = new StringBuilder("{\"jsonrpc\":\"2.0\",\"id\":").append(id).append(",\"error\":{\"code\":").append(code).append(",\"message\":");
        JsonCompact.appendString(sb, message);
        return sb.append("}}").toString();
    }
}
