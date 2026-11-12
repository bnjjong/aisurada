package aisurada.cmd;

import aisurada.args.ParsedArgs;
import aisurada.cli.AisException;
import aisurada.engine.WritePipeline;
import aisurada.resource.Resource;
import aisurada.resource.ResourceFormat;
import aisurada.resource.ResourceKind;
import aisurada.resource.ResourceRefs;
import aisurada.resource.ResourceResolver;
import aisurada.resource.ResourceStore;
import aisurada.text.Trim;
import aisurada.write.TestApparatus;
import aisurada.write.WorkspaceMarker;

import java.nio.file.Path;
import java.time.OffsetDateTime;
import java.time.ZoneId;
import java.time.format.DateTimeFormatter;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * 스킬과 에이전트 정의(자원)를 다루는 명령 8개. <b>외부 설명서에 없는 확장</b>이고 지금은 명령줄 표면만 있다(MCP 도구는 아직 없다).
 *
 * <p>자원은 <b>글로벌</b>과 <b>프로젝트</b> 두 범위이고, 프로젝트는 글로벌 자원을 ID로 <b>참조</b>해야 쓸 수 있다.
 * 범위는 {@code scope=global}이나 {@code project=<프로젝트 slug>}로 준다. 참조는 이름이 아니라 {@code sk…}/{@code ap…} ID로 건다.
 */
public final class ResourceCommands {

    public static final String LIST_USAGE = "list-resources [scope=global|project=<프로젝트 slug>] [kind=skill|agent] [bodies=true. 지침까지 싣는다] [trash=true. 휴지통 목록]";
    public static final String GET_USAGE = "get-resource <자원 id> [version=<list-resource-versions 의 판>]";
    public static final String SAVE_USAGE = "save-resource kind=skill|agent scope=global|project=<프로젝트 slug> name=<이름> description=<설명> [body=<지침>] "
            + "[permission=read|edit|publish. 에이전트] [skills=<스킬 id JSON 배열. 에이전트> ] [repo=<프로젝트 저장소 이름. 프로젝트 에이전트. 빈 값이면 지운다>] "
            + "[tools=<도구별 칸 JSON 객체. 예: {\"claude\":{...}}. 빈 값이면 비운다>]  |  고칠 때: save-resource id=<자원 id> f1=<직전에 읽은 토큰> [고칠 칸...]";
    public static final String DELETE_USAGE = "delete-resource <자원 id>";
    public static final String RESTORE_USAGE = "restore-resource <휴지통의 자원 id. list-resources trash=true 로 조회한다>";
    public static final String REF_USAGE = "set-resource-ref <프로젝트 slug> <글로벌 자원 id> [remove=true] [with-skills=true. 에이전트를 참조할 때 그 스킬도 함께]";
    public static final String USABLE_USAGE = "usable-resources <프로젝트 slug> [bodies=true]";
    public static final String VERSIONS_USAGE = "list-resource-versions <자원 id>";

    public static final Set<String> NAMES = Set.of("list-resources", "get-resource", "save-resource", "delete-resource", "restore-resource",
            "set-resource-ref", "usable-resources", "list-resource-versions");

    private static final DateTimeFormatter STAMP = DateTimeFormatter.ISO_OFFSET_DATE_TIME;

    private ResourceCommands() {}

    public static Object execute(Path vault, ParsedArgs parsed, String sessionId) {
        WorkspaceMarker.check(vault);
        return switch (parsed.commandName()) {
            case "list-resources" -> list(vault, parsed.namedOptions());
            case "get-resource" -> get(vault, parsed);
            case "save-resource" -> save(vault, parsed.namedOptions(), sessionId);
            case "delete-resource" -> delete(vault, parsed);
            case "restore-resource" -> restore(vault, parsed);
            case "set-resource-ref" -> ref(vault, parsed);
            case "usable-resources" -> usable(vault, parsed);
            case "list-resource-versions" -> versions(vault, parsed);
            default -> throw new AisException(400, "모르는 명령이다: " + parsed.commandName());
        };
    }

