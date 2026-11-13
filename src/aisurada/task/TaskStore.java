package aisurada.task;

import aisurada.args.JsonCompact;
import aisurada.args.JsonReader;
import aisurada.args.JsonValue;
import aisurada.cli.AisException;
import aisurada.resource.ResourceStore;
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
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Set;

/**
 * 태스크 파일을 vault에서 읽고 쓰는 곳. <b>외부 설명서에 없는 확장</b>입니다.
 *
 * <pre>
 * .aisurada/tasks/
 *   TSK-14.md                  태스크(머리말 + 설명)
 *   TSK-14/activity.jsonl      활동(만들기, 상태 바뀜, 고침, 연결, 댓글). 한 줄에 사건 하나, 최근 500줄까지
 *   .trash/&lt;id&gt;/             지운 태스크
 * </pre>
 *
 * <p>스킬·에이전트 자원({@code .aisurada/resources/})처럼 설명서가 정한 {@code notes/}와 {@code .aisurada/projects/} 등을 건드리지 않는
 * 새 영역이고, 노트를 훑는 명령은 이 영역을 보지 않습니다. 연결은 건 쪽 파일에만 적고 반대쪽(막혀 있음 따위)은 읽을 때 계산하므로,
 * 두 파일이 어긋날 일이 없습니다. 실행 기록(러너가 만드는 로그)은 vault가 아니라 앱 폴더에 둡니다.
 */
public final class TaskStore {

    public static final String ROOT = ".aisurada/tasks";
    public static final int ACTIVITY_KEEP = 500;

    private TaskStore() {}

    /** 훑은 결과. {@code skipped}는 못 읽어 건너뛴 파일과 이유다. */
    public record Scan(List<Task> items, List<String> skipped) {
        public Optional<Task> byKey(String key) {
            for (Task t : items) {
                if (t.key().equals(key)) return Optional.of(t);
            }
            return Optional.empty();
        }

        public Optional<Task> byId(String id) {
            for (Task t : items) {
                if (t.id().equals(id)) return Optional.of(t);
            }
            return Optional.empty();
        }
    }

    /**
     * 고칠 값. null은 「안 줌(그대로 둔다)」이다. {@code repo}, {@code project}, {@code baseBranch}는 빈 글("")이 「지운다」이고,
     * 목록 칸({@code labels}, {@code criteria}, {@code refs})은 빈 목록이 「비운다」다.
     */
    public record Fields(String title, String type, String priority, String repo, String project, String baseBranch, String body,
                         List<String> labels, List<Task.Criterion> criteria, List<Task.Ref> refs) {}

    /** 누가 언제 고쳤는지의 기록. 사람이 고쳤으면 session은 null이다. */
    public record Stamp(String at, String session, String agent) {}

    /** 지운 태스크 한 줄. */
    public record TrashEntry(String id, String key, String title, String relPath, String deletedAt) {}

    /** 이 태스크에 걸린 연결 한 줄(건 쪽과 받은 쪽을 모두 담는다). {@code type}은 {@code blocks}, {@code blocked-by}, {@code relates}, {@code duplicates}, {@code duplicated-by}. */
    public record Related(String type, String key, String title, TaskStatus status) {}

    // ------------------------------------------------------------------ 경로

    public static Path root(Path vault) {
        return vault.resolve(ROOT);
    }

    private static Path filePath(Path vault, String key) {
        return root(vault).resolve(key + ".md");
    }

    private static Path activityPath(Path vault, String key) {
        return root(vault).resolve(key).resolve("activity.jsonl");
    }

    // ------------------------------------------------------------------ 읽기

