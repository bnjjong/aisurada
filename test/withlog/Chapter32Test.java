package withlog;

import withlog.cmd.CommandRegistry;
import withlog.engine.EngineExecutor;
import withlog.engine.EngineResult;
import withlog.git.VaultGit;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.TimeUnit;
import static withlog.ChapterTestSupport.*;

/** 4부 없는 명령의 검사 순서와 1부가 모르는 파일의 수명주기를 검사한다. */
public final class Chapter32Test {
    private static final String ID = "ndfixture-00000001", SUB = "snfixture-00000001";
    private static final String NO_PART4 = "WL-409 이 판에는 4부(Claude 연동)가 없다. 문답과 레일 대화와 세션 기록 명령은 wl-claude 모듈을 넣어 지은 사이드카에서만 돈다.\n";
    private static final Map<String, List<String>> OPTIONS = Map.of(
            "get-ask", List.of(),
            "append-ask-turn", List.of("session-id", "turn-id", "mark-id", "asked-at", "quote", "question", "answer", "error", "model", "effort", "elapsed-ms", "session-lines", "images"),
            "list-chats", List.of(),
            "append-chat-turn", List.of("chat-id", "source-session", "turn-id", "asked-at", "question", "answer", "error", "model", "effort", "elapsed-ms", "images"),
            "read-session", List.of("limit", "before", "urls", "note", "author"),
            "session-titles", List.of(), "recent-sessions", List.of("days"));
    public static void main(String[] args) throws Exception {
        try (var t = new ChapterTestSupport("chapter32")) {
            commands(t); files(t); shards(t); pending(t); doctor(t); t.done();
        }
    }
    private static void commands(ChapterTestSupport t) throws Exception {
        Path v = t.vault();
        for (var entry : OPTIONS.entrySet()) {
            String name = entry.getKey();
            eq(stderr(error(v, 409, name)), NO_PART4, "missing positional returns WL-409: " + name);
            for (String option : entry.getValue()) eq(stderr(error(v, 409, name, option + "=x")), NO_PART4, "allowed option " + name + " " + option);
            var unknown = error(v, 400, name, "unknown=x"); check(stderr(unknown).contains("모르는 옵션이다: unknown"), "unknown option first");
            eq(run(v, name.replace('-', '_')).status(), 2, "no tool surface " + name);
            eq(stderr(error(v, 409, name, "\u0000wl-session-id", "session")), NO_PART4, "command surface discards session marker");
        }
        for (String name : List.of("get-ask", "list-chats", "append-ask-turn", "append-chat-turn")) {
            var r = error(v, 400, name, "one", "two", "three");
            check(stderr(r).contains("자리 인자는 2 개인데 3 개"), "positional maximum " + name);
            if (name.startsWith("append-")) check(stderr(r).contains("[images=<attach/이름,…>]"), "exact usage ellipsis like manual 32 and A.3 " + name);
            error(v, 400, name, "one", "two", "three", "unknown=x");
        }
        error(v, 400, "read-session", "one", "two"); error(v, 400, "recent-sessions", "one");
        String[] many = new String[150]; Arrays.fill(many, "session"); String[] argv = new String[151]; argv[0] = "session-titles"; System.arraycopy(many, 0, argv, 1, many.length);
        error(v, 409, argv);
        check(CommandRegistry.USAGE_LINES.stream().filter(s -> s.startsWith("append-ask-turn ") || s.startsWith("append-chat-turn ")).allMatch(s -> s.contains("[images=<attach/이름,…>]")), "registry usage matches chapter 32");
        Files.delete(v.resolve(".withlog/workspace.json"));
        for (String name : List.of("append-ask-turn", "append-chat-turn")) {
            check(stderr(error(v, 400, name)).contains("워크스페이스"), "workspace before no part4");
            check(stderr(error(v, 400, name, "unknown=x")).contains("모르는 옵션"), "syntax before workspace");
        }
        error(v, 409, "get-ask"); error(v, 409, "read-session");
        put(v, ".withlog/workspace.json", "{}");
        put(v, ".withlog/agendas/　.md", "---\nid: agfixture-00000002\n---\n");
        for (String name : List.of("append-ask-turn", "append-chat-turn")) error(v, 400, name);
        error(v, 409, "get-ask");
    }

