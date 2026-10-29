package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.text.Utf16;
import withlog.vault.TemplateReader;

import java.nio.file.Path;
import java.util.*;

/**
 * list_templates 및 list-templates 명령 실행기.
 * 설명서 24장에 따라 문서 및 리추얼 템플릿 목록을 반환합니다.
 */
public final class ListTemplates {

    private static final String INCOMPLETE_NOTE = "일부 파일을 읽지 못했어요(형식 오류·크기 초과) — 목록이 불완전할 수 있어요.";

    private ListTemplates() {}

    public static Object execute(
            Path vaultRoot,
            ParsedArgs parsed
    ) {
        boolean isTool = parsed.isToolSurface();

        String rawCategory = null;
        String rawType = null;
        String rawBodies = null;

        if (isTool) {
            Map<String, Object> opts = parsed.toolOptions();
            if (opts != null) {
                rawCategory = (String) opts.get("category");
                rawType = (String) opts.get("type");
            }
        } else {
            Map<String, String> opts = parsed.namedOptions();
            rawCategory = opts.get("category");
            rawType = opts.get("type");
            rawBodies = opts.get("bodies");
        }

        // 1. category 검증 (24.3 1번)
        String targetCategory = null;
        if (rawCategory != null) {
            String trimmedCat = Trim.trim(rawCategory);
            if (!trimmedCat.isEmpty()) {
                String lowerCat = trimmedCat.toLowerCase(Locale.ROOT);
                if (!"doc".equals(lowerCat) && !"ritual".equals(lowerCat)) {
                    throw new WlException(400, "모르는 템플릿 갈래 이다: " + trimmedCat +
                            ". 쓸 수 있는 것은 [doc, ritual] 뿐이다.");
                }
                targetCategory = lowerCat;
            }
        }

        // 2. type 검증 (24.3 2번)
        String targetType = null;
        if (rawType != null) {
            String trimmedType = Trim.trim(rawType);
            if (!trimmedType.isEmpty()) {
                String lowerType = trimmedType.toLowerCase(Locale.ROOT);
                Set<String> allowed = Set.of("plan", "build", "report", "research", "meeting", "free");
                if (!allowed.contains(lowerType)) {
                    throw new WlException(400, "모르는 템플릿 유형 이다: " + trimmedType +
                            ". 쓸 수 있는 것은 [plan, build, report, research, meeting, free] 뿐이다.");
                }
                targetType = lowerType;
            }
        }

        // 3. 폴더 훑기 (24.3 3번)
        TemplateReader.ScanResult scan = TemplateReader.scan(vaultRoot);
        if (!isTool) printDiagnosticsIfAny(scan.skipped(), targetCategory, targetType);

        // 4. bodies 검증 (명령 표면만, 24.3 4번: 폴더 훑은 뒤 검사)
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

        // 5. 템플릿 필터링 및 정렬
        List<TemplateReader.TemplateRecord> filtered = new ArrayList<>();
        for (TemplateReader.TemplateRecord t : scan.templates()) {
            if (targetCategory != null && !targetCategory.equals(t.category())) {
                continue;
            }
            if (targetType != null) {
                if (!"doc".equals(t.category()) || !targetType.equalsIgnoreCase(t.type())) {
                    continue;
                }
            }
            filtered.add(t);
        }

        // 정렬: category(doc -> ritual), id UTF-16 차례, 같으면 읽은 차례
        filtered.sort((t1, t2) -> {
            int c1 = "doc".equals(t1.category()) ? 0 : 1;
            int c2 = "doc".equals(t2.category()) ? 0 : 1;
            int cmpCat = Integer.compare(c1, c2);
            if (cmpCat != 0) return cmpCat;

            return Utf16.compare(t1.id(), t2.id());
        });

        List<Map<String, Object>> templateRows = new ArrayList<>(filtered.size());
        for (TemplateReader.TemplateRecord t : filtered) {
            templateRows.add(formatRow(t, includeBodies));
        }

        // 6. incomplete 필터링 및 봉투 구성 (24.6)
        List<TemplateReader.SkippedTemplate> filteredSkipped = new ArrayList<>();
        for (TemplateReader.SkippedTemplate s : scan.skipped()) {
            if (targetCategory != null && !targetCategory.equals(s.category())) {
                continue;
            }
            if (targetType != null && !"doc".equals(s.category())) {
                continue;
            }
            filteredSkipped.add(s);
        }

        if (!filteredSkipped.isEmpty()) {
            Map<String, Object> incompleteObj = new LinkedHashMap<>();
            incompleteObj.put("skipped", filteredSkipped.size());
            incompleteObj.put("conflicts", 0);
            incompleteObj.put("note", INCOMPLETE_NOTE);

            Map<String, Object> envelope = new LinkedHashMap<>();
            envelope.put("templates", templateRows);
            envelope.put("incomplete", incompleteObj);
            return envelope;
        }

        return templateRows;
    }

    private static void printDiagnosticsIfAny(List<TemplateReader.SkippedTemplate> allSkipped, String targetCategory, String targetType) {
        List<TemplateReader.SkippedTemplate> filtered = new ArrayList<>();
        for (TemplateReader.SkippedTemplate s : allSkipped) {
            if (targetCategory != null && !targetCategory.equals(s.category())) {
                continue;
            }
            if (targetType != null && !"doc".equals(s.category())) {
                continue;
            }
            filtered.add(s);
        }
        if (!filtered.isEmpty()) {
            printDiagnostics(filtered);
        }
    }

    private static void printDiagnostics(List<TemplateReader.SkippedTemplate> skippedList) {
        withlog.engine.Diagnostics.line("템플릿 " + skippedList.size() + "건을 못 읽어 목록에서 뺐다.");
        for (TemplateReader.SkippedTemplate s : skippedList) {
            withlog.engine.Diagnostics.line("  " + s.relPath() + " — " + s.reason());
        }
    }

    public static Map<String, Object> formatRow(TemplateReader.TemplateRecord t, boolean includeBody) {
        Map<String, Object> row = new LinkedHashMap<>();
        row.put("id", t.id());
        row.put("category", t.category());
        row.put("name", t.name());

        if ("doc".equals(t.category())) {
            row.put("type", t.type());
            row.put("format", t.format());
            row.put("desc", t.desc());
            if (includeBody) {
                row.put("body", t.body());
            }
            row.put("builtin", t.builtin());
        } else {
            row.put("rev", t.rev());
            row.put("icon", t.icon());
            row.put("format", t.format());
            row.put("desc", t.desc());
            if (includeBody) {
                row.put("body", t.body());
            }
        }
        return row;
    }
}
