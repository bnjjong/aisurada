package withlog.git;

import withlog.config.EngineConfig;
import withlog.text.Trim;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Clock;
import java.time.Duration;
import java.time.Instant;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.locks.ReentrantLock;

/** 7.1/31.1~31.3: vault별 커밋 대기 기록. 실제 쓰기는 잠그지 않고 git과 기록만 같은 잠금으로 보호한다. */
public final class VaultGit {
    private static final Map<Path, VaultGit> SERVING = new ConcurrentHashMap<>();
    private final Path vault;
    private final boolean enabled;
    private final long idleSeconds;
    private final Clock clock;
    private final Runner runner;
    private final ReentrantLock lock = new ReentrantLock();
    private final LinkedHashSet<String> writers = new LinkedHashSet<>(), sessions = new LinkedHashSet<>(), pairs = new LinkedHashSet<>();
    private Instant firstWrite, lastWrite, retryAfter = Instant.EPOCH;
    private int failures;

    @FunctionalInterface interface Runner { CommandResult run(List<String> args, byte[] input); }
    record CommandResult(int code, byte[] output) { String text() { return new String(output, StandardCharsets.UTF_8); } }
    public record Result(String outcome, String blocked) {}

    VaultGit(Path vault, boolean enabled, long idleSeconds, Clock clock, Runner runner) {
        this.vault = vault.toAbsolutePath().normalize();
        this.enabled = enabled; this.idleSeconds = idleSeconds; this.clock = clock;
        this.runner = runner == null ? new GitProcess(this.vault) : runner;
    }

    private static VaultGit create(Path vault) {
        EngineConfig config = EngineConfig.active();
        return new VaultGit(vault, config == null || config.autocommitEnabled(),
                config == null ? 300 : config.autocommitIdleSeconds(), Clock.systemUTC(), null);
    }

    public static VaultGit serving(Path vault) {
        return SERVING.computeIfAbsent(vault.toAbsolutePath().normalize(), VaultGit::create);
    }

    /** 단발 호출끼리는 대기 기록을 공유하지 않는다. serve 요청만 같은 vault의 기록을 공유한다. */
    public static VaultGit forRequest(Path vault, boolean serve) { return serve ? serving(vault) : create(vault); }

    public void beforeWrite(boolean tool, String session) {
        lock.lock();
        try {
            String pair = (tool ? "tool:" : "app:") + (tool && session != null ? session : "");
            if (!pairs.isEmpty() && !(pairs.size() == 1 && pairs.contains(pair)) && !clock.instant().isBefore(retryAfter)) {
                attemptLocked("writer");
            }
        } finally { lock.unlock(); }
    }

    public void afterWrite(boolean tool, String session) {
        lock.lock();
        try {
            if (!enabled) return;
            writers.add(tool ? "tool" : "app");
            if (tool && session != null) sessions.add(session);
            pairs.add((tool ? "tool:" : "app:") + (tool && session != null ? session : ""));
            Instant now = clock.instant();
            if (firstWrite == null) firstWrite = now;
            lastWrite = now;
        } finally { lock.unlock(); }
    }

    public void tick() {
        lock.lock();
        try {
            Instant now = clock.instant();
            if (pairs.isEmpty() || now.isBefore(retryAfter)) return;
            if (Duration.between(lastWrite, now).getSeconds() >= idleSeconds) attemptLocked("idle");
            else if (Duration.between(firstWrite, now).compareTo(Duration.ofMinutes(15)) >= 0) attemptLocked("overdue");
        } finally { lock.unlock(); }
    }

    /** startup/shutdown은 재시도 시각을 기다리지 않는다. 커밋 실패는 쓰기의 실패가 아니다. */
    public Result commit(String reason) {
        lock.lock();
        try { return attemptLocked(reason); }
        finally { lock.unlock(); }
    }

    private Result attemptLocked(String reason) {
        Result result;
        try { result = attempt(reason); }
        catch (Exception e) { result = new Result("failed", null); }
        switch (result.outcome()) {
            case "committed", "nothing" -> { clearPending(); failures = 0; }
            case "disabled" -> clearPending();
            case "skipped" -> retryAfter = clock.instant().plusSeconds(30);
            case "failed" -> {
                failures = Math.min(failures + 1, 5);
                retryAfter = clock.instant().plusSeconds(Math.min(480, 30L << (failures - 1)));
                System.err.println("경고: vault git 커밋을 못 했다. 다음 정비에서 다시 시도한다: " + vault);
            }
            default -> throw new IllegalStateException(result.outcome());
        }
        return result;
    }

    private void clearPending() { writers.clear(); sessions.clear(); pairs.clear(); firstWrite = lastWrite = null; }

