package withlog.git;

import withlog.text.Trim;
import withlog.vault.Frontmatter;
import withlog.write.MetaWriter;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Instant;
import java.time.ZoneOffset;
import java.time.format.DateTimeFormatter;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;

/** 31.4: 경로 이력, 꼬리 줄, 옛 스냅숏을 합친다. 각 git 실패는 그 걸음의 빈 결과다. */
public final class VaultHistory {
    private static final byte[] EMPTY = new byte[0];
    private static final DateTimeFormatter TAKEN_AT = DateTimeFormatter.ofPattern("yyyy-MM-dd'T'HH:mm:ssxxx").withZone(ZoneOffset.ofHours(9));
    private final Path vault;
    private final VaultGit.Runner git, legacy;

    public record Version(String commit, long seconds, String source, String path, long bytes, String blob) {}
    public record Content(String full, String body, Map<String, Object> frontmatter) {}

    public VaultHistory(Path vault) { this(vault, new GitProcess(vault), new LegacyGitProcess(vault)); }
    VaultHistory(Path vault, VaultGit.Runner git, VaultGit.Runner legacy) {
        this.vault = vault; this.git = git; this.legacy = legacy;
    }

    public static boolean commitShape(String value) {
        if (value == null || value.length() != 40) return false;
        for (int i = 0; i < value.length(); i++) {
            char c = value.charAt(i);
            if (!(c >= 'a' && c <= 'f') && !Character.isDigit(c)) return false;
        }
        return true;
    }
    static boolean safeId(String id) { return id != null && id.matches("[A-Za-z0-9_-]{1,128}"); }
    static boolean safePath(String path) {
        if (path == null || path.isEmpty() || path.length() > 1024 || path.startsWith("/") || path.startsWith("-")
                || path.indexOf('\0') >= 0 || path.indexOf('\n') >= 0) return false;
        for (String part : path.split("/", -1)) if (part.equals("..")) return false;
        return true;
    }

    public List<Version> versions(String id) {
        if (!safeId(id) || !Files.isDirectory(vault.resolve(".git"))) return List.of();
        List<Version> versions = new ArrayList<>();
        var paths = run(git, List.of("grep", "-l", "-z", "-F", "-e", id, "--", "*note.md"), EMPTY);
        if (paths.code() == 0) for (String path : paths.text().split("\u0000")) {
            if (!safePath(path)) continue;
            try (var file = Files.newInputStream(vault.resolve(path))) {
                if (!id.equals(VaultGit.headerId(file.readNBytes(4096)))) continue;
            } catch (Exception e) { continue; }
            var log = run(git, List.of("log", "main", "--follow", "--format=%x1e%H%x1f%ct%x1f%an", "--name-only", "--", path), EMPTY);
            if (log.code() != 0) continue;
            for (String record : log.text().split("\u001e")) {
                List<String> lines = record.lines().filter(s -> !Trim.isBlank(s)).toList();
                if (lines.size() < 2) continue;
                Version version = entry(lines.getFirst().split("\u001f", -1), Trim.trim(lines.get(1)));
                if (version != null) versions.add(version);
            }
        }
        Set<String> found = new HashSet<>();
        for (var version : versions) found.add(version.commit());
        var trailers = run(git, List.of("log", "main", "--format=%x1e%H%x1f%ct%x1f%an%x1f%B", "-F", "--grep=" + id), EMPTY);
        if (trailers.code() == 0) for (String record : trailers.text().split("\u001e")) {
            String[] fields = record.split("\u001f", 4);
            if (fields.length != 4 || found.contains(fields[0]) || !mentions(fields[3], id)) continue;
            Version version = entry(fields, "");
            if (version == null) continue;
            var diff = run(git, List.of("diff-tree", "-r", "--root", "--no-commit-id", "--name-only", "-z", version.commit()), EMPTY);
            if (diff.code() != 0) continue;
            List<String> candidates = new ArrayList<>();
            for (String path : diff.text().split("\u0000"))
                if (safePath(path) && path.substring(path.lastIndexOf('/') + 1).equals("note.md")) candidates.add(path);
            List<Version> options = new ArrayList<>();
            for (String path : candidates) options.add(new Version(version.commit(), version.seconds(), version.source(), path, 0, null));
            List<byte[]> bodies = readBytes(options);
            for (int i = 0; i < candidates.size(); i++) if (id.equals(VaultGit.headerId(bodies.get(i)))) {
                versions.add(options.get(i)); break;
            }
        }
        Map<String, Version> unique = new LinkedHashMap<>();
        for (var version : versions) unique.putIfAbsent(version.commit() + "\0" + version.path(), version);
        versions = new ArrayList<>(unique.values());
        versions.addAll(snapshots(id));
        return fold(versions);
    }

    private static Version entry(String[] fields, String path) {
        if (fields.length < 3 || !commitShape(fields[0])) return null;
        try {
            long seconds = Long.parseLong(fields[1]);
            TAKEN_AT.format(Instant.ofEpochSecond(seconds));
            return new Version(fields[0], seconds, Trim.trim(fields[2]).equals("withlog-auto") ? "auto" : "manual", path, 0, null);
        } catch (RuntimeException e) { return null; }
    }
    private static boolean mentions(String message, String id) {
        return message.lines().filter(line -> line.startsWith("Withlog-Notes:"))
                .anyMatch(line -> List.of(Trim.trim(line.substring("Withlog-Notes:".length())).split(" ", -1)).contains(id));
    }

