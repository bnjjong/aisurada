package aisurada.cli;

import aisurada.cmd.CommandRegistry;
import aisurada.frame.FrameException;
import aisurada.frame.RequestFrame;
import aisurada.frame.ResponseFrame;
import aisurada.serve.StateFile;
import aisurada.session.SessionAgents;
import aisurada.text.Trim;
import aisurada.vault.VaultKey;
import aisurada.vault.VaultPath;
import aisurada.vault.VaultPointer;

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
 * aisurada CLI 클라이언트 진입점 (설명서 3.3).
 */
public final class AisCliMain {

    /**
     * 같은 JVM에서 도구를 여러 번 부르는 mcp 모드가 쓰는 종료 대용입니다.
     * 켜져 있으면 프로세스를 끝내지 않고 종료 코드만 실어 던집니다. 일반 실행은 그대로 {@link System#exit}입니다.
     */
    static final class CliExit extends RuntimeException {
        final int code;

        CliExit(int code) {
            super("exit " + code, null, false, false);
            this.code = code;
        }
    }

    /** 한 번의 도구 호출 결과입니다. 종료 코드는 단발 실행의 종료 코드와 같습니다. */
    record Call(int status, String stdout, String stderr) {}

    private static volatile boolean embedded;

    private static void exit(int code) {
        if (embedded) {
            throw new CliExit(code);
        }
        System.exit(code);
    }

    /** mcp 모드가 시작할 때 한 번 켭니다. 이 뒤로는 종료 호출이 프로세스를 끝내지 않고 {@link CliExit}을 던집니다. */
    static void enableEmbedded() {
        embedded = true;
    }

    /**
     * 단발 실행과 같은 길(소켓 찾기, 엔진 띄우기, 요청 왕복)로 도구나 명령 하나를 부르고, 프로세스를 끝내지 않고 결과를 돌려줍니다.
     * 이 길이 표준 출력과 표준 오류에 쓰는 글은 모두 모아서 돌려주므로 mcp 모드의 프로토콜 출력과 섞이지 않습니다.
     * 한 번에 하나씩만 부릅니다.
     */
    static synchronized Call callEmbedded(String[] args) {
        java.io.PrintStream oldOut = System.out;
        java.io.PrintStream oldErr = System.err;
        java.io.ByteArrayOutputStream out = new java.io.ByteArrayOutputStream();
        java.io.ByteArrayOutputStream err = new java.io.ByteArrayOutputStream();
        int status = 0;
        boolean wasEmbedded = embedded;
        embedded = true;
        try {
            System.setOut(new java.io.PrintStream(out, true, StandardCharsets.UTF_8));
            System.setErr(new java.io.PrintStream(err, true, StandardCharsets.UTF_8));
            runEnginePipeline(args);
        } catch (CliExit e) {
            status = e.code;
        } catch (RuntimeException e) {
            System.err.println("ais-cli 안에서 예외가 났다: " + e);
            status = 1;
        } finally {
            System.out.flush();
            System.err.flush();
            System.setOut(oldOut);
            System.setErr(oldErr);
            embedded = wasEmbedded;
        }
        return new Call(status, out.toString(StandardCharsets.UTF_8), err.toString(StandardCharsets.UTF_8));
    }

    public static final List<String> CLI_TOOL_NAMES = List.of(
            "create_doc", "update_doc", "set_favorite", "evaluate_doc", "link_docs",
            "list_docs", "search_docs", "list_tags", "get_doc", "delete_doc",
            "list_trash", "restore_doc", "purge_doc", "plan_day",
            "list_templates", "get_template", "save_template",
            "create_sub_note", "list_sub_notes", "get_sub_note", "update_sub_note", "delete_sub_note",
            "write_attachment", "list_attachments", "list_comments", "export_markdown",
            "list_resources", "get_resource", "save_resource", "delete_resource", "restore_resource",
            "set_resource_ref", "usable_resources", "list_resource_versions",
            "list_resource_changes", "get_resource_change",
            "list_tasks", "get_task"
    );

