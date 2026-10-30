package withlog;

import withlog.out.JsonWriter;
import java.nio.file.*;
import java.util.*;
import static withlog.ChapterTestSupport.*;

public final class Chapter26Test {
    private static final String TRASH = ".withlog/trash/trash.json", ID = "trfixture-00000001";
    private static Map<String, Object> entry(String id, String file, String slug) {
        Map<String, Object> m = new LinkedHashMap<>();
        m.put("trashId", id); m.put("file", file); m.put("originalPath", "notes/" + slug + "/original.md"); m.put("docId", "ndfixture-00000001");
        m.put("agendaId", "agfixture-00000001"); m.put("agendaSlug", slug); m.put("title", "보관물"); m.put("type", "build"); m.put("format", "markdown");
        m.put("created", "2026-06-16T10:00:00+09:00"); m.put("trashedAt", "2026-06-20T00:00:00.123Z"); return m;
    }
    private static void manifest(Path v, List<?> entries) throws Exception {
        Path p = v.resolve(TRASH); Files.createDirectories(p.getParent()); Files.write(p, JsonWriter.toFileBytes(entries));
    }
    public static void main(String[] args) throws Exception {
        try (var t = new ChapterTestSupport("chapter26")) {
            Path v = t.vault(); String original = "notes/demo/hand-renamed";
            root(v, "hand-renamed", 1, "2026-06-16T10:00:00+09:00", "favorite: true\nfavoriteCategory: 매뉴얼\ntier: record\n", "original\n");
            put(v, original + "/attach/keep.txt", "attached"); put(v, original + "/comments.json", "[]");
            byte[] before = Files.readAllBytes(v.resolve(original + "/note.md")); random("4c5d6e7f");
            var sent = ok(v, "delete_doc", "{\"id\":\"ndfixture-00000001\"}");
            eq(sent.keySet().stream().toList(), List.of("trashId", "id", "title", "originalPath", "file", "wasFavorite", "favoriteCategory", "warning"), "send key order");
            check(!Files.exists(v.resolve(original)), "record note moved by tool"); check(sent.get("warning").toString().contains("카테고리 '매뉴얼'"), "favorite warning");
            String trashId = (String) sent.get("trashId"); Path archive = v.resolve(".withlog/trash/files/" + sent.get("file"));
            eq(Files.readString(archive.resolve("attach/keep.txt")), "attached", "whole folder archived");
            String raw = Files.readString(v.resolve(TRASH));
            check(raw.indexOf("\"file\"") < raw.indexOf("\"docId\""), "manifest file before docId"); check(raw.endsWith("\n"), "manifest final LF");
            eq(list(v, "list-trash").getFirst().get("trashId"), trashId, "manifest readable immediately");
            var restored = ok(v, "restore_doc", "{\"trashId\":\"" + trashId + "\"}"); eq(restored.get("path"), original, "restore hand name");
            check(Arrays.equals(before, Files.readAllBytes(v.resolve(original + "/note.md"))), "same agenda restore preserves raw bytes");
            eq(Files.readString(v.resolve(TRASH)), "[]\n", "restored manifest empty"); error(v, 404, "restore-doc", trashId);
            manifest(v, List.of(entry(ID, "old.md", "gone")));
            note(v, ".withlog/trash/files/old.md", "ndfixture-00000001", "gone", "2026-06-16T10:00:00+09:00", "status: review\n", "# recover\n");
            restored = ok(v, "restore-doc", ID); eq(restored.get("path"), "notes/recovered/original.md", "missing agenda recovered");
            check(Files.readString(v.resolve("notes/recovered/original.md")).contains("agenda: recovered\n"), "frontmatter agenda rewritten");
            check(Files.readString(v.resolve("notes/recovered/original.md")).contains("status: review\n"), "unknown metadata preserved on rewrite");
            String recovered = Files.readString(v.resolve(".withlog/agendas/recovered.md")); check(!recovered.endsWith("\n"), "recovered agenda no final LF");
            manifest(v, List.of(entry(ID, "missing", "demo"))); error(v, 400, "restore-doc", ID); eq(Files.readString(v.resolve(TRASH)), "[]\n", "missing archive failure cleans manifest");
            var escaping = entry(ID, "../outside", "gone-again"); manifest(v, List.of(escaping));
            error(v, 400, "purge-doc", ID); eq(list(v, "list-trash").size(), 1, "unsafe archive preserves manifest");
            var empty = ok(v, "empty-trash"); eq(empty.get("purged"), 0, "failed item not purged"); eq(((List<?>) empty.get("failed")).size(), 1, "empty trash collects path failure");
            manifest(v, List.of(entry(ID, "single.md", "demo")));
            note(v, ".withlog/trash/files/single.md", "ndfixture-00000001", "demo", "2026-06-16", "", "single");
            put(v, "notes/demo/original.md", "occupied");
            restored = ok(v, "restore-doc", ID); eq(restored.get("path"), "notes/demo/original-restored-000001.md", "collision suffix before extension");
            eq(Files.readString(v.resolve("notes/demo/original.md")), "occupied", "collision target preserved");
            manifest(v, List.of(entry(ID, "bad.md", "absent")));
            note(v, ".withlog/trash/files/bad.md", "ndfixture-00000001", "absent", "2026-06-16", "mins: 12.5\n", "bad");
            error(v, 400, "restore-doc", ID); check(Files.exists(v.resolve(".withlog/trash/files/bad.md")), "unserializable metadata stays archived");
            eq(list(v, "list-trash").size(), 1, "failed restore retains entry");
            Path target = t.temp.resolve("outside-target"); Files.writeString(target, "outside");
            Files.createSymbolicLink(v.resolve(".withlog/trash/files/link"), target);
            manifest(v, List.of(entry(ID, "link", "demo"), entry(ID, "second", "demo")));
            ok(v, "purge_doc", "{\"trashId\":\"" + ID + "\"}"); eq(Files.readString(target), "outside", "purge deletes link only");
            eq(Files.readString(v.resolve(TRASH)), "[]\n", "duplicate trash ids all removed");
            Files.createSymbolicLink(v.resolve(".withlog/trash/files/broken"), t.temp.resolve("does-not-exist"));
            manifest(v, List.of(entry(ID, "broken", "demo"))); ok(v, "purge-doc", ID);
            check(Files.isSymbolicLink(v.resolve(".withlog/trash/files/broken")), "broken archive link remains orphan");
            put(v, ".withlog/trash/files/.hidden-orphan", "keep"); manifest(v, List.of(entry(ID, "missing", "demo")));
            empty = ok(v, "empty-trash"); eq(empty.get("purged"), 1, "absent archive counts as purged"); check(((Number) empty.get("orphans")).intValue() >= 2, "hidden and broken orphan counted");
            check(Files.exists(v.resolve(".withlog/trash/files/.hidden-orphan")), "orphans not deleted");
            put(v, TRASH, "[5, {\"trashId\":\"broken\"}]"); empty = ok(v, "empty-trash"); eq(empty.get("skipped"), 2, "bad manifest items counted and dropped");
            eq(Files.readString(v.resolve(TRASH)), "[]\n", "empty rewrites only readable entries");
            Path clean = t.vault(); ok(clean, "empty-trash"); eq(Files.readString(clean.resolve(TRASH)), "[]\n", "never-used trash created");
            root(clean, "partial", 1, "2026-06-16", "", "keep"); Files.delete(clean.resolve(TRASH)); Files.createDirectory(clean.resolve(TRASH));
            random("12345678"); error(clean, 500, "delete-doc", "ndfixture-00000001");
            check(!Files.exists(clean.resolve("notes/demo/partial")), "manifest failure leaves moved archive");
            try (var paths = Files.list(clean.resolve(".withlog/trash/files"))) { eq(paths.count(), 1L, "orphan archive remains after failed manifest"); }
            for (String tool : List.of("delete_doc", "restore_doc", "purge_doc")) error(v, 400, tool, "{}");
            Path suffix = t.vault(); agenda(suffix, "other", "agfixture-00000002", "다른 아젠다");
            note(suffix, "notes/demo/folder.md/note.md", "ndfixture-00000001", "other", "2026-06-16", "", "body");
            var suffixSent = ok(suffix, "delete-doc", "ndfixture-00000001");
            eq(list(suffix, "list-trash").getFirst().get("agenda"), "demo", "suffix folder fallback uses directory agenda");
            check(suffixSent.get("file").toString().endsWith(".md"), "suffix folder archived with extension");
            Files.delete(suffix.resolve(".withlog/agendas/demo.md"));
            var suffixRestore = run(suffix, "restore-doc", suffixSent.get("trashId").toString()); eq(suffixRestore.status(), 0, "suffix folder restored despite rewrite warning");
            check(stderr(suffixRestore).contains("보관물 본문을 못 읽어"), "suffix folder preserves unreadable body warning");
            eq(Files.readString(suffix.resolve("notes/recovered/folder.md/note.md")).contains("agenda: other\n"), true, "suffix folder agenda remains unchanged");
            for (String format : List.of("html", "json")) for (boolean single : List.of(false, true)) {
                Path shaped = t.vault();
                String relative = "notes/demo/shaped" + (single ? "." + format : "/note." + format);
                String body = format.equals("html") ? "<p>원문</p>" : "{\"root\":{\"children\":[]}}";
                note(shaped, relative, "ndfixture-00000001", "demo", "2026-06-16", "status: keep\n", body);
                byte[] source = Files.readAllBytes(shaped.resolve(relative));
                var shapedSent = ok(shaped, "delete-doc", "ndfixture-00000001");
                eq(shapedSent.get("file").toString().endsWith("." + format), single, "single/folder archive shape " + format);
                Path archived = shaped.resolve(".withlog/trash/files/" + shapedSent.get("file"));
                check(Arrays.equals(source, Files.readAllBytes(single ? archived : archived.resolve("note." + format))), "archive bytes unchanged " + format);
                Files.delete(shaped.resolve(".withlog/agendas/demo.md"));
                var shapedRestore = ok(shaped, "restore-doc", shapedSent.get("trashId").toString());
                Path restoredFile = shaped.resolve(shapedRestore.get("path").toString());
                if (!single) restoredFile = restoredFile.resolve("note." + format);
                String text = Files.readString(restoredFile);
                check(text.contains("agenda: recovered\n") && text.contains("status: keep\n"), "recovery rewrites only agenda in " + format);
                check(text.endsWith(body), "recovery preserves raw " + format + " body");
            }
            error(v, 400, "restore-doc", " ndfixture-00000001 "); error(v, 400, "empty-trash", "x");
            Path broken = t.vault(); put(broken, TRASH, "{}");
            eq(stderr(run(broken, "list_trash", "{}")), "휴지통 항목 1건을 못 읽어 목록에서 뺐다.\n  .withlog/trash/trash.json — 매니페스트가 배열이 아니다\n", "tool list writes trash diagnostics");
            eq(stderr(run(broken, "list-trash")), stderr(run(broken, "list_trash", "{}")), "both surfaces write the same trash diagnostics");
            t.done();
        }
    }
}
