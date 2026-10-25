package withlog.cli;

import withlog.cmd.CommandRegistry;
import withlog.frame.FrameException;
import withlog.frame.RequestFrame;
import withlog.frame.ResponseFrame;
import withlog.serve.StateFile;
import withlog.text.Trim;
import withlog.vault.VaultKey;
import withlog.vault.VaultPath;

import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.StandardProtocolFamily;
import java.net.UnixDomainSocketAddress;
import java.nio.channels.Channels;
import java.nio.channels.SocketChannel;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.nio.file.attribute.FileTime;
import java.time.Instant;
import java.util.*;

/**
 * withlog CLI 클라이언트 진입점 (설명서 3.3).
 */
public final class WlCliMain {

    public static final List<String> CLI_TOOL_NAMES = List.of(
            "create_doc", "update_doc", "set_favorite", "evaluate_doc", "link_docs",
            "list_docs", "search_docs", "list_tags", "get_doc", "delete_doc",
            "list_trash", "restore_doc", "purge_doc", "plan_day", "list_reports",
            "get_report", "list_templates", "get_template", "save_report", "save_template",
            "create_sub_note", "list_sub_notes", "get_sub_note", "update_sub_note", "delete_sub_note",
            "write_attachment", "list_attachments", "list_comments", "export_markdown"
    );

    public static final List<String> DIRECT_COMMANDS = List.of(
            "doctor", "sweep", "report", "schedule", "install"
    );

    public static final List<String> ENGINE_COMMANDS_61 = List.of(
            "reindex", "reindex-paths", "list-docs", "search-docs", "get-doc",
            "list-sub-notes", "get-sub-note", "list-attachments", "list-comments", "list-tags",
            "list-trash", "list-reports", "get-report", "list-templates", "get-template",
            "list-agendas", "list-projects", "create-project", "update-project", "delete-project",
            "save-report", "save-template", "set-favorite", "evaluate-doc", "link-docs",
            "write-attachment", "delete-doc", "restore-doc", "purge-doc", "create-sub-note",
            "update-sub-note", "delete-sub-note", "export-markdown", "create-doc", "update-doc",
            "empty-trash", "create-agenda", "update-agenda", "delete-agenda", "save-ritual-template",
            "delete-doc-template", "init-workspace", "create-comment", "update-comment", "delete-comment",
            "shard-notes", "verify-shard", "plan-day", "check-notes", "history",
            "commit-vault", "get-agenda-facts", "get-ask", "append-ask-turn", "list-chats",
            "append-chat-turn", "update-agenda-facts", "facts-brief", "read-session", "session-titles",
            "recent-sessions"
    );

    public static void main(String[] args) {
        if (args.length == 0 || "--help".equals(args[0]) || "-h".equals(args[0])) {
            printUsageAndList();
            System.exit(0);
            return;
        }

        if ("--list".equals(args[0]) || "-l".equals(args[0])) {
            printListOnly();
            System.exit(0);
            return;
        }

        String firstArg = args[0];

        // 직접 하는 명령 분기
        if ("doctor".equals(firstArg)) {
            DoctorCommand.run(Arrays.copyOfRange(args, 1, args.length));
            return;
        } else if ("sweep".equals(firstArg)) {
            SweepCommand.run(Arrays.copyOfRange(args, 1, args.length));
            return;
        } else if ("install".equals(firstArg)) {
            InstallCommand.run(Arrays.copyOfRange(args, 1, args.length));
            return;
        } else if ("report".equals(firstArg) || "schedule".equals(firstArg)) {
            // 결정 3: 모르는 이름으로 간주하여 종료 코드 2
            printUnknownName(firstArg);
            System.exit(2);
            return;
        }

        // 도구 이름 29개 또는 사이드카 고유 명령 61개
        boolean isTool = CLI_TOOL_NAMES.contains(firstArg);
        boolean isCmd = ENGINE_COMMANDS_61.contains(firstArg);

        if (!isTool && !isCmd) {
            printUnknownName(firstArg);
            System.exit(2);
            return;
        }

        // 엔진 갈래
        runEnginePipeline(args);
    }

