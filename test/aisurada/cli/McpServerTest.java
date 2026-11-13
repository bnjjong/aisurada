package aisurada.cli;

import aisurada.args.ArgsParser;
import aisurada.args.JsonReader;
import aisurada.args.JsonValue;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.UUID;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.TimeUnit;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * {@code ais-cli mcp}의 시험. 세 층이다.
 * 1) MCP 스키마의 키가 엔진({@link ArgsParser})이 받는 키와 정확히 같은가
 * 2) 가짜 호출기로 프로토콜(협상, 목록, 호출, 오류, 읽기 전용)이 규격대로 도는가
 * 3) 진짜 엔진과 진짜 {@code mcp} 프로세스로 표준 입출력 왕복이 되는가(오류가 나도 서버가 안 죽는가 포함)
 */
public final class McpServerTest {

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

    // ---- JSON 읽기 도움 ----

    private static JsonValue parse(String s) {
        return JsonReader.parse(s);
    }

    private static JsonValue at(JsonValue v, Object... path) {
        JsonValue cur = v;
        for (Object key : path) {
            if (key instanceof String k && cur instanceof JsonValue.JsonObject o) {
                cur = o.properties().get(k);
            } else if (key instanceof Integer i && cur instanceof JsonValue.JsonArray a) {
                cur = a.elements().get(i);
            } else {
                return null;
            }
            if (cur == null) {
                return null;
            }
        }
        return cur;
    }

    private static String text(JsonValue v, Object... path) {
        JsonValue x = at(v, path);
        if (x instanceof JsonValue.JsonString s) {
            return s.value();
        }
        return x instanceof JsonValue.JsonLiteral l ? l.raw() : null;
    }

    private static Set<String> keys(JsonValue v) {
        return v instanceof JsonValue.JsonObject o ? new HashSet<>(o.properties().keySet()) : Set.of();
    }

    // ---- 1) 스키마가 엔진과 같은가 ----

    private static final Pattern ALLOWED = Pattern.compile("쓸 수 있는 것은 \\[(.*)\\] 뿐이다");
    private static final Pattern REQUIRED = Pattern.compile("\\): (\\S+) 가 필요하다");

    private static void schemasMatchEngine() {
        eq(McpTools.all().stream().map(McpTools.Tool::name).toList(), AisCliMain.CLI_TOOL_NAMES, "도구 이름과 차례가 엔진 도구 36개와 같다");
        for (McpTools.Tool tool : McpTools.all()) {
            JsonValue schema = parse(tool.inputSchema());
            eq(text(schema, "type"), "object", tool.name() + " 스키마는 객체");
            eq(text(schema, "additionalProperties"), "false", tool.name() + " 모르는 키를 막는다");
            Set<String> props = keys(at(schema, "properties"));

            // 엔진이 모르는 키를 거절할 때 허용 키를 실어 주는 문구로, 스키마의 키와 정확히 같은지 잰다
            String message = null;
            try {
                ArgsParser.parse(tool.name(), List.of("{\"__nope\":1}"));
            } catch (AisException e) {
                message = e.getMessage();
            }
            check(message != null, tool.name() + " 모르는 키를 엔진이 거절한다");
            Matcher allowed = ALLOWED.matcher(message);
            check(allowed.find(), tool.name() + " 거절 문구에 허용 키가 있다: " + message);
            Set<String> engineKeys = new HashSet<>();
            for (String k : allowed.group(1).split(",")) {
                if (!k.isBlank()) {
                    engineKeys.add(k.strip());
                }
            }
            eq(props, engineKeys, tool.name() + " 스키마 키 = 엔진 허용 키");

            // 필수 키: 스키마의 required가 곧 엔진이 요구하는 키여야 한다
            Set<String> required = new HashSet<>();
            if (at(schema, "required") instanceof JsonValue.JsonArray arr) {
                arr.elements().forEach(e -> required.add(((JsonValue.JsonString) e).value()));
            }
            check(props.containsAll(required), tool.name() + " required는 properties 안에 있다");
            for (String key : required) {
                eq(at(schema, "properties", key, "type") instanceof JsonValue.JsonString s ? s.value() : null, "string",
                        tool.name() + "." + key + " 필수 칸은 문자열");
                StringBuilder others = new StringBuilder("{");
                for (String other : required) {
                    if (!other.equals(key)) {
                        others.append(others.length() > 1 ? "," : "").append('"').append(other).append("\":\"x\"");
                    }
                }
                others.append('}');
                String failure = null;
                try {
                    ArgsParser.parse(tool.name(), List.of(others.toString()));
                } catch (AisException e) {
                    failure = e.getMessage();
                }
                check(failure != null && failure.contains(key + " 가 필요하다"), tool.name() + " 에서 " + key + " 를 빼면 엔진이 요구한다: " + failure);
            }
            // 필수만 다 주면 엔진이 받아들인다(스키마에 없는 필수 키가 엔진에 없다)
            StringBuilder full = new StringBuilder("{");
            for (String key : required) {
                full.append(full.length() > 1 ? "," : "").append('"').append(key).append("\":\"x\"");
            }
            full.append('}');
            try {
                ArgsParser.parse(tool.name(), List.of(full.toString()));
            } catch (AisException e) {
                Matcher m = REQUIRED.matcher(e.getMessage());
                check(!m.find(), tool.name() + " 스키마에 없는 필수 키를 엔진이 요구한다: " + e.getMessage());
            }
        }
    }

