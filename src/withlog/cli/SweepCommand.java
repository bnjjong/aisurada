package withlog.cli;

import withlog.serve.StateFile;
import withlog.text.Trim;
import withlog.vault.VaultKey;
import withlog.vault.VaultPath;

import java.io.File;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.time.Duration;
import java.time.Instant;
import java.util.*;

/**
 * wl-cli sweep 명령 및 하루 청소 자동 실행을 처리합니다. (설명서 3.5)
 */
public final class SweepCommand {

    private SweepCommand() {}

    public static void run(String[] args) {
        boolean dryRun = false;
        for (String a : args) {
            if ("--dry-run".equals(a)) {
                dryRun = true;
                break;
            }
        }

        String envVault = System.getenv("WITHLOG_VAULT");
        String currentVaultKey = null;
        if (envVault != null && !Trim.isBlank(envVault)) {
            // 엔진 갈래(3.3의 2번)처럼 3.1의 경로 정리를 지난 글로 키를 짓는다. 안 그러면 끝 / 나 상대 경로로 준 지금 vault를 남으로 본다
            String trimmedVault = Trim.trim(envVault);
            String normalizedVault;
            try {
                normalizedVault = VaultPath.normalize(trimmedVault);
            } catch (Exception e) {
                normalizedVault = trimmedVault;
            }
            currentVaultKey = VaultKey.compute(normalizedVault);
        }

        long quietHours = getQuietHours();
        String userHome = System.getProperty("user.home");
        Path runDir = Paths.get(userHome, "Library", "Application Support", "withlog", "run");

        List<Target> targets;
        try {
            targets = findTargets(runDir, currentVaultKey, quietHours);
        } catch (Exception e) {
            System.err.println("run 폴더 목록을 읽지 못했다: " + e.getMessage());
            System.exit(1);
            return;
        }

        System.out.println(quietHours + "시간 넘게 조용한 사이드카 " + targets.size() + "개");
        for (Target t : targets) {
            String v = t.state.getVault() != null ? t.state.getVault() : "(못 읽음)";
            String ago = TimeFormat.formatAgo(t.state.getLastActiveAt());
            String aliveStr = t.isAlive ? "그 pid 가 돈다" : "그 pid 로 도는 것이 없다";
            System.out.println("  " + t.label);
            System.out.println("    vault=" + v + "  마지막 활동 " + ago + "  (" + aliveStr + ")");
        }

        if (dryRun) {
            System.out.println("  아직 안 내렸다. 진짜 내리려면 --dry-run 를 뺀다.");
            System.exit(0);
            return;
        }

        // 실제 내리기
        String uid;
        try {
            uid = Launchctl.getUid();
        } catch (Launchctl.LaunchctlException e) {
            System.err.println(e.getMessage());
            System.exit(4);
            return;
        }

        for (Target t : targets) {
            Launchctl.Result r = Launchctl.runLaunchctl(List.of("bootout", "gui/" + uid + "/" + t.label));
            if (r.error() != null) {
                System.err.println("  내렸다: " + t.label + " (bootout 실패: " + r.error() + ")");
            } else {
                System.err.println("  내렸다: " + t.label + " (bootout rc=" + r.exitCode() + ")");
            }

            if (!t.isAlive) {
                try {
                    Files.deleteIfExists(t.file);
                    System.err.println("    죽은 흔적을 지웠다: " + t.file.getFileName());
                } catch (Exception ignored) {}
            }
        }

        recordLastSweep(userHome);
        System.exit(0);
    }