    private List<Version> snapshots(String id) {
        List<Version> result = new ArrayList<>();
        var log = run(legacy, List.of("log", "--format=%H %ct", "refs/withlog/snapshots/" + id), EMPTY);
        if (log.code() != 0) return result;
        for (String line : log.text().lines().toList()) {
            String[] fields = Trim.trim(line).split(" ", -1);
            if (fields.length != 2 || !commitShape(fields[0])) continue;
            try {
                long seconds = Long.parseLong(fields[1]);
                TAKEN_AT.format(Instant.ofEpochSecond(seconds));
                var tree = run(legacy, List.of("ls-tree", "-l", fields[0]), EMPTY);
                if (tree.code() != 0) continue;
                String first = tree.text().lines().filter(s -> !Trim.isBlank(s)).findFirst().orElse("");
                String[] pair = first.split("\t", 2);
                if (pair.length != 2) continue;
                String[] meta = pair[0].split(" ");
                long size = Long.parseLong(meta[meta.length - 1]);
                result.add(new Version(fields[0], seconds, "snapshot", Trim.trim(pair[1]), size, null));
            } catch (RuntimeException ignored) {}
        }
        return result;
    }

    private List<Version> fold(List<Version> versions) {
        if (versions.isEmpty()) return List.of();
        var check = run(git, List.of("cat-file", "--batch-check"), GitBatch.input(versions.stream().map(VaultHistory::spec).toList()));
        if (check.code() != 0) return List.of();
        List<String> lines = check.text().lines().filter(s -> !Trim.isBlank(s)).toList();
        List<Version> valid = new ArrayList<>();
        for (int i = 0; i < Math.min(lines.size(), versions.size()); i++) {
            String[] fields = lines.get(i).split(" ");
            if (fields.length != 3 || !commitShape(fields[0]) || !fields[1].equals("blob")) continue;
            try {
                long bytes = Long.parseLong(fields[2]);
                if (bytes < 0) continue;
                var version = versions.get(i);
                valid.add(new Version(version.commit(), version.seconds(), version.source(), version.path(), bytes, fields[0]));
            } catch (NumberFormatException ignored) {}
        }
        valid.sort(Comparator.comparingLong(Version::seconds).reversed()); // 같은 시각이면 모은 차례를 지킨다
        List<Version> folded = new ArrayList<>();
        for (var version : valid)
            if (folded.isEmpty() || !folded.getLast().blob().equals(version.blob())) folded.add(version);
        return folded;
    }

    private static String spec(Version v) { return v.commit() + ":" + v.path(); }
    private List<byte[]> readBytes(List<Version> versions) {
        List<byte[]> result = new ArrayList<>();
        for (int i = 0; i < versions.size(); i++) result.add(null);
        List<Integer> readable = new ArrayList<>();
        List<String> specs = new ArrayList<>();
        for (int i = 0; i < versions.size(); i++) {
            var version = versions.get(i);
            if (commitShape(version.commit()) && safePath(version.path()) && version.bytes() <= Integer.MAX_VALUE) {
                readable.add(i); specs.add(spec(version));
            }
        }
        if (specs.isEmpty()) return result;
        var batch = run(git, List.of("cat-file", "--batch"), GitBatch.input(specs));
        if (batch.code() != 0) return result;
        List<byte[]> bodies = GitBatch.read(specs.size(), batch.output());
        for (int i = 0; i < readable.size(); i++) result.set(readable.get(i), bodies.get(i));
        return result;
    }

    public List<Content> read(List<Version> versions) {
        List<Content> result = new ArrayList<>();
        for (byte[] bytes : readBytes(versions)) {
            if (bytes == null) { result.add(null); continue; }
            String text = new String(bytes, StandardCharsets.UTF_8);
            var parts = MetaWriter.split(text);
            Map<String, Object> fm = parts.fenceFound() && parts.yamlPart() != null && parts.yamlPart().length() <= 16_000
                    ? Frontmatter.parse(parts.yamlPart()) : null;
            result.add(new Content(text, fm == null ? text : parts.body(), fm));
        }
        return result;
    }

    public static Map<String, Object> row(Version v, int index, Content content) {
        Map<String, Object> row = new LinkedHashMap<>();
        row.put("index", index); row.put("takenAt", TAKEN_AT.format(Instant.ofEpochSecond(v.seconds())));
        row.put("source", v.source()); row.put("commit", v.commit()); row.put("path", v.path()); row.put("bytes", v.bytes());
        Map<String, Object> fm = content == null ? null : content.frontmatter();
        if (fm != null && fm.get("editedBy") instanceof String by && fm.get("editedAt") instanceof String at
                && !Trim.isBlank(by) && !Trim.isBlank(at)) {
            row.put("editedBy", Trim.trim(by));
            if (fm.get("editedSessionId") instanceof String session && !Trim.isBlank(session)) row.put("editedSessionId", Trim.trim(session));
        }
        return row;
    }

    private static VaultGit.CommandResult run(VaultGit.Runner runner, List<String> args, byte[] input) {
        try { return runner.run(args, input); }
        catch (Exception e) { return new VaultGit.CommandResult(-1, EMPTY); }
    }
}