    private static void exposureRules() {
        List<String> reads = McpTools.exposed(false).stream().map(McpTools.Tool::name).toList();
        eq(reads, List.of("list_docs", "search_docs", "list_tags", "get_doc", "list_trash",
                "list_templates", "get_template", "list_sub_notes", "get_sub_note", "list_attachments", "list_comments",
                "list_resources", "get_resource", "usable_resources", "list_resource_versions", "list_tasks", "get_task"),
                "기본은 읽기 도구 17개만");
        List<String> all = McpTools.exposed(true).stream().map(McpTools.Tool::name).toList();
        eq(all.size(), 35, "쓰기를 켜면 영구 삭제만 빼고 35개");
        check(!all.contains("purge_doc") && !reads.contains("purge_doc"), "purge_doc은 어느 모드에서도 안 내놓는다");
        for (McpTools.Tool tool : McpTools.all()) {
            eq(tool.write(), aisurada.cmd.CommandRegistry.WRITE_COMMANDS.contains(tool.name().replace('_', '-')),
                    tool.name() + " 쓰기 구분은 엔진의 쓰기 깃발에서 나온다");
        }
    }

    // ---- 2) 가짜 호출기로 프로토콜 ----

    private static final class FakeCaller implements java.util.function.Function<String[], AisCliMain.Call> {
        final List<List<String>> calls = new ArrayList<>();
        AisCliMain.Call next = new AisCliMain.Call(0, "{\"ok\":true}\n", "");

        @Override
        public AisCliMain.Call apply(String[] argv) {
            calls.add(List.of(argv));
            return next;
        }
    }

    private static JsonValue send(McpServer server, String line) {
        String reply = server.handle(line);
        check(reply != null, "답이 있어야 한다: " + line);
        check(!reply.contains("\n") && !reply.contains("\r"), "답은 한 줄이다");
        JsonValue v = parse(reply);
        eq(text(v, "jsonrpc"), "2.0", "jsonrpc 2.0");
        return v;
    }

