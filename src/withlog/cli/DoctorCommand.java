package withlog.cli;

import withlog.serve.StateFile;
import withlog.text.Trim;
import withlog.vault.VaultKey;
import withlog.vault.VaultPath;

import java.io.File;
import java.io.IOException;
import java.net.UnixDomainSocketAddress;
import java.nio.channels.SocketChannel;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.time.Duration;
import java.time.Instant;
import java.util.*;

/**
 * wl-cli doctor [--plist] [--prune [--dry-run]] 명령 처리. (설명서 3.6)
 */
public final class DoctorCommand {

    private DoctorCommand() {}

    public static void run(String[] args) {
        boolean showPlist = false;
        boolean prune = false;
        boolean dryRun = false;

        for (String a : args) {
            if ("--plist".equals(a)) showPlist = true;
            else if ("--prune".equals(a)) prune = true;
            else if ("--dry-run".equals(a)) dryRun = true;
        }

        // 1. WITHLOG_VAULT 확인
        String envVault = System.getenv("WITHLOG_VAULT");
        if (envVault == null || Trim.isBlank(envVault)) {
            System.err.println("WL-400 WITHLOG_VAULT 환경변수가 없다. vault 폴더의 절대 경로를 넣고 다시 실행한다.");
            System.exit(2);
            return;
        }

        String userHome = System.getProperty("user.home");
        String userDir = System.getProperty("user.dir", ".");

        // vault 글자 정리 (//와 끝 / 정리, 절대 경로로는 안 바꾼다)
        String trimmedVault = Trim.trim(envVault);
        String cleanedVault = cleanVaultDisplay(trimmedVault);
        String normalizedVault;
        try {
            normalizedVault = VaultPath.normalize(trimmedVault);
        } catch (Exception e) {
            normalizedVault = trimmedVault;
        }

        String vaultKey = VaultKey.compute(normalizedVault);
        String hash16 = VaultKey.computeHash16(normalizedVault);
        String label = "co.kcd.withlog.sidecar." + vaultKey;

        // 2. 소켓 경로 결정 및 길이 검사
        String rawSocket = System.getenv("WITHLOG_SIDECAR_SOCKET");
        if (rawSocket == null || Trim.isBlank(rawSocket)) {
            rawSocket = userHome + "/Library/Caches/withlog/wl-" + hash16 + ".sock";
        } else {
            rawSocket = Trim.trim(rawSocket);
        }
        Path socketPath = Paths.get(VaultPath.normalize(rawSocket));
        byte[] socketBytes = socketPath.toString().getBytes(StandardCharsets.UTF_8);
        if (socketBytes.length > 103) {
            System.err.println("WL-400 소켓 경로가 103 바이트를 넘는다(" + socketBytes.length + " 바이트): " + socketPath + ". WITHLOG_SIDECAR_SOCKET 로 더 짧은 자리를 지정한다.");
            System.exit(2);
            return;
        }

        // 3. uid 읽기 (launchctl 호출 및 진단용)
        String uid;
        try {
            uid = Launchctl.getUid();
        } catch (Launchctl.LaunchctlException e) {
            System.err.println(e.getMessage());
            System.exit(4);
            return;
        }

        Path plistPath = Paths.get(userHome, "Library", "LaunchAgents", label + ".plist");
        Path stateFilePath = Paths.get(userHome, "Library", "Application Support", "withlog", "run", vaultKey + ".json");
        Path lockFilePath = Paths.get(userHome, "Library", "Application Support", "withlog", "run", vaultKey + ".lock");
        Path logFilePath = Paths.get(userHome, "Library", "Application Support", "withlog", "logs", "sidecar-" + vaultKey + ".log");

        // 출력 시작
        System.out.println("vault      " + cleanedVault);
        System.out.println("키         " + vaultKey);
        System.out.println("라벨       " + label);

        // 소켓 상태 진단
        String socketStatus;
        if (!Files.exists(socketPath, LinkOption.NOFOLLOW_LINKS)) {
            socketStatus = "파일 없음. 안 떠 있다";
        } else {
            try (SocketChannel ch = SocketChannel.open(UnixDomainSocketAddress.of(socketPath))) {
                socketStatus = "붙는다. 살아 있다";
            } catch (IOException e) {
                socketStatus = "파일은 있는데 안 붙는다. 죽었거나 아직 뜨는 중이다 (" + e.getMessage() + ")";
            }
        }
        System.out.println("소켓       " + socketPath + "  (" + socketStatus + ")");
        System.out.println("plist      " + plistPath + "  (" + (Files.exists(plistPath) ? "있음" : "없음") + ")");

        // 실행 상태 진단
        boolean hasStateFile = Files.exists(stateFilePath);
        System.out.println("실행 상태  " + stateFilePath + "  (" + (hasStateFile ? "있음" : "없음") + ")");
        StateFile state = null;
        if (hasStateFile) {
            state = StateFile.read(stateFilePath);
            String pidStr = state.getPid() != null ? String.valueOf(state.getPid()) : "(모름)";
            boolean pidAlive = false;
            if (state.getPid() != null) {
                pidAlive = ProcessHandle.of(state.getPid()).map(ProcessHandle::isAlive).orElse(false);
            }
            String pidAliveStr = pidAlive ? "그 pid 가 돈다" : "그 pid 로 도는 것이 없다";
            String lastActiveStr = TimeFormat.formatAgo(state.getLastActiveAt());
            String openConnsStr = state.getOpenConnections() != null ? (state.getOpenConnections() + "개") : "(모름)";
            String startedStr = TimeFormat.formatAgo(state.getStartedAt());

            System.out.println("           pid=" + pidStr + " (" + pidAliveStr + ")  마지막 활동 " + lastActiveStr + "  열린 연결 " + openConnsStr + "  뜬 것은 " + startedStr);
        }

        System.out.println("잠금       " + lockFilePath + "  (" + (Files.exists(lockFilePath) ? "있음" : "없음") + ")");
        System.out.println("로그       " + logFilePath + "  (" + (Files.exists(logFilePath) ? "있음" : "없음") + ")");

        // vault git 진단
        printVaultGit(normalizedVault);

        // 사이드카 jar 진단
        String sidecarJar = PlistGenerator.resolveSidecarJar(userHome);
        String sidecarJarSource;
        if (System.getenv("WITHLOG_SIDECAR_JAR") != null && !Trim.isBlank(System.getenv("WITHLOG_SIDECAR_JAR"))) {
            sidecarJarSource = "WITHLOG_SIDECAR_JAR 로 못박음";
        } else if (sidecarJar != null) {
            sidecarJarSource = "설치본";
        } else {
            sidecarJarSource = "(안 잡힘. `install` 을 돌리거나 WITHLOG_SIDECAR_JAR 에 절대 경로를 넣는다)";
        }
        if (sidecarJar != null && !Files.exists(Paths.get(sidecarJar))) {
            sidecarJarSource += ", ★파일이 없다";
        }
        if (sidecarJar != null) {
            System.out.println("사이드카 jar  " + sidecarJar + "  (" + sidecarJarSource + ")");
        } else {
            System.out.println("사이드카 jar  " + sidecarJarSource);
        }

        // wl-cli jar 진단
        String cliJar = resolveCliJar(userHome);
        String cliJarSource;
        if (System.getenv("WITHLOG_CLI_JAR") != null && !Trim.isBlank(System.getenv("WITHLOG_CLI_JAR"))) {
            cliJarSource = "WITHLOG_CLI_JAR 로 못박음";
        } else if (cliJar != null) {
            cliJarSource = "설치본";
        } else {
            cliJarSource = "(안 잡힘. `install` 을 돌리거나 WITHLOG_CLI_JAR 에 절대 경로를 넣는다)";
        }
        if (cliJar != null && !Files.exists(Paths.get(cliJar))) {
            cliJarSource += ", ★파일이 없다";
        }
        if (cliJar != null) {
            System.out.println("wl-cli jar   " + cliJar + "  (" + cliJarSource + ")");
        } else {
            System.out.println("wl-cli jar   " + cliJarSource);
        }

        // 사이드카 자바
        String sidecarJava = PlistGenerator.resolveSidecarJava();
        System.out.println("사이드카 자바 " + sidecarJava);

        // plist 사이드카
        String plistJar = PlistGenerator.parseJarFromPlist(plistPath);
        if (plistJar != null) {
            System.out.println("plist 사이드카 " + plistJar);
            if (!Files.exists(Paths.get(plistJar))) {
                System.out.println("  ⚠ 그 파일이 지금 없다. 다음에 뜰 때 실패한다");
            }
        }

        // 커밋 4줄
        printCommitLines(state, plistJar, userDir, userHome);

        // 동시 상한 및 하루 청소
        printConcurrencyAndSweep(userHome, vaultKey, label);

        // 이 기기의 다른 흔적
        printOtherTraces(userHome, vaultKey);

        // 설치된 LaunchAgent 목록
        printInstalledLaunchAgents(userHome, vaultKey);

        // --plist 옵션 처리
        if (showPlist) {
            if (sidecarJar == null || !Files.exists(Paths.get(sidecarJar))) {
                System.err.println("WL-400 WITHLOG_SIDECAR_JAR 가 없고 설치된 wl-sidecar.jar 도 없다(" +
                        Paths.get(userHome, "Library", "Application Support", "withlog", "bin", "wl-sidecar.jar") +
                        "). `install` 을 돌리거나 jar 의 절대 경로를 WITHLOG_SIDECAR_JAR 에 넣는다. 이 값이 plist 에 그대로 실리므로 상대 경로를 쓰면 launchd 가 못 찾는다.");
                System.exit(2);
                return;
            }
            String plistContent = PlistGenerator.generatePlist(label, sidecarJava, sidecarJar, normalizedVault, socketPath.toString(), logFilePath.toString());
            System.out.println();
            System.out.println("이 값으로 plist 를 쓴다 (아직 안 썼다)");
            System.out.print(plistContent);
            System.out.println();
        }

        // --prune 옵션 처리
        if (prune) {
            runPrune(userHome, uid, vaultKey, dryRun);
        }

        System.exit(0);
    }