    private Result attempt(String reason) {
        if (!enabled) return new Result("disabled", "off");
        Path git = vault.resolve(".git");
        if (!Files.isDirectory(git)) return new Result("disabled", "not-repo");
        Path indexLock = git.resolve("index.lock");
        try {
            if (Duration.between(Files.getLastModifiedTime(indexLock).toInstant(), clock.instant()).compareTo(Duration.ofMinutes(10)) <= 0)
                return new Result("skipped", "index-lock");
            Files.delete(indexLock);
            System.err.println("경고: 10분 넘은 vault git 잠금을 지웠다: " + indexLock);
        } catch (java.nio.file.NoSuchFileException ignored) {
            // 깨진 링크도 따라가면 없으므로 잠금이 아니다.
        } catch (IOException e) { return new Result("skipped", "index-lock"); }
        for (String marker : List.of("rebase-merge", "rebase-apply", "MERGE_HEAD", "CHERRY_PICK_HEAD", "REVERT_HEAD")) {
            if (Files.exists(git.resolve(marker))) return new Result("skipped", "in-progress");
        }
        if (!Trim.trim(run(List.of("symbolic-ref", "-q", "HEAD"), new byte[0]).text()).equals("refs/heads/main"))
            return new Result("skipped", "not-main");
        if (run(List.of("add", "-A", "--", ".", ":(exclude)*.tmp"), new byte[0]).code() != 0) return new Result("failed", null);
        var diff = run(List.of("diff", "--cached", "--no-renames", "--name-status", "-z"), new byte[0]);
        if (diff.code() != 0) return new Result("failed", null);
        List<Change> changes = new ArrayList<>();
        String[] fields = diff.text().split("\\x00", -1);
        for (int i = 0; i + 1 < fields.length; i += 2) {
            if (!fields[i].isEmpty() && !fields[i + 1].isEmpty()) changes.add(new Change(fields[i].charAt(0), fields[i + 1]));
        }
        if (changes.isEmpty()) return new Result("nothing", null);
        var ids = noteIds(changes);
        String actors = writers.isEmpty() ? "outside" : String.join(",", writers);
        String actorValues = writers.isEmpty() ? "outside" : String.join(" ", writers);
        StringBuilder message = new StringBuilder("wl(" + actors + "): 노트 " + ids.size() + "개, 파일 " + changes.size() + "개\n\n")
                .append("Withlog-Writer: ").append(actorValues).append('\n').append("Withlog-Reason: ").append(reason).append('\n');
        if (!sessions.isEmpty()) message.append("Withlog-Session: ").append(String.join(" ", sessions)).append('\n');
        if (!ids.isEmpty()) message.append("Withlog-Notes: ").append(String.join(" ", ids.stream().limit(200).toList())).append('\n');
        return run(List.of("commit", "-q", "--no-verify", "-F", "-"), message.toString().getBytes(StandardCharsets.UTF_8)).code() == 0
                ? new Result("committed", null) : new Result("failed", null);
    }

    private record Change(char status, String path) {}
    private CommandResult run(List<String> args, byte[] input) { return runner.run(args, input); }

    private LinkedHashSet<String> noteIds(List<Change> changes) {
        List<String> deleted = changes.stream().filter(c -> c.status() == 'D' && notePath(c.path())).map(Change::path).toList();
        Map<String, byte[]> previous = new LinkedHashMap<>();
        if (!deleted.isEmpty()) {
            byte[] input = (String.join("\n", deleted.stream().map(p -> "HEAD:" + p).toList()) + "\n").getBytes(StandardCharsets.UTF_8);
            var batch = run(List.of("cat-file", "--batch"), input);
            if (batch.code() == 0) {
                List<byte[]> contents = GitBatch.read(deleted.size(), batch.output());
                for (int i = 0; i < deleted.size(); i++) previous.put(deleted.get(i), contents.get(i));
            }
        }
        LinkedHashSet<String> ids = new LinkedHashSet<>();
        for (Change change : changes) {
            if (!notePath(change.path())) continue;
            byte[] bytes = previous.get(change.path());
            if (change.status() != 'D') {
                try (var file = Files.newInputStream(vault.resolve(change.path()))) { bytes = file.readNBytes(4096); }
                catch (IOException e) { continue; }
            }
            String id = headerId(bytes);
            if (id != null) ids.add(id);
        }
        return ids;
    }

    private static boolean notePath(String path) { return path.substring(path.lastIndexOf('/') + 1).equals("note.md"); }

    static String headerId(byte[] bytes) {
        if (bytes == null) return null;
        String text = new String(bytes, StandardCharsets.UTF_8);
        if (!text.startsWith("---")) return null;
        int end = text.indexOf("\n---", 3);
        if (end < 0) return null;
        for (String line : text.substring(3, end).split("\n")) {
            if (!line.startsWith("id:")) continue;
            String id = Trim.trim(line.substring(3));
            int first = 0, last = id.length();
            while (first < last && (id.charAt(first) == '\'' || id.charAt(first) == '"')) first++;
            while (last > first && (id.charAt(last - 1) == '\'' || id.charAt(last - 1) == '"')) last--;
            id = id.substring(first, last);
            return id.length() <= 128 && id.matches("[\\p{L}\\p{N}_-]+") ? id : null;
        }
        return null;
    }

}