    private static void protocolWithFakeCaller() {
        FakeCaller fake = new FakeCaller();
        McpServer readOnly = new McpServer(false, fake);
        McpServer writable = new McpServer(true, fake);

        // 협상: 아는 개정은 그대로, 모르는 개정은 우리 최신으로 답한다
        for (String v : McpServer.PROTOCOL_VERSIONS) {
            JsonValue r = send(readOnly, "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"" + v + "\"}}");
            eq(text(r, "result", "protocolVersion"), v, "아는 개정은 그대로 답한다 " + v);
        }
        JsonValue unknown = send(readOnly, "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2099-01-01\"}}");
        eq(text(unknown, "result", "protocolVersion"), McpServer.PROTOCOL_VERSIONS.get(0), "모르는 개정에는 최신으로 답한다");
        eq(text(unknown, "result", "serverInfo", "name"), "aisurada", "서버 이름");
        check(at(unknown, "result", "capabilities", "tools") != null, "tools 기능을 알린다");
        check(at(unknown, "result", "capabilities", "resources") == null, "안 하는 기능은 알리지 않는다");
        // 엔진은 마크다운 노트를 고칠 때 f1을 꼭 요구한다. 선택처럼 읽히는 안내는 모델이 한 번 실패한 뒤에야 알게 만든다.
        String instructions = text(unknown, "result", "instructions");
        check(instructions != null && instructions.contains("f1이 꼭 필요"), "접속 안내가 f1이 꼭 필요하다고 말한다");
        for (String name : List.of("update_doc", "set_favorite", "evaluate_doc", "link_docs", "update_sub_note")) {
            check(McpTools.find(name).inputSchema().contains("꼭 필요한 쓰기 전 토큰"), name + "의 f1 설명이 꼭 필요하다고 말한다");
        }

        // 알림에는 답이 없다
        check(readOnly.handle("{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}") == null, "알림에는 답하지 않는다");
        check(readOnly.handle("{\"jsonrpc\":\"2.0\",\"method\":\"notifications/cancelled\",\"params\":{}}") == null, "다른 알림도 답하지 않는다");

        // id는 숫자든 문자열이든 그대로 돌려준다
        eq(text(send(readOnly, "{\"jsonrpc\":\"2.0\",\"id\":42,\"method\":\"ping\"}"), "id"), "42", "숫자 id");
        eq(text(send(readOnly, "{\"jsonrpc\":\"2.0\",\"id\":\"a-1\",\"method\":\"ping\"}"), "id"), "a-1", "문자열 id");
        check(at(send(readOnly, "{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}"), "result") instanceof JsonValue.JsonObject, "ping은 빈 객체");

        // 목록
        JsonValue readTools = send(readOnly, "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}");
        eq(((JsonValue.JsonArray) at(readTools, "result", "tools")).elements().size(), 17, "읽기 모드 목록은 17개");
        for (JsonValue t : ((JsonValue.JsonArray) at(readTools, "result", "tools")).elements()) {
            eq(text(t, "annotations", "readOnlyHint"), "true", text(t, "name") + " 은 읽기 힌트");
            check(at(t, "inputSchema", "properties") != null, text(t, "name") + " 에 inputSchema");
        }
        JsonValue writeTools = send(writable, "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}");
        eq(((JsonValue.JsonArray) at(writeTools, "result", "tools")).elements().size(), 35, "쓰기 모드 목록은 35개");
        for (JsonValue t : ((JsonValue.JsonArray) at(writeTools, "result", "tools")).elements()) {
            String name = text(t, "name");
            if ("delete_doc".equals(name) || "update_doc".equals(name)) {
                eq(text(t, "annotations", "destructiveHint"), "true", name + " 은 파괴 힌트");
            }
            if ("create_doc".equals(name)) {
                eq(text(t, "annotations", "destructiveHint"), "false", name + " 은 파괴 아님");
            }
        }

        // 호출: 도구 이름과 압축한 인자 JSON이 엔진 길로 간다
        JsonValue ok = send(readOnly, "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/call\",\"params\":{\"name\":\"get_doc\",\"arguments\":{ \"id\" : \"nd-1\" }}}");
        eq(fake.calls.get(0), List.of("get_doc", "{\"id\":\"nd-1\"}"), "호출 인자");
        eq(text(ok, "result", "isError"), "false", "성공은 isError false");
        eq(text(ok, "result", "content", 0, "type"), "text", "본문은 text");
        eq(text(ok, "result", "content", 0, "text"), "{\"ok\":true}\n", "엔진 출력이 그대로 간다");
        send(readOnly, "{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"tools/call\",\"params\":{\"name\":\"list_tags\"}}");
        eq(fake.calls.get(1), List.of("list_tags", "{}"), "arguments가 없으면 빈 객체");
        send(readOnly, "{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"tools/call\",\"params\":{\"name\":\"list_tags\",\"arguments\":null}}");
        eq(fake.calls.get(2), List.of("list_tags", "{}"), "arguments가 null이어도 빈 객체");

        // 한글과 개행이 든 인자도 한 줄 JSON으로 간다
        send(writable, "{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"tools/call\",\"params\":{\"name\":\"create_doc\",\"arguments\":{\"title\":\"한글\\n제목\"}}}");
        eq(fake.calls.get(3), List.of("create_doc", "{\"title\":\"한글\\n제목\"}"), "한글과 개행을 보존한다");

        // 엔진 오류는 isError와 함께 오류 글과 종료 코드가 간다
        fake.next = new AisCliMain.Call(2, "", "AIS-400 인자 검증 실패(get_doc): id 가 필요하다.\n");
        JsonValue failed = send(readOnly, "{\"jsonrpc\":\"2.0\",\"id\":6,\"method\":\"tools/call\",\"params\":{\"name\":\"get_doc\",\"arguments\":{}}}");
        eq(text(failed, "result", "isError"), "true", "엔진 오류는 isError");
        check(text(failed, "result", "content", 0, "text").contains("AIS-400") && text(failed, "result", "content", 0, "text").contains("종료 코드 2"), "오류 글과 종료 코드");
        fake.next = new AisCliMain.Call(0, "", "");
        eq(text(send(readOnly, "{\"jsonrpc\":\"2.0\",\"id\":6,\"method\":\"tools/call\",\"params\":{\"name\":\"get_doc\",\"arguments\":{\"id\":\"x\"}}}"), "result", "content", 0, "text"),
                "(출력 없음)", "출력이 없어도 text가 비지 않는다");

        // 읽기 전용에서는 쓰기 도구를 엔진에 보내지 않는다. 영구 삭제는 쓰기 모드에서도 안 보낸다
        int before = fake.calls.size();
        JsonValue refused = send(readOnly, "{\"jsonrpc\":\"2.0\",\"id\":8,\"method\":\"tools/call\",\"params\":{\"name\":\"create_doc\",\"arguments\":{}}}");
        eq(text(refused, "result", "isError"), "true", "읽기 전용은 쓰기 도구를 거절");
        check(text(refused, "result", "content", 0, "text").contains("--write"), "다시 띄우는 법을 알려 준다");
        JsonValue purge = send(writable, "{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"tools/call\",\"params\":{\"name\":\"purge_doc\",\"arguments\":{\"trashId\":\"x\"}}}");
        eq(text(purge, "result", "isError"), "true", "purge_doc은 쓰기 모드에서도 거절");
        eq(fake.calls.size(), before, "거절한 호출은 엔진에 가지 않는다");

        // 규격 오류
        eq(text(send(readOnly, "{\"jsonrpc\":\"2.0\",\"id\":10,\"method\":\"tools/call\",\"params\":{\"name\":\"nope\"}}"), "error", "code"), "-32602", "없는 도구는 -32602");
        eq(text(send(readOnly, "{\"jsonrpc\":\"2.0\",\"id\":11,\"method\":\"tools/call\",\"params\":{}}"), "error", "code"), "-32602", "이름이 없으면 -32602");
        eq(text(send(readOnly, "{\"jsonrpc\":\"2.0\",\"id\":12,\"method\":\"tools/call\",\"params\":{\"name\":\"get_doc\",\"arguments\":[1]}}"), "error", "code"), "-32602", "arguments가 배열이면 -32602");
        eq(text(send(readOnly, "{\"jsonrpc\":\"2.0\",\"id\":13,\"method\":\"resources/list\"}"), "error", "code"), "-32601", "안 하는 메서드는 -32601");
        eq(text(send(readOnly, "not json"), "error", "code"), "-32700", "깨진 JSON은 -32700");
        eq(text(send(readOnly, "[1,2]"), "error", "code"), "-32600", "배열은 -32600");
        eq(fake.calls.size(), before, "규격 오류도 엔진에 가지 않는다");
    }

