package withlog.vault;

import withlog.text.Trim;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.regex.Pattern;

/**
 * 설명서 1.12의 코멘트 색인 사본 읽기를 흉내 냅니다.
 * 지금 엔진은 노트를 색인할 때 코멘트 파일을 1.8의 YAML 읽개로 1번 더 읽어서, 안 잡히는 표기가 들거나(1.15의 21번)
 * 다른 코멘트 파일과 사본 항목의 id가 겹치면(1.15의 23번) 맞추기 전체가 예상 밖 실패로 섭니다.
 * 이 엔진은 사본을 안 두고 그 2가지 실패만 같은 자리에서 냅니다.
 */
final class CommentCopy {

    private static final Pattern COMMENT_ID = Pattern.compile("^cm[0-9a-z]+-[0-9a-f]{8}$");

    private CommentCopy() {}

    /**
     * 루트 노트 1건의 코멘트 파일(루트의 comments.json, 하위 노트마다 직계 컨테이너의 sub-comments/<하위 id>.json)을
     * 사본 규칙으로 읽고, 사본 항목의 id를 [seenIds]에 더합니다. 단일 파일 메인 노트에는 코멘트 파일이 없습니다.
     */
    static void check(Path vaultRoot, NoteScanner.NoteRecord root, Set<String> seenIds) {
        if (root.isSingleFile()) {
            return;
        }
        read(vaultRoot.resolve(root.vaultRelPath()).resolve("comments.json"), seenIds);
        for (NoteScanner.NoteRecord sub : root.subNotes().values()) {
            Path container = vaultRoot.resolve(sub.vaultRelPath()).getParent().getParent();
            read(container.resolve("sub-comments").resolve(sub.id() + ".json"), seenIds);
        }
    }

    private static void read(Path file, Set<String> seenIds) {
        byte[] bytes;
        try {
            if (!Files.isRegularFile(file)) {
                return;
            }
            bytes = Files.readAllBytes(file);
        } catch (IOException e) {
            return;
        }
        String text;
        try {
            text = StrictUtf8.decodeStrict(bytes);
        } catch (Exception e) {
            return; // 깨진 UTF-8이면 코멘트 0건
        }
        // 1.8의 안 잡히는 표기는 여기서 그대로 던진다(1.15의 21번)
        Map<String, Object> map = Frontmatter.parse(text);
        if (map == null || !(map.get("comments") instanceof List<?> items)) {
            return;
        }
        Set<String> inFile = new HashSet<>();
        for (Object item : items) {
            if (!(item instanceof Map<?, ?> m)) continue;
            if (!(m.get("id") instanceof String id) || !COMMENT_ID.matcher(id).matches()) continue;
            if (!(m.get("createdAt") instanceof String at) || !NoteMeta.readsAsAt(at)) continue;
            if (!(m.get("body") instanceof String body) || Trim.isBlank(body)) continue;
            if (!inFile.add(id)) continue;
            if (!seenIds.add(id)) {
                throw new IllegalStateException("UNIQUE constraint failed: comments.id (" + id + ", " + file + ")");
            }
        }
    }
}