    private static String cleanVaultDisplay(String raw) {
        String s = raw.replaceAll("/+", "/");
        if (s.length() > 1 && s.endsWith("/")) {
            s = s.substring(0, s.length() - 1);
        }
        return s;
    }

    private static String resolveCliJar(String userHome) {
        String env = System.getenv("WITHLOG_CLI_JAR");
        if (env != null && !Trim.isBlank(env)) return Trim.trim(env);
        Path p = Paths.get(userHome, "Library", "Application Support", "withlog", "bin", "wl-cli.jar");
        if (Files.isRegularFile(p)) {
            return p.toAbsolutePath().normalize().toString();
        }
        return null;
    }

    private static void printVaultGit(String normalizedVault) {
        Path gitDir = Paths.get(normalizedVault, ".git");
        if (!Files.isDirectory(gitDir)) {
            System.out.println("vault git  git 저장소가 아니다. 자동 커밋이 안 돈다");
            return;
        }

        // git 명령어 실행 (5초 타임아웃)
        ProcessResult logRes = runGitCommand(normalizedVault, List.of("log", "-1", "--author=withlog-auto", "--format=%ct %h %s"));
        ProcessResult statusRes = runGitCommand(normalizedVault, List.of("status", "--porcelain", "--untracked-files=all"));

        String commitInfo = "자동 커밋이 아직 없다";
        if (logRes.exitCode == 0 && !logRes.output.isBlank()) {
            String[] parts = logRes.output.strip().split(" ", 3);
            if (parts.length >= 2) {
                try {
                    long epochSec = Long.parseLong(parts[0]);
                    Instant commitTime = Instant.ofEpochSecond(epochSec);
                    String ago = TimeFormat.formatAgo(commitTime);
                    String shortHash = parts[1];
                    String subject = parts.length >= 3 ? parts[2] : "";
                    commitInfo = "마지막 자동 커밋 " + ago + " (" + shortHash + " " + subject + ")";
                } catch (Exception ignored) {}
            }
        }

        String untrackedInfo = "커밋 밖 (못 쟀다)";
        if (statusRes.exitCode == 0) {
            String[] lines = statusRes.output.split("\n");
            int count = 0;
            for (String l : lines) {
                if (!l.isBlank() && !l.endsWith(".tmp")) {
                    count++;
                }
            }
            untrackedInfo = "커밋 밖 " + count + "개";
        }

        System.out.println("vault git  " + commitInfo + "  " + untrackedInfo);

        // 경고 줄 확인
        Path indexLock = gitDir.resolve("index.lock");
        if (Files.exists(indexLock)) {
            Instant mtime = Instant.EPOCH;
            try { mtime = Files.getLastModifiedTime(indexLock).toInstant(); } catch (Exception ignored) {}
            System.out.println("⚠ .git/index.lock 이 있다(" + TimeFormat.formatAgo(mtime) + "에 생겼다). 자동 커밋과 되돌리기가 이것에 막힌다. 오래된 잠금은 사이드카가 다음 커밋 때 지운다");
        } else if (Files.exists(gitDir.resolve("rebase-merge")) || Files.exists(gitDir.resolve("rebase-apply"))
                || Files.exists(gitDir.resolve("MERGE_HEAD")) || Files.exists(gitDir.resolve("CHERRY_PICK_HEAD"))
                || Files.exists(gitDir.resolve("REVERT_HEAD"))) {
            System.out.println("⚠ vault 에서 병합이나 리베이스가 진행 중이다. 끝날 때까지 자동 커밋을 건너뛴다");
        }
    }

