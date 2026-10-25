package withlog.serve;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.AtomicMoveNotSupportedException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.nio.file.StandardOpenOption;
import java.time.Instant;
import java.time.format.DateTimeParseException;
import java.util.List;

/**
 * serve 모드 엔진의 실행 상태 파일(run/<VaultKey>.json)을 쓰고 읽습니다.
 * 설명서 3.2 및 3.5에 따라 줄 단위 파싱과 원자적 교체를 지원합니다.
 */
public final class StateFile {
    private final Long pid;
    private final String vault;
    private final String socket;
    private final Instant startedAt;
    private final Instant lastActiveAt;
    private final Integer openConnections;
    private final String builtCommit;

    public StateFile(Long pid, String vault, String socket, Instant startedAt, Instant lastActiveAt, Integer openConnections, String builtCommit) {
        this.pid = pid;
        this.vault = vault;
        this.socket = socket;
        this.startedAt = startedAt;
        this.lastActiveAt = lastActiveAt != null ? lastActiveAt.truncatedTo(java.time.temporal.ChronoUnit.MILLIS) : null;
        this.openConnections = openConnections;
        this.builtCommit = builtCommit;
    }

    public Long getPid() { return pid; }
    public String getVault() { return vault; }
    public String getSocket() { return socket; }
    public Instant getStartedAt() { return startedAt; }
    public Instant getLastActiveAt() { return lastActiveAt; }
    public Integer getOpenConnections() { return openConnections; }
    public String getBuiltCommit() { return builtCommit; }

    /**
     * 실행 상태 파일 JSON 문자열을 생성합니다.
     */
    public String toJsonString() {
        StringBuilder sb = new StringBuilder();
        sb.append("{\n");
        sb.append("  \"pid\": ").append(pid != null ? pid : "null").append(",\n");
        sb.append("  \"vault\": ");
        if (vault != null) escapeJsonString(sb, vault); else sb.append("null");
        sb.append(",\n");

        sb.append("  \"socket\": ");
        if (socket != null) escapeJsonString(sb, socket); else sb.append("null");
        sb.append(",\n");

        sb.append("  \"startedAt\": ");
        if (startedAt != null) escapeJsonString(sb, startedAt.toString()); else sb.append("null");
        sb.append(",\n");

        sb.append("  \"lastActiveAt\": ");
        if (lastActiveAt != null) escapeJsonString(sb, lastActiveAt.truncatedTo(java.time.temporal.ChronoUnit.MILLIS).toString()); else sb.append("null");
        sb.append(",\n");

        sb.append("  \"openConnections\": ").append(openConnections != null ? openConnections : "null").append(",\n");

        sb.append("  \"builtCommit\": ");
        if (builtCommit != null) escapeJsonString(sb, builtCommit); else sb.append("null");
        sb.append("\n}\n");

        return sb.toString();
    }

    private static void escapeJsonString(StringBuilder sb, String str) {
        sb.append('"');
        for (int i = 0; i < str.length(); i++) {
            char c = str.charAt(i);
            switch (c) {
                case '"' -> sb.append("\\\"");
                case '\\' -> sb.append("\\\\");
                case '\b' -> sb.append("\\b");
                case '\t' -> sb.append("\\t");
                case '\n' -> sb.append("\\n");
                case '\f' -> sb.append("\\f");
                case '\r' -> sb.append("\\r");
                default -> {
                    if (c < 0x20) {
                        sb.append(String.format("\\u%04x", (int) c));
                    } else {
                        sb.append(c);
                    }
                }
            }
        }
        sb.append('"');
    }

