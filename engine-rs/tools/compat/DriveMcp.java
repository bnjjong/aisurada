package aisurada.cli;

import aisurada.session.SessionAgents;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;

/**
 * Java MCP 서버의 대화를 시험용 호출기로 돌려 본다. 첫 인자는 JSON-RPC 메시지가 한 줄에 하나씩 든 파일, 둘째 인자는 `--write`(선택)다.
 * 도구 호출은 엔진 대신 아래의 가짜 호출기가 받는다(인자와 세션 출처를 되돌려 주거나, 인자의 낱말에 따라 정해진 실패를 낸다).
 * Rust `cli_compat`의 `mcp_compat` 예제가 같은 가짜 호출기로 같은 대화를 돌려 한 줄씩 견준다.
 * 출력은 메시지마다 `<번호> <답 한 줄>`이고, 답이 없는 알림은 `<번호> <none>`이다.
 */
public class DriveMcp {
    static AisCliMain.Call fake(String[] args) {
        String arguments = args[1];
        SessionAgents.Source session = SessionAgents.callOverride();
        String label = session == null ? "-" : session.agent() + "/" + session.sessionId();
        if (arguments.contains("fail")) return new AisCliMain.Call(3, "", "AIS-400 boom\n");
        if (arguments.contains("silent")) return new AisCliMain.Call(0, "", "");
        if (arguments.contains("nostderr")) return new AisCliMain.Call(3, "out text", "");
        if (arguments.contains("blank")) return new AisCliMain.Call(5, "", "  \n");
        if (arguments.contains("both")) return new AisCliMain.Call(3, "stdout part", "AIS-412 stale \n  ");
        return new AisCliMain.Call(0, "ARGS:" + String.join("|", args) + " SESSION:" + label + "\n", "");
    }

    public static void main(String[] args) throws Exception {
        List<String> lines = Files.readAllLines(Path.of(args[0]), StandardCharsets.UTF_8);
        boolean write = args.length > 1 && "--write".equals(args[1]);
        McpServer server = new McpServer(write, DriveMcp::fake);
        StringBuilder out = new StringBuilder();
        int n = 0;
        for (String line : lines) {
            if (line.isBlank()) continue;
            String response = server.handle(line);
            out.append(n++).append(' ').append(response == null ? "<none>" : response).append('\n');
        }
        System.out.write(out.toString().getBytes(StandardCharsets.UTF_8));
        System.out.flush();
    }
}