    /** --list의 도구 설명. 장마다 첫머리 요약에서 지은 1줄이고 계약이 아니다(3.3). */
    private static final Map<String, String> TOOL_DESCRIPTIONS = Map.ofEntries(
            Map.entry("create_doc", "새 노트 1건을 만든다"),
            Map.entry("update_doc", "노트의 제목, 본문, 태그, 날짜, 시각을 고치고 자리를 다시 정한다"),
            Map.entry("set_favorite", "즐겨찾기 별과 라벨을 고친다"),
            Map.entry("evaluate_doc", "평가 점수와 메모를 새기거나 지운다"),
            Map.entry("link_docs", "다른 노트로의 연결 1건을 잇거나 끊는다"),
            Map.entry("list_docs", "조건에 맞는 노트 목록을 만든 시각 차례로 낸다"),
            Map.entry("search_docs", "제목, 메타, 본문, 첨부 글에서 찾아 대목과 함께 낸다"),
            Map.entry("list_tags", "태그와 쓰인 횟수를 낸다"),
            Map.entry("get_doc", "노트 1건을 본문, 도장, 코멘트와 함께 낸다"),
            Map.entry("delete_doc", "노트를 휴지통으로 보낸다"),
            Map.entry("list_trash", "휴지통 목록을 낸다"),
            Map.entry("restore_doc", "휴지통의 노트를 되살린다"),
            Map.entry("purge_doc", "휴지통의 노트를 영구히 지운다"),
            Map.entry("plan_day", "하루 계획 초안을 짓고, 원하면 노트로 저장한다"),
            Map.entry("list_templates", "문서와 리추얼 템플릿 목록을 낸다"),
            Map.entry("get_template", "템플릿 1건을 낸다"),
            Map.entry("save_template", "문서 템플릿을 저장한다"),
            Map.entry("create_sub_note", "하위 노트 1건을 만든다"),
            Map.entry("list_sub_notes", "바로 아래 하위 노트 목록을 낸다"),
            Map.entry("get_sub_note", "하위 노트 1건을 낸다"),
            Map.entry("update_sub_note", "하위 노트의 제목과 본문을 고친다"),
            Map.entry("delete_sub_note", "하위 노트를 지운다"),
            Map.entry("write_attachment", "노트나 하위 노트에 글 첨부 1개를 쓴다"),
            Map.entry("list_attachments", "첨부 목록과 본문에 붙일 참조 조각을 낸다"),
            Map.entry("list_comments", "코멘트 목록을 낸다"),
            Map.entry("export_markdown", "노트를 마크다운 사본으로 내보낸다"),
            Map.entry("list_resources", "스킬과 에이전트 정의의 관리 목록을 낸다"),
            Map.entry("get_resource", "스킬이나 에이전트 정의 1건을 지침과 함께 낸다"),
            Map.entry("save_resource", "스킬이나 에이전트 정의를 만들거나 고친다"),
            Map.entry("delete_resource", "스킬이나 에이전트 정의를 휴지통으로 보낸다"),
            Map.entry("restore_resource", "휴지통의 스킬이나 에이전트 정의를 되살린다"),
            Map.entry("set_resource_ref", "프로젝트가 글로벌 자원을 참조하는 것을 켜거나 끈다"),
            Map.entry("usable_resources", "프로젝트가 쓸 수 있는 스킬과 에이전트 정의를 낸다"),
            Map.entry("list_resource_versions", "스킬이나 에이전트 정의의 고치기 전 판 목록을 낸다"),
            Map.entry("list_resource_changes", "승인을 기다리는 스킬·에이전트 변경과 승인 안 된 직접 수정을 낸다"),
            Map.entry("get_resource_change", "승인 대기 변경 1건을 이전 판과 새 판과 함께 낸다"),
            Map.entry("list_tasks", "태스크(티켓) 목록을 상태, 유형, 저장소, 라벨로 거른다"),
            Map.entry("get_task", "태스크 1건을 설명, 완료 기준, 연결, 활동과 함께 낸다")
    );

    public static final List<String> DIRECT_COMMANDS = List.of(
            "doctor", "sweep", "schedule", "install", "mcp", "claude-mcp", "codex-mcp", "gemini-mcp"
    );

