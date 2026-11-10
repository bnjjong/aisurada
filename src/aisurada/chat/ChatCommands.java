package aisurada.chat;

import aisurada.args.JsonReader;
import aisurada.args.JsonValue;
import aisurada.args.ParsedArgs;
import aisurada.cli.AisException;
import aisurada.out.JsonWriter;
import aisurada.text.IdShape;
import aisurada.text.Trim;
import aisurada.vault.NoteIndex;
import aisurada.vault.NoteScanner;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.time.Instant;
import java.time.OffsetDateTime;
import java.time.format.DateTimeParseException;
import java.util.*;
import java.util.regex.Pattern;

/**
 * 8장 노트 옆 세션 레일과 레일 대화 (list-chats, append-chat-turn) 엔진 명령 처리기.
 * 설명서 8장 및 32장에 따른 chats/<대화id>.json 읽기, 목록 정렬 및 원자적 쓰기를 수행합니다.
 */
public final class ChatCommands {
    public static final long MAX_CHAT_FILE_BYTES = 1_000_000L;
    private static final int MAX_TEXT_CHARS = 20_000;
    private static final int MAX_IMAGES = 8;
    private static final Pattern CHAT_ID_PATTERN = Pattern.compile("^[A-Za-z0-9_-]{1,64}$");

    private ChatCommands() {}

    /**
     * 대상 노트의 디렉토리와 id 정보를 담는 DTO.
     */
    public record ChatTarget(String noteId, Path noteDir, Path chatsDir) {}

    /**
     * 자리 인자로부터 레일 대화 대상 노트를 검증하고 찾습니다.
     *
     * @param vaultRoot 워크스페이스 루트 경로
     * @param parsed 파싱된 CLI 인자
     * @param scanResult 노트 색인 스캔 결과
     * @param commandName 실행 중인 명령 이름
     * @return 검증된 대상 노 정보
     */
    public static ChatTarget resolveTarget(Path vaultRoot, ParsedArgs parsed, NoteScanner.ScanResult scanResult, String commandName) {
        return locateTarget(vaultRoot, checkPositionalIds(parsed, commandName), scanResult);
    }

    /**
     * 자리 인자의 있고 없음과 id 모양만 검사합니다. 문답 파일 쪽과 같은 규칙이고 색인을 안 봅니다.
     */
    public static String[] checkPositionalIds(ParsedArgs parsed, String commandName) {
        return aisurada.ask.AskCommands.checkPositionalIds(parsed, commandName);
    }

    /**
     * 검사를 마친 id로 색인에서 대상 노트의 폴더와 chats 폴더 자리를 찾습니다.
     */
    public static ChatTarget locateTarget(Path vaultRoot, String[] ids, NoteScanner.ScanResult scanResult) {
        String rawRootId = ids[0];
        String rawSubId = ids[1];

        if (rawSubId == null) {
            NoteScanner.NoteRecord note = scanResult.notesById().get(rawRootId);
            if (note == null) {
                throw new AisException(404, "Note not found: " + rawRootId);
            }
            if (note.isSingleFile()) {
                throw new AisException(500, "폴더로 안 올라간 단일 파일 노트에는 레일 대화를 둘 수 없다: " + note.contentFilePath());
            }
            Path noteDir = note.contentFilePath().getParent();
            Path chatsDir = noteDir.resolve("chats");
            return new ChatTarget(rawRootId, noteDir, chatsDir);
        }

        NoteScanner.NoteRecord rootNote = scanResult.notesById().get(rawRootId);
        if (rootNote == null) {
            throw new AisException(404, "Note not found: " + rawRootId);
        }

        NoteScanner.NoteRecord subNote = rootNote.subNotes().get(rawSubId);
        if (subNote == null) {
            throw new AisException(404, "SubNote not found: " + rawSubId);
        }

        if (subNote.isSingleFile()) {
            throw new AisException(500, "폴더로 안 올라간 단일 파일 하위 노트에는 레일 대화를 둘 수 없다: " + subNote.contentFilePath());
        }

        Path noteDir = subNote.contentFilePath().getParent();
        Path chatsDir = noteDir.resolve("chats");
        return new ChatTarget(rawSubId, noteDir, chatsDir);
    }

