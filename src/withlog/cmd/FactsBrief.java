package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.engine.WritePipeline;
import withlog.text.Trim;
import withlog.text.Utf16;
import withlog.vault.FactsReader;
import withlog.vault.NoteIndex;
import withlog.vault.NoteScanner;
import withlog.write.TestApparatus;

import java.nio.file.Path;
import java.time.LocalDate;
import java.time.ZoneId;
import java.time.format.DateTimeParseException;
import java.util.*;

/** 22.5/22.6: 창 안 읽기 층 루트 노트를 세고 사실층 프롬프트 절을 짓는다. */
public final class FactsBrief {
    public static final String USAGE = "facts-brief [from=<YYYY-MM-DD>] [to=<YYYY-MM-DD>] [min-new-file=<노트 수>]";

    private FactsBrief() {}

    public static Map<String, Object> execute(Path vaultRoot, ParsedArgs parsed) {
        NoteIndex index = WritePipeline.index(vaultRoot);
        Map<String, String> options = parsed.namedOptions();
        LocalDate from = date(options.get("from"), "from");
        LocalDate to = date(options.get("to"), "to");
        int minNewFile = minimum(options.get("min-new-file"));
        if (to == null) to = TestApparatus.today(ZoneId.systemDefault());
        if (from == null) from = to.minusDays(6);
        if (from.isAfter(to)) {
            throw new WlException(400, "날짜 구간이 뒤집혔다: " + from + " ~ " + to);
        }

        String first = from.toString();
        String last = to.toString();
        Map<String, Integer> counts = new HashMap<>();
        for (NoteScanner.NoteRecord note : index.allRootNotes()) {
            var meta = note.meta();
            if (!"reading".equals(meta.tier())) continue;
            // 날짜를 다시 파싱하지 않는다. 색인의 날짜 글자와 바이트로 맞대는 22.9의 13번을 보존한다.
            if (NoteIndex.compareUtf8Bytes(meta.date(), first) < 0 ||
                    NoteIndex.compareUtf8Bytes(meta.date(), last) > 0) continue;
            if (!index.agendaTable().slugToAgenda().containsKey(meta.agendaSlug())) continue;
            counts.merge(meta.agendaSlug(), 1, Integer::sum);
        }
        List<String> slugs = new ArrayList<>(counts.keySet());
        slugs.sort((a, b) -> {
            int byCount = Integer.compare(counts.get(b), counts.get(a));
            return byCount != 0 ? byCount : Utf16.compare(a, b);
        });

        List<Map<String, Object>> items = new ArrayList<>();
        List<String> blocks = new ArrayList<>();
        for (String slug : slugs) {
            int notes = counts.get(slug);
            String facts = FactsReader.readFacts(vaultRoot, slug);
            if (facts == null && notes < minNewFile) continue;
            Map<String, Object> item = new LinkedHashMap<>();
            item.put("agenda", slug);
            item.put("notes", notes);
            String heading = "### " + slug + " — 노트 " + notes + " 건";
            if (facts == null) {
                blocks.add(heading + ", 사실층 파일이 아직 없다");
            } else {
                item.put("facts", facts);
                blocks.add(heading + "\n\n" + body(facts));
            }
            items.add(item);
        }
        Map<String, Object> result = new LinkedHashMap<>();
        result.put("from", first);
        result.put("to", last);
        result.put("count", items.size());
        result.put("section", String.join("\n\n", blocks));
        result.put("items", items);
        return result;
    }

    private static LocalDate date(String raw, String field) {
        String value = Trim.trim(raw);
        if (value.isEmpty()) return null;
        try {
            return LocalDate.parse(value);
        } catch (DateTimeParseException e) {
            throw new WlException(400, field + " 은 YYYY-MM-DD 모양이어야 한다: " + value);
        }
    }

    private static int minimum(String raw) {
        String value = Trim.trim(raw);
        if (value.isEmpty()) return 5;
        int number;
        try {
            number = Integer.parseInt(value);
        } catch (NumberFormatException e) {
            throw new WlException(400, "min-new-file 은 숫자여야 한다: " + value);
        }
        if (number < 1) throw new WlException(400, "min-new-file 은 1 이상이어야 한다: " + number);
        return number;
    }

    private static String body(String facts) {
        String[] lines = facts.split("\r\n|\r|\n", -1);
        int first = lines[0].startsWith("# ") ? 1 : 0;
        while (first < lines.length && Trim.isBlank(lines[first])) first++;
        String body = String.join("\n", Arrays.copyOfRange(lines, first, lines.length));
        int end = body.length();
        while (end > 0 && Trim.isWhitespace(body.charAt(end - 1))) end--;
        return body.substring(0, end);
    }
}