    private static void printUsageAndList() {
        System.out.println("사용법: wl-cli <도구 또는 명령> [인자...]");
        System.out.println("  도구는 JSON 한 덩이를 받습니다: wl-cli get_doc '{\"id\":\"...\"}'");
        System.out.println("  명령은 이름과 값을 받습니다: wl-cli list-docs limit=0");
        System.out.println();
        printListOnly();
    }

    private static void printListOnly() {
        for (String tool : CLI_TOOL_NAMES) {
            System.out.println(tool + " — " + tool + " 도구");
        }
        System.out.println();
        System.out.println("doctor — 사이드카와 관련 파일의 상태를 진단합니다");
        System.out.println("sweep — 오래 조용한 사이드카를 정리합니다");
        System.out.println("report — 리포트 작업을 다룹니다");
        System.out.println("schedule — 주기적 작업을 다룹니다");
        System.out.println("install — 실행에 필요한 jar 파일을 설치합니다");
        System.out.println();
        System.out.println("사이드카 고유 명령: " + String.join(", ", ENGINE_COMMANDS_61));
        System.out.println("  앱이 쓰는 배선입니다. 이름이 하이픈인 것이 표시이고, 도구 이름(밑줄)과 다른 표면입니다.");
    }

    private static void printUnknownName(String name) {
        System.err.println("알 수 없는 이름: " + name);
        System.err.println("도구: " + String.join(", ", CLI_TOOL_NAMES));
        System.err.println("명령: " + String.join(", ", DIRECT_COMMANDS));
        System.err.println("사이드카 고유 명령: " + String.join(", ", ENGINE_COMMANDS_61));
        System.err.println("전체 설명은 wl-cli --list 로 본다.");
    }

