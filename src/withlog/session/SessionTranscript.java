package withlog.session;

import withlog.args.JsonReader;
import withlog.args.JsonValue;

import java.io.*;
import java.net.URI;
import java.nio.ByteBuffer;
import java.nio.channels.FileChannel;
import java.nio.charset.CharsetDecoder;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.time.Duration;
import java.time.Instant;
import java.util.*;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * Claude Code 세션 전사 파일(.jsonl) 파서 및 정보 추출기.
 * 설명서 7장의 전사 읽기 규칙 및 D18 결함 고침(UTF-8 대체 문자 치환)을 준수합니다.
 */
public final class SessionTranscript {

    public static final Pattern SESSION_ID_PATTERN = Pattern.compile("^[0-9a-fA-F-]{8,64}$");
    public static final Pattern NOTE_ID_PATTERN = Pattern.compile("^[0-9a-z-]{10,64}$");

    private static final int MAX_TEXT_CHARS = 2000;
    private static final int CHUNK_BYTES = 65536;

    private static final Pattern URL_PATTERN = Pattern.compile("https?://[^ \\t\\r\\n<>\"'`)\\]}]+");
    private static final Pattern ENTRYPOINT_PATTERN = Pattern.compile("\"entrypoint\"\\s*:\\s*\"([^\"]*)\"");
    private static final Pattern TIMESTAMP_PATTERN = Pattern.compile("\"timestamp\"\\s*:\\s*\"([^\"]*)\"");
    private static final Pattern GIT_BRANCH_PATTERN = Pattern.compile("\"gitBranch\"\\s*:\\s*\"([^\"]*)\"");
    private static final Pattern CWD_PATTERN = Pattern.compile("\"cwd\"\\s*:\\s*\"([^\"]*)\"");
    private static final Pattern TASK_CREATE_PATTERN = Pattern.compile("^Task #(\\d+) created successfully:\\s*(.*)$");
    private static final Pattern TASK_UPDATE_PATTERN = Pattern.compile("^Updated task #(\\d+).*$");

    private static final Set<String> LOCAL_HOSTS = Set.of(
            "localhost", "ipc.localhost", "asset.localhost", "127.0.0.1"
    );

    private SessionTranscript() {}

    /**
     * Claude 프로젝트 디렉토리 경로(.claude/projects)를 반환합니다.
     * 테스트 및 시스템 속성(withlog.claude.home)으로 홈 경로 오버라이드가 가능합니다.
     */
    public static Path getClaudeProjectsDir() {
        String customHome = System.getProperty("withlog.claude.home");
        if (customHome != null && !customHome.trim().isEmpty()) {
            return Path.of(customHome.trim()).resolve(".claude").resolve("projects");
        }
        String userHome = System.getProperty("user.home");
        if (userHome == null || userHome.trim().isEmpty()) {
            return null;
        }
        return Path.of(userHome).resolve(".claude").resolve("projects");
    }

    /**
     * 지정된 세션 식별자에 해당하는 전사 파일(.jsonl)을 프로젝트 폴더들에서 찾습니다.
     *
     * @param projectsDir .claude/projects 디렉토리 경로
     * @param sessionId   세션 식별자
     * @return 찾은 전사 파일의 Path 또는 null
     */
    public static Path findTranscriptFile(Path projectsDir, String sessionId) {
        if (projectsDir == null || !Files.isDirectory(projectsDir)) {
            return null;
        }
        String targetFileName = sessionId + ".jsonl";
        try (DirectoryStream<Path> stream = Files.newDirectoryStream(projectsDir)) {
            for (Path projectDir : stream) {
                if (Files.isDirectory(projectDir)) {
                    Path candidate = projectDir.resolve(targetFileName);
                    if (Files.isRegularFile(candidate)) {
                        return candidate;
                    }
                    // 대소문자 매칭을 위해 디렉토리 내부 검색 (대소문자 보존/구분 환경 호환)
                    try (DirectoryStream<Path> fileStream = Files.newDirectoryStream(projectDir, "*.jsonl")) {
                        for (Path f : fileStream) {
                            if (Files.isRegularFile(f) && f.getFileName().toString().equalsIgnoreCase(targetFileName)) {
                                return f;
                            }
                        }
                    } catch (IOException ignored) {}
                }
            }
        } catch (IOException ignored) {}
        return null;
    }

    /**
     * D18 결함 고침: UTF-8 깨진 바이트가 있어도 대체 문자(U+FFFD)로 치환하여 유연하게 읽는 BufferedReader를 생성합니다.
     */
    public static BufferedReader newFlexibleReader(Path file) throws IOException {
        CharsetDecoder decoder = StandardCharsets.UTF_8.newDecoder()
                .onMalformedInput(CodingErrorAction.REPLACE)
                .onUnmappableCharacter(CodingErrorAction.REPLACE);
        InputStream is = Files.newInputStream(file);
        return new BufferedReader(new InputStreamReader(is, decoder));
    }

    /**
     * read-session 명령 실행을 위한 전사 훑기 결과를 담는 레코드.
     */
    public record ReadSessionResult(
            String sessionId,
            String filePath,
            long bytes,
            int lines,
            int skipped,
            int messages,
            boolean hasMore,
            List<Map<String, Object>> urls,
            List<Map<String, Object>> runningAgents,
            List<Map<String, Object>> todos,
            Map<String, Object> runState,
            List<Integer> noteMentions,
            Map<String, Object> author,
            List<Map<String, Object>> turns
    ) {}