    public static final List<String> ENGINE_COMMANDS_61 = List.of(
            "reindex", "reindex-paths", "list-docs", "search-docs", "get-doc",
            "list-sub-notes", "get-sub-note", "list-attachments", "list-comments", "list-tags",
            "list-trash", "list-templates", "get-template",
            "list-agendas", "list-projects", "create-project", "update-project", "delete-project",
            "list-resources", "get-resource", "save-resource", "delete-resource", "restore-resource",
            "set-resource-ref", "usable-resources", "list-resource-versions",
            "save-template", "set-favorite", "evaluate-doc", "link-docs",
            "write-attachment", "delete-doc", "restore-doc", "purge-doc", "create-sub-note",
            "update-sub-note", "delete-sub-note", "export-markdown", "create-doc", "update-doc",
            "empty-trash", "create-agenda", "update-agenda", "delete-agenda", "save-ritual-template",
            "delete-doc-template", "init-workspace", "create-comment", "update-comment", "delete-comment",
            "shard-notes", "verify-shard", "plan-day", "check-notes", "history",
            "commit-vault", "get-agenda-facts", "get-ask", "append-ask-turn", "list-chats",
            "append-chat-turn", "update-agenda-facts", "facts-brief", "read-session", "session-titles",
            "recent-sessions"
    );

    /**
     * 태스크 명령 9개. 엔진({@code CommandRegistry})에는 있지만 이 목록에 빠지면 {@code ais-cli}가 「알 수 없는 이름」으로 거절해
     * 앱의 태스크 화면과 러너가 모두 못 쓴다. 엔진 이름과 어긋나지 않는지는 {@code TasksTest}가 대조한다.
     */
    public static final List<String> ENGINE_COMMANDS_TASKS = List.of(
            "create-task", "get-task", "list-tasks", "update-task", "transition-task",
            "link-tasks", "add-task-comment", "delete-task", "restore-task"
    );

    /**
     * 승인 워크플로 명령 7개(#53). 사람이 앱에서 승인·거절하고 정책을 바꾸는 길이라서 도구 이름(MCP)으로는 열지 않는다.
     * 이 목록에 빠지면 앱의 승인 화면이 「알 수 없는 이름」으로 거절된다. 엔진 이름과의 대조는 {@code TasksTest} 0장이 한다.
     */
    public static final List<String> ENGINE_COMMANDS_APPROVALS = List.of(
            "list-resource-changes", "get-resource-change", "approve-resource-change", "reject-resource-change",
            "resolve-resource-edit", "get-resource-policy", "set-resource-policy"
    );

    public static void main(String[] args) {
        if (args.length == 0 || "--help".equals(args[0]) || "-h".equals(args[0])) {
            printUsageAndList();
            exit(0);
            return;
        }

        if ("--list".equals(args[0]) || "-l".equals(args[0])) {
            printListOnly();
            exit(0);
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
        } else if ("mcp".equals(firstArg)) {
            McpServer.run(Arrays.copyOfRange(args, 1, args.length));
            return;
        } else if ("claude-mcp".equals(firstArg)) {
            ClaudeMcpCommand.run(Arrays.copyOfRange(args, 1, args.length));
            return;
        } else if ("codex-mcp".equals(firstArg)) {
            AgentMcpCommand.run(AgentMcpCommand.Kind.CODEX, Arrays.copyOfRange(args, 1, args.length));
            return;
        } else if ("gemini-mcp".equals(firstArg)) {
            AgentMcpCommand.run(AgentMcpCommand.Kind.GEMINI, Arrays.copyOfRange(args, 1, args.length));
            return;
        } else if ("schedule".equals(firstArg)) {
            // 결정 3: 모르는 이름으로 간주하여 종료 코드 2
            printUnknownName(firstArg);
            exit(2);
            return;
        }

        // 도구 이름 26개 또는 사이드카 고유 명령 58개
        boolean isTool = CLI_TOOL_NAMES.contains(firstArg);
        boolean isCmd = ENGINE_COMMANDS_61.contains(firstArg) || ENGINE_COMMANDS_TASKS.contains(firstArg) || ENGINE_COMMANDS_APPROVALS.contains(firstArg);

        if (!isTool && !isCmd) {
            printUnknownName(firstArg);
            exit(2);
            return;
        }

        // 엔진 갈래
        runEnginePipeline(args);
    }

