package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.vault.TrashReader;

import java.nio.file.Path;
import java.util.*;

/**
 * list_trash 및 list-trash 명령 실행기.
 * 설명서 26장에 따라 휴지통 매니페스트 항목을 trashedAt 시각 내림차순으로 반환합니다.
 */
public final class ListTrash {

    private static final String INCOMPLETE_NOTE = "일부 파일을 읽지 못했어요(형식 오류·크기 초과) — 목록이 불완전할 수 있어요.";

    private ListTrash() {}

    public static Object execute(
            Path vaultRoot,
            ParsedArgs parsed
    ) {
        TrashReader.ScanResult scan = TrashReader.scan(vaultRoot);

        List<Map<String, Object>> rows = new ArrayList<>(scan.entries().size());
        for (TrashReader.TrashRecord r : scan.entries()) {
            Map<String, Object> row = new LinkedHashMap<>();
            row.put("trashId", r.trashId());
            row.put("docId", r.docId());
            row.put("title", r.title());
            row.put("agenda", r.agendaSlug());
            row.put("agendaId", r.agendaId());
            row.put("type", r.type());
            row.put("format", r.format());
            row.put("originalPath", r.originalPath());
            row.put("file", r.file());
            row.put("created", r.created());
            row.put("trashedAt", r.trashedAt());
            rows.add(row);
        }

        if (!scan.skipped().isEmpty()) {
            if (!parsed.isToolSurface()) {
                System.err.println("휴지통 항목 " + scan.skipped().size() + "건을 못 읽어 목록에서 뺐다.");
                for (TrashReader.SkippedTrash s : scan.skipped()) {
                    System.err.println("  " + s.location() + " — " + s.reason());
                }
            }
            Map<String, Object> incomplete = new LinkedHashMap<>();
            incomplete.put("skipped", scan.skipped().size());
            incomplete.put("conflicts", 0);
            incomplete.put("note", INCOMPLETE_NOTE);

            Map<String, Object> envelope = new LinkedHashMap<>();
            envelope.put("trash", rows);
            envelope.put("incomplete", incomplete);
            return envelope;
        }

        return rows;
    }
}