    private static void printCommitLines(StateFile state, String plistJar, String userDir, String userHome) {
        // 주체 4개
        System.out.println("이 사이드카 커밋   " + (state != null && state.getBuiltCommit() != null ? state.getBuiltCommit().substring(0, Math.min(7, state.getBuiltCommit().length())) : "(못 쟀다 — 빌드 커밋 정보가 없다)"));
        System.out.println("도는 wl-cli 커밋   (못 쟀다 — 빌드 커밋 정보가 없다)");
        System.out.println("설치본 wl-cli 커밋   (못 쟀다 — 빌드 커밋 정보가 없다)");
        System.out.println("설치본 사이드카 커밋   (못 쟀다 — 빌드 커밋 정보가 없다)");
    }

    private static void printConcurrencyAndSweep(String userHome, String vaultKey, String label) {
        Path runDir = Paths.get(userHome, "Library", "Application Support", "withlog", "run");
        int limit = WlCliMain.getConcurrencyLimit();
        int aliveCount = 0;
        int traceCount = 0;
        if (Files.isDirectory(runDir)) {
            try (DirectoryStream<Path> s = Files.newDirectoryStream(runDir, "*.json")) {
                for (Path p : s) {
                    traceCount++;
                    StateFile sf = StateFile.read(p);
                    if (sf != null && sf.getPid() != null && ProcessHandle.of(sf.getPid()).map(ProcessHandle::isAlive).orElse(false)) {
                        aliveCount++;
                    }
                }
            } catch (Exception ignored) {}
        }
        System.out.println("동시 상한   " + limit + "개 중 살아 있는 것 " + aliveCount + "개 (흔적 파일 " + traceCount + "개)");

        List<SweepCommand.Target> toEvict;
        try {
            toEvict = WlCliMain.findConcurrencyEvictionTargets(runDir, vaultKey, limit);
        } catch (Exception e) {
            toEvict = Collections.emptyList();
        }
        if (!toEvict.isEmpty()) {
            System.out.println("           이 vault 를 새로 띄우면 " + toEvict.size() + "개를 내린다:");
            for (SweepCommand.Target t : toEvict) {
                String ago = TimeFormat.formatAgo(t.state.getLastActiveAt());
                System.out.println("             " + t.label + "  (마지막 활동 " + ago + ")");
            }
        }

        Path lastSweep = Paths.get(userHome, "Library", "Caches", "withlog", "last-sweep");
        if (Files.exists(lastSweep)) {
            try {
                Instant t = Instant.parse(Trim.trim(Files.readString(lastSweep, StandardCharsets.UTF_8)));
                String ago = TimeFormat.formatAgo(t);
                boolean due = Duration.between(t, Instant.now()).toHours() >= 24;
                System.out.println("하루 청소   마지막 " + ago + "  (" + (due ? "다음 읽기 호출에서 돈다" : "아직 아니다") + ")");
            } catch (Exception ignored) {
                System.out.println("하루 청소   마지막 (모름)  (다음 읽기 호출에서 돈다)");
            }
        } else {
            System.out.println("하루 청소   마지막 (모름)  (다음 읽기 호출에서 돈다)");
        }
    }

