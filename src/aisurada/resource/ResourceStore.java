package aisurada.resource;

import aisurada.args.JsonReader;
import aisurada.args.JsonValue;
import aisurada.cli.AisException;
import aisurada.vault.ProjectReader;
import aisurada.vault.ProjectRepo;
import aisurada.write.AtomicWriter;
import aisurada.write.DocToken;
import aisurada.write.IdGenerator;
import aisurada.write.TestApparatus;
import aisurada.write.TreeRemover;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.DirectoryStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Set;

/**
 * 자원 파일을 vault에서 읽고 쓰는 곳. <b>외부 설명서에 없는 확장</b>입니다.
 *
 * <pre>
 * .aisurada/resources/
 *   global/{skills/&lt;slug&gt;/SKILL.md, agents/&lt;slug&gt;.md}     글로벌 자원
 *   projects/&lt;프로젝트 slug&gt;/{skills/…, agents/…, refs.md}   프로젝트 자원과 참조 목록
 *   .trash/&lt;id&gt;/   지운 자원      .history/&lt;id&gt;/   고치기 전의 판
 * </pre>
 *
 * <p>설명서가 정한 {@code notes/}와 {@code .aisurada/projects/} 등을 건드리지 않는 새 영역입니다. 자원의 범위는 파일 위치로 정해지고,
 * 이름을 바꿔도 {@code id}는 머리말 안에 있어서 참조가 끊기지 않습니다.
 */
public final class ResourceStore {

    public static final String ROOT = ".aisurada/resources";
    public static final int HISTORY_KEEP = 30;

    private ResourceStore() {}

    /** 훑은 결과. {@code skipped}는 못 읽어 건너뛴 파일과 이유다. */
    public record Scan(List<Resource> items, List<String> skipped) {
        public Optional<Resource> byId(String id) {
            for (Resource r : items) {
                if (r.id().equals(id)) return Optional.of(r);
            }
            return Optional.empty();
        }
    }

    /**
     * 고칠 값. null은 「안 줌(그대로 둔다)」이다. 글 칸의 빈 값은 {@code repo}만 「지운다」이고, {@code skills}와 {@code tools}는 빈 목록·빈 맵이 「비운다」이다.
     */
    public record Fields(String name, String description, String body, String permission, List<String> skills, String repo,
                         Map<String, Object> tools) {}

    /** 누가 언제 고쳤는지의 기록. 사람이 고쳤으면 session은 null이다. */
    public record Stamp(String at, String session, String agent) {}

    /** 지운 자원 한 줄. */
    public record TrashEntry(String id, ResourceKind kind, String project, String name, String relPath, String deletedAt) {}

    /** 고치기 전의 판 한 줄. 새것이 앞이다. */
    public record Version(String version, long savedAt, int bytes, String editedAt, String editedSession) {}

    // ------------------------------------------------------------------ 경로

    public static Path root(Path vault) {
        return vault.resolve(ROOT);
    }

    /** 범위 폴더. project가 null이면 글로벌이다. */
    public static Path scopeDir(Path vault, String project) {
        return project == null ? root(vault).resolve("global") : root(vault).resolve("projects").resolve(project);
    }

    private static Path entryPath(Path vault, ResourceKind kind, String project, String slug) {
        Path dir = scopeDir(vault, project).resolve(kind.dir());
        return kind == ResourceKind.SKILL ? dir.resolve(slug).resolve("SKILL.md") : dir.resolve(slug + ".md");
    }

    /** 자원 파일의 폴더·파일 이름(slug). 스킬은 폴더 이름, 에이전트는 파일 이름이다. */
    public static String slugOf(Resource r) {
        String rel = r.relPath();
        if (r.kind() == ResourceKind.SKILL) {
            String parent = rel.substring(0, rel.lastIndexOf('/'));
            return parent.substring(parent.lastIndexOf('/') + 1);
        }
        String file = rel.substring(rel.lastIndexOf('/') + 1);
        return file.substring(0, file.length() - 3);
    }

    // ------------------------------------------------------------------ 읽기

    /** 모든 범위의 자원을 훑는다. 못 읽는 파일은 건너뛰고 이유를 {@code skipped}에 적는다. 점으로 시작하는 폴더({@code .trash} 등)는 안 본다. */
    public static Scan scan(Path vault) {
        List<Resource> items = new ArrayList<>();
        List<String> skipped = new ArrayList<>();
        Set<String> ids = new HashSet<>();
        scanScope(vault, null, scopeDir(vault, null), items, skipped, ids);
        Path projects = root(vault).resolve("projects");
        for (Path dir : sortedEntries(projects)) {
            String name = dir.getFileName().toString();
            if (name.startsWith(".") || !Files.isDirectory(dir)) continue;
            scanScope(vault, name, dir, items, skipped, ids);
        }
        return new Scan(List.copyOf(items), List.copyOf(skipped));
    }

