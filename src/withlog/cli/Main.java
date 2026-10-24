package withlog.cli;

import withlog.args.ArgsParser;
import withlog.args.ParsedArgs;
import withlog.cmd.GetDoc;
import withlog.cmd.GetSubNote;
import withlog.cmd.Reindex;
import withlog.out.JsonWriter;
import withlog.text.Trim;
import withlog.vault.AgendaReader;
import withlog.vault.NoteIndex;
import withlog.vault.NoteScanner;
import withlog.vault.VaultPath;

import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.*;

/**
 * withlog 1부 엔진 단발 모드 실행 진입점.
 * 설명서 3.1, 5장, 6장, 11장, 34장에 따라 단발 실행 차례 및 규격 출력을 처리합니다.
 */
public class Main {

    private static final Set<String> TOOL_NAMES = Set.of(
            "list_docs", "search_docs", "get_doc", "list_tags",
            "list_sub_notes", "get_sub_note", "list_attachments", "list_comments",
            "list_trash", "get_report", "list_reports", "list_templates", "get_template",
            "create_doc", "update_doc", "delete_doc", "set_favorite", "evaluate_doc", "link_docs",
            "create_sub_note", "update_sub_note", "delete_sub_note", "write_attachment", "delete_attachment",
            "save_report", "save_template", "restore_doc", "purge_doc", "plan_day"
    );

    private static final Set<String> COMMAND_NAMES = Set.of(
            "reindex", "reindex-paths", "list-docs", "search-docs", "get-doc", "list-tags",
            "create-doc", "update-doc", "set-favorite", "evaluate-doc", "link-docs", "delete-doc",
            "list-sub-notes", "get-sub-note", "create-sub-note", "update-sub-note", "delete-sub-note",
            "list-attachments", "write-attachment", "delete-attachment", "list-comments", "create-comment",
            "update-comment", "delete-comment", "list-agendas", "create-agenda", "update-agenda", "delete-agenda",
            "list-projects", "create-project", "update-project", "delete-project", "get-agenda-facts",
            "update-agenda-facts", "facts-brief", "init-workspace", "list-templates", "get-template",
            "save-template", "save-ritual-template", "delete-doc-template", "list-reports", "get-report",
            "save-report", "list-trash", "restore-doc", "purge-doc", "empty-trash", "export-markdown",
            "shard-notes", "verify-shard", "plan-day", "check-notes", "history", "commit-vault",
            "chat-reply", "chat-init", "chat-send", "ask", "read-session", "list-sessions", "delete-session"
    );

