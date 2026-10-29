package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.engine.WritePipeline;
import withlog.text.Trim;
import withlog.vault.*;

import java.io.IOException;
import java.nio.file.Path;
import java.time.*;
import java.time.format.DateTimeFormatter;
import java.time.temporal.ChronoUnit;
import java.util.*;

/** 30장: 마지막 코멘트와 마지막 변경을 각자의 오프셋으로 비교한다. */
public final class CheckNotes {
    private static final DateTimeFormatter DISPLAY = DateTimeFormatter.ofPattern("uuuu-MM-dd HH:mm");
    private record Pending(Map<String, Object> item, OffsetDateTime comment, String line) {}
    private CheckNotes() {}

    @SuppressWarnings("unchecked")
    public static Map<String, Object> execute(Path vault, ParsedArgs args) {
        var index = WritePipeline.index(vault);
        LocalDate date = DailyNotes.date(args.namedOptions().get("date")), start = date.minusDays(14);
        var notes = DailyNotes.read(vault, index);
        Map<String, OffsetDateTime> stamps = new HashMap<>();
        for (var note : notes) {
            if (note.format().equals("json")) continue;
            try {
                var parsed = NoteFile.parse(note.contentFilePath());
                var meta = parsed.frontmatter();
                if (meta == null || !(meta.get("editedBy") instanceof String by) || Trim.isBlank(by) || !(meta.get("editedAt") instanceof String at)) continue;
                OffsetDateTime stamp = DailyNotes.offset(Trim.trim(at));
                if (stamp != null) stamps.put(note.id(), stamp);
            } catch (IOException ignored) {}
        }
        OffsetDateTime earliest = stamps.values().stream().min(OffsetDateTime::compareTo).orElse(null);
        List<Pending> pending = new ArrayList<>();
        for (var note : notes) {
            if (note.isSingleFile()) continue;
            Map<String, Object> envelope = CommentsReader.readComments(vault, vault.resolve(note.vaultRelPath()).resolve("comments.json"));
            if (!"loaded".equals(envelope.get("state"))) continue;
            var comments = (List<Map<String, Object>>) envelope.get("comments");
            OffsetDateTime comment = comments.stream().map(c -> DailyNotes.offset((String) c.get("createdAt"))).filter(Objects::nonNull).max(OffsetDateTime::compareTo).orElse(null);
            if (comment == null || comment.toLocalDate().isBefore(start)) continue;
            OffsetDateTime created = DailyNotes.offset(note.meta().created()), change = stamps.get(note.id());
            boolean stamped = change != null;
            if (!stamped) {
                if (created == null || earliest != null && created.toInstant().isBefore(earliest.toInstant())) continue;
                change = created;
            }
            if (!comment.truncatedTo(ChronoUnit.MINUTES).toInstant().isAfter(change.truncatedTo(ChronoUnit.MINUTES).toInstant())) continue;
            String agenda = index.agendaTable().slugToTitle().get(note.meta().agendaSlug());
            Map<String, Object> item = new LinkedHashMap<>();
            item.put("check", "unanswered-comments"); item.put("checkDisplay", "답 없는 코멘트"); item.put("id", note.id()); item.put("title", note.meta().title());
            if (agenda != null) item.put("agenda", agenda);
            item.put("date", note.meta().date()); item.put("comments", comments.size()); item.put("lastCommentAt", comment.toString()); item.put("lastChangeAt", change.toString()); item.put("stamped", stamped);
            String line = "- 답 없는 코멘트: \"" + note.meta().title() + "\"" + (agenda == null ? "" : " (" + agenda + ")") + ", " + note.meta().date() + ". 코멘트 " + comments.size() + "개, 마지막 " + DISPLAY.format(comment) + ". 그 뒤로 노트가 " +
                    (stamped ? "바뀌지 않았다(마지막 편집 " : "바뀐 기록이 없다(편집 도장 없음, 작성 ") + DISPLAY.format(change) + ").";
            pending.add(new Pending(item, comment, line));
        }
        pending.sort((a, b) -> b.comment().compareTo(a.comment()));
        Map<String, Object> result = new LinkedHashMap<>();
        result.put("date", date.toString()); result.put("windowDays", 14); result.put("count", pending.size());
        result.put("section", String.join("\n", pending.stream().map(Pending::line).toList()));
        result.put("items", pending.stream().map(Pending::item).toList()); return result;
    }
}