    private static void scanScope(Path vault, String project, Path scope, List<Resource> items, List<String> skipped, Set<String> ids) {
        for (ResourceKind kind : ResourceKind.values()) {
            Path kindDir = scope.resolve(kind.dir());
            for (Path entry : sortedEntries(kindDir)) {
                String fileName = entry.getFileName().toString();
                if (fileName.startsWith(".")) continue;
                Path file;
                if (kind == ResourceKind.SKILL) {
                    if (!Files.isDirectory(entry)) continue;
                    file = entry.resolve("SKILL.md");
                } else {
                    if (!fileName.endsWith(".md")) continue;
                    file = entry;
                }
                if (!Files.isRegularFile(file)) continue;
                String rel = ResourceFormat.rel(vault, file);
                try {
                    if (Files.size(file) > ResourceFormat.MAX_FILE_BYTES) {
                        skipped.add(rel + ": 파일이 너무 크다");
                        continue;
                    }
                    Resource r = ResourceFormat.parse(Files.readAllBytes(file), kind, project, rel);
                    if (!ids.add(r.id())) {
                        skipped.add(rel + ": id 가 다른 자원과 겹친다: " + r.id());
                        continue;
                    }
                    items.add(r);
                } catch (IllegalArgumentException | IOException e) {
                    skipped.add(rel + ": " + e.getMessage());
                }
            }
        }
    }

    private static List<Path> sortedEntries(Path dir) {
        List<Path> out = new ArrayList<>();
        if (!Files.isDirectory(dir)) return out;
        try (DirectoryStream<Path> ds = Files.newDirectoryStream(dir)) {
            for (Path p : ds) out.add(p);
        } catch (IOException e) {
            return out;
        }
        out.sort(Comparator.comparing(p -> p.getFileName().toString()));
        return out;
    }

    /** ID로 자원 하나를 찾는다. 없으면 AIS-404다. */
    public static Resource find(Path vault, String id) {
        if (!ResourceKind.isId(id)) {
            throw new AisException(400, "자원 id 모양이 아니다(sk… 또는 ap…): " + id);
        }
        return scan(vault).byId(id).orElseThrow(() -> new AisException(404, "자원을 못 찾았다: " + id));
    }

    /** 프로젝트 slug가 실제 프로젝트인지 확인한다. 아니면 AIS-404다. */
    public static ProjectReader.ProjectInfo requireProject(Path vault, String slug) {
        if (slug == null || slug.isBlank()) {
            throw new AisException(400, "프로젝트 slug 가 필요하다");
        }
        for (ProjectReader.ProjectInfo p : ProjectReader.scanProjects(vault).values()) {
            if (p.slug().equals(slug)) return p;
        }
        throw new AisException(404, "프로젝트를 못 찾았다: " + slug);
    }

    // ------------------------------------------------------------------ 만들기

    /** 새 자원을 만든다. name과 description은 필수다. */
    public static Resource create(Path vault, ResourceKind kind, String project, Fields f, Stamp stamp) {
        if (project != null) requireProject(vault, project);
        if (f.name() == null) throw new AisException(400, "name 이 필요하다");
        if (f.description() == null) throw new AisException(400, "description 이 필요하다");
        Scan scan = scan(vault);
        String slug = ResourceFormat.slugOf(f.name());
        ensureFree(scan, kind, project, slug, null);

        String permission = null;
        List<String> skills = List.of();
        String repo = null;
        if (kind == ResourceKind.AGENT) {
            permission = f.permission() == null ? "read" : f.permission();
            skills = f.skills() == null ? List.of() : f.skills();
            repo = f.repo() == null || f.repo().isEmpty() ? null : f.repo();
            validateAgentLinks(vault, scan, project, skills, repo);
        }
        String id;
        do {
            id = IdGenerator.nextId(kind.idPrefix());
        } while (scan.byId(id).isPresent());

        Path path = entryPath(vault, kind, project, slug);
        Resource r = new Resource(id, kind, project, f.name(), f.description(), permission, skills, repo,
                f.tools() == null ? Map.of() : f.tools(), f.body() == null ? "" : f.body(),
                stamp.at(), stamp.session(), stamp.agent(), ResourceFormat.rel(vault, path), null);
        byte[] bytes = ResourceFormat.render(r);
        AtomicWriter.writeAtomic(path, bytes);
        return r.withToken(DocToken.compute(bytes));
    }

