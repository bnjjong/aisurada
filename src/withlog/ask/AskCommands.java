package withlog.ask;

import withlog.args.JsonReader;
import withlog.args.JsonValue;
import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.out.JsonWriter;
import withlog.text.IdShape;
import withlog.text.Trim;
import withlog.vault.NoteIndex;
import withlog.vault.NoteScanner;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import java.util.regex.Pattern;

/**
 * 2장 문답 표식과 문답 파일 (get-ask, append-ask-turn) 엔진 명령 처리기.
 * 설명서 2장 및 32장에 따른 ask.json 읽기 및 원자적 쓰기를 수행합니다.
 */
public final class AskCommands {
    public static final long MAX_ASK_FILE_BYTES = 1_000_000L;
    private static final int MAX_QUOTE_CHARS = 2_000;
    private static final int MAX_TEXT_CHARS = 20_000;
    private static final int MAX_IMAGES = 8;
    private static final int MAX_ATTACHMENT_NAME_LENGTH = 120;
    private static final Set<String> ALLOWED_IMAGE_EXTS = Set.of(
            "svg", "png", "jpg", "jpeg", "gif", "webp", "avif", "bmp"
    );

    private AskCommands() {}

    /**
     * 대상 노트의 디렉토리와 id 정보를 담는 DTO.
     */
    public record NoteTarget(String noteId, Path noteDir, Path askFile, String vaultRelPath) {}

    /**
     * 자리 인자로부터 문답 대상 노트를 검증하고 찾습니다.
     */
    public static NoteTarget resolveTarget(Path vaultRoot, ParsedArgs parsed, NoteScanner.ScanResult scanResult, String commandName) {
        List<String> pos = parsed.positionalArgs();
        if (pos.isEmpty() || pos.get(0) == null || Trim.isBlank(pos.get(0))) {
            throw new WlException(400, "인자가 하나 필요하다: " + commandName + " <노트 id> [하위 노트 id]");
        }

        String rawRootId = pos.get(0);
        if (pos.size() > 1 && (pos.get(1) == null || Trim.isBlank(pos.get(1)))) {
            throw new WlException(400, "하위 노트 id 칸이 비어 있다. 노트에 붙이려면 그 칸을 빼라");
        }

        if (!IdShape.NOTE.matcher(rawRootId).matches()) {
            throw new WlException(400, "노트 id 모양이 아니다: " + rawRootId);
        }

        if (pos.size() == 1) {
            NoteScanner.NoteRecord note = scanResult.notesById().get(rawRootId);
            if (note == null) {
                throw new WlException(404, "Note not found: " + rawRootId);
            }
            if (note.isSingleFile()) {
                throw new WlException(500, "폴더로 안 올라간 단일 파일 노트에는 문답을 둘 수 없다: " + note.contentFilePath());
            }
            Path noteDir = note.contentFilePath().getParent();
            Path askFile = noteDir.resolve("ask.json");
            String rel = vaultRoot.relativize(askFile).toString().replace('\\', '/');
            return new NoteTarget(rawRootId, noteDir, askFile, rel);
        }

        String rawSubId = pos.get(1);
        if (!IdShape.SUB_NOTE.matcher(rawSubId).matches()) {
            throw new WlException(400, "하위 노트 id 모양이 아니다: " + rawSubId);
        }

        NoteScanner.NoteRecord rootNote = scanResult.notesById().get(rawRootId);
        if (rootNote == null) {
            throw new WlException(404, "Note not found: " + rawRootId);
        }

        NoteScanner.NoteRecord subNote = rootNote.subNotes().get(rawSubId);
        if (subNote == null) {
            throw new WlException(404, "SubNote not found: " + rawSubId);
        }

        if (subNote.isSingleFile()) {
            throw new WlException(500, "폴더로 안 올라간 단일 파일 하위 노트에는 문답을 둘 수 없다: " + subNote.contentFilePath());
        }

        Path noteDir = subNote.contentFilePath().getParent();
        Path askFile = noteDir.resolve("ask.json");
        String rel = vaultRoot.relativize(askFile).toString().replace('\\', '/');
        return new NoteTarget(rawSubId, noteDir, askFile, rel);
    }

    /**
     * get-ask 명령 실행.
     */
    public static Map<String, Object> executeGetAsk(Path vaultRoot, ParsedArgs parsed, NoteIndex noteIndex) {
        NoteTarget target = resolveTarget(vaultRoot, parsed, noteIndex.noteScan(), "get-ask");
        if (!Files.exists(target.askFile)) {
            return null; // stdout: null
        }

        return readAskJson(target.askFile, target.noteId);
    }