    private static void runEnginePipeline(String[] args) {
        // 1. WITHLOG_VAULT 확인
        String envVault = System.getenv("WITHLOG_VAULT");
        if (envVault == null || Trim.isBlank(envVault)) {
            System.err.println("WL-400 WITHLOG_VAULT 환경변수가 없다. vault 폴더의 절대 경로를 넣고 다시 실행한다.");
            System.exit(2);
            return;
        }

        String userHome = System.getProperty("user.home");
        String userDir = System.getProperty("user.dir", ".");
        String trimmedVault = Trim.trim(envVault);
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
        byte[] socketPathBytes = socketPath.toString().getBytes(StandardCharsets.UTF_8);
        if (socketPathBytes.length > 103) {
            System.err.println("WL-400 소켓 경로가 103 바이트를 넘는다(" + socketPathBytes.length + " 바이트): " + socketPath + ". WITHLOG_SIDECAR_SOCKET 로 더 짧은 자리를 지정한다.");
            System.exit(2);
            return;
        }

        // 3. 생존 확인 연결 시도
        boolean connected = false;
        if (Files.exists(socketPath, LinkOption.NOFOLLOW_LINKS)) {
            try (SocketChannel ch = SocketChannel.open(UnixDomainSocketAddress.of(socketPath))) {
                connected = true;
            } catch (IOException ignored) {}
        }

        String uid = null;

        if (connected) {
            // 4. 다시 띄우기 판정
            try {
                uid = Launchctl.getUid();
            } catch (Launchctl.LaunchctlException e) {
                System.err.println(e.getMessage());
                System.exit(4);
                return;
            }
            checkRestart(userHome, uid, vaultKey, label, socketPath);
        } else {
            // 5. 엔진 띄우기
            try {
                uid = Launchctl.getUid();
            } catch (Launchctl.LaunchctlException e) {
                System.err.println(e.getMessage());
                System.exit(4);
                return;
            }
            launchEngine(userHome, uid, vaultKey, label, normalizedVault, socketPath);
        }

        // 6. 소켓 연결 및 요청 전송
        if (!Files.exists(socketPath, LinkOption.NOFOLLOW_LINKS)) {
            System.err.println("사이드카가 안 떠 있다. 소켓 파일이 없다: " + socketPath);
            System.exit(4);
            return;
        }

        SocketChannel clientChannel;
        try {
            clientChannel = SocketChannel.open(UnixDomainSocketAddress.of(socketPath));
        } catch (IOException e) {
            System.err.println("소켓 파일은 있는데 못 붙는다: " + socketPath + ". 사이드카가 죽었거나 아직 뜨는 중이다. 이유=" + e.getMessage());
            System.exit(4);
            return;
        }

        // 요청 프레임 구성
        List<String> reqArgs = new ArrayList<>(Arrays.asList(args));
        String rawSid = System.getenv("CLAUDE_CODE_SESSION_ID");
        if (rawSid != null) {
            String sid = Trim.trim(rawSid);
            if (!sid.isEmpty()) {
                reqArgs.add("\u0000wl-session-id");
                reqArgs.add(sid);
            }
        }

        // 64칸 초과 및 1 MiB 초과 검사
        if (reqArgs.size() > 64) {
            throw new IllegalArgumentException("인자 개수가 64개를 초과했습니다: " + reqArgs.size());
        }
        for (String a : reqArgs) {
            if (a.getBytes(StandardCharsets.UTF_8).length > 1_048_576) {
                throw new IllegalArgumentException("인자 길이가 1 MiB를 초과했습니다");
            }
        }

        ResponseFrame.HeaderInfo header;
        try {
            OutputStream out = Channels.newOutputStream(clientChannel);
            RequestFrame reqFrame = new RequestFrame(reqArgs);
            reqFrame.write(out);
            clientChannel.shutdownOutput();

            // 8. 응답 머리 및 스트리밍 읽기
            InputStream in = Channels.newInputStream(clientChannel);
            header = ResponseFrame.streamRead(in, System.out, System.err);
        } catch (FrameException fe) {
            System.err.println("사이드카와 말이 안 통한다: " + fe.getMessage());
            System.exit(4);
            return;
        } catch (IOException e) {
            e.printStackTrace(System.err);
            System.exit(1);
            return;
        } finally {
            try { clientChannel.close(); } catch (IOException ignored) {}
        }

        // 9. 하루 한 번 청소 확인
        SweepCommand.runDailySweepIfDue(vaultKey);

        // 10. 종료 코드 반환 (아래 8비트)
        System.exit(header.status() % 256);
    }

    private static void checkRestart(String userHome, String uid, String vaultKey, String label, Path socketPath) {
        Path stateFilePath = Paths.get(userHome, "Library", "Application Support", "withlog", "run", vaultKey + ".json");
        Path plistPath = Paths.get(userHome, "Library", "LaunchAgents", label + ".plist");

        if (!Files.exists(stateFilePath) || !Files.exists(plistPath)) {
            return;
        }

        StateFile state = StateFile.read(stateFilePath);
        if (state == null || state.getStartedAt() == null || state.getOpenConnections() == null || state.getOpenConnections() != 0) {
            return;
        }

        String jarPath = PlistGenerator.parseJarFromPlist(plistPath);
        if (jarPath == null) {
            return;
        }

        Path jarFile = Paths.get(jarPath);
        if (!Files.exists(jarFile)) {
            return;
        }

        Instant jarMtime;
        try {
            jarMtime = Files.getLastModifiedTime(jarFile).toInstant();
        } catch (IOException e) {
            return;
        }

        if (!jarMtime.isAfter(state.getStartedAt())) {
            return;
        }

        System.err.println("사이드카를 다시 띄운다 — jar 가 뜬 뒤에 바뀌었다: " + jarPath);
        Long oldPid = state.getPid();

        Launchctl.Result r = Launchctl.runLaunchctl(List.of("kickstart", "-k", "gui/" + uid + "/" + label));
        if (r.error() != null) {
            System.err.println(r.error());
            System.exit(4);
            return;
        }

        if (r.exitCode() == 0 || r.exitCode() == 37) {
            // pid 변경 대기 (100ms 간격, 30초 대기)
            long timeoutSec = getReadyTimeoutSeconds();
            long startWait = System.currentTimeMillis();
            if (oldPid != null) {
                while ((System.currentTimeMillis() - startWait) < timeoutSec * 1000L) {
                    StateFile cur = StateFile.read(stateFilePath);
                    if (cur != null && cur.getPid() != null && !cur.getPid().equals(oldPid)) {
                        break;
                    }
                    try { Thread.sleep(100); } catch (InterruptedException ignored) { break; }
                }
            }
            waitForSocketReady(socketPath, uid, label, timeoutSec);
        } else if (r.exitCode() == 113) {
            System.err.println("사이드카 재시작 대상이 없다 — 그 라벨이 launchd 에 없다(113): " + label);
            System.err.println("낡은 jar 로 계속 돈다. wl-cli install 로 그 라벨을 다시 등록해야 한다.");
            long timeoutSec = getReadyTimeoutSeconds();
            waitForSocketReady(socketPath, uid, label, timeoutSec);
        } else {
            System.err.println("사이드카 재시작이 실패했다. launchctl kickstart -k 가 " + r.exitCode() + " 를 줬다: " + label);
            if (!r.output().trim().isEmpty()) {
                System.err.println(r.output().trim());
            }
            System.exit(4);
        }
    }