    /**
     * 엔진 호출 종료 시 호출되는 하루 한 번 청소.
     */
    public static void runDailySweepIfDue(String currentVaultKey) {
        String userHome = System.getProperty("user.home");
        Path lastSweepFile = Paths.get(userHome, "Library", "Caches", "withlog", "last-sweep");

        Instant now = Instant.now();
        if (Files.exists(lastSweepFile)) {
            try {
                String content = Trim.trim(Files.readString(lastSweepFile, StandardCharsets.UTF_8));
                Instant lastTime = Instant.parse(content);
                if (lastTime.isAfter(now)) {
                    // 미래 시각이면 그 시각 지나 24시간까지 안 돈다
                    return;
                }
                if (Duration.between(lastTime, now).toHours() < 24) {
                    return;
                }
            } catch (Exception ignored) {
                // 못 읽으면 지난 것으로 본다
            }
        }

        // 시각 파일 먼저 쓰기
        try {
            if (lastSweepFile.getParent() != null) {
                Files.createDirectories(lastSweepFile.getParent());
            }
            Files.writeString(lastSweepFile, now.toString() + "\n", StandardCharsets.UTF_8,
                    StandardOpenOption.CREATE, StandardOpenOption.TRUNCATE_EXISTING, StandardOpenOption.WRITE);
        } catch (Exception e) {
            System.err.println("청소 시각을 못 남겼다: " + lastSweepFile + " (" + e.getMessage() + ")");
            return;
        }

        try {
            long quietHours = getQuietHours();
            Path runDir = Paths.get(userHome, "Library", "Application Support", "withlog", "run");
            List<Target> targets = findTargets(runDir, currentVaultKey, quietHours);
            if (targets.isEmpty()) {
                return;
            }

            String uid;
            try {
                uid = Launchctl.getUid();
            } catch (Exception e) {
                return;
            }

            System.err.println(quietHours + "시간 넘게 조용한 사이드카 " + targets.size() + "개를 내린다.");
            for (Target t : targets) {
                Launchctl.Result r = Launchctl.runLaunchctl(List.of("bootout", "gui/" + uid + "/" + t.label));
                if (r.error() != null) {
                    System.err.println("  내렸다: " + t.label + " (bootout 실패: " + r.error() + ")");
                } else {
                    System.err.println("  내렸다: " + t.label + " (bootout rc=" + r.exitCode() + ")");
                }

                if (!t.isAlive) {
                    try {
                        Files.deleteIfExists(t.file);
                        System.err.println("    죽은 흔적을 지웠다: " + t.file.getFileName());
                    } catch (Exception ignored) {}
                }
            }
        } catch (Exception e) {
            System.err.println("하루 한 번 청소가 실패했다: " + e.getMessage());
        }
    }

    private static void recordLastSweep(String userHome) {
        Path lastSweepFile = Paths.get(userHome, "Library", "Caches", "withlog", "last-sweep");
        try {
            if (lastSweepFile.getParent() != null) {
                Files.createDirectories(lastSweepFile.getParent());
            }
            Files.writeString(lastSweepFile, Instant.now().toString() + "\n", StandardCharsets.UTF_8,
                    StandardOpenOption.CREATE, StandardOpenOption.TRUNCATE_EXISTING, StandardOpenOption.WRITE);
        } catch (Exception ignored) {}
    }

    public static long getQuietHours() {
        String env = System.getenv("WITHLOG_SWEEP_QUIET_HOURS");
        if (env == null) {
            return 24;
        }
        String trimmed = Trim.trim(env);
        if (trimmed.isEmpty()) {
            return 24;
        }
        try {
            long val = Long.parseLong(trimmed);
            if (val <= 0) return 24;
            // 설명서 3.8의 22번: 2,562,047,788,015,216 이상이면 오버플로우
            if (val >= 2562047788015216L) {
                throw new ArithmeticException("WITHLOG_SWEEP_QUIET_HOURS overflow: " + val);
            }
            return val;
        } catch (NumberFormatException e) {
            return 24;
        }
    }

    public static class Target {
        public final Path file;
        public final String vaultKey;
        public final String label;
        public final StateFile state;
        public final boolean isAlive;

        public Target(Path file, String vaultKey, String label, StateFile state, boolean isAlive) {
            this.file = file;
            this.vaultKey = vaultKey;
            this.label = label;
            this.state = state;
            this.isAlive = isAlive;
        }
    }

    public static List<Target> findTargets(Path runDir, String currentVaultKey, long quietHours) throws IOException {
        if (!Files.isDirectory(runDir)) {
            return Collections.emptyList();
        }

        List<Path> jsonFiles = new ArrayList<>();
        try (DirectoryStream<Path> stream = Files.newDirectoryStream(runDir, "*.json")) {
            for (Path p : stream) {
                jsonFiles.add(p);
            }
        }
        jsonFiles.sort(Comparator.comparing(p -> p.getFileName().toString()));

        List<Target> targets = new ArrayList<>();
        Instant now = Instant.now();

        for (Path file : jsonFiles) {
            String fname = file.getFileName().toString();
            String vKey = fname.substring(0, fname.length() - ".json".length());
            if (currentVaultKey != null && currentVaultKey.equals(vKey)) {
                continue; // 지금 vault 제외
            }

            StateFile state = StateFile.read(file);
            if (state == null) continue;

            int openConns = state.getOpenConnections() != null ? state.getOpenConnections() : 0;
            if (openConns != 0) continue;

            Instant lastActive = state.getLastActiveAt();
            if (lastActive == null) continue;

            long hours = Duration.between(lastActive, now).toHours();
            if (hours < quietHours) continue;

            boolean isAlive = false;
            if (state.getPid() != null) {
                isAlive = ProcessHandle.of(state.getPid()).map(ProcessHandle::isAlive).orElse(false);
            }

            targets.add(new Target(file, vKey, "co.kcd.withlog.sidecar." + vKey, state, isAlive));
        }

        return targets;
    }
}
