package withlog;

import withlog.config.EngineConfig;
import withlog.engine.EngineExecutor;
import withlog.git.VaultGit;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.TimeUnit;
import static withlog.ChapterTestSupport.*;

/** 실제 저장소에서 명령 계약, 이동/삭제 이력, 스냅숏과 본문 쓰기의 연결을 검증한다. */
public final class Chapter31Test {
    private static final String ID = "ndfixture-00000001";
    private static long seconds = 1792540800L;
    private static String git(Path vault, String... args) throws Exception {
        List<String> command = new ArrayList<>(List.of("git", "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgsign=false", "-c", "core.quotePath=false"));
        command.addAll(List.of(args));
        ProcessBuilder builder = new ProcessBuilder(command).directory(vault.toFile()).redirectErrorStream(true);
        var env = builder.environment(); env.remove("GIT_INDEX_FILE");
        env.put("GIT_DIR", vault.resolve(".git").toString()); env.put("GIT_WORK_TREE", vault.toString());
        env.put("GIT_AUTHOR_NAME", "Human"); env.put("GIT_COMMITTER_NAME", "Human");
        env.put("GIT_AUTHOR_EMAIL", "test@example.invalid"); env.put("GIT_COMMITTER_EMAIL", "test@example.invalid");
        env.put("GIT_AUTHOR_DATE", "@" + seconds + " +0000"); env.put("GIT_COMMITTER_DATE", "@" + seconds + " +0000");
        Process child = builder.start(); byte[] output = child.getInputStream().readAllBytes();
        check(child.waitFor(10, TimeUnit.SECONDS), "git completes");
        eq(child.exitValue(), 0, "git " + List.of(args) + ": " + new String(output, StandardCharsets.UTF_8));
        return new String(output, StandardCharsets.UTF_8);
    }
    private static String commit(Path v, String message) throws Exception {
        seconds += 60; git(v, "add", "-A"); git(v, "commit", "-qm", message); return git(v, "rev-parse", "HEAD").trim();
    }
    @SuppressWarnings("unchecked") private static List<Map<String, Object>> versions(Map<String, Object> r) { return (List<Map<String, Object>>) r.get("versions"); }
    private static void config(Path v, boolean auto, boolean snapshot) {
        EngineConfig.activate(new EngineConfig(v.toString(), v.toString(), null, false, 10, auto, 300, snapshot));
    }
    private static String snapshotRef(Path v, String id) throws Exception { return git(v, "rev-parse", "refs/withlog/snapshots/" + id).trim(); }
    public static void main(String[] args) throws Exception {
        try (var t = new ChapterTestSupport("chapter31")) {
            arguments(t); history(t); snapshots(t); outcomes(t); t.done();
        }
    }
    private static void arguments(ChapterTestSupport t) throws Exception {
        Path v = t.vault(); Files.delete(v.resolve(".withlog/workspace.json"));
        eq(ok(v, "history", "missing"), Map.of("id", "missing", "count", 0L, "versions", List.of()), "history needs neither workspace nor index");
        eq(ok(v, "commit-vault"), Map.of("outcome", "disabled", "blocked", "not-repo"), "not repo successful outcome");
        eq(stderr(error(v, 400, "history", "x", "bad", "extra", "unknown=x")), "WL-400 history 이 모르는 옵션이다: unknown. 쓸 수 있는 것은 [commit, query] 뿐이다.\n", "unknown option before positions");
        check(stderr(error(v, 400, "history", "x", "bad", "extra")).contains("자리 인자는 2"), "extra position before number");
        for (String value : List.of("0", "-2", "2147483648", "", " 1", "1 ", "+", "𝟐"))
            eq(stderr(error(v, 400, "history", "", value, "commit=bad")), "WL-400 판 번호는 1 이상의 정수여야 한다: " + value + "\n", "number validated first");
        eq(stderr(error(v, 400, "history", "", "commit= BAD ")), "WL-400 커밋은 40 자리 16 진수여야 한다: BAD\n", "trimmed commit before id");
        String hash = "a".repeat(40);
        check(stderr(error(v, 400, "history", "", "+1", "commit=" + hash)).contains("함께 줄 수 없다"), "mutual exclusion before id");
        check(stderr(error(v, 400, "history")).contains(HistoryUsage()), "missing id usage");
        for (String id : List.of("한글", " x", "../x", "a.b", "x".repeat(129))) {
            eq(ok(v, "history", id).get("count"), 0L, "unsafe id empty history");
            eq(ok(v, "history", id).get("id"), id, "raw id kept");
        }
        for (String number : List.of("+2", "002", "۲", "２"))
            check(stderr(error(v, 409, "history", "missing", number)).contains("판이 아직 없다"), "unicode and signed numbers parsed");
        String digits = "２".repeat(40);
        check(stderr(error(v, 409, "history", "missing", "commit=" + digits)).contains(digits), "unicode commit passes syntax");
        error(v, 400, "history", "x", "commit=" + "A".repeat(40));
        check(!ok(v, "history", "missing", "query=\u3000").containsKey("matches"), "blank query omitted");
        eq(ok(v, "history", "missing", "query=x").get("matches"), List.of(), "nonempty query includes empty matches");
        eq(stderr(error(v, 400, "commit-vault", "extra", "unknown=x")), "WL-400 commit-vault 이 모르는 옵션이다: unknown. 쓸 수 있는 것은 [] 뿐이다.\n", "commit option before extra positional");
        eq(stderr(error(v, 400, "commit-vault", "extra")), "WL-400 commit-vault 이 받는 자리 인자는 0 개인데 1 개를 받았다: commit-vault\n", "commit no arguments");
        eq(run(v, "commit_vault").status(), 2, "no tool commit surface");
        eq(run(v, "history_tool").status(), 2, "no history tool surface");
    }
    private static String HistoryUsage() { return "history <노트 id> [번호] [commit=<커밋>] [query=<글자>]"; }