    // ---- 3) 진짜 엔진과 진짜 mcp 프로세스 ----

    private static final class Mcp implements AutoCloseable {
        final Process process;
        final BufferedReader out;
        final OutputStream in;
        int nextId = 1;

        Mcp(List<String> command, Map<String, String> env, Path errFile) throws IOException {
            ProcessBuilder builder = new ProcessBuilder(command);
            builder.environment().clear();
            builder.environment().putAll(env);
            builder.redirectError(errFile.toFile());
            process = builder.start();
            out = new BufferedReader(new InputStreamReader(process.getInputStream(), StandardCharsets.UTF_8));
            in = process.getOutputStream();
        }

        JsonValue request(String method, String paramsJson) throws Exception {
            int id = nextId++;
            String line = "{\"jsonrpc\":\"2.0\",\"id\":" + id + ",\"method\":\"" + method + "\"" + (paramsJson == null ? "" : ",\"params\":" + paramsJson) + "}";
            in.write((line + "\n").getBytes(StandardCharsets.UTF_8));
            in.flush();
            String reply = CompletableFuture.supplyAsync(() -> {
                try {
                    return out.readLine();
                } catch (IOException e) {
                    return null;
                }
            }).get(30, TimeUnit.SECONDS);
            check(reply != null, "프로세스가 답해야 한다: " + method);
            JsonValue v = parse(reply);
            eq(text(v, "jsonrpc"), "2.0", "표준 출력의 모든 줄은 JSON-RPC다: " + reply);
            eq(text(v, "id"), Integer.toString(id), "id가 맞다");
            return v;
        }

        JsonValue call(String tool, String argumentsJson) throws Exception {
            return request("tools/call", "{\"name\":\"" + tool + "\",\"arguments\":" + argumentsJson + "}");
        }

        String resultText(JsonValue reply) {
            return text(reply, "result", "content", 0, "text");
        }

        boolean isError(JsonValue reply) {
            return "true".equals(text(reply, "result", "isError"));
        }

        @Override
        public void close() throws Exception {
            try {
                in.close();
            } catch (IOException ignored) {
                // 이미 끝난 프로세스
            }
            if (!process.waitFor(15, TimeUnit.SECONDS)) {
                process.destroyForcibly().waitFor();
                throw new AssertionError("표준 입력을 닫아도 mcp 프로세스가 끝나지 않았다");
            }
        }
    }

