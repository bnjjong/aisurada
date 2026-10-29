package withlog.cmd;

import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.vault.*;
import withlog.write.TestApparatus;

import java.nio.file.Path;
import java.time.*;
import java.time.format.DateTimeParseException;
import java.util.*;

/** 29/30장 공용 날짜와 읽기 층 메인 노트 순서. */
final class DailyNotes {
    private DailyNotes() {}
    static LocalDate date(String raw) {
        String value = Trim.trim(raw);
        if (value.isEmpty()) return TestApparatus.today(ZoneId.systemDefault());
        try { return LocalDate.parse(value); }
        catch (DateTimeParseException e) { throw new WlException(400, "date 은 YYYY-MM-DD 모양이어야 한다: " + value); }
    }
    static boolean bool(String raw, String key) {
        String value = Trim.trim(raw);
        if (value.isEmpty() || value.equals("false")) return false;
        if (value.equals("true")) return true;
        throw new WlException(400, key + " 은 true 나 false 여야 한다: " + value);
    }
    static List<NoteScanner.NoteRecord> read(Path vault, NoteIndex index) {
        List<NoteScanner.NoteRecord> notes = new ArrayList<>();
        for (var note : index.allRootNotes()) {
            if (!"reading".equals(note.meta().tier())) continue;
            AttachText.extract(vault, note); notes.add(note);
        }
        notes.sort((a, b) -> {
            int time = NoteIndex.compareUtf8Bytes(b.meta().created(), a.meta().created());
            return time != 0 ? time : NoteIndex.compareUtf8Bytes(a.vaultRelPath().replace('/', '\u0001'), b.vaultRelPath().replace('/', '\u0001'));
        });
        return notes;
    }
    static OffsetDateTime offset(String text) {
        if (text == null) return null;
        try { return OffsetDateTime.parse(text); }
        catch (DateTimeParseException e) { return null; }
    }
}