    // ------------------------------------------------------------------ 고치기

    /**
     * 자원을 고친다. 직전에 읽은 {@code f1} 토큰이 지금 파일과 맞아야 쓴다(맞지 않으면 AIS-412). 실제로 달라지는 게 없으면 파일도 기록도 안 바꾼다.
     * 고치기 전의 판은 이력에 남긴다. 종류, 범위, id는 바꿀 수 없다.
     */
    public static Resource update(Path vault, Resource cur, Fields f, String f1, Stamp stamp) {
        if (f1 == null || f1.isBlank()) {
            throw new AisException(412, "f1 이 필요하다. get-resource 로 읽은 precondition 을 f1= 으로 넘긴다: " + cur.relPath());
        }
        if (!f1.trim().equals(cur.token())) {
            throw new AisException(412, "읽은 뒤에 파일이 바뀌었다. 다시 읽고 고친다: " + cur.relPath());
        }
        Scan scan = scan(vault);
        String name = f.name() != null ? f.name() : cur.name();
        String description = f.description() != null ? f.description() : cur.description();
        String body = f.body() != null ? f.body() : cur.body();
        Map<String, Object> tools = f.tools() != null ? f.tools() : cur.tools();

        String permission = cur.permission();
        List<String> skills = cur.skills();
        String repo = cur.repo();
        if (cur.kind() == ResourceKind.AGENT) {
            if (f.permission() != null) permission = f.permission();
            if (f.skills() != null) skills = f.skills();
            if (f.repo() != null) repo = f.repo().isEmpty() ? null : f.repo();
            if (f.skills() != null || f.repo() != null) validateAgentLinks(vault, scan, cur.project(), skills, repo);
        }

        String oldSlug = slugOf(cur);
        String newSlug = ResourceFormat.slugOf(name);
        if (!newSlug.equals(oldSlug)) ensureFree(scan, cur.kind(), cur.project(), newSlug, cur.id());

        Resource same = new Resource(cur.id(), cur.kind(), cur.project(), name, description, permission, skills, repo, tools, body,
                cur.editedAt(), cur.editedSession(), cur.editedAgent(), cur.relPath(), cur.token());
        if (newSlug.equals(oldSlug) && java.util.Arrays.equals(ResourceFormat.render(same), ResourceFormat.render(cur))) {
            return cur;
        }

        Path oldPath = vault.resolve(cur.relPath());
        byte[] oldBytes;
        try {
            oldBytes = Files.readAllBytes(oldPath);
        } catch (IOException e) {
            throw new AisException(500, "자원 파일을 못 읽었다: " + cur.relPath());
        }
        snapshot(vault, cur.id(), oldBytes);

        Path newPath = entryPath(vault, cur.kind(), cur.project(), newSlug);
        if (!newSlug.equals(oldSlug)) {
            try {
                if (cur.kind() == ResourceKind.SKILL) {
                    Files.move(oldPath.getParent(), newPath.getParent());
                } else {
                    Files.move(oldPath, newPath);
                }
            } catch (IOException e) {
                throw new AisException(500, "이름을 바꾸며 옮기지 못했다: " + cur.relPath());
            }
        }
        Resource next = new Resource(cur.id(), cur.kind(), cur.project(), name, description, permission, skills, repo, tools, body,
                stamp.at(), stamp.session(), stamp.agent(), ResourceFormat.rel(vault, newPath), null);
        byte[] bytes = ResourceFormat.render(next);
        AtomicWriter.writeAtomic(newPath, bytes);
        return next.withToken(DocToken.compute(bytes));
    }

    private static void ensureFree(Scan scan, ResourceKind kind, String project, String slug, String exceptId) {
        for (Resource r : scan.items()) {
            if (r.kind() != kind || !java.util.Objects.equals(r.project(), project) || r.id().equals(exceptId)) continue;
            if (slugOf(r).equals(slug)) {
                throw new AisException(400, "같은 이름의 " + (kind == ResourceKind.SKILL ? "스킬" : "에이전트") + "이 이 범위에 이미 있다: " + r.name());
            }
        }
    }