    private static void files(ChapterTestSupport t) throws Exception {
        Path v = t.vault(); root(v, "root", 1, "2026-06-16T10:00:00+09:00", "", "본문\n");
        note(v, "notes/demo/root/sub/child/note.md", SUB, "demo", "2026-06-16T10:00:00+09:00", "", "child\n");
        String root = "notes/demo/root", child = root + "/sub/child";
        put(v, root + "/ask.json", "not even JSON\n"); put(v, root + "/chats/one.json", "\0some chat\n");
        put(v, child + "/ask.json", "child thread\n"); put(v, child + "/chats/two.json", "child chat\n");
        String ghost = "---\nid: ndfixture-00000009\nagenda: demo\ntype: free\ntitle: ghost\ncreated: '2026-06-16'\n---\nx\n";
        put(v, "notes/demo/orphan/ask.json", ghost); put(v, "notes/demo/orphan/chats/ghost.json", ghost);
        put(v, root + "/sub/ask.json", ghost); put(v, root + "/sub/chats/ghost.json", ghost);
        eq(ok(v, "reindex").get("indexed"), 1L, "ask and chats do not create root notes");
        eq(((List<?>) ok(v, "list-sub-notes", ID).get("subNotes")).size(), 1, "ask and chats do not create subnotes");
        for (String path : List.of(root + "/ask.json", root + "/chats/one.json", child + "/ask.json", child + "/chats/two.json"))
            eq(ok(v, "reindex-paths", path).get("indexed"), 1L, "restricted path folds to root " + path);
        var exported = ok(v, "export-markdown"); Path dest = Path.of(exported.get("dest").toString());
        try (var paths = Files.walk(dest)) { check(paths.filter(Files::isRegularFile).noneMatch(p -> p.getFileName().toString().equals("ask.json") || p.toString().contains("/chats/")), "export excludes part4 files"); }
        var plan = ok(v, "shard-notes"); String mapping = Files.readString(Path.of(plan.get("mappingFile").toString()));
        String row = mapping.lines().filter(s -> s.startsWith(ID + "\t")).findFirst().orElseThrow();
        String[] cols = row.split("\t", -1);
        try (var paths = Files.walk(v.resolve(root))) { eq(Long.parseLong(cols[3]), paths.filter(Files::isRegularFile).count(), "shard file count includes part4"); }
        try (var paths = Files.walk(v.resolve(root))) { long bytes = 0; for (Path p : paths.filter(Files::isRegularFile).toList()) bytes += Files.size(p); eq(Long.parseLong(cols[4]), bytes, "shard byte sum includes part4"); }
        var updated = ok(v, "update-doc", ID, "title=moved", "f1=" + ok(v, "get-doc", ID).get("precondition")); root = updated.get("path").toString();
        eq(Files.readString(v.resolve(root + "/ask.json")), "not even JSON\n", "root rename preserves ask bytes");
        eq(Files.readString(v.resolve(root + "/sub/child/chats/two.json")), "child chat\n", "root rename moves child chats");
        var subUpdated = ok(v, "update-sub-note", ID, SUB, "title=renamed", "f1=" + ok(v, "get-sub-note", ID, SUB).get("precondition"));
        child = subUpdated.get("path").toString(); eq(Files.readString(v.resolve(child + "/ask.json")), "child thread\n", "sub rename moves ask");
        eq(Files.readString(v.resolve(child + "/chats/two.json")), "child chat\n", "sub rename moves chats");
        var removed = ok(v, "delete-doc", ID); Path archive = v.resolve(".withlog/trash/files").resolve(removed.get("file").toString());
        check(Files.exists(archive.resolve("ask.json")), "trash retains root ask"); check(Files.exists(archive.resolve("sub").resolve(Path.of(child).getFileName()).resolve("chats/two.json")), "trash retains child chat");
        var restored = ok(v, "restore-doc", removed.get("trashId").toString()); root = restored.get("path").toString();
        check(Files.exists(v.resolve(root + "/ask.json")), "restore keeps ask");
        ok(v, "delete-sub-note", ID, SUB); check(!Files.exists(v.resolve(child)), "delete child removes part4 with folder");
        removed = ok(v, "delete-doc", ID); archive = v.resolve(".withlog/trash/files").resolve(removed.get("file").toString());
        ok(v, "purge-doc", removed.get("trashId").toString()); check(!Files.exists(archive), "purge removes root with part4 files");
    }