    private static void printOtherTraces(String userHome, String vaultKey) {
        Path runDir = Paths.get(userHome, "Library", "Application Support", "withlog", "run");
        if (!Files.isDirectory(runDir)) return;
        List<Path> others = new ArrayList<>();
        try (DirectoryStream<Path> s = Files.newDirectoryStream(runDir)) {
            for (Path p : s) {
                String fn = p.getFileName().toString();
                if (!fn.startsWith(vaultKey + ".")) {
                    others.add(p);
                }
            }
        } catch (Exception ignored) {}

        if (!others.isEmpty()) {
            others.sort(Comparator.comparing(p -> p.getFileName().toString()));
            System.out.println();
            System.out.println("이 기기의 다른 흔적");
            for (Path p : others) {
                String fn = p.getFileName().toString();
                System.out.println("  " + fn);
                if (fn.endsWith(".json")) {
                    StateFile otherState = StateFile.read(p);
                    if (otherState != null) {
                        String oVault = otherState.getVault() != null ? otherState.getVault() : "(못 읽음)";
                        System.out.println("    vault=" + oVault);
                        String oPidStr = otherState.getPid() != null ? String.valueOf(otherState.getPid()) : "(모름)";
                        boolean oPidAlive = false;
                        if (otherState.getPid() != null) {
                            oPidAlive = ProcessHandle.of(otherState.getPid()).map(ProcessHandle::isAlive).orElse(false);
                        }
                        String oPidAliveStr = oPidAlive ? "그 pid 가 돈다" : "그 pid 로 도는 것이 없다";
                        String oLastActiveStr = TimeFormat.formatAgo(otherState.getLastActiveAt());
                        String oOpenConnsStr = otherState.getOpenConnections() != null ? (otherState.getOpenConnections() + "개") : "(모름)";
                        String oStartedStr = TimeFormat.formatAgo(otherState.getStartedAt());
                        System.out.println("    pid=" + oPidStr + " (" + oPidAliveStr + ")  마지막 활동 " + oLastActiveStr + "  열린 연결 " + oOpenConnsStr + "  뜬 것은 " + oStartedStr);
                    }
                }
            }
            System.out.println("  json 은 도는 사이드카를 뜻하고 lock 만 남은 것은 정상이다. 잠금 파일은 안 지운다.");
        }
    }

