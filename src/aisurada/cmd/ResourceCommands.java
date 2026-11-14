package aisurada.cmd;

import aisurada.args.ParsedArgs;
import aisurada.cli.AisException;
import aisurada.engine.WritePipeline;
import aisurada.resource.Approvals;
import aisurada.resource.Resource;
import aisurada.resource.ResourceChanges;
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
import java.util.Objects;
import java.util.Set;

/**
 * 스킬과 에이전트 정의(자원)를 다루는 명령. <b>외부 설명서에 없는 확장</b>입니다.
 *
 * <p>자원은 <b>글로벌</b>과 <b>프로젝트</b> 두 범위이고, 프로젝트는 글로벌 자원을 ID로 <b>참조</b>해야 쓸 수 있다.
 * 범위는 {@code scope=global}이나 {@code project=<프로젝트 slug>}로 준다. 참조는 이름이 아니라 {@code sk…}/{@code ap…} ID로 건다.
 *
 * <p><b>승인 워크플로(#53).</b> AI가 보낸 변경(MCP 도구 이름으로 부른 요청, 또는 AI 세션 표식이 달린 요청)은 정책에 따라 바로 적용하거나
 * 「대기 변경」으로 올린다. 대기 변경은 사람이 승인해야 적용되고, 그 전에는 목록·조회·{@code usable-resources}에 이전 판이 그대로 보인다.
 * 권한 올림, 참조 켜기, 도구 칸 변경은 정책과 상관없이 늘 승인을 거친다. 사람이 앱이나 명령줄에서 보낸 변경은 바로 적용한다.
 * 승인, 거절, 정책 변경은 사람만 한다(AI가 보내면 AIS-403).
 */
public final class ResourceCommands {

    public static final String LIST_USAGE = "list-resources [scope=global|project=<프로젝트 slug>] [kind=skill|agent] [bodies=true. 지침까지 싣는다] [trash=true. 휴지통 목록]";
    public static final String GET_USAGE = "get-resource <자원 id> [version=<list-resource-versions 의 판>]";
    public static final String SAVE_USAGE = "save-resource kind=skill|agent scope=global|project=<프로젝트 slug> name=<이름> description=<설명> [body=<지침>] "
            + "[permission=read|edit|publish. 에이전트] [skills=<스킬 id JSON 배열. 에이전트> ] [repo=<프로젝트 저장소 이름. 프로젝트 에이전트. 빈 값이면 지운다>] "
            + "[tools=<도구별 칸 JSON 객체. 예: {\"claude\":{...}}. 빈 값이면 비운다>]  |  고칠 때: save-resource id=<자원 id> f1=<직전에 읽은 토큰> [고칠 칸...]. "
            + "AI가 보낸 변경은 정책에 따라 승인 대기에 오를 수 있다(응답의 queued)";
    public static final String DELETE_USAGE = "delete-resource <자원 id>";
    public static final String RESTORE_USAGE = "restore-resource <휴지통의 자원 id. list-resources trash=true 로 조회한다>";
    public static final String REF_USAGE = "set-resource-ref <프로젝트 slug> <글로벌 자원 id> [remove=true] [with-skills=true. 에이전트를 참조할 때 그 스킬도 함께]";
    public static final String USABLE_USAGE = "usable-resources <프로젝트 slug> [bodies=true]";
    public static final String VERSIONS_USAGE = "list-resource-versions <자원 id>";
    public static final String CHANGES_USAGE = "list-resource-changes [decided=<결정이 난 변경을 몇 건 싣나. 기본 20, 0이면 안 싣는다>]";
    public static final String CHANGE_GET_USAGE = "get-resource-change <변경 id. list-resource-changes 로 조회한다>";
    public static final String APPROVE_USAGE = "approve-resource-change <변경 id>. 사람만 한다";
    public static final String REJECT_USAGE = "reject-resource-change <변경 id> [reason=<거절 이유>]. 사람만 한다";
    public static final String RESOLVE_USAGE = "resolve-resource-edit <자원 id 또는 refs:<프로젝트 slug>> action=approve|revert. 승인 안 된 직접 수정을 받아들이거나(approve) "
            + "승인한 판으로 되돌린다(revert. 새 파일이면 휴지통으로). 사람만 한다";
    public static final String POLICY_GET_USAGE = "get-resource-policy";
    public static final String POLICY_SET_USAGE = "set-resource-policy [project=auto|approve] [global=auto|approve] [refs=auto|approve] [permission=auto|approve]. 사람만 한다";

