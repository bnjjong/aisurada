package withlog.git;

import java.io.ByteArrayOutputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.TimeUnit;

/** 실패한 git, 배치 경계와 옛 실행 규약을 격리된 프로세스 및 출력 주입으로 검증한다. */
public final class VaultHistoryTest {
    private static int checks;
    private static final byte[] EMPTY = new byte[0];
    private static final String A = "a".repeat(40), B = "b".repeat(40), C = "c".repeat(40), D = "d".repeat(40);
    private static final String X = "1".repeat(40), Y = "2".repeat(40), Z = "3".repeat(40);
    public static void main(String[] args) throws Exception {
        if (args.length > 0) { probe(Path.of(args[0])); return; }
        Path temp = Files.createTempDirectory("aisurada-history-test-");
        try {
            Path vault = Files.createDirectory(temp.resolve("vault")); Files.createDirectory(vault.resolve(".git"));
            batch(); folds(vault); failures(vault); stableOrder(vault); snapshot(vault); process(temp, vault);
            System.out.println("vault history: " + checks + " checks passed");
        } finally { try (var paths = Files.walk(temp)) { for (var path : paths.sorted(Comparator.reverseOrder()).toList()) Files.deleteIfExists(path); } }
    }
    private static VaultGit.CommandResult result(int code, String text) { return new VaultGit.CommandResult(code, text.getBytes(StandardCharsets.UTF_8)); }
    private static byte[] batchBytes(byte[]... bodies) throws Exception {
        ByteArrayOutputStream stream = new ByteArrayOutputStream();
        for (byte[] body : bodies) {
            if (body == null) stream.write("x missing\n".getBytes(StandardCharsets.UTF_8));
            else { stream.write((A + " blob " + body.length + "\n").getBytes(StandardCharsets.UTF_8)); stream.write(body); stream.write('\n'); }
        }
        return stream.toByteArray();
    }
    private static void batch() throws Exception {
        byte[] text = "한글\n\u0000body".getBytes(StandardCharsets.UTF_8);
        var bodies = GitBatch.read(3, batchBytes(null, text, EMPTY));
        eq(bodies.getFirst(), null, "missing item placeholder"); check(Arrays.equals(bodies.get(1), text), "UTF8 byte length with NUL and LF"); eq(bodies.get(2).length, 0, "empty blob");
        bodies = GitBatch.read(2, (A + " blob 2147483648\n").getBytes(StandardCharsets.UTF_8));
        eq(bodies, Arrays.asList(null, null), "overflow body and following items unreadable");
        eq(GitBatch.read(1, (A + " blob 2\na\n").getBytes(StandardCharsets.UTF_8)).getFirst(), null, "truncated batch body rejected");
        byte[] output = (A + " tree 3\nabc\n" + A + " blob 1\nx\n").getBytes(StandardCharsets.UTF_8);
        bodies = GitBatch.read(2, output); eq(bodies.getFirst(), null, "tree unreadable"); eq(new String(bodies.get(1), StandardCharsets.UTF_8), "x", "tree body skipped before next blob");
    }
    private static void folds(Path vault) {
        VaultGit.Runner legacy = (args, input) -> {
            if (args.getFirst().equals("log")) return result(0, A + " 400\n" + B + " 300\n" + C + " 200\n" + D + " 100\n");
            return result(0, "100644 blob " + X + " 5\tnote.md\n");
        };
        List<List<String>> calls = new ArrayList<>();
        VaultGit.Runner git = (args, input) -> {
            calls.add(args);
            if (args.equals(List.of("cat-file", "--batch-check"))) return result(0,
                    X + " blob 5\n" + X + " blob 5\n" + Y + " blob 2147483648\n" + X + " blob 5\n");
            return result(1, "failure");
        };
        var history = new VaultHistory(vault, git, legacy); var versions = history.versions("valid");
        eq(versions.stream().map(VaultHistory.Version::commit).toList(), List.of(A, C, D), "adjacent duplicates folded; separated duplicate retained");
        eq(versions.get(1).bytes(), 2147483648L, "oversized blob version retained");
        eq(history.read(versions), Arrays.asList(null, null, null), "failed read keeps versions without content");
        int count = calls.size(); history.versions("unsafe.id"); eq(calls.size(), count, "unsafe id never invokes git");
        var row = VaultHistory.row(versions.get(1), 2, null);
        eq(row.keySet().stream().toList(), List.of("index", "takenAt", "source", "commit", "path", "bytes"), "unreadable has no stamp");
        eq(row.get("takenAt"), "1970-01-01T09:03:20+09:00", "epoch rendered KST seconds");
        var raw = new VaultHistory.Content("", "", Map.of("editedBy", "app", "editedAt", "x", "editedSessionId", " session "));
        eq(VaultHistory.row(versions.getFirst(), 1, raw).get("editedSessionId"), "session", "app session survives history");
        check(!VaultHistory.row(versions.getFirst(), 1, new VaultHistory.Content("", "", Map.of("editedBy", "tool"))).containsKey("editedBy"), "partial stamp excluded");
    }
    private static void failures(Path vault) throws Exception {
        VaultGit.Runner no = (args, input) -> { throw new IllegalStateException("cannot spawn git"); };
        eq(new VaultHistory(vault, no, no).versions("valid"), List.of(), "missing git gives empty successful history");
        List<byte[]> examples = List.of(
                "---\nid: valid\n---\r\n\nbody\n".getBytes(StandardCharsets.UTF_8),
                "\ufeffplain\n".getBytes(StandardCharsets.UTF_8),
                "---\n[not: a: map\n---\ntext\n".getBytes(StandardCharsets.UTF_8),
                new byte[]{'x', (byte) 0xff, 'y'});
        var versions = List.of(new VaultHistory.Version(A, 0, "manual", "note.md", 10, X));
        for (byte[] bytes : examples) {
            var process = new VaultHistory(vault, (args, input) -> {
                try { return new VaultGit.CommandResult(0, batchBytes(bytes)); }
                catch (Exception e) { throw new AssertionError(e); }
            }, no);
            var content = process.read(versions).getFirst(); check(content != null, "raw blob readable");
            if (bytes == examples.getFirst()) eq(content.body(), "body\n", "CRLF then LF stripped by common frontmatter split");
            else eq(content.body(), new String(bytes, StandardCharsets.UTF_8), "invalid/no frontmatter returns whole file");
        }
        check(VaultHistory.safePath("notes/한글/note.md"), "Unicode path safe");
        for (String path : List.of("", "/absolute", "-option", "x/../y", "x\ny", "x\u0000y", "x".repeat(1025))) check(!VaultHistory.safePath(path), "unsafe path rejected");
        check(VaultHistory.commitShape("２".repeat(40)), "BMP digit commit allowed");
        check(!VaultHistory.commitShape("𝟐".repeat(20)), "supplementary digit commit rejected");
    }
    private static void snapshot(Path vault) throws Exception {
        Path file = vault.resolve("note.json"); Files.writeString(file, "old bytes");
        List<List<String>> calls = new ArrayList<>(); List<String> inputs = new ArrayList<>();
        VaultGit.Runner git = (args, input) -> {
            calls.add(args); inputs.add(new String(input, StandardCharsets.UTF_8));
            return switch (args.getFirst()) {
                case "hash-object" -> result(0, A + "\n"); case "mktree" -> result(0, B + "\n");
                case "rev-parse" -> result(1, "failed previous ref"); case "commit-tree" -> result(0, C + "\n");
                default -> result(1, "ignored update-ref failure");
            };
        };
        VaultSnapshot.save(vault, "valid", file, git);
        eq(calls.stream().map(List::getFirst).toList(), List.of("hash-object", "mktree", "rev-parse", "commit-tree", "update-ref"), "legacy snapshot sequence");
        eq(inputs.get(1), "100644 blob " + A + "\tnote.json\n", "actual json filename in tree");
        check(!calls.get(3).contains("-p"), "failed parent lookup makes orphan snapshot");
        eq(calls.get(3).getLast(), "withlog-auto: valid — note.json", "snapshot commit message");
        int count = calls.size(); VaultSnapshot.save(vault, "invalid.id", file, git); VaultSnapshot.save(vault, "valid", vault.resolve("absent"), git);
        eq(calls.size(), count, "unsafe id and new file do not run git");
        Path bad = vault.resolve("bad..md"); Files.writeString(bad, "old");
        VaultSnapshot.save(vault, "valid", bad, git); eq(calls.size(), count + 1, "unsafe filename stops after hash-object");
    }
    private static void stableOrder(Path vault) throws Exception {
        Files.writeString(vault.resolve("note.md"), "---\nid: valid\n---\nbody");
        VaultGit.Runner git = (args, input) -> {
            if (args.getFirst().equals("grep")) return result(0, "note.md\u0000");
            if (args.contains("--follow")) return result(0, "\u001e" + A + "\u001f100\u001fHuman\n\nnote.md\n\u001e" + B + "\u001f100\u001fwithlog-auto\n\nnote.md\n");
            if (args.contains("--batch-check")) return result(0, X + " blob 1\n" + Y + " blob 1\n" + Z + " blob 1\n");
            return result(1, "failed log trailers");
        };
        VaultGit.Runner legacy = (args, input) -> args.getFirst().equals("log") ? result(0, C + " 100\n") : result(0, "100644 blob " + Z + " 1\tnote.md\n");
        var versions = new VaultHistory(vault, git, legacy).versions("valid");
        eq(versions.stream().map(VaultHistory.Version::commit).toList(), List.of(A, B, C), "equal timestamps keep path log then snapshot order");
        eq(versions.stream().map(VaultHistory.Version::source).toList(), List.of("manual", "auto", "snapshot"), "sources classified by author and snapshot reference");
        VaultGit.Runner failedCheck = (args, input) -> args.contains("--batch-check") ? result(1, "bad objects") : git.run(args, input);
        eq(new VaultHistory(vault, failedCheck, legacy).versions("valid"), List.of(), "batch-check failure drops all collected versions");
    }
    private static void process(Path temp, Path vault) throws Exception {
        Path bin = Files.createDirectory(temp.resolve("bin")); Path executable = bin.resolve("git");
        Files.writeString(executable, "#!/bin/sh\nif [ \"$1\" = large ]; then /usr/bin/head -c 200000 /dev/zero; exit; fi\nprintf '%s\\n' \"$GIT_DIR\" \"$GIT_WORK_TREE\" \"$GIT_INDEX_FILE\" \"$*\" \"$GIT_AUTHOR_NAME\" \"$GIT_COMMITTER_NAME\"\n");
        check(executable.toFile().setExecutable(true), "fake git executable");
        ProcessBuilder builder = new ProcessBuilder(Path.of(System.getProperty("java.home"), "bin/java").toString(), "-cp", System.getProperty("java.class.path"), VaultHistoryTest.class.getName(), vault.toString()).redirectErrorStream(true);
        var env = builder.environment(); env.put("PATH", bin + ":" + System.getenv("PATH"));
        env.put("GIT_DIR", "/outside/.git"); env.put("GIT_WORK_TREE", "/outside"); env.put("GIT_INDEX_FILE", "/outside/index");
        Process child = builder.start(); check(child.waitFor(12, TimeUnit.SECONDS), "legacy probe finishes");
        String output = new String(child.getInputStream().readAllBytes(), StandardCharsets.UTF_8);
        eq(child.exitValue(), 0, "legacy process assertions: " + output);
    }
    private static void probe(Path vault) {
        var process = new LegacyGitProcess(vault);
        var output = process.run(List.of("probe"), EMPTY); eq(output.code(), 0, "legacy environment probe exit");
        check(output.text().startsWith("/outside/.git\n/outside\n/outside/index\nprobe\n"), "legacy keeps git environment and has no config prefix");
        output = process.run(List.of("commit-tree", A, "-m", "message"), EMPTY);
        check(output.text().endsWith("withlog-auto\nwithlog-auto\n"), "legacy pins author and committer for commit-tree");
        long start = System.nanoTime(); output = process.run(List.of("large"), EMPTY);
        eq(output.code(), -1, "legacy output pipe saturation times out");
        check(TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - start) >= 4900, "legacy five-second timeout");
    }
    private static void check(boolean condition, String message) { checks++; if (!condition) throw new AssertionError(message); }
    private static void eq(Object actual, Object expected, String message) { check(Objects.equals(actual, expected), message + ": " + actual + " != " + expected); }
}