    private static void printUsageAndList() {
        System.out.println("사용법: ais-cli <도구 또는 명령> [인자...]");
        System.out.println("  도구는 JSON 한 덩이를 받습니다: ais-cli get_doc '{\"id\":\"...\"}'");
        System.out.println("  명령은 이름과 값을 받습니다: ais-cli list-docs limit=0");
        System.out.println();
        printListOnly();
    }

    private static void printListOnly() {
        for (String tool : CLI_TOOL_NAMES) {
            System.out.println(tool + " — " + TOOL_DESCRIPTIONS.get(tool));
        }
        System.out.println();
        System.out.println("doctor — 사이드카와 관련 파일의 상태를 진단합니다");
        System.out.println("sweep — 오래 조용한 사이드카를 정리합니다");
        System.out.println("schedule — 주기적 작업을 다룹니다");
        System.out.println("install — 실행에 필요한 jar 파일을 설치합니다");
        System.out.println("mcp — Claude 같은 MCP 클라이언트가 노트를 읽도록 MCP 서버로 선다(표준 입출력). 쓰기 도구는 --write를 줄 때만 연다");
        System.out.println("claude-mcp — Claude Code에 이 MCP 서버를 등록한다(status, add [--write], remove)");
        System.out.println("codex-mcp — Codex에 이 MCP 서버를 등록한다(status, add [--write], remove)");
        System.out.println("gemini-mcp — Antigravity(agy)나 Gemini CLI에 이 MCP 서버를 등록한다(status, add [--write], remove)");
        System.out.println();
        System.out.println("사이드카 고유 명령: " + String.join(", ", ENGINE_COMMANDS_61) + ", " + String.join(", ", ENGINE_COMMANDS_TASKS) + ", " + String.join(", ", ENGINE_COMMANDS_APPROVALS));
        System.out.println("  앱이 쓰는 배선입니다. 이름이 하이픈인 것이 표시이고, 도구 이름(밑줄)과 다른 표면입니다.");
    }

    /** 3.3: uid는 launchctl을 부를 때마다 다시 읽는다. 못 읽으면 그 글 1줄과 종료 코드 4다. */
    private static String uidOrExit() {
        try {
            return Launchctl.getUid();
        } catch (Launchctl.LaunchctlException e) {
            System.err.println(e.getMessage());
            exit(4);
            return null;
        }
    }

    /** 3.3: launchctl을 못 돌렸거나 10초를 넘기면 그 글 1줄과 종료 코드 4다. 돌았으면 종료 코드는 부르는 쪽이 본다. */
    private static Launchctl.Result launchctlOrExit(List<String> args) {
        Launchctl.Result r = Launchctl.runLaunchctl(args);
        if (r.error() != null) {
            System.err.println(r.error());
            exit(4);
        }
        return r;
    }

    private static void printUnknownName(String name) {
        System.err.println("알 수 없는 이름: " + name);
        System.err.println("도구: " + String.join(", ", CLI_TOOL_NAMES));
        System.err.println("명령: " + String.join(", ", DIRECT_COMMANDS));
        System.err.println("사이드카 고유 명령: " + String.join(", ", ENGINE_COMMANDS_61) + ", " + String.join(", ", ENGINE_COMMANDS_TASKS) + ", " + String.join(", ", ENGINE_COMMANDS_APPROVALS));
        System.err.println("전체 설명은 ais-cli --list 로 본다.");
    }

