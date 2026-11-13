package aisurada.engine;

import aisurada.args.ArgsParser;
import aisurada.args.ParsedArgs;
import aisurada.cli.AisException;
import aisurada.cmd.*;
import aisurada.out.JsonWriter;
import aisurada.vault.NoteIndex;

import java.io.ByteArrayOutputStream;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Path;
import java.util.*;

/**
 * aisurada 엔진의 명령 해석 및 실행기.
 * 단발 모드와 serve 모드 양쪽에서 공통으로 명령을 실행하고 규격화된 EngineResult를 생성합니다.
 */
public final class EngineExecutor {

    public static final String NO_PART4_MESSAGE =
            "AIS-409 이 판에는 4부(Claude 연동)가 없다. 문답과 레일 대화와 세션 기록 명령은 ais-claude 모듈을 넣어 지은 사이드카에서만 돈다.\n";

    /** 빌드가 jar에 싣는 스위치 자원. 4부를 켠 jar는 part4=true, 4부를 뺀 jar는 part4=false다. */
    private static final String BUILD_FLAGS_RESOURCE = "/aisurada/engine-build.properties";

    /**
     * 이 엔진이 4부 명령 7개를 도는지 판정합니다.
     * jar에 실린 빌드 스위치만 따릅니다. 런타임 속성이나 환경 변수는 빌드에 없는 클래스를 켜지 못합니다.
     */
    public static boolean isPart4Enabled() {
        return builtWithPart4();
    }

    private static boolean builtWithPart4() {
        try (java.io.InputStream in = EngineExecutor.class.getResourceAsStream(BUILD_FLAGS_RESOURCE)) {
            if (in == null) return false;
            Properties flags = new Properties();
            flags.load(in);
            return "true".equalsIgnoreCase(flags.getProperty("part4", "false").trim());
        } catch (java.io.IOException e) {
            return false;
        }
    }

    private EngineExecutor() {}

