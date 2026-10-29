package withlog.vault;

import withlog.args.JsonReader;
import withlog.args.JsonValue;
import withlog.text.Utf16;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;
import java.util.regex.Pattern;

/**
 * 코멘트 파일(comments.json 및 sub-comments/<id>.json) 파서.
 * 설명서 1.12 및 11.6에 따라 absent, loaded, unreadable 봉투를 생성합니다.
 */
public final class CommentsReader {
    private static final Pattern COMMENT_ID_PATTERN = Pattern.compile(
            "^cm[a-z0-9]+-[a-z0-9]{1,8}$",
            Pattern.CASE_INSENSITIVE | Pattern.UNICODE_CASE
    );

    private CommentsReader() {}

    /**
     * 코멘트 파일을 읽어 규격 응답 맵(comments 봉투)을 반환합니다.
     * @param vaultRoot vault 루트 경로
     * @param commentFile 코멘트 파일 경로 (없을 수도 있음)
     * @return comments 봉투 Map (state: absent | loaded | unreadable)
     */
    public static Map<String, Object> readComments(Path vaultRoot, Path commentFile) {
        Map<String, Object> envelope = new LinkedHashMap<>();

        if (commentFile == null || !Files.isRegularFile(commentFile)) {
            envelope.put("state", "absent");
            return envelope;
        }

        String relPath = vaultRoot.relativize(commentFile).toString().replace('\\', '/');

        long size;
        try {
            size = Files.size(commentFile);
        } catch (IOException e) {
            size = 0;
        }

        if (size > 200_000) {
            envelope.put("state", "unreadable");
            envelope.put("reason", "파일이 너무 크다(" + size + "바이트). 상한은 200000바이트다.");
            return envelope;
        }

        byte[] rawBytes;
        try {
            rawBytes = Files.readAllBytes(commentFile);
        } catch (IOException e) {
            envelope.put("state", "unreadable");
            envelope.put("reason", "파일을 못 읽었다: " + relPath);
            return envelope;
        }

        // 깨진 바이트는 U+FFFD로 변환 (1.12 4번)
        String jsonText = new String(rawBytes, StandardCharsets.UTF_8);

        JsonValue jsonVal;
        try {
            jsonVal = JsonReader.parseFile(jsonText);
        } catch (Exception e) {
            envelope.put("state", "unreadable");
            envelope.put("reason", "내용이 JSON이 아니다: " + relPath);
            return envelope;
        }

        List<JsonValue> candidateObjects = new ArrayList<>();
        if (jsonVal instanceof JsonValue.JsonArray arr) {
            for (JsonValue item : arr.elements()) {
                if (item instanceof JsonValue.JsonObject) {
                    candidateObjects.add(item);
                }
            }
        } else if (jsonVal instanceof JsonValue.JsonObject obj) {
            JsonValue cVal = obj.properties().get("comments");
            if (cVal instanceof JsonValue.JsonArray arr) {
                for (JsonValue item : arr.elements()) {
                    if (item instanceof JsonValue.JsonObject) {
                        candidateObjects.add(item);
                    }
                }
            }
        }

        Set<String> claimedIds = new HashSet<>();
        List<Map<String, Object>> liveComments = new ArrayList<>();

        for (JsonValue cand : candidateObjects) {
            if (!(cand instanceof JsonValue.JsonObject candObj)) {
                continue;
            }
            Map<String, JsonValue> props = candObj.properties();

            JsonValue idVal = props.get("id");
            if (!(idVal instanceof JsonValue.JsonString strId)) {
                continue;
            }
            String rawId = strId.value();
            if (!COMMENT_ID_PATTERN.matcher(rawId).matches()) {
                continue;
            }
            if (claimedIds.contains(rawId)) {
                continue;
            }
            claimedIds.add(rawId);

            JsonValue createdVal = props.get("createdAt");
            if (!(createdVal instanceof JsonValue.JsonString strCreatedAt)) {
                continue;
            }
            String createdAt = strCreatedAt.value();
            if (createdAt.isEmpty()) {
                continue;
            }

            JsonValue bodyVal = props.get("body");
            if (!(bodyVal instanceof JsonValue.JsonString strBody)) {
                continue;
            }
            String body = Utf16.cut(strBody.value(), 20_000);

            Map<String, Object> item = new LinkedHashMap<>();
            item.put("id", rawId);
            item.put("createdAt", createdAt);

            JsonValue editedVal = props.get("editedAt");
            if (editedVal instanceof JsonValue.JsonString strEdited && !strEdited.value().isEmpty()) {
                item.put("editedAt", strEdited.value());
            }

            JsonValue anchorVal = props.get("anchor");
            if (anchorVal instanceof JsonValue.JsonObject anchorObj) {
                JsonValue quoteVal = anchorObj.properties().get("quote");
                if (quoteVal instanceof JsonValue.JsonString strQuote && !strQuote.value().isEmpty()) {
                    Map<String, Object> anchorMap = new LinkedHashMap<>();
                    anchorMap.put("quote", Utf16.cut(strQuote.value(), 2_000));
                    item.put("anchor", anchorMap);
                }
            }

            item.put("body", body);
            liveComments.add(item);
        }

        sort(liveComments);

        envelope.put("state", "loaded");
        envelope.put("comments", liveComments);
        return envelope;
    }

    /** 읽기와 파일 전체 재쓰기가 같은 UTF-16 차례를 사용한다(19.5, 19.8). */
    public static void sort(List<Map<String, Object>> comments) {
        comments.sort((a, b) -> {
            int cmp = Utf16.compare((String) a.get("createdAt"), (String) b.get("createdAt"));
            if (cmp != 0) return cmp;
            return Utf16.compare((String) a.get("id"), (String) b.get("id"));
        });

    }
}
