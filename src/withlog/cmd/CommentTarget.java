package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.text.IdShape;
import withlog.text.Trim;
import withlog.vault.NoteIndex;
import withlog.vault.NoteScanner;

import java.nio.file.Path;

/** 19.3/19.5의 루트 id + 하위 id. 하위 노트 사슬과 구별한다. */
final class CommentTarget {
    record Ids(String root, String sub) {}
    record Resolved(NoteScanner.NoteRecord root, NoteScanner.NoteRecord owner, Path file) {}

    private CommentTarget() {}

    static Ids parse(ParsedArgs parsed, String usage) {
        var pos = parsed.positionalArgs();
        if (pos.isEmpty() || Trim.isBlank(pos.get(0))) {
            throw new WlException(400, "인자가 하나 필요하다: " + usage);
        }
        String root = pos.get(0);
        String sub = pos.size() > 1 && !Trim.isBlank(pos.get(1)) ? pos.get(1) : null;
        if (!IdShape.NOTE.matcher(root).matches()) throw new WlException(400, "노트 id 모양이 아니다: " + root);
        if (sub != null && !IdShape.SUB_NOTE.matcher(sub).matches()) throw new WlException(400, "하위 노트 id 모양이 아니다: " + sub);
        return new Ids(root, sub);
    }

    static Resolved resolve(Path vault, NoteIndex index, Ids ids) {
        var root = index.notesById().get(ids.root());
        if (root == null || root.vaultRelPath() == null) throw new WlException(404, "Note not found: " + ids.root());
        if (ids.sub() == null) return new Resolved(root, root, vault.resolve(root.vaultRelPath()).resolve("comments.json"));
        var sub = root.subNotes().get(ids.sub());
        if (sub == null) throw new WlException(404, "SubNote not found: " + ids.sub());
        String parentId = sub.directParentId();
        var container = parentId == null || parentId.isEmpty() || parentId.equals(root.id())
                ? root : root.subNotes().get(parentId);
        if (container == null || container.vaultRelPath() == null) throw new WlException(404, "SubNote not found: " + parentId);
        return new Resolved(root, sub, vault.resolve(container.vaultRelPath()).resolve("sub-comments").resolve(ids.sub() + ".json"));
    }
}