    /**
     * 상태 파일을 .tmp를 거쳐 원자적으로 작성합니다. 내용이 지난번과 같으면 다시 쓰지 않습니다.
     * @return 쓴 경우 새 JSON 문자열, 안 쓴 경우 null
     */
    public static String writeIfChanged(Path runDir, String vaultKey, StateFile state, String lastWrittenJson) throws IOException {
        String json = state.toJsonString();
        if (json.equals(lastWrittenJson)) {
            return null; // 변경 없음
        }
        Path tmpFile = runDir.resolve(vaultKey + ".json.tmp");
        Path targetFile = runDir.resolve(vaultKey + ".json");

        Files.createDirectories(runDir);
        Files.writeString(tmpFile, json, StandardCharsets.UTF_8,
                StandardOpenOption.CREATE, StandardOpenOption.TRUNCATE_EXISTING, StandardOpenOption.WRITE);
        try {
            Files.move(tmpFile, targetFile, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
        } catch (AtomicMoveNotSupportedException e) {
            Files.move(tmpFile, targetFile, StandardCopyOption.REPLACE_EXISTING);
        }
        return json;
    }

    /**
     * 상태 파일을 줄 단위로 읽습니다. (3.2 읽는 규칙 표 구현)
     */
    public static StateFile read(Path file) {
        if (!Files.isRegularFile(file)) {
            return null;
        }
        List<String> lines;
        try {
            lines = Files.readAllLines(file, StandardCharsets.UTF_8);
        } catch (Exception e) {
            return new StateFile(null, null, null, null, null, null, null);
        }

        Long pid = null;
        String vault = null;
        String socket = null;
        Instant startedAt = null;
        Instant lastActiveAt = null;
        Integer openConnections = null;
        String builtCommit = null;

        for (String rawLine : lines) {
            String line = rawLine.stripLeading();
            if (line.startsWith("\"pid\"")) {
                pid = parseLongField(line);
            } else if (line.startsWith("\"vault\"")) {
                vault = parseStringField(line);
            } else if (line.startsWith("\"socket\"")) {
                socket = parseStringField(line);
            } else if (line.startsWith("\"startedAt\"")) {
                startedAt = parseInstantField(line);
            } else if (line.startsWith("\"lastActiveAt\"")) {
                lastActiveAt = parseInstantField(line);
            } else if (line.startsWith("\"openConnections\"")) {
                openConnections = parseIntField(line);
            } else if (line.startsWith("\"builtCommit\"")) {
                builtCommit = parseStringField(line);
            }
        }

        return new StateFile(pid, vault, socket, startedAt, lastActiveAt, openConnections, builtCommit);
    }

    private static String extractValueText(String line) {
        int colon = line.indexOf(':');
        if (colon < 0) return null;
        String val = line.substring(colon + 1).strip();
        if (val.endsWith(",")) {
            val = val.substring(0, val.length() - 1).strip();
        }
        return val;
    }

    private static String parseStringField(String line) {
        String val = extractValueText(line);
        if (val == null || !val.startsWith("\"") || !val.endsWith("\"") || val.length() < 2) {
            return null;
        }
        String inner = val.substring(1, val.length() - 1);
        // 안쪽의 \"를 "로, 그다음 \\를 \로 되돌린다. 그 밖의 탈출(\n, u00xx)은 글자 그대로 남는다
        inner = inner.replace("\\\"", "\"");
        inner = inner.replace("\\\\", "\\");
        return inner;
    }

    private static Long parseLongField(String line) {
        String val = extractValueText(line);
        if (val == null || val.startsWith("\"")) {
            return null;
        }
        try {
            return Long.parseLong(val);
        } catch (NumberFormatException e) {
            return null;
        }
    }

    private static Integer parseIntField(String line) {
        String val = extractValueText(line);
        if (val == null || val.startsWith("\"")) {
            return null;
        }
        try {
            return Integer.parseInt(val);
        } catch (NumberFormatException e) {
            return null;
        }
    }

    private static Instant parseInstantField(String line) {
        String val = parseStringField(line);
        if (val == null) return null;
        try {
            return Instant.parse(val);
        } catch (DateTimeParseException e) {
            return null;
        }
    }
}
