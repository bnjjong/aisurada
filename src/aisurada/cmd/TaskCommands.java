package aisurada.cmd;

import aisurada.args.ParsedArgs;
import aisurada.cli.AisException;
import aisurada.engine.WritePipeline;
import aisurada.task.Task;
import aisurada.task.TaskFormat;
import aisurada.task.TaskStatus;
import aisurada.task.TaskStore;
import aisurada.task.TaskTransitions;
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
import java.util.Locale;
import java.util.Map;
import java.util.Set;

/**
 * 태스크(티켓)를 다루는 명령 9개. <b>외부 설명서에 없는 확장</b>입니다. MCP에는 읽기 도구(list_tasks, get_task)만 열고,
 * 상태를 바꾸는 일은 사람(화면)과 러너만 합니다.
 *
 * <p>태스크는 키({@code TSK-14})나 ID({@code tk…})로 부릅니다. 완료 기준은 만들 때 반드시 적어야 하고(일이 끝났는지 확인하는 기준이다),
 * 상태 전이의 규칙과 조건은 {@code transition-task}가 한 곳에서 검사합니다({@link TaskTransitions}).
 */
public final class TaskCommands {

    public static final String CREATE_USAGE = "create-task title=<요약> repo=<owner/name> criteria=<완료 기준 JSON 배열. 하나 이상. 예: [\"…\",{\"text\":\"…\"}]> "
            + "[type=feature|bug|chore] [priority=high|medium|low] [project=<프로젝트 slug>] [base=<기준 브랜치>] [body=<설명>] "
            + "[labels=<JSON 배열>] [refs=<연결한 자원 JSON 배열. 예: [{\"kind\":\"note\",\"target\":\"nd…\"},{\"kind\":\"github\",\"target\":\"owner/name#53\"}]>] [actor=human|runner]";
    public static final String GET_USAGE = "get-task <키(TSK-14) 또는 id> [activity=<최근 활동 개수. 기본 100. 0이면 전부>]";
    public static final String LIST_USAGE = "list-tasks [status=<상태>] [type=feature|bug|chore] [repo=<owner/name>] [label=<라벨>] [q=<요약에서 찾을 글>] "
            + "[blocked=true. 다른 태스크에 막힌 것만] [bodies=true. 설명까지 싣는다] [trash=true. 휴지통 목록]";
    public static final String UPDATE_USAGE = "update-task <키 또는 id> f1=<직전에 읽은 토큰> [title=<요약>] [type=] [priority=] [repo=<owner/name>] [project=<프로젝트 slug. 빈 값이면 지운다>] "
            + "[base=<기준 브랜치. 빈 값이면 지운다>] [body=<설명>] [labels=<JSON 배열. 전체 교체>] [criteria=<JSON 배열. 전체 교체>] [refs=<JSON 배열. 전체 교체>] [actor=human|runner]";
    public static final String TRANSITION_USAGE = "transition-task <키 또는 id> to=<상태: draft|ready-for-develop|in-progress|waiting-answer|in-review|done|failed|cancelled> "
            + "[actor=human|runner. 기본 human] [from=<기대하는 지금 상태. 다르면 거절한다>]";
    public static final String LINK_USAGE = "link-tasks <건 쪽 키> <받는 쪽 키> type=blocks|relates|duplicates [remove=true] [actor=human|runner]";
    public static final String COMMENT_USAGE = "add-task-comment <키 또는 id> body=<댓글> [actor=human|runner]";
    public static final String DELETE_USAGE = "delete-task <키 또는 id>";
    public static final String RESTORE_USAGE = "restore-task <휴지통의 태스크 키 또는 id. list-tasks trash=true 로 조회한다>";

    public static final Set<String> NAMES = Set.of("create-task", "get-task", "list-tasks", "update-task", "transition-task", "link-tasks",
            "add-task-comment", "delete-task", "restore-task");

    private static final DateTimeFormatter STAMP = DateTimeFormatter.ISO_OFFSET_DATE_TIME;
    private static final int DEFAULT_ACTIVITY = 100;

    private TaskCommands() {}