    /**
     * Codex는 도구 호출마다 params._meta에 세션 id를 싣고, 환경변수로는 MCP 서버에 세션을 알려 주지 않는다.
     * 그 값은 initialize에서 codex 클라이언트라고 밝힌 연결의 호출에만 세션 출처가 된다.
     */
    private static void sessionFromMeta() {
        List<aisurada.session.SessionAgents.Source> seen = new ArrayList<>();
        McpServer codex = new McpServer(true, args -> {
            seen.add(aisurada.session.SessionAgents.callOverride());
            return new AisCliMain.Call(0, "{}", "");
        });
        String withMeta = "{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"tools/call\",\"params\":{\"name\":\"list_docs\",\"arguments\":{},\"_meta\":{\"sessionId\":\"t-1\",\"threadId\":\"t-2\"}}}";
        String withoutMeta = "{\"jsonrpc\":\"2.0\",\"id\":8,\"method\":\"tools/call\",\"params\":{\"name\":\"list_docs\",\"arguments\":{}}}";
        send(codex, withMeta);
        eq(seen.get(0), null, "initialize 전에는 누구인지 몰라 _meta를 믿지 않는다");
        send(codex, "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\",\"clientInfo\":{\"name\":\"codex-mcp-client\"}}}");
        send(codex, withMeta);
        eq(seen.get(1), new aisurada.session.SessionAgents.Source("codex", "t-1"), "codex 클라이언트의 _meta.sessionId가 출처가 된다");
        eq(aisurada.session.SessionAgents.callOverride(), null, "호출이 끝나면 걸린 출처를 지운다");
        send(codex, withoutMeta);
        eq(seen.get(2), null, "_meta가 없으면 출처를 정하지 않는다");

        List<aisurada.session.SessionAgents.Source> otherSeen = new ArrayList<>();
        McpServer other = new McpServer(true, args -> {
            otherSeen.add(aisurada.session.SessionAgents.callOverride());
            return new AisCliMain.Call(0, "{}", "");
        });
        send(other, "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-06-18\",\"clientInfo\":{\"name\":\"claude-code\"}}}");
        send(other, withMeta);
        eq(otherSeen.get(0), null, "다른 클라이언트의 _meta는 믿지 않는다");
    }

