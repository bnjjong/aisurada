package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.vault.AttachText;
import withlog.vault.CommentsReader;
import withlog.vault.NoteScanner;
import withlog.vault.PathJoin;

import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.regex.Pattern;

/**
 * get_sub_note 및 get-sub-note 명령 실행기.
 * 설명서 11.8에 따라 부모 참조 사슬 검증 및 하위 노트 조회 응답을 생성합니다.
 */
public final class GetSubNote {
    private static final Pattern NOTE_ID_PATTERN = Pattern.compile("^nd[0-9a-z]+-[0-9a-f]{8}$");
    private static final Pattern SUB_NOTE_ID_PATTERN = Pattern.compile("^sn[0-9a-z]+-[0-9a-f]{8}$");

    private GetSubNote() {}

    /**
     * get_sub_note 또는 get-sub-note 명령을 실행합니다.
     * @param vaultRoot vault 루트 절대 경로
     * @param parsed 파싱된 인자
     * @param scanResult 노트 훑기 결과
     * @return 규격 응답 Map
     */
    public static Map<String, Object> execute(
            Path vaultRoot,
            ParsedArgs parsed,
            NoteScanner.ScanResult scanResult
    ) {
        List<String> pos = parsed.positionalArgs();

        // 1. 자리 인자 개수 및 공백 검사
        if (pos.isEmpty() || pos.get(0) == null || Trim.isBlank(pos.get(0))) {
            throw new WlException(400, "인자가 하나 필요하다: get-sub-note <부모 노트 id> <하위 노트 id>");
        }
        if (pos.size() < 2 || pos.get(1) == null || Trim.isBlank(pos.get(1))) {
            throw new WlException(400, "인자가 둘 필요하다: get-sub-note <부모 노트 id> <하위 노트 id>");
        }

        String rawParent = pos.get(0);
        String rawTargetId = pos.get(1); // 11.8: id는 다듬지 않는다

        // 2. parent 분리 및 다듬기
        String[] split = rawParent.split("/");
        List<String> segments = new ArrayList<>();
        for (String s : split) {
            String t = Trim.trim(s);
            if (!t.isEmpty()) {
                segments.add(t);
            }
        }

        if (segments.isEmpty()) {
            String origin = parsed.rawParentSubNote() != null ? parsed.rawParentSubNote() : rawParent;
            throw new WlException(400, "부모 노트 참조가 비었다: " + origin);
        }

        // 3. 첫 조각은 노트 id 모양
        String rootId = segments.get(0);
        if (!NOTE_ID_PATTERN.matcher(rootId).matches()) {
            throw new WlException(400, "노트 id 모양이 아니다: " + rootId);
        }

        NoteScanner.NoteRecord rootNote = scanResult.notesById().get(rootId);

        // 4. 뒤 조각 순차 검증
        String prevId = rootId;
        for (int i = 1; i < segments.size(); i++) {
            String seg = segments.get(i);
            if (!SUB_NOTE_ID_PATTERN.matcher(seg).matches()) {
                throw new WlException(400, "하위 노트 id 모양이 아니다: " + seg);
            }
            if (rootNote == null || !rootNote.subNotes().containsKey(seg)) {
                throw new WlException(404, "SubNote not found: " + rootId + "/" + seg);
            }
            NoteScanner.NoteRecord sub = rootNote.subNotes().get(seg);
            if (!prevId.equals(sub.directParentId())) {
                throw new WlException(404, "SubNote not found: " + seg);
            }
            prevId = seg;
        }

        // 5. 대상 id 검증
        if (!SUB_NOTE_ID_PATTERN.matcher(rawTargetId).matches()) {
            throw new WlException(400, "하위 노트 id 모양이 아니다: " + rawTargetId);
        }

        if (rootNote == null || !rootNote.subNotes().containsKey(rawTargetId)) {
            throw new WlException(404, "SubNote not found: " + rootId + "/" + rawTargetId);
        }

        NoteScanner.NoteRecord targetSub = rootNote.subNotes().get(rawTargetId);
        if (!prevId.equals(targetSub.directParentId())) {
            throw new WlException(404, "SubNote not found: " + rawTargetId);
        }

        // 6. 도구 표면이면 루트 노트 존재 및 층 검사 (11.8, 11.9의 17번)
        if (parsed.isToolSurface()) {
            if (rootNote == null) {
                throw new WlException(404, "Note not found: " + rootId);
            }
            // 11.8 & 11.9 17번: 도구 표면에서 루트 층을 볼 때 루트의 HTML 첨부까지 읽음 (입출력 오류 시 예상 밖 실패)
            AttachText.extract(vaultRoot, rootNote);

            if ("record".equals(rootNote.meta().tier())) {
                throw new WlException(409, "도구 표면이 읽을 수 없는 노트다(층=record): " + rootId +
                        ". 기계가 남긴 기록 노트는 앱에서만 열립니다.");
            }
        }

        // 7. 코멘트 읽기: 직계 컨테이너 폴더의 sub-comments/<하위 id>.json (11.4 16번, 11.8)
        NoteScanner.NoteRecord directContainer;
        if (rootId.equals(targetSub.directParentId())) {
            directContainer = rootNote;
        } else {
            directContainer = rootNote.subNotes().get(targetSub.directParentId());
        }

        Path containerDir = vaultRoot;
        if (directContainer != null && directContainer.vaultRelPath() != null) {
            String[] pathSegments = directContainer.vaultRelPath().split("/");
            for (String seg : pathSegments) {
                containerDir = PathJoin.join(containerDir, seg);
            }
        }

        Path commentFile = PathJoin.join(containerDir, "sub-comments", targetSub.id() + ".json");
        Map<String, Object> commentsEnvelope = CommentsReader.readComments(vaultRoot, commentFile);

        // 8. 응답 생성 (11.8 표 순서)
        String normalizedParent = String.join("/", segments);
        return withlog.out.NoteRow.forGetSubNote(targetSub, normalizedParent, commentsEnvelope);
    }
}