    public static Object execute(Path vault, ParsedArgs parsed, String sessionId) {
        WorkspaceMarker.check(vault);
        return switch (parsed.commandName()) {
            case "create-task" -> create(vault, parsed.namedOptions(), sessionId);
            case "get-task" -> get(vault, parsed);
            case "list-tasks" -> list(vault, parsed.namedOptions());
            case "update-task" -> update(vault, parsed, sessionId);
            case "transition-task" -> transition(vault, parsed, sessionId);
            case "link-tasks" -> link(vault, parsed, sessionId);
            case "add-task-comment" -> comment(vault, parsed, sessionId);
            case "delete-task" -> delete(vault, parsed);
            case "restore-task" -> restore(vault, parsed);
            default -> throw new AisException(400, "모르는 명령이다: " + parsed.commandName());
        };
    }

    // ------------------------------------------------------------------ 만들기·고치기

    private static Object create(Path vault, Map<String, String> opts, String sessionId) {
        TaskStore.Fields f = fields(opts);
        Task t = TaskStore.create(vault, f, stamp(sessionId), actorOf(opts));
        TaskStore.Scan scan = TaskStore.scan(vault);
        Map<String, Object> m = detail(vault, scan, scan.byKey(t.key()).orElse(t), 0);
        m.put("created", true);
        return m;
    }

    private static Object update(Path vault, ParsedArgs parsed, String sessionId) {
        Task cur = TaskStore.find(vault, onlyRef(parsed, UPDATE_USAGE));
        Map<String, String> opts = parsed.namedOptions();
        Task next = TaskStore.update(vault, cur, fields(opts), opts.get("f1"), stamp(sessionId), actorOf(opts));
        TaskStore.Scan scan = TaskStore.scan(vault);
        Map<String, Object> m = detail(vault, scan, scan.byKey(next.key()).orElse(next), 0);
        m.put("changed", next != cur);
        return m;
    }

    private static TaskStore.Fields fields(Map<String, String> opts) {
        return new TaskStore.Fields(
                opts.containsKey("title") ? TaskFormat.title(opts.get("title")) : null,
                opts.containsKey("type") ? TaskFormat.type(opts.get("type")) : null,
                opts.containsKey("priority") ? TaskFormat.priority(opts.get("priority")) : null,
                opts.containsKey("repo") ? orEmpty(TaskFormat.repo(opts.get("repo"))) : null,
                opts.containsKey("project") ? Trim.trim(opts.get("project")) : null,
                opts.containsKey("base") ? orEmpty(TaskFormat.baseBranch(opts.get("base"))) : null,
                opts.containsKey("body") ? TaskFormat.body(opts.get("body")) : null,
                opts.containsKey("labels") ? TaskFormat.labels(opts.get("labels")) : null,
                opts.containsKey("criteria") ? TaskFormat.criteria(opts.get("criteria")) : null,
                opts.containsKey("refs") ? TaskFormat.refs(opts.get("refs")) : null);
    }

    private static String orEmpty(String s) {
        return s == null ? "" : s;
    }

    // ------------------------------------------------------------------ 상태·연결·댓글

    private static Object transition(Path vault, ParsedArgs parsed, String sessionId) {
        Task cur = TaskStore.find(vault, onlyRef(parsed, TRANSITION_USAGE));
        Map<String, String> opts = parsed.namedOptions();
        String toRaw = opts.get("to");
        if (toRaw == null || Trim.isBlank(toRaw)) {
            throw new AisException(400, "to 가 필요하다: " + TRANSITION_USAGE);
        }
        TaskStatus to = TaskStatus.parse(toRaw);
        if (to == null) {
            throw new AisException(400, "to 는 상태 이름이어야 한다(draft, ready-for-develop, in-progress, waiting-answer, in-review, done, failed, cancelled): " + toRaw);
        }
        String from = opts.containsKey("from") ? Trim.trim(opts.get("from")) : null;
        Task next = TaskStore.transition(vault, cur, to, actorOf(opts), from, stamp(sessionId));
        TaskStore.Scan scan = TaskStore.scan(vault);
        Map<String, Object> m = detail(vault, scan, scan.byKey(next.key()).orElse(next), 0);
        m.put("from", cur.status().label());
        return m;
    }