    /**
     * 에이전트가 쓸 스킬과 저장소를 검사한다. 글로벌 에이전트는 글로벌 스킬만 쓸 수 있고(프로젝트 것에 기대면 다른 프로젝트에서 깨진다),
     * 프로젝트 에이전트는 글로벌 스킬과 자기 프로젝트의 스킬을 쓸 수 있다. 저장소는 프로젝트 에이전트만 정하고, 프로젝트의 저장소 이름이어야 한다.
     */
    private static void validateAgentLinks(Path vault, Scan scan, String project, List<String> skills, String repo) {
        for (String id : skills) {
            Optional<Resource> s = scan.byId(id);
            boolean ok = s.isPresent() && s.get().kind() == ResourceKind.SKILL
                    && (s.get().isGlobal() || (project != null && project.equals(s.get().project())));
            if (!ok) {
                throw new AisException(400, "skills 의 " + id + " 는 이 에이전트가 쓸 수 있는 스킬이 아니다"
                        + (project == null ? "(글로벌 에이전트는 글로벌 스킬만 쓴다)" : "(없거나 다른 프로젝트의 것이다)"));
            }
        }
        if (repo != null) {
            if (project == null) {
                throw new AisException(400, "글로벌 에이전트에는 repo 를 줄 수 없다. 저장소는 작업을 시킬 때 프로젝트의 것을 고른다");
            }
            ProjectReader.ProjectInfo p = requireProject(vault, project);
            List<String> names = p.repos().stream().map(ProjectRepo::name).toList();
            if (!names.contains(repo)) {
                throw new AisException(400, "repo 는 프로젝트의 저장소 이름이어야 한다: " + repo + ". 있는 이름: " + names);
            }
        }
    }

    // ------------------------------------------------------------------ 지우기·되살리기

    /** 자원을 휴지통으로 보낸다(스킬은 폴더째). 되살릴 수 있다. */
    public static void delete(Path vault, Resource r) {
        Path trash = root(vault).resolve(".trash").resolve(r.id());
        if (Files.exists(trash)) {
            throw new AisException(409, "휴지통에 같은 id 가 이미 있다: " + r.id());
        }
        Path src = vault.resolve(r.relPath());
        Path item = trash.resolve(r.kind() == ResourceKind.SKILL ? "item" : "item.md");
        try {
            Files.createDirectories(trash);
            Files.move(r.kind() == ResourceKind.SKILL ? src.getParent() : src, item);
        } catch (IOException e) {
            throw new AisException(500, "휴지통으로 옮기지 못했다: " + r.relPath());
        }
        String meta = "{\"id\":" + q(r.id()) + ",\"kind\":" + q(r.kind().label()) + ",\"project\":" + (r.project() == null ? "null" : q(r.project()))
                + ",\"name\":" + q(r.name()) + ",\"relPath\":" + q(r.relPath()) + ",\"deletedAt\":" + q(java.time.Instant.ofEpochMilli(TestApparatus.nowMillis()).toString()) + "}\n";
        AtomicWriter.writeAtomic(trash.resolve("meta.json"), meta.getBytes(StandardCharsets.UTF_8));
    }

    /** 휴지통의 자원을 되살린다. 제자리에 같은 이름이 이미 있으면 AIS-409다. */
    public static Resource restore(Path vault, String id) {
        Path trash = root(vault).resolve(".trash").resolve(id);
        TrashEntry e = readTrashEntry(trash);
        if (e == null) {
            throw new AisException(404, "휴지통에서 못 찾았다: " + id);
        }
        Path target = vault.resolve(e.relPath());
        Path destination = e.kind() == ResourceKind.SKILL ? target.getParent() : target;
        if (Files.exists(destination)) {
            throw new AisException(409, "제자리에 같은 이름이 이미 있어 되살리지 못했다. 그 이름을 바꾸거나 지운 뒤 다시 한다: " + e.relPath());
        }
        Path item = trash.resolve(e.kind() == ResourceKind.SKILL ? "item" : "item.md");
        try {
            Files.createDirectories(destination.getParent());
            Files.move(item, destination);
        } catch (IOException ex) {
            throw new AisException(500, "되살리지 못했다: " + e.relPath());
        }
        TreeRemover.remove(trash);
        return find(vault, id);
    }

    /** 휴지통 목록. 지운 시각이 늦은 것이 앞이다. */
    public static List<TrashEntry> trash(Path vault) {
        List<TrashEntry> out = new ArrayList<>();
        for (Path dir : sortedEntries(root(vault).resolve(".trash"))) {
            TrashEntry e = readTrashEntry(dir);
            if (e != null) out.add(e);
        }
        out.sort(Comparator.comparing(TrashEntry::deletedAt).reversed());
        return out;
    }

