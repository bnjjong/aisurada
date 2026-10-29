package withlog;

import java.nio.file.*;
import java.util.*;
import static withlog.ChapterTestSupport.*;

public final class Chapter27Test {
    public static void main(String[] args) throws Exception {
        try (var t = new ChapterTestSupport("chapter27")) {
            Path v = t.vault(); root(v, "hand-renamed", 1, "2026-06-16", "", "# exact\r\n\r\nattach/x #sub:id <!--wlc:c-->\n");
            note(v, "notes/demo/old.md", "ndfixture-00000002", "demo", "2026-06-17", "", "no final newline");
            note(v, "notes/demo/html/note.html", "ndfixture-00000003", "demo", "2026-06-18", "", "<p>html</p>");
            note(v, "notes/demo/json/note.json", "ndfixture-00000004", "demo", "2026-06-19", "", "{}");
            root(v, "record", 5, "2026-06-20", "tier: record\n", "secret");
            root(v, "a..b", 6, "2026-06-21", "", "unsafe"); root(v, "folder.md", 7, "2026-06-22", "", "hidden by suffix bug");
            note(v, "notes/demo/hand-renamed/sub/child/note.md", "snfixture-00000001", "demo", "2026-06-16", "", "child");
            put(v, ".withlog/md-export/stale.md", "stale");
            var response = ok(v, "export_markdown", "{}");
            eq(response.keySet().stream().toList(), List.of("dest", "exported", "totalNotes", "skippedHtml", "skippedUnsafe", "skippedJson"), "conditional response key order");
            eq(response.get("totalNotes"), 6, "reading roots only"); eq(response.get("exported"), 3, "three markdown writes");
            eq(response.get("skippedUnsafe"), 1, "unsafe path skipped"); eq(response.get("skippedHtml"), 1, "html skipped"); eq(response.get("skippedJson"), 1, "json skipped");
            Path dest = v.resolve(".withlog/md-export");
            eq(Files.readString(dest.resolve("demo/hand-renamed.md")), "# exact\r\n\r\nattach/x #sub:id <!--wlc:c-->\n", "exact body including engine references");
            eq(Files.readString(dest.resolve("demo/old.md")), "no final newline", "single file body no added LF");
            eq(Files.size(dest.resolve("demo/folder.md")), 0, "folder extension compatibility produces empty file");
            check(!Files.exists(dest.resolve("stale.md")), "previous export removed");
            error(v, 400, "export_markdown", "{\"agenda\":\"demo\"}");
            check(Files.exists(dest.resolve("demo/old.md")), "syntax failure preserves old export");
            root(v, "x", 8, "2026-06-23", "", "first");
            note(v, "notes/demo/x.md", "ndfixture-00000009", "demo", "2026-06-24", "", "second");
            response = ok(v, "export-markdown"); eq(response.get("exported"), 5, "colliding writes both counted");
            eq(Files.readString(dest.resolve("demo/x.md")), "second", "created order determines collision winner");
            Path outside = Files.createDirectory(t.temp.resolve("outside")); Files.writeString(outside.resolve("keep.md"), "keep");
            withlog.write.TreeRemover.remove(dest); Files.createSymbolicLink(dest, outside);
            ok(v, "export-markdown"); check(!Files.isSymbolicLink(dest), "export replaces destination symlink");
            eq(Files.readString(outside.resolve("keep.md")), "keep", "destination symlink target untouched");
            t.done();
        }
    }
}