    private static void history(ChapterTestSupport t) throws Exception {
        Path v = t.vault(); git(v, "init", "-b", "main");
        root(v, "root", 1, "2026-06-16T10:00:00+09:00", "editedBy: ' app '\neditedAt: ' old stamp '\neditedSessionId: ' session-A '\n", "Alpha\nß ı İ K Σ ς 𐐀\n");
        String initial = Files.readString(v.resolve("notes/demo/root/note.md"));
        String first = commit(v, "manual baseline");
        root(v, "root", 1, "2026-06-16T10:00:00+09:00", "editedBy: tool\neditedAt: now\neditedSessionId: session-B\n", "Beta\n");
        eq(ok(v, "commit-vault").get("outcome"), "committed", "explicit commit");
        String auto = git(v, "rev-parse", "HEAD").trim();
        check(git(v, "show", "-s", "--format=%B", "HEAD").contains("Withlog-Reason: restore"), "explicit reason restore");
        check(git(v, "show", "-s", "--format=%B", "HEAD").startsWith("wl(outside)"), "oneshot writer outside");
        var result = ok(v, "history", ID);
        eq(result.keySet().stream().toList(), List.of("id", "count", "versions"), "history root key order");
        eq(result.get("count"), 2L, "manual and auto combined");
        var rows = versions(result);
        eq(rows.getFirst().get("source"), "auto", "author source auto"); eq(rows.get(1).get("source"), "manual", "author source manual");
        eq(rows.get(1).get("takenAt"), "2026-10-21T09:01:00+09:00", "committer seconds in KST");
        eq(rows.get(1).get("editedBy"), "app", "trimmed stamp"); eq(rows.get(1).get("editedSessionId"), "session-A", "app session included");
        eq(rows.getFirst().keySet().stream().toList(), List.of("index", "takenAt", "source", "commit", "path", "bytes", "editedBy", "editedSessionId"), "version key order");
        result = ok(v, "history", ID, "commit=" + first, "query= alpha ");
        eq(result.keySet().stream().toList(), List.of("id", "count", "body", "noteBody", "matches", "versions"), "selection search key order");
        eq(result.get("body"), initial, "full body exact"); eq(result.get("noteBody"), "Alpha\nß ı İ K Σ ς 𐐀\n", "frontmatter stripped");
        eq(result.get("matches"), List.of(first), "query trimmed and case insensitive");
        for (String query : List.of("I", "k", "σ", "𐐨")) eq(ok(v, "history", ID, "query=" + query).get("matches"), List.of(first), "Java Unicode case search " + query);
        eq(ok(v, "history", ID, "query=SS").get("matches"), List.of(), "no multi-character case folding");
        eq(ok(v, "history", ID, "query=ndfixture").get("matches"), List.of(), "query does not search header");
        eq(ok(v, "history", ID, "۲").get("body"), initial, "unicode selected number");
        check(stderr(error(v, 409, "history", ID, "+3")).contains(": 3\n"), "range error prints parsed number");
        eq(ok(v, "commit-vault").get("outcome"), "nothing", "no changes");
        // 본문 유사성이 없는 이동도 Withlog-Notes로 잇는다.
        Path old = v.resolve("notes/demo/root"); Path moved = v.resolve("notes/demo/moved"); Files.move(old, moved);
        root(v, "moved", 1, "2026-06-16T10:00:00+09:00", "", "entirely different\n".repeat(30));
        ok(v, "commit-vault"); result = ok(v, "history", ID); eq(result.get("count"), 2L, "trailer joins rewritten moved note; unlinked manual ancestor is not rediscovered");
        check(versions(result).stream().anyMatch(r -> r.get("commit").equals(auto)), "old commit retained after rename");
        Files.delete(moved.resolve("note.md")); ok(v, "commit-vault"); result = ok(v, "history", ID);
        eq(result.get("count"), 2L, "deleted note retains trailer versions and excludes deletion blob");
        eq(ok(v, "history", ID, "commit=" + auto).get("noteBody"), "Beta\n", "deleted note selectable without index");
        error(v, 409, "history", ID, "commit=" + first);
        git(v, "checkout", "-qb", "other"); eq(ok(v, "history", ID).get("count"), 2L, "history reads main from other branch");
        // note.md가 아닌 파일은 경로 탐색에서 빠지고, 끝만 맞는 단일 파일은 든다.
        note(v, "notes/demo/json/note.json", "jsonid", "demo", "2026-06-16", "", "{}");
        note(v, "notes/demo/.-keynote.md", "singleid", "demo", "2026-06-16", "", "single");
        commit(v, "nonmain files"); git(v, "branch", "-f", "main", "HEAD");
        eq(ok(v, "history", "jsonid").get("count"), 0L, "JSON no path history");
        eq(ok(v, "history", "singleid").get("count"), 1L, "keynote.md suffix path history");
        EngineConfig.activate(null);
    }

