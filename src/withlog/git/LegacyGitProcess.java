package withlog.git;

import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.TimeUnit;

/** 31.6의 옛 실행 규약: 환경/설정은 상속하고, 끝난 뒤 출력 읽기, 5초 뒤 KILL. */
final class LegacyGitProcess implements VaultGit.Runner {
    private final Path vault;
    LegacyGitProcess(Path vault) { this.vault = vault; }

    @Override public VaultGit.CommandResult run(List<String> args, byte[] input) {
        List<String> argv = new ArrayList<>(List.of("git"));
        argv.addAll(args);
        Process child = null;
        try {
            ProcessBuilder builder = new ProcessBuilder(argv).directory(vault.toFile()).redirectErrorStream(true);
            if (!args.isEmpty() && args.getFirst().equals("commit-tree")) {
                var env = builder.environment();
                env.put("GIT_AUTHOR_NAME", "withlog-auto"); env.put("GIT_COMMITTER_NAME", "withlog-auto");
                env.put("GIT_AUTHOR_EMAIL", "noreply@withlog.local"); env.put("GIT_COMMITTER_EMAIL", "noreply@withlog.local");
            }
            child = builder.start();
            try (var stdin = child.getOutputStream()) { stdin.write(input); }
            if (!child.waitFor(5, TimeUnit.SECONDS)) {
                child.destroyForcibly();
                return new VaultGit.CommandResult(-1, new byte[0]);
            }
            return new VaultGit.CommandResult(child.exitValue(), child.getInputStream().readAllBytes());
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            return new VaultGit.CommandResult(-1, new byte[0]);
        } catch (Exception e) {
            return new VaultGit.CommandResult(-1, new byte[0]);
        } finally {
            if (child != null) {
                if (child.isAlive()) child.destroyForcibly();
                try { child.getInputStream().close(); child.getOutputStream().close(); } catch (Exception ignored) {}
            }
        }
    }
}