    private static void runEnginePipeline(String[] args) {
        // 1. vault 정하기: AISURADA_VAULT가 있으면 그것, 없으면 앱이 열어 둔 vault. 호출마다 다시 읽어서 앱을 따라간다
        String userHome = System.getProperty("user.home");
        String envVault = VaultPointer.resolve(System.getenv("AISURADA_VAULT"), userHome);
        if (envVault == null || Trim.isBlank(envVault)) {
            System.err.println("AIS-400 " + VaultPointer.MISSING_MESSAGE);
            exit(2);
            return;
        }

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
        String label = "co.kcd.aisurada.sidecar." + vaultKey;

        // 2. 소켓 경로 결정 및 길이 검사
        aisurada.serve.SidecarLocation location = aisurada.serve.SidecarLocation.resolve(normalizedVault);
        Path socketPath = location.socketPath();
        if (location.isSocketPathTooLong()) {
            System.err.println("AIS-400 소켓 경로가 102 바이트를 넘는다(" + location.socketPathBytes().length + " 바이트): " + socketPath + ". AISURADA_SIDECAR_SOCKET 로 더 짧은 자리를 지정한다.");
            exit(2);
            return;
        }

        // 3. 생존 확인 연결 시도
        boolean connected = false;
        if (Files.exists(socketPath, LinkOption.NOFOLLOW_LINKS)) {
            try (SocketChannel ch = SocketChannel.open(UnixDomainSocketAddress.of(socketPath))) {
                connected = true;
            } catch (IOException ignored) {}
        }

        // 3.3: uid는 launchctl을 부를 때마다 다시 읽고, launchctl을 안 부르는 호출은 uid를 안 읽는다
        if (connected) {
            // 4. 다시 띄우기 판정
            checkRestart(userHome, vaultKey, label, socketPath);
        } else {
            // 5. 엔진 띄우기
            launchEngine(userHome, vaultKey, label, normalizedVault, socketPath);
        }

        // 6. 소켓 연결 및 요청 전송
        if (!Files.exists(socketPath, LinkOption.NOFOLLOW_LINKS)) {
            System.err.println("사이드카가 안 떠 있다. 소켓 파일이 없다: " + socketPath);
            exit(4);
            return;
        }

        SocketChannel clientChannel;
        try {
            clientChannel = SocketChannel.open(UnixDomainSocketAddress.of(socketPath));
        } catch (IOException e) {
            System.err.println("소켓 파일은 있는데 못 붙는다: " + socketPath + ". 사이드카가 죽었거나 아직 뜨는 중이다. 이유=" + e.getMessage());
            exit(4);
            return;
        }

        // 요청 프레임 구성
        List<String> reqArgs = new ArrayList<>(Arrays.asList(args));
        // 세션 출처: MCP 요청이 실어 온 값, 래퍼가 넣은 AISURADA_SESSION_*, 에이전트 고유 변수 차례로 정한다.
        // claude가 아닌 에이전트일 때만 에이전트 표식 2칸을 더해서, Claude 호출의 요청은 전과 같다
        SessionAgents.Source sessionSource = SessionAgents.callOverride();
        if (sessionSource == null) sessionSource = SessionAgents.fromEnvironment(System.getenv());
        if (sessionSource != null) {
            reqArgs.add("\u0000ais-session-id");
            reqArgs.add(sessionSource.sessionId());
            if (SessionAgents.forFile(sessionSource.agent()) != null) {
                reqArgs.add("\u0000ais-session-agent");
                reqArgs.add(sessionSource.agent());
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
            exit(4);
            return;
        } catch (IOException e) {
            e.printStackTrace(System.err);
            exit(1);
            return;
        } finally {
            try { clientChannel.close(); } catch (IOException ignored) {}
        }

        // 9. 하루 한 번 청소 확인
        SweepCommand.runDailySweepIfDue(vaultKey);

        // 10. 종료 코드 반환 (아래 8비트)
        exit(header.status() % 256);
    }

    private static void checkRestart(String userHome, String vaultKey, String label, Path socketPath) {
        Path stateFilePath = Paths.get(userHome, "Library", "Application Support", "aisurada", "run", vaultKey + ".json");
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

        Launchctl.Result r = launchctlOrExit(List.of("kickstart", "-k", "gui/" + uidOrExit() + "/" + label));

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
            waitForSocketReady(socketPath, label, timeoutSec);
        } else if (r.exitCode() == 113) {
            System.err.println("사이드카 재시작 대상이 없다 — 그 라벨이 launchd 에 없다(113): " + label);
            System.err.println("낡은 jar 로 계속 돈다. ais-cli install 로 그 라벨을 다시 등록해야 한다.");
            long timeoutSec = getReadyTimeoutSeconds();
            waitForSocketReady(socketPath, label, timeoutSec);
        } else {
            System.err.println("사이드카 재시작이 실패했다. launchctl kickstart -k 가 " + r.exitCode() + " 를 줬다: " + label);
            if (!r.output().trim().isEmpty()) {
                System.err.println(r.output().trim());
            }
            exit(4);
        }
    }