    public static void main(String[] args) {
        long startTime = System.currentTimeMillis();

        // 환경변수 설정 유효성 검사 (11.2)
        validateEnvConfig("WITHLOG_NOTES_SHARD_BY_MONTH", "withlog.notes.shard.by.month");
        validateEnvConfig("WITHLOG_VAULT_SNAPSHOT_ENABLED", "withlog.vault.snapshot.enabled");
        validateEnvConfig("WITHLOG_VAULT_AUTOCOMMIT_ENABLED", "withlog.vault.autocommit.enabled");
        validateEnvConfig("WITHLOG_VAULT_AUTOCOMMIT_IDLE_SECONDS", "withlog.vault.autocommit.idle.seconds");
        validateEnvConfig("WITHLOG_IDLE_TIMEOUT_MINUTES", "withlog.idle.timeout.minutes");

        // 1. vault 문턱 (3.1 1번)
        String envVault = System.getenv("WITHLOG_VAULT");
        boolean hasCliVaultPrefix = false;
        for (String arg : args) {
            if (arg.startsWith("--withlog.vault=")) {
                hasCliVaultPrefix = true;
                break;
            }
        }

        if ((envVault == null || Trim.isBlank(envVault)) && !hasCliVaultPrefix) {
            System.err.println("WITHLOG_VAULT 환경변수가 없다. vault 폴더의 절대 경로를 넣고 다시 실행한다.");
            System.err.println("설정으로 주려면 --withlog.vault=<경로> 를 붙인다.");
            System.exit(2);
        }

        // 2. 설정 칸 읽기 (3.1 2번)
        String chosenVault = null;
        boolean exactVaultMatchFound = false;

        List<String> remainingArgv = new ArrayList<>();
        boolean insideSettings = true;

        for (String arg : args) {
            if (insideSettings && "--".equals(arg)) {
                insideSettings = false;
                continue;
            }

            if (insideSettings && arg.startsWith("--")) {
                int eqIdx = arg.indexOf('=');
                String configName;
                String configVal;
                if (eqIdx != -1) {
                    configName = arg.substring(2, eqIdx);
                    configVal = arg.substring(eqIdx + 1);
                } else {
                    configName = arg.substring(2);
                    configVal = "";
                }

                if (configName.isEmpty()) {
                    // 이름이 빈 칸 (--=x) -> 엔진 시작 실패
                    System.err.println("Invalid argument syntax: name is empty in " + arg);
                    System.exit(1);
                }

                validateConfig(configName, configVal);

                if (isMatchingVaultConfig(configName)) {
                    if ("withlog.vault".equals(configName)) {
                        if (exactVaultMatchFound) {
                            chosenVault = chosenVault + "," + configVal;
                        } else {
                            chosenVault = configVal;
                            exactVaultMatchFound = true;
                        }
                    } else if (!exactVaultMatchFound) {
                        chosenVault = configVal;
                    }
                }
            } else {
                remainingArgv.add(arg);
            }
        }

        // 3. 엔진 조립 (vault 해석)
        String rawVault = chosenVault != null ? chosenVault : envVault;
        Path vaultRoot;
        try {
            String resolved = VaultPath.resolveAndValidate(rawVault);
            vaultRoot = Paths.get(resolved);
        } catch (Exception e) {
            System.err.println("Application run failed: " + e.getMessage());
            System.exit(1);
            return;
        }

        // 4. serve 모드 확인
        if (args.length > 0 && "serve".equals(args[0])) {
            System.err.println("WL-409 이 판에는 serve이 없다");
            System.exit(3);
            return;
        }

        // 5. 세션 표식 분리 (단발 모드에선 NUL 불가로 통상 없음)
        if (remainingArgv.size() >= 2) {
            int secondFromLast = remainingArgv.size() - 2;
            if ("\u0000wl-session-id".equals(remainingArgv.get(secondFromLast))) {
                remainingArgv.remove(secondFromLast + 1);
                remainingArgv.remove(secondFromLast);
            }
        }

        // 6. 이름 찾기 (5.1)
        if (remainingArgv.isEmpty()) {
            printUnknownCommandAndExit("(없음)");
            return;
        }

        String firstArg = remainingArgv.get(0);
        boolean isTool = TOOL_NAMES.contains(firstArg);
        boolean isCmd = !isTool && COMMAND_NAMES.contains(firstArg);

        if (!isTool && !isCmd) {
            printUnknownCommandAndExit(firstArg);
            return;
        }

        // 지원 대상 외 명령 거절 (spec-gaps 후보 6번: 종료 코드 1)
        boolean isSupported = "get_doc".equals(firstArg) || "get-doc".equals(firstArg) ||
                "get_sub_note".equals(firstArg) || "get-sub-note".equals(firstArg) ||
                "list_attachments".equals(firstArg) || "list-attachments".equals(firstArg) ||
                "list_sub_notes".equals(firstArg) || "list-sub-notes".equals(firstArg) ||
                "list_docs".equals(firstArg) || "list-docs".equals(firstArg) ||
                "list_tags".equals(firstArg) || "list-tags".equals(firstArg) ||
                "search_docs".equals(firstArg) || "search-docs".equals(firstArg) ||
                "list_templates".equals(firstArg) || "list-templates".equals(firstArg) ||
                "get_template".equals(firstArg) || "get-template".equals(firstArg) ||
                "list_reports".equals(firstArg) || "list-reports".equals(firstArg) ||
                "get_report".equals(firstArg) || "get-report".equals(firstArg) ||
                "list_trash".equals(firstArg) || "list-trash".equals(firstArg) ||
                "reindex".equals(firstArg);

        if (!isSupported) {
            System.err.println("WL-409 이 판에는 " + firstArg + "이 없다");
            System.exit(3);
            return;
        }

        // 7. 명령 인자 분리 및 실행
        List<String> commandArgs = remainingArgv.subList(1, remainingArgv.size());

        try {
            ParsedArgs parsed = ArgsParser.parse(firstArg, commandArgs);
            Object response;

            if ("reindex".equals(parsed.commandName())) {
                response = Reindex.execute(vaultRoot, startTime);
            } else if ("get-doc".equals(parsed.commandName())) {
                NoteIndex noteIndex = NoteIndex.get(vaultRoot);
                response = GetDoc.execute(vaultRoot, parsed, noteIndex.agendaTable(), noteIndex.noteScan());
            } else if ("get-sub-note".equals(parsed.commandName())) {
                NoteIndex noteIndex = NoteIndex.get(vaultRoot);
                response = GetSubNote.execute(vaultRoot, parsed, noteIndex.noteScan());
            } else if ("list-attachments".equals(parsed.commandName())) {
                NoteIndex noteIndex = NoteIndex.get(vaultRoot);
                response = withlog.cmd.ListAttachments.execute(vaultRoot, parsed, noteIndex);
            } else if ("list-sub-notes".equals(parsed.commandName())) {
                NoteIndex noteIndex = NoteIndex.get(vaultRoot);
                response = withlog.cmd.ListSubNotes.execute(vaultRoot, parsed, noteIndex);
            } else if ("list-docs".equals(parsed.commandName())) {
                NoteIndex noteIndex = NoteIndex.get(vaultRoot);
                response = withlog.cmd.ListDocs.execute(vaultRoot, parsed, noteIndex);
            } else if ("list-tags".equals(parsed.commandName())) {
                NoteIndex noteIndex = NoteIndex.get(vaultRoot);
                response = withlog.cmd.ListTags.execute(parsed, noteIndex);
            } else if ("search-docs".equals(parsed.commandName())) {
                NoteIndex noteIndex = NoteIndex.get(vaultRoot);
                response = withlog.cmd.SearchDocs.execute(vaultRoot, parsed, noteIndex);
            } else if ("list-templates".equals(parsed.commandName())) {
                response = withlog.cmd.ListTemplates.execute(vaultRoot, parsed);
            } else if ("get-template".equals(parsed.commandName())) {
                response = withlog.cmd.GetTemplate.execute(vaultRoot, parsed);
            } else if ("list-reports".equals(parsed.commandName())) {
                response = withlog.cmd.ListReports.execute(vaultRoot, parsed);
            } else if ("get-report".equals(parsed.commandName())) {
                response = withlog.cmd.GetReport.execute(vaultRoot, parsed);
            } else if ("list-trash".equals(parsed.commandName())) {
                response = withlog.cmd.ListTrash.execute(vaultRoot, parsed);
            } else {
                throw new IllegalStateException("Unhandled command: " + parsed.commandName());
            }

            byte[] stdoutBytes = JsonWriter.toBytes(response);
            System.out.write(stdoutBytes);
            System.out.flush();
            System.exit(0);

        } catch (WlException e) {
            System.err.println(e.toWlLine());
            System.exit(e.getExitCode());
        } catch (Throwable t) {
            System.err.println("Application run failed: " + t.getMessage());
            t.printStackTrace(System.err);
            System.exit(1);
        }
    }