    // ------------------------------------------------------------------ 목록·보기

    private static Object list(Path vault, Map<String, String> opts) {
        Scope scope = scopeOf(opts, false);
        ResourceKind kind = opts.containsKey("kind") ? kindOf(opts.get("kind")) : null;
        if (scope.project != null) ResourceStore.requireProject(vault, scope.project);

        Map<String, Object> out = new LinkedHashMap<>();
        if (isTrue(opts.get("trash"))) {
            List<Object> rows = new ArrayList<>();
            for (ResourceStore.TrashEntry t : ResourceStore.trash(vault)) {
                if (kind != null && t.kind() != kind) continue;
                if (scope.given && !java.util.Objects.equals(scope.project, t.project())) continue;
                Map<String, Object> m = new LinkedHashMap<>();
                m.put("id", t.id());
                m.put("kind", t.kind().label());
                m.put("scope", t.project() == null ? "global" : "project");
                if (t.project() != null) m.put("project", t.project());
                m.put("name", t.name());
                m.put("path", t.relPath());
                m.put("deletedAt", t.deletedAt());
                rows.add(m);
            }
            out.put("trash", rows);
            return out;
        }

        boolean bodies = isTrue(opts.get("bodies"));
        ResourceStore.Scan scan = ResourceStore.scan(vault);
        List<Resource> items = new ArrayList<>();
        for (Resource r : scan.items()) {
            if (kind != null && r.kind() != kind) continue;
            if (scope.given && !java.util.Objects.equals(scope.project, r.project())) continue;
            items.add(r);
        }
        items.sort(Comparator.comparing((Resource r) -> r.project() == null ? "" : r.project())
                .thenComparing(r -> r.kind().ordinal())
                .thenComparing(r -> r.name().toLowerCase(java.util.Locale.ROOT)));
        List<Object> rows = new ArrayList<>();
        for (Resource r : items) rows.add(row(r, bodies, false));
        out.put("resources", rows);
        if (!scan.skipped().isEmpty()) out.put("skipped", scan.skipped());
        return out;
    }

    private static Object get(Path vault, ParsedArgs parsed) {
        String id = onlyId(parsed, GET_USAGE);
        Resource cur = ResourceStore.find(vault, id);
        String version = parsed.namedOptions().get("version");
        Map<String, Object> m;
        if (version != null && !Trim.isBlank(version)) {
            Resource old = ResourceStore.readVersion(vault, cur, version.trim());
            m = row(old.withToken(null), true, false);
            m.put("version", version.trim());
        } else {
            m = row(cur, true, false);
        }
        // 고칠 때 넘길 토큰은 늘 지금 파일의 것이다(옛 판을 읽어도, 그 내용으로 되돌려 쓰려면 지금 파일의 토큰이 필요하다)
        m.put("precondition", cur.token());
        if (cur.isGlobal()) m.put("referencedBy", ResourceRefs.projectsReferencing(vault, cur.id()));
        return m;
    }

    private static Object versions(Path vault, ParsedArgs parsed) {
        String id = onlyId(parsed, VERSIONS_USAGE);
        Resource cur = ResourceStore.find(vault, id);
        List<Object> rows = new ArrayList<>();
        for (ResourceStore.Version v : ResourceStore.versions(vault, cur)) {
            Map<String, Object> m = new LinkedHashMap<>();
            m.put("version", v.version());
            m.put("savedAt", v.savedAt());
            m.put("bytes", (long) v.bytes());
            if (v.editedAt() != null) m.put("editedAt", v.editedAt());
            if (v.editedSession() != null) m.put("editedSession", v.editedSession());
            rows.add(m);
        }
        Map<String, Object> out = new LinkedHashMap<>();
        out.put("id", cur.id());
        out.put("versions", rows);
        return out;
    }

    // ------------------------------------------------------------------ 저장·지우기