    private static void printInstalledLaunchAgents(String userHome, String vaultKey) {
        Path agentsDir = Paths.get(userHome, "Library", "LaunchAgents");
        List<Path> plists = new ArrayList<>();
        if (Files.isDirectory(agentsDir)) {
            try (DirectoryStream<Path> s = Files.newDirectoryStream(agentsDir, "co.kcd.withlog.sidecar.*.plist")) {
                for (Path p : s) plists.add(p);
            } catch (Exception ignored) {}
        }
        plists.sort(Comparator.comparing(Path::toString));

        System.out.println();
        System.out.println("설치된 사이드카 LaunchAgent " + plists.size() + "개 (" + agentsDir + ")");
        if (plists.isEmpty()) {
            System.out.println("  (없음. 아직 아무 vault 도 사이드카를 안 띄웠다)");
            return;
        }

        for (Path p : plists) {
            String fname = p.getFileName().toString();
            String l = fname.substring(0, fname.length() - ".plist".length());
            String v = PlistGenerator.parseVaultFromPlist(p);
            String desc;
            if (v == null) {
                desc = "plist 에서 vault 를 못 읽었다";
            } else if (l.endsWith("-" + vaultKey) || l.equals("co.kcd.withlog.sidecar." + vaultKey)) {
                desc = "지금 이 vault";
            } else if (!Files.isDirectory(Paths.get(v))) {
                desc = "vault 폴더가 없다. --prune 대상";
            } else {
                desc = "쓴다";
            }
            System.out.println("  " + l);
            System.out.println("    vault=" + (v != null ? v : "(못 읽음)") + "  (" + desc + ")");
        }
    }