    /**
     * ask.json 파일을 규격에 맞게 너그럽게 읽습니다.
     */
    @SuppressWarnings("unchecked")
    public static Map<String, Object> readAskJson(Path askFile, String fallbackNoteId) {
        try {
            long size = Files.size(askFile);
            if (size > MAX_ASK_FILE_BYTES) {
                throw new WlException(500, "문답 파일을 못 읽었다: 파일이 너무 크다(" + size + "바이트). 상한은 1000000바이트다.");
            }
        } catch (IOException e) {
            throw new WlException(500, "문답 파일을 못 읽었다: " + e.getMessage());
        }

        String content;
        try {
            content = Files.readString(askFile, StandardCharsets.UTF_8);
        } catch (IOException e) {
            throw new WlException(500, "문답 파일을 못 읽었다: " + e.getMessage());
        }

        JsonValue parsedVal;
        try {
            parsedVal = JsonReader.parseFile(content);
        } catch (Exception e) {
            throw new WlException(500, "문답 파일을 못 읽었다: " + e.getMessage());
        }

        if (!(parsedVal instanceof JsonValue.JsonObject)) {
            throw new WlException(500, "문답 파일을 못 읽었다: 최상위가 객체가 아니다");
        }

        Map<String, Object> rawMap = (Map<String, Object>) toJavaValue(parsedVal);

        Map<String, Object> result = new LinkedHashMap<>();
        Object rawNoteId = rawMap.get("noteId");
        String noteId = (rawNoteId instanceof String s && !s.isEmpty()) ? s : fallbackNoteId;
        result.put("noteId", noteId);

        Object rawSessionId = rawMap.get("sessionId");
        String sessionId = (rawSessionId instanceof String s) ? s : "";
        result.put("sessionId", sessionId);

        List<Map<String, Object>> turns = new ArrayList<>();
        Object rawTurns = rawMap.get("turns");
        if (rawTurns instanceof List<?> rawList) {
            for (Object item : rawList) {
                if (item instanceof Map<?, ?> turnMap) {
                    Map<String, Object> turn = sanitizeTurn(turnMap);
                    turns.add(turn);
                }
            }
        }
        result.put("turns", turns);
        return result;
    }

    private static Map<String, Object> sanitizeTurn(Map<?, ?> turnMap) {
        Map<String, Object> t = new LinkedHashMap<>();

        // id
        Object idVal = turnMap.get("id");
        t.put("id", idVal instanceof String s ? s : "");

        // markId
        Object markIdVal = turnMap.get("markId");
        if (markIdVal instanceof String s && !s.isEmpty()) {
            t.put("markId", s);
        }

        // askedAt
        Object askedAtVal = turnMap.get("askedAt");
        t.put("askedAt", askedAtVal instanceof String s ? s : "");

        // quote
        Object quoteVal = turnMap.get("quote");
        if (quoteVal instanceof String s && !s.isEmpty()) {
            t.put("quote", s);
        }

        // question
        Object qVal = turnMap.get("question");
        t.put("question", qVal instanceof String s ? s : "");

        // answer
        Object aVal = turnMap.get("answer");
        t.put("answer", aVal instanceof String s ? s : "");

        // error
        Object errVal = turnMap.get("error");
        if (errVal instanceof String s && !s.isEmpty()) {
            t.put("error", s);
        }

        // model
        Object mVal = turnMap.get("model");
        t.put("model", mVal instanceof String s ? s : "");

        // effort
        Object effVal = turnMap.get("effort");
        t.put("effort", effVal instanceof String s ? s : "");

        // elapsedMs
        Object elVal = turnMap.get("elapsedMs");
        long elapsed = 0;
        if (elVal instanceof Number n) {
            elapsed = n.longValue();
        } else if (elVal instanceof String s) {
            try { elapsed = Long.parseLong(s); } catch (Exception ignored) {}
        }
        t.put("elapsedMs", elapsed);

        // sessionLines
        Object slVal = turnMap.get("sessionLines");
        if (slVal instanceof Number n && n.intValue() > 0) {
            t.put("sessionLines", n.intValue());
        }

        // images
        Object imgVal = turnMap.get("images");
        if (imgVal instanceof List<?> imgList) {
            List<String> validImages = new ArrayList<>();
            for (Object elem : imgList) {
                if (elem instanceof String s && s.startsWith("attach/")) {
                    String name = s.substring("attach/".length());
                    if (isValidAttachmentName(name)) {
                        validImages.add(s);
                    }
                }
            }
            if (!validImages.isEmpty()) {
                t.put("images", validImages);
            }
        }

        return t;
    }

