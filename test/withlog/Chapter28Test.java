package withlog;

import withlog.vault.FileSize;
import java.nio.file.*;
import java.util.*;
import static withlog.ChapterTestSupport.*;

public final class Chapter28Test {
    @SuppressWarnings("unchecked")
    public static void main(String[] args) throws Exception {
        try (var t = new ChapterTestSupport("chapter28")) {
            Path v = t.vault(); sharded(v, true);
            root(v, "a", 1, "2026-06-16T10:00:00+09:00", "", "A");
            root(v, "group/b", 2, "2026-06-17T23:00:00-10:00", "", "B");
            put(v, "notes/demo/idless/note.md", "---\ncreated: '2026-06-18'\n---\nwithout id");
            put(v, "notes/demo/bad/note.md", "no frontmatter");
            put(v, "notes/demo/a (conflicted copy).md", "conflict");
            Files.createSymbolicLink(v.resolve("notes/demo/link"), t.temp.resolve("absent"));
            note(v, "notes/other/orphan/note.md", "ndfixture-00000003", "missing", "2026-06-19", "", "orphan agenda");
            put(v, "notes/demo/a/attach/plain.bin", "1234");
            Path external = t.temp.resolve("size.bin"); Files.writeString(external, "1234567");
            Files.createSymbolicLink(v.resolve("notes/demo/a/file-link"), external);
            Files.createSymbolicLink(v.resolve("notes/demo/a/dir-link"), v.resolve("notes/demo/group"));
            eq(FileSize.measure(v.resolve("notes/demo/a")).files(), 3, "count file links, skip directory links");
            var plan = ok(v, "shard-notes"); eq(plan.get("total"), 7, "disk candidates include idless/orphan");
            eq(plan.get("planned"), 4, "four movable candidates"); eq(plan.get("moved"), 0, "plan does not move");
            eq(((List<?>) plan.get("unmovable")).size(), 3, "three unmovable reasons");
            String mapping = (String) plan.get("mappingFile"); String tsv = Files.readString(Path.of(mapping));
            check(tsv.startsWith("noteId\toldRel\tnewRel\tfileCount\tbyteSum\treason\nndfixture-00000003\t"), "small agenda first in TSV");
            check(tsv.contains("\n\tnotes/demo/idless\tnotes/demo/2026-06/idless\t"), "idless node in TSV");
            var applied = ok(v, "shard-notes", "apply=true"); eq(applied.get("moved"), 4, "apply moves four");
            eq(applied.get("prunedEmptyDirs"), 1, "old grouping parent pruned");
            check(Files.exists(v.resolve("notes/demo/2026-06/a/attach/plain.bin")), "attachments travel with root");
            check(Files.exists(v.resolve("notes/other/2026-06/orphan/note.md")), "unindexed orphan agenda moved");
            var verified = ok(v, "verify-shard"); eq(verified.get("ok"), true, "baseline verification"); eq(verified.get("rows"), 7, "all TSV rows"); eq(verified.get("checked"), 4, "four compared paths");
            Files.delete(v.resolve("notes/demo/2026-06/a/attach/plain.bin"));
            var changed = run(v, "verify-shard"); eq(changed.status(), 0, "size mismatch is success");
            check(stdout(changed).contains("\"ok\": false"), "mismatch output flag"); check(stderr(changed).contains("크기가 갈린 것 1 건"), "mismatch stderr travels with result");
            ok(v, "shard-notes", "apply=true");
            check(stderr(error(v, 409, "verify-shard")).contains("자리 바뀜 0"), "ambiguous baseline lists counts");
            String basename = Path.of((String) applied.get("mappingFile")).getFileName().toString();
            eq(ok(v, "verify-shard", "file=../outside/" + basename + "/").get("ok"), false, "file uses basename only");
            sharded(v, false); var flat = ok(v, "shard-notes", "apply=true"); eq(flat.get("moved"), 4, "unshard four");
            check(!Files.exists(v.resolve("notes/demo/2026-06")), "empty month directory pruned");
            check(Files.exists(v.resolve("notes/demo/a/note.md")), "flat path restored");
            Files.delete(v.resolve(".withlog/workspace.json")); ok(v, "verify-shard", "file=" + basename);
            error(v, 400, "shard-notes", "apply=bad");

            Path stopped = t.vault(); sharded(stopped, true);
            root(stopped, "a", 1, "2026-06-16", "", "body"); put(stopped, "notes/demo/relative.bin", "123");
            Files.createSymbolicLink(stopped.resolve("notes/demo/a/link"), Path.of("../relative.bin"));
            var interrupted = ok(stopped, "shard-notes", "apply=true");
            check(interrupted.containsKey("stoppedAt"), "relative file link changes measured size after move");
            eq(interrupted.get("moved"), 0, "stopped current move not counted (compatibility)");
            check(Files.exists(stopped.resolve("notes/demo/2026-06/a/note.md")), "stopped current node physically moved");

            Path collision = t.vault(); sharded(collision, false);
            root(collision, "g1/same", 1, "2026-06-16", "", "first"); root(collision, "g2/same", 2, "2026-06-17", "", "second");
            put(collision, ".withlog/reports/rv-evening-2026-06-16.json", "body: !!int abc");
            String failure = stderr(error(collision, 500, "shard-notes", "apply=true"));
            check(failure.startsWith("색인을 못 맞췄다."), "final full reconciliation diagnostic precedes original WL failure");
            check(failure.contains("\nWL-500 옮길 자리에"), "original move failure preserved after final reconcile failure");
            check(Files.exists(collision.resolve("notes/demo/same/note.md")), "earlier collision candidate stays moved");
            check(Files.exists(collision.resolve("notes/demo/g2/same/note.md")), "later candidate preserved on collision");
            check(Files.isDirectory(collision.resolve("notes/demo/g1")), "error skips empty parent cleanup");

            Path manual = t.vault(); sharded(manual, null); put(manual, "one.bin", "abc");
            String prefix = "ignored header\r\n";
            put(manual, ".withlog/migration/custom.tsv", prefix + "id\tone.bin\tone.bin\t１\t+003\t\textra\r\n" + "bad\tx\ty\t2147483648\t0\t\n" + "\u3000\n" + "skip\t../bad\t\t0\t0\tunmovable\n");
            verified = ok(manual, "verify-shard", "file=custom.tsv"); eq(verified.get("rows"), 2, "TSV invalid rows dropped; Nd and signs parsed"); eq(verified.get("ok"), true, "unmovable does not affect ok");
            put(manual, ".withlog/migration/custom.tsv", prefix + "id\t../bad\tmissing\t0\t0\t\n");
            eq(ok(manual, "verify-shard", "file=custom.tsv").get("checked"), 1, "missing new path never checks invalid old path");
            put(manual, ".withlog/migration/custom.tsv", prefix + "id\tone.bin\t../bad\t1\t3\t\n"); error(manual, 400, "verify-shard", "file=custom.tsv");
            put(manual, ".withlog/migration/custom.tsv", prefix + "id\tone.bin\t \t1\t3\t\n"); error(manual, 400, "verify-shard", "file=custom.tsv");
            put(manual, ".withlog/migration/custom.tsv", prefix + "id\tone.bin\t" + manual.resolve("one.bin") + "\t1\t3\t\n");
            verified = ok(manual, "verify-shard", "file=custom.tsv");
            eq(((List<?>) verified.get("leftBehind")).size(), 1, "absolute path accepted; distinct path text reports old place too");
            eq(run(manual, "verify-shard", "file=/").status(), 1, "slash-only name preserves unexpected failure");
            Path empty = t.vault(); eq(ok(empty, "shard-notes").get("total"), 0, "empty plan still writes header"); error(empty, 404, "verify-shard");
            String deep = "notes/demo/" + "deep/".repeat(9); Files.createDirectories(empty.resolve(deep));
            Files.createSymbolicLink(empty.resolve("notes/demo/attach"), t.temp.resolve("absent-reserved"));
            Files.createSymbolicLink(empty.resolve("notes/demo/conflicted copy"), t.temp.resolve("absent-conflict"));
            var boundaries = ok(empty, "shard-notes"); eq(boundaries.get("total"), 2, "reserved link omitted, conflict link and depth cap counted");
            var unmovable = (List<Map<String, Object>>) boundaries.get("unmovable");
            eq(unmovable.getFirst().get("reason"), "심링크라 따라가지 않았다", "symlink checked before conflict name");
            eq(unmovable.get(1).get("reason"), "스캔 깊이 상한을 넘었다", "depth-eight fallback");
            eq(run(empty, "shard_notes").status(), 2, "shard command-only"); eq(run(empty, "verify_shard").status(), 2, "verify command-only");
            error(empty, 400, "shard-notes", "x", " apply =yes", "unknown=x"); t.done();
        }
    }
}