    private static void launchEngine(String userHome, String uid, String vaultKey, String label, String normalizedVault, Path socketPath) {
        String sidecarJar = PlistGenerator.resolveSidecarJar(userHome);
        if (sidecarJar == null || !Files.exists(Paths.get(sidecarJar))) {
            Path installed = Paths.get(userHome, "Library", "Application Support", "withlog", "bin", "wl-sidecar.jar");
            System.err.println("WL-400 WITHLOG_SIDECAR_JAR 가 없고 설치된 wl-sidecar.jar 도 없다(" + installed + "). `install` 을 돌리거나 jar 의 절대 경로를 WITHLOG_SIDECAR_JAR 에 넣는다. 이 값이 plist 에 그대로 실리므로 상대 경로를 쓰면 launchd 가 못 찾는다.");
            System.exit(2);
            return;
        }

        String sidecarJava = PlistGenerator.resolveSidecarJava();
        Path logPath = Paths.get(userHome, "Library", "Application Support", "withlog", "logs", "sidecar-" + vaultKey + ".log");
        String plistContent = PlistGenerator.generatePlist(label, sidecarJava, sidecarJar, normalizedVault, socketPath.toString(), logPath.toString());

        Path plistFile = Paths.get(userHome, "Library", "LaunchAgents", label + ".plist");
        boolean plistDiff = true;
        if (Files.exists(plistFile)) {
            try {
                String existing = Files.readString(plistFile, StandardCharsets.UTF_8);
                if (plistContent.equals(existing)) {
                    plistDiff = false;
                }
            } catch (Exception ignored) {}
        }

        if (plistDiff) {
            try {
                if (plistFile.getParent() != null) {
                    Files.createDirectories(plistFile.getParent());
                }
                Files.writeString(plistFile, plistContent, StandardCharsets.UTF_8);
                Launchctl.runLaunchctl(List.of("bootout", "gui/" + uid + "/" + label));
            } catch (Exception e) {
                e.printStackTrace(System.err);
                System.exit(1);
                return;
            }
        }

        // 동시 상한 3 지키기
        enforceConcurrencyLimit(userHome, uid, vaultKey);

        // kickstart
        Launchctl.Result kr = Launchctl.runLaunchctl(List.of("kickstart", "gui/" + uid + "/" + label));
        if (kr.error() != null) {
            System.err.println(kr.error());
            System.exit(4);
            return;
        }

        if (kr.exitCode() == 113) {
            Launchctl.runLaunchctl(List.of("bootstrap", "gui/" + uid, plistFile.toString()));
            kr = Launchctl.runLaunchctl(List.of("kickstart", "gui/" + uid + "/" + label));
            if (kr.error() != null) {
                System.err.println(kr.error());
                System.exit(4);
                return;
            }
        }

        if (kr.exitCode() == 113) {
            Launchctl.Result printDis = Launchctl.runLaunchctl(List.of("print-disabled", "gui/" + uid));
            boolean isDisabled = false;
            for (String l : printDis.output().split("\n")) {
                if (l.contains(label)) {
                    int arrow = l.indexOf("=>");
                    if (arrow >= 0 && l.substring(arrow + 2).strip().startsWith("true")) {
                        isDisabled = true;
                        break;
                    }
                }
            }
            if (isDisabled) {
                Launchctl.runLaunchctl(List.of("enable", "gui/" + uid + "/" + label));
                Launchctl.runLaunchctl(List.of("bootstrap", "gui/" + uid, plistFile.toString()));
                kr = Launchctl.runLaunchctl(List.of("kickstart", "gui/" + uid + "/" + label));
                if (kr.error() != null) {
                    System.err.println(kr.error());
                    System.exit(4);
                    return;
                }
            }
        }

        if (kr.exitCode() != 0) {
            System.err.println("사이드카를 못 띄웠다. launchctl kickstart 가 " + kr.exitCode() + " 를 줬다: " + label);
            System.err.println("plist=" + plistFile);
            System.err.println("로그=" + logPath);
            System.err.println(kr.output().trim());
            System.exit(4);
            return;
        }

        long timeoutSec = getReadyTimeoutSeconds();
        waitForSocketReady(socketPath, uid, label, timeoutSec);
    }

