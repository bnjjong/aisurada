package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.vault.ReportReader;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * get_report 및 get-report 명령 실행기.
 * 설명서 25장에 따라 회고 id로 파일을 직접 조회하여 본문과 메모를 반환합니다.
 */
public final class GetReport {

    private static final Pattern ID_PATTERN = Pattern.compile("^rv-([a-z]+)-([0-9]{4}-[0-9]{2}-[0-9]{2})$");

    private GetReport() {}

    public static Map<String, Object> execute(
            Path vaultRoot,
            ParsedArgs parsed
    ) {
        String id;
        if (parsed.isToolSurface()) {
            Map<String, Object> opts = parsed.toolOptions();
            id = (opts != null) ? (String) opts.get("id") : null;
            if (id == null) {
                throw new WlException(400, "인자 검증 실패(get_report): id 가 필요하다.");
            }
        } else {
            if (parsed.positionalArgs().isEmpty()) {
                throw new WlException(400, "인자가 하나 필요하다: get-report <회고 id. 예: rv-evening-2026-06-04>");
            }
            if (parsed.positionalArgs().size() > 1) {
                throw new WlException(400, "get-report 이 받는 자리 인자는 1 개인데 " +
                        parsed.positionalArgs().size() + " 개를 받았다: get-report <회고 id. 예: rv-evening-2026-06-04>");
            }
            id = parsed.positionalArgs().get(0);
        }

        // 값 검증 (다듬지 않는다)
        if (id.isEmpty() || Trim.isBlank(id)) {
            throw new WlException(400, "인자가 하나 필요하다: get-report <회고 id. 예: rv-evening-2026-06-04>");
        }

        Matcher m = ID_PATTERN.matcher(id);
        if (!m.matches()) {
            throw new WlException(400, "회고 id 모양이 아니다: " + id);
        }

        String kind = m.group(1);
        String date = m.group(2);

        if (!ReportReader.ALLOWED_KINDS.contains(kind)) {
            throw new WlException(404, "Review not found: " + id);
        }

        Path file = vaultRoot.resolve(".withlog").resolve("reports").resolve(id + ".json");
        if (!Files.isRegularFile(file)) {
            throw new WlException(404, "Review not found: " + id);
        }

        String relPath = ".withlog/reports/" + id + ".json";
        ReportReader.ParseResult res = ReportReader.parseReportFile(file, id, kind, date, relPath);
        if (res instanceof ReportReader.ParseResult.Success succ) {
            ReportReader.ReportRecord r = succ.record();
            Map<String, Object> resp = new LinkedHashMap<>();
            resp.put("id", r.id());
            resp.put("kind", r.kind());
            resp.put("date", r.date());
            resp.put("body", r.body()); // null if absent/blank
            resp.put("memo", r.memo()); // null if absent/blank
            return resp;
        } else if (res instanceof ReportReader.ParseResult.Failed fail) {
            if (fail.step() == 6) {
                // 껍데기는 WL-404
                throw new WlException(404, "Review not found: " + id);
            } else {
                throw new WlException(500, "회고 파일이 자리에 있는데 읽지 못했다: " + relPath + " (" + fail.reason() + ")");
            }
        }

        throw new WlException(404, "Review not found: " + id);
    }
}
