package withlog.cmd;

import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.vault.NoteIndex;
import withlog.vault.NoteScanner;

import java.util.ArrayList;
import java.util.List;
import java.util.regex.Pattern;

/**
 * 설명서 17.3의 부모 참조와 대상 하위 id 풀기.
 * 하위 노트 쓰기 명령 3개가 같은 차례로 `<노트 id>/<하위 id>/…` 사슬을 색인에서 한 마디씩 봅니다.
 */
final class SubNoteChain {

    private static final Pattern NOTE_ID_PATTERN = withlog.text.IdShape.NOTE;
    private static final Pattern SUB_NOTE_ID_PATTERN = withlog.text.IdShape.SUB_NOTE;

    private SubNoteChain() {}

    /**
     * 부모 참조를 푼 결과입니다. [root]는 색인에 없으면 null이고, [container]는 하위 조각이 없으면 [root]입니다.
     * [parentText]는 다듬은 조각을 `/`로 다시 이은 글로 응답의 `parent`입니다.
     */
    record Parent(String rootId, NoteScanner.NoteRecord root, NoteScanner.NoteRecord container, String parentText) {}

    /**
     * 17.3의 부모 참조 4걸음입니다. 조각마다 다듬고 빈 조각을 버린 뒤, 첫 조각은 노트 id 모양만 보고,
     * 뒤 조각은 1개마다 모양, 색인, 직계 부모를 다 본 뒤 다음 조각으로 갑니다.
     */
    static Parent resolveParent(NoteIndex noteIndex, String rawParent) {
        List<String> segments = new ArrayList<>();
        for (String s : rawParent.split("/")) {
            String t = Trim.trim(s);
            if (!t.isEmpty()) {
                segments.add(t);
            }
        }
        if (segments.isEmpty()) {
            throw new WlException(400, "부모 노트 참조가 비었다: " + rawParent);
        }
        String rootId = segments.get(0);
        if (!NOTE_ID_PATTERN.matcher(rootId).matches()) {
            throw new WlException(400, "노트 id 모양이 아니다: " + rootId);
        }
        NoteScanner.NoteRecord root = noteIndex.notesById().get(rootId);
        NoteScanner.NoteRecord container = root;
        String prevId = rootId;
        for (int i = 1; i < segments.size(); i++) {
            container = directChild(root, rootId, prevId, segments.get(i));
            prevId = container.id();
        }
        return new Parent(rootId, root, container, String.join("/", segments));
    }

    /**
     * 고치기와 지우기의 대상 하위 id입니다. 다듬지 않고 부모 참조의 뒤 조각과 같은 3가지 검사를 받습니다.
     */
    static NoteScanner.NoteRecord resolveTarget(Parent parent, String rawId) {
        String containerId = parent.container() == null ? parent.rootId() : parent.container().id();
        return directChild(parent.root(), parent.rootId(), containerId, rawId);
    }

    /** 모양(WL-400), 그 루트 아래 색인(WL-404 `<루트 id>/<하위 id>`), 직계 부모(WL-404 `<하위 id>`) 차례입니다. */
    private static NoteScanner.NoteRecord directChild(
            NoteScanner.NoteRecord root, String rootId, String parentId, String subId) {
        if (!SUB_NOTE_ID_PATTERN.matcher(subId).matches()) {
            throw new WlException(400, "하위 노트 id 모양이 아니다: " + subId);
        }
        NoteScanner.NoteRecord sub = root == null ? null : root.subNotes().get(subId);
        if (sub == null) {
            throw new WlException(404, "SubNote not found: " + rootId + "/" + subId);
        }
        if (!parentId.equals(sub.directParentId())) {
            throw new WlException(404, "SubNote not found: " + subId);
        }
        return sub;
    }
}