    /** 모든 태스크를 훑는다. 못 읽는 파일은 건너뛰고 이유를 {@code skipped}에 적는다. 점으로 시작하는 폴더({@code .trash})는 안 본다. */
    public static Scan scan(Path vault) {
        List<Task> items = new ArrayList<>();
        List<String> skipped = new ArrayList<>();
        Set<String> ids = new HashSet<>();
        Set<String> keys = new HashSet<>();
        for (Path file : sortedEntries(root(vault))) {
            String name = file.getFileName().toString();
            if (name.startsWith(".") || !name.endsWith(".md") || !Files.isRegularFile(file)) continue;
            String rel = TaskFormat.rel(vault, file);
            try {
                if (Files.size(file) > TaskFormat.MAX_FILE_BYTES) {
                    skipped.add(rel + ": 파일이 너무 크다");
                    continue;
                }
                Task t = TaskFormat.parse(Files.readAllBytes(file), rel);
                if (!name.equals(t.key() + ".md")) {
                    skipped.add(rel + ": 파일 이름과 key(" + t.key() + ")가 다르다");
                    continue;
                }
                if (!ids.add(t.id())) {
                    skipped.add(rel + ": id 가 다른 태스크와 겹친다: " + t.id());
                    continue;
                }
                if (!keys.add(t.key())) {
                    skipped.add(rel + ": key 가 다른 태스크와 겹친다: " + t.key());
                    continue;
                }
                items.add(t);
            } catch (IllegalArgumentException | IOException e) {
                skipped.add(rel + ": " + e.getMessage());
            }
        }
        items.sort(Comparator.comparingInt(Task::number));
        return new Scan(List.copyOf(items), List.copyOf(skipped));
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

    /** 키({@code TSK-14})나 ID({@code tk…})로 태스크 하나를 찾는다. 없으면 AIS-404다. */
    public static Task find(Path vault, String ref) {
        return find(scan(vault), ref);
    }

    /** 훑어 둔 결과에서 찾는다. */
    public static Task find(Scan scan, String ref) {
        if (ref == null || ref.isBlank()) {
            throw new AisException(400, "태스크 키(TSK-14)나 id(tk…)가 필요하다");
        }
        String r = ref.trim();
        if (TaskFormat.KEY.matcher(r).matches()) {
            return scan.byKey(r).orElseThrow(() -> new AisException(404, "태스크를 못 찾았다: " + r));
        }
        if (TaskFormat.ID.matcher(r).matches()) {
            return scan.byId(r).orElseThrow(() -> new AisException(404, "태스크를 못 찾았다: " + r));
        }
        throw new AisException(400, "태스크 키(TSK-14)나 id(tk…) 모양이 아니다: " + ref);
    }

    /** 이 태스크를 막고 있으면서 아직 Done이 아닌 태스크 키들. 비면 막히지 않았다. */
    public static List<String> unfinishedBlockers(Scan scan, String key) {
        List<String> out = new ArrayList<>();
        for (Task t : scan.items()) {
            if (t.status() == TaskStatus.DONE) continue;
            for (Task.Link l : t.links()) {
                if (l.type().equals("blocks") && l.to().equals(key)) {
                    out.add(t.key());
                    break;
                }
            }
        }
        return out;
    }

    /** 이 태스크에 걸린 모든 연결. 이 태스크가 건 것과, 다른 태스크가 이 태스크에 건 것(반대쪽 이름)을 함께 낸다. 없는 태스크를 가리키면 뺀다. */
    public static List<Related> related(Scan scan, Task t) {
        List<Related> out = new ArrayList<>();
        for (Task.Link l : t.links()) {
            scan.byKey(l.to()).ifPresent(o -> out.add(new Related(l.type(), o.key(), o.title(), o.status())));
        }
        for (Task o : scan.items()) {
            if (o.key().equals(t.key())) continue;
            for (Task.Link l : o.links()) {
                if (!l.to().equals(t.key())) continue;
                String inverse = switch (l.type()) {
                    case "blocks" -> "blocked-by";
                    case "duplicates" -> "duplicated-by";
                    default -> "relates";
                };
                if (inverse.equals("relates") && t.links().stream().anyMatch(x -> x.type().equals("relates") && x.to().equals(o.key()))) continue;
                out.add(new Related(inverse, o.key(), o.title(), o.status()));
            }
        }
        out.sort(Comparator.comparing(Related::type).thenComparing(Related::key));
        return out;
    }

    // ------------------------------------------------------------------ 만들기

    /**
     * 새 태스크를 Draft로 만든다. 요약(title), 저장소(repo), 완료 기준(criteria 1개 이상)이 필수다.
     * 완료 기준은 일이 끝났는지 확인하는 기준이라 처음부터 적어야 한다(사용자 결정, 2026-11-13).
     */
    public static Task create(Path vault, Fields f, Stamp stamp, String actor) {
        if (f.title() == null) throw new AisException(400, "title 이 필요하다");
        if (f.repo() == null || f.repo().isEmpty()) throw new AisException(400, "repo 가 필요하다(owner/name)");
        if (f.criteria() == null || f.criteria().isEmpty()) {
            throw new AisException(400, "완료 기준(criteria)을 하나 이상 써야 만들 수 있다. 일이 끝났는지 이 기준으로 확인한다");
        }
        String project = f.project() == null || f.project().isEmpty() ? null : f.project();
        if (project != null) ResourceStore.requireProject(vault, project);

        Scan scan = scan(vault);
        int number = nextNumber(vault, scan);
        String key = "TSK-" + number;
        String id;
        do {
            id = IdGenerator.nextId("tk");
        } while (scan.byId(id).isPresent());

        Path path = filePath(vault, key);
        Task t = new Task(key, id, f.title(), f.type() == null ? TaskFormat.DEFAULT_TYPE : f.type(),
                f.priority() == null ? TaskFormat.DEFAULT_PRIORITY : f.priority(), TaskStatus.DRAFT, f.repo(), project,
                f.baseBranch() == null || f.baseBranch().isEmpty() ? null : f.baseBranch(),
                f.labels() == null ? List.of() : f.labels(), normalizeCriteria(List.of(), f.criteria(), stamp, actor), List.of(),
                f.refs() == null ? List.of() : f.refs(), f.body() == null ? "" : f.body(), Map.of(), stamp.at(), stamp.at(),
                stamp.session(), stamp.agent(), TaskFormat.rel(vault, path), null);
        byte[] bytes = TaskFormat.render(t);
        AtomicWriter.writeAtomic(path, bytes);
        Map<String, Object> ev = event(stamp, actor, "created");
        appendActivity(vault, key, ev);
        return t.withToken(DocToken.compute(bytes));
    }

    /** 다음 번호. 지금 있는 태스크와 휴지통에 있는 태스크의 가장 큰 번호 + 1이다(지운 번호를 다시 쓰지 않는다). */
    private static int nextNumber(Path vault, Scan scan) {
        int max = 0;
        for (Task t : scan.items()) max = Math.max(max, t.number());
        for (TrashEntry e : trash(vault)) max = Math.max(max, TaskFormat.numberOf(e.key()));
        return max + 1;
    }

    // ------------------------------------------------------------------ 고치기

    /**
     * 태스크의 내용을 고친다. 직전에 읽은 {@code f1} 토큰이 지금 파일과 맞아야 쓴다(맞지 않으면 AIS-412). 실제로 달라지는 게 없으면 파일도
     * 활동도 안 바꾼다. 상태와 연결은 여기서 못 바꾼다({@link #transition}, {@link #link}).
     */
    public static Task update(Path vault, Task cur, Fields f, String f1, Stamp stamp, String actor) {
        if (f1 == null || f1.isBlank()) {
            throw new AisException(412, "f1 이 필요하다. get-task 로 읽은 precondition 을 f1= 으로 넘긴다: " + cur.relPath());
        }
        if (!f1.trim().equals(cur.token())) {
            throw new AisException(412, "읽은 뒤에 파일이 바뀌었다. 다시 읽고 고친다: " + cur.relPath());
        }
        String title = f.title() != null ? f.title() : cur.title();
        String type = f.type() != null ? f.type() : cur.type();
        String priority = f.priority() != null ? f.priority() : cur.priority();
        String repo = f.repo() != null ? (f.repo().isEmpty() ? null : f.repo()) : cur.repo();
        String project = f.project() != null ? (f.project().isEmpty() ? null : f.project()) : cur.project();
        String base = f.baseBranch() != null ? (f.baseBranch().isEmpty() ? null : f.baseBranch()) : cur.baseBranch();
        String body = f.body() != null ? f.body() : cur.body();
        List<String> labels = f.labels() != null ? f.labels() : cur.labels();
        List<Task.Criterion> criteria = f.criteria() != null ? normalizeCriteria(cur.criteria(), f.criteria(), stamp, actor) : cur.criteria();
        List<Task.Ref> refs = f.refs() != null ? f.refs() : cur.refs();
        if (project != null && !project.equals(cur.project())) ResourceStore.requireProject(vault, project);

        Task same = new Task(cur.key(), cur.id(), title, type, priority, cur.status(), repo, project, base, labels, criteria, cur.links(), refs,
                body, cur.extra(), cur.createdAt(), cur.editedAt(), cur.editedSession(), cur.editedAgent(), cur.relPath(), cur.token());
        if (java.util.Arrays.equals(TaskFormat.render(same), TaskFormat.render(cur))) {
            return cur;
        }
        Task next = new Task(cur.key(), cur.id(), title, type, priority, cur.status(), repo, project, base, labels, criteria, cur.links(), refs,
                body, cur.extra(), cur.createdAt(), stamp.at(), stamp.session(), stamp.agent(), cur.relPath(), null);
        byte[] bytes = TaskFormat.render(next);
        AtomicWriter.writeAtomic(vault.resolve(cur.relPath()), bytes);

        List<Object> changed = new ArrayList<>();
        if (!title.equals(cur.title())) changed.add("title");
        if (!type.equals(cur.type())) changed.add("type");
        if (!priority.equals(cur.priority())) changed.add("priority");
        if (!java.util.Objects.equals(repo, cur.repo())) changed.add("repo");
        if (!java.util.Objects.equals(project, cur.project())) changed.add("project");
        if (!java.util.Objects.equals(base, cur.baseBranch())) changed.add("baseBranch");
        if (!body.equals(cur.body())) changed.add("body");
        if (!labels.equals(cur.labels())) changed.add("labels");
        if (!criteria.equals(cur.criteria())) changed.add("criteria");
        if (!refs.equals(cur.refs())) changed.add("refs");
        Map<String, Object> ev = event(stamp, actor, "edited");
        ev.put("fields", changed);
        appendActivity(vault, cur.key(), ev);
        return next.withToken(DocToken.compute(bytes));
    }

    /** 완료 기준을 다듬는다. 새로 확인된 항목에는 확인한 시각과 쪽을 적고, 확인이 풀린 항목은 그 기록을 지운다. 이미 확인된 항목은 기록을 그대로 둔다. */
    private static List<Task.Criterion> normalizeCriteria(List<Task.Criterion> old, List<Task.Criterion> given, Stamp stamp, String actor) {
        List<Task.Criterion> out = new ArrayList<>();
        for (Task.Criterion c : given) {
            if (!c.done()) {
                out.add(new Task.Criterion(c.text(), false, null, null));
                continue;
            }
            Task.Criterion before = null;
            for (Task.Criterion o : old) {
                if (o.done() && o.text().equals(c.text())) {
                    before = o;
                    break;
                }
            }
            if (before != null) {
                out.add(before);
            } else {
                out.add(new Task.Criterion(c.text(), true, c.checkedAt() != null ? c.checkedAt() : stamp.at(), c.checkedBy() != null ? c.checkedBy() : actor));
            }
        }
        return List.copyOf(out);
    }

    // ------------------------------------------------------------------ 상태

    /**
     * 상태를 바꾼다. 규칙과 조건은 {@link TaskTransitions}가 검사한다. {@code expectedFrom}을 주면 지금 상태가 그것과 같을 때만 바꾼다(그 사이에
     * 러너나 다른 창이 먼저 바꿨으면 AIS-409).
     */
    public static Task transition(Path vault, Task cur, TaskStatus to, String actor, String expectedFrom, Stamp stamp) {
        if (expectedFrom != null) {
            TaskStatus exp = TaskStatus.parse(expectedFrom);
            if (exp == null) throw new AisException(400, "from 은 상태 이름이어야 한다: " + expectedFrom);
            if (exp != cur.status()) {
                throw new AisException(409, "상태가 이미 바뀌었다: 지금 " + cur.status().label() + ", 기대한 것 " + exp.label());
            }
        }
        Scan scan = scan(vault);
        TaskTransitions.require(cur, to, actor, unfinishedBlockers(scan, cur.key()));
        Task next = new Task(cur.key(), cur.id(), cur.title(), cur.type(), cur.priority(), to, cur.repo(), cur.project(), cur.baseBranch(),
                cur.labels(), cur.criteria(), cur.links(), cur.refs(), cur.body(), cur.extra(), cur.createdAt(), stamp.at(), stamp.session(),
                stamp.agent(), cur.relPath(), null);
        byte[] bytes = TaskFormat.render(next);
        AtomicWriter.writeAtomic(vault.resolve(cur.relPath()), bytes);
        Map<String, Object> ev = event(stamp, actor, "status");
        ev.put("from", cur.status().label());
        ev.put("to", to.label());
        appendActivity(vault, cur.key(), ev);
        return next.withToken(DocToken.compute(bytes));
    }

    // ------------------------------------------------------------------ 연결

    /**
     * 연결을 걸거나 푼다. 연결은 건 쪽({@code from}) 파일에만 적는다. 푼다({@code remove})면 두 방향 가운데 있는 쪽을 지운다.
     * 자기 자신에게 걸 수 없고, 막고 있음({@code blocks})은 서로를 막는 순환을 만들 수 없다. 이미 있는 연결을 다시 걸면 바뀐 게 없다.
     *
     * @return 파일을 고친 태스크. 바뀐 게 없으면 {@code from}
     */
    public static Task link(Path vault, Task from, Task to, String type, boolean remove, Stamp stamp, String actor) {
        if (!TaskFormat.LINK_TYPES.contains(type)) {
            throw new AisException(400, "type 은 " + TaskFormat.LINK_TYPES + " 가운데 하나여야 한다: " + type);
        }
        if (from.key().equals(to.key())) {
            throw new AisException(400, "자기 자신에게는 연결할 수 없다: " + from.key());
        }
        Task.Link forward = new Task.Link(type, to.key());
        Task.Link backward = new Task.Link(type, from.key());
        Task owner;
        List<Task.Link> links;
        String eventKind;
        if (remove) {
            if (from.links().contains(forward)) {
                owner = from;
            } else if (to.links().contains(backward)) {
                owner = to;
            } else {
                return from;
            }
            Task.Link target = owner == from ? forward : backward;
            links = new ArrayList<>(owner.links());
            links.remove(target);
            eventKind = "unlinked";
        } else {
            if (from.links().contains(forward)) return from;
            if (type.equals("relates") && to.links().contains(backward)) return from;
            if (type.equals("blocks") && to.links().contains(new Task.Link("blocks", from.key()))) {
                throw new AisException(400, to.key() + " 가 이미 " + from.key() + " 를 막고 있어 서로 막게 할 수 없다");
            }
            if (from.links().size() >= TaskFormat.MAX_LINKS) {
                throw new AisException(400, "연결은 태스크마다 " + TaskFormat.MAX_LINKS + "개까지다");
            }
            owner = from;
            links = new ArrayList<>(from.links());
            links.add(forward);
            eventKind = "linked";
        }
        Task next = new Task(owner.key(), owner.id(), owner.title(), owner.type(), owner.priority(), owner.status(), owner.repo(), owner.project(),
                owner.baseBranch(), owner.labels(), owner.criteria(), List.copyOf(links), owner.refs(), owner.body(), owner.extra(), owner.createdAt(),
                stamp.at(), stamp.session(), stamp.agent(), owner.relPath(), null);
        byte[] bytes = TaskFormat.render(next);
        AtomicWriter.writeAtomic(vault.resolve(owner.relPath()), bytes);
        for (Task side : List.of(from, to)) {
            Map<String, Object> ev = event(stamp, actor, eventKind);
            ev.put("type", type);
            ev.put("with", side == from ? to.key() : from.key());
            ev.put("direction", side == owner ? "out" : "in");
            appendActivity(vault, side.key(), ev);
        }
        return next.withToken(DocToken.compute(bytes));
    }

    // ------------------------------------------------------------------ 활동

    private static Map<String, Object> event(Stamp stamp, String actor, String kind) {
        Map<String, Object> ev = new LinkedHashMap<>();
        ev.put("at", stamp.at());
        ev.put("kind", kind);
        ev.put("by", actor);
        if (stamp.session() != null) ev.put("session", stamp.session());
        if (stamp.agent() != null) ev.put("agent", stamp.agent());
        return ev;
    }

    /** 댓글을 활동에 남긴다. */
    public static Map<String, Object> comment(Path vault, Task t, String body, Stamp stamp, String actor) {
        Map<String, Object> ev = event(stamp, actor, "comment");
        ev.put("body", body);
        appendActivity(vault, t.key(), ev);
        return ev;
    }

    private static void appendActivity(Path vault, String key, Map<String, Object> ev) {
        Path path = activityPath(vault, key);
        List<String> lines = new ArrayList<>();
        if (Files.isRegularFile(path)) {
            try {
                for (String line : Files.readAllLines(path, StandardCharsets.UTF_8)) {
                    if (!line.isBlank()) lines.add(line);
                }
            } catch (IOException e) {
                throw new AisException(500, "활동 기록을 못 읽었다: " + TaskFormat.rel(vault, path));
            }
        }
        lines.add(compactLine(ev));
        while (lines.size() > ACTIVITY_KEEP) lines.remove(0);
        AtomicWriter.writeAtomic(path, (String.join("\n", lines) + "\n").getBytes(StandardCharsets.UTF_8));
    }

    /** 활동을 오래된 것부터 읽는다. {@code limit}이 양수면 가장 최근 그만큼만 낸다. 못 읽는 줄은 건너뛴다. */
    public static List<Map<String, Object>> activity(Path vault, String key, int limit) {
        Path path = activityPath(vault, key);
        List<Map<String, Object>> out = new ArrayList<>();
        if (!Files.isRegularFile(path)) return out;
        try {
            for (String line : Files.readAllLines(path, StandardCharsets.UTF_8)) {
                if (line.isBlank()) continue;
                try {
                    if (fromJson(JsonReader.parse(line)) instanceof Map<?, ?> m) {
                        @SuppressWarnings("unchecked")
                        Map<String, Object> row = (Map<String, Object>) m;
                        out.add(row);
                    }
                } catch (RuntimeException e) {
                    // 못 읽는 줄은 건너뛴다
                }
            }
        } catch (IOException e) {
            throw new AisException(500, "활동 기록을 못 읽었다: " + TaskFormat.rel(vault, path));
        }
        if (limit > 0 && out.size() > limit) return new ArrayList<>(out.subList(out.size() - limit, out.size()));
        return out;
    }

    private static String compactLine(Object o) {
        StringBuilder sb = new StringBuilder();
        compact(sb, o);
        return sb.toString();
    }

    private static void compact(StringBuilder sb, Object o) {
        if (o == null) {
            sb.append("null");
        } else if (o instanceof Boolean || o instanceof Number) {
            sb.append(o);
        } else if (o instanceof Map<?, ?> m) {
            sb.append('{');
            boolean first = true;
            for (Map.Entry<?, ?> e : m.entrySet()) {
                if (!first) sb.append(',');
                first = false;
                JsonCompact.appendString(sb, String.valueOf(e.getKey()));
                sb.append(':');
                compact(sb, e.getValue());
            }
            sb.append('}');
        } else if (o instanceof List<?> l) {
            sb.append('[');
            boolean first = true;
            for (Object e : l) {
                if (!first) sb.append(',');
                first = false;
                compact(sb, e);
            }
            sb.append(']');
        } else {
            JsonCompact.appendString(sb, String.valueOf(o));
        }
    }

    private static Object fromJson(JsonValue v) {
        if (v instanceof JsonValue.JsonString s) return s.value();
        if (v instanceof JsonValue.JsonArray a) {
            List<Object> list = new ArrayList<>();
            for (JsonValue e : a.elements()) list.add(fromJson(e));
            return list;
        }
        if (v instanceof JsonValue.JsonObject o) {
            Map<String, Object> map = new LinkedHashMap<>();
            for (Map.Entry<String, JsonValue> e : o.properties().entrySet()) map.put(e.getKey(), fromJson(e.getValue()));
            return map;
        }
        String raw = ((JsonValue.JsonLiteral) v).raw();
        if ("true".equals(raw)) return Boolean.TRUE;
        if ("false".equals(raw)) return Boolean.FALSE;
        if ("null".equals(raw)) return null;
        if (raw.matches("-?\\d{1,18}")) return Long.parseLong(raw);
        return raw;
    }

    // ------------------------------------------------------------------ 지우기·되살리기

    /** 태스크를 휴지통으로 보낸다(활동 기록도 함께). 되살릴 수 있다. */
    public static void delete(Path vault, Task t) {
        Path trash = root(vault).resolve(".trash").resolve(t.id());
        if (Files.exists(trash)) {
            throw new AisException(409, "휴지통에 같은 id 가 이미 있다: " + t.id());
        }
        Path src = vault.resolve(t.relPath());
        Path activityDir = root(vault).resolve(t.key());
        try {
            Files.createDirectories(trash);
            Files.move(src, trash.resolve("item.md"));
            if (Files.isDirectory(activityDir)) Files.move(activityDir, trash.resolve("activity"));
        } catch (IOException e) {
            throw new AisException(500, "휴지통으로 옮기지 못했다: " + t.relPath());
        }
        Map<String, Object> meta = new LinkedHashMap<>();
        meta.put("id", t.id());
        meta.put("key", t.key());
        meta.put("title", t.title());
        meta.put("relPath", t.relPath());
        meta.put("deletedAt", java.time.Instant.ofEpochMilli(TestApparatus.nowMillis()).toString());
        AtomicWriter.writeAtomic(trash.resolve("meta.json"), (compactLine(meta) + "\n").getBytes(StandardCharsets.UTF_8));
    }

    /** 휴지통의 태스크를 되살린다. 같은 키 자리가 이미 차 있으면 AIS-409다. */
    public static Task restore(Path vault, String ref) {
        TrashEntry e = findTrash(vault, ref);
        Path trash = root(vault).resolve(".trash").resolve(e.id());
        Path target = vault.resolve(e.relPath());
        if (Files.exists(target)) {
            throw new AisException(409, "제자리에 같은 키의 태스크가 이미 있어 되살리지 못했다: " + e.relPath());
        }
        try {
            Files.createDirectories(target.getParent());
            Files.move(trash.resolve("item.md"), target);
            Path activity = trash.resolve("activity");
            if (Files.isDirectory(activity)) Files.move(activity, root(vault).resolve(e.key()));
        } catch (IOException ex) {
            throw new AisException(500, "되살리지 못했다: " + e.relPath());
        }
        TreeRemover.remove(trash);
        return find(vault, e.key());
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

    private static TrashEntry findTrash(Path vault, String ref) {
        for (TrashEntry e : trash(vault)) {
            if (e.id().equals(ref) || e.key().equals(ref)) return e;
        }
        throw new AisException(404, "휴지통에서 못 찾았다: " + ref);
    }

    private static TrashEntry readTrashEntry(Path dir) {
        Path meta = dir.resolve("meta.json");
        if (!Files.isRegularFile(meta)) return null;
        try {
            JsonValue v = JsonReader.parse(Files.readString(meta, StandardCharsets.UTF_8));
            if (!(v instanceof JsonValue.JsonObject o)) return null;
            String id = str(o, "id");
            String key = str(o, "key");
            String relPath = str(o, "relPath");
            if (id == null || key == null || relPath == null || !TaskFormat.ID.matcher(id).matches() || !TaskFormat.KEY.matcher(key).matches()) return null;
            return new TrashEntry(id, key, String.valueOf(str(o, "title")), relPath, String.valueOf(str(o, "deletedAt")));
        } catch (RuntimeException | IOException e) {
            return null;
        }
    }

    private static String str(JsonValue.JsonObject o, String key) {
        JsonValue v = o.properties().get(key);
        return v instanceof JsonValue.JsonString s ? s.value() : null;
    }
}
