package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.engine.Diagnostics;
import withlog.text.Trim;
import withlog.vault.FileSize;
import withlog.write.PathConfinement;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;

/** 28.8: 이동 전 매핑의 수와 현재 디스크를 대조하며 불일치도 종료 코드 0으로 답한다. */
public final class VerifyShard {
    private record Row(String id, String oldRel, String newRel, int files, long bytes, String reason) {}
    private VerifyShard() {}
    public static Map<String, Object> execute(Path vault, ParsedArgs args) {
        String given = Trim.trim(args.namedOptions().get("file"));
        Path directory = vault.resolve(".withlog/migration").toAbsolutePath().normalize(), file;
        if (!given.isEmpty()) {
            if (!Files.isDirectory(directory)) throw new WlException(404, "shard map not found: " + given);
            file = directory.resolve(Objects.requireNonNull(Path.of(given).getFileName()).toString());
            if (!Files.isRegularFile(file)) throw new WlException(404, "shard map not found: " + given);
        } else {
            List<Path> paths = new ArrayList<>();
            if (Files.isDirectory(directory)) try (var entries = Files.newDirectoryStream(directory)) {
                for (Path p : entries) if (Files.isRegularFile(p) && p.getFileName().toString().startsWith("shard-map-") && p.getFileName().toString().endsWith(".tsv")) paths.add(p);
            } catch (IOException e) { throw new UncheckedIOException(e); }
            paths.sort(Comparator.comparing(p -> p.getFileName().toString()));
            List<String> details = new ArrayList<>();
            for (Path p : paths) {
                var rows = read(p); long moving = rows.stream().filter(r -> !r.newRel().isEmpty() && !r.newRel().equals(r.oldRel())).count();
                details.add("  " + p.getFileName() + " (줄 " + rows.size() + ", 자리 바뀜 " + moving + ")");
            }
            if (paths.isEmpty()) throw new WlException(404, "shard map not found: (이 vault 에 기준값 파일이 없다)");
            if (paths.size() > 1) throw new WlException(409, "기준값 파일이 " + paths.size() + " 개다. 어느 회차와 맞댈지 file= 로 못박아야 한다.\n" + String.join("\n", details) + "\n자리 바뀜이 0 인 회차는 한 건도 안 옮겼다. 그 파일의 크기 값은 이동 뒤에 잰 것이라 기준값이 아니다.");
            file = paths.getFirst();
        }
        var rows = read(file); int checked = 0;
        List<Map<String, Object>> missing = new ArrayList<>(), left = new ArrayList<>(), mismatch = new ArrayList<>(), unmovable = new ArrayList<>();
        for (Row row : rows) {
            if (row.newRel().isEmpty()) { unmovable.add(item(row.id(), row.oldRel(), row.reason())); continue; }
            checked++; Path target = resolve(vault, row.newRel());
            if (!Files.exists(target)) { missing.add(item(row.id(), row.newRel(), "옮겨 갔다고 적힌 자리가 없다")); continue; }
            FileSize size = FileSize.measure(target);
            if (!row.oldRel().equals(row.newRel()) && Files.exists(resolve(vault, row.oldRel()))) left.add(item(row.id(), row.oldRel(), "옮겼는데 옛 자리에도 남아 있다"));
            if (size.files() != row.files() || size.bytes() != row.bytes()) mismatch.add(map("noteId", row.id(), "path", row.newRel(), "fileCountBefore", row.files(), "fileCountAfter", size.files(), "byteSumBefore", row.bytes(), "byteSumAfter", size.bytes()));
        }
        boolean ok = missing.isEmpty() && left.isEmpty() && mismatch.isEmpty();
        if (!ok) {
            Diagnostics.line("이동 전 기준값과 다르다. 없는 자리 " + missing.size() + " 건, 옛 자리에 남은 것 " + left.size() + " 건, 크기가 갈린 것 " + mismatch.size() + " 건.");
            Diagnostics.line("기준값: " + file); Diagnostics.line("되돌리려면 방향을 거짓으로 못박고 shard-notes apply=true 를 돌린다. 그전에 쓰는 주체 셋을 내린다.");
        }
        return map("ok", ok, "mappingFile", file.toString(), "rows", rows.size(), "checked", checked, "missing", missing, "leftBehind", left, "sizeMismatch", mismatch, "unmovable", unmovable);
    }
    private static List<Row> read(Path file) {
        if (!Files.exists(file)) return List.of();
        String text;
        try { text = new String(Files.readAllBytes(file), StandardCharsets.UTF_8); }
        catch (IOException e) { throw new WlException(500, "파일을 읽지 못했다: " + file); }
        List<Row> rows = new ArrayList<>(); String[] lines = text.split("\r\n|\r|\n", -1);
        for (int i = 1; i < lines.length; i++) {
            if (Trim.isBlank(lines[i])) continue;
            String[] columns = lines[i].split("\t", -1); if (columns.length < 6) continue;
            try { rows.add(new Row(columns[0], columns[1], columns[2], Integer.parseInt(columns[3]), Long.parseLong(columns[4]), columns[5])); }
            catch (NumberFormatException ignored) {}
        }
        return rows;
    }
    private static Path resolve(Path vault, String path) {
        PathConfinement.checkSegment(vault, path); Path target = vault.resolve(path).toAbsolutePath().normalize();
        PathConfinement.checkConfinement(vault, target); return target;
    }
    private static Map<String, Object> item(String id, String path, String reason) { return map("noteId", id, "path", path, "reason", reason); }
    private static Map<String, Object> map(Object... values) {
        Map<String, Object> result = new LinkedHashMap<>(); for (int i = 0; i < values.length; i += 2) result.put((String) values[i], values[i + 1]); return result;
    }
}