    /**
     * plist를 만들어 디스크와 다르면 덮어쓰고 이전 등록을 내린다. jar를 못 정하거나 plist를 못 쓰면 종료 코드를 정하고 {@code false}를 준다.
     */
    private static boolean writeRegistration(String userHome, String label, String normalizedVault, Path socketPath, Path plistFile, Path logPath) {
        // 3.4: AISURADA_SIDECAR_JAR는 상대 경로든 없는 파일이든 확인 없이 싣는다. 못 정했을 때만 선다
        String sidecarJar = PlistGenerator.resolveSidecarJar(userHome);
        if (sidecarJar == null) {
            Path installed = Paths.get(userHome, "Library", "Application Support", "aisurada", "bin", "ais-sidecar.jar");
            System.err.println("AIS-400 AISURADA_SIDECAR_JAR 가 없고 설치된 ais-sidecar.jar 도 없다(" + installed + "). `install` 을 돌리거나 jar 의 절대 경로를 AISURADA_SIDECAR_JAR 에 넣는다. 이 값이 plist 에 그대로 실리므로 상대 경로를 쓰면 launchd 가 못 찾는다.");
            exit(2);
            return false;
        }

        String sidecarJava = PlistGenerator.resolveSidecarJava();
        String plistContent = PlistGenerator.generatePlist(label, sidecarJava, sidecarJar, normalizedVault, socketPath.toString(), logPath.toString());

        boolean plistDiff = true;
        if (Files.exists(plistFile)) {
            // 3.3 띄우기 2번: 파일은 있는데 못 읽으면(UTF-8이 아님 포함) 덮지 않고 예외로 죽어 종료 코드 1이다
            try {
                String existing = Files.readString(plistFile, StandardCharsets.UTF_8);
                if (plistContent.equals(existing)) {
                    plistDiff = false;
                }
            } catch (Exception e) {
                e.printStackTrace(System.err);
                exit(1);
                return false;
            }
        }

        if (plistDiff) {
            try {
                if (plistFile.getParent() != null) {
                    Files.createDirectories(plistFile.getParent());
                }
                Files.writeString(plistFile, plistContent, StandardCharsets.UTF_8);
            } catch (Exception e) {
                e.printStackTrace(System.err);
                exit(1);
                return false;
            }
            // bootout의 종료 코드는 안 본다. 못 돌렸거나 10초를 넘기면 종료 코드 4다
            launchctlOrExit(List.of("bootout", "gui/" + uidOrExit() + "/" + label));
        }
        return true;
    }