    private static void snapshots(ChapterTestSupport t) throws Exception {
        Path v = t.vault(); git(v, "init", "-b", "main");
        root(v, "root", 1, "2026-06-16T10:00:00+09:00", "", "before\n"); commit(v, "baseline");
        // 꺼진 상태의 쓰기는 참조를 만들지 않는다.
        config(v, false, false); ok(v, "set-favorite", ID, "favorite=true", "f1=" + ok(v, "get-doc", ID).get("precondition"));
        eq(git(v, "for-each-ref", "refs/withlog/snapshots"), "", "snapshot default off");
        config(v, false, true); String old = Files.readString(v.resolve("notes/demo/root/note.md"));
        ok(v, "set-favorite", ID, "favorite=false", "f1=" + ok(v, "get-doc", ID).get("precondition")); String snap = snapshotRef(v, ID);
        eq(git(v, "show", snap + ":note.md"), old, "metadata snapshot exact old bytes");
        eq(git(v, "show", "-s", "--format=%an <%ae>|%cn <%ce>", snap).trim(), "withlog-auto <noreply@withlog.local>|withlog-auto <noreply@withlog.local>", "snapshot identity");
        var history = ok(v, "history", ID); check(versions(history).stream().anyMatch(r -> r.get("source").equals("snapshot")), "snapshots merged");
        old = Files.readString(v.resolve("notes/demo/root/note.md"));
        var updated = ok(v, "update-doc", ID, "title=새 제목", "body=# new\n", "f1=" + ok(v, "get-doc", ID).get("precondition"));
        String current = (String) updated.get("path"); snap = snapshotRef(v, ID);
        eq(git(v, "show", snap + ":note.md"), old, "renamed root snapshot old bytes at new location");
        check(git(v, "show", "-s", "--format=%B", snap).contains(current + "/note.md"), "snapshot renamed location");
        var sub = ok(v, "create-sub-note", ID, "title=child", "body=# child\n"); String subId = (String) sub.get("id");
        eq(git(v, "for-each-ref", "--format=%(refname)", "refs/withlog/snapshots/" + subId), "", "new subnote has no snapshot");
        String subPath = (String) sub.get("path"); old = Files.readString(v.resolve(subPath).resolve("note.md"));
        ok(v, "update-sub-note", ID, subId, "body=# changed\n", "f1=" + ok(v, "get-sub-note", ID, subId).get("precondition"));
        eq(git(v, "show", snapshotRef(v, subId) + ":note.md"), old, "subnote overwrite snapshot");
        ok(v, "update-doc", ID, "body=# new\n[child](#sub:" + subId + ")\n", "f1=" + ok(v, "get-doc", ID).get("precondition"));
        old = Files.readString(v.resolve(current).resolve("note.md"));
        ok(v, "delete-sub-note", ID, subId);
        eq(git(v, "show", snapshotRef(v, ID) + ":note.md"), old, "derived root card removal snapshot");
        String refBefore = snapshotRef(v, ID);
        var rejected = run(v, "update-doc", ID, "body=stale", "f1=f1:stale");
        eq(rejected.status(), 5, "stale precondition exit"); eq(rejected.errorCode(), 412, "stale precondition WL code");
        eq(snapshotRef(v, ID), refBefore, "rejected write has no snapshot");
        config(v, false, false); eq(ok(v, "history", subId).get("count"), 1L, "snapshot readable when feature off");
        EngineConfig.activate(null);
    }

