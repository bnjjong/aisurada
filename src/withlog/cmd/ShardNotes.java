package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.engine.Diagnostics;
import withlog.engine.WritePipeline;
import withlog.text.Utf16;
import withlog.vault.*;
import withlog.write.*;

import java.io.IOException;
import java.nio.file.*;
import java.time.ZoneId;
import java.time.format.DateTimeFormatter;
import java.util.*;
import java.util.regex.Pattern;

/** 28장: 색인에 들지 못한 노트까지 디스크에서 모아 계획하고 원자적으로 옮긴다. */
public final class ShardNotes {
    private static final Set<String> RESERVED = Set.of("attach", "sub", "sub-comments", "chats");
    private static final Pattern CONFLICT = Pattern.compile("conflicted copy|\\(conflicted\\b|충돌(된)?\\s*사본|conflicted version", Pattern.CASE_INSENSITIVE | Pattern.UNICODE_CASE);
    private record Candidate(String id, String oldRel, String newRel, FileSize size, String reason, String agenda) {}
    private ShardNotes() {}

    public static Map<String, Object> execute(Path vault, ParsedArgs args) {
        WritePipeline.index(vault);
        boolean apply = DailyNotes.bool(args.namedOptions().get("apply"), "apply");
        try { return migrate(vault, apply); }
        finally {
            if (apply) try { Reindex.execute(vault, System.currentTimeMillis()); }
            catch (Exception e) { Diagnostics.line("색인을 못 맞췄다. 앱을 열기 전에 reindex 를 먼저 돌린다: " + e.getMessage()); }
        }
    }
    private static Map<String, Object> migrate(Path vault, boolean apply) {
        boolean sharded = ShardLayout.isSharded(vault);
        List<Candidate> candidates = new ArrayList<>(); Path notes = vault.resolve("notes");
        if (Files.isDirectory(notes)) {
            List<Path> agendas;
            try { agendas = children(notes); }
            catch (IOException e) { throw new WlException(500, "노트 폴더를 읽지 못했다: " + notes); }
            for (Path agenda : agendas) if (Files.isDirectory(agenda)) collect(vault, agenda, agenda.getFileName().toString(), 0, sharded, candidates);
        }
        Map<String, Integer> totals = new HashMap<>(); for (var c : candidates) totals.merge(c.agenda(), 1, Integer::sum);
        candidates.sort(Comparator.comparingInt((Candidate c) -> totals.get(c.agenda())).thenComparing(Candidate::agenda).thenComparing(Candidate::oldRel));
        Path mapping = mapping(vault, apply);
        StringBuilder tsv = new StringBuilder("noteId\toldRel\tnewRel\tfileCount\tbyteSum\treason\n");
        for (var c : candidates) tsv.append(c.id()).append('\t').append(c.oldRel()).append('\t').append(c.newRel()).append('\t').append(c.size().files()).append('\t').append(c.size().bytes()).append('\t').append(c.reason().replace('\t', ' ').replace('\n', ' ')).append('\n');
        AtomicWriter.writeAtomic(mapping, Utf16.toUtf8BytesWithQuestionForUnpaired(tsv.toString()));
        int planned = 0, moved = 0, already = 0, pruned = 0;
        List<Map<String, Object>> unmovable = new ArrayList<>();
        Map<String, int[]> agendas = new HashMap<>(); Set<Path> oldParents = new LinkedHashSet<>();
        Map<String, Object> stopped = null;
        for (var c : candidates) {
            if (c.newRel().isEmpty()) { unmovable.add(row("path", c.oldRel(), "reason", c.reason())); continue; }
            if (c.newRel().equals(c.oldRel())) { already++; continue; }
            planned++; agendas.computeIfAbsent(c.agenda(), a -> new int[2])[0]++;
        }
        if (apply) for (var c : candidates) {
            if (c.newRel().isEmpty() || c.newRel().equals(c.oldRel())) continue;
            Path source = vault.resolve(c.oldRel()), target = vault.resolve(c.newRel());
            PathConfinement.checkSegment(vault, c.newRel()); PathConfinement.checkConfinement(vault, target);
            NoteMover.move(source, target);
            FileSize after = FileSize.measure(target);
            if (!after.equals(c.size())) {
                stopped = row("notePath", c.oldRel(), "movedTo", c.newRel(), "fileCountBefore", c.size().files(), "fileCountAfter", after.files(), "byteSumBefore", c.size().bytes(), "byteSumAfter", after.bytes());
                Diagnostics.line("이동을 중간에 세웠다. " + moved + " 건까지 옮겼다. 옮긴 자리의 크기가 옮기기 전과 다르다: " + c.oldRel() + " (파일 " + c.size().files() + "->" + after.files() + ", 바이트 " + c.size().bytes() + "->" + after.bytes() + ")");
                Diagnostics.line("다른 주체가 같은 시각에 vault 를 고치고 있다. 사이드카와 앱과 launchd 잡을 내리고 다시 돌린다."); Diagnostics.line("매핑 파일: " + mapping); break;
            }
            moved++; agendas.get(c.agenda())[1]++; oldParents.add(source.getParent());
        }
        for (Path parent : oldParents) {
            Path relative = vault.relativize(parent);
            if (relative.getNameCount() < 3 || !relative.startsWith("notes")) continue;
            try { Files.delete(parent); pruned++; } catch (IOException ignored) {}
        }
        List<String> names = new ArrayList<>(agendas.keySet()); names.sort(Comparator.comparingInt((String a) -> agendas.get(a)[0]).thenComparing(a -> a));
        List<Map<String, Object>> byAgenda = names.stream().map(a -> row("agenda", a, "planned", agendas.get(a)[0], "moved", agendas.get(a)[1])).toList();
        Map<String, Object> result = row("sharded", sharded, "applied", apply, "mappingFile", mapping.toString(), "total", candidates.size(), "planned", planned, "moved", moved, "alreadyInPlace", already, "prunedEmptyDirs", pruned, "unmovable", unmovable, "byAgenda", byAgenda);
        if (stopped != null) result.put("stoppedAt", stopped); return result;
    }
    private static Path mapping(Path vault, boolean apply) {
        Path directory = vault.resolve(".withlog/migration").toAbsolutePath().normalize();
        if (!apply) return directory.resolve("shard-plan.tsv");
        String stem = "shard-map-" + DateTimeFormatter.ofPattern("yyyyMMdd-HHmmss").format(TestApparatus.nowLocalDateTime(ZoneId.systemDefault()));
        Path file = directory.resolve(stem + ".tsv");
        for (int n = 2; Files.exists(file); n++) file = directory.resolve(stem + "~" + n + ".tsv");
        return file;
    }
    private static void collect(Path vault, Path directory, String agenda, int depth, boolean sharded, List<Candidate> candidates) {
        List<Path> paths; try { paths = children(directory); } catch (IOException e) { return; }
        for (Path path : paths) {
            String name = path.getFileName().toString(), relative = vault.relativize(path).toString().replace('\\', '/');
            if (RESERVED.contains(name)) continue;
            String reason = null;
            if (Files.isSymbolicLink(path)) reason = "심링크라 따라가지 않았다";
            else if (CONFLICT.matcher(name).find()) reason = "동기화 충돌 사본이다";
            if (reason != null) { candidates.add(new Candidate("", relative, "", new FileSize(0, 0), reason, agenda)); continue; }
            Path body = null;
            if (Files.isDirectory(path)) {
                body = NoteScanner.findFolderContentFile(path);
                if (body == null) {
                    if (depth >= 8) candidates.add(new Candidate("", relative, "", new FileSize(0, 0), "스캔 깊이 상한을 넘었다", agenda));
                    else collect(vault, path, agenda, depth + 1, sharded, candidates);
                    continue;
                }
            } else if (Files.isRegularFile(path) && name.matches("(?s).*\\.(md|html|json)$") && !Set.of("comments.json", "ask.json").contains(name)) body = path;
            if (body == null) continue;
            String id = "", destination = ""; reason = "";
            try {
                var parsed = NoteFile.parse(body);
                if (!parsed.hasFrontmatter()) reason = "프론트매터가 없다";
                else {
                    var meta = parsed.frontmatter(); if (meta.get("id") instanceof String s) id = s;
                    var created = meta.get("created") instanceof String s ? NoteMeta.parseCreated(s) : null;
                    if (created == null) reason = "created 가 달력에 없는 날짜다";
                    else destination = "notes/" + agenda + (sharded ? "/" + created.date().substring(0, 7) : "") + "/" + name;
                }
            } catch (IOException e) { reason = "본문 파일을 못 읽었다"; }
            candidates.add(new Candidate(id, relative, destination, FileSize.measure(path), reason, agenda));
        }
    }
    private static List<Path> children(Path path) throws IOException {
        List<Path> paths = new ArrayList<>(); try (var entries = Files.newDirectoryStream(path)) { for (Path entry : entries) paths.add(entry); }
        paths.sort(Comparator.comparing(p -> p.getFileName().toString())); return paths;
    }
    private static Map<String, Object> row(Object... values) {
        Map<String, Object> result = new LinkedHashMap<>(); for (int i = 0; i < values.length; i += 2) result.put((String) values[i], values[i + 1]); return result;
    }
}
