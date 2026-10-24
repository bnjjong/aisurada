package withlog.vault;

import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.text.Utf16;

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
        Path notesDir = vaultRoot.resolve("notes");
        List<Map<String, String>> skipped = new ArrayList<>();
        List<Map<String, String>> conflicts = new ArrayList<>();
        List<String> unsharded = new ArrayList<>();
        Map<String, NoteRecord> notesById = new LinkedHashMap<>();

        if (!Files.exists(notesDir)) {
            return new ScanResult(notesById, skipped, conflicts, unsharded);
        }
        if (!Files.isDirectory(notesDir)) {
            return new ScanResult(notesById, skipped, conflicts, unsharded);
        }

        List<Path> agendaDirs = new ArrayList<>();
        try (DirectoryStream<Path> ds = Files.newDirectoryStream(notesDir)) {
            for (Path p : ds) {
                if (Files.isDirectory(p)) {
                    agendaDirs.add(p);
                }
            }
        } catch (IOException e) {
            throw new WlException(500, "노트 폴더를 읽지 못했다: " + notesDir.toAbsolutePath().normalize());
        }

        agendaDirs.sort((a, b) -> Utf16.compare(a.getFileName().toString(), b.getFileName().toString()));

        try {
            for (Path agendaDir : agendaDirs) {
                String folderAgendaSlug = agendaDir.getFileName().toString();
                scanAgendaFolder(vaultRoot, agendaDir, folderAgendaSlug, 0, agendaTable, notesById, skipped, conflicts, haltOnFatal);
            }
        } catch (ScanHaltedException e) {
            // 읽기 명령에서는 비정상 노트 발생 시 훑기를 중단하고 이전까지 읽은 노트를 보존함 (설명서 8.6, 8.7 14번)
        }

        return new ScanResult(notesById, skipped, conflicts, unsharded);
    }

    private static void scanAgendaFolder(
            Path vaultRoot,
            Path dir,
            String folderAgendaSlug,
            int depth,
            AgendaTable agendaTable,
            Map<String, NoteRecord> notesById,
            List<Map<String, String>> skipped,
            List<Map<String, String>> conflicts,
            boolean haltOnFatal
    ) {
        List<Path> entries = new ArrayList<>();
        try (DirectoryStream<Path> ds = Files.newDirectoryStream(dir)) {
            for (Path p : ds) {
                entries.add(p);
            }
        } catch (IOException e) {
            // 1.2 550행: 목록을 못 읽은 하위 폴더는 빈 폴더로 본다
            return;
        }

        entries.sort((a, b) -> Utf16.compare(a.getFileName().toString(), b.getFileName().toString()));

        for (Path entry : entries) {
            String name = entry.getFileName().toString();
            String relPath = vaultRoot.relativize(entry).toString().replace('\\', '/');

            // 2. 예약 이름 건너뜀
            if (RESERVED_NAMES.contains(name)) {
                continue;
            }

            // 3. 심볼릭 링크 -> skipped
            if (Files.isSymbolicLink(entry)) {
                addSkipped(skipped, relPath, "심링크라 따라가지 않았다");
                continue;
            }

            // 4. 충돌 사본 모양 -> conflicts
            if (CONFLICT_PATTERN.matcher(name).find()) {
                Map<String, String> c = new LinkedHashMap<>();
                c.put("kind", "note");
                c.put("path", relPath);
                conflicts.add(c);
                continue;
            }

            if (Files.isDirectory(entry)) {
                // 본문 파일이 있는지 확인
                Path contentFile = findFolderContentFile(entry);
                if (contentFile != null) {
                    // 깊이 0~8 어디서든 노트로 적고 안으로 안 내려감 (1.2 5번)
                    processRootCandidate(vaultRoot, entry, contentFile, folderAgendaSlug, false, agendaTable, notesById, skipped, haltOnFatal);
                } else {
                    // 본문 파일이 없는 폴더
                    if (depth < 8) {
                        scanAgendaFolder(vaultRoot, entry, folderAgendaSlug, depth + 1, agendaTable, notesById, skipped, conflicts, haltOnFatal);
                    } else {
                        addSkipped(skipped, relPath, "스캔 깊이 상한을 넘었다");
                    }
                }
            } else if (Files.isRegularFile(entry)) {
                // 7. 일반 파일: 단일 파일 노트 이름일 때만 노트로 적고 깊이 검사 안 받음
                if (isSingleFileNoteName(name)) {
                    processRootCandidate(vaultRoot, entry, entry, folderAgendaSlug, true, agendaTable, notesById, skipped, haltOnFatal);
                }
            }
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
                scanSubNotes(vaultRoot, candidatePath, meta.id(), meta.id(), folderAgendaSlug, 1, subNotes, skipped, haltOnFatal);
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
