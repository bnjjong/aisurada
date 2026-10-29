package withlog.vault;

import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.text.Utf16;
import withlog.write.PathConfinement;
import withlog.write.ShardLayout;

import java.io.IOException;
import java.nio.file.DirectoryStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;
import java.util.regex.Pattern;

/**
 * vault의 notes/ 디렉토리 훑기(Note Scanner) 및 하위 노트 탐색기.
 * 설명서 1.2, 1.12, 8.4에 따라 노트 및 하위 노트를 훑고 메타데이터를 구축합니다.
 */
public final class NoteScanner {

    private static final Pattern CONFLICT_PATTERN = Pattern.compile(
            "conflicted copy|\\(conflicted\\b|충돌(된)?\\s*사본|conflicted version",
            Pattern.CASE_INSENSITIVE | Pattern.UNICODE_CASE
    );

    private static final Set<String> RESERVED_NAMES = Set.of("attach", "sub", "sub-comments", "chats");

    /**
     * 스캔된 단일 노트 (루트 노트 또는 하위 노트) 정보 DTO.
     */
    public record NoteRecord(
            String id,
            boolean isSubNote,
            String rootNoteId,
            String directParentId,
            String parentChainPath,
            String vaultRelPath,
            String format,
            Path contentFilePath,
            NoteMeta.MetaData meta,
            NoteFile.ParsedNoteFile parsedFile,
            Map<String, NoteRecord> subNotes,
            boolean isSingleFile,
            boolean isExtensionFolder
    ) {}

    /**
     * 전체 훑기 결과 DTO.
     */
    public record ScanResult(
            Map<String, NoteRecord> notesById,
            List<Map<String, String>> skipped,
            List<Map<String, String>> conflicts,
            List<String> unsharded
    ) {}

    private static class ScanHaltedException extends RuntimeException {}

    private NoteScanner() {}

    public static ScanResult scan(Path vaultRoot, AgendaTable agendaTable) {
        return scan(vaultRoot, agendaTable, true);
    }

    /**
     * vaultRoot의 notes/ 디렉토리를 훑어 전체 노트를 수집합니다.
     * @param vaultRoot vault 루트 절대 경로
     * @param agendaTable 사전에 스캔된 아젠다 테이블
     * @param haltOnFatal 치명적 파싱 실패 시 예외를 던질지 여부 (reindex=true, 읽기 명령=false)
     * @return 훑기 결과 ScanResult
     */
    public static ScanResult scan(Path vaultRoot, AgendaTable agendaTable, boolean haltOnFatal) {
        Collector collector = new Collector(vaultRoot);
        collector.collectAll();
        return collector.open(agendaTable, haltOnFatal);
    }

    /** 8.5: 받은 자리만 먼저 모은 뒤 연다. 경로 검사 실패 전에는 노트 파일을 열지 않는다. */
    public static ScanResult scanPaths(Path vaultRoot, AgendaTable agendas, List<String> paths) {
        Collector collector = new Collector(vaultRoot);
        Set<String> seen = new LinkedHashSet<>();
        for (String raw : paths) {
            String path = Trim.trim(raw);
            int first = 0, last = path.length();
            while (first < last && path.charAt(first) == '/') first++;
            while (last > first && path.charAt(last - 1) == '/') last--;
            path = path.substring(first, last);
            if (path.startsWith("notes/") && seen.add(path)) collector.collectPath(path);
        }
        return collector.open(agendas, true);
    }

    private record Candidate(Path path, Path body, String agenda, boolean single) {}

    /** 전체 훑기와 지정 경로 훑기가 같은 1.2 후보 판정·깊이·순서를 쓴다. */
    private static final class Collector {
        final Path vault;
        final List<Map<String, String>> skipped = new ArrayList<>();
        final List<Map<String, String>> conflicts = new ArrayList<>();
        final Map<Path, Candidate> candidates = new LinkedHashMap<>();

        Collector(Path vault) { this.vault = vault.toAbsolutePath().normalize(); }

