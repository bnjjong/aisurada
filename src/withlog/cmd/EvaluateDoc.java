package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.text.Utf16;
import withlog.vault.NoteIndex;
import withlog.vault.NoteMeta;
import withlog.vault.NoteScanner;
import withlog.write.MetaWriter;
import withlog.write.TestApparatus;
import withlog.write.WorkspaceMarker;

import java.nio.file.Path;
import java.time.Instant;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.regex.Pattern;

/**
 * 설명서 15장 evaluate_doc 및 evaluate-doc 명령 구현체.
 * 노트 평가 정보(score, note, at)를 기록하거나 지우며(clear=true),
 * 시각이 새겨지므로 항상 파일을 원자적으로 다시 씁니다.
 */
public final class EvaluateDoc {

    public static final String USAGE =
            "evaluate-doc <노트 id> [score=<0~100>] [note=<평가 메모>] [clear=true] [f1=<직전에 읽은 토큰>]";

    private static final Pattern NOTE_ID_PATTERN = Pattern.compile("^nd[0-9a-z]+-[0-9a-f]{8}$");

    private EvaluateDoc() {}

    /**
     * evaluate_doc / evaluate-doc 명령을 실행합니다.
     */
    public static Map<String, Object> execute(Path vaultRoot, ParsedArgs parsed, String sessionId) {
        boolean isTool = parsed.isToolSurface();

        // 2. 워크스페이스 표식 확인 (15.4 row 2)
        WorkspaceMarker.check(vaultRoot);

        // 3. 색인 맞추기 (15.4 row 3). 맞추기를 세우는 파일이 있으면 쓰기도 선다(8.7의 4번)
        NoteIndex noteIndex = NoteIndex.reconciled(vaultRoot);

        // 4. id 검증 (15.4 row 4, 6)
        String rawId;
        if (isTool) {
            rawId = (String) parsed.toolOptions().get("id");
        } else {
            rawId = parsed.positionalArgs().isEmpty() ? null : parsed.positionalArgs().get(0);
        }

        if (rawId == null || Trim.isBlank(rawId)) {
            throw new WlException(400, "인자가 하나 필요하다: " + USAGE);
        }

        if (!NOTE_ID_PATTERN.matcher(rawId).matches()) {
            throw new WlException(400, "노트 id 모양이 아니다: " + rawId);
        }

        String noteId = rawId;

        // 9. clear 옵션 파싱 (15.4 row 9)
        boolean clear = false;
        if (isTool) {
            if (parsed.toolOptions().containsKey("clear")) {
                Object cVal = parsed.toolOptions().get("clear");
                if (cVal instanceof Boolean b) {
                    clear = b;
                }
            }
        } else {
            if (parsed.namedOptions().containsKey("clear")) {
                String cStr = parsed.namedOptions().get("clear");
                String trimmed = Trim.trim(cStr);
                if (!trimmed.isEmpty()) {
                    if ("true".equals(trimmed)) {
                        clear = true;
                    } else if ("false".equals(trimmed)) {
                        clear = false;
                    } else {
                        throw new WlException(400, "clear 은 true 나 false 여야 한다: " + trimmed);
                    }
                }
            }
        }

        // 10, 11, 12. score 검증 (clear가 아닐 때만)
        Double parsedScore = null;
        if (!clear) {
            if (isTool) {
                if (!parsed.toolOptions().containsKey("score")) {
                    throw new WlException(400, "score 를 주거나 clear=true 를 지정한다: " + USAGE);
                }
                Object sObj = parsed.toolOptions().get("score");
                if (sObj instanceof Number num) {
                    parsedScore = num.doubleValue();
                } else {
                    // 도구 표면에서 문자열이면 타입 검사(5.4)에서 이미 걸림
                    throw new WlException(400, "score 는 숫자 여야 한다: " + sObj);
                }
            } else {
                if (!parsed.namedOptions().containsKey("score")) {
                    throw new WlException(400, "score 를 주거나 clear=true 를 지정한다: " + USAGE);
                }
                String sStr = parsed.namedOptions().get("score");
                String trimmed = Trim.trim(sStr);
                if (trimmed.isEmpty()) {
                    throw new WlException(400, "score 를 주거나 clear=true 를 지정한다: " + USAGE);
                }
                try {
                    parsedScore = Double.valueOf(trimmed);
                } catch (NumberFormatException e) {
                    throw new WlException(400, "score 은 숫자여야 한다: " + trimmed);
                }
            }

            // 12. 범위 검사 (0 이상 100 이하)
            if (parsedScore.isNaN() || parsedScore < 0.0 || parsedScore > 100.0) {
                throw new WlException(400, "평가 점수는 0 에서 100 사이여야 한다: " + parsedScore);
            }
        }

        // note 옵션 파싱
        String noteStr = null;
        if (isTool) {
            if (parsed.toolOptions().containsKey("note")) {
                noteStr = (String) parsed.toolOptions().get("note");
            }
        } else {
            if (parsed.namedOptions().containsKey("note")) {
                noteStr = parsed.namedOptions().get("note");
            }
        }

        // f1 토큰 파싱
        String f1Token = null;
        if (isTool) {
            Object rawF1 = parsed.toolOptions().get("f1");
            if (rawF1 instanceof String s) {
                String trimmed = Trim.trim(s);
                if (!trimmed.isEmpty()) {
                    f1Token = trimmed;
                }
            }
        } else {
            String rawF1 = parsed.namedOptions().get("f1");
            if (rawF1 != null) {
                String trimmed = Trim.trim(rawF1);
                if (!trimmed.isEmpty()) {
                    f1Token = trimmed;
                }
            }
        }

        // 16. 색인에서 노트 검색 (15.4 row 16)
        NoteScanner.NoteRecord note = noteIndex.notesById().get(noteId);
        if (note == null) {
            throw new WlException(404, "Note not found: " + noteId);
        }

        // 새 평가 구성
        Map<String, Object> newEvaluation = null;
        if (!clear) {
            long rounded = Math.round(parsedScore);
            int intScore = (int) Math.max(0, Math.min(100, rounded));
            String cleanNote = (noteStr != null) ? Utf16.cut(Trim.trim(noteStr), 1000) : "";
            Instant now = TestApparatus.nowInstant();
            String at = NoteMeta.CreatedResult.class.getName(); // use NoteMeta format
            at = java.time.format.DateTimeFormatter.ofPattern("uuuu-MM-dd'T'HH:mm:ss.SSS'Z'")
                    .withZone(java.time.ZoneOffset.UTC).format(now);

            newEvaluation = new LinkedHashMap<>();
            newEvaluation.put("score", intScore);
            newEvaluation.put("note", cleanNote);
            newEvaluation.put("at", at);
        }

        // evaluate_doc은 늘 파일을 쓴다 (15.5)
        MetaWriter.WriteResult writeResult = MetaWriter.write(
                vaultRoot,
                note,
                f1Token,
                note.meta().favorite(),
                note.meta().favoriteCategory(),
                newEvaluation,
                note.meta().links(),
                isTool,
                sessionId
        );

        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("id", noteId);
        resp.put("path", writeResult.path());
        resp.put("precondition", writeResult.preconditionToken());
        resp.put("evaluation", newEvaluation);

        return resp;
    }
}