    /**
     * list-chats 명령 실행.
     *
     * @param vaultRoot 워크스페이스 루트 경로
     * @param parsed 파싱된 CLI 인자
     * @param noteIndex 노트 색인
     * @return chats 목록과 unreadable 파일명 목록을 담은 Map
     */
    public static Map<String, Object> executeListChats(Path vaultRoot, ParsedArgs parsed, NoteIndex noteIndex) {
        ChatTarget target = resolveTarget(vaultRoot, parsed, noteIndex.noteScan(), "list-chats");

        List<Map<String, Object>> chats = new ArrayList<>();
        List<String> unreadable = new ArrayList<>();

        if (!Files.exists(target.chatsDir)) {
            Map<String, Object> resp = new LinkedHashMap<>();
            resp.put("chats", chats);
            resp.put("unreadable", unreadable);
            return resp;
        }

        if (!Files.isDirectory(target.chatsDir)) {
            throw new AisException(500, "chats 경로가 디렉토리가 아니다: " + target.chatsDir);
        }

        try (DirectoryStream<Path> stream = Files.newDirectoryStream(target.chatsDir)) {
            for (Path entry : stream) {
                // 심볼릭 링크는 따라가지 않는다
                if (Files.isSymbolicLink(entry)) {
                    continue;
                }
                if (!Files.isRegularFile(entry, LinkOption.NOFOLLOW_LINKS)) {
                    continue;
                }

                String filename = entry.getFileName().toString();
                if (!filename.endsWith(".json")) {
                    continue;
                }
                String baseChatId = filename.substring(0, filename.length() - ".json".length());
                if (!CHAT_ID_PATTERN.matcher(baseChatId).matches()) {
                    continue;
                }

                try {
                    long size = Files.size(entry);
                    if (size > MAX_CHAT_FILE_BYTES) {
                        unreadable.add(filename);
                        continue;
                    }

                    Map<String, Object> chat = readChatFile(entry, target.noteId, baseChatId);
                    chats.add(chat);
                } catch (Exception e) {
                    unreadable.add(filename);
                }
            }
        } catch (IOException e) {
            throw new AisException(500, "chats 폴더를 읽지 못했다: " + e.getMessage());
        }

        // 정렬: createdAt 시각 오름차순, 같으면 chatId 오름차순
        chats.sort((c1, c2) -> {
            String t1 = (String) c1.get("createdAt");
            String t2 = (String) c2.get("createdAt");
            int timeCmp = compareCreatedAt(t1, t2);
            if (timeCmp != 0) {
                return timeCmp;
            }
            String id1 = (String) c1.get("chatId");
            String id2 = (String) c2.get("chatId");
            return Objects.compare(id1, id2, Comparator.nullsLast(Comparator.naturalOrder()));
        });

        // unreadable 이름순 정렬
        Collections.sort(unreadable);

        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("chats", chats);
        resp.put("unreadable", unreadable);
        return resp;
    }