    private static void runPrune(String userHome, String uid, String currentVaultKey, boolean dryRun) {
        Path agentsDir = Paths.get(userHome, "Library", "LaunchAgents");
        List<Path> plists = new ArrayList<>();
        if (Files.isDirectory(agentsDir)) {
            try (DirectoryStream<Path> s = Files.newDirectoryStream(agentsDir, "co.kcd.withlog.sidecar.*.plist")) {
                for (Path p : s) plists.add(p);
            } catch (Exception ignored) {}
        }

        List<TargetPrune> pruneList = new ArrayList<>();
        for (Path p : plists) {
            String fname = p.getFileName().toString();
            String l = fname.substring(0, fname.length() - ".plist".length());
            if (l.equals("co.kcd.withlog.sidecar." + currentVaultKey)) {
                continue; // 지금 vault 제외
            }
            String v = PlistGenerator.parseVaultFromPlist(p);
            if (v != null && !Files.isDirectory(Paths.get(v))) {
                pruneList.add(new TargetPrune(p, l, v));
            }
        }

        System.out.println();
        if (pruneList.isEmpty()) {
            System.out.println("정리할 것이 없다. vault 폴더가 사라진 에이전트가 0개다.");
            return;
        }

        if (dryRun) {
            System.out.println("정리 대상 " + pruneList.size() + "개 (아직 안 지웠다)");
            for (TargetPrune t : pruneList) {
                System.out.println("  " + t.label + "  vault=" + t.vault);
            }
            System.out.println("  진짜 지우려면 --dry-run 를 뺀다.");
            System.out.println("  잠금 파일(.lock)은 안 지운다. 크기 0 이라 해가 없고, 지우면 잠금이 갈린다.");
            return;
        }

        System.out.println("정리 " + pruneList.size() + "개");
        for (TargetPrune t : pruneList) {
            System.out.println("  " + t.label + "  vault=" + t.vault);
            Launchctl.Result r = Launchctl.runLaunchctl(List.of("bootout", "gui/" + uid + "/" + t.label));
            if (r.error() != null) {
                System.err.println(r.error());
                System.exit(4);
                return;
            }
            System.out.println("    bootout rc=" + r.exitCode());
            try {
                Files.deleteIfExists(t.plistFile);
                System.out.println("    plist 지웠다: " + t.plistFile);
            } catch (Exception e) {
                System.out.println("    plist 못 지웠다: " + t.plistFile);
            }
        }
        System.out.println("  잠금 파일(.lock)은 안 지운다. 크기 0 이라 해가 없고, 지우면 잠금이 갈린다.");
    }

    private static class TargetPrune {
        final Path plistFile;
        final String label;
        final String vault;

        TargetPrune(Path plistFile, String label, String vault) {
            this.plistFile = plistFile;
            this.label = label;
            this.vault = vault;
        }
    }

    private static class ProcessResult {
        final int exitCode;
        final String output;
        ProcessResult(int exitCode, String output) {
            this.exitCode = exitCode;
            this.output = output;
        }
    }

    private static ProcessResult runGitCommand(String vaultRoot, List<String> subArgs) {
        List<String> cmd = new ArrayList<>();
        cmd.add("git");
        cmd.add("--no-optional-locks");
        cmd.addAll(subArgs);
        ProcessBuilder pb = new ProcessBuilder(cmd);
        pb.directory(new File(vaultRoot));
        pb.redirectErrorStream(true);
        try {
            Process p = pb.start();
            boolean finished = p.waitFor(5, java.util.concurrent.TimeUnit.SECONDS);
            if (!finished) {
                p.destroyForcibly();
                return new ProcessResult(-1, "");
            }
            String out = new String(p.getInputStream().readAllBytes(), StandardCharsets.UTF_8);
            return new ProcessResult(p.exitValue(), out);
        } catch (Exception e) {
            return new ProcessResult(-1, "");
        }
    }
}