    private static void outcomes(ChapterTestSupport t) throws Exception {
        Path v = t.vault(); git(v, "init", "-b", "main"); root(v, "root", 1, "2026-06-16T10:00:00+09:00", "", "root"); commit(v, "baseline");
        put(v, "external", "outside"); config(v, false, false);
        eq(ok(v, "commit-vault"), Map.of("outcome", "disabled", "blocked", "off"), "explicit honors off");
        config(v, true, false); Files.writeString(v.resolve(".git/index.lock"), "");
        eq(ok(v, "commit-vault"), Map.of("outcome", "skipped", "blocked", "index-lock"), "index lock successful skip");
        Files.delete(v.resolve(".git/index.lock"));
        eq(ok(v, "commit-vault").get("outcome"), "committed", "manual commit bypasses skip retry");
        Files.writeString(v.resolve(".git/MERGE_HEAD"), "");
        eq(ok(v, "commit-vault").get("blocked"), "in-progress", "merge skip"); Files.delete(v.resolve(".git/MERGE_HEAD"));
        git(v, "checkout", "-qb", "other"); eq(ok(v, "commit-vault").get("blocked"), "not-main", "other branch skip"); git(v, "checkout", "main");
        // 31.5 is read-flagged: it shares pending state in serve, without registering itself as app.
        config(v, true, false); put(v, "external", "serve tool"); VaultGit.serving(v).afterWrite(true, "session raw");
        var r = EngineExecutor.execute(v, new String[]{"commit-vault"}, true, System.currentTimeMillis()); eq(r.status(), 0, "serve explicit commit success");
        String msg = git(v, "show", "-s", "--format=%B", "HEAD"); check(msg.startsWith("wl(tool)"), "explicit keeps pending writer"); check(msg.contains("Withlog-Session: session raw"), "explicit keeps raw session");
        Files.delete(v.resolve(".withlog/workspace.json")); put(v, "notes/poison/note.md", "---\nmins: !!int bad\n---\n");
        eq(ok(v, "commit-vault").get("outcome"), "committed", "explicit ignores workspace and poisoned index");
        Files.writeString(v.resolve(".git/index"), "corrupt index");
        eq(ok(v, "commit-vault"), Map.of("outcome", "failed"), "git add failure stays successful command");
        Path worktree = t.vault(); put(worktree, ".git", "gitdir: elsewhere"); config(worktree, true, false);
        eq(ok(worktree, "commit-vault").get("blocked"), "not-repo", "git file is not an autocommit repository");
        eq(ok(worktree, "history", ID).get("count"), 0L, "git file has empty history");
        EngineConfig.activate(null);
    }
}
