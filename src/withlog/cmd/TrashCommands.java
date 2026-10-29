package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.engine.Diagnostics;
import withlog.engine.WritePipeline;
import withlog.out.JsonWriter;
import withlog.text.Trim;
import withlog.text.Utf16;
import withlog.vault.*;
import withlog.write.*;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.file.*;
import java.time.ZoneOffset;
import java.time.format.DateTimeFormatter;
import java.util.*;

/** 26장: 보관물을 먼저 옮기거나 지운 뒤 읽힌 항목만 매니페스트에 다시 쓴다. */
public final class TrashCommands {
    private static final DateTimeFormatter UTC = DateTimeFormatter.ofPattern("uuuu-MM-dd'T'HH:mm:ss.SSS'Z'").withZone(ZoneOffset.UTC);
    private TrashCommands() {}

    public static Map<String, Object> execute(Path vault, ParsedArgs args) {
        var index = WritePipeline.index(vault);
        if (args.commandName().equals("empty-trash")) return empty(vault);
        String usage = args.commandName().equals("delete-doc") ? "delete-doc <노트 id>" : args.commandName() + " <휴지통 항목 id. list-trash 로 조회한다>";
        if (args.positionalArgs().isEmpty() || Trim.isBlank(args.positionalArgs().getFirst())) throw new WlException(400, "인자가 하나 필요하다: " + usage);
        String id = args.positionalArgs().getFirst();
        if (args.commandName().equals("delete-doc")) {
            if (!IdGenerator.isValidNoteId(id)) throw new WlException(400, "노트 id 모양이 아니다: " + id);
            return delete(vault, index, id);
        }
        if (!id.matches("tr[0-9a-z]+-[0-9a-f]{8}")) throw new WlException(400, "휴지통 항목 id 모양이 아니다: " + id);
        var first = entries(vault).stream().filter(r -> r.trashId().equals(id)).findFirst().orElseThrow(() -> new WlException(404, "TrashEntry not found: " + id));
        if (args.commandName().equals("restore-doc")) return restore(vault, index, first);
        var entries = entries(vault);
        TreeRemover.remove(archive(vault, first.file()));
        save(vault, entries.stream().filter(r -> !r.trashId().equals(id)).toList());
        return row("trashId", id, "purged", true);
    }

    private static Map<String, Object> delete(Path vault, NoteIndex index, String id) {
        var note = index.notesById().get(id);
        if (note == null) throw new WlException(404, "Note not found: " + id);
        AttachText.extract(vault, note); // 기록 층도 보낸다. 도구의 읽기 게이트는 적용하지 않는다.
        String trashId = IdGenerator.nextId("tr"), path = note.vaultRelPath();
        String extension = note.format().equals("markdown") ? "md" : note.format();
        boolean single = path.matches("(?s).*\\.(md|html|json)$");
        String file = trashId + (single ? "." + extension : "");
        String slug = note.meta().agendaSlug();
        try {
            var meta = NoteFile.parse(single ? vault.resolve(path) : note.contentFilePath()).frontmatter();
            if (meta != null && meta.get("agenda") instanceof String s && !Trim.isBlank(s)) slug = s;
            else slug = path.split("/")[1];
        } catch (IOException e) { slug = path.split("/")[1]; }
        var now = TestApparatus.nowInstant();
        var entry = new TrashReader.TrashRecord(trashId, id, note.meta().title(), slug,
                index.agendaTable().slugToId().get(note.meta().agendaSlug()), note.meta().type(), note.format(), path, file,
                note.meta().created(), UTC.format(now), now, -1);
        var entries = entries(vault);
        Path source = vault.resolve(path);
        if (!Files.exists(source)) throw new WlException(400, "옮길 노트 파일이 없다: " + path);
        NoteMover.move(source, archive(vault, file));
        List<TrashReader.TrashRecord> updated = new ArrayList<>(); updated.add(entry);
        entries.stream().filter(r -> !r.trashId().equals(trashId)).forEach(updated::add); save(vault, updated);
        Map<String, Object> result = row("trashId", trashId, "id", id, "title", note.meta().title(), "originalPath", path, "file", file);
        String category = note.meta().favoriteCategory();
        if (note.meta().favorite()) result.put("wasFavorite", true);
        if (category != null) result.put("favoriteCategory", category);
        if (note.meta().favorite() || category != null) result.put("warning", "이 노트는 즐겨찾기였어요" + (category == null ? "" : "(카테고리 '" + category + "')") +
                " — 즐겨찾기는 노트 id에 붙으니 같은 내용을 새 노트로 다시 만들면 별이 따라오지 않습니다. 내용을 다시 쓰려던 거면 update_doc으로 이 노트를 키우고, 되살리려면 restore_doc({ trashId })을 쓰세요.");
        return result;
    }

