package aisurada.session;

import aisurada.args.JsonValue;

import java.io.BufferedReader;
import java.io.IOException;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

import static aisurada.session.AgentTranscripts.addText;
import static aisurada.session.AgentTranscripts.addTool;
import static aisurada.session.AgentTranscripts.arr;
import static aisurada.session.AgentTranscripts.number;
import static aisurada.session.AgentTranscripts.obj;
import static aisurada.session.AgentTranscripts.readObject;
import static aisurada.session.AgentTranscripts.str;

/**
 * Codex 기록(~/.codex/sessions/연/월/일/rollout-*.jsonl) 읽기.
 *
 * <p>한 줄이 {@code {timestamp, type, payload}} 하나입니다. 새 형식은 {@code event_msg}의
 * {@code item_completed}에 사용자 메시지, 에이전트 메시지, 명령 실행, 도구 호출이 정리되어 담기고,
 * 예전 형식은 {@code user_message}, {@code agent_message}와 {@code response_item}의 함수 호출입니다.
 * 둘이 한 파일에 섞이지 않으므로 한 번 훑으면서 두 갈래를 따로 모으고, 새 형식이 있으면 그것을 씁니다.
 * 파일이 수십 MB가 될 수 있어서 필요한 표지가 든 줄만 JSON으로 읽습니다.
 */
final class CodexTranscript {

    private CodexTranscript() {}

    static AgentTranscripts.Parsed parse(Path file) throws IOException {
        List<AgentTranscripts.Turn> items = new ArrayList<>();
        List<AgentTranscripts.Turn> legacy = new ArrayList<>();
        String cwd = null;
        String model = null;
        int lines = 0;
        int skipped = 0;

        try (BufferedReader in = AgentTranscripts.reader(file)) {
            String line;
            while ((line = in.readLine()) != null) {
                lines++;
                if (line.isBlank()) continue;
                // 필요한 줄만 읽는다: 대화, 도구, 위치와 모델이 든 줄
                boolean wanted = line.contains("\"item_completed\"") || line.contains("\"user_message\"")
                        || line.contains("\"agent_message\"") || line.contains("\"function_call\"")
                        || line.contains("\"session_meta\"") || line.contains("\"turn_context\"");
                if (!wanted) continue;
                Map<String, JsonValue> record = readObject(line);
                if (record == null) {
                    skipped++;
                    continue;
                }
                String type = str(record, "type");
                String at = str(record, "timestamp");
                Map<String, JsonValue> payload = obj(record, "payload");
                if (type == null || payload == null) continue;

                switch (type) {
                    case "session_meta" -> {
                        if (cwd == null) cwd = str(payload, "cwd");
                    }
                    case "turn_context" -> {
                        String m = str(payload, "model");
                        if (m != null) model = m;
                        if (cwd == null) cwd = str(payload, "cwd");
                    }
                    case "event_msg" -> {
                        String kind = str(payload, "type");
                        if ("item_completed".equals(kind)) {
                            item(obj(payload, "item"), at, items);
                        } else if ("user_message".equals(kind)) {
                            addText(legacy, "user", at, str(payload, "message"));
                        } else if ("agent_message".equals(kind)) {
                            addText(legacy, "assistant", at, str(payload, "message"));
                        }
                    }
                    case "response_item" -> {
                        if ("function_call".equals(str(payload, "type"))) {
                            addTool(legacy, at, str(payload, "name"), legacyArgument(str(payload, "arguments")), null, false);
                        }
                    }
                    default -> { }
                }
            }
        }

        List<AgentTranscripts.Turn> turns = items.isEmpty() ? legacy : items;
        return new AgentTranscripts.Parsed(turns, lines, skipped, cwd, model);
    }

    /** 새 형식의 항목 하나를 회차로 바꾼다. 생각(Reasoning)과 맥락 압축 따위는 버린다. */
    private static void item(Map<String, JsonValue> item, String at, List<AgentTranscripts.Turn> turns) {
        if (item == null) return;
        String kind = str(item, "type");
        if (kind == null) return;
        switch (kind) {
            case "UserMessage" -> addText(turns, "user", at, joinText(item.get("content")));
            case "AgentMessage" -> addText(turns, "assistant", at, joinText(item.get("content")));
            case "CommandExecution" -> {
                Double exit = number(item.get("exit_code"));
                addTool(turns, at, "shell", commandText(item.get("command")), seconds(obj(item, "duration")), exit != null && exit != 0.0);
            }
            case "McpToolCall" -> {
                String server = str(item, "server");
                String tool = str(item, "tool");
                boolean failed = !"completed".equals(str(item, "status"));
                addTool(turns, at, (server == null ? "" : server + "/") + (tool == null ? "mcp" : tool), null, seconds(obj(item, "duration")), failed);
            }
            case "FileChange" -> addTool(turns, at, "apply_patch", changedFiles(obj(item, "changes")), null, !"completed".equals(str(item, "status")));
            default -> { }
        }
    }

    /** content 배열의 글 조각을 이어 붙인다. 조각 종류 이름은 버전마다 text, Text, input_text, output_text로 다르다. */
    private static String joinText(JsonValue content) {
        List<JsonValue> parts = arr(content);
        if (parts == null) return str(content);
        StringBuilder sb = new StringBuilder();
        for (JsonValue part : parts) {
            String text = str(obj(part), "text");
            if (text != null) {
                if (sb.length() > 0) sb.append('\n');
                sb.append(text);
            }
        }
        return sb.toString();
    }

    /** ["/bin/zsh","-lc","실제 명령"] 꼴이면 마지막 칸만, 아니면 칸을 공백으로 이어서 낸다. */
    private static String commandText(JsonValue command) {
        List<JsonValue> parts = arr(command);
        if (parts == null || parts.isEmpty()) return str(command);
        List<String> words = new ArrayList<>();
        for (JsonValue part : parts) {
            String word = str(part);
            if (word != null) words.add(word);
        }
        if (words.size() >= 3 && words.get(1).endsWith("c") && words.get(1).startsWith("-")) return words.get(words.size() - 1);
        return String.join(" ", words);
    }

    private static Double seconds(Map<String, JsonValue> duration) {
        if (duration == null) return null;
        Double secs = number(duration.get("secs"));
        Double nanos = number(duration.get("nanos"));
        if (secs == null && nanos == null) return null;
        double total = (secs == null ? 0 : secs) + (nanos == null ? 0 : nanos) / 1e9;
        return Math.round(total * 10.0) / 10.0;
    }

    private static String changedFiles(Map<String, JsonValue> changes) {
        if (changes == null || changes.isEmpty()) return null;
        List<String> names = new ArrayList<>();
        for (String path : changes.keySet()) {
            int slash = path.lastIndexOf('/');
            names.add(slash >= 0 ? path.substring(slash + 1) : path);
        }
        return String.join(", ", names);
    }

    /** 예전 형식의 arguments는 JSON 글이다: {"command":["bash","-lc","..."]}. */
    private static String legacyArgument(String arguments) {
        if (arguments == null) return null;
        Map<String, JsonValue> parsed = readObject(arguments);
        if (parsed == null) return arguments;
        String command = commandText(parsed.get("command"));
        return command != null ? command : arguments;
    }
}
