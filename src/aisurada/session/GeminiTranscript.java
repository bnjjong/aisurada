package aisurada.session;

import aisurada.args.JsonReader;
import aisurada.args.JsonValue;

import java.io.BufferedReader;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import static aisurada.session.AgentTranscripts.addText;
import static aisurada.session.AgentTranscripts.addTool;
import static aisurada.session.AgentTranscripts.arr;
import static aisurada.session.AgentTranscripts.obj;
import static aisurada.session.AgentTranscripts.readObject;
import static aisurada.session.AgentTranscripts.str;

/**
 * Gemini 기록(~/.gemini/tmp/&lt;프로젝트&gt;/chats/session-*.jsonl) 읽기.
 *
 * <p>첫 줄이 {@code {sessionId, projectHash, startTime, kind}} 머리이고, 그 뒤로 {@code user}와 {@code gemini}
 * 메시지가 한 줄씩 오고, {@code $set}(갱신 시각)과 {@code $rewindTo}(그 메시지부터 되감기)가 끼어듭니다.
 * 기록 파일에는 작업 폴더가 없어서, 프로젝트 폴더 이름을 ~/.gemini/projects.json으로 거꾸로 찾습니다.
 */
final class GeminiTranscript {

    /** 대화가 시작될 때 CLI가 사용자 줄로 끼워 넣는 환경 설명. 사용자가 쓴 글이 아니다. */
    private static final String INJECTED_PREFIX = "<session_context>";

    private GeminiTranscript() {}

    /** 첫 줄 머리의 sessionId. 못 읽으면 null입니다. */
    static String headerSessionId(Path file) {
        try (BufferedReader in = AgentTranscripts.reader(file)) {
            String first = in.readLine();
            return first == null ? null : str(readObject(first), "sessionId");
        } catch (IOException | RuntimeException e) {
            return null;
        }
    }

    static AgentTranscripts.Parsed parse(Path file, Path tmpDir) throws IOException {
        List<AgentTranscripts.Turn> turns = new ArrayList<>();
        Map<String, Integer> firstTurnOfMessage = new HashMap<>();
        String model = null;
        int lines = 0;
        int skipped = 0;

        try (BufferedReader in = AgentTranscripts.reader(file)) {
            String line;
            while ((line = in.readLine()) != null) {
                lines++;
                if (line.isBlank()) continue;
                Map<String, JsonValue> record = readObject(line);
                if (record == null) {
                    skipped++;
                    continue;
                }
                String rewindTo = str(record, "$rewindTo");
                if (rewindTo != null) {
                    Integer from = firstTurnOfMessage.get(rewindTo);
                    if (from != null && from <= turns.size()) {
                        turns.subList(from, turns.size()).clear();
                    }
                    continue;
                }
                String type = str(record, "type");
                if (type == null) continue; // 머리와 $set 줄
                String at = str(record, "timestamp");
                String id = str(record, "id");
                if (id != null) firstTurnOfMessage.put(id, turns.size());

                if ("user".equals(type)) {
                    String text = joinText(record.get("content"));
                    if (text != null && !text.startsWith(INJECTED_PREFIX)) addText(turns, "user", at, text);
                } else if ("gemini".equals(type)) {
                    String m = str(record, "model");
                    if (m != null) model = m;
                    addText(turns, "assistant", at, joinText(record.get("content")));
                    List<JsonValue> calls = arr(record.get("toolCalls"));
                    if (calls != null) {
                        for (JsonValue call : calls) tool(obj(call), at, turns);
                    }
                }
            }
        }
        for (int i = 0; i < turns.size(); i++) turns.get(i).index = i;
        return new AgentTranscripts.Parsed(turns, lines, skipped, workingDirectory(file, tmpDir), model);
    }

    private static void tool(Map<String, JsonValue> call, String at, List<AgentTranscripts.Turn> turns) {
        if (call == null) return;
        String name = str(call, "name");
        if (name == null) return;
        boolean failed = call.get("status") != null && !"success".equals(str(call, "status"));
        addTool(turns, at, name, summarize(obj(call, "args")), null, failed);
    }

    /** 인자 가운데 사람이 읽을 만한 것 하나를 고른다. Claude 쪽 도구 요약과 같은 키 차례다. */
    private static String summarize(Map<String, JsonValue> args) {
        if (args == null) return null;
        for (String key : new String[] {"description", "file_path", "pattern", "query", "command", "url", "dir_path"}) {
            String value = str(args, key);
            if (value != null && !value.isBlank()) return value;
        }
        return null;
    }

    /** content는 {@code [{text}]} 배열이거나 그냥 글이다. */
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

    /** …/tmp/&lt;프로젝트 이름&gt;/chats/파일 에서 프로젝트 이름을 꺼내 projects.json의 거꾸로 찾는다. */
    private static String workingDirectory(Path file, Path tmpDir) {
        Path chats = file.getParent();
        Path project = chats == null ? null : chats.getParent();
        if (project == null || project.getFileName() == null || tmpDir == null) return null;
        String name = project.getFileName().toString();
        Path projectsFile = tmpDir.getParent() == null ? null : tmpDir.getParent().resolve("projects.json");
        if (projectsFile == null || !Files.isRegularFile(projectsFile)) return null;
        try {
            Map<String, JsonValue> root = obj(JsonReader.parse(Files.readString(projectsFile)));
            Map<String, JsonValue> projects = obj(root, "projects");
            if (projects == null) return null;
            for (Map.Entry<String, JsonValue> entry : projects.entrySet()) {
                if (name.equals(str(entry.getValue()))) return entry.getKey();
            }
        } catch (IOException | RuntimeException ignored) {
            // projects.json을 못 읽으면 작업 폴더를 모르는 채로 둔다
        }
        return null;
    }
}