    private static Map<String, Object> restore(Path vault, NoteIndex index, TrashReader.TrashRecord entry) {
        String slug = entry.agendaSlug();
        if (!AgendaReader.isValidSlugSegment(slug) || !index.agendaTable().slugToAgenda().containsKey(slug)) {
            slug = "recovered";
            if (!index.agendaTable().slugToAgenda().containsKey(slug)) AtomicWriter.writeAtomic(vault.resolve(".withlog/agendas/recovered.md"), Utf16.toUtf8BytesWithQuestionForUnpaired(
                    "---\nid: ag-recovered\nslug: recovered\ntitle: 복구함\nhue: blue\n---\n복구함\n\n삭제됐다가 복구된 노트 중, 원래 아젠다가 사라진 것들이 모이는 곳이에요."));
        }
        var entries = entries(vault);
        Path source = archive(vault, entry.file());
        if (!Files.exists(source)) {
            save(vault, entries.stream().filter(r -> !r.trashId().equals(entry.trashId())).toList());
            throw new WlException(400, "보관 파일이 사라져 복구할 수 없어요(휴지통 목록 정리됨).");
        }
        String[] pieces = entry.originalPath().split("[/\\\\]", -1);
        String name = pieces[pieces.length - 1];
        if (name.isEmpty() || name.contains("..") || name.startsWith(".")) name = entry.file();
        String parent = "notes/" + slug + (ShardLayout.isSharded(vault) ? "/" + entry.created().substring(0, 7) : "");
        PathConfinement.checkSegment(vault, parent); PathConfinement.checkSegment(vault, name);
        Path target = vault.resolve(parent).resolve(name).normalize(); PathConfinement.checkConfinement(vault, target);
        if (Files.exists(target)) {
            int dot = name.lastIndexOf('.');
            String tail = "-restored-" + entry.trashId().substring(entry.trashId().length() - 6);
            name = dot > 0 ? name.substring(0, dot) + tail + name.substring(dot) : name + tail;
            target = vault.resolve(parent).resolve(name).normalize(); PathConfinement.checkConfinement(vault, target);
        }
        if (!slug.equals(entry.agendaSlug())) rewriteAgenda(source, entry.format(), slug);
        NoteMover.move(source, target);
        save(vault, entries.stream().filter(r -> !r.trashId().equals(entry.trashId())).toList());
        return row("trashId", entry.trashId(), "restored", true, "path", vault.relativize(target).toString().replace('\\', '/'));
    }

