package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.vault.NoteIndex;
import withlog.vault.NoteScanner;
import withlog.write.ChildPageCards;
import withlog.write.DocToken;
import withlog.write.DerivedBodyWriter;
import withlog.write.SubNoteWriter;
import withlog.write.TreeRemover;
import withlog.engine.WritePipeline;

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

    /**
     * delete_sub_note / delete-sub-note 명령을 실행합니다. 검사 차례는 17.3, 지우는 차례는 17.5의 지우기입니다.
     */
    public static Map<String, Object> execute(Path vaultRoot, ParsedArgs parsed) {
        // 3. 워크스페이스 표식, 색인 맞추기 (17.4의 2번)

        NoteIndex noteIndex = WritePipeline.index(vaultRoot);

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
        DerivedBodyWriter.Read read = DerivedBodyWriter.read(container.contentFilePath());
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
                SubNoteWriter.Placed placed = DerivedBodyWriter.rewriteSub(vaultRoot, parent.root(), container, stripped);
                written = placed.bytes();
                // 7번: 컨테이너 폴더가 옮겨졌으면 지울 자리도 옮긴 뒤의 자리다
                subRel = placed.folderRel() + subRel.substring(container.vaultRelPath().length());
            } else {
                written = DerivedBodyWriter.rewriteRoot(vaultRoot, container, stripped);
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

}
