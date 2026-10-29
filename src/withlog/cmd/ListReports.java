package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.vault.ReportReader;

import java.nio.file.Path;
import java.util.*;
import java.util.regex.Pattern;

/**
 * list_reports 및 list-reports 명령 실행기.
 * 설명서 25장에 따라 회고 목록을 날짜 및 종류 순서로 반환합니다.
 */
public final class ListReports {

    private static final Pattern DATE_PATTERN = Pattern.compile("^[0-9]{4}-[0-9]{2}-[0-9]{2}$");
    private static final String INCOMPLETE_NOTE = "일부 파일을 읽지 못했어요(형식 오류·크기 초과) — 목록이 불완전할 수 있어요.";

    private ListReports() {}

    public static Object execute(
            Path vaultRoot,
            ParsedArgs parsed
    ) {
        boolean isTool = parsed.isToolSurface();

        String rawKind = null;
        String rawFrom = null;
        String rawTo = null;
        String rawBodies = null;

        if (isTool) {
            Map<String, Object> opts = parsed.toolOptions();
            if (opts != null) {
                rawKind = (String) opts.get("kind");
                rawFrom = (String) opts.get("dateFrom");
                rawTo = (String) opts.get("dateTo");
            }
        } else {
            Map<String, String> opts = parsed.namedOptions();
            rawKind = opts.get("kind");
            rawFrom = opts.get("from");
            rawTo = opts.get("to");
            rawBodies = opts.get("bodies");
        }

        // 1. bodies 검증 (명령 표면만, 25.3 1번)
        boolean includeBodies = false;
        if (!isTool && rawBodies != null) {
            String trimmedBodies = Trim.trim(rawBodies);
            if (!trimmedBodies.isEmpty()) {
                if ("true".equals(trimmedBodies)) {
                    includeBodies = true;
                } else if ("false".equals(trimmedBodies)) {
                    includeBodies = false;
                } else {
                    throw new WlException(400, "bodies 은 true 나 false 여야 한다: " + trimmedBodies);
                }
            }
        }

        // 2. kind 검증 (25.3 2번)
        String targetKind = null;
        if (rawKind != null) {
            String trimmedKind = Trim.trim(rawKind);
            if (!trimmedKind.isEmpty()) {
                String lowerKind = trimmedKind.toLowerCase(Locale.ROOT);
                if (!ReportReader.ALLOWED_KINDS.contains(lowerKind)) {
                    throw new WlException(400, "모르는 회고 종류 이다: " + trimmedKind +
                            ". 쓸 수 있는 것은 [morning, evening, weekly] 뿐이다.");
                }
                targetKind = lowerKind;
            }
        }

        // 3. from 검증 (25.3 3번)
        String targetFrom = null;
        if (rawFrom != null) {
            String trimmedFrom = Trim.trim(rawFrom);
            if (!trimmedFrom.isEmpty()) {
                if (!DATE_PATTERN.matcher(trimmedFrom).matches()) {
                    throw new WlException(400, "from 은 YYYY-MM-DD 모양이어야 한다: " + trimmedFrom);
                }
                targetFrom = trimmedFrom;
            }
        }

        // 4. to 검증 (25.3 4번)
        String targetTo = null;
        if (rawTo != null) {
            String trimmedTo = Trim.trim(rawTo);
            if (!trimmedTo.isEmpty()) {
                if (!DATE_PATTERN.matcher(trimmedTo).matches()) {
                    throw new WlException(400, "to 은 YYYY-MM-DD 모양이어야 한다: " + trimmedTo);
                }
                targetTo = trimmedTo;
            }
        }

        // 5. 날짜 구간 검증 (25.3 5번)
        if (targetFrom != null && targetTo != null) {
            if (targetFrom.compareTo(targetTo) > 0) {
                throw new WlException(400, "날짜 구간이 뒤집혔다: " + targetFrom + " ~ " + targetTo);
            }
        }

        // 6. 폴더 훑기 (25.3 6번)
        ReportReader.ScanResult scan = ReportReader.scan(vaultRoot);

        // 7. 필터링 및 정렬
        List<ReportReader.ReportRecord> filtered = new ArrayList<>();
        for (ReportReader.ReportRecord r : scan.reports()) {
            if (targetKind != null && !targetKind.equals(r.kind())) {
                continue;
            }
            if (targetFrom != null && r.date().compareTo(targetFrom) < 0) {
                continue;
            }
            if (targetTo != null && r.date().compareTo(targetTo) > 0) {
                continue;
            }
            filtered.add(r);
        }

        filtered.sort((r1, r2) -> {
            int cmpDate = r1.date().compareTo(r2.date());
            if (cmpDate != 0) return cmpDate;
            return r1.kind().compareTo(r2.kind());
        });

        List<Map<String, Object>> rows = new ArrayList<>(filtered.size());
        for (ReportReader.ReportRecord r : filtered) {
            Map<String, Object> row = new LinkedHashMap<>();
            row.put("id", r.id());
            row.put("kind", r.kind());
            row.put("date", r.date());
            row.put("hasBody", r.hasBody());
            row.put("hasMemo", r.hasMemo());
            if (includeBodies) {
                row.put("body", r.body());
                row.put("memo", r.memo());
            }
            rows.add(row);
        }

        // 8. incomplete 봉투 (25.6: 못 읽은 파일은 필터로 거르지 않고 모두 센다)
        if (!scan.skipped().isEmpty()) {
            if (!isTool) {
                withlog.engine.Diagnostics.line("회고 " + scan.skipped().size() + "건을 못 읽어 목록에서 뺐다.");
                for (ReportReader.SkippedReport s : scan.skipped()) {
                    withlog.engine.Diagnostics.line("  " + s.relPath() + " — " + s.reason());
                }
            }
            Map<String, Object> incomplete = new LinkedHashMap<>();
            incomplete.put("skipped", scan.skipped().size());
            incomplete.put("conflicts", 0);
            incomplete.put("note", INCOMPLETE_NOTE);

            Map<String, Object> envelope = new LinkedHashMap<>();
            envelope.put("reports", rows);
            envelope.put("incomplete", incomplete);
            return envelope;
        }

        return rows;
    }
}