        void collectAll() {
            Path notes = vault.resolve("notes");
            if (!Files.isDirectory(notes)) return;
            List<Path> dirs = new ArrayList<>();
            try (var entries = Files.newDirectoryStream(notes)) {
                for (Path entry : entries) if (Files.isDirectory(entry)) dirs.add(entry);
            } catch (IOException e) {
                throw new WlException(500, "노트 폴더를 읽지 못했다: " + notes);
            }
            dirs.sort((a,b) -> Utf16.compare(a.getFileName().toString(), b.getFileName().toString()));
            for (Path dir : dirs) collectFolder(dir, dir.getFileName().toString(), 0);
        }

        void collectPath(String raw) {
            List<String> pieces = Arrays.stream(raw.split("/")).filter(s -> !Trim.isBlank(s)).toList();
            if (pieces.size() < 2) return;
            String agenda = pieces.get(1);
            PathConfinement.checkSegment(vault, agenda);
            Path current = vault.resolve("notes").resolve(agenda).normalize();
            // 두 번째 조각의 기준만 vault다. notes/..은 vault 자신이라 통과한다(8.5/8.7).
            PathConfinement.checkConfinement(vault, current);
            if (!Files.isDirectory(current)) return;
            int depth = 0;
            for (int i = 2; i < pieces.size(); i++) {
                String piece = pieces.get(i);
                PathConfinement.checkSegment(current, piece);
                Path next = current.resolve(piece).normalize();
                PathConfinement.checkConfinement(current, next);
                if (!Files.exists(next)) return;
                String name = next.getFileName().toString();
                boolean ordinary = Files.isDirectory(next) && !Files.isSymbolicLink(next)
                        && !RESERVED_NAMES.contains(name) && !CONFLICT_PATTERN.matcher(name).find()
                        && findFolderContentFile(next) == null && depth < 8;
                if (!ordinary) { collectEntry(next, agenda, depth); return; }
                current = next;
                depth++;
            }
            collectFolder(current, agenda, depth);
        }

        void collectFolder(Path dir, String agenda, int depth) {
            List<Path> entries = new ArrayList<>();
            try (var stream = Files.newDirectoryStream(dir)) { for (Path entry : stream) entries.add(entry); }
            catch (IOException e) { return; }
            entries.sort((a,b) -> Utf16.compare(a.getFileName().toString(), b.getFileName().toString()));
            for (Path entry : entries) collectEntry(entry, agenda, depth);
        }

        void collectEntry(Path entry, String agenda, int depth) {
            String name = entry.getFileName().toString();
            String relative = vault.relativize(entry).toString().replace('\\', '/');
            if (RESERVED_NAMES.contains(name)) return;
            if (Files.isSymbolicLink(entry)) { addSkipped(skipped, relative, "심링크라 따라가지 않았다"); return; }
            if (CONFLICT_PATTERN.matcher(name).find()) {
                Map<String, String> row = new LinkedHashMap<>(); row.put("kind", "note"); row.put("path", relative);
                conflicts.add(row); return;
            }
            if (Files.isDirectory(entry)) {
                Path body = findFolderContentFile(entry);
                if (body != null) candidates.putIfAbsent(entry, new Candidate(entry, body, agenda, false));
                else if (depth < 8) collectFolder(entry, agenda, depth + 1);
                else addSkipped(skipped, relative, "스캔 깊이 상한을 넘었다");
            } else if (Files.isRegularFile(entry) && isSingleFileNoteName(name)) {
                candidates.putIfAbsent(entry, new Candidate(entry, entry, agenda, true));
            }
        }