    private static Object save(Path vault, Map<String, String> opts, String sessionId) {
        ResourceStore.Stamp stamp = stamp(sessionId);
        String id = opts.get("id");
        boolean creating = id == null || Trim.isBlank(id);
        if (creating) {
            ResourceKind kind = kindOf(opts.get("kind"));
            Scope scope = scopeOf(opts, true);
            rejectAgentOnlyForSkill(kind, opts);
            ResourceStore.Fields f = fields(opts);
            Resource r = ResourceStore.create(vault, kind, scope.project, f, stamp);
            Map<String, Object> m = row(r, true, true);
            m.put("created", true);
            return m;
        }
        Resource cur = ResourceStore.find(vault, id.trim());
        if (opts.containsKey("kind") && kindOf(opts.get("kind")) != cur.kind()) {
            throw new AisException(400, "종류는 바꿀 수 없다: " + cur.kind().label());
        }
        Scope scope = scopeOf(opts, false);
        if (scope.given && !java.util.Objects.equals(scope.project, cur.project())) {
            throw new AisException(400, "범위는 바꿀 수 없다: " + (cur.isGlobal() ? "global" : "project=" + cur.project()));
        }
        rejectAgentOnlyForSkill(cur.kind(), opts);
        Resource next = ResourceStore.update(vault, cur, fields(opts), opts.get("f1"), stamp);
        Map<String, Object> m = row(next, true, true);
        m.put("created", false);
        m.put("changed", next != cur);
        return m;
    }

    private static void rejectAgentOnlyForSkill(ResourceKind kind, Map<String, String> opts) {
        if (kind == ResourceKind.SKILL && (opts.containsKey("permission") || opts.containsKey("skills") || opts.containsKey("repo"))) {
            throw new AisException(400, "permission, skills, repo 는 에이전트에만 쓴다");
        }
    }

    private static ResourceStore.Fields fields(Map<String, String> opts) {
        return new ResourceStore.Fields(
                opts.containsKey("name") ? ResourceFormat.name(opts.get("name")) : null,
                opts.containsKey("description") ? ResourceFormat.description(opts.get("description")) : null,
                opts.containsKey("body") ? ResourceFormat.body(opts.get("body")) : null,
                opts.containsKey("permission") ? ResourceFormat.permission(opts.get("permission")) : null,
                opts.containsKey("skills") ? ResourceFormat.skillIds(opts.get("skills")) : null,
                opts.containsKey("repo") ? Trim.trim(opts.get("repo")) : null,
                opts.containsKey("tools") ? ResourceFormat.tools(opts.get("tools")) : null);
    }

    private static Object delete(Path vault, ParsedArgs parsed) {
        String id = onlyId(parsed, DELETE_USAGE);
        Resource cur = ResourceStore.find(vault, id);
        List<String> referencedBy = cur.isGlobal() ? ResourceRefs.projectsReferencing(vault, cur.id()) : List.of();
        ResourceStore.delete(vault, cur);
        Map<String, Object> m = new LinkedHashMap<>();
        m.put("id", cur.id());
        m.put("kind", cur.kind().label());
        m.put("scope", cur.scope());
        if (cur.project() != null) m.put("project", cur.project());
        m.put("name", cur.name());
        m.put("trashed", true);
        m.put("referencedBy", referencedBy);
        return m;
    }

    private static Object restore(Path vault, ParsedArgs parsed) {
        String id = onlyId(parsed, RESTORE_USAGE);
        Resource r = ResourceStore.restore(vault, id);
        Map<String, Object> m = row(r, true, true);
        m.put("restored", true);
        return m;
    }

    // ------------------------------------------------------------------ 참조·쓸 수 있는 자원