    /**
     * 전사 파일을 훑어 세션 대화 및 메타데이터를 파싱합니다.
     */
    public static ReadSessionResult readSession(
            Path projectsDir,
            String sessionId,
            int limit,
            Integer before,
            boolean includeUrls,
            String noteId,
            List<String> authorNotes
    ) {
        Path file = findTranscriptFile(projectsDir, sessionId);
        if (file == null) {
            // 기록이 없음 (7.9)
            return new ReadSessionResult(
                    sessionId,
                    null,
                    0, 0, 0, 0,
                    false,
                    Collections.emptyList(),
                    Collections.emptyList(),
                    Collections.emptyList(),
                    null,
                    noteId != null ? Collections.emptyList() : null,
                    authorNotes != null && !authorNotes.isEmpty() ? null : null,
                    Collections.emptyList()
            );
        }

        long fileBytes;
        try {
            fileBytes = Files.size(file);
        } catch (IOException e) {
            fileBytes = 0;
        }

        int lineCount = 0;
        int skippedCount = 0;
        List<RawTurn> allTurns = new ArrayList<>();
        Map<String, RawTurn> toolUseMap = new HashMap<>(); // tool_use id -> RawTurn
        Set<String> matchedToolUseIds = new HashSet<>();
        Map<Integer, Map<String, Object>> todosMap = new TreeMap<>(); // number -> todo
        Map<String, Integer> urlCountMap = new LinkedHashMap<>();

        // 노트 mentions 수집
        Set<String> notesToTrack = new HashSet<>();
        if (noteId != null) notesToTrack.add(noteId);
        if (authorNotes != null) notesToTrack.addAll(authorNotes);
        Map<String, List<Integer>> noteMentionsMap = new HashMap<>();
        for (String nid : notesToTrack) {
            noteMentionsMap.put(nid, new ArrayList<>());
        }

        // 실행 상태 (runState) 추적
        String currentModel = null;
        String currentEffort = null;
        Long currentContextTokens = null;
        boolean quotaBlocked = false;
        Long quotaResetsAt = null;

        try (BufferedReader reader = newFlexibleReader(file)) {
            String line;
            while ((line = reader.readLine()) != null) {
                lineCount++;
                if (line.trim().isEmpty()) {
                    continue; // 빈 줄은 안 센다 (7.4)
                }

                JsonValue jsonValue;
                try {
                    jsonValue = JsonReader.parseFile(line);
                } catch (Exception e) {
                    skippedCount++;
                    continue;
                }

                if (!(jsonValue instanceof JsonValue.JsonObject obj)) {
                    skippedCount++;
                    continue;
                }

                Map<String, JsonValue> props = obj.properties();
                String type = getStringProp(props, "type");
                if (type == null) {
                    continue;
                }

                boolean isUserOrAssistant = "user".equals(type) || "assistant".equals(type);

                // user / assistant 줄에서 원문 노트 ID 매칭 (7.8)
                if (isUserOrAssistant && !notesToTrack.isEmpty() && !allTurns.isEmpty()) {
                    int lastTurnIdx = allTurns.size() - 1;
                    for (String nid : notesToTrack) {
                        if (line.contains(nid)) {
                            List<Integer> list = noteMentionsMap.get(nid);
                            if (list.isEmpty() || list.get(list.size() - 1) != lastTurnIdx) {
                                list.add(lastTurnIdx);
                            }
                        }
                    }
                }

                String timestamp = getStringProp(props, "timestamp");

                if ("user".equals(type)) {
                    JsonValue msgVal = props.get("message");
                    if (msgVal instanceof JsonValue.JsonObject msgObj) {
                        JsonValue contentVal = msgObj.properties().get("content");
                        if (contentVal instanceof JsonValue.JsonString strVal) {
                            String text = strVal.value();
                            if (includeUrls) extractUrls(text, urlCountMap);
                            addTextTurn(allTurns, "user", timestamp, text);
                        } else if (contentVal instanceof JsonValue.JsonArray arrVal) {
                            for (JsonValue elem : arrVal.elements()) {
                                if (elem instanceof JsonValue.JsonObject blockObj) {
                                    Map<String, JsonValue> bProps = blockObj.properties();
                                    String bType = getStringProp(bProps, "type");
                                    if ("text".equals(bType)) {
                                        String text = getStringProp(bProps, "text");
                                        if (text != null) {
                                            if (includeUrls) extractUrls(text, urlCountMap);
                                            addTextTurn(allTurns, "user", timestamp, text);
                                        }
                                    } else if ("tool_result".equals(bType)) {
                                        // tool_result 매칭
                                        String toolUseId = getStringProp(bProps, "tool_use_id");
                                        boolean isError = getBooleanProp(bProps, "is_error", false);
                                        JsonValue resultContent = bProps.get("content");

                                        if (toolUseId != null && toolUseMap.containsKey(toolUseId)) {
                                            matchedToolUseIds.add(toolUseId);
                                            RawTurn toolTurn = toolUseMap.get(toolUseId);
                                            if (isError) {
                                                toolTurn.toolFailed = true;
                                            }
                                            if (toolTurn.at != null && timestamp != null) {
                                                try {
                                                    Instant start = Instant.parse(toolTurn.at);
                                                    Instant end = Instant.parse(timestamp);
                                                    double diffSec = (double) Duration.between(start, end).toMillis() / 1000.0;
                                                    if (diffSec < 0) diffSec = 0.0;
                                                    toolTurn.toolSeconds = diffSec;
                                                } catch (Exception ignored) {}
                                            }

                                            // TaskCreate / TaskUpdate 분석 (7.6)
                                            if (resultContent instanceof JsonValue.JsonString resStr) {
                                                String resText = resStr.value();
                                                String firstLine = getFirstLine(resText);
                                                String toolName = toolTurn.toolName;
                                                if ("TaskCreate".equals(toolName)) {
                                                    Matcher m = TASK_CREATE_PATTERN.matcher(firstLine);
                                                    if (m.matches()) {
                                                        int num = Integer.parseInt(m.group(1));
                                                        String tTitle = m.group(2).trim();
                                                        Map<String, Object> todo = new LinkedHashMap<>();
                                                        todo.put("number", num);
                                                        todo.put("title", tTitle);
                                                        todo.put("status", "pending");
                                                        todosMap.put(num, todo);
                                                    } else {
                                                        // Task #N created successfully 꼴이 아니면 skipped++ (7.4)
                                                        skippedCount++;
                                                    }
                                                } else if ("TaskUpdate".equals(toolName)) {
                                                    Matcher m = TASK_UPDATE_PATTERN.matcher(firstLine);
                                                    if (m.matches()) {
                                                        int num = Integer.parseInt(m.group(1));
                                                        Map<String, JsonValue> inProps = toolTurn.toolInputProps;
                                                        if (inProps != null) {
                                                            String status = getStringProp(inProps, "status");
                                                            String subject = getStringProp(inProps, "subject");
                                                            if ("deleted".equals(status)) {
                                                                todosMap.remove(num);
                                                            } else if (todosMap.containsKey(num)) {
                                                                Map<String, Object> todo = todosMap.get(num);
                                                                if (subject != null) todo.put("title", subject);
                                                                if (status != null && Set.of("pending", "in_progress", "completed").contains(status)) {
                                                                    todo.put("status", status);
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else if ("assistant".equals(type)) {
                    // 실행 상태 (runState) 갱신 (7.6)
                    JsonValue msgVal = props.get("message");
                    if (msgVal instanceof JsonValue.JsonObject msgObj) {
                        Map<String, JsonValue> msgProps = msgObj.properties();
                        String model = getStringProp(msgProps, "model");
                        if (model != null && !"<synthetic>".equals(model)) {
                            currentModel = model;
                            currentEffort = getStringProp(props, "effort");

                            JsonValue usageVal = msgProps.get("usage");
                            if (usageVal instanceof JsonValue.JsonObject usageObj) {
                                Map<String, JsonValue> uProps = usageObj.properties();
                                Long inTok = getLongProp(uProps, "input_tokens");
                                if (inTok != null) {
                                    long creation = getLongProp(uProps, "cache_creation_input_tokens", 0L);
                                    long read = getLongProp(uProps, "cache_read_input_tokens", 0L);
                                    currentContextTokens = inTok + creation + read;
                                } else {
                                    currentContextTokens = null;
                                }
                            } else {
                                currentContextTokens = null;
                            }
                        }

                        // quotaLimits 확인 (synthetic 줄에서도 반영하지만 모델이 설정된 이후여야 함)
                        JsonValue quotaVal = props.get("quotaLimits");
                        if (quotaVal instanceof JsonValue.JsonObject qObj) {
                            if (currentModel != null) {
                                quotaBlocked = true;
                                Long resetsAt = getLongProp(qObj.properties(), "resetsAt");
                                if (resetsAt != null) {
                                    quotaResetsAt = resetsAt;
                                }
                            }
                        }

                        // content 처리: 글 블록 먼저, 그 뒤 tool_use 블록 (7.4)
                        JsonValue contentVal = msgProps.get("content");
                        if (contentVal instanceof JsonValue.JsonString strVal) {
                            String text = strVal.value();
                            if (includeUrls) extractUrls(text, urlCountMap);
                            addTextTurn(allTurns, "assistant", timestamp, text);
                        } else if (contentVal instanceof JsonValue.JsonArray arrVal) {
                            List<JsonValue.JsonObject> textBlocks = new ArrayList<>();
                            List<JsonValue.JsonObject> toolBlocks = new ArrayList<>();
                            for (JsonValue elem : arrVal.elements()) {
                                if (elem instanceof JsonValue.JsonObject blockObj) {
                                    String bType = getStringProp(blockObj.properties(), "type");
                                    if ("text".equals(bType)) {
                                        textBlocks.add(blockObj);
                                    } else if ("tool_use".equals(bType)) {
                                        toolBlocks.add(blockObj);
                                    }
                                }
                            }

                            // 1) 글 회차
                            for (JsonValue.JsonObject b : textBlocks) {
                                String text = getStringProp(b.properties(), "text");
                                if (text != null) {
                                    if (includeUrls) extractUrls(text, urlCountMap);
                                    addTextTurn(allTurns, "assistant", timestamp, text);
                                }
                            }

                            // 2) 도구 회차
                            for (JsonValue.JsonObject b : toolBlocks) {
                                Map<String, JsonValue> bProps = b.properties();
                                String toolId = getStringProp(bProps, "id");
                                String toolName = getStringProp(bProps, "name");
                                if (toolId == null || toolName == null) {
                                    continue;
                                }

                                JsonValue inputVal = bProps.get("input");
                                Map<String, JsonValue> inputProps = (inputVal instanceof JsonValue.JsonObject inObj) ? inObj.properties() : null;
                                String argSummary = summarizeToolArg(inputProps);

                                RawTurn toolTurn = new RawTurn();
                                toolTurn.index = allTurns.size();
                                toolTurn.who = "tool";
                                toolTurn.at = timestamp;
                                toolTurn.chars = 0;
                                toolTurn.truncated = false;
                                toolTurn.text = "";
                                toolTurn.toolName = toolName;
                                toolTurn.toolArg = argSummary;
                                toolTurn.toolInputProps = inputProps;
                                toolTurn.toolUseId = toolId;

                                allTurns.add(toolTurn);
                                toolUseMap.put(toolId, toolTurn);
                            }
                        }
                    }
                }
            }
        } catch (IOException e) {
            throw new RuntimeException("사이드카 안에서 실패했다: " + e.getMessage() + ". 자세한 것은 사이드카 로그에 있다.", e);
        }

        // 전체 회차 수
        int totalMessages = allTurns.size();

        // 페이지 슬라이싱 (7.5)
        int effectiveLimit = Math.max(1, Math.min(500, limit));
        int effectiveBefore = (before == null) ? totalMessages : Math.max(0, Math.min(totalMessages, before));

        List<RawTurn> pageTurns = sliceTurns(allTurns, effectiveLimit, effectiveBefore);
        boolean hasMore = false;
        if (!pageTurns.isEmpty()) {
            hasMore = pageTurns.get(0).index > 0;
        }

        // runningAgents 수집 (7.6)
        List<Map<String, Object>> runningAgents = new ArrayList<>();
        for (RawTurn turn : allTurns) {
            if ("tool".equals(turn.who) && "Agent".equals(turn.toolName)) {
                if (!matchedToolUseIds.contains(turn.toolUseId)) {
                    Map<String, Object> agentObj = new LinkedHashMap<>();
                    if (turn.toolArg != null) {
                        agentObj.put("what", turn.toolArg);
                    }
                    if (turn.at != null) {
                        agentObj.put("at", turn.at);
                    }
                    runningAgents.add(agentObj);
                }
            }
        }

        // urls 수집 (7.6)
        List<Map<String, Object>> urlsList = new ArrayList<>();
        if (includeUrls) {
            List<Map.Entry<String, Integer>> sortedUrls = new ArrayList<>(urlCountMap.entrySet());
            sortedUrls.sort((a, b) -> Integer.compare(b.getValue(), a.getValue()));
            for (Map.Entry<String, Integer> entry : sortedUrls) {
                Map<String, Object> u = new LinkedHashMap<>();
                u.put("url", entry.getKey());
                u.put("count", entry.getValue());
                urlsList.add(u);
            }
        }

        // todos 리스트 (7.6)
        List<Map<String, Object>> todosList = new ArrayList<>(todosMap.values());

        // runState 객체 (7.6)
        Map<String, Object> runStateObj = null;
        if (currentModel != null) {
            runStateObj = new LinkedHashMap<>();
            runStateObj.put("model", currentModel);
            if (currentEffort != null) {
                runStateObj.put("effort", currentEffort);
            }
            if (currentContextTokens != null) {
                runStateObj.put("contextTokens", currentContextTokens);
            }
            runStateObj.put("quotaBlocked", quotaBlocked);
            if (quotaBlocked && quotaResetsAt != null) {
                runStateObj.put("quotaResetsAt", quotaResetsAt);
            }
        }

        // noteMentions 결과
        List<Integer> noteMentionsResult = (noteId != null) ? noteMentionsMap.getOrDefault(noteId, Collections.emptyList()) : null;

        // author 결과 (7.8)
        Map<String, Object> authorResult = null;
        if (authorNotes != null && !authorNotes.isEmpty()) {
            String selectedNote = null;
            List<Integer> selectedMentions = null;
            for (String an : authorNotes) {
                List<Integer> m = noteMentionsMap.get(an);
                if (m != null && !m.isEmpty()) {
                    selectedNote = an;
                    selectedMentions = m;
                    break;
                }
            }

            if (selectedNote != null) {
                authorResult = new LinkedHashMap<>();
                authorResult.put("note", selectedNote);
                authorResult.put("mentions", selectedMentions);

                int lastIdx = selectedMentions.get(selectedMentions.size() - 1);
                List<RawTurn> latestRaw = sliceTurns(allTurns, effectiveLimit, lastIdx + 1);
                List<Map<String, Object>> latestTurns = convertTurnsToMaps(latestRaw);

                boolean latestStartsAtZero = !latestRaw.isEmpty() && latestRaw.get(0).index == 0;
                if (selectedMentions.size() > 1 && !latestStartsAtZero) {
                    int firstIdx = selectedMentions.get(0);
                    List<RawTurn> creationRaw = sliceTurns(allTurns, effectiveLimit, firstIdx + 1);
                    authorResult.put("creation", convertTurnsToMaps(creationRaw));
                }

                authorResult.put("latest", latestTurns);
            }
        }

        return new ReadSessionResult(
                sessionId,
                file.toAbsolutePath().toString(),
                fileBytes,
                lineCount,
                skippedCount,
                totalMessages,
                hasMore,
                urlsList,
                runningAgents,
                todosList,
                runStateObj,
                noteMentionsResult,
                authorResult,
                convertTurnsToMaps(pageTurns)
        );
    }

    /**
     * 끝 자리(before)에서 역방향으로 도구가 아닌 회차를 limit개 세어 슬라이싱합니다.
     */
    private static List<RawTurn> sliceTurns(List<RawTurn> allTurns, int limit, int before) {
        int end = Math.min(before, allTurns.size());
        if (end <= 0) return Collections.emptyList();

        int start = end;
        int nonToolCount = 0;
        while (start > 0 && nonToolCount < limit) {
            start--;
            RawTurn t = allTurns.get(start);
            if (!"tool".equals(t.who)) {
                nonToolCount++;
            }
        }
        return new ArrayList<>(allTurns.subList(start, end));
    }

    private static List<Map<String, Object>> convertTurnsToMaps(List<RawTurn> rawTurns) {
        List<Map<String, Object>> list = new ArrayList<>(rawTurns.size());
        for (RawTurn t : rawTurns) {
            Map<String, Object> map = new LinkedHashMap<>();
            map.put("index", t.index);
            map.put("who", t.who);
            map.put("at", t.at);
            map.put("chars", t.chars);
            map.put("truncated", t.truncated);
            if ("tool".equals(t.who)) {
                Map<String, Object> toolMap = new LinkedHashMap<>();
                toolMap.put("name", t.toolName);
                if (t.toolArg != null) {
                    toolMap.put("arg", t.toolArg);
                }
                if (t.toolSeconds != null) {
                    toolMap.put("seconds", t.toolSeconds);
                }
                if (t.toolFailed) {
                    toolMap.put("failed", true);
                }
                map.put("tool", toolMap);
            }
            map.put("text", t.text != null ? t.text : "");
            list.add(map);
        }
        return list;
    }

    private static void addTextTurn(List<RawTurn> turns, String who, String at, String rawText) {
        if (rawText == null || rawText.trim().isEmpty()) {
            return; // 공백뿐인 글은 버린다 (7.4)
        }
        RawTurn turn = new RawTurn();
        turn.index = turns.size();
        turn.who = who;
        turn.at = at;
        turn.chars = rawText.length();
        if (rawText.length() > MAX_TEXT_CHARS) {
            turn.text = rawText.substring(0, MAX_TEXT_CHARS);
            turn.truncated = true;
        } else {
            turn.text = rawText;
            turn.truncated = false;
        }
        turns.add(turn);
    }

    private static String summarizeToolArg(Map<String, JsonValue> inputProps) {
        if (inputProps == null || inputProps.isEmpty()) return null;
        String[] candidateKeys = {"description", "file_path", "pattern", "query", "command", "url"};
        String found = null;
        for (String k : candidateKeys) {
            String val = getStringProp(inputProps, k);
            if (val != null && !val.trim().isEmpty()) {
                found = val;
                break;
            }
        }
        if (found == null) return null;

        String firstLine = getFirstLine(found.trim()).trim();
        if (firstLine.isEmpty()) return null;

        if (firstLine.length() <= 120) {
            return firstLine;
        }

        if (firstLine.startsWith("/")) {
            // 끝 119자 + 앞 …
            return "…" + firstLine.substring(firstLine.length() - 119);
        } else {
            // 앞 119자 + 뒤 …
            return firstLine.substring(0, 119) + "…";
        }
    }

    private static void extractUrls(String text, Map<String, Integer> map) {
        if (text == null || text.isEmpty()) return;
        Matcher m = URL_PATTERN.matcher(text);
        while (m.find()) {
            String candidate = m.group();
            // 끝의 .,;:!? 걷어냄
            while (candidate.endsWith(".") || candidate.endsWith(",") || candidate.endsWith(";") ||
                    candidate.endsWith(":") || candidate.endsWith("!") || candidate.endsWith("?")) {
                candidate = candidate.substring(0, candidate.length() - 1);
            }
            if (candidate.length() <= 10) continue;

            try {
                URI uri = URI.create(candidate);
                String host = uri.getHost();
                if (host != null && LOCAL_HOSTS.contains(host.toLowerCase(Locale.ROOT))) {
                    continue;
                }
            } catch (Exception e) {
                // URI 파싱 실패 시 localhost 문자열 체크
                String lower = candidate.toLowerCase(Locale.ROOT);
                if (lower.contains("://localhost") || lower.contains("://127.0.0.1") ||
                        lower.contains("://ipc.localhost") || lower.contains("://asset.localhost")) {
                    continue;
                }
            }

            map.put(candidate, map.getOrDefault(candidate, 0) + 1);
        }
    }

    /**
     * session-titles 명령 실행 결과 레코드.
     */
    public record SessionTitleInfo(
            String sessionId,
            boolean found,
            String title,
            String branch,
            String cwd
    ) {}

    /**
     * 세션의 머리와 꼬리를 읽어 제목, 브랜치, 초기 작업 디렉토리를 추출합니다 (7.10).
     */
    public static SessionTitleInfo readTitleInfo(Path projectsDir, String sessionId) {
        Path file = findTranscriptFile(projectsDir, sessionId);
        if (file == null) {
            return new SessionTitleInfo(sessionId, false, null, null, null);
        }

        long size;
        try {
            size = Files.size(file);
        } catch (IOException e) {
            return new SessionTitleInfo(sessionId, false, null, null, null);
        }

        HeadAndTail headTail = readHeadAndTail(file, size);

        // 1. Title 추출
        String title = resolveTitle(file, headTail);

        // 2. Branch 추출
        String branch = resolveBranch(headTail);

        // 3. Cwd 추출
        String cwd = resolveCwd(headTail);

        return new SessionTitleInfo(sessionId, true, title, branch, cwd);
    }

    private static HeadAndTail readHeadAndTail(Path file, long size) {
        if (size <= CHUNK_BYTES) {
            try {
                byte[] all = Files.readAllBytes(file);
                String text = decodeUtf8Flexible(all);
                List<String> lines = splitLines(text);
                return new HeadAndTail(text, text, lines, lines);
            } catch (IOException e) {
                return new HeadAndTail("", "", Collections.emptyList(), Collections.emptyList());
            }
        }

        // size > 65536: 머리와 꼬리를 각각 읽고 가장자리 불완전 줄 제거
        try (FileChannel ch = FileChannel.open(file, StandardOpenOption.READ)) {
            // 머리 읽기
            ByteBuffer headBuf = ByteBuffer.allocate(CHUNK_BYTES);
            ch.read(headBuf);
            byte[] headBytes = headBuf.array();
            int lastNl = lastIndexOfByte(headBytes, (byte) '\n');
            byte[] validHead = (lastNl >= 0) ? Arrays.copyOf(headBytes, lastNl + 1) : new byte[0];
            String headText = decodeUtf8Flexible(validHead);
            List<String> headLines = splitLines(headText);

            // 꼬리 읽기
            ByteBuffer tailBuf = ByteBuffer.allocate(CHUNK_BYTES);
            ch.position(size - CHUNK_BYTES);
            ch.read(tailBuf);
            byte[] tailBytes = tailBuf.array();
            int firstNl = indexOfByte(tailBytes, (byte) '\n');
            byte[] validTail = (firstNl >= 0 && firstNl + 1 < tailBytes.length)
                    ? Arrays.copyOfRange(tailBytes, firstNl + 1, tailBytes.length)
                    : new byte[0];
            String tailText = decodeUtf8Flexible(validTail);
            List<String> tailLines = splitLines(tailText);

            return new HeadAndTail(headText, tailText, headLines, tailLines);
        } catch (IOException e) {
            return new HeadAndTail("", "", Collections.emptyList(), Collections.emptyList());
        }
    }

    private static String resolveTitle(Path transcriptFile, HeadAndTail ht) {
        // 1. 꼬리 창 마지막 custom-title 줄의 customTitle
        for (int i = ht.tailLines.size() - 1; i >= 0; i--) {
            String title = extractTitleFromLine(ht.tailLines.get(i), "custom-title", "customTitle");
            if (title != null) return title;
        }

        // 2. <프로젝트폴더>/<세션id>/custom-title.json
        Path projectDir = transcriptFile.getParent();
        String sId = transcriptFile.getFileName().toString().replaceFirst("\\.jsonl$", "");
        if (projectDir != null) {
            Path customTitleFile = projectDir.resolve(sId).resolve("custom-title.json");
            if (Files.isRegularFile(customTitleFile)) {
                try {
                    String content = Files.readString(customTitleFile, StandardCharsets.UTF_8);
                    JsonValue jv = JsonReader.parse(content);
                    if (jv instanceof JsonValue.JsonObject obj) {
                        String title = getStringProp(obj.properties(), "customTitle");
                        if (title != null && !title.trim().isEmpty()) {
                            return title;
                        }
                    }
                } catch (Exception ignored) {}
            }
        }

        // 3. 머리 창 마지막 custom-title 줄
        for (int i = ht.headLines.size() - 1; i >= 0; i--) {
            String title = extractTitleFromLine(ht.headLines.get(i), "custom-title", "customTitle");
            if (title != null) return title;
        }

        // 4. 꼬리 창 마지막 ai-title 줄의 aiTitle
        for (int i = ht.tailLines.size() - 1; i >= 0; i--) {
            String title = extractTitleFromLine(ht.tailLines.get(i), "ai-title", "aiTitle");
            if (title != null) return title;
        }

        // 5. 머리 창 마지막 ai-title 줄
        for (int i = ht.headLines.size() - 1; i >= 0; i--) {
            String title = extractTitleFromLine(ht.headLines.get(i), "ai-title", "aiTitle");
            if (title != null) return title;
        }

        return null;
    }

    private static String extractTitleFromLine(String line, String expectedType, String key) {
        if (!line.contains(expectedType)) return null;
        try {
            JsonValue jv = JsonReader.parseFile(line);
            if (jv instanceof JsonValue.JsonObject obj) {
                if (expectedType.equals(getStringProp(obj.properties(), "type"))) {
                    String title = getStringProp(obj.properties(), key);
                    if (title != null && !title.trim().isEmpty()) {
                        return title;
                    }
                }
            }
        } catch (Exception ignored) {}
        return null;
    }

    private static String resolveBranch(HeadAndTail ht) {
        // 1. 꼬리 창에서 마지막 gitBranch 줄
        for (int i = ht.tailLines.size() - 1; i >= 0; i--) {
            String branch = extractPropFromLine(ht.tailLines.get(i), "gitBranch");
            if (branch != null) return branch;
        }

        // 2. 꼬리 창 글자에서 마지막 "gitBranch":"…"
        String tailRegexBranch = findLastRegexMatch(ht.tailText, GIT_BRANCH_PATTERN);
        if (tailRegexBranch != null) return unescapeJson(tailRegexBranch);

        // 3. 머리 창에서 첫 gitBranch 줄
        for (String line : ht.headLines) {
            String branch = extractPropFromLine(line, "gitBranch");
            if (branch != null) return branch;
        }

        // 4. 머리 창 글자에서 첫 "gitBranch":"…"
        String headRegexBranch = findFirstRegexMatch(ht.headText, GIT_BRANCH_PATTERN);
        if (headRegexBranch != null) return unescapeJson(headRegexBranch);

        return null;
    }

    private static String resolveCwd(HeadAndTail ht) {
        // 1. 머리 창에서 첫 cwd
        for (String line : ht.headLines) {
            String cwd = extractPropFromLine(line, "cwd");
            if (cwd != null) return cwd;
        }

        // 2. 머리 창 글자에서 첫 "cwd":"…"
        String headRegexCwd = findFirstRegexMatch(ht.headText, CWD_PATTERN);
        if (headRegexCwd != null) return unescapeJson(headRegexCwd);

        // 꼬리는 안 본다!
        return null;
    }

    private static String extractPropFromLine(String line, String key) {
        if (!line.contains("\"" + key + "\"")) return null;
        try {
            JsonValue jv = JsonReader.parseFile(line);
            if (jv instanceof JsonValue.JsonObject obj) {
                String val = getStringProp(obj.properties(), key);
                if (val != null && !val.trim().isEmpty()) {
                    return val;
                }
            }
        } catch (Exception ignored) {}
        return null;
    }

    /**
     * recent-sessions 명령 실행 결과 레코드 (7.11).
     */
    public record RecentSessionEntry(
            String sessionId,
            long lastActiveAt,
            Long startedAt,
            String title,
            String branch,
            String cwd
    ) {}

    public record RecentSessionsResult(
            int days,
            int dropped,
            List<RecentSessionEntry> sessions
    ) {}

    /**
     * 최근 N일(days) 이내의 CLI 세션 목록을 반환합니다 (7.11).
     */
    public static RecentSessionsResult recentSessions(Path projectsDir, int days) {
        if (projectsDir == null || !Files.isDirectory(projectsDir)) {
            return new RecentSessionsResult(days, 0, Collections.emptyList());
        }

        long now = System.currentTimeMillis();
        long cutoff = now - ((long) days * 86_400_000L);

        List<Path> candidateFiles = new ArrayList<>();
        try (DirectoryStream<Path> projStream = Files.newDirectoryStream(projectsDir)) {
            for (Path pDir : projStream) {
                if (Files.isDirectory(pDir)) {
                    try (DirectoryStream<Path> fileStream = Files.newDirectoryStream(pDir, "*.jsonl")) {
                        for (Path f : fileStream) {
                            if (Files.isRegularFile(f)) {
                                String fname = f.getFileName().toString();
                                String sId = fname.substring(0, fname.length() - ".jsonl".length());
                                if (SESSION_ID_PATTERN.matcher(sId).matches()) {
                                    try {
                                        long lastMod = Files.getLastModifiedTime(f).toMillis();
                                        if (lastMod >= cutoff) {
                                            candidateFiles.add(f);
                                        }
                                    } catch (IOException ignored) {}
                                }
                            }
                        }
                    } catch (IOException ignored) {}
                }
            }
        } catch (IOException ignored) {}

        // 3. 마지막 수정 시각 내림차순, 같으면 세션 id 차례
        candidateFiles.sort((a, b) -> {
            try {
                long la = Files.getLastModifiedTime(a).toMillis();
                long lb = Files.getLastModifiedTime(b).toMillis();
                int cmp = Long.compare(lb, la);
                if (cmp != 0) return cmp;
            } catch (IOException ignored) {}
            return a.getFileName().toString().compareTo(b.getFileName().toString());
        });

        // 4. 머리 64KB에서 entrypoint: cli 인 것만 남긴다
        List<RecentSessionFile> cliFiles = new ArrayList<>();
        for (Path f : candidateFiles) {
            String headText = readHeadText(f, CHUNK_BYTES);
            String entrypoint = findFirstRegexMatch(headText, ENTRYPOINT_PATTERN);
            if ("cli".equals(entrypoint)) {
                cliFiles.add(new RecentSessionFile(f, headText));
            }
        }

        // 5. 앞에서 200개를 낸다. 나머지는 dropped
        int totalCli = cliFiles.size();
        int limit = Math.min(200, totalCli);
        int dropped = totalCli - limit;

        List<RecentSessionEntry> sessionEntries = new ArrayList<>();
        for (int i = 0; i < limit; i++) {
            RecentSessionFile rf = cliFiles.get(i);
            Path f = rf.path;
            String sId = f.getFileName().toString().replaceFirst("\\.jsonl$", "");
            long lastActiveAt;
            try {
                lastActiveAt = Files.getLastModifiedTime(f).toMillis();
            } catch (IOException e) {
                lastActiveAt = 0;
            }

            // startedAt: 머리 글자에서 처음 나온 timestamp를 ISO 시각으로 파싱
            Long startedAt = null;
            String tsStr = findFirstRegexMatch(rf.headText, TIMESTAMP_PATTERN);
            if (tsStr != null) {
                try {
                    startedAt = Instant.parse(tsStr).toEpochMilli();
                } catch (Exception ignored) {}
            }

            // title, branch, cwd (7.10 규칙)
            long size;
            try { size = Files.size(f); } catch (IOException e) { size = 0; }
            HeadAndTail ht = readHeadAndTail(f, size);
            String title = resolveTitle(f, ht);
            String branch = resolveBranch(ht);
            String cwd = resolveCwd(ht);

            sessionEntries.add(new RecentSessionEntry(sId, lastActiveAt, startedAt, title, branch, cwd));
        }

        return new RecentSessionsResult(days, dropped, sessionEntries);
    }

    private static String readHeadText(Path file, int maxBytes) {
        try (InputStream is = Files.newInputStream(file)) {
            byte[] buf = is.readNBytes(maxBytes);
            return decodeUtf8Flexible(buf);
        } catch (IOException e) {
            return "";
        }
    }

    private static String decodeUtf8Flexible(byte[] bytes) {
        CharsetDecoder decoder = StandardCharsets.UTF_8.newDecoder()
                .onMalformedInput(CodingErrorAction.REPLACE)
                .onUnmappableCharacter(CodingErrorAction.REPLACE);
        try {
            return decoder.decode(ByteBuffer.wrap(bytes)).toString();
        } catch (Exception e) {
            return new String(bytes, StandardCharsets.UTF_8);
        }
    }

    private static List<String> splitLines(String text) {
        if (text == null || text.isEmpty()) return Collections.emptyList();
        String[] split = text.split("\r?\n", -1);
        List<String> list = new ArrayList<>(split.length);
        for (String s : split) {
            list.add(s);
        }
        return list;
    }

    private static int lastIndexOfByte(byte[] arr, byte b) {
        for (int i = arr.length - 1; i >= 0; i--) {
            if (arr[i] == b) return i;
        }
        return -1;
    }

    private static int indexOfByte(byte[] arr, byte b) {
        for (int i = 0; i < arr.length; i++) {
            if (arr[i] == b) return i;
        }
        return -1;
    }

    private static String findFirstRegexMatch(String text, Pattern pattern) {
        if (text == null) return null;
        Matcher m = pattern.matcher(text);
        if (m.find()) {
            return m.group(1);
        }
        return null;
    }

    private static String findLastRegexMatch(String text, Pattern pattern) {
        if (text == null) return null;
        Matcher m = pattern.matcher(text);
        String last = null;
        while (m.find()) {
            last = m.group(1);
        }
        return last;
    }

    private static String unescapeJson(String s) {
        if (s == null) return null;
        StringBuilder sb = new StringBuilder();
        int len = s.length();
        for (int i = 0; i < len; i++) {
            char c = s.charAt(i);
            if (c == '\\' && i + 1 < len) {
                char next = s.charAt(i + 1);
                switch (next) {
                    case '"' -> { sb.append('"'); i++; }
                    case '\\' -> { sb.append('\\'); i++; }
                    case '/' -> { sb.append('/'); i++; }
                    case 'b' -> { sb.append('\b'); i++; }
                    case 'f' -> { sb.append('\f'); i++; }
                    case 'n' -> { sb.append('\n'); i++; }
                    case 'r' -> { sb.append('\r'); i++; }
                    case 't' -> { sb.append('\t'); i++; }
                    case 'u' -> {
                        if (i + 5 < len) {
                            String hex = s.substring(i + 2, i + 6);
                            try {
                                sb.append((char) Integer.parseInt(hex, 16));
                                i += 5;
                            } catch (NumberFormatException e) {
                                sb.append(c);
                            }
                        } else {
                            sb.append(c);
                        }
                    }
                    default -> sb.append(c);
                }
            } else {
                sb.append(c);
            }
        }
        return sb.toString();
    }

    private static String getFirstLine(String text) {
        if (text == null) return "";
        int nl = text.indexOf('\n');
        if (nl < 0) return text;
        return text.substring(0, nl);
    }

    private static String getStringProp(Map<String, JsonValue> props, String key) {
        if (props == null) return null;
        JsonValue val = props.get(key);
        if (val instanceof JsonValue.JsonString s) return s.value();
        if (val instanceof JsonValue.JsonLiteral lit && !lit.isNull()) return lit.raw();
        return null;
    }

    private static Long getLongProp(Map<String, JsonValue> props, String key) {
        return getLongProp(props, key, null);
    }

    private static Long getLongProp(Map<String, JsonValue> props, String key, Long defaultVal) {
        if (props == null) return defaultVal;
        JsonValue val = props.get(key);
        if (val instanceof JsonValue.JsonLiteral lit) {
            try {
                return Long.parseLong(lit.raw());
            } catch (Exception ignored) {}
        }
        return defaultVal;
    }

    private static boolean getBooleanProp(Map<String, JsonValue> props, String key, boolean defaultVal) {
        if (props == null) return defaultVal;
        JsonValue val = props.get(key);
        if (val instanceof JsonValue.JsonLiteral lit) {
            if ("true".equals(lit.raw())) return true;
            if ("false".equals(lit.raw())) return false;
        }
        return defaultVal;
    }

    private static class RawTurn {
        int index;
        String who;
        String at;
        int chars;
        boolean truncated;
        String text;
        String toolName;
        String toolArg;
        Double toolSeconds;
        boolean toolFailed;
        String toolUseId;
        Map<String, JsonValue> toolInputProps;
    }

    private record HeadAndTail(
            String headText,
            String tailText,
            List<String> headLines,
            List<String> tailLines
    ) {}

    private record RecentSessionFile(
            Path path,
            String headText
    ) {}
}