        ScanResult open(AgendaTable agendas, boolean haltOnFatal) {
            Map<String, NoteRecord> notes = new LinkedHashMap<>();
            List<Map<String, String>> openingSkipped = new ArrayList<>();
            try {
                for (Candidate c : candidates.values()) processRootCandidate(vault, c.path(), c.body(), c.agenda(), c.single(),
                        agendas, notes, openingSkipped, haltOnFatal);
            } catch (ScanHaltedException ignored) { /* 새 색인의 첫 실패 앞까지만 남긴다(8.6). */ }
            Set<String> commentIds = new HashSet<>();
            List<String> ids = new ArrayList<>(notes.keySet());
            for (int i = 0; i < ids.size(); i++) {
                try { CommentCopy.check(vault, notes.get(ids.get(i)), commentIds); }
                catch (RuntimeException e) {
                    if (haltOnFatal) throw e;
                    for (int j = i; j < ids.size(); j++) notes.remove(ids.get(j));
                    break;
                }
            }
            List<Map<String, String>> allSkipped = new ArrayList<>(skipped); allSkipped.addAll(openingSkipped);
            List<String> unsharded = new ArrayList<>();
            if (!candidates.isEmpty() && ShardLayout.isSharded(vault)) {
                for (Path path : candidates.keySet()) {
                    Path rel = vault.relativize(path);
                    if (rel.getNameCount() != 4 || !rel.getName(2).toString().matches("[0-9]{4}-(0[1-9]|1[0-2])"))
                        unsharded.add(rel.toString().replace('\\', '/'));
                }
                unsharded.sort(Utf16::compare);
            }
            return new ScanResult(notes, allSkipped, conflicts, unsharded);
        }
    }

    private static Path findFolderContentFile(Path folder) {
        Path md = null;
        Path json = null;
        Path html = null;
        try (DirectoryStream<Path> ds = Files.newDirectoryStream(folder)) {
            for (Path p : ds) {
                if (!Files.isRegularFile(p)) continue;
                String fn = p.getFileName().toString();
                if ("note.md".equals(fn)) {
                    md = p;
                } else if ("note.json".equals(fn)) {
                    json = p;
                } else if ("note.html".equals(fn)) {
                    html = p;
                }
            }
        } catch (IOException e) {
            return null;
        }
        if (md != null) return md;
        if (json != null) return json;
        if (html != null) return html;
        return null;
    }

    private static boolean isSingleFileNoteName(String name) {
        if ("comments.json".equals(name) || "ask.json".equals(name)) {
            return false;
        }
        return name.endsWith(".md") || name.endsWith(".html") || name.endsWith(".json");
    }

    private static String determineFormat(String name) {
        if (name.endsWith(".md")) return "markdown";
        if (name.endsWith(".json")) return "json";
        if (name.endsWith(".html")) return "html";
        return "markdown";
    }