    private static boolean isMatchingVaultConfig(String name) {
        if (name == null) return false;
        String[] parts = name.split("\\.");
        if (parts.length != 2) return false;
        String p1 = parts[0].replace("-", "").toLowerCase(Locale.ROOT);
        String p2 = parts[1].replace("-", "").toLowerCase(Locale.ROOT);
        return "withlog".equals(p1) && "vault".equals(p2);
    }

    private static final Set<String> TRUTHY_BOOLEANS = Set.of("true", "on", "yes", "1");
    private static final Set<String> FALSY_BOOLEANS = Set.of("false", "off", "no", "0");

    private static String normalizeConfigName(String name) {
        if (name == null) return "";
        return name.replace(".", "").replace("-", "").replace("_", "").toLowerCase(Locale.ROOT);
    }

    private static String trimAscii(String s) {
        if (s == null) return "";
        int start = 0;
        while (start < s.length() && s.charAt(start) <= ' ') start++;
        int end = s.length();
        while (end > start && s.charAt(end - 1) <= ' ') end--;
        return s.substring(start, end);
    }

    private static void validateShardByMonth(String rawVal) {
        String trimmed = trimAscii(rawVal);
        if (trimmed.isEmpty()) {
            return;
        }
        String lower = trimmed.toLowerCase(Locale.ROOT);
        if (!TRUTHY_BOOLEANS.contains(lower) && !FALSY_BOOLEANS.contains(lower)) {
            System.err.println("Invalid boolean value for withlog.notes.shard.by.month: " + rawVal);
            System.exit(1);
        }
    }