    private static void launchEngine(String userHome, String vaultKey, String label, String normalizedVault, Path socketPath) {
        Path logPath = Paths.get(userHome, "Library", "Application Support", "aisurada", "logs", "sidecar-" + vaultKey + ".log");
        Path plistFile = Paths.get(userHome, "Library", "LaunchAgents", label + ".plist");

        // 빌드 폴더나 개발 실행의 앱은 이미 있는 등록을 덮어쓰지 않는다(PlistGenerator.keepsExistingRegistration)
        if (PlistGenerator.keepsExistingRegistration(System.getenv(PlistGenerator.KEEP_REGISTRATION_ENV), plistFile)) {
            System.err.println("이미 있는 엔진 등록을 그대로 쓴다(" + PlistGenerator.KEEP_REGISTRATION_ENV + "): " + plistFile);
        } else if (!writeRegistration(userHome, label, normalizedVault, socketPath, plistFile, logPath)) {
            return;
        }

        // 동시 상한 3 지키기
        enforceConcurrencyLimit(userHome, vaultKey);

        // kickstart
        Launchctl.Result kr = launchctlOrExit(List.of("kickstart", "gui/" + uidOrExit() + "/" + label));

        if (kr.exitCode() == 113) {
            launchctlOrExit(List.of("bootstrap", "gui/" + uidOrExit(), plistFile.toString()));
            kr = launchctlOrExit(List.of("kickstart", "gui/" + uidOrExit() + "/" + label));
        }

        if (kr.exitCode() == 113) {
            Launchctl.Result printDis = launchctlOrExit(List.of("print-disabled", "gui/" + uidOrExit()));
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
                launchctlOrExit(List.of("enable", "gui/" + uidOrExit() + "/" + label));
                launchctlOrExit(List.of("bootstrap", "gui/" + uidOrExit(), plistFile.toString()));
                kr = launchctlOrExit(List.of("kickstart", "gui/" + uidOrExit() + "/" + label));
            }
        }

        if (kr.exitCode() != 0) {
            System.err.println("사이드카를 못 띄웠다. launchctl kickstart 가 " + kr.exitCode() + " 를 줬다: " + label);
            System.err.println("plist=" + plistFile);
            System.err.println("로그=" + logPath);
            System.err.println(kr.output().trim());
            exit(4);
            return;
        }

        long timeoutSec = getReadyTimeoutSeconds();
        waitForSocketReady(socketPath, label, timeoutSec);
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

                targets.add(new SweepCommand.Target(p, vKey, "co.kcd.aisurada.sidecar." + vKey, sf, true));
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

    private static void enforceConcurrencyLimit(String userHome, String currentVaultKey) {
        Path runDir = Paths.get(userHome, "Library", "Application Support", "aisurada", "run");
        int limit = getConcurrencyLimit();
        List<SweepCommand.Target> quietTargets;
        try {
            quietTargets = findConcurrencyEvictionTargets(runDir, currentVaultKey, limit);
        } catch (IOException e) {
            e.printStackTrace(System.err);
            exit(1);
            return;
        }
        for (SweepCommand.Target t : quietTargets) {
            Launchctl.Result br;
            try {
                br = Launchctl.runLaunchctl(List.of("bootout", "gui/" + Launchctl.getUid() + "/" + t.label));
            } catch (Launchctl.LaunchctlException e) {
                br = new Launchctl.Result(-1, "", false, e.getMessage());
            }
            String ago = TimeFormat.formatAgo(t.state.getLastActiveAt());
            if (br.error() != null) {
                System.err.println("사이드카 동시 상한 " + limit + "개를 지키려고 가장 오래 조용했던 것을 내린다: " + t.label + " (마지막 활동 " + ago + ", bootout 실패: " + br.error() + ")");
            } else {
                System.err.println("사이드카 동시 상한 " + limit + "개를 지키려고 가장 오래 조용했던 것을 내린다: " + t.label + " (마지막 활동 " + ago + ", bootout rc=" + br.exitCode() + ")");
            }
        }
    }

    private static void waitForSocketReady(Path socketPath, String label, long timeoutSec) {
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
        Launchctl.Result pr = launchctlOrExit(List.of("print", "gui/" + uidOrExit() + "/" + label));

        System.err.println("사이드카가 " + timeoutSec + " 초 안에 준비되지 않았다: " + socketPath);
        System.err.println("AISURADA_READY_TIMEOUT_SECONDS 로 더 기다리게 할 수 있다.");
        String printOut = pr.output().trim();
        if (printOut.length() > 1200) {
            printOut = printOut.substring(0, 1200);
        }
        System.err.println(printOut);
        exit(4);
    }

    private static long getReadyTimeoutSeconds() {
        String env = System.getenv("AISURADA_READY_TIMEOUT_SECONDS");
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
        String env = System.getenv("AISURADA_SIDECAR_LIMIT");
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