    public static final Set<String> NAMES = Set.of("list-resources", "get-resource", "save-resource", "delete-resource", "restore-resource",
            "set-resource-ref", "usable-resources", "list-resource-versions", "list-resource-changes", "get-resource-change",
            "approve-resource-change", "reject-resource-change", "resolve-resource-edit", "get-resource-policy", "set-resource-policy");

    private static final DateTimeFormatter STAMP = DateTimeFormatter.ISO_OFFSET_DATE_TIME;

    private ResourceCommands() {}

    /**
     * 한 번의 실행. {@code ai}는 AI의 통로로 온 요청인지, {@code apply}는 승인된 대기 변경을 적용하는 중인지다.
     * 적용 중에는 대기에 다시 올리지 않고 올린 사람의 기록({@code stamp})으로 쓰며, 승인 기록에는 {@code by}가 남는다.
     */
    private record Run(Path vault, boolean ai, ResourceStore.Stamp stamp, boolean apply, String by, String forcedId, String channel) {}

    /** AI의 통로 판별: AI 세션 표식이 달렸거나 MCP 도구 이름으로 불렀으면 AI다. 앱 화면은 명령 이름으로 세션 표식 없이 부른다. */
    private static boolean isAi(String sessionId, boolean viaTool) {
        return viaTool || (sessionId != null && !sessionId.isBlank());
    }

    public static Object execute(Path vault, ParsedArgs parsed, String sessionId, boolean viaTool) {
        WorkspaceMarker.check(vault);
        // 기록장이 없으면 여기서 기준선을 만든다. 쓰기보다 먼저여야 방금 쓴 파일이 기준선에 섞이지 않는다
        Approvals.ledger(vault);
        boolean ai = isAi(sessionId, viaTool);
        return switch (parsed.commandName()) {
            case "list-resources" -> list(vault, parsed.namedOptions());
            case "get-resource" -> get(vault, parsed);
            case "save-resource" -> save(run(vault, ai, sessionId, viaTool), parsed.namedOptions());
            case "delete-resource" -> delete(run(vault, ai, sessionId, viaTool), onlyId(parsed, DELETE_USAGE));
            case "restore-resource" -> restore(run(vault, ai, sessionId, viaTool), onlyId(parsed, RESTORE_USAGE));
            case "set-resource-ref" -> ref(run(vault, ai, sessionId, viaTool), parsed);
            case "usable-resources" -> usable(vault, parsed);
            case "list-resource-versions" -> versions(vault, parsed);
            case "list-resource-changes" -> changes(vault, parsed);
            case "get-resource-change" -> changeGet(vault, parsed);
            case "approve-resource-change" -> approve(vault, parsed, ai);
            case "reject-resource-change" -> reject(vault, parsed, ai);
            case "resolve-resource-edit" -> resolve(vault, parsed, ai);
            case "get-resource-policy" -> policyGet(vault);
            case "set-resource-policy" -> policySet(vault, parsed, ai);
            default -> throw new AisException(400, "모르는 명령이다: " + parsed.commandName());
        };
    }

    private static Run run(Path vault, boolean ai, String sessionId, boolean viaTool) {
        return new Run(vault, ai, stamp(sessionId), false, ai ? "policy" : "human", null, viaTool ? "mcp" : "cli");
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
        ResourceStore.Served scan = ResourceStore.scanServed(vault);
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
        for (Resource r : items) {
            Map<String, Object> m = row(r, bodies, false);
            if ("dirty".equals(scan.states().get(r.id()))) m.put("approval", "dirty");
            rows.add(m);
        }
        out.put("resources", rows);
        if (!scan.skipped().isEmpty()) out.put("skipped", scan.skipped());
        out.put("approvals", approvalCounts(vault));
        return out;
    }