    private static Object toJavaValue(JsonValue val) {
        if (val == null) return null;
        if (val instanceof JsonValue.JsonString s) return s.value();
        if (val instanceof JsonValue.JsonArray arr) {
            List<Object> list = new ArrayList<>();
            for (JsonValue elem : arr.elements()) {
                list.add(toJavaValue(elem));
            }
            return list;
        }
        if (val instanceof JsonValue.JsonObject obj) {
            Map<String, Object> map = new LinkedHashMap<>();
            for (Map.Entry<String, JsonValue> entry : obj.properties().entrySet()) {
                map.put(entry.getKey(), toJavaValue(entry.getValue()));
            }
            return map;
        }
        if (val instanceof JsonValue.JsonLiteral lit) {
            String raw = lit.raw();
            if ("null".equals(raw)) return null;
            if ("true".equals(raw)) return Boolean.TRUE;
            if ("false".equals(raw)) return Boolean.FALSE;
            try { return Long.parseLong(raw); } catch (Exception ignored) {}
            try { return Double.parseDouble(raw); } catch (Exception ignored) {}
            return raw;
        }
        return null;
    }

    private static boolean isValidAttachmentName(String name) {
        if (name == null || name.isEmpty() || name.length() > MAX_ATTACHMENT_NAME_LENGTH) return false;
        if (name.startsWith(".") || name.contains("/") || name.contains("\\") || name.contains("..")) return false;
        for (char c : name.toCharArray()) {
            if (c < 32 || c == 127) return false;
        }
        int dot = name.lastIndexOf('.');
        if (dot < 0) return false;
        String ext = name.substring(dot + 1).toLowerCase(Locale.ROOT);
        return ALLOWED_IMAGE_EXTS.contains(ext);
    }

