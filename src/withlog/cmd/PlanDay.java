package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.engine.Diagnostics;
import withlog.engine.WritePipeline;
import withlog.text.Trim;
import withlog.vault.NoteScanner;

import java.nio.file.Path;
import java.time.LocalDate;
import java.util.*;

/** 29장: 직전 기록일과 오늘의 초안을 짓고 create_doc과 같은 경로로 저장한다. */
public final class PlanDay {
    private PlanDay() {}
    public static Map<String, Object> execute(Path vault, ParsedArgs args) {
        var index = WritePipeline.index(vault);
        LocalDate date = DailyNotes.date(args.namedOptions().get("date"));
        boolean dryRun = DailyNotes.bool(args.namedOptions().get("dryRun"), "dryRun");
        var notes = DailyNotes.read(vault, index);
        String previousLimit = date.minusDays(1).toString();
        String previous = notes.stream().map(n -> n.meta().date()).filter(d -> d.compareTo(previousLimit) <= 0).max(String::compareTo).orElse(null);
        List<NoteScanner.NoteRecord> yesterday = new ArrayList<>(), today = new ArrayList<>();
        for (var note : notes) {
            if (note.meta().date().equals(previous)) yesterday.add(note);
            if (note.meta().date().equals(date.toString())) today.add(note);
        }
        yesterday.sort(Comparator.comparing((NoteScanner.NoteRecord n) -> n.meta().agendaSlug()).thenComparing(n -> n.meta().time()).thenComparing(n -> n.meta().title()));
        today.sort(Comparator.comparing((NoteScanner.NoteRecord n) -> n.meta().time()).thenComparing(n -> n.meta().agendaSlug()).thenComparing(n -> n.meta().title()));
        String title = date + " 데일리 플랜";
        StringBuilder body = new StringBuilder("# " + title + "\n\n## 어제 한 일" + (previous == null ? "" : " (" + previous + ")") + "\n");
        append(body, yesterday, index.agendaTable().slugToTitle(), "- (기록 없음)");
        body.append("\n## 오늘\n"); append(body, today, index.agendaTable().slugToTitle(), "- (아직 예정 없음)");
        String agenda = Trim.trim(args.namedOptions().get("agenda"));
        Map<String, Object> result = new LinkedHashMap<>();
        if (agenda.isEmpty() || dryRun) {
            result.put("saved", false); result.put("date", date.toString());
        } else {
            Map<String, String> values = Map.of("agenda", agenda, "type", "plan", "title", title, "body", body.toString(), "date", date.toString(), "time", "09:00", "mins", "0");
            var saved = CreateDoc.execute(vault, new ParsedArgs(false, "create-doc", List.of(), values, null), null);
            result.put("saved", true);
            for (String key : List.of("id", "agenda", "agendaCreated", "path")) result.put(key, saved.get(key));
            if (saved.get("warning") instanceof String warning) Diagnostics.line(warning);
        }
        result.put("body", body.toString()); return result;
    }
    private static void append(StringBuilder body, List<NoteScanner.NoteRecord> notes, Map<String, String> titles, String empty) {
        if (notes.isEmpty()) body.append(empty).append('\n');
        else for (var note : notes) body.append("- ").append(note.meta().title()).append(" · ").append(titles.getOrDefault(note.meta().agendaSlug(), "")).append(" (").append(note.meta().type()).append(")\n");
    }
}