    private static Object get(Path vault, ParsedArgs parsed) {
        String id = onlyId(parsed, GET_USAGE);
        Resource live = ResourceStore.find(vault, id);
        // 읽는 것은 승인한 판이다. 승인 기록이 없는 새 파일은 보여 주지 않는다
        Resource cur = ResourceStore.scanServed(vault).byId(id).orElseThrow(() -> new AisException(404,
                "승인되지 않은 자원이라 조회되지 않는다(list-resource-changes 로 승인 상태를 본다): " + id));
        String version = parsed.namedOptions().get("version");
        Map<String, Object> m;
        if (version != null && !Trim.isBlank(version)) {
            Resource old = ResourceStore.readVersion(vault, cur, version.trim());
            m = row(old.withToken(null), true, false);
            m.put("version", version.trim());
        } else {
            m = row(cur, true, false);
        }
        if ("dirty".equals(ResourceStore.approvalState(vault, live))) m.put("approval", "dirty");
        // 고칠 때 넘길 토큰은 늘 지금 파일의 것이다(옛 판을 읽어도, 그 내용으로 되돌려 쓰려면 지금 파일의 토큰이 필요하다)
        m.put("precondition", live.token());
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

    private static Object save(Run run, Map<String, String> opts) {
        Path vault = run.vault();
        String id = opts.get("id");
        boolean creating = id == null || Trim.isBlank(id);
        if (creating) {
            ResourceKind kind = kindOf(opts.get("kind"));
            Scope scope = scopeOf(opts, true);
            rejectAgentOnlyForSkill(kind, opts);
            ResourceStore.Fields f = fields(opts);
            ResourceStore.Plan plan = ResourceStore.planCreate(vault, kind, scope.project, f, run.stamp(), run.forcedId());
            List<Approvals.Category> cats = Approvals.classify(null, plan.after());
            Map<String, Object> queued = queueIfNeeded(run, "save-resource", opts, List.of(), plan.after().id(), null, plan.after(), cats, null);
            if (queued != null) return queued;
            Resource r = ResourceStore.commitCreate(vault, plan);
            Approvals.record(vault, Approvals.resKey(r.id()), ResourceStore.liveBytes(vault, r), run.by());
            Map<String, Object> m = row(r, true, true);
            m.put("created", true);
            return m;
        }
        Resource live = ResourceStore.find(vault, id.trim());
        if (opts.containsKey("kind") && kindOf(opts.get("kind")) != live.kind()) {
            throw new AisException(400, "종류는 바꿀 수 없다: " + live.kind().label());
        }
        Scope scope = scopeOf(opts, false);
        if (scope.given && !java.util.Objects.equals(scope.project, live.project())) {
            throw new AisException(400, "범위는 바꿀 수 없다: " + (live.isGlobal() ? "global" : "project=" + live.project()));
        }
        rejectAgentOnlyForSkill(live.kind(), opts);
        // 고칠 바탕은 승인한 판이다. 직접 고쳐진 파일의 내용은 이어받지 않는다(받아들이려면 resolve-resource-edit)
        Resource base = ResourceStore.baseForEdit(vault, live);
        ResourceStore.Plan plan = ResourceStore.planUpdate(vault, base, fields(opts), opts.get("f1"), run.stamp());
        if (!plan.changed()) {
            Map<String, Object> m = row(base, true, true);
            m.put("created", false);
            m.put("changed", false);
            return m;
        }
        List<Approvals.Category> cats = Approvals.classify(base, plan.after());
        Map<String, Object> queued = queueIfNeeded(run, "save-resource", opts, List.of(), null, base, plan.after(), cats, null);
        if (queued != null) return queued;
        Resource next = ResourceStore.commitUpdate(vault, plan);
        Approvals.record(vault, Approvals.resKey(next.id()), ResourceStore.liveBytes(vault, next), run.by());
        Map<String, Object> m = row(next, true, true);
        m.put("created", false);
        m.put("changed", true);
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

    private static Object delete(Run run, String id) {
        Path vault = run.vault();
        Resource cur = ResourceStore.find(vault, id);
        List<Approvals.Category> cats = Approvals.classify(cur, null);
        Map<String, Object> queued = queueIfNeeded(run, "delete-resource", Map.of(), List.of(id), null, cur, null, cats, null);
        if (queued != null) return queued;
        List<String> referencedBy = cur.isGlobal() ? ResourceRefs.projectsReferencing(vault, cur.id()) : List.of();
        ResourceStore.delete(vault, cur);
        Approvals.forget(vault, Approvals.resKey(cur.id()));
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

    private static Object restore(Run run, String id) {
        Path vault = run.vault();
        Resource back = ResourceStore.peekTrash(vault, id);
        List<Approvals.Category> cats = Approvals.classify(null, back);
        Map<String, Object> queued = queueIfNeeded(run, "restore-resource", Map.of(), List.of(id), null, null, back, cats, null);
        if (queued != null) return queued;
        Resource r = ResourceStore.restore(vault, id);
        Approvals.record(vault, Approvals.resKey(r.id()), ResourceStore.liveBytes(vault, r), run.by());
        Map<String, Object> m = row(r, true, true);
        m.put("restored", true);
        return m;
    }

    // ------------------------------------------------------------------ 참조·쓸 수 있는 자원

    private static Object ref(Run run, ParsedArgs parsed) {
        Path vault = run.vault();
        List<String> pos = parsed.positionalArgs();
        if (pos.size() != 2) {
            throw new AisException(400, "set-resource-ref 는 프로젝트 slug 와 글로벌 자원 id 를 받는다: " + REF_USAGE);
        }
        boolean remove = isTrue(parsed.namedOptions().get("remove"));
        boolean withSkills = isTrue(parsed.namedOptions().get("with-skills"));
        return refImpl(run, pos.get(0), pos.get(1), remove, withSkills, parsed.namedOptions());
    }

    private static Object refImpl(Run run, String project, String id, boolean remove, boolean withSkills, Map<String, String> opts) {
        Path vault = run.vault();
        ResourceStore.requireProject(vault, project);
        if (!ResourceKind.isId(id)) {
            throw new AisException(400, "자원 id 모양이 아니다(sk… 또는 ap…): " + id);
        }

        // 바탕은 승인한 참조 목록과 승인한 자원이다(직접 고쳐진 refs.md의 내용은 이어받지 않는다)
        ResourceStore.Served scan = ResourceStore.scanServed(vault);
        List<ResourceRefs.Entry> refs = new ArrayList<>(ResourceRefs.readApproved(vault, project));
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

        List<Approvals.Category> cats = Approvals.classifyRefs(added, removed);
        if (!cats.isEmpty()) {
            Map<String, Object> refInfo = new LinkedHashMap<>();
            refInfo.put("project", project);
            refInfo.put("added", added);
            refInfo.put("removed", removed);
            Map<String, Object> queued = queueIfNeeded(run, "set-resource-ref", withoutFlags(opts), List.of(project, id), null, null, null, cats, refInfo);
            if (queued != null) return queued;
        }
        ResourceRefs.write(vault, project, next);
        Approvals.record(vault, Approvals.refsKey(project), ResourceRefs.liveBytes(vault, project), run.by());

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

    private static Map<String, String> withoutFlags(Map<String, String> opts) {
        return new LinkedHashMap<>(opts);
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

    // ------------------------------------------------------------------ 승인 워크플로(#53)

    /**
     * 변경이 승인을 기다려야 하면 대기에 올리고 응답을 돌려준다. 바로 적용해도 되면 null이다.
     * 같은 변경이 이미 대기 중이면 새로 올리지 않고 그것을 알려 준다(AI가 같은 요청을 되풀이해도 쌓이지 않게).
     */
    private static Map<String, Object> queueIfNeeded(Run run, String command, Map<String, String> args, List<String> positional, String plannedId,
                                                     Resource before, Resource after, List<Approvals.Category> cats, Map<String, Object> refInfo) {
        if (run.apply()) return null;
        Path vault = run.vault();
        Approvals.Decision d = Approvals.decide(Approvals.policy(vault), cats, run.ai());
        if (!d.queue()) return null;

        for (Map<String, Object> m : ResourceChanges.pending(vault)) {
            if (command.equals(m.get("command")) && Objects.equals(args, m.get("args")) && Objects.equals(positional, m.get("positional"))) {
                return queuedResponse(m, true);
            }
        }
        Resource any = after != null ? after : before;
        Map<String, Object> c = new LinkedHashMap<>();
        c.put("id", ResourceChanges.newId());
        c.put("status", ResourceChanges.PENDING);
        c.put("command", command);
        c.put("createdAt", run.stamp().at());
        Map<String, Object> proposer = new LinkedHashMap<>();
        if (run.stamp().session() != null) proposer.put("session", run.stamp().session());
        if (run.stamp().agent() != null) proposer.put("agent", run.stamp().agent());
        proposer.put("channel", run.channel());
        c.put("proposer", proposer);
        List<Object> all = new ArrayList<>();
        for (Approvals.Category x : cats) all.add(x.code());
        c.put("categories", all);
        List<Object> reasons = new ArrayList<>();
        boolean fixed = false;
        for (Approvals.Category x : d.reasons()) {
            reasons.add(x.code());
            fixed |= x.fixed();
        }
        c.put("reasons", reasons);
        c.put("fixed", fixed);
        Map<String, Object> target = new LinkedHashMap<>();
        if (any != null) {
            target.put("kind", any.kind().label());
            target.put("scope", any.scope());
            if (any.project() != null) target.put("project", any.project());
            target.put("id", any.id());
            target.put("name", any.name());
        } else if (refInfo != null) {
            target.put("scope", "project");
            target.put("project", refInfo.get("project"));
        }
        c.put("target", target);
        c.put("args", new LinkedHashMap<>(args));
        c.put("positional", new ArrayList<>(positional));
        if (plannedId != null) c.put("forcedId", plannedId);
        if (before != null) c.put("before", ResourceChanges.view(before));
        if (after != null) c.put("after", ResourceChanges.view(after));
        if (refInfo != null) c.put("ref", refInfo);
        c.put("summary", summarize(command, before, after, refInfo));
        ResourceChanges.write(vault, c);
        return queuedResponse(c, false);
    }

    private static Map<String, Object> queuedResponse(Map<String, Object> c, boolean duplicate) {
        Map<String, Object> m = new LinkedHashMap<>();
        m.put("queued", true);
        m.put("changeId", c.get("id"));
        m.put("status", ResourceChanges.PENDING);
        if (duplicate) m.put("duplicate", true);
        m.put("summary", c.get("summary"));
        m.put("reasons", reasonRows(c));
        m.put("message", "변경을 적용하지 않고 승인 대기에 올렸다. 사람이 앱의 스킬·에이전트 화면에서 승인해야 적용되고, 그때까지 목록과 조회에는 이전 판이 그대로 보인다. "
                + "처리 결과는 get-resource-change 로 본다.");
        return m;
    }

    private static List<Object> reasonRows(Map<String, Object> c) {
        List<Object> rows = new ArrayList<>();
        Object reasons = c.get("reasons");
        if (reasons instanceof List<?> list) {
            for (Object code : list) {
                Approvals.Category cat = Approvals.Category.ofCode(String.valueOf(code));
                if (cat == null) continue;
                Map<String, Object> m = new LinkedHashMap<>();
                m.put("category", cat.code());
                m.put("label", cat.label());
                m.put("fixed", cat.fixed());
                rows.add(m);
            }
        }
        return rows;
    }

    private static String summarize(String command, Resource before, Resource after, Map<String, Object> refInfo) {
        if ("set-resource-ref".equals(command)) {
            List<?> added = (List<?>) refInfo.get("added");
            List<?> removed = (List<?>) refInfo.get("removed");
            StringBuilder sb = new StringBuilder("프로젝트 '").append(refInfo.get("project")).append("'의 글로벌 자원 참조 ");
            if (!added.isEmpty()) sb.append(added.size()).append("개 켜기");
            if (!added.isEmpty() && !removed.isEmpty()) sb.append(", ");
            if (!removed.isEmpty()) sb.append(removed.size()).append("개 끄기");
            return sb.toString();
        }
        Resource any = after != null ? after : before;
        String what = (any.isGlobal() ? "글로벌 " : "프로젝트 ") + (any.kind() == ResourceKind.SKILL ? "스킬" : "에이전트") + " '" + any.name() + "'";
        return switch (command) {
            case "delete-resource" -> what + " 지우기";
            case "restore-resource" -> what + " 되살리기";
            default -> before == null ? what + " 만들기" : what + " 고치기(" + String.join(", ", changedFields(before, after)) + ")";
        };
    }

    private static List<String> changedFields(Resource a, Resource b) {
        List<String> out = new ArrayList<>();
        if (!Objects.equals(a.name(), b.name())) out.add("이름");
        if (!Objects.equals(a.description(), b.description())) out.add("설명");
        if (!Objects.equals(a.body(), b.body())) out.add("지침");
        if (!Objects.equals(a.permission(), b.permission())) out.add("권한 " + a.permission() + " → " + b.permission());
        if (!Objects.equals(a.skills(), b.skills())) out.add("스킬 목록");
        if (!Objects.equals(a.repo(), b.repo())) out.add("저장소");
        if (!Objects.equals(a.tools(), b.tools())) out.add("도구 칸");
        return out;
    }

    private static void requireHuman(boolean ai, String what) {
        if (ai) {
            throw new AisException(403, what + "은(는) 사람만 할 수 있다. 앱의 스킬·에이전트 화면에서 한다");
        }
    }

    private static Map<String, Object> approvalCounts(Path vault) {
        Map<String, Object> m = new LinkedHashMap<>();
        m.put("pending", (long) ResourceChanges.pending(vault).size());
        m.put("unapproved", (long) unapprovedRows(vault).size());
        return m;
    }

    private static Object changes(Path vault, ParsedArgs parsed) {
        int decidedLimit = 20;
        String raw = parsed.namedOptions().get("decided");
        if (raw != null && !Trim.isBlank(raw)) {
            try {
                decidedLimit = Integer.parseInt(raw.trim());
            } catch (NumberFormatException e) {
                decidedLimit = -1;
            }
            if (decidedLimit < 0) throw new AisException(400, "decided 는 0 이상의 정수여야 한다: " + raw);
        }
        List<Object> pending = new ArrayList<>();
        List<Object> decided = new ArrayList<>();
        for (Map<String, Object> c : ResourceChanges.list(vault)) {
            if (ResourceChanges.PENDING.equals(c.get("status"))) {
                pending.add(changeRow(c, false));
            } else if (decided.size() < decidedLimit) {
                decided.add(changeRow(c, false));
            }
        }
        List<Object> unapproved = unapprovedRows(vault);
        Map<String, Object> counts = new LinkedHashMap<>();
        counts.put("pending", (long) pending.size());
        counts.put("unapproved", (long) unapproved.size());
        Map<String, Object> out = new LinkedHashMap<>();
        out.put("counts", counts);
        out.put("pending", pending);
        out.put("unapproved", unapproved);
        out.put("decided", decided);
        out.put("policy", Approvals.policy(vault).toMap());
        return out;
    }

    private static Object changeGet(Path vault, ParsedArgs parsed) {
        return changeRow(ResourceChanges.read(vault, onlyId(parsed, CHANGE_GET_USAGE)), true);
    }

    /** 변경 한 줄. 목록에서는 비교용 칸(before, after, args)을 빼고, 하나를 볼 때만 싣는다. */
    private static Map<String, Object> changeRow(Map<String, Object> c, boolean detail) {
        Map<String, Object> m = new LinkedHashMap<>();
        for (String key : List.of("id", "status", "command", "createdAt", "summary", "target", "proposer", "fixed")) {
            if (c.containsKey(key)) m.put(key, c.get(key));
        }
        m.put("reasons", reasonRows(c));
        List<Object> cats = new ArrayList<>();
        if (c.get("categories") instanceof List<?> list) {
            for (Object code : list) {
                Approvals.Category cat = Approvals.Category.ofCode(String.valueOf(code));
                if (cat == null) continue;
                Map<String, Object> r = new LinkedHashMap<>();
                r.put("category", cat.code());
                r.put("label", cat.label());
                r.put("fixed", cat.fixed());
                cats.add(r);
            }
        }
        m.put("categories", cats);
        for (String key : List.of("decidedAt", "decidedBy", "reason")) {
            if (c.containsKey(key)) m.put(key, c.get(key));
        }
        if (detail) {
            for (String key : List.of("before", "after", "ref", "args", "positional", "forcedId")) {
                if (c.containsKey(key)) m.put(key, c.get(key));
            }
            if (c.get("before") instanceof Map<?, ?> b && c.get("after") instanceof Map<?, ?> a) {
                List<Object> changedKeys = new ArrayList<>();
                for (String key : List.of("name", "description", "permission", "skills", "repo", "tools", "body")) {
                    if (!Objects.equals(b.get(key), a.get(key))) changedKeys.add(key);
                }
                m.put("changedFields", changedKeys);
            }
        }
        return m;
    }

    /** 승인 안 된 직접 수정: 승인한 뒤 파일이 바뀌었거나(dirty), 승인 기록 없이 생긴(new) 자원 파일과 참조 목록. */
    private static List<Object> unapprovedRows(Path vault) {
        List<Object> rows = new ArrayList<>();
        ResourceStore.Served served = ResourceStore.scanServed(vault);
        ResourceStore.Scan raw = ResourceStore.scan(vault);
        for (Resource approved : served.items()) {
            if (!"dirty".equals(served.states().get(approved.id()))) continue;
            Resource live = raw.byId(approved.id()).orElse(null);
            Map<String, Object> m = unapprovedRow(approved, "dirty");
            m.put("approved", ResourceChanges.view(approved));
            if (live != null) m.put("live", ResourceChanges.view(live));
            rows.add(m);
        }
        for (Resource r : served.untrusted()) {
            Map<String, Object> m = unapprovedRow(r, "new");
            m.put("live", ResourceChanges.view(r));
            rows.add(m);
        }
        java.util.TreeSet<String> projects = new java.util.TreeSet<>(ResourceRefs.projectsWithRefsFile(vault));
        for (String key : Approvals.ledger(vault).items().keySet()) {
            if (key.startsWith("refs:")) projects.add(key.substring(5));
        }
        for (String project : projects) {
            ResourceRefs.State st = ResourceRefs.state(vault, project);
            if (st == ResourceRefs.State.APPROVED) continue;
            Map<String, Object> m = new LinkedHashMap<>();
            m.put("key", Approvals.refsKey(project));
            m.put("type", "refs");
            m.put("state", st == ResourceRefs.State.DIRTY ? "dirty" : "new");
            m.put("project", project);
            m.put("approved", refIds(ResourceRefs.readApproved(vault, project)));
            m.put("live", refIds(ResourceRefs.read(vault, project)));
            rows.add(m);
        }
        return rows;
    }

    private static List<Object> refIds(List<ResourceRefs.Entry> entries) {
        List<Object> out = new ArrayList<>();
        for (ResourceRefs.Entry e : entries) out.add(e.id());
        return out;
    }

    private static Map<String, Object> unapprovedRow(Resource r, String state) {
        Map<String, Object> m = new LinkedHashMap<>();
        m.put("key", r.id());
        m.put("type", "resource");
        m.put("state", state);
        m.put("id", r.id());
        m.put("kind", r.kind().label());
        m.put("scope", r.scope());
        if (r.project() != null) m.put("project", r.project());
        m.put("name", r.name());
        m.put("path", r.relPath());
        return m;
    }

    private static Object approve(Path vault, ParsedArgs parsed, boolean ai) {
        requireHuman(ai, "승인");
        Map<String, Object> change = pendingChange(vault, onlyId(parsed, APPROVE_USAGE));
        Run run = applyRun(vault, change);
        Object result;
        try {
            result = applyChange(run, change);
        } catch (AisException e) {
            int code = e.getAisCode();
            if (code == 400 || code == 404 || code == 409 || code == 412) {
                decide(vault, change, ResourceChanges.STALE, "적용하지 못했다: " + e.getMessage());
            }
            throw e;
        }
        decide(vault, change, ResourceChanges.APPROVED, null);
        Map<String, Object> out = new LinkedHashMap<>();
        out.put("approved", true);
        out.put("changeId", change.get("id"));
        out.put("summary", change.get("summary"));
        out.put("result", result);
        return out;
    }

    private static Object reject(Path vault, ParsedArgs parsed, boolean ai) {
        requireHuman(ai, "거절");
        Map<String, Object> change = pendingChange(vault, onlyId(parsed, REJECT_USAGE));
        String reason = parsed.namedOptions().get("reason");
        decide(vault, change, ResourceChanges.REJECTED, reason == null || Trim.isBlank(reason) ? null : Trim.trim(reason));
        Map<String, Object> out = new LinkedHashMap<>();
        out.put("rejected", true);
        out.put("changeId", change.get("id"));
        out.put("summary", change.get("summary"));
        return out;
    }

    private static Map<String, Object> pendingChange(Path vault, String id) {
        Map<String, Object> change = ResourceChanges.read(vault, id);
        if (!ResourceChanges.PENDING.equals(change.get("status"))) {
            throw new AisException(409, "이미 처리된 변경이다(" + change.get("status") + "): " + id);
        }
        return change;
    }

    private static void decide(Path vault, Map<String, Object> change, String status, String reason) {
        change.put("status", status);
        change.put("decidedAt", STAMP.format(TestApparatus.nowInstant().atZone(ZoneId.systemDefault()).toOffsetDateTime().withNano(0)));
        change.put("decidedBy", "human");
        if (reason != null) change.put("reason", reason);
        ResourceChanges.write(vault, change);
        ResourceChanges.prune(vault);
    }

    /** 승인한 변경을 적용할 실행. 올린 사람의 기록으로 쓰고, 대기에 다시 올리지 않는다. */
    private static Run applyRun(Path vault, Map<String, Object> change) {
        Map<?, ?> proposer = change.get("proposer") instanceof Map<?, ?> p ? p : Map.of();
        String session = proposer.get("session") == null ? null : String.valueOf(proposer.get("session"));
        String agent = proposer.get("agent") == null ? null : String.valueOf(proposer.get("agent"));
        String channel = proposer.get("channel") == null ? "mcp" : String.valueOf(proposer.get("channel"));
        ResourceStore.Stamp stamp = new ResourceStore.Stamp(String.valueOf(change.get("createdAt")), session, agent);
        String forcedId = change.get("forcedId") == null ? null : String.valueOf(change.get("forcedId"));
        return new Run(vault, false, stamp, true, "approved", forcedId, channel);
    }

    private static Object applyChange(Run run, Map<String, Object> change) {
        Map<String, String> args = new LinkedHashMap<>();
        if (change.get("args") instanceof Map<?, ?> a) {
            for (Map.Entry<?, ?> e : a.entrySet()) args.put(String.valueOf(e.getKey()), String.valueOf(e.getValue()));
        }
        List<String> positional = new ArrayList<>();
        if (change.get("positional") instanceof List<?> l) {
            for (Object o : l) positional.add(String.valueOf(o));
        }
        String command = String.valueOf(change.get("command"));
        return switch (command) {
            case "save-resource" -> save(run, args);
            case "delete-resource" -> delete(run, positional.get(0));
            case "restore-resource" -> restore(run, positional.get(0));
            case "set-resource-ref" -> refImpl(run, positional.get(0), positional.get(1), isTrue(args.get("remove")), isTrue(args.get("with-skills")), args);
            default -> throw new AisException(500, "알 수 없는 대기 변경 명령이다: " + command);
        };
    }

    private static Object resolve(Path vault, ParsedArgs parsed, boolean ai) {
        requireHuman(ai, "승인 안 된 직접 수정의 처리");
        String key = onlyId(parsed, RESOLVE_USAGE);
        String action = parsed.namedOptions().get("action");
        if (action == null || !(action.trim().equals("approve") || action.trim().equals("revert"))) {
            throw new AisException(400, "action 은 approve 나 revert 여야 한다: " + RESOLVE_USAGE);
        }
        boolean approve = action.trim().equals("approve");
        Map<String, Object> out = new LinkedHashMap<>();
        out.put("key", key);
        out.put("action", action.trim());
        if (key.startsWith("refs:")) {
            String project = key.substring(5);
            ResourceRefs.State st = ResourceRefs.state(vault, project);
            if (st == ResourceRefs.State.APPROVED) {
                throw new AisException(409, "승인 안 된 변경이 없다: " + key);
            }
            if (approve) {
                Approvals.record(vault, key, ResourceRefs.liveBytes(vault, project), "adopted");
            } else if (st == ResourceRefs.State.DIRTY) {
                byte[] copy = Approvals.copy(vault, key);
                ResourceStore.restoreRefsBytes(vault, project, copy == null ? new byte[0] : copy);
            } else {
                ResourceRefs.write(vault, project, List.of());
            }
            return out;
        }
        Resource live = ResourceStore.find(vault, key);
        String state = ResourceStore.approvalState(vault, live);
        if ("approved".equals(state)) {
            throw new AisException(409, "승인 안 된 변경이 없다: " + key);
        }
        if (approve) {
            Approvals.record(vault, Approvals.resKey(live.id()), ResourceStore.liveBytes(vault, live), "adopted");
        } else if ("dirty".equals(state)) {
            ResourceStore.revertToApproved(vault, live);
        } else {
            ResourceStore.delete(vault, live);
            Approvals.forget(vault, Approvals.resKey(live.id()));
        }
        return out;
    }

    private static Object policyGet(Path vault) {
        Map<String, Object> out = new LinkedHashMap<>();
        out.put("policy", Approvals.policy(vault).toMap());
        out.put("defaults", Approvals.Policy.defaults().toMap());
        List<Object> cats = new ArrayList<>();
        for (Approvals.Category c : Approvals.Category.values()) {
            Map<String, Object> m = new LinkedHashMap<>();
            m.put("category", c.code());
            m.put("label", c.label());
            m.put("fixed", c.fixed());
            cats.add(m);
        }
        out.put("categories", cats);
        return out;
    }

    private static Object policySet(Path vault, ParsedArgs parsed, boolean ai) {
        requireHuman(ai, "승인 정책 변경");
        Approvals.Policy p = Approvals.policy(vault);
        boolean any = false;
        for (String key : Approvals.Policy.KEYS) {
            String v = parsed.namedOptions().get(key);
            if (v == null) continue;
            p = p.with(key, Trim.trim(v));
            any = true;
        }
        if (!any) throw new AisException(400, "바꿀 칸이 필요하다: " + POLICY_SET_USAGE);
        Approvals.setPolicy(vault, p);
        return policyGet(vault);
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