    private static void processRootCandidate(
            Path vaultRoot,
            Path candidatePath,
            Path contentFilePath,
            String folderAgendaSlug,
            boolean isSingleFile,
            AgendaTable agendaTable,
            Map<String, NoteRecord> notesById,
            List<Map<String, String>> skipped,
            boolean haltOnFatal
    ) {
        String relPath = vaultRoot.relativize(candidatePath).toString().replace('\\', '/');
        String format = determineFormat(contentFilePath.getFileName().toString());

        // 4. 본문 파일 읽기
        NoteFile.ParsedNoteFile parsed;
        try {
            parsed = NoteFile.parse(contentFilePath);
        } catch (IOException e) {
            addSkipped(skipped, relPath, "본문 파일을 못 읽었다");
            return;
        } catch (RuntimeException e) {
            if (haltOnFatal) {
                throw e;
            } else {
                throw new ScanHaltedException();
            }
        }

        // 5. 프론트매터 검증
        if (!parsed.hasFrontmatter() || parsed.frontmatter() == null) {
            addSkipped(skipped, relPath, "프론트매터가 없다");
            return;
        }

        // 6, 7, 8 메타데이터 검증
        NoteMeta.MetaData meta;
        try {
            meta = NoteMeta.parseMeta(parsed.frontmatter(), folderAgendaSlug, agendaTable, false);
            if (meta == null) {
                addSkipped(skipped, relPath, "노트 id 가 없거나 모양이 어긋난다");
                return;
            }
        } catch (IllegalArgumentException e) {
            String msg = e.getMessage();
            if (msg.contains("created 가 달력에 없는 날짜다")) {
                addSkipped(skipped, relPath, "created 가 달력에 없는 날짜다");
            } else if (msg.contains("아젠다 slug 를 식별자로 못 바꾼다")) {
                addSkipped(skipped, relPath, msg);
            } else {
                addSkipped(skipped, relPath, "노트 id 가 없거나 모양이 어긋난다");
            }
            return;
        } catch (RuntimeException e) {
            if (haltOnFatal) {
                throw e;
            } else {
                throw new ScanHaltedException();
            }
        }

        // 9. 중복 id 검증
        if (notesById.containsKey(meta.id())) {
            addSkipped(skipped, relPath, "이미 읽은 노트가 같은 id 를 쓴다: " + meta.id());
            return;
        }

        // json 노트 및 확장자 폴더 노트 특이점 (1.13): 본문 빈 글, 토큰/도장/sessionId 없음
        String candidateName = candidatePath.getFileName().toString();
        boolean isExtensionFolder = !isSingleFile && (
                candidateName.endsWith(".md") || candidateName.endsWith(".html") || candidateName.endsWith(".json")
        );

        NoteFile.ParsedNoteFile effectiveParsed = parsed;
        NoteMeta.MetaData effectiveMeta = meta;
        if ("json".equals(format) || isExtensionFolder) {
            effectiveParsed = new NoteFile.ParsedNoteFile(parsed.frontmatter(), "", null, true);
            effectiveMeta = new NoteMeta.MetaData(
                    meta.id(), meta.agendaSlug(), meta.type(), meta.title(),
                    meta.created(), meta.date(), meta.time(),
                    null, meta.mins(), meta.tier(),
                    null, null, null,
                    meta.tags(), meta.links(), meta.favorite(), meta.favoriteCategory(),
                    meta.template(), meta.evaluation()
            );
        }

        Map<String, NoteRecord> subNotes = new LinkedHashMap<>();
        NoteRecord rootRecord = new NoteRecord(
                meta.id(), false, meta.id(), null, null,
                relPath, format, contentFilePath, effectiveMeta, effectiveParsed, subNotes,
                isSingleFile, isExtensionFolder
        );

        notesById.put(meta.id(), rootRecord);

        // 하위 노트 스캔 (단일 파일 노트가 아닌 경우)
        if (!isSingleFile) {
            Path subDir = candidatePath.resolve("sub");
            if (Files.isDirectory(subDir)) {
                try {
                    scanSubNotes(vaultRoot, candidatePath, meta.id(), meta.id(), folderAgendaSlug, 1, subNotes, skipped, haltOnFatal);
                } catch (ScanHaltedException e) {
                    // 8.4: 하위 노트에서 서면 그 메인 노트부터 노트가 아니다(1.15의 6번, mins: .nan 같은 것)
                    notesById.remove(meta.id());
                    throw e;
                }
            }
        }
    }