    private static Object ref(Path vault, ParsedArgs parsed) {
        List<String> pos = parsed.positionalArgs();
        if (pos.size() != 2) {
            throw new AisException(400, "set-resource-ref 는 프로젝트 slug 와 글로벌 자원 id 를 받는다: " + REF_USAGE);
        }
        String project = pos.get(0);
        String id = pos.get(1);
        ResourceStore.requireProject(vault, project);
        boolean remove = isTrue(parsed.namedOptions().get("remove"));
        boolean withSkills = isTrue(parsed.namedOptions().get("with-skills"));
        if (!ResourceKind.isId(id)) {
            throw new AisException(400, "자원 id 모양이 아니다(sk… 또는 ap…): " + id);
        }

        ResourceStore.Scan scan = ResourceStore.scan(vault);
        List<ResourceRefs.Entry> refs = new ArrayList<>(ResourceRefs.read(vault, project));
        List<String> added = new ArrayList<>();
        List<String> removed = new ArrayList<>();
        if (remove) {
            if (refs.removeIf(e -> e.id().equals(id))) removed.add(id);
        } else {
            Resource g = scan.byId(id).orElseThrow(() -> new AisException(404, "자원을 못 찾았다: " + id));
            if (!g.isGlobal()) {
                throw new AisException(400, "글로벌 자원만 참조한다. 프로젝트 자원은 참조하지 않아도 쓸 수 있다: " + id);
            }
            addRef(refs, g, added);
            if (withSkills && g.kind() == ResourceKind.AGENT) {
                for (String sid : g.skills()) {
                    scan.byId(sid).filter(Resource::isGlobal).ifPresent(s -> addRef(refs, s, added));
                }
            }
        }
        // 이름 힌트를 지금 이름으로 새로 적는다(ID는 그대로다)
        List<ResourceRefs.Entry> next = new ArrayList<>();
        for (ResourceRefs.Entry e : refs) {
            next.add(new ResourceRefs.Entry(e.id(), scan.byId(e.id()).map(Resource::name).orElse(e.hint())));
        }
        ResourceRefs.write(vault, project, next);

        Map<String, Object> out = new LinkedHashMap<>();
        out.put("project", project);
        out.put("added", added);
        out.put("removed", removed);
        List<Object> rows = new ArrayList<>();
        for (ResourceRefs.Entry e : next) {
            Map<String, Object> m = new LinkedHashMap<>();
            m.put("id", e.id());
            var found = scan.byId(e.id()).filter(Resource::isGlobal);
            m.put("status", found.isPresent() ? "ok" : "missing");
            m.put("kind", ResourceKind.ofId(e.id()).label());
            m.put("name", found.map(Resource::name).orElse(e.hint()));
            rows.add(m);
        }
        out.put("refs", rows);
        return out;
    }

    private static void addRef(List<ResourceRefs.Entry> refs, Resource g, List<String> added) {
        if (refs.stream().noneMatch(e -> e.id().equals(g.id()))) {
            refs.add(new ResourceRefs.Entry(g.id(), g.name()));
            added.add(g.id());
        }
    }

    private static Object usable(Path vault, ParsedArgs parsed) {
        String project = onlyId(parsed, USABLE_USAGE);
        boolean bodies = isTrue(parsed.namedOptions().get("bodies"));
        ResourceResolver.Usable u = ResourceResolver.resolve(vault, project);
        Map<String, Object> out = new LinkedHashMap<>();
        out.put("project", u.project());
        out.put("skills", itemRows(u.skills(), bodies));
        out.put("agents", itemRows(u.agents(), bodies));
        List<Object> broken = new ArrayList<>();
        for (ResourceResolver.Broken b : u.broken()) {
            Map<String, Object> m = new LinkedHashMap<>();
            m.put("id", b.id());
            m.put("hint", b.hint());
            broken.add(m);
        }
        out.put("broken", broken);
        List<Object> missing = new ArrayList<>();
        for (ResourceResolver.Missing mi : u.missingSkills()) {
            Map<String, Object> m = new LinkedHashMap<>();
            Map<String, Object> agent = new LinkedHashMap<>();
            agent.put("id", mi.agent().id());
            agent.put("name", mi.agent().name());
            m.put("agent", agent);
            m.put("skillId", mi.skillId());
            if (mi.skill() != null) {
                Map<String, Object> skill = new LinkedHashMap<>();
                skill.put("name", mi.skill().name());
                skill.put("scope", mi.skill().scope());
                m.put("skill", skill);
            }
            m.put("canReference", mi.canReference());
            missing.add(m);
        }
        out.put("missingSkills", missing);
        return out;
    }