    /**
     * 3.5 「동시 상한 3」 1~6번에 따라 새로 띄우기 위해 내려야 할 사이드카 목록을 계산합니다.
     */
    public static List<SweepCommand.Target> findConcurrencyEvictionTargets(Path runDir, String currentVaultKey, int limit) throws IOException {
        if (runDir == null || !Files.exists(runDir)) return Collections.emptyList();

        List<SweepCommand.Target> targets = new ArrayList<>();
        try (DirectoryStream<Path> s = Files.newDirectoryStream(runDir, "*.json")) {
            for (Path p : s) {
                String fname = p.getFileName().toString();
                String vKey = fname.substring(0, fname.length() - ".json".length());
                if (currentVaultKey != null && vKey.equals(currentVaultKey)) continue;

                StateFile sf = StateFile.read(p);
                if (sf == null || sf.getPid() == null) continue;
                boolean alive = ProcessHandle.of(sf.getPid()).map(ProcessHandle::isAlive).orElse(false);
                if (!alive) continue;

                targets.add(new SweepCommand.Target(p, vKey, "co.kcd.withlog.sidecar." + vKey, sf, true));
            }
        } catch (DirectoryIteratorException die) {
            throw die.getCause();
        }

        targets.sort(Comparator.comparing(t -> t.file.getFileName().toString()));

        int aliveCount = targets.size();
        int needToStop = aliveCount - (limit - 1);
        if (needToStop <= 0) return Collections.emptyList();

        // openConnections가 0인 것만 필터링 (5번)
        List<SweepCommand.Target> quietTargets = new ArrayList<>();
        for (SweepCommand.Target t : targets) {
            int openConns = t.state.getOpenConnections() != null ? t.state.getOpenConnections() : 0;
            if (openConns == 0) {
                quietTargets.add(t);
            }
        }

        // lastActiveAt 오래된 순 정렬 (null은 맨 뒤, 같으면 이름 순) (6번)
        quietTargets.sort((a, b) -> {
            Instant ia = a.state.getLastActiveAt();
            Instant ib = b.state.getLastActiveAt();
            if (ia == null && ib == null) return a.file.getFileName().compareTo(b.file.getFileName());
            if (ia == null) return 1;
            if (ib == null) return -1;
            int cmp = ia.compareTo(ib);
            if (cmp != 0) return cmp;
            return a.file.getFileName().compareTo(b.file.getFileName());
        });

        int stopCount = Math.min(needToStop, quietTargets.size());
        return new ArrayList<>(quietTargets.subList(0, stopCount));
    }