    private static void realEngineRoundTrip() throws Exception {
        Path temp = Files.createTempDirectory("aisurada-mcp-");
        Path home = Files.createDirectory(temp.resolve("home"));
        Path vault = Files.createDirectory(temp.resolve("vault"));
        Files.createDirectories(vault.resolve(".aisurada/agendas"));
        Files.writeString(vault.resolve(".aisurada/workspace.json"), "{}\n");
        Files.writeString(vault.resolve(".aisurada/agendas/demo.md"), "---\nid: agfixture-00000001\nslug: demo\ntitle: 데모\n---\n");
        Path socket = Path.of("/tmp", "ais-mcp-" + UUID.randomUUID().toString().substring(0, 8) + ".sock");
        String java = Path.of(System.getProperty("java.home"), "bin/java").toString();
        String engineJar = Path.of("dist/ais-engine.jar").toAbsolutePath().toString();
        String cliJar = Path.of("dist/ais-cli.jar").toAbsolutePath().toString();
        Map<String, String> env = Map.of("HOME", home.toString(), "PATH", "/usr/bin:/bin", "AISURADA_VAULT", vault.toString(),
                "AISURADA_SIDECAR_SOCKET", socket.toString());

        ProcessBuilder serverBuilder = new ProcessBuilder(java, "-Duser.home=" + home, "-jar", engineJar, "serve");
        serverBuilder.environment().clear();
        serverBuilder.environment().putAll(env);
        serverBuilder.redirectOutput(home.resolve("server.out").toFile());
        serverBuilder.redirectError(home.resolve("server.err").toFile());
        Process server = serverBuilder.start();
        try {
            long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(20);
            while (!Files.exists(socket) && server.isAlive() && System.nanoTime() < deadline) {
                Thread.sleep(20);
            }
            check(Files.exists(socket), "엔진이 떴다: " + Files.readString(home.resolve("server.err")));

            // 쓰기 모드
            try (Mcp mcp = new Mcp(List.of(java, "-Duser.home=" + home, "-jar", cliJar, "mcp", "--write"), env, home.resolve("mcp-write.err"))) {
                JsonValue init = mcp.request("initialize", "{\"protocolVersion\":\"2025-06-18\",\"capabilities\":{},\"clientInfo\":{\"name\":\"test\",\"version\":\"1\"}}");
                eq(text(init, "result", "serverInfo", "name"), "aisurada", "initialize");
                in(mcp, "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}");
                eq(((JsonValue.JsonArray) at(mcp.request("tools/list", null), "result", "tools")).elements().size(), 35, "쓰기 모드 목록");

                JsonValue created = mcp.call("create_doc", "{\"agenda\":\"demo\",\"type\":\"free\",\"title\":\"한글 제목\",\"body\":\"첫 본문\\n두 번째 줄\",\"tags\":[\"mcp\"]}");
                check(!mcp.isError(created), "create_doc 성공: " + mcp.resultText(created));
                JsonValue doc = parse(mcp.resultText(created));
                String id = text(doc, "id");
                check(id != null && !id.isEmpty(), "id가 왔다");

                JsonValue got = mcp.call("get_doc", "{\"id\":\"" + id + "\"}");
                check(!mcp.isError(got), "get_doc 성공");
                check(mcp.resultText(got).contains("두 번째 줄"), "본문을 읽었다");
                String precondition = text(parse(mcp.resultText(got)), "precondition");
                check(precondition != null && precondition.startsWith("f1:"), "쓰기 전 토큰이 왔다: " + precondition);

                JsonValue updated = mcp.call("update_doc", "{\"id\":\"" + id + "\",\"body\":\"고친 본문\",\"f1\":\"" + precondition + "\"}");
                check(!mcp.isError(updated), "update_doc 성공: " + mcp.resultText(updated));
                JsonValue stale = mcp.call("update_doc", "{\"id\":\"" + id + "\",\"body\":\"낡은 토큰으로 덮기\",\"f1\":\"" + precondition + "\"}");
                check(mcp.isError(stale), "낡은 토큰으로는 덮어쓰지 못한다: " + mcp.resultText(stale));
                JsonValue noToken = mcp.call("update_doc", "{\"id\":\"" + id + "\",\"body\":\"토큰 없이 덮기\"}");
                check(mcp.isError(noToken) && mcp.resultText(noToken).contains("AIS-412"), "f1 없이는 고치지 못한다: " + mcp.resultText(noToken));
                String reread = mcp.resultText(mcp.call("get_doc", "{\"id\":\"" + id + "\"}"));
                check(reread.contains("고친 본문") && !reread.contains("두 번째 줄") && !reread.contains("낡은 토큰") && !reread.contains("토큰 없이"),
                        "본문은 통째로 바뀌었고 낡은 쓰기와 토큰 없는 쓰기는 안 먹었다");

                check(mcp.resultText(mcp.call("list_docs", "{}")).contains(id), "list_docs에 보인다");
                check(mcp.resultText(mcp.call("search_docs", "{\"query\":\"고친\"}")).contains(id), "search_docs로 찾는다");
                check(mcp.resultText(mcp.call("list_tags", "{}")).contains("mcp"), "list_tags");

                // 오류는 서버를 죽이지 않고 isError로 온다
                JsonValue bad = mcp.call("get_doc", "{}");
                check(mcp.isError(bad) && mcp.resultText(bad).contains("AIS-400"), "엔진 오류는 isError: " + mcp.resultText(bad));
                JsonValue unknownKey = mcp.call("get_doc", "{\"id\":\"x\",\"zzz\":1}");
                check(mcp.isError(unknownKey), "스키마 밖의 키는 엔진이 거절한다");

                // 휴지통 왕복, 영구 삭제는 막힘
                check(!mcp.isError(mcp.call("delete_doc", "{\"id\":\"" + id + "\"}")), "delete_doc");
                String trash = mcp.resultText(mcp.call("list_trash", "{}"));
                String trashId = text(((JsonValue.JsonArray) parse(trash)).elements().get(0), "trashId");
                check(trashId != null, "휴지통 항목: " + trash);
                check(mcp.isError(mcp.call("purge_doc", "{\"trashId\":\"" + trashId + "\"}")), "purge_doc은 거절");
                check(mcp.resultText(mcp.call("list_trash", "{}")).contains(trashId), "거절된 뒤에도 휴지통에 남아 있다");
                check(!mcp.isError(mcp.call("restore_doc", "{\"trashId\":\"" + trashId + "\"}")), "restore_doc");
                check(mcp.resultText(mcp.call("list_docs", "{}")).contains(id), "되살아났다");
                check(at(mcp.request("ping", null), "result") instanceof JsonValue.JsonObject, "마지막까지 서버가 살아 있다");
            }
            // 스킬·에이전트 자원 도구(확장): 만들고, f1 로 지키며 고치고, 참조하고, 규칙 4를 지키고, 지우고 되살린다
            Files.createDirectories(vault.resolve(".aisurada/projects"));
            Files.writeString(vault.resolve(".aisurada/projects/앱.md"), "---\nid: pjmwmcpres1-0000abcd\nslug: 앱\ntitle: 앱\nhue: blue\n---\n");
            String initParams = "{\"protocolVersion\":\"2025-06-18\",\"capabilities\":{},\"clientInfo\":{\"name\":\"test\",\"version\":\"1\"}}";
            try (Mcp mcp = new Mcp(List.of(java, "-Duser.home=" + home, "-jar", cliJar, "mcp", "--write"), env, home.resolve("mcp-res.err"))) {
                mcp.request("initialize", initParams);
                in(mcp, "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}");
                JsonValue skill = mcp.call("save_resource", "{\"kind\":\"skill\",\"scope\":\"global\",\"name\":\"번역 검수\",\"description\":\"번역을 검수한다\",\"body\":\"# 지침\","
                        + "\"tools\":{\"claude\":{\"allowed-tools\":[\"Read\"]}}}");
                check(!mcp.isError(skill), "save_resource(스킬): " + mcp.resultText(skill));
                String sid = text(parse(mcp.resultText(skill)), "id");
                check(sid != null && sid.startsWith("sk"), "스킬 id 는 sk 머리: " + sid);
                check(mcp.resultText(skill).contains("allowed-tools"), "도구 칸이 응답에 있다");
                JsonValue agent = mcp.call("save_resource", "{\"kind\":\"agent\",\"scope\":\"global\",\"name\":\"코드 리뷰어\",\"description\":\"리뷰한다\",\"permission\":\"edit\",\"skills\":[\"" + sid + "\"]}");
                check(!mcp.isError(agent), "save_resource(에이전트): " + mcp.resultText(agent));
                String aid = text(parse(mcp.resultText(agent)), "id");
                check(aid != null && aid.startsWith("ap") && mcp.resultText(agent).contains(sid), "에이전트 id 와 연결된 스킬");
                JsonValue asString = mcp.call("save_resource", "{\"kind\":\"agent\",\"scope\":\"global\",\"name\":\"문자열로 준 스킬\",\"description\":\"d\",\"skills\":\"[\\\"" + sid + "\\\"]\"}");
                check(!mcp.isError(asString), "skills 를 JSON 글로 줘도 받는다: " + mcp.resultText(asString));

                // f1 보호: 읽고 고치면 되고, 낡은 토큰과 토큰 없음은 AIS-412 로 거절된다
                String tok = text(parse(mcp.resultText(mcp.call("get_resource", "{\"id\":\"" + sid + "\"}"))), "precondition");
                check(tok != null && tok.startsWith("f1:"), "get_resource 가 쓰기 전 토큰을 준다");
                JsonValue edited = mcp.call("save_resource", "{\"id\":\"" + sid + "\",\"f1\":\"" + tok + "\",\"description\":\"고친 설명\"}");
                check(!mcp.isError(edited) && mcp.resultText(edited).contains("고친 설명"), "f1 로 고친다: " + mcp.resultText(edited));
                JsonValue stale = mcp.call("save_resource", "{\"id\":\"" + sid + "\",\"f1\":\"" + tok + "\",\"description\":\"낡은 토큰으로 덮기\"}");
                check(mcp.isError(stale) && mcp.resultText(stale).contains("AIS-412"), "낡은 토큰은 거절: " + mcp.resultText(stale));
                JsonValue noTok = mcp.call("save_resource", "{\"id\":\"" + sid + "\",\"description\":\"토큰 없이 덮기\"}");
                check(mcp.isError(noTok) && mcp.resultText(noTok).contains("AIS-412"), "토큰 없이는 거절: " + mcp.resultText(noTok));
                String newTok = text(parse(mcp.resultText(edited)), "precondition");
                JsonValue kindSwap = mcp.call("save_resource", "{\"id\":\"" + sid + "\",\"f1\":\"" + newTok + "\",\"kind\":\"agent\"}");
                check(mcp.isError(kindSwap) && mcp.resultText(kindSwap).contains("종류는 바꿀 수 없다"), "종류는 못 바꾼다");
                check(mcp.resultText(mcp.call("list_resource_versions", "{\"id\":\"" + sid + "\"}")).contains("version"), "고치기 전 판이 이력에 남는다");

                // 규칙 4: 참조하기 전에는 쓸 수 있는 자원에 안 나오고, 참조(withSkills)하면 에이전트와 스킬이 함께 나온다
                JsonValue before = mcp.call("usable_resources", "{\"project\":\"앱\"}");
                check(!mcp.isError(before) && ((JsonValue.JsonArray) at(parse(mcp.resultText(before)), "skills")).elements().isEmpty(), "참조 전에는 비었다: " + mcp.resultText(before));
                check(mcp.resultText(mcp.call("list_resources", "{\"scope\":\"global\"}")).contains(sid), "관리 목록에는 참조하지 않은 글로벌도 나온다");
                JsonValue ref = mcp.call("set_resource_ref", "{\"project\":\"앱\",\"id\":\"" + aid + "\",\"withSkills\":true}");
                check(!mcp.isError(ref) && mcp.resultText(ref).contains(aid) && mcp.resultText(ref).contains(sid), "set_resource_ref withSkills: " + mcp.resultText(ref));
                JsonValue after = parse(mcp.resultText(mcp.call("usable_resources", "{\"project\":\"앱\"}")));
                eq(((JsonValue.JsonArray) at(after, "skills")).elements().size(), 1, "참조한 스킬이 쓸 수 있게 됐다");
                eq(((JsonValue.JsonArray) at(after, "agents")).elements().size(), 1, "참조한 에이전트가 쓸 수 있게 됐다");
                check(mcp.isError(mcp.call("set_resource_ref", "{\"project\":\"앱\",\"id\":\"" + aid + "\",\"withSkills\":\"yes\"}")), "withSkills 는 true/false 만");
                check(mcp.isError(mcp.call("get_resource", "{\"id\":\"" + sid + "\",\"zzz\":1}")), "스키마 밖의 키는 거절");

                // 지우기와 되살리기: 지우면 깨진 참조, 되살리면 이어진다
                JsonValue del = mcp.call("delete_resource", "{\"id\":\"" + sid + "\"}");
                check(!mcp.isError(del) && mcp.resultText(del).contains("referencedBy"), "delete_resource: " + mcp.resultText(del));
                check(mcp.resultText(mcp.call("usable_resources", "{\"project\":\"앱\"}")).contains("broken"), "지우면 깨진 참조가 보인다");
                check(mcp.resultText(mcp.call("list_resources", "{\"trash\":true}")).contains(sid), "휴지통 목록");
                check(!mcp.isError(mcp.call("restore_resource", "{\"id\":\"" + sid + "\"}")), "restore_resource");
                check(((JsonValue.JsonArray) at(parse(mcp.resultText(mcp.call("usable_resources", "{\"project\":\"앱\"}"))), "broken")).elements().isEmpty(), "되살리면 이어진다");
                check(at(mcp.request("ping", null), "result") instanceof JsonValue.JsonObject, "자원 도구를 쓴 뒤에도 서버가 살아 있다");
            }
            // AI 세션이 쓰면 누가 고쳤는지 남는다
            Map<String, String> sessionEnv = new java.util.HashMap<>(env);
            sessionEnv.put("AISURADA_SESSION_ID", "res-session-7");
            sessionEnv.put("AISURADA_SESSION_AGENT", "codex");
            try (Mcp mcp = new Mcp(List.of(java, "-Duser.home=" + home, "-jar", cliJar, "mcp", "--write"), sessionEnv, home.resolve("mcp-res2.err"))) {
                mcp.request("initialize", initParams);
                in(mcp, "{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}");
                JsonValue stamped = mcp.call("save_resource", "{\"kind\":\"skill\",\"scope\":\"global\",\"name\":\"세션 기록\",\"description\":\"d\"}");
                check(!mcp.isError(stamped), "세션이 있어도 저장된다: " + mcp.resultText(stamped));
                eq(text(parse(mcp.resultText(stamped)), "editedSession"), "res-session-7", "AI 세션 ID 가 남는다");
                eq(text(parse(mcp.resultText(stamped)), "editedAgent"), "codex", "CLI 이름이 남는다");
            }
            check(Files.readString(home.resolve("mcp-write.err")).contains("읽기+쓰기"), "시작할 때 모드를 표준 오류로 알린다");

            // 읽기 전용 모드: 쓰기 도구는 엔진에 닿지 않는다
            try (Mcp mcp = new Mcp(List.of(java, "-Duser.home=" + home, "-jar", cliJar, "mcp"), env, home.resolve("mcp-read.err"))) {
                eq(((JsonValue.JsonArray) at(mcp.request("tools/list", null), "result", "tools")).elements().size(), 17, "읽기 모드 목록");
                JsonValue refused = mcp.call("create_doc", "{\"agenda\":\"demo\",\"type\":\"free\",\"title\":\"막혀야 함\",\"body\":\"x\"}");
                check(mcp.isError(refused), "읽기 전용은 create_doc을 거절");
                check(!mcp.isError(mcp.call("list_resources", "{}")), "읽기 전용에서도 list_resources는 된다");
                check(mcp.isError(mcp.call("save_resource", "{\"kind\":\"skill\",\"scope\":\"global\",\"name\":\"막혀야 함\",\"description\":\"d\"}")), "읽기 전용은 save_resource를 거절");
                check(mcp.isError(mcp.call("delete_resource", "{\"id\":\"skmwzzzzzz-00000000\"}")), "읽기 전용은 delete_resource를 거절");
                check(!mcp.resultText(mcp.call("list_docs", "{}")).contains("막혀야 함"), "거절된 노트는 안 생겼다");
            }

            // vault가 없는 환경: 도구 호출은 isError로 실패하고 서버는 살아 있다(종료 호출이 프로세스를 끝내지 않는다)
            Map<String, String> noVault = Map.of("HOME", home.toString(), "PATH", "/usr/bin:/bin");
            try (Mcp mcp = new Mcp(List.of(java, "-Duser.home=" + home, "-jar", cliJar, "mcp"), noVault, home.resolve("mcp-novault.err"))) {
                JsonValue failed = mcp.call("list_docs", "{}");
                check(mcp.isError(failed) && mcp.resultText(failed).contains("AISURADA_VAULT"), "vault가 없다는 오류가 온다: " + mcp.resultText(failed));
                check(at(mcp.request("ping", null), "result") != null, "오류 뒤에도 서버가 산다");
            }
        } finally {
            if (server.isAlive()) {
                server.destroy();
                if (!server.waitFor(15, TimeUnit.SECONDS)) {
                    server.destroyForcibly().waitFor();
                }
            }
            Files.deleteIfExists(socket);
        }
    }

    private static void in(Mcp mcp, String line) throws IOException {
        mcp.in.write((line + "\n").getBytes(StandardCharsets.UTF_8));
        mcp.in.flush();
    }

    public static void main(String[] args) throws Exception {
        schemasMatchEngine();
        exposureRules();
        protocolWithFakeCaller();
        sessionFromMeta();
        realEngineRoundTrip();
        System.out.println("McpServerTest: " + checks + "건 통과");
    }
}