    /**
     * append-ask-turn 명령 실행.
     */
    public static Map<String, Object> executeAppendAskTurn(Path vaultRoot, ParsedArgs parsed, NoteIndex noteIndex) {
        NoteTarget target = resolveTarget(vaultRoot, parsed, noteIndex.noteScan(), "append-ask-turn");
        Map<String, String> opts = parsed.namedOptions();

        // 1. 필수 인자 검사
        String sessionId = requireOption(opts, "session-id");
        String turnId = requireOption(opts, "turn-id");
        String markId = requireOption(opts, "mark-id");
        String askedAt = requireOption(opts, "asked-at");
        String question = requireOptionRaw(opts, "question");
        String answer = requireOptionRaw(opts, "answer");
        String model = requireOption(opts, "model");
        String effort = requireOption(opts, "effort");
        String rawElapsed = requireOption(opts, "elapsed-ms");

        long elapsedMs;
        try {
            elapsedMs = Long.parseLong(rawElapsed);
            if (elapsedMs < 0) throw new NumberFormatException();
        } catch (Exception e) {
            throw new WlException(400, "elapsed-ms 은 숫자여야 한다: " + rawElapsed);
        }

        if (question.length() > MAX_TEXT_CHARS) {
            throw new WlException(400, "질문이 상한을 넘었다: " + question.length() + "자. 상한은 20000자다.");
        }
        if (answer.length() > MAX_TEXT_CHARS) {
            throw new WlException(400, "답변이 상한을 넘었다: " + answer.length() + "자. 상한은 20000자다.");
        }

        String quote = opts.get("quote");
        if (quote != null && quote.length() > MAX_QUOTE_CHARS) {
            throw new WlException(400, "인용이 상한을 넘었다: " + quote.length() + "자. 상한은 2000자다.");
        }

        String error = opts.get("error");

        Integer sessionLines = null;
        String rawSessionLines = opts.get("session-lines");
        if (rawSessionLines != null) {
            try {
                int lines = Integer.parseInt(rawSessionLines.trim());
                if (lines < 1) throw new NumberFormatException();
                sessionLines = lines;
            } catch (Exception e) {
                throw new WlException(400, "session-lines 은 1 이상이어야 한다: " + rawSessionLines);
            }
        }

        List<String> imageList = null;
        String rawImages = opts.get("images");
        if (rawImages != null) {
            String[] parts = rawImages.split(",");
            List<String> valid = new ArrayList<>();
            for (String p : parts) {
                String trimmed = p.trim();
                if (!trimmed.isEmpty()) {
                    valid.add(trimmed);
                }
            }
            if (valid.size() > MAX_IMAGES) {
                throw new WlException(400, "그림은 한 물음에 8장까지다: " + valid.size() + "장");
            }
            for (String img : valid) {
                if (!img.startsWith("attach/")) {
                    throw new WlException(400, "그림 자리가 attach/<그림 파일 이름> 꼴이 아니다: " + img);
                }
                String name = img.substring("attach/".length());
                if (!isValidAttachmentName(name)) {
                    throw new WlException(400, "그림 자리가 attach/<그림 파일 이름> 꼴이 아니다: " + img);
                }
            }
            if (!valid.isEmpty()) {
                imageList = valid;
            }
        }

        // 기존 ask.json 읽기 또는 새 구조체 생성
        Map<String, Object> currentThread;
        if (Files.exists(target.askFile)) {
            currentThread = readAskJson(target.askFile, target.noteId);
        } else {
            currentThread = new LinkedHashMap<>();
            currentThread.put("v", 1);
            currentThread.put("noteId", target.noteId);
            currentThread.put("sessionId", sessionId);
            currentThread.put("turns", new ArrayList<Map<String, Object>>());
        }

        @SuppressWarnings("unchecked")
        List<Map<String, Object>> turns = (List<Map<String, Object>>) currentThread.computeIfAbsent("turns", k -> new ArrayList<>());

        Map<String, Object> newTurn = new LinkedHashMap<>();
        newTurn.put("id", turnId);
        newTurn.put("markId", markId);
        newTurn.put("askedAt", askedAt);
        if (quote != null) {
            newTurn.put("quote", quote);
        }
        newTurn.put("question", question);
        newTurn.put("answer", answer);
        if (error != null) {
            newTurn.put("error", error);
        }
        newTurn.put("model", model);
        newTurn.put("effort", effort);
        newTurn.put("elapsedMs", elapsedMs);
        if (sessionLines != null) {
            newTurn.put("sessionLines", sessionLines);
        }
        if (imageList != null && !imageList.isEmpty()) {
            newTurn.put("images", imageList);
        }

        turns.add(newTurn);

        // 파일 저장용 직렬화 객체 (v=1 포함)
        Map<String, Object> fileMap = new LinkedHashMap<>();
        fileMap.put("v", 1);
        fileMap.put("noteId", currentThread.get("noteId"));
        fileMap.put("sessionId", currentThread.get("sessionId"));
        fileMap.put("turns", turns);

        byte[] fileBytes = JsonWriter.toFileBytes(fileMap);

        // D1 고침 (cc12691b): 1,000,000 바이트 초과 사전 검사
        if (fileBytes.length > MAX_ASK_FILE_BYTES) {
            throw new WlException(500, "문답 파일이 상한을 넘어 붙일 수 없다: 붙이면 " + fileBytes.length + "바이트다. 상한은 1000000바이트다.");
        }

        // 원자적 파일 쓰기
        atomicWrite(target.askFile, fileBytes);

        // 반환 응답 객체 (noteId, sessionId, turns, path)
        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("noteId", currentThread.get("noteId"));
        resp.put("sessionId", currentThread.get("sessionId"));
        resp.put("turns", turns);
        resp.put("path", target.vaultRelPath);
        return resp;
    }

    private static String requireOption(Map<String, String> opts, String key) {
        String val = opts.get(key);
        if (val == null || val.trim().isEmpty()) {
            throw new WlException(400, key + " 이 필요하다");
        }
        return val.trim();
    }

    private static String requireOptionRaw(Map<String, String> opts, String key) {
        String val = opts.get(key);
        if (val == null) {
            throw new WlException(400, key + " 이 필요하다");
        }
        return val;
    }

    private static void atomicWrite(Path targetFile, byte[] data) {
        Path parent = targetFile.getParent();
        if (parent != null && !Files.exists(parent)) {
            try {
                Files.createDirectories(parent);
            } catch (IOException e) {
                throw new WlException(500, "폴더를 만들지 못했다: " + parent);
            }
        }
        Path tmp = parent.resolve(targetFile.getFileName() + "." + UUID.randomUUID() + ".tmp");
        try {
            Files.write(tmp, data, StandardOpenOption.CREATE, StandardOpenOption.TRUNCATE_EXISTING, StandardOpenOption.WRITE);
            try {
                Files.move(tmp, targetFile, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
            } catch (AtomicMoveNotSupportedException e) {
                Files.move(tmp, targetFile, StandardCopyOption.REPLACE_EXISTING);
            }
        } catch (IOException e) {
            try { Files.deleteIfExists(tmp); } catch (IOException ignored) {}
            throw new WlException(500, "문답 파일 쓰기 실패: " + e.getMessage());
        }
    }
}