    private static TrashEntry readTrashEntry(Path dir) {
        Path meta = dir.resolve("meta.json");
        if (!Files.isRegularFile(meta)) return null;
        try {
            JsonValue v = JsonReader.parse(Files.readString(meta, StandardCharsets.UTF_8));
            if (!(v instanceof JsonValue.JsonObject o)) return null;
            String id = jsonStr(o, "id");
            ResourceKind kind = ResourceKind.parse(jsonStr(o, "kind"));
            String relPath = jsonStr(o, "relPath");
            if (id == null || kind == null || relPath == null || ResourceKind.ofId(id) != kind) return null;
            return new TrashEntry(id, kind, jsonStr(o, "project"), jsonStr(o, "name"), relPath, String.valueOf(jsonStr(o, "deletedAt")));
        } catch (RuntimeException | IOException e) {
            return null;
        }
    }

    private static String jsonStr(JsonValue.JsonObject o, String key) {
        JsonValue v = o.properties().get(key);
        return v instanceof JsonValue.JsonString s ? s.value() : null;
    }

    private static String q(String s) {
        StringBuilder sb = new StringBuilder("\"");
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"' -> sb.append("\\\"");
                case '\\' -> sb.append("\\\\");
                case '\n' -> sb.append("\\n");
                case '\r' -> sb.append("\\r");
                case '\t' -> sb.append("\\t");
                default -> {
                    if (c < 0x20) sb.append(String.format("\\u%04x", (int) c));
                    else sb.append(c);
                }
            }
        }
        return sb.append('"').toString();
    }

    // ------------------------------------------------------------------ 이력

    private static Path historyDir(Path vault, String id) {
        return root(vault).resolve(".history").resolve(id);
    }

    private static void snapshot(Path vault, String id, byte[] bytes) {
        Path dir = historyDir(vault, id);
        String base = Long.toString(TestApparatus.nowMillis(), 36);
        String name = base;
        int n = 1;
        while (Files.exists(dir.resolve(name + ".md"))) {
            name = base + "-" + n++;
        }
        AtomicWriter.writeAtomic(dir.resolve(name + ".md"), bytes);
        List<Path> all = sortedEntries(dir);
        all.sort(Comparator.comparing((Path p) -> p.getFileName().toString().length()).thenComparing(p -> p.getFileName().toString()));
        for (int i = 0; i < all.size() - HISTORY_KEEP; i++) {
            TreeRemover.remove(all.get(i));
        }
    }

    /** 고치기 전의 판 목록. 새것이 앞이다. */
    public static List<Version> versions(Path vault, Resource r) {
        List<Version> out = new ArrayList<>();
        List<Path> all = sortedEntries(historyDir(vault, r.id()));
        all.sort(Comparator.comparing((Path p) -> p.getFileName().toString().length()).thenComparing(p -> p.getFileName().toString()).reversed());
        for (Path p : all) {
            String file = p.getFileName().toString();
            if (!file.endsWith(".md")) continue;
            String version = file.substring(0, file.length() - 3);
            try {
                byte[] bytes = Files.readAllBytes(p);
                Resource old = ResourceFormat.parse(bytes, r.kind(), r.project(), r.relPath());
                long savedAt = Long.parseLong(version.contains("-") ? version.substring(0, version.indexOf('-')) : version, 36);
                out.add(new Version(version, savedAt, bytes.length, old.editedAt(), old.editedSession()));
            } catch (IllegalArgumentException | IOException e) {
                // 못 읽는 판은 목록에서 뺀다
            }
        }
        return out;
    }

    /** 이력의 한 판을 자원으로 읽는다. 없으면 AIS-404다. */
    public static Resource readVersion(Path vault, Resource r, String version) {
        if (version == null || !version.matches("[0-9a-z]+(-[0-9]+)?")) {
            throw new AisException(400, "version 모양이 아니다: " + version);
        }
        Path p = historyDir(vault, r.id()).resolve(version + ".md");
        if (!Files.isRegularFile(p)) {
            throw new AisException(404, "그 판을 못 찾았다: " + version);
        }
        try {
            byte[] bytes = Files.readAllBytes(p);
            return ResourceFormat.parse(bytes, r.kind(), r.project(), r.relPath());
        } catch (IllegalArgumentException | IOException e) {
            throw new AisException(500, "그 판을 못 읽었다: " + version);
        }
    }
}
