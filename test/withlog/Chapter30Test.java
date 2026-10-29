package withlog;

import withlog.out.JsonWriter;
import java.nio.file.*;
import java.util.*;
import static withlog.ChapterTestSupport.*;

public final class Chapter30Test {
    private static void comments(Path v, String path, String... at) throws Exception {
        List<Map<String, Object>> rows = new ArrayList<>();
        for (int i = 0; i < at.length; i++) rows.add(Map.of("id", "cmfixture-" + String.format("%08x", i + 1), "createdAt", at[i], "body", "말", "editedAt", "2099-01-01T00:00Z"));
        Path file = v.resolve(path); Files.createDirectories(file.getParent()); Files.write(file, JsonWriter.toFileBytes(rows));
    }
    @SuppressWarnings("unchecked") private static List<Map<String, Object>> items(Map<String, Object> result) { return (List<Map<String, Object>>) result.get("items"); }
    private static Set<Object> ids(Map<String, Object> result) { return new HashSet<>(items(result).stream().map(i -> i.get("id")).toList()); }
    public static void main(String[] args) throws Exception {
        try (var t = new ChapterTestSupport("chapter30")) {
            Path v = t.vault(); root(v, "root", 1, "2026-06-16T10:00:00+09:00", "title: 픽스처 노트\n", "");
            eq(ok(v, "check-notes", "date=2026-06-20").get("count"), 0, "absent comments silent");
            comments(v, "notes/demo/root/comments.json", "2026-06-16T10:30:00Z", "2026-06-16T10:31:00Z", "2026-06-16T10:32:00.000Z");
            var response = ok(v, "check-notes", "date=2026-06-20");
            eq(response.keySet().stream().toList(), List.of("date", "windowDays", "count", "section", "items"), "check key order"); eq(response.get("count"), 1, "one unanswered note");
            var item = items(response).getFirst(); eq(item.get("lastCommentAt"), "2026-06-16T10:32Z", "ISO shortest offset datetime"); eq(item.get("lastChangeAt"), "2026-06-16T10:00+09:00", "created retains own offset");
            eq(item.get("comments"), 3, "three valid comment records"); eq(item.get("stamped"), false, "no stamp uses created");
            eq(response.get("section"), "- 답 없는 코멘트: \"픽스처 노트\" (데모), 2026-06-16. 코멘트 3개, 마지막 2026-06-16 10:32. 그 뒤로 노트가 바뀐 기록이 없다(편집 도장 없음, 작성 2026-06-16 10:00).", "manual-style exact section");
            eq(ok(v, "check-notes", "date=2026-06-30").get("count"), 1, "14-day lower boundary inclusive");
            eq(ok(v, "check-notes", "date=2026-07-01").get("count"), 0, "outside lower window");
            eq(ok(v, "check-notes", "date=2026-05-01").get("count"), 1, "no upper window bound");
            comments(v, "notes/demo/root/comments.json", "2026-06-16T10:32Z", "2026-06-16T19:32+09:00", "2026-06-16T20:00", " 2026-06-16T20:00Z");
            item = items(ok(v, "check-notes", "date=2026-06-20")).getFirst();
            eq(item.get("lastCommentAt"), "2026-06-16T19:32+09:00", "same instant uses larger wall clock"); eq(item.get("comments"), 4, "unparseable timestamps still count");
            root(v, "root", 1, "2026-06-16T10:00:00+09:00", "title: 픽스처 노트\neditedBy: app\neditedAt: '2026-06-16T19:32:00+09:00'\n", "");
            comments(v, "notes/demo/root/comments.json", "2026-06-16T19:32:59.999+09:00");
            eq(ok(v, "check-notes", "date=2026-06-20").get("count"), 0, "same minute treated answered");
            comments(v, "notes/demo/root/comments.json", "2026-06-16T19:33:00+09:00");
            item = items(ok(v, "check-notes", "date=2026-06-20")).getFirst(); eq(item.get("stamped"), true, "stamp used");
            root(v, "old", 2, "2026-06-15T10:00:00+09:00", "", ""); comments(v, "notes/demo/old/comments.json", "2026-06-25T10:00Z");
            check(!ids(ok(v, "check-notes", "date=2026-06-20")).contains("ndfixture-00000002"), "unstamped before earliest stamp excluded");
            root(v, "equal", 3, "2026-06-16T19:32:00+09:00", "", ""); comments(v, "notes/demo/equal/comments.json", "2026-06-17T10:00Z");
            check(ids(ok(v, "check-notes", "date=2026-06-20")).contains("ndfixture-00000003"), "created equal earliest instant included");
            root(v, "record", 4, "2026-01-01", "tier: record\neditedBy: app\neditedAt: '2026-01-02T10:00Z'\n", ""); comments(v, "notes/demo/record/comments.json", "2026-06-20T10:00Z");
            check(!ids(ok(v, "check-notes", "date=2026-06-20")).contains("ndfixture-00000002"), "record stamp not used as earliest");
            note(v, "notes/demo/json/note.json", "ndfixture-00000005", "demo", "2026-06-18", "editedBy: app\neditedAt: '2026-06-25T10:00Z'\n", "{}");
            comments(v, "notes/demo/json/comments.json", "2026-06-20T10:00Z");
            response = ok(v, "check-notes", "date=2026-06-20"); check(ids(response).contains("ndfixture-00000005"), "JSON stamp ignored");
            item = items(response).stream().filter(i -> i.get("id").equals("ndfixture-00000005")).findFirst().orElseThrow(); eq(item.get("stamped"), false, "JSON uses created");
            put(v, "notes/demo/json/comments.json", "bad JSON"); check(!ids(ok(v, "check-notes", "date=2026-06-20")).contains("ndfixture-00000005"), "unreadable comments skipped");
            note(v, "notes/demo/earlier.html", "ndfixture-00000006", "demo", "2026-01-01", "editedBy: robot\neditedAt: '2026-06-14T10:00Z'\n", "<p>old</p>");
            check(ids(ok(v, "check-notes", "date=2026-06-20")).contains("ndfixture-00000002"), "single HTML note contributes earliest raw stamp");
            root(v, "invalid", 7, "2026-06-18", "editedBy: app\neditedAt: 'bad'\n", ""); comments(v, "notes/demo/invalid/comments.json", "2026-06-19T10:00:00.123456789+09:00:30");
            response = ok(v, "check-notes", "date=2026-06-20"); item = items(response).stream().filter(i -> i.get("id").equals("ndfixture-00000007")).findFirst().orElseThrow();
            eq(item.get("stamped"), false, "invalid stamp treated absent"); eq(item.get("lastCommentAt"), "2026-06-19T10:00:00.123456789+09:00:30", "nanoseconds and offset seconds preserved");
            eq(run(v, "check_notes").status(), 2, "command only"); error(v, 400, "check-notes", "date=2026-02-30"); error(v, 400, "check-notes", "2026-06-20");
            Files.delete(v.resolve(".withlog/workspace.json")); check(stderr(error(v, 400, "check-notes", "date=bad")).contains("워크스페이스"), "workspace before date");
            t.done();
        }
    }
}