    /**
     * append-chat-turn 명령 실행.
     *
     * @param vaultRoot 워크스페이스 루트 경로
     * @param parsed 파싱된 CLI 인자
     * @param noteIndex 노트 색인
     * @return 갱신된 chat 객체와 vault 상대 경로를 담은 Map
     */
    public static Map<String, Object> executeAppendChatTurn(Path vaultRoot, ParsedArgs parsed, NoteIndex noteIndex) {
        // 자리 인자 모양, 이름 인자 순으로 먼저 재고 나서야 색인에서 노트를 찾는다(8.13의 붙이는 차례)
        String[] ids = checkPositionalIds(parsed, "append-chat-turn");
        Map<String, String> opts = parsed.namedOptions();

        String chatId = requireTrimmed(opts, "chat-id", "chat-id 이 필요하다");
        if (!CHAT_ID_PATTERN.matcher(chatId).matches()) {
            throw new AisException(400, "대화 id 모양이 아니다: " + chatId);
        }

        String sourceSession = requireTrimmed(opts, "source-session", "source-session 이 필요하다");
        if (sourceSession.length() > 128) {
            throw new AisException(400, "source-session 은 128자 이하여야 한다");
        }

        String turnId = requireTrimmed(opts, "turn-id", "turn-id 이 필요하다");
        String askedAt = requireTrimmed(opts, "asked-at", "asked-at 이 필요하다");

        String question = requireRaw(opts, "question", "question 이 필요하다");
        if (question.length() > MAX_TEXT_CHARS) {
            throw new AisException(400, "질문은 " + MAX_TEXT_CHARS + "자 이하여야 한다");
        }

        String answer = requireRaw(opts, "answer", "answer 가 필요하다");
        if (answer.length() > MAX_TEXT_CHARS) {
            throw new AisException(400, "답은 " + MAX_TEXT_CHARS + "자 이하여야 한다");
        }

        String error = opts.get("error"); // raw 유지

        String model = requireTrimmed(opts, "model", "model 이 필요하다");
        String effort = requireTrimmed(opts, "effort", "effort 이 필요하다");

        String elapsedMsStr = requireTrimmed(opts, "elapsed-ms", "elapsed-ms 가 필요하다");
        int elapsedMs;
        try {
            // 정수는 32비트다. 문답 파일의 elapsed-ms 와 같은 범위다(2.7)
            elapsedMs = Integer.parseInt(elapsedMsStr);
            if (elapsedMs < 0) {
                throw new AisException(400, "elapsed-ms 는 0 이상의 정수여야 한다");
            }
        } catch (NumberFormatException e) {
            throw new AisException(400, "elapsed-ms 는 0 이상의 정수여야 한다");
        }

        List<String> imageList = null;
        String rawImages = opts.get("images");
        if (rawImages != null && !Trim.isBlank(rawImages)) {
            // 쉼표로 가르고 조각마다 앞뒤 공백을 걷고 빈 조각을 버린 뒤에 센다(2.7)
            List<String> valid = new ArrayList<>();
            for (String p : rawImages.split(",", -1)) {
                String img = Trim.trim(p);
                if (!img.isEmpty()) valid.add(img);
            }
            if (valid.size() > MAX_IMAGES) {
                throw new AisException(400, "그림 첨부는 " + MAX_IMAGES + "개까지만 가능하다");
            }
            for (String img : valid) {
                if (!img.startsWith("attach/")) {
                    throw new AisException(400, "그림 자리가 attach/<그림 파일 이름> 꼴이 아니다: " + img);
                }
                String name = img.substring("attach/".length());
                if (!isValidAttachmentName(name)) {
                    throw new AisException(400, "그림 자리가 attach/<그림 파일 이름> 꼴이 아니다: " + img);
                }
            }
            if (!valid.isEmpty()) {
                imageList = valid;
            }
        }

        // 인자가 다 맞은 뒤에야 노트를 찾는다. 없으면 AIS-404, 단일 파일 노트면 AIS-500
        ChatTarget target = locateTarget(vaultRoot, ids, noteIndex.noteScan());

        Path chatFile = target.chatsDir.resolve(chatId + ".json");
        String vaultRelPath = vaultRoot.relativize(chatFile).toString().replace('\\', '/');

        Map<String, Object> currentChat;
        if (Files.exists(chatFile)) {
            long size;
            try {
                size = Files.size(chatFile);
            } catch (IOException e) {
                throw new AisException(500, "레일 대화 파일을 못 읽어 붙일 수 없다: " + e.getMessage());
            }
            if (size > MAX_CHAT_FILE_BYTES) {
                throw new AisException(500, "레일 대화 파일을 못 읽어 붙일 수 없다: 파일이 상한을 넘었다: " + size + "바이트");
            }

            try {
                currentChat = readChatFile(chatFile, target.noteId, chatId);
            } catch (Exception e) {
                throw new AisException(500, "레일 대화 파일을 못 읽어 붙일 수 없다: " + e.getMessage());
            }
        } else {
            currentChat = new LinkedHashMap<>();
            currentChat.put("noteId", target.noteId);
            currentChat.put("chatId", chatId);
            currentChat.put("sourceSessionId", sourceSession);
            currentChat.put("createdAt", askedAt);
            currentChat.put("turns", new ArrayList<Map<String, Object>>());
        }

        @SuppressWarnings("unchecked")
        List<Map<String, Object>> turns = (List<Map<String, Object>>) currentChat.computeIfAbsent("turns", k -> new ArrayList<>());

        Map<String, Object> newTurn = new LinkedHashMap<>();
        newTurn.put("id", turnId);
        newTurn.put("askedAt", askedAt);
        newTurn.put("question", question);
        newTurn.put("answer", answer);
        if (error != null) {
            newTurn.put("error", error); // 빈 error= 도 "error": "" 로 쓴다. 읽을 때 빈 글은 없는 칸이다
        }
        newTurn.put("model", model);
        newTurn.put("effort", effort);
        newTurn.put("elapsedMs", elapsedMs);
        if (imageList != null && !imageList.isEmpty()) {
            newTurn.put("images", imageList);
        }

        turns.add(newTurn);

        // 파일 저장용 맵 (v=1 포함)
        Map<String, Object> fileMap = new LinkedHashMap<>();
        fileMap.put("v", 1);
        fileMap.put("noteId", currentChat.get("noteId"));
        fileMap.put("chatId", chatId);
        fileMap.put("sourceSessionId", currentChat.get("sourceSessionId"));
        fileMap.put("createdAt", currentChat.get("createdAt"));
        fileMap.put("turns", turns);

        byte[] fileBytes = JsonWriter.toFileBytes(fileMap);

        // D19 결함 고침 (cc12691b): 1,000,000 바이트 초과 사전 검사
        if (fileBytes.length > MAX_CHAT_FILE_BYTES) {
            throw new AisException(500, "레일 대화 파일이 상한을 넘어 붙일 수 없다: 붙이면 " + fileBytes.length + "바이트다. 상한은 1000000바이트다: " + vaultRelPath);
        }

        // 원자적 파일 쓰기
        atomicWrite(chatFile, fileBytes);

        // 반환 응답 객체 (chat, path)
        Map<String, Object> respChat = new LinkedHashMap<>();
        respChat.put("noteId", currentChat.get("noteId"));
        respChat.put("chatId", chatId);
        respChat.put("sourceSessionId", currentChat.get("sourceSessionId"));
        respChat.put("createdAt", currentChat.get("createdAt"));
        respChat.put("turns", turns);

        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("chat", respChat);
        resp.put("path", vaultRelPath);
        return resp;
    }

