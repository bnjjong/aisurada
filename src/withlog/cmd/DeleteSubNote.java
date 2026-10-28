package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.text.Utf16;
import withlog.vault.Frontmatter;
import withlog.vault.NoteIndex;
import withlog.vault.NoteScanner;
import withlog.vault.StrictUtf8;
import withlog.write.AtomicWriter;
import withlog.write.ChildPageCards;
import withlog.write.DocToken;
import withlog.write.FrontmatterWriter;
import withlog.write.MetaWriter;
import withlog.write.SubNoteWriter;
import withlog.write.TreeRemover;
import withlog.write.WorkspaceMarker;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;

/**
 * 설명서 17장 delete_sub_note / delete-sub-note 명령.
 * 직계 컨테이너 본문에서 그 하위 노트의 카드 줄을 먼저 걷고(7.3의 파생 쓰기), 하위 노트 폴더를 휴지통 없이 통째로 지웁니다.
 */
public final class DeleteSubNote {

    private DeleteSubNote() {}

    public static final String USAGE = "delete-sub-note <부모 노트 id> <하위 노트 id>";

    /** 걷을 카드를 찾으려고 읽은 컨테이너 본문과 그 바이트입니다. 못 읽었으면 본문은 빈 글이고 바이트는 null입니다. */
    private record ContainerRead(String body, byte[] bytes) {}

    /**
     * delete_sub_note / delete-sub-note 명령을 실행합니다. 검사 차례는 17.3, 지우는 차례는 17.5의 지우기입니다.
     */
    public static Map<String, Object> execute(Path vaultRoot, ParsedArgs parsed) {
        // 3. 워크스페이스 표식, 색인 맞추기 (17.4의 2번)
        WorkspaceMarker.check(vaultRoot);
        NoteIndex noteIndex = NoteIndex.reconciled(vaultRoot);

        // 6. 자리 인자 2칸, 부모 참조, 하위 id의 3가지 검사. 17.5 지우기 1번이라 카드를 걷기 전에 다 본다
        List<String> pos = parsed.positionalArgs();
        if (pos.isEmpty() || Trim.isBlank(pos.get(0))) {
            throw new WlException(400, "인자가 하나 필요하다: " + USAGE);
        }
        if (pos.size() < 2 || Trim.isBlank(pos.get(1))) {
            throw new WlException(400, "인자가 둘 필요하다: " + USAGE);
        }
        SubNoteChain.Parent parent = SubNoteChain.resolveParent(noteIndex, pos.get(0));
        NoteScanner.NoteRecord sub = SubNoteChain.resolveTarget(parent, pos.get(1));

        // 2번, 3번: 직계 컨테이너가 markdown이 아니면 카드가 있든 없든 아무것도 안 지운다
        NoteScanner.NoteRecord container = parent.container();
        if (!"markdown".equals(container.format())) {
            throw new WlException(409, "본문이 markdown 이 아니라 하위 페이지 카드를 걷어낼 수 없다(포맷="
                    + container.format().toUpperCase(Locale.ROOT) + "): " + container.id());
        }

        // 4번~6번: 그 순간 읽은 본문에서 카드 줄을 걷고, 걷을 것이 있을 때만 컨테이너를 다시 쓴다
        ContainerRead read = readContainer(container.contentFilePath());
        String stripped = ChildPageCards.strip(read.body(), sub.id());
        String subRel = sub.vaultRelPath();
        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("id", sub.id());
        resp.put("parent", parent.parentText());
        resp.put("deleted", true);
        String containerBefore = null;
        String containerAfter = null;
        if (!stripped.equals(read.body())) {
            byte[] written;
            if (container.isSubNote()) {
                SubNoteWriter.Placed placed = rewriteSubContainer(vaultRoot, parent.root(), container, stripped);
                written = placed.bytes();
                // 7번: 컨테이너 폴더가 옮겨졌으면 지울 자리도 옮긴 뒤의 자리다
                subRel = placed.folderRel() + subRel.substring(container.vaultRelPath().length());
            } else {
                written = rewriteRoot(vaultRoot, container, stripped);
            }
            containerBefore = DocToken.compute(read.bytes());
            containerAfter = DocToken.compute(written);
        }

        // 7번: 하위 노트 자리를 통째로 지운다. 빈 sub/와 직계 컨테이너의 sub-comments/<하위 id>.json은 남는다
        TreeRemover.remove(vaultRoot.resolve(subRel));

        // 8번: 쓴 뒤 루트를 다시 읽는 일은 없다. 이 엔진은 부를 때마다 디스크를 훑는다(8.6의 4번)
        resp.put("path", subRel);
        resp.put("cardRemoved", containerBefore != null);
        if (containerBefore != null) {
            resp.put("containerBefore", containerBefore);
            resp.put("containerAfter", containerAfter);
        }
        return resp;
    }

    /**
     * 7.3의 3번 읽기입니다. 1.6으로 프론트매터를 뗀 본문이고, 프론트매터가 없으면 파일 전체이고,
     * 파일을 못 읽으면 빈 글이라 걷을 카드가 없는 것으로 칩니다.
     */
    private static ContainerRead readContainer(Path file) {
        try {
            if (!Files.isRegularFile(file) || Files.size(file) > 1_000_000) {
                return new ContainerRead("", null);
            }
            byte[] bytes = Files.readAllBytes(file);
            String content = StrictUtf8.decodeStrict(bytes);
            MetaWriter.FileParts parts = MetaWriter.split(content);
            boolean hasFrontmatter = parts.fenceFound() && parts.yamlPart() != null
                    && parts.yamlPart().length() <= 16_000 && Frontmatter.parse(parts.yamlPart()) != null;
            return new ContainerRead(hasFrontmatter ? parts.body() : content, bytes);
        } catch (IOException e) {
            return new ContainerRead("", null);
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
    private static byte[] rewriteRoot(Path vaultRoot, NoteScanner.NoteRecord root, String body) {
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
    private static SubNoteWriter.Placed rewriteSubContainer(
            Path vaultRoot, NoteScanner.NoteRecord root, NoteScanner.NoteRecord container, String body) {
        reopen(vaultRoot, container.contentFilePath(), "하위 노트 본문을 저장하지");
        String title = MetaWriter.asIndexed(container.meta().title());
        String fileText = SubNoteWriter.fileText(container, SubNoteWriter.rootAgenda(root), title,
                SubNoteWriter.indexedTags(container), body);
        SubNoteWriter.requireMarkdownFolder(container);
        return SubNoteWriter.placeAndWrite(vaultRoot, container, title, fileText);
    }
}