    private static void rewriteAgenda(Path source, String format, String slug) {
        String extension = format.equals("markdown") ? "md" : format;
        boolean single = source.getFileName().toString().matches("(?s).*\\.(md|html|json)$");
        Path content = single ? source : source.resolve("note." + extension);
        NoteFile.ParsedNoteFile parsed;
        try { parsed = NoteFile.parse(content); }
        catch (IOException e) { Diagnostics.line("경고: 보관물 본문을 못 읽어 아젠다를 고치지 않았다: " + content); return; }
        if (!parsed.hasFrontmatter()) { Diagnostics.line("경고: 프론트매터가 없어 아젠다를 고치지 않았다: " + content); return; }
        Map<String, Object> meta = new LinkedHashMap<>(parsed.frontmatter()); meta.put("agenda", slug);
        String frontmatter = FrontmatterWriter.dumpGeneralMap(meta);
        String text = (format.equals("html") ? "<!--withlog\n" : "---\n") + frontmatter + (format.equals("html") ? "-->\n" : "---\n") + parsed.body();
        AtomicWriter.writeAtomic(content, Utf16.toUtf8BytesWithQuestionForUnpaired(text));
    }

    private static Path archive(Path vault, String file) {
        Path base = vault.resolve(".withlog/trash/files").toAbsolutePath().normalize();
        PathConfinement.checkSegment(base, file);
        Path target = base.resolve(file).normalize(); PathConfinement.checkConfinement(base, target); return target;
    }
    private static List<TrashReader.TrashRecord> entries(Path vault) {
        var scan = TrashReader.scan(vault);
        if (!scan.skipped().isEmpty()) System.getLogger(TrashCommands.class.getName()).log(System.Logger.Level.WARNING, "못 읽은 휴지통 항목 " + scan.skipped().size() + "개는 다음 쓰기에서 빠진다.");
        var entries = new ArrayList<>(scan.entries()); entries.sort(Comparator.comparingInt(TrashReader.TrashRecord::fileOrder)); return entries;
    }
    private static void save(Path vault, List<TrashReader.TrashRecord> entries) {
        var maps = entries.stream().map(r -> row("trashId", r.trashId(), "file", r.file(), "originalPath", r.originalPath(), "docId", r.docId(), "agendaId", r.agendaId(), "agendaSlug", r.agendaSlug(), "title", r.title(), "type", r.type(), "format", r.format(), "created", r.created(), "trashedAt", r.trashedAt())).toList();
        AtomicWriter.writeAtomic(vault.resolve(".withlog/trash/trash.json"), JsonWriter.toFileBytes(maps));
    }
    private static Map<String, Object> empty(Path vault) {
        var scan = TrashReader.scan(vault); var entries = new ArrayList<>(scan.entries()); entries.sort(Comparator.comparingInt(TrashReader.TrashRecord::fileOrder));
        if (!scan.skipped().isEmpty()) System.getLogger(TrashCommands.class.getName()).log(System.Logger.Level.WARNING, "못 읽은 휴지통 항목 " + scan.skipped().size() + "개는 다음 쓰기에서 빠진다.");
        Set<String> names = new HashSet<>(); for (var entry : entries) names.add(entry.file());
        int orphans = 0;
        try (var paths = Files.newDirectoryStream(vault.resolve(".withlog/trash/files"))) { for (Path path : paths) if (!names.contains(path.getFileName().toString())) orphans++; }
        catch (IOException ignored) {}
        List<TrashReader.TrashRecord> retained = new ArrayList<>(); List<Map<String, Object>> failures = new ArrayList<>();
        for (var entry : entries) {
            try { TreeRemover.remove(archive(vault, entry.file())); }
            catch (Exception e) {
                retained.add(entry); Throwable cause = e instanceof UncheckedIOException ? e.getCause() : e;
                String reason = e instanceof UncheckedIOException ? cause.toString() : e.getMessage();
                failures.add(row("trashId", entry.trashId(), "reason", reason == null ? e.getClass().getSimpleName() : reason));
            }
        }
        save(vault, retained); return row("purged", entries.size() - retained.size(), "failed", failures, "skipped", scan.skipped().size(), "orphans", orphans);
    }
    private static Map<String, Object> row(Object... values) {
        Map<String, Object> result = new LinkedHashMap<>(); for (int i = 0; i < values.length; i += 2) result.put((String) values[i], values[i + 1]); return result;
    }
}