    private static Object link(Path vault, ParsedArgs parsed, String sessionId) {
        List<String> pos = parsed.positionalArgs();
        if (pos.size() != 2) {
            throw new AisException(400, "link-tasks 는 건 쪽 키와 받는 쪽 키를 받는다: " + LINK_USAGE);
        }
        Map<String, String> opts = parsed.namedOptions();
        String type = opts.get("type");
        if (type == null || Trim.isBlank(type)) {
            throw new AisException(400, "type 이 필요하다(blocks, relates, duplicates): " + LINK_USAGE);
        }
        TaskStore.Scan scan = TaskStore.scan(vault);
        Task from = TaskStore.find(scan, pos.get(0));
        Task to = TaskStore.find(scan, pos.get(1));
        boolean remove = isTrue(opts.get("remove"));
        Task changed = TaskStore.link(vault, from, to, Trim.trim(type).toLowerCase(Locale.ROOT), remove, stamp(sessionId), actorOf(opts));
        TaskStore.Scan after = TaskStore.scan(vault);
        Map<String, Object> out = new LinkedHashMap<>();
        out.put("from", from.key());
        out.put("to", to.key());
        out.put("type", Trim.trim(type).toLowerCase(Locale.ROOT));
        out.put("removed", remove);
        out.put("changed", changed != from);
        out.put("links", relatedRows(after, after.byKey(from.key()).orElse(from)));
        return out;
    }

    private static Object comment(Path vault, ParsedArgs parsed, String sessionId) {
        Task t = TaskStore.find(vault, onlyRef(parsed, COMMENT_USAGE));
        Map<String, String> opts = parsed.namedOptions();
        String body = TaskFormat.comment(opts.get("body"));
        Map<String, Object> ev = TaskStore.comment(vault, t, body, stamp(sessionId), actorOf(opts));
        Map<String, Object> out = new LinkedHashMap<>();
        out.put("key", t.key());
        out.put("comment", ev);
        return out;
    }

    // ------------------------------------------------------------------ 목록·보기

    private static Object list(Path vault, Map<String, String> opts) {
        Map<String, Object> out = new LinkedHashMap<>();
        if (isTrue(opts.get("trash"))) {
            List<Object> rows = new ArrayList<>();
            for (TaskStore.TrashEntry e : TaskStore.trash(vault)) {
                Map<String, Object> m = new LinkedHashMap<>();
                m.put("key", e.key());
                m.put("id", e.id());
                m.put("title", e.title());
                m.put("path", e.relPath());
                m.put("deletedAt", e.deletedAt());
                rows.add(m);
            }
            out.put("trash", rows);
            return out;
        }
        TaskStatus status = null;
        if (opts.containsKey("status") && !Trim.isBlank(opts.get("status"))) {
            status = TaskStatus.parse(opts.get("status"));
            if (status == null) throw new AisException(400, "status 는 상태 이름이어야 한다: " + opts.get("status"));
        }
        String type = opts.containsKey("type") && !Trim.isBlank(opts.get("type")) ? TaskFormat.type(opts.get("type")) : null;
        String repo = opts.containsKey("repo") && !Trim.isBlank(opts.get("repo")) ? TaskFormat.repo(opts.get("repo")) : null;
        String label = opts.containsKey("label") ? Trim.trim(opts.get("label")) : null;
        String q = opts.containsKey("q") ? Trim.trim(opts.get("q")).toLowerCase(Locale.ROOT) : null;
        boolean onlyBlocked = isTrue(opts.get("blocked"));
        boolean bodies = isTrue(opts.get("bodies"));

        TaskStore.Scan scan = TaskStore.scan(vault);
        List<Task> items = new ArrayList<>();
        for (Task t : scan.items()) {
            if (status != null && t.status() != status) continue;
            if (type != null && !t.type().equals(type)) continue;
            if (repo != null && !repo.equals(t.repo())) continue;
            if (label != null && !label.isEmpty() && !t.labels().contains(label)) continue;
            if (q != null && !q.isEmpty() && !t.title().toLowerCase(Locale.ROOT).contains(q) && !t.key().toLowerCase(Locale.ROOT).contains(q)) continue;
            if (onlyBlocked && TaskStore.unfinishedBlockers(scan, t.key()).isEmpty()) continue;
            items.add(t);
        }
        items.sort(Comparator.comparingInt(Task::number).reversed());
        List<Object> rows = new ArrayList<>();
        for (Task t : items) rows.add(row(scan, t, bodies));
        out.put("tasks", rows);
        if (!scan.skipped().isEmpty()) out.put("skipped", scan.skipped());
        return out;
    }