    /**
     * chats/<대화id>.json 파일을 규격에 맞게 너그럽게 읽습니다.
     * 최상위 v는 반환 객체에서 제외됩니다.
     */
    @SuppressWarnings("unchecked")
    public static Map<String, Object> readChatFile(Path chatFile, String fallbackNoteId, String fileChatId) throws IOException {
        // 깨진 UTF-8 바이트는 U+FFFD로 바꿔 읽는다. 바이트 하나로 대화 전체를 못 읽는 파일로 치지 않는다(8.11)
        String content = new String(Files.readAllBytes(chatFile), StandardCharsets.UTF_8);
        JsonValue parsed = JsonReader.parseFile(content);
        if (!(parsed instanceof JsonValue.JsonObject)) {
            throw new AisException(500, "최상위 객체가 아니다");
        }

        Map<String, Object> root = (Map<String, Object>) toJavaValue(parsed);
        if (root == null) {
            throw new AisException(500, "최상위 객체가 아니다");
        }

        String noteId = fallbackNoteId;
        Object rawNoteId = root.get("noteId");
        if (rawNoteId instanceof String s && !Trim.isBlank(s)) {
            noteId = s;
        }

        String chatId = fileChatId; // 파일 이름에서 온 id가 이김

        String sourceSessionId = "";
        Object rawSessionId = root.get("sourceSessionId");
        if (rawSessionId instanceof String s) {
            sourceSessionId = s;
        }

        List<Map<String, Object>> turns = new ArrayList<>();
        Object rawTurns = root.get("turns");
        if (rawTurns instanceof List<?> list) {
            for (Object item : list) {
                if (item instanceof Map<?, ?> m) {
                    Map<String, Object> turn = new LinkedHashMap<>();
                    turn.put("id", m.get("id") instanceof String s ? s : "");
                    turn.put("askedAt", m.get("askedAt") instanceof String s ? s : "");
                    turn.put("question", m.get("question") instanceof String s ? s : "");
                    turn.put("answer", m.get("answer") instanceof String s ? s : "");

                    Object err = m.get("error");
                    if (err instanceof String s && !s.isEmpty()) {
                        turn.put("error", s);
                    }

                    turn.put("model", m.get("model") instanceof String s ? s : "");
                    turn.put("effort", m.get("effort") instanceof String s ? s : "");

                    long elapsedMs = 0L;
                    Object el = m.get("elapsedMs");
                    if (el instanceof Long num) {
                        elapsedMs = num; // 7600.5 와 1e3 는 정수 글자가 아니라 수로 안 읽는다
                    } else if (el instanceof String str) {
                        try { elapsedMs = Long.parseLong(str); } catch (NumberFormatException ignored) {}
                    }
                    turn.put("elapsedMs", elapsedMs);

                    Object imgs = m.get("images");
                    if (imgs instanceof List<?> imgList) {
                        List<String> validImgs = new ArrayList<>();
                        for (Object imgObj : imgList) {
                            if (imgObj instanceof String is && is.startsWith("attach/")) {
                                String name = is.substring("attach/".length());
                                if (isValidAttachmentName(name)) {
                                    validImgs.add(is);
                                }
                            }
                        }
                        if (!validImgs.isEmpty()) {
                            turn.put("images", validImgs);
                        }
                    }

                    turns.add(turn);
                }
            }
        }

        String createdAt = "";
        Object rawCreatedAt = root.get("createdAt");
        if (rawCreatedAt instanceof String s && !Trim.isBlank(s)) {
            createdAt = s;
        } else if (!turns.isEmpty()) {
            String firstAskedAt = (String) turns.get(0).get("askedAt");
            if (firstAskedAt != null) {
                createdAt = firstAskedAt;
            }
        }

        Map<String, Object> result = new LinkedHashMap<>();
        result.put("noteId", noteId);
        result.put("chatId", chatId);
        result.put("sourceSessionId", sourceSessionId);
        result.put("createdAt", createdAt);
        result.put("turns", turns);
        return result;
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

    private static int compareCreatedAt(String t1, String t2) {
        Instant i1 = parseInstantSafe(t1);
        Instant i2 = parseInstantSafe(t2);
        if (i1 != null && i2 != null) {
            return i1.compareTo(i2);
        }
        if (i1 != null) {
            return -1;
        }
        if (i2 != null) {
            return 1;
        }
        return 0;
    }

    private static Instant parseInstantSafe(String s) {
        if (s == null || Trim.isBlank(s)) {
            return null;
        }
        try {
            return Instant.parse(s);
        } catch (DateTimeParseException e1) {
            try {
                return OffsetDateTime.parse(s).toInstant();
            } catch (DateTimeParseException e2) {
                return null;
            }
        }
    }

    private static String requireTrimmed(Map<String, String> opts, String key, String errorMsg) {
        String val = opts.get(key);
        if (val == null || Trim.isBlank(val)) {
            throw new AisException(400, errorMsg);
        }
        return Trim.trim(val);
    }

    private static String requireRaw(Map<String, String> opts, String key, String errorMsg) {
        String val = opts.get(key);
        if (val == null) {
            throw new AisException(400, errorMsg);
        }
        return val;
    }

    private static boolean isValidAttachmentName(String name) {
        return aisurada.ask.AskCommands.isValidAttachmentName(name);
    }

    private static void atomicWrite(Path targetFile, byte[] data) {
        aisurada.write.AtomicWriter.writeAtomic(targetFile, data);
    }
}
