package withlog.git;

import withlog.config.EngineConfig;
import withlog.text.Trim;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

/** 31.6: 원자적 본문 쓰기 바로 전에 옛 바이트를 노트별 참조에 남기는 선택 기능. */
public final class VaultSnapshot {
    private VaultSnapshot() {}
    public static void beforeWrite(Path vault, String id, Path file) {
        EngineConfig config = EngineConfig.active();
        if (config == null || !config.snapshotEnabled()) return;
        save(vault, id, file, new LegacyGitProcess(vault));
    }

    static void save(Path vault, String id, Path file, VaultGit.Runner git) {
        try {
            if (!VaultHistory.safeId(id) || !Files.exists(vault.resolve(".git")) || !Files.exists(file)) return;
            String blob = value(git, List.of("hash-object", "-w", "--", file.toAbsolutePath().toString()), new byte[0]);
            String name = file.getFileName().toString();
            if (!VaultHistory.commitShape(blob) || name.isEmpty() || name.length() > 255 || name.contains("/") || name.contains("..")) return;
            String tree = value(git, List.of("mktree"), ("100644 blob " + blob + "\t" + name + "\n").getBytes(StandardCharsets.UTF_8));
            if (tree == null) return;
            String ref = "refs/withlog/snapshots/" + id;
            String parent = value(git, List.of("rev-parse", "--verify", "-q", ref), new byte[0]);
            List<String> args = new ArrayList<>(List.of("commit-tree", tree));
            if (parent != null) args.addAll(List.of("-p", parent));
            String relative;
            try { relative = vault.toAbsolutePath().relativize(file.toAbsolutePath()).toString().replace('\\', '/'); }
            catch (IllegalArgumentException e) { relative = file.toAbsolutePath().toString(); }
            args.addAll(List.of("-m", "withlog-auto: " + id + " — " + relative));
            String commit = value(git, args, new byte[0]);
            if (commit != null) git.run(List.of("update-ref", ref, commit), new byte[0]);
        } catch (Exception ignored) {
            // 스냅숏은 본문 쓰기의 성공 조건이 아니다. 실패해도 재시도하지 않는다.
        }
    }

    private static String value(VaultGit.Runner git, List<String> args, byte[] input) {
        var result = git.run(args, input);
        String value = Trim.trim(result.text());
        return result.code() == 0 && !value.isEmpty() ? value : null;
    }
}