    private static List<Object> itemRows(List<ResourceResolver.Item> items, boolean bodies) {
        List<Object> rows = new ArrayList<>();
        for (ResourceResolver.Item it : items) {
            Map<String, Object> m = row(it.resource(), bodies, false);
            m.put("source", it.source());
            rows.add(m);
        }
        return rows;
    }

    // ------------------------------------------------------------------ 공통

    /** 응답 행. 글 칸은 있을 때만 싣는다. */
    private static Map<String, Object> row(Resource r, boolean withBody, boolean withToken) {
        Map<String, Object> m = new LinkedHashMap<>();
        m.put("id", r.id());
        m.put("kind", r.kind().label());
        m.put("scope", r.scope());
        if (r.project() != null) m.put("project", r.project());
        m.put("name", r.name());
        m.put("description", r.description());
        if (r.kind() == ResourceKind.AGENT) {
            m.put("permission", r.permission());
            m.put("skills", r.skills());
            if (r.repo() != null) m.put("repo", r.repo());
        }
        if (r.tools() != null && !r.tools().isEmpty()) m.put("tools", r.tools());
        if (r.editedAt() != null) m.put("editedAt", r.editedAt());
        if (r.editedSession() != null) m.put("editedSession", r.editedSession());
        if (r.editedAgent() != null) m.put("editedAgent", r.editedAgent());
        m.put("path", r.relPath());
        if (withBody) m.put("body", r.body());
        if (withToken && r.token() != null) m.put("precondition", r.token());
        return m;
    }

    private record Scope(boolean given, String project) {}

    private static Scope scopeOf(Map<String, String> opts, boolean required) {
        String scope = opts.get("scope");
        String project = opts.get("project");
        boolean g = scope != null && !Trim.isBlank(scope);
        boolean p = project != null && !Trim.isBlank(project);
        if (g && p) {
            throw new AisException(400, "scope=global 과 project=<프로젝트 slug> 는 함께 줄 수 없다");
        }
        if (g) {
            if (!"global".equals(scope.trim())) {
                throw new AisException(400, "scope 는 global 이어야 한다. 프로젝트 자원은 project=<프로젝트 slug> 로 준다: " + scope);
            }
            return new Scope(true, null);
        }
        if (p) return new Scope(true, project.trim());
        if (required) {
            throw new AisException(400, "scope=global 이나 project=<프로젝트 slug> 가 필요하다");
        }
        return new Scope(false, null);
    }

    private static ResourceKind kindOf(String raw) {
        ResourceKind k = ResourceKind.parse(raw);
        if (k == null) {
            throw new AisException(400, "kind 는 skill 이나 agent 여야 한다: " + raw);
        }
        return k;
    }

    private static boolean isTrue(String v) {
        return v != null && "true".equals(Trim.trim(v).toLowerCase(java.util.Locale.ROOT));
    }

    private static String onlyId(ParsedArgs parsed, String usage) {
        List<String> pos = parsed.positionalArgs();
        if (pos.size() != 1 || Trim.isBlank(pos.get(0))) {
            throw new AisException(400, "인자가 하나 필요하다: " + usage);
        }
        return pos.get(0).trim();
    }

    /** 고친 기록. AI 세션이 고쳤으면 세션 ID와(Claude가 아니면) CLI 이름이 남고, 사람이 고쳤으면 시각만 남는다. */
    private static ResourceStore.Stamp stamp(String sessionId) {
        OffsetDateTime now = TestApparatus.nowInstant().atZone(ZoneId.systemDefault()).toOffsetDateTime().withNano(0);
        String session = sessionId == null || sessionId.isBlank() ? null : sessionId;
        String agent = null;
        if (session != null) {
            String a = WritePipeline.currentSessionAgent();
            if (a != null && !"claude".equals(a)) agent = a;
        }
        return new ResourceStore.Stamp(STAMP.format(now), session, agent);
    }
}
