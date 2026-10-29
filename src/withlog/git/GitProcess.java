package withlog.git;

import java.io.IOException;
import java.nio.file.Path;
import java.time.Duration;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.TimeUnit;

/** 31.2: 출력 파이프를 먼저 비우고 60초 뒤 TERM, 3초 뒤 KILL한다. 셸은 사용하지 않는다. */
final class GitProcess implements VaultGit.Runner {
    private final Path vault;
    private final long timeoutMillis;
    private final long graceMillis;

    GitProcess(Path vault) { this(vault, Duration.ofSeconds(60), Duration.ofSeconds(3)); }
    GitProcess(Path vault, Duration timeout, Duration grace) {
        this.vault = vault;
        this.timeoutMillis = timeout.toMillis();
        this.graceMillis = grace.toMillis();
    }

    @Override public VaultGit.CommandResult run(List<String> arguments, byte[] input) {
        List<String> argv = new ArrayList<>(List.of("git", "-c", "core.hooksPath=/dev/null", "-c",
                "commit.gpgsign=false", "-c", "core.quotePath=false"));
        argv.addAll(arguments);
        Process process = null;
        try {
            ProcessBuilder builder = new ProcessBuilder(argv).directory(vault.toFile()).redirectErrorStream(true);
            var env = builder.environment();
            env.remove("GIT_INDEX_FILE");
            env.put("GIT_DIR", vault.resolve(".git").toString());
            env.put("GIT_WORK_TREE", vault.toString());
            if (!arguments.isEmpty() && arguments.get(0).equals("commit")) {
                env.put("GIT_AUTHOR_NAME", "withlog-auto"); env.put("GIT_COMMITTER_NAME", "withlog-auto");
                env.put("GIT_AUTHOR_EMAIL", "noreply@withlog.local"); env.put("GIT_COMMITTER_EMAIL", "noreply@withlog.local");
            }
            process = builder.start();
            Process child = process;
            CompletableFuture<byte[]> output = new CompletableFuture<>();
            Thread.ofVirtual().start(() -> {
                try { output.complete(child.getInputStream().readAllBytes()); }
                catch (IOException e) { output.completeExceptionally(e); }
            });
            // batch/commit의 입력도 자식과 병렬로 쓴다. 자식이 읽지 않더라도 시간 제한은 동작한다.
            CompletableFuture<Void> written = new CompletableFuture<>();
            Thread.ofVirtual().start(() -> {
                try (var stdin = child.getOutputStream()) { stdin.write(input); written.complete(null); }
                catch (IOException e) { written.completeExceptionally(e); }
            });
            if (!process.waitFor(timeoutMillis, TimeUnit.MILLISECONDS)) {
                process.destroy();
                if (!process.waitFor(graceMillis, TimeUnit.MILLISECONDS)) process.destroyForcibly();
                return new VaultGit.CommandResult(-1, new byte[0]);
            }
            byte[] bytes = output.get(Math.max(1, graceMillis), TimeUnit.MILLISECONDS);
            written.get(Math.max(1, graceMillis), TimeUnit.MILLISECONDS);
            return new VaultGit.CommandResult(process.exitValue(), bytes);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            return new VaultGit.CommandResult(-1, new byte[0]);
        } catch (Exception e) {
            return new VaultGit.CommandResult(-1, new byte[0]);
        } finally {
            if (process != null) {
                if (process.isAlive()) process.destroyForcibly();
                try { process.getInputStream().close(); process.getOutputStream().close(); } catch (IOException ignored) {}
            }
        }
    }
}