    private static Object get(Path vault, ParsedArgs parsed) {
        Task t = TaskStore.find(vault, onlyRef(parsed, GET_USAGE));
        int activity = DEFAULT_ACTIVITY;
        String a = parsed.namedOptions().get("activity");
        if (a != null && !Trim.isBlank(a)) {
            try {
                activity = Integer.parseInt(Trim.trim(a));
            } catch (NumberFormatException e) {
                throw new AisException(400, "activity 는 0 이상의 정수여야 한다: " + a);
            }
            if (activity < 0) throw new AisException(400, "activity 는 0 이상의 정수여야 한다: " + a);
        }
        TaskStore.Scan scan = TaskStore.scan(vault);
        return detail(vault, scan, scan.byKey(t.key()).orElse(t), activity);
    }

    // ------------------------------------------------------------------ 지우기·되살리기

    private static Object delete(Path vault, ParsedArgs parsed) {
        TaskStore.Scan scan = TaskStore.scan(vault);
        Task t = TaskStore.find(scan, onlyRef(parsed, DELETE_USAGE));
        List<String> blocking = new ArrayList<>();
        for (Task o : scan.items()) {
            if (o.links().stream().anyMatch(l -> l.type().equals("blocks") && l.to().equals(t.key()))) blocking.add(o.key());
        }
        List<String> blocked = new ArrayList<>();
        for (Task.Link l : t.links()) {
            if (l.type().equals("blocks")) blocked.add(l.to());
        }
        TaskStore.delete(vault, t);
        Map<String, Object> m = new LinkedHashMap<>();
        m.put("key", t.key());
        m.put("id", t.id());
        m.put("title", t.title());
        m.put("trashed", true);
        // 지운 태스크가 막고 있던 태스크는 이제 막히지 않는다. 되살리면 다시 막는다
        m.put("wasBlocking", blocked);
        m.put("wasBlockedBy", blocking);
        return m;
    }

    private static Object restore(Path vault, ParsedArgs parsed) {
        Task t = TaskStore.restore(vault, onlyRef(parsed, RESTORE_USAGE));
        TaskStore.Scan scan = TaskStore.scan(vault);
        Map<String, Object> m = detail(vault, scan, scan.byKey(t.key()).orElse(t), 0);
        m.put("restored", true);
        return m;
    }

    // ------------------------------------------------------------------ 응답 모양

    /** 목록 행. 설명과 활동은 {@code bodies}일 때만(설명) 싣는다. */
    private static Map<String, Object> row(TaskStore.Scan scan, Task t, boolean withBody) {
        Map<String, Object> m = new LinkedHashMap<>();
        m.put("key", t.key());
        m.put("id", t.id());
        m.put("title", t.title());
        m.put("type", t.type());
        m.put("priority", t.priority());
        m.put("status", t.status().label());
        m.put("statusTitle", t.status().title());
        if (t.repo() != null) m.put("repo", t.repo());
        if (t.project() != null) m.put("project", t.project());
        if (t.baseBranch() != null) m.put("baseBranch", t.baseBranch());
        m.put("labels", t.labels());
        m.put("criteriaDone", (long) t.criteria().stream().filter(Task.Criterion::done).count());
        m.put("criteriaTotal", (long) t.criteria().size());
        m.put("blockedBy", TaskStore.unfinishedBlockers(scan, t.key()));
        List<String> blocks = new ArrayList<>();
        for (Task.Link l : t.links()) {
            if (l.type().equals("blocks") && scan.byKey(l.to()).isPresent()) blocks.add(l.to());
        }
        m.put("blocks", blocks);
        if (t.createdAt() != null) m.put("createdAt", t.createdAt());
        if (t.editedAt() != null) m.put("editedAt", t.editedAt());
        if (t.editedSession() != null) m.put("editedSession", t.editedSession());
        if (t.editedAgent() != null) m.put("editedAgent", t.editedAgent());
        m.put("path", t.relPath());
        if (withBody) m.put("body", t.body());
        return m;
    }

