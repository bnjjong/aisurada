package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.engine.WritePipeline;
import withlog.out.JsonWriter;
import withlog.text.Trim;
import withlog.vault.ReportReader;
import withlog.write.AtomicWriter;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;

/** 25장: 자연키로 회고를 통째로 저장하며 두 칸이 모두 비면 삭제한다. */
public final class SaveReport {
    public static final String USAGE = "save-report kind=<morning|evening|weekly> date=<YYYY-MM-DD> [body=<다듬은 본문>] [memo=<개인 메모>]";
    private SaveReport() {}

    public static Map<String, Object> execute(Path vault, ParsedArgs args) {
        WritePipeline.index(vault);
        Map<String, String> options = args.namedOptions();
        String rawKind = Trim.trim(options.get("kind"));
        String kind = rawKind.toLowerCase(Locale.ROOT);
        if (kind.isEmpty()) throw new WlException(400, "kind 이 필요하다: " + USAGE);
        if (!ReportReader.ALLOWED_KINDS.contains(kind)) throw new WlException(400,
                "모르는 회고 종류 이다: " + rawKind + ". 쓸 수 있는 것은 [morning, evening, weekly] 뿐이다.");
        String date = Trim.trim(options.get("date"));
        if (date.isEmpty()) throw new WlException(400, "date 이 필요하다: " + USAGE);
        if (!date.matches("[0-9]{4}-[0-9]{2}-[0-9]{2}")) throw new WlException(400, "date 은 YYYY-MM-DD 모양이어야 한다: " + date);
        String id = "rv-" + kind + "-" + date, relative = ".withlog/reports/" + id + ".json";
        Path file = vault.resolve(relative);
        Map<String, Object> content = new LinkedHashMap<>();
        for (String key : List.of("body", "memo")) {
            String value = options.get(key);
            if (!Trim.isBlank(value)) content.put(key, value);
        }
        Map<String, Object> result = new LinkedHashMap<>();
        if (content.isEmpty()) {
            try { Files.deleteIfExists(file); }
            catch (IOException e) { throw new WlException(500, "파일을 지우지 못했다: " + file.toAbsolutePath().normalize()); }
            result.put("saved", false); result.put("deleted", true); result.put("id", id);
        } else {
            AtomicWriter.writeAtomic(file, JsonWriter.toFileBytes(content));
            result.put("saved", true); result.put("id", id); result.put("kind", kind); result.put("date", date);
            result.put("hasBody", content.containsKey("body")); result.put("hasMemo", content.containsKey("memo")); result.put("path", relative);
        }
        return result;
    }
}
