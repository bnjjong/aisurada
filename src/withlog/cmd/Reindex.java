package withlog.cmd;

import withlog.vault.*;

import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * reindex 명령 실행기.
 * 설명서 8.4 및 8.6에 따라 디스크를 전체 훑고 규격 진단 결과를 반환합니다.
 */
public final class Reindex {
    private Reindex() {}

    /**
     * reindex 명령을 실행하고 규격 JSON 응답 Map을 생성합니다.
     * @param vaultRoot vault 루트 절대 경로
     * @param startTimeMillis 시작 시각 (밀리초)
     * @return 8.4 표 순서에 맞춘 응답 Map
     */
    public static Map<String, Object> execute(Path vaultRoot, long startTimeMillis) {
        // 1. 아젠다 훑기 (haltOnFatal = true)
        AgendaReader.ScanResult agendaScan = AgendaReader.scanAgendas(vaultRoot, true);

        // 2. 노트 훑기 (haltOnFatal = true)
        NoteScanner.ScanResult noteScan = NoteScanner.scan(vaultRoot, agendaScan.table(), true);

        // 3. 템플릿 훑기 (2.4, 24장)
        TemplateReader.ScanResult templateScan = TemplateReader.scan(vaultRoot);

        // 4. 회고 훑기 (2.5, 25장) - .withlog/reports/ 폴더 목록 못 읽으면 WL-500 던짐
        ReportReader.ScanResult reportScan = ReportReader.scan(vaultRoot);

        // 5. 휴지통 훑기 (2.6, 26장)
        TrashReader.ScanResult trashScan = TrashReader.scan(vaultRoot);

        // 6. skipped 합치기 차례 (8.4):
        // 아젠다, 노트 훑기 단계의 건너뜀 및 노트를 열다 생긴 건너뜀, 템플릿, 회고, 휴지통
        List<Map<String, String>> allSkipped = new ArrayList<>();
        allSkipped.addAll(agendaScan.skipped());
        allSkipped.addAll(noteScan.skipped());

        for (TemplateReader.SkippedTemplate st : templateScan.skipped()) {
            Map<String, String> m = new LinkedHashMap<>();
            m.put("kind", "doc".equals(st.category()) ? "docTemplate" : "ritualTemplate");
            m.put("path", st.relPath());
            m.put("reason", st.reason());
            allSkipped.add(m);
        }

        List<Map<String, String>> reviewConflicts = new ArrayList<>();
        for (ReportReader.SkippedReport sr : reportScan.skipped()) {
            if (sr.isConflict()) {
                Map<String, String> m = new LinkedHashMap<>();
                m.put("kind", "review");
                m.put("path", sr.relPath());
                reviewConflicts.add(m);
            } else {
                Map<String, String> m = new LinkedHashMap<>();
                m.put("kind", "review");
                m.put("path", sr.relPath());
                m.put("reason", sr.reason());
                allSkipped.add(m);
            }
        }

        for (TrashReader.SkippedTrash str : trashScan.skipped()) {
            Map<String, String> m = new LinkedHashMap<>();
            m.put("kind", "trash");
            m.put("path", str.location());
            m.put("reason", str.reason());
            allSkipped.add(m);
        }

        // conflicts: note conflicts 뒤에 review conflicts
        List<Map<String, String>> allConflicts = new ArrayList<>(noteScan.conflicts());
        allConflicts.addAll(reviewConflicts);

        return response("full", startTimeMillis, agendaScan, noteScan, allSkipped, allConflicts);
    }

    /** 8.5: 200칸 초과만 전체 노트로 물러서고, 두 갈래 모두 템플릿/회고/휴지통은 열지 않는다. */
    public static Map<String, Object> paths(Path vaultRoot, List<String> paths, long startTimeMillis) {
        var agendas = AgendaReader.scanAgendas(vaultRoot, true);
        boolean full = paths.size() > 200;
        var notes = full ? NoteScanner.scan(vaultRoot, agendas.table(), true)
                : NoteScanner.scanPaths(vaultRoot, agendas.table(), paths);
        List<Map<String, String>> skipped = new ArrayList<>(agendas.skipped()); skipped.addAll(notes.skipped());
        return response(full ? "full" : "paths", startTimeMillis, agendas, notes, skipped, notes.conflicts());
    }

    private static Map<String, Object> response(String mode, long startTimeMillis, AgendaReader.ScanResult agendaScan,
            NoteScanner.ScanResult noteScan, List<Map<String, String>> allSkipped, List<Map<String, String>> allConflicts) {
        int indexed = noteScan.notesById().size();
        int unchanged = 0;
        int scanned = indexed + unchanged;

        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("mode", mode);
        resp.put("elapsedMs", Math.min(Integer.MAX_VALUE, Math.max(0, System.currentTimeMillis() - startTimeMillis)));
        resp.put("agendas", agendaScan.table().slugToId().size());
        resp.put("scanned", scanned);
        resp.put("indexed", indexed);
        resp.put("unchanged", unchanged);
        resp.put("removed", 0);
        resp.put("skipped", allSkipped);
        resp.put("conflicts", allConflicts);
        resp.put("unsharded", noteScan.unsharded());

        return resp;
    }
}
