package withlog.write;

import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.text.Utf16;
import withlog.vault.Frontmatter;
import withlog.vault.NoteScanner;
import withlog.vault.StrictUtf8;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.LinkedHashMap;
import java.util.Map;

/** 7.3: 카드와 코멘트 마커 제거가 공유하는 본문 읽기와 도장 없는 파생 쓰기. */
public final class DerivedBodyWriter {
    private DerivedBodyWriter() {}
    public record Read(String body, byte[] bytes) {}

    /**
     * 7.3의 3번 읽기입니다. 1.6으로 프론트매터를 뗀 본문이고, 프론트매터가 없으면 파일 전체이고,
     * 파일을 못 읽으면 빈 글이라 걷을 카드가 없는 것으로 칩니다.
     */
    public static Read read(Path file) {
        try {
            if (!Files.isRegularFile(file) || Files.size(file) > 1_000_000) {
                return new Read("", null);
            }
            byte[] bytes = Files.readAllBytes(file);
            String content = StrictUtf8.decodeStrict(bytes);
            MetaWriter.FileParts parts = MetaWriter.split(content);
            boolean hasFrontmatter = parts.fenceFound() && parts.yamlPart() != null
                    && parts.yamlPart().length() <= 16_000 && Frontmatter.parse(parts.yamlPart()) != null;
            return new Read(hasFrontmatter ? parts.body() : content, bytes);
        } catch (IOException e) {
            return new Read("", null);
        }
    }

    /**
     * 걷은 본문을 쓰기 직전에 파일을 다시 열어 7.2의 2번과 6번만 봅니다. 토큰과 도장은 안 봅니다(7.3의 5번).
     * 못 읽거나 프론트매터가 없으면 `WL-500`이고, 그 파일의 YAML 맵을 돌려줍니다.
     */
    private static Map<String, Object> reopen(Path vaultRoot, Path file, String snippet) {
        String rel = vaultRoot.relativize(file).toString().replace('\\', '/');
        String content;
        try {
            if (!Files.isRegularFile(file) || Files.size(file) > 1_000_000) {
                throw new IOException("unreadable");
            }
            content = StrictUtf8.decodeStrict(Files.readAllBytes(file));
        } catch (IOException e) {
            throw new WlException(500, "노트 본문 파일을 못 읽어 " + snippet + " 않았다: " + rel);
        }
        MetaWriter.FileParts parts = MetaWriter.split(content);
        Map<String, Object> map = parts.fenceFound() && parts.yamlPart() != null && parts.yamlPart().length() <= 16_000
                ? Frontmatter.parse(parts.yamlPart()) : null;
        if (map == null) {
            throw new WlException(500, "프론트매터가 없어 " + snippet + " 않았다: " + rel);
        }
        return map;
    }

    /**
     * 루트 노트 컨테이너를 제자리에 다시 씁니다(7.3의 6번). 메타는 색인의 값이고 `agenda` 글자와 `sessionId`만 파일에서 오며,
     * 옛 도장을 그대로 다시 적는 파생 쓰기라 `status:` 같은 모르는 키는 사라지고 새 도장은 안 생깁니다.
     */
    public static byte[] rewriteRoot(Path vaultRoot, NoteScanner.NoteRecord root, String body) {
        Map<String, Object> rawYamlMap = reopen(vaultRoot, root.contentFilePath(), "본문을 저장하지");
        String agenda;
        if (rawYamlMap.get("agenda") instanceof String sa && !sa.isEmpty()) {
            agenda = sa;
        } else {
            String[] segs = root.vaultRelPath().split("/");
            agenda = segs.length >= 2 ? segs[1] : root.meta().agendaSlug();
        }
        String fileSessionId = null;
        if (rawYamlMap.get("sessionId") instanceof String ss && !Trim.trim(ss).isEmpty()) {
            fileSessionId = Utf16.cut(Trim.trim(ss), 128);
        }
        Map<String, Object> evaluation = root.meta().evaluation();
        if (evaluation != null && evaluation.get("note") instanceof String en) {
            evaluation = new LinkedHashMap<>(evaluation);
            evaluation.put("note", MetaWriter.asIndexed(en));
        }
        String frontmatter = FrontmatterWriter.buildFullNoteFrontmatter(
                root.id(), agenda, root.meta().type(), MetaWriter.asIndexed(root.meta().title()), root.meta().created(),
                fileSessionId, root.meta().mins(), root.meta().tier(),
                root.meta().editedBy(), root.meta().editedAt(), root.meta().editedSessionId(),
                SubNoteWriter.indexedTags(root), root.meta().favorite(), MetaWriter.asIndexed(root.meta().favoriteCategory()),
                root.meta().template(), evaluation, root.meta().links());
        String fileText = "---\n" + frontmatter + "---\n" + body;
        if (fileText.length() > 1_000_000) {
            throw new WlException(500, "노트가 저장 상한을 넘어 저장하지 않았다: "
                    + vaultRoot.relativize(root.contentFilePath()).toString().replace('\\', '/'));
        }
        byte[] bytes = Utf16.toUtf8BytesWithQuestionForUnpaired(fileText);
        AtomicWriter.writeAtomic(root.contentFilePath(), bytes);
        return bytes;
    }

    /**
     * 하위 노트 컨테이너를 다시 씁니다. `update_sub_note`와 같은 걸음이라 폴더 이름을 다시 계산해 정식 이름으로 옮깁니다(17.9의 3번).
     */
    public static SubNoteWriter.Placed rewriteSub(
            Path vaultRoot, NoteScanner.NoteRecord root, NoteScanner.NoteRecord container, String body) {
        reopen(vaultRoot, container.contentFilePath(), "하위 노트 본문을 저장하지");
        String title = MetaWriter.asIndexed(container.meta().title());
        String fileText = SubNoteWriter.fileText(container, SubNoteWriter.rootAgenda(root), title,
                SubNoteWriter.indexedTags(container), body);
        SubNoteWriter.requireMarkdownFolder(container);
        return SubNoteWriter.placeAndWrite(vaultRoot, container, title, fileText);
    }
}