    private static void scanSubNotes(
            Path vaultRoot,
            Path containerDir,
            String rootId,
            String directParentId,
            String folderAgendaSlug,
            int nestLevel,
            Map<String, NoteRecord> subNotes,
            List<Map<String, String>> skipped,
            boolean haltOnFatal
    ) {
        if (nestLevel > 8) {
            // 1.12 1123행: 9겹째부터는 이유 문구 없이 조용히 안 본다
            return;
        }

        Path subDir = containerDir.resolve("sub");
        if (!Files.isDirectory(subDir)) {
            return;
        }

        List<Path> entries = new ArrayList<>();
        try (DirectoryStream<Path> ds = Files.newDirectoryStream(subDir)) {
            for (Path p : ds) {
                // 심볼릭 링크 항목은 조용히 무시 (1.12)
                if (Files.isSymbolicLink(p)) {
                    continue;
                }
                entries.add(p);
            }
        } catch (IOException e) {
            return;
        }

        entries.sort((a, b) -> Utf16.compare(a.getFileName().toString(), b.getFileName().toString()));

        for (Path entry : entries) {
            String name = entry.getFileName().toString();
            String relPath = vaultRoot.relativize(entry).toString().replace('\\', '/');

            if (RESERVED_NAMES.contains(name) || CONFLICT_PATTERN.matcher(name).find()) {
                continue;
            }

            Path contentFile = null;
            boolean isFolder = false;

            if (Files.isDirectory(entry)) {
                contentFile = findFolderContentFile(entry);
                if (contentFile == null) {
                    // 본문 파일 없는 폴더는 하위 노트 아니고 아래로도 안 내려감
                    continue;
                }
                isFolder = true;
            } else if (Files.isRegularFile(entry)) {
                if (nestLevel == 1 && name.endsWith(".md")) {
                    contentFile = entry;
                } else {
                    continue;
                }
            } else {
                continue;
            }

            // 하위 노트 파싱
            NoteFile.ParsedNoteFile parsed;
            try {
                parsed = NoteFile.parse(contentFile);
            } catch (IOException e) {
                addSkipped(skipped, relPath, "본문 파일을 못 읽었다");
                skipDescendants(vaultRoot, entry, skipped, "직계 부모 하위 노트를 못 읽어 이 하위 노트도 건너뛴다");
                continue;
            } catch (RuntimeException e) {
                if (haltOnFatal) {
                    throw e;
                } else {
                    throw new ScanHaltedException();
                }
            }

            if (!parsed.hasFrontmatter() || parsed.frontmatter() == null) {
                addSkipped(skipped, relPath, "프론트매터가 없다");
                skipDescendants(vaultRoot, entry, skipped, "직계 부모 하위 노트를 못 읽어 이 하위 노트도 건너뛴다");
                continue;
            }

            NoteMeta.MetaData meta;
            try {
                meta = NoteMeta.parseMeta(parsed.frontmatter(), folderAgendaSlug, null, true, rootId);
                if (meta == null) {
                    addSkipped(skipped, relPath, "하위 노트 id 가 없거나 모양이 어긋난다");
                    skipDescendants(vaultRoot, entry, skipped, "직계 부모 하위 노트를 못 읽어 이 하위 노트도 건너뛴다");
                    continue;
                }
            } catch (IllegalArgumentException e) {
                String msg = e.getMessage();
                if (msg.contains("created 가 달력에 없는 날짜다")) {
                    addSkipped(skipped, relPath, "created 가 달력에 없는 날짜다");
                } else {
                    addSkipped(skipped, relPath, "하위 노트 id 가 없거나 모양이 어긋난다");
                }
                skipDescendants(vaultRoot, entry, skipped, "직계 부모 하위 노트를 못 읽어 이 하위 노트도 건너뛴다");
                continue;
            } catch (RuntimeException e) {
                if (haltOnFatal) {
                    throw e;
                } else {
                    throw new ScanHaltedException();
                }
            }

            if (subNotes.containsKey(meta.id())) {
                addSkipped(skipped, relPath, "이미 읽은 노트가 같은 id 를 쓴다: " + meta.id());
                skipDescendants(vaultRoot, entry, skipped, "직계 부모 하위 노트를 못 읽어 이 하위 노트도 건너뛴다");
                continue;
            }

            String format = determineFormat(contentFile.getFileName().toString());
            NoteRecord subRecord = new NoteRecord(
                    meta.id(), true, rootId, directParentId, null,
                    relPath, format, contentFile, meta, parsed, new LinkedHashMap<>(),
                    !isFolder, false
            );

            subNotes.put(meta.id(), subRecord);

            if (isFolder) {
                scanSubNotes(vaultRoot, entry, rootId, meta.id(), folderAgendaSlug, nestLevel + 1, subNotes, skipped, haltOnFatal);
            }
        }
    }

    private static void skipDescendants(Path vaultRoot, Path container, List<Map<String, String>> skipped, String reason) {
        Path sub = container.resolve("sub");
        if (!Files.isDirectory(sub)) return;
        List<Path> entries = new ArrayList<>();
        try (DirectoryStream<Path> ds = Files.newDirectoryStream(sub)) {
            for (Path p : ds) {
                if (!Files.isSymbolicLink(p)) {
                    entries.add(p);
                }
            }
        } catch (IOException e) {
            return;
        }
        entries.sort((a, b) -> Utf16.compare(a.getFileName().toString(), b.getFileName().toString()));
        for (Path p : entries) {
            String rel = vaultRoot.relativize(p).toString().replace('\\', '/');
            addSkipped(skipped, rel, reason);
            if (Files.isDirectory(p)) {
                skipDescendants(vaultRoot, p, skipped, reason);
            }
        }
    }

    private static void addSkipped(List<Map<String, String>> skipped, String path, String reason) {
        Map<String, String> map = new LinkedHashMap<>();
        map.put("kind", "note");
        map.put("path", path);
        map.put("reason", reason);
        skipped.add(map);
    }
}