    private static String git(Path v, String... args) throws Exception {
        List<String> cmd = new ArrayList<>(List.of("git", "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgsign=false")); cmd.addAll(List.of(args));
        var builder = new ProcessBuilder(cmd).directory(v.toFile()).redirectErrorStream(true); builder.environment().remove("GIT_INDEX_FILE"); builder.environment().put("GIT_DIR", v.resolve(".git").toString()); builder.environment().put("GIT_WORK_TREE", v.toString());
        Process p = builder.start(); String output = new String(p.getInputStream().readAllBytes(), StandardCharsets.UTF_8);
        if (p.waitFor() != 0) throw new AssertionError("git: " + output); return output;
    }
    private static void shards(ChapterTestSupport t) throws Exception {
        Path v = t.vault(); root(v, "root", 1, "2026-06-16T10:00:00+09:00", "", "본문\n");
        put(v, "notes/demo/root/ask.json", "thread"); put(v, "notes/demo/root/chats/session.json", "conversation");
        sharded(v, true); var result = ok(v, "shard-notes", "apply=true");
        eq(result.get("moved"), 1L, "shard moves root with part4 files");
        String mapping = Files.readString(Path.of(result.get("mappingFile").toString()));
        String[] row = mapping.lines().filter(s -> s.startsWith(ID + "\t")).findFirst().orElseThrow().split("\t", -1);
        Path target = v.resolve(row[2]); eq(Files.readString(target.resolve("ask.json")), "thread", "shard preserves ask");
        eq(Files.readString(target.resolve("chats/session.json")), "conversation", "shard preserves chat");
        var trashed = ok(v, "delete-doc", ID); Path archive = v.resolve(".withlog/trash/files").resolve(trashed.get("file").toString());
        ok(v, "empty-trash"); check(!Files.exists(archive), "empty trash removes part4 files");
        withlog.config.EngineConfig.activate(null);
    }
    private static void pending(ChapterTestSupport t) throws Exception {
        Path v = t.vault(); git(v, "init", "-b", "main"); VaultGit.serving(v).commit("startup");
        put(v, "outside", "first");
        EngineResult syntax = EngineExecutor.execute(v, new String[]{"append-ask-turn", "unknown=x"}, true, 0);
        eq(syntax.errorCode(), 400, "syntax failure in serve"); VaultGit.serving(v).commit("restore");
        check(git(v, "show", "-s", "--format=%B", "HEAD").startsWith("wl(outside)"), "syntax failure leaves no writer record");
        put(v, "outside", "second");
        EngineResult missing = EngineExecutor.execute(v, new String[]{"append-chat-turn", "\u0000wl-session-id", "discarded"}, true, 0);
        eq(missing.errorCode(), 409, "unavailable write in serve"); eq(stderr(missing), NO_PART4, "serve error bytes"); VaultGit.serving(v).commit("restore");
        String message = git(v, "show", "-s", "--format=%B", "HEAD");
        check(message.startsWith("wl(app)"), "unavailable write records app writer"); check(!message.contains("Withlog-Session"), "command discards session marker");
    }
    private static void doctor(ChapterTestSupport t) throws Exception {
        Path v = t.vault(), home = Files.createDirectory(t.temp.resolve("doctor-home"));
        ProcessBuilder b = new ProcessBuilder(Path.of(System.getProperty("java.home"), "bin/java").toString(), "-Duser.home=" + home, "-jar", Path.of("dist/wl-cli.jar").toAbsolutePath().toString(), "doctor").redirectErrorStream(true);
        b.environment().put("WITHLOG_VAULT", v.toString()); b.environment().put("WITHLOG_AGENT_SOCKET", "/");
        b.environment().put("WITHLOG_SIDECAR_SOCKET", "/tmp/wl-ch32-doctor.sock");
        Process p = b.start(); check(p.waitFor(15, TimeUnit.SECONDS), "doctor returns"); String output = new String(p.getInputStream().readAllBytes(), StandardCharsets.UTF_8);
        eq(p.exitValue(), 0, "part 1 doctor exits normally: " + output); check(!output.contains("wl-agent 소켓") && !output.contains("wl-agent 로그"), "part 1 doctor omits optional part4 lines");
    }
}