    private static void enforceConcurrencyLimit(String userHome, String uid, String currentVaultKey) {
        Path runDir = Paths.get(userHome, "Library", "Application Support", "withlog", "run");
        int limit = getConcurrencyLimit();
        List<SweepCommand.Target> quietTargets;
        try {
            quietTargets = findConcurrencyEvictionTargets(runDir, currentVaultKey, limit);
        } catch (IOException e) {
            e.printStackTrace(System.err);
            System.exit(1);
            return;
        }
        for (SweepCommand.Target t : quietTargets) {
            Launchctl.Result br = Launchctl.runLaunchctl(List.of("bootout", "gui/" + uid + "/" + t.label));
            String ago = TimeFormat.formatAgo(t.state.getLastActiveAt());
            if (br.error() != null) {
                System.err.println("사이드카 동시 상한 " + limit + "개를 지키려고 가장 오래 조용했던 것을 내린다: " + t.label + " (마지막 활동 " + ago + ", bootout 실패: " + br.error() + ")");
            } else {
                System.err.println("사이드카 동시 상한 " + limit + "개를 지키려고 가장 오래 조용했던 것을 내린다: " + t.label + " (마지막 활동 " + ago + ", bootout rc=" + br.exitCode() + ")");
            }
        }
    }

    private static void waitForSocketReady(Path socketPath, String uid, String label, long timeoutSec) {
        long startWait = System.currentTimeMillis();
        boolean warned10s = false;

        while ((System.currentTimeMillis() - startWait) < timeoutSec * 1000L) {
            if (Files.exists(socketPath, LinkOption.NOFOLLOW_LINKS)) {
                try (SocketChannel ch = SocketChannel.open(UnixDomainSocketAddress.of(socketPath))) {
                    return; // 준비 완료!
                } catch (IOException ignored) {}
            }

            if (!warned10s && (System.currentTimeMillis() - startWait) >= 10_000L) {
                System.err.println("사이드카가 아직 뜨는 중이다. " + timeoutSec + "초까지 기다린다: " + socketPath);
                warned10s = true;
            }

            try {
                Thread.sleep(100);
            } catch (InterruptedException ignored) {
                break;
            }
        }

        // 상한 초과 시 print 호출 후 종료 코드 4
        Launchctl.Result pr = Launchctl.runLaunchctl(List.of("print", "gui/" + uid + "/" + label));
        if (pr.error() != null) {
            System.err.println(pr.error());
            System.exit(4);
            return;
        }

        System.err.println("사이드카가 " + timeoutSec + " 초 안에 준비되지 않았다: " + socketPath);
        System.err.println("WITHLOG_READY_TIMEOUT_SECONDS 로 더 기다리게 할 수 있다.");
        String printOut = pr.output().trim();
        if (printOut.length() > 1200) {
            printOut = printOut.substring(0, 1200);
        }
        System.err.println(printOut);
        System.exit(4);
    }

    private static long getReadyTimeoutSeconds() {
        String env = System.getenv("WITHLOG_READY_TIMEOUT_SECONDS");
        if (env == null) return 30;
        String trimmed = Trim.trim(env);
        if (trimmed.isEmpty()) return 30;
        try {
            long val = Long.parseLong(trimmed);
            if (val <= 0) return 30;
            return val;
        } catch (NumberFormatException e) {
            return 30;
        }
    }

    public static int getConcurrencyLimit() {
        String env = System.getenv("WITHLOG_SIDECAR_LIMIT");
        if (env == null) return 3;
        String trimmed = Trim.trim(env);
        if (trimmed.isEmpty()) return 3;
        try {
            int val = Integer.parseInt(trimmed);
            if (val <= 0) return 3;
            return val;
        } catch (NumberFormatException e) {
            return 3;
        }
    }
}
