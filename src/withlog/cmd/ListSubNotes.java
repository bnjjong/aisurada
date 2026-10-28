package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.out.NoteRow;
import withlog.text.Trim;
import withlog.vault.AttachText;
import withlog.vault.NoteIndex;
import withlog.vault.NoteScanner;

import java.nio.file.Path;
import java.util.*;
import java.util.regex.Pattern;

/**
 * list_sub_notes 및 list-sub-notes 명령 실행기.
 * 설명서 16장에 따라 부모 참조가 가리키는 컨테이너의 직계 하위 노트 목록을 생성합니다.
 */
public final class ListSubNotes {

    private static final Pattern NOTE_ID_PATTERN = Pattern.compile("^nd[0-9a-z]+-[0-9a-f]{8}$");
    private static final Pattern SUB_NOTE_ID_PATTERN = Pattern.compile("^sn[0-9a-z]+-[0-9a-f]{8}$");
    private static final String USAGE = "list-sub-notes <부모 노트 id> [bodies=true. 본문까지 싣는다. 기본은 메타만]";

    private ListSubNotes() {}

    public static Map<String, Object> execute(
            Path vaultRoot,
            ParsedArgs parsed,
            NoteIndex noteIndex
    ) {
        List<String> pos = parsed.positionalArgs();

        // 1. 첫 자리 인자 공백 검사 (16.3 1번). 「부모 노트 참조가 비었다」는 조각이 0개일 때(2번)만이다
        if (pos.isEmpty() || pos.get(0) == null || Trim.isBlank(pos.get(0))) {
            throw new WlException(400, "인자가 하나 필요하다: " + USAGE);
        }

        String rawParent = pos.get(0);

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

        NoteScanner.NoteRecord rootNote = noteIndex.notesById().get(rootId);

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

        // 5. 도구 표면이면 메인 노트 색인 조회 및 층 검사 (16.3 4번)
        if (parsed.isToolSurface()) {
            if (rootNote == null) {
                throw new WlException(404, "Note not found: " + rootId);
            }
            // 16.5, 16.9의 7번: 메인 노트를 찾을 때 HTML 첨부까지 읽는다(입출력 오류면 예상 밖 실패)
            AttachText.extract(vaultRoot, rootNote);
            if ("record".equals(rootNote.meta().tier())) {
                throw new WlException(409, "도구 표면이 읽을 수 없는 노트다(층=record): " + rootId +
                        ". 기계가 남긴 기록 노트는 앱에서만 열립니다.");
            }
        } else {
            // 명령 표면이고 컨테이너가 메인 노트인 경우 메인 노트 색인 검사 (16.3 5번)
            if (segments.size() == 1) {
                if (rootNote == null) {
                    throw new WlException(404, "Note not found: " + rootId);
                }
                // 16.9의 7번: 명령 표면은 컨테이너가 메인 노트일 때만 메인 노트를 찾는다
                AttachText.extract(vaultRoot, rootNote);
            }
        }

        // 6. bodies 검증 (명령 표면)
        boolean includeBodies = false;
        if (!parsed.isToolSurface() && parsed.namedOptions().containsKey("bodies")) {
            String rawBodies = parsed.namedOptions().get("bodies");
            String trimmedBodies = Trim.trim(rawBodies);
            if ("true".equals(trimmedBodies)) {
                includeBodies = true;
            } else if ("false".equals(trimmedBodies) || trimmedBodies.isEmpty()) {
                includeBodies = false;
            } else {
                throw new WlException(400, "bodies 은 true 나 false 여야 한다: " + trimmedBodies);
            }
        }

        // 7. 직계 자식 하위 노트 수집
        String containerId = prevId;
        List<NoteScanner.NoteRecord> directChildren = new ArrayList<>();
        if (rootNote != null) {
            for (NoteScanner.NoteRecord sub : rootNote.subNotes().values()) {
                if (containerId.equals(sub.directParentId())) {
                    directChildren.add(sub);
                }
            }
        }

        // 8. 정렬 (16.5): created 글자 바이트 차례, 같으면 id 글자 바이트 차례
        directChildren.sort((a, b) -> {
            String c1 = a.meta() != null && a.meta().created() != null ? a.meta().created() : "";
            String c2 = b.meta() != null && b.meta().created() != null ? b.meta().created() : "";
            int cmp = NoteIndex.compareUtf8Bytes(c1, c2);
            if (cmp != 0) return cmp;
            return NoteIndex.compareUtf8Bytes(a.id(), b.id());
        });

        // 9. 응답 구성 (16.6)
        String normalizedParent = String.join("/", segments);
        List<Map<String, Object>> subNotesList = new ArrayList<>();
        for (NoteScanner.NoteRecord sub : directChildren) {
            subNotesList.add(NoteRow.forListSubNotes(sub, normalizedParent, includeBodies));
        }

        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("parent", normalizedParent);
        resp.put("subNotes", subNotesList);
        return resp;
    }
}