    /**
     * 명령 argv를 받아 실행하고 규격화된 결과를 반환합니다.
     *
     * @param vaultRoot   검증된 vault 폴더 경로
     * @param rawArgv     명령어 및 인자 배열
     * @param isServeMode serve 모드 여부
     * @param startTime   시작 시각(밀리초)
     * @return EngineResult (status, errorCode, stdoutBytes, stderrBytes)
     */
    public static EngineResult execute(Path vaultRoot, String[] rawArgv, boolean isServeMode, long startTime) {
        List<String> argv = new ArrayList<>(Arrays.asList(rawArgv));

        // 1. 세션 표식 분리 (4.4). 확장: claude가 아닌 에이전트의 요청은 세션 표식 뒤에 에이전트 표식 2칸이 더 온다
        String sessionAgent = null;
        if (argv.size() >= 2) {
            int secondFromLast = argv.size() - 2;
            if ("\u0000ais-session-agent".equals(argv.get(secondFromLast))) {
                sessionAgent = argv.get(secondFromLast + 1);
                argv.remove(secondFromLast + 1);
                argv.remove(secondFromLast);
            }
        }
        String sessionId = null;
        if (argv.size() >= 2) {
            int secondFromLast = argv.size() - 2;
            if ("\u0000ais-session-id".equals(argv.get(secondFromLast))) {
                sessionId = argv.get(secondFromLast + 1);
                argv.remove(secondFromLast + 1);
                argv.remove(secondFromLast);
            }
        }
        // 세션 표식이 없으면 에이전트 표식도 뜻이 없다
        if (sessionId == null) sessionAgent = null;

        // 2. 명령 존재 확인
        if (argv.isEmpty()) {
            byte[] err = CommandRegistry.buildUnknownCommandMessage("(없음)").getBytes(StandardCharsets.UTF_8);
            return EngineResult.unknownCommand(err);
        }

        String firstArg = argv.get(0);

        // serve 모드에 전달된 'serve'는 모르는 명령 (3.2)
        if (isServeMode && "serve".equals(firstArg)) {
            byte[] err = CommandRegistry.buildUnknownCommandMessage("serve").getBytes(StandardCharsets.UTF_8);
            return EngineResult.unknownCommand(err);
        }

        boolean isTool = CommandRegistry.TOOL_NAMES.contains(firstArg);
        boolean isCmd = !isTool && CommandRegistry.COMMAND_NAMES.contains(firstArg);

        if (!isTool && !isCmd) {
            byte[] err = CommandRegistry.buildUnknownCommandMessage(firstArg).getBytes(StandardCharsets.UTF_8);
            return EngineResult.unknownCommand(err);
        }

        boolean isPart4 = CommandRegistry.isPart4Command(firstArg);
        if (!isPart4 && !CommandRegistry.isSupported(firstArg)) {
            byte[] err = ("AIS-409 이 판에는 " + firstArg + "이 없다\n").getBytes(StandardCharsets.UTF_8);
            return EngineResult.aisError(3, 409, err);
        }

        List<String> commandArgs = argv.subList(1, argv.size());

        Diagnostics diagnostics = new Diagnostics();
        try (diagnostics) {
            ParsedArgs parsed = ArgsParser.parse(firstArg, commandArgs);

            try (WritePipeline scope = WritePipeline.begin(vaultRoot, parsed.commandName(), isTool, isServeMode, sessionId, sessionAgent)) {
                if (isPart4 && !isPart4Enabled()) {
                    // 5. 4부가 없음 (32.2)
                    byte[] err = NO_PART4_MESSAGE.getBytes(StandardCharsets.UTF_8);
                    return EngineResult.aisError(3, 409, err);
                }

                java.util.function.Supplier<NoteIndex> lazyIndex = new java.util.function.Supplier<>() {
                    private NoteIndex cached;
                    @Override
                    public NoteIndex get() {
                        if (cached == null) {
                            cached = NoteIndex.fresh(vaultRoot);
                        }
                        return cached;
                    }
                };

                Object response;

                if ("get-ask".equals(parsed.commandName())) {
                    response = aisurada.ask.AskCommands.executeGetAsk(vaultRoot, parsed, lazyIndex.get());
                } else if ("append-ask-turn".equals(parsed.commandName())) {
                    response = aisurada.ask.AskCommands.executeAppendAskTurn(vaultRoot, parsed, lazyIndex.get());
                } else if ("read-session".equals(parsed.commandName())) {
                    response = aisurada.session.SessionCommands.executeReadSession(parsed);
                } else if ("session-titles".equals(parsed.commandName())) {
                    response = aisurada.session.SessionCommands.executeSessionTitles(parsed);
                } else if ("recent-sessions".equals(parsed.commandName())) {
                    response = aisurada.session.SessionCommands.executeRecentSessions(parsed);
                } else if ("list-chats".equals(parsed.commandName())) {
                    response = aisurada.chat.ChatCommands.executeListChats(vaultRoot, parsed, lazyIndex.get());
                } else if ("append-chat-turn".equals(parsed.commandName())) {
                    response = aisurada.chat.ChatCommands.executeAppendChatTurn(vaultRoot, parsed, lazyIndex.get());
                } else if ("reindex".equals(parsed.commandName())) {
                    response = Reindex.execute(vaultRoot, startTime);
                } else if ("history".equals(parsed.commandName())) {
                    response = History.execute(vaultRoot, parsed);
                } else if ("commit-vault".equals(parsed.commandName())) {
                    response = CommitVault.execute(vaultRoot, isServeMode);
                } else if ("reindex-paths".equals(parsed.commandName())) {
                    response = Reindex.paths(vaultRoot, parsed.positionalArgs(), startTime);
                } else if ("get-doc".equals(parsed.commandName())) {
                    NoteIndex noteIndex = lazyIndex.get();
                    response = GetDoc.execute(vaultRoot, parsed, noteIndex.agendaTable(), noteIndex.noteScan());
                } else if ("get-sub-note".equals(parsed.commandName())) {
                    response = GetSubNote.execute(vaultRoot, parsed, lazyIndex.get().noteScan());
                } else if ("list-attachments".equals(parsed.commandName())) {
                    response = ListAttachments.execute(vaultRoot, parsed, lazyIndex.get());
                } else if ("list-comments".equals(parsed.commandName())) {
                    response = ListComments.execute(vaultRoot, parsed, lazyIndex.get());
                } else if (Set.of("create-comment", "update-comment", "delete-comment").contains(parsed.commandName())) {
                    response = CommentCommands.execute(vaultRoot, parsed);
                } else if ("list-sub-notes".equals(parsed.commandName())) {
                    response = ListSubNotes.execute(vaultRoot, parsed, lazyIndex.get());
                } else if ("list-docs".equals(parsed.commandName())) {
                    response = ListDocs.execute(vaultRoot, parsed, lazyIndex.get());
                } else if ("list-tags".equals(parsed.commandName())) {
                    response = ListTags.execute(parsed, lazyIndex.get());
                } else if ("search-docs".equals(parsed.commandName())) {
                    response = SearchDocs.execute(vaultRoot, parsed, lazyIndex.get());
                } else if ("list-templates".equals(parsed.commandName())) {
                    response = ListTemplates.execute(vaultRoot, parsed);
                } else if ("get-template".equals(parsed.commandName())) {
                    response = GetTemplate.execute(vaultRoot, parsed);
                } else if (Set.of("save-template", "delete-doc-template", "save-ritual-template").contains(parsed.commandName())) {
                    response = TemplateCommands.execute(vaultRoot, parsed);
                } else if ("export-markdown".equals(parsed.commandName())) {
                    response = ExportMarkdown.execute(vaultRoot);
                } else if ("plan-day".equals(parsed.commandName())) {
                    response = PlanDay.execute(vaultRoot, parsed);
                } else if ("check-notes".equals(parsed.commandName())) {
                    response = CheckNotes.execute(vaultRoot, parsed);
                } else if ("shard-notes".equals(parsed.commandName())) {
                    response = ShardNotes.execute(vaultRoot, parsed);
                } else if ("verify-shard".equals(parsed.commandName())) {
                    response = VerifyShard.execute(vaultRoot, parsed);
                } else if ("list-trash".equals(parsed.commandName())) {
                    response = ListTrash.execute(vaultRoot, parsed);
                } else if (Set.of("delete-doc", "restore-doc", "purge-doc", "empty-trash").contains(parsed.commandName())) {
                    response = TrashCommands.execute(vaultRoot, parsed);
                } else if ("list-projects".equals(parsed.commandName())) {
                    response = ListProjects.execute(vaultRoot);
                } else if ("create-project".equals(parsed.commandName())) {
                    response = CreateProject.execute(vaultRoot, parsed);
                } else if ("update-project".equals(parsed.commandName())) {
                    response = UpdateProject.execute(vaultRoot, parsed);
                } else if ("delete-project".equals(parsed.commandName())) {
                    response = DeleteProject.execute(vaultRoot, parsed);
                } else if (aisurada.cmd.ResourceCommands.NAMES.contains(parsed.commandName())) {
                    response = aisurada.cmd.ResourceCommands.execute(vaultRoot, parsed, sessionId);
                } else if (aisurada.cmd.TaskCommands.NAMES.contains(parsed.commandName())) {
                    response = aisurada.cmd.TaskCommands.execute(vaultRoot, parsed, sessionId);
                } else if ("list-agendas".equals(parsed.commandName())) {
                    response = ListAgendas.execute(vaultRoot);
                } else if ("create-agenda".equals(parsed.commandName())) {
                    response = CreateAgenda.execute(vaultRoot, parsed);
                } else if ("update-agenda".equals(parsed.commandName())) {
                    response = UpdateAgenda.execute(vaultRoot, parsed);
                } else if ("delete-agenda".equals(parsed.commandName())) {
                    response = DeleteAgenda.execute(vaultRoot, parsed);
                } else if ("get-agenda-facts".equals(parsed.commandName())) {
                    response = AgendaFacts.get(vaultRoot, parsed);
                } else if ("update-agenda-facts".equals(parsed.commandName())) {
                    response = AgendaFacts.update(vaultRoot, parsed);
                } else if ("facts-brief".equals(parsed.commandName())) {
                    response = FactsBrief.execute(vaultRoot, parsed);
                } else if ("init-workspace".equals(parsed.commandName())) {
                    response = InitWorkspace.execute(vaultRoot, parsed);
                } else if ("create-doc".equals(parsed.commandName())) {
                    response = aisurada.cmd.CreateDoc.execute(vaultRoot, parsed, sessionId);
                } else if ("write-attachment".equals(parsed.commandName())) {
                    response = aisurada.cmd.WriteAttachment.execute(vaultRoot, parsed);
                } else if ("create-sub-note".equals(parsed.commandName())) {
                    response = CreateSubNote.execute(vaultRoot, parsed);
                } else if ("update-sub-note".equals(parsed.commandName())) {
                    response = UpdateSubNote.execute(vaultRoot, parsed, sessionId);
                } else if ("delete-sub-note".equals(parsed.commandName())) {
                    response = DeleteSubNote.execute(vaultRoot, parsed);
                } else if ("update-doc".equals(parsed.commandName())) {
                    response = aisurada.cmd.UpdateDoc.execute(vaultRoot, parsed, sessionId);
                } else if ("set-favorite".equals(parsed.commandName())) {
                    response = aisurada.cmd.SetFavorite.execute(vaultRoot, parsed, sessionId);
                } else if ("evaluate-doc".equals(parsed.commandName())) {
                    response = aisurada.cmd.EvaluateDoc.execute(vaultRoot, parsed, sessionId);
                } else if ("link-docs".equals(parsed.commandName())) {
                    response = aisurada.cmd.LinkDocs.execute(vaultRoot, parsed, sessionId);
                } else {
                    throw new IllegalStateException("Unhandled command: " + parsed.commandName());
                }

                byte[] stdoutBytes = JsonWriter.toBytes(response);
                return new EngineResult(0, 0, stdoutBytes, diagnostics.text().getBytes(StandardCharsets.UTF_8));
            }

        } catch (AisException e) {
            byte[] err = (diagnostics.text() + e.toAisLine() + "\n").getBytes(StandardCharsets.UTF_8);
            return EngineResult.aisError(e.getExitCode(), e.getAisCode(), err);
        } catch (Throwable t) {
            if (isServeMode) {
                // 설명서 4.3: 상태 3, 오류 코드 0, stdout 0바이트, stderr 1줄
                String desc = t.getMessage();
                if (desc == null || desc.trim().isEmpty()) {
                    desc = t.getClass().getSimpleName();
                }
                if (desc == null || desc.trim().isEmpty()) {
                    desc = "알 수 없는 실패";
                }
                byte[] err = ("사이드카 안에서 실패했다: " + desc + ". 자세한 것은 사이드카 로그에 있다.\n").getBytes(StandardCharsets.UTF_8);
                t.printStackTrace(System.err);
                return EngineResult.internalError(3, 0, err);
            } else {
                // 단발 모드: 종료 코드 1, stdout 0바이트
                ByteArrayOutputStream baos = new ByteArrayOutputStream();
                PrintStream ps = new PrintStream(baos, true, StandardCharsets.UTF_8);
                ps.println("Application run failed: " + t.getMessage());
                t.printStackTrace(ps);
                return EngineResult.internalError(1, 0, baos.toByteArray());
            }
        }
    }
}