    private static void validateStrictBoolean(String configName, String rawVal) {
        String trimmed = trimAscii(rawVal);
        if (trimmed.isEmpty()) {
            System.err.println("Empty value for " + configName);
            System.exit(1);
        }
        String lower = trimmed.toLowerCase(Locale.ROOT);
        if (!TRUTHY_BOOLEANS.contains(lower) && !FALSY_BOOLEANS.contains(lower)) {
            System.err.println("Invalid boolean value for " + configName + ": " + rawVal);
            System.exit(1);
        }
    }

    private static boolean isTrimWhitespace(char c) {
        if ((c >= 0x0009 && c <= 0x000D) || (c >= 0x001C && c <= 0x001F)) {
            return true;
        }
        int type = Character.getType(c);
        return type == Character.SPACE_SEPARATOR ||
               type == Character.LINE_SEPARATOR ||
               type == Character.PARAGRAPH_SEPARATOR;
    }

    private static long parseCustomLong(String s) {
        boolean negative = false;
        String rem = s;
        if (rem.startsWith("-")) {
            negative = true;
            rem = rem.substring(1);
        } else if (rem.startsWith("+")) {
            rem = rem.substring(1);
            if (rem.startsWith("0x") || rem.startsWith("0X") || rem.startsWith("#")) {
                throw new NumberFormatException("+0x not allowed");
            }
        }
        int radix = 10;
        if (rem.startsWith("0x") || rem.startsWith("0X")) {
            radix = 16;
            rem = rem.substring(2);
        } else if (rem.startsWith("#")) {
            radix = 16;
            rem = rem.substring(1);
        }
        if (rem.isEmpty()) throw new NumberFormatException("empty digits");

        StringBuilder normalized = new StringBuilder();
        for (int i = 0; i < rem.length(); i++) {
            char ch = rem.charAt(i);
            int d = Character.digit(ch, radix);
            if (d < 0) throw new NumberFormatException("invalid digit: " + ch);
            normalized.append(Integer.toString(d, radix));
        }
        long val = Long.parseLong(normalized.toString(), radix);
        return negative ? -val : val;
    }

    private static void validateIntegerConfig(String configName, String rawVal) {
        if (rawVal == null || rawVal.isEmpty()) {
            System.err.println("Empty integer value for " + configName);
            System.exit(1);
        }
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < rawVal.length(); i++) {
            char ch = rawVal.charAt(i);
            if (ch == 0x00A0 || ch == 0x2007 || ch == 0x202F) {
                sb.append(ch);
            } else if (isTrimWhitespace(ch)) {
                // remove whitespace
            } else {
                sb.append(ch);
            }
        }
        String s = sb.toString();
        if (s.isEmpty()) {
            System.err.println("Empty integer value for " + configName);
            System.exit(1);
        }
        try {
            parseCustomLong(s);
        } catch (Exception e) {
            System.err.println("Invalid integer value for " + configName + ": " + rawVal);
            System.exit(1);
        }
    }

    private static void validateConfig(String name, String val) {
        String norm = normalizeConfigName(name);
        switch (norm) {
            case "withlognotesshardbymonth" -> validateShardByMonth(val);
            case "withlogvaultsnapshotenabled", "withlogvaultautocommitenabled" -> validateStrictBoolean(name, val);
            case "withlogvaultautocommitidleseconds", "withlogidletimeoutminutes" -> validateIntegerConfig(name, val);
            default -> {}
        }
    }

    private static void validateEnvConfig(String envKey, String configName) {
        String val = System.getenv(envKey);
        if (val != null) {
            validateConfig(configName, val);
        }
    }

    private static void printUnknownCommandAndExit(String name) {
        System.err.println("모르는 명령이다: " + name);
        System.err.println("도구 이름으로 부르려면 이렇게 한다: <도구> '<json>'");
        System.err.println("  list_docs, search_docs, list_tags, get_doc, list_trash, list_reports, get_report, list_templates, get_template, list_sub_notes, get_sub_note, list_attachments");
        System.err.println("사이드카 고유 명령은 이렇다.");
        System.err.println("  reindex");
        System.err.println("  reindex-paths <vault 상대 경로> [경로...]");
        System.exit(2);
    }
}
