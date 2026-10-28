package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.vault.NoteIndex;
import withlog.vault.NoteScanner;
import withlog.write.MetaWriter;
import withlog.write.WorkspaceMarker;

import java.nio.file.Path;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Objects;
import java.util.regex.Pattern;

/**
 * 설명서 15장 set_favorite 및 set-favorite 명령 구현체.
 * 즐겨찾기 상태(favorite) 및 라벨(favoriteCategory)을 변경하거나 조회하며,
 * 변경이 없는 경우 파일을 열지 않고 changed: false를 반환합니다.
 */
public final class SetFavorite {

    public static final String USAGE =
            "set-favorite <노트 id> [favorite=true|false. 안 주면 지금 값을 뒤집는다] [category=<라벨. 빈 값이면 지운다>] [f1=<직전에 읽은 토큰>]";

    private static final Pattern NOTE_ID_PATTERN = Pattern.compile("^nd[0-9a-z]+-[0-9a-f]{8}$");

    private SetFavorite() {}

    /**
     * set_favorite / set-favorite 명령을 실행합니다.
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

        // 8. favorite 옵션 파싱 (15.4 row 8)
        Boolean favoriteOpt = null;
        if (isTool) {
            if (parsed.toolOptions().containsKey("favorite")) {
                favoriteOpt = (Boolean) parsed.toolOptions().get("favorite");
            }
        } else {
            if (parsed.namedOptions().containsKey("favorite")) {
                String favStr = parsed.namedOptions().get("favorite");
                String trimmed = Trim.trim(favStr);
                if (!trimmed.isEmpty()) {
                    if ("true".equals(trimmed)) {
                        favoriteOpt = true;
                    } else if ("false".equals(trimmed)) {
                        favoriteOpt = false;
                    } else {
                        throw new WlException(400, "favorite 은 true 나 false 여야 한다: " + trimmed);
                    }
                }
            }
        }

        // category 옵션 파싱
        boolean categoryGiven = false;
        String rawCategory = null;
        if (isTool) {
            if (parsed.toolOptions().containsKey("category")) {
                categoryGiven = true;
                rawCategory = (String) parsed.toolOptions().get("category");
            }
        } else {
            if (parsed.namedOptions().containsKey("category")) {
                categoryGiven = true;
                rawCategory = parsed.namedOptions().get("category");
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

        // 새 favorite 및 category 결정 (15.3, 15.5)
        boolean newFavorite;
        if (favoriteOpt != null) {
            newFavorite = favoriteOpt;
        } else if (categoryGiven) {
            // 안 주고 category만 주면 지금 값을 지킨다
            newFavorite = note.meta().favorite();
        } else {
            // 둘 다 안 주면 지금 값을 뒤집는다
            newFavorite = !note.meta().favorite();
        }

        String newCategory;
        if (categoryGiven) {
            if (rawCategory == null || Trim.isBlank(rawCategory)) {
                newCategory = null; // 공백뿐이면 지운다
            } else {
                newCategory = Trim.trim(rawCategory); // 길이를 자르지 않음 (15.9 2번)
            }
        } else {
            newCategory = note.meta().favoriteCategory();
        }

        // 값이 그대로인지 확인 (15.5)
        boolean unchanged = (newFavorite == note.meta().favorite()) &&
                Objects.equals(newCategory, note.meta().favoriteCategory());

        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("id", noteId);
        resp.put("favorite", newFavorite);
        if (newCategory != null) {
            resp.put("category", newCategory);
        }

        if (unchanged) {
            resp.put("changed", false);
            return resp;
        }

        // 메타 쓰기 실행
        MetaWriter.WriteResult writeResult = MetaWriter.write(
                vaultRoot,
                note,
                f1Token,
                newFavorite,
                newCategory,
                note.meta().evaluation(),
                note.meta().links(),
                isTool,
                sessionId
        );

        resp.put("changed", true);
        resp.put("path", writeResult.path());
        resp.put("precondition", writeResult.preconditionToken());

        return resp;
    }
}