    /** 상세. 설명, 완료 기준, 자원, 연결, Ready 조건, 사람이 바꿀 수 있는 상태, 활동, 고칠 때 넘길 토큰을 모두 싣는다. */
    private static Map<String, Object> detail(Path vault, TaskStore.Scan scan, Task t, int activityLimit) {
        Map<String, Object> m = row(scan, t, true);
        List<Object> criteria = new ArrayList<>();
        for (Task.Criterion c : t.criteria()) {
            Map<String, Object> r = new LinkedHashMap<>();
            r.put("text", c.text());
            r.put("done", c.done());
            if (c.checkedAt() != null) r.put("checkedAt", c.checkedAt());
            if (c.checkedBy() != null) r.put("checkedBy", c.checkedBy());
            criteria.add(r);
        }
        m.put("criteria", criteria);
        List<Object> refs = new ArrayList<>();
        for (Task.Ref r0 : t.refs()) {
            Map<String, Object> r = new LinkedHashMap<>();
            r.put("kind", r0.kind());
            r.put("target", r0.target());
            if (r0.title() != null) r.put("title", r0.title());
            refs.add(r);
        }
        m.put("refs", refs);
        m.put("links", relatedRows(scan, t));
        List<Object> readiness = new ArrayList<>();
        boolean ready = true;
        for (TaskTransitions.Check c : TaskTransitions.readiness(t)) {
            Map<String, Object> r = new LinkedHashMap<>();
            r.put("item", c.item());
            r.put("ok", c.ok());
            r.put("label", c.label());
            readiness.add(r);
            ready &= c.ok();
        }
        m.put("readiness", readiness);
        m.put("ready", ready);
        List<Object> options = new ArrayList<>();
        for (TaskTransitions.Option o : TaskTransitions.options(t, TaskTransitions.HUMAN, TaskStore.unfinishedBlockers(scan, t.key()))) {
            Map<String, Object> r = new LinkedHashMap<>();
            r.put("to", o.to().label());
            r.put("title", o.to().title());
            r.put("ok", o.ok());
            if (o.reason() != null) r.put("reason", o.reason());
            options.add(r);
        }
        m.put("transitions", options);
        if (activityLimit >= 0) {
            m.put("activity", new ArrayList<Object>(TaskStore.activity(vault, t.key(), activityLimit == 0 ? 0 : activityLimit)));
        }
        if (t.token() != null) m.put("precondition", t.token());
        return m;
    }

    private static List<Object> relatedRows(TaskStore.Scan scan, Task t) {
        List<Object> rows = new ArrayList<>();
        for (TaskStore.Related r : TaskStore.related(scan, t)) {
            Map<String, Object> m = new LinkedHashMap<>();
            m.put("type", r.type());
            m.put("key", r.key());
            m.put("title", r.title());
            m.put("status", r.status().label());
            rows.add(m);
        }
        return rows;
    }

    // ------------------------------------------------------------------ 공통

    private static boolean isTrue(String v) {
        return v != null && "true".equals(Trim.trim(v).toLowerCase(Locale.ROOT));
    }

    private static String onlyRef(ParsedArgs parsed, String usage) {
        List<String> pos = parsed.positionalArgs();
        if (pos.size() != 1 || Trim.isBlank(pos.get(0))) {
            throw new AisException(400, "인자가 하나 필요하다: " + usage);
        }
        return pos.get(0).trim();
    }

    private static String actorOf(Map<String, String> opts) {
        String a = opts.get("actor");
        if (a == null || Trim.isBlank(a)) return TaskTransitions.HUMAN;
        String v = Trim.trim(a).toLowerCase(Locale.ROOT);
        if (!TaskTransitions.ACTORS.contains(v)) {
            throw new AisException(400, "actor 는 human 이나 runner 여야 한다: " + a);
        }
        return v;
    }

    /** 고친 기록. AI 세션이 고쳤으면 세션 ID와(Claude가 아니면) CLI 이름이 남고, 사람이 고쳤으면 시각만 남는다. */
    private static TaskStore.Stamp stamp(String sessionId) {
        OffsetDateTime now = TestApparatus.nowInstant().atZone(ZoneId.systemDefault()).toOffsetDateTime().withNano(0);
        String session = sessionId == null || sessionId.isBlank() ? null : sessionId;
        String agent = null;
        if (session != null) {
            String a = WritePipeline.currentSessionAgent();
            if (a != null && !"claude".equals(a)) agent = a;
        }
        return new TaskStore.Stamp(STAMP.format(now), session, agent);
    }
}
