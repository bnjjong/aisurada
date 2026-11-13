package aisurada.task;

import aisurada.args.JsonReader;
import aisurada.args.JsonValue;
import aisurada.cli.AisException;
import aisurada.text.IdShape;
import aisurada.text.Trim;
import aisurada.vault.Frontmatter;
import aisurada.vault.StrictUtf8;
import org.yaml.snakeyaml.DumperOptions;
import org.yaml.snakeyaml.Yaml;

import java.nio.charset.StandardCharsets;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.regex.Pattern;

/**
 * 태스크 파일의 모양(머리말 + 설명)을 읽고 쓰고, 인자로 들어온 값을 검사합니다. <b>외부 설명서에 없는 확장</b>입니다.
 *
 * <p>파일은 {@code ---} 머리말(YAML)과 설명 본문입니다. 머리말은 스킬·에이전트 자원처럼 SnakeYAML로 읽고 쓰므로 완료 기준 같은
 * 목록을 들여써서 적을 수 있습니다(프로젝트·아젠다 파일처럼 줄 읽개로 읽는 파일이 아니다). 모르는 칸은 {@link Task#extra()}에 보존합니다.
 * 본문은 파일에서 늘 끝에 줄바꿈 하나가 붙은 채 적히고, 읽을 때 그 하나를 뗍니다.
 */
public final class TaskFormat {

    public static final int MAX_FILE_BYTES = 400_000;
    public static final int MAX_TITLE = 200;
    public static final int MAX_BODY = 100_000;
    public static final int MAX_LABELS = 20;
    public static final int MAX_LABEL = 30;
    public static final int MAX_CRITERIA = 50;
    public static final int MAX_CRITERION = 500;
    public static final int MAX_REFS = 50;
    public static final int MAX_LINKS = 50;
    public static final int MAX_COMMENT = 20_000;

    public static final List<String> TYPES = List.of("feature", "bug", "chore");
    public static final List<String> PRIORITIES = List.of("high", "medium", "low");
    public static final List<String> LINK_TYPES = List.of("blocks", "relates", "duplicates");
    public static final String DEFAULT_TYPE = "feature";
    public static final String DEFAULT_PRIORITY = "medium";

    public static final Pattern KEY = Pattern.compile("^TSK-[1-9][0-9]{0,8}$");
    public static final Pattern ID = Pattern.compile("^tk[0-9a-z]+-[0-9a-f]{8}$");
    private static final Pattern REPO = Pattern.compile("^[A-Za-z0-9_.-]{1,100}/[A-Za-z0-9_.-]{1,100}$");
    private static final Pattern BRANCH = Pattern.compile("^[A-Za-z0-9][A-Za-z0-9._/-]{0,99}$");
    private static final Pattern GITHUB_SHORT = Pattern.compile("^[A-Za-z0-9_.-]{1,100}/[A-Za-z0-9_.-]{1,100}#[1-9][0-9]{0,8}$");
    private static final Pattern GITHUB_URL = Pattern.compile("^https://github\\.com/[A-Za-z0-9_.-]{1,100}/[A-Za-z0-9_.-]{1,100}/(issues|pull)/[1-9][0-9]{0,8}$");

    /** 머리말의 알려진 칸. 이 밖의 칸은 {@code extra}로 보존한다. */
    private static final Set<String> KNOWN = Set.of("key", "id", "title", "type", "priority", "status", "repo", "project", "baseBranch",
            "labels", "criteria", "links", "refs", "createdAt", "editedAt", "editedSession", "editedAgent");

    private TaskFormat() {}

    /** {@code TSK-14}의 번호 부분. 키 모양이 아니면 0이다. */
    public static int numberOf(String key) {
        if (key == null || !KEY.matcher(key).matches()) return 0;
        return Integer.parseInt(key.substring(4));
    }

    // ------------------------------------------------------------------ 읽기

    /**
     * 파일 바이트를 태스크로 읽는다. 머리말이 없거나 못 읽거나 키·ID·상태가 틀리면 {@link IllegalArgumentException}이다.
     * 목록을 만들 때는 이런 파일을 건너뛰고 이유를 알린다.
     */
    public static Task parse(byte[] bytes, String relPath) {
        if (bytes.length > MAX_FILE_BYTES) {
            throw new IllegalArgumentException("파일이 " + MAX_FILE_BYTES + "바이트를 넘는다");
        }
        String text;
        try {
            text = StrictUtf8.decodeStrict(bytes);
        } catch (Exception e) {
            throw new IllegalArgumentException("UTF-8 글이 아니다");
        }
        text = text.replace("\r\n", "\n");
        if (!text.startsWith("---\n")) {
            throw new IllegalArgumentException("머리말(---)로 시작하지 않는다");
        }
        int close = -1;
        int from = 3;
        while (true) {
            int i = text.indexOf("\n---", from);
            if (i < 0) break;
            int after = i + 4;
            if (after == text.length() || text.charAt(after) == '\n') {
                close = i;
                break;
            }
            from = i + 1;
        }
        if (close < 0) {
            throw new IllegalArgumentException("머리말을 닫는 ---가 없다");
        }
        String yamlText = text.substring(4, Math.max(4, close + 1));
        int bodyStart = Math.min(close + 4, text.length());
        if (bodyStart < text.length() && text.charAt(bodyStart) == '\n') bodyStart++;
        String body = text.substring(bodyStart);
        if (body.endsWith("\n")) body = body.substring(0, body.length() - 1);

        Map<String, Object> m = Frontmatter.parse(yamlText);
        if (m == null) {
            throw new IllegalArgumentException("머리말을 YAML로 읽지 못했다");
        }
        String key = str(m.get("key"));
        if (key == null || !KEY.matcher(key).matches()) {
            throw new IllegalArgumentException("key 가 없거나 TSK-<번호> 모양이 아니다");
        }
        String id = str(m.get("id"));
        if (id == null || !ID.matcher(id).matches()) {
            throw new IllegalArgumentException("id 가 없거나 tk… 모양이 아니다");
        }
        String title = str(m.get("title"));
        if (title == null || title.isBlank()) {
            throw new IllegalArgumentException("title 이 없다");
        }
        TaskStatus status = TaskStatus.parse(str(m.get("status")));
        if (status == null) {
            throw new IllegalArgumentException("status 를 모른다: " + m.get("status"));
        }
        String type = str(m.get("type"));
        type = type != null && TYPES.contains(type.trim().toLowerCase(Locale.ROOT)) ? type.trim().toLowerCase(Locale.ROOT) : DEFAULT_TYPE;
        String priority = str(m.get("priority"));
        priority = priority != null && PRIORITIES.contains(priority.trim().toLowerCase(Locale.ROOT)) ? priority.trim().toLowerCase(Locale.ROOT) : DEFAULT_PRIORITY;

        Map<String, Object> extra = new LinkedHashMap<>();
        for (Map.Entry<String, Object> e : m.entrySet()) {
            if (!KNOWN.contains(e.getKey())) extra.put(e.getKey(), e.getValue());
        }
        return new Task(key, id, title.trim(), type, priority, status, blankToNull(str(m.get("repo"))), blankToNull(str(m.get("project"))),
                blankToNull(str(m.get("baseBranch"))), labelList(m.get("labels")), criteriaList(m.get("criteria")), linkList(m.get("links")),
                refList(m.get("refs")), body, extra, str(m.get("createdAt")), str(m.get("editedAt")), str(m.get("editedSession")),
                str(m.get("editedAgent")), relPath, aisurada.write.DocToken.compute(bytes));
    }

    private static String str(Object o) {
        return o == null ? null : String.valueOf(o);
    }

    private static String blankToNull(String s) {
        return s == null || s.isBlank() ? null : s.trim();
    }

    private static List<String> labelList(Object o) {
        List<String> out = new ArrayList<>();
        if (o instanceof List<?> list) {
            for (Object e : list) {
                String s = str(e);
                if (s != null && !s.isBlank() && !out.contains(s.trim())) out.add(s.trim());
            }
        }
        return List.copyOf(out);
    }

    private static List<Task.Criterion> criteriaList(Object o) {
        List<Task.Criterion> out = new ArrayList<>();
        if (o instanceof List<?> list) {
            for (Object e : list) {
                if (e instanceof Map<?, ?> m) {
                    String text = str(m.get("text"));
                    if (text == null || text.isBlank()) continue;
                    boolean done = "true".equalsIgnoreCase(str(m.get("done")));
                    out.add(new Task.Criterion(text.trim(), done, done ? str(m.get("checkedAt")) : null, done ? str(m.get("checkedBy")) : null));
                } else if (e != null && !String.valueOf(e).isBlank()) {
                    out.add(new Task.Criterion(String.valueOf(e).trim(), false, null, null));
                }
            }
        }
        return List.copyOf(out);
    }

    private static List<Task.Link> linkList(Object o) {
        List<Task.Link> out = new ArrayList<>();
        if (o instanceof List<?> list) {
            for (Object e : list) {
                if (!(e instanceof Map<?, ?> m)) continue;
                String type = str(m.get("type"));
                String to = str(m.get("to"));
                if (type == null || to == null || !LINK_TYPES.contains(type) || !KEY.matcher(to).matches()) continue;
                Task.Link link = new Task.Link(type, to);
                if (!out.contains(link)) out.add(link);
            }
        }
        return List.copyOf(out);
    }

    private static List<Task.Ref> refList(Object o) {
        List<Task.Ref> out = new ArrayList<>();
        if (o instanceof List<?> list) {
            for (Object e : list) {
                if (!(e instanceof Map<?, ?> m)) continue;
                String kind = str(m.get("kind"));
                String target = str(m.get("target"));
                if (kind == null || target == null || !validRef(kind, target)) continue;
                Task.Ref ref = new Task.Ref(kind, target, blankToNull(str(m.get("title"))));
                if (out.stream().noneMatch(r -> r.kind().equals(ref.kind()) && r.target().equals(ref.target()))) out.add(ref);
            }
        }
        return List.copyOf(out);
    }

    private static boolean validRef(String kind, String target) {
        return switch (kind) {
            case "note" -> IdShape.NOTE.matcher(target).matches() || IdShape.SUB_NOTE.matcher(target).matches();
            case "github" -> GITHUB_SHORT.matcher(target).matches() || GITHUB_URL.matcher(target).matches();
            default -> false;
        };
    }

    // ------------------------------------------------------------------ 쓰기

    /** 태스크를 파일 바이트로 만든다. 같은 값이면 늘 같은 바이트다. */
    public static byte[] render(Task t) {
        Map<String, Object> m = new LinkedHashMap<>();
        m.put("key", t.key());
        m.put("id", t.id());
        m.put("title", t.title());
        m.put("type", t.type());
        m.put("priority", t.priority());
        m.put("status", t.status().label());
        if (t.repo() != null) m.put("repo", t.repo());
        if (t.project() != null) m.put("project", t.project());
        if (t.baseBranch() != null) m.put("baseBranch", t.baseBranch());
        if (t.labels() != null && !t.labels().isEmpty()) m.put("labels", new ArrayList<>(t.labels()));
        if (t.criteria() != null && !t.criteria().isEmpty()) {
            List<Object> rows = new ArrayList<>();
            for (Task.Criterion c : t.criteria()) {
                Map<String, Object> r = new LinkedHashMap<>();
                r.put("text", c.text());
                r.put("done", c.done());
                if (c.done() && c.checkedAt() != null) r.put("checkedAt", c.checkedAt());
                if (c.done() && c.checkedBy() != null) r.put("checkedBy", c.checkedBy());
                rows.add(r);
            }
            m.put("criteria", rows);
        }
        if (t.links() != null && !t.links().isEmpty()) {
            List<Object> rows = new ArrayList<>();
            for (Task.Link l : t.links()) {
                Map<String, Object> r = new LinkedHashMap<>();
                r.put("type", l.type());
                r.put("to", l.to());
                rows.add(r);
            }
            m.put("links", rows);
        }
        if (t.refs() != null && !t.refs().isEmpty()) {
            List<Object> rows = new ArrayList<>();
            for (Task.Ref r0 : t.refs()) {
                Map<String, Object> r = new LinkedHashMap<>();
                r.put("kind", r0.kind());
                r.put("target", r0.target());
                if (r0.title() != null) r.put("title", r0.title());
                rows.add(r);
            }
            m.put("refs", rows);
        }
        if (t.createdAt() != null) m.put("createdAt", t.createdAt());
        if (t.editedAt() != null) m.put("editedAt", t.editedAt());
        if (t.editedSession() != null) m.put("editedSession", t.editedSession());
        if (t.editedAgent() != null) m.put("editedAgent", t.editedAgent());
        if (t.extra() != null) {
            for (Map.Entry<String, Object> e : t.extra().entrySet()) {
                if (!m.containsKey(e.getKey())) m.put(e.getKey(), e.getValue());
            }
        }

        DumperOptions options = new DumperOptions();
        options.setDefaultFlowStyle(DumperOptions.FlowStyle.BLOCK);
        options.setAllowUnicode(true);
        options.setSplitLines(false);
        options.setIndent(2);
        options.setIndicatorIndent(2);
        options.setIndentWithIndicator(true);
        String yaml = new Yaml(options).dump(m);
        String text = "---\n" + yaml + "---\n" + (t.body() == null ? "" : t.body()) + "\n";
        return text.getBytes(StandardCharsets.UTF_8);
    }

    // ------------------------------------------------------------------ 인자 검사

    public static String title(String raw) {
        String t = raw == null ? "" : Trim.trim(raw);
        if (t.isEmpty()) {
            throw new AisException(400, "title 이 필요하다");
        }
        if (t.codePointCount(0, t.length()) > MAX_TITLE) {
            throw new AisException(400, "title 은 " + MAX_TITLE + "자까지다");
        }
        for (int i = 0; i < t.length(); i++) {
            if (Character.isISOControl(t.charAt(i))) {
                throw new AisException(400, "title 에 제어 문자를 쓸 수 없다");
            }
        }
        return t;
    }

    public static String type(String raw) {
        String v = raw == null ? "" : raw.trim().toLowerCase(Locale.ROOT);
        if (!TYPES.contains(v)) {
            throw new AisException(400, "type 은 " + TYPES + " 가운데 하나여야 한다: " + raw);
        }
        return v;
    }

    public static String priority(String raw) {
        String v = raw == null ? "" : raw.trim().toLowerCase(Locale.ROOT);
        if (!PRIORITIES.contains(v)) {
            throw new AisException(400, "priority 는 " + PRIORITIES + " 가운데 하나여야 한다: " + raw);
        }
        return v;
    }

    /** {@code owner/name}. 빈 값이면 null(지운다)이다. */
    public static String repo(String raw) {
        String v = raw == null ? "" : Trim.trim(raw);
        if (v.isEmpty()) return null;
        if (!REPO.matcher(v).matches() || v.contains("..")) {
            throw new AisException(400, "repo 는 owner/name 모양이어야 한다: " + raw);
        }
        return v;
    }

    /** 기준 브랜치. 빈 값이면 null(저장소의 기본 브랜치)이다. */
    public static String baseBranch(String raw) {
        String v = raw == null ? "" : Trim.trim(raw);
        if (v.isEmpty()) return null;
        if (!BRANCH.matcher(v).matches() || v.contains("..") || v.endsWith("/") || v.endsWith(".lock") || v.contains("//")) {
            throw new AisException(400, "base 는 브랜치 이름이어야 한다: " + raw);
        }
        return v;
    }

    public static String body(String raw) {
        String b = raw == null ? "" : raw;
        if (b.length() > MAX_BODY) {
            throw new AisException(400, "body 는 " + MAX_BODY + "자까지다");
        }
        if (b.indexOf('\0') >= 0) {
            throw new AisException(400, "body 에 널 문자를 쓸 수 없다");
        }
        return b;
    }

    public static String comment(String raw) {
        String b = raw == null ? "" : Trim.trim(raw);
        if (b.isEmpty()) {
            throw new AisException(400, "body 가 필요하다");
        }
        if (b.length() > MAX_COMMENT) {
            throw new AisException(400, "body 는 " + MAX_COMMENT + "자까지다");
        }
        if (b.indexOf('\0') >= 0) {
            throw new AisException(400, "body 에 널 문자를 쓸 수 없다");
        }
        return b;
    }

    /** {@code labels=} 값(JSON 배열). 빈 값은 빈 목록(비우기)이다. */
    public static List<String> labels(String raw) {
        if (raw == null || Trim.isBlank(raw)) return List.of();
        JsonValue.JsonArray array = array(raw, "labels");
        if (array.elements().size() > MAX_LABELS) {
            throw new AisException(400, "labels 는 " + MAX_LABELS + "개까지다");
        }
        Set<String> seen = new LinkedHashSet<>();
        for (JsonValue v : array.elements()) {
            if (!(v instanceof JsonValue.JsonString s)) {
                throw new AisException(400, "labels 의 항목은 글이어야 한다: " + v);
            }
            String label = Trim.trim(s.value());
            if (label.isEmpty() || label.codePointCount(0, label.length()) > MAX_LABEL) {
                throw new AisException(400, "라벨은 1~" + MAX_LABEL + "자다: " + s.value());
            }
            for (int i = 0; i < label.length(); i++) {
                if (Character.isISOControl(label.charAt(i))) {
                    throw new AisException(400, "라벨에 제어 문자를 쓸 수 없다");
                }
            }
            seen.add(label);
        }
        return List.copyOf(seen);
    }

    /**
     * {@code criteria=} 값(JSON 배열). 항목은 글이거나 {@code {"text":"…","done":true}}다. 빈 값은 빈 목록이다.
     * 확인한 시각과 사람은 호출한 쪽({@link TaskStore})이 채운다.
     */
    public static List<Task.Criterion> criteria(String raw) {
        if (raw == null || Trim.isBlank(raw)) return List.of();
        JsonValue.JsonArray array = array(raw, "criteria");
        if (array.elements().size() > MAX_CRITERIA) {
            throw new AisException(400, "criteria 는 " + MAX_CRITERIA + "개까지다");
        }
        List<Task.Criterion> out = new ArrayList<>();
        for (JsonValue v : array.elements()) {
            String text;
            boolean done = false;
            String checkedAt = null;
            String checkedBy = null;
            if (v instanceof JsonValue.JsonString s) {
                text = s.value();
            } else if (v instanceof JsonValue.JsonObject o) {
                for (String k : o.properties().keySet()) {
                    if (!Set.of("text", "done", "checkedAt", "checkedBy").contains(k)) {
                        throw new AisException(400, "criteria 항목에 모르는 키가 있다: " + k);
                    }
                }
                text = stringOf(o, "text");
                JsonValue d = o.properties().get("done");
                if (d != null) {
                    if (!(d instanceof JsonValue.JsonLiteral l) || !(l.raw().equals("true") || l.raw().equals("false"))) {
                        throw new AisException(400, "criteria 의 done 은 true 나 false 여야 한다");
                    }
                    done = l.raw().equals("true");
                }
                checkedAt = stringOf(o, "checkedAt");
                checkedBy = stringOf(o, "checkedBy");
            } else {
                throw new AisException(400, "criteria 의 항목은 글이나 객체여야 한다: " + v);
            }
            text = text == null ? "" : Trim.trim(text);
            if (text.isEmpty() || text.codePointCount(0, text.length()) > MAX_CRITERION) {
                throw new AisException(400, "완료 기준 한 줄은 1~" + MAX_CRITERION + "자다");
            }
            out.add(new Task.Criterion(text, done, done ? checkedAt : null, done ? checkedBy : null));
        }
        return List.copyOf(out);
    }

    /** {@code refs=} 값(JSON 배열). 항목은 {@code {"kind":"note","target":"nd…"}}나 {@code {"kind":"github","target":"owner/name#53"}}다. */
    public static List<Task.Ref> refs(String raw) {
        if (raw == null || Trim.isBlank(raw)) return List.of();
        JsonValue.JsonArray array = array(raw, "refs");
        if (array.elements().size() > MAX_REFS) {
            throw new AisException(400, "refs 는 " + MAX_REFS + "개까지다");
        }
        List<Task.Ref> out = new ArrayList<>();
        for (JsonValue v : array.elements()) {
            if (!(v instanceof JsonValue.JsonObject o)) {
                throw new AisException(400, "refs 의 항목은 객체여야 한다: " + v);
            }
            for (String k : o.properties().keySet()) {
                if (!Set.of("kind", "target", "title").contains(k)) {
                    throw new AisException(400, "refs 항목에 모르는 키가 있다: " + k);
                }
            }
            String kind = stringOf(o, "kind");
            String target = stringOf(o, "target");
            if (kind == null || target == null || !validRef(kind.trim(), target.trim())) {
                throw new AisException(400, "refs 항목이 틀렸다. note 는 노트 id(nd…), github 는 owner/name#번호나 이슈·PR 주소여야 한다: " + v);
            }
            String title = stringOf(o, "title");
            Task.Ref ref = new Task.Ref(kind.trim(), target.trim(), title == null || Trim.isBlank(title) ? null : Trim.trim(title));
            if (out.stream().anyMatch(r -> r.kind().equals(ref.kind()) && r.target().equals(ref.target()))) {
                throw new AisException(400, "refs 에 같은 항목이 겹친다: " + ref.target());
            }
            out.add(ref);
        }
        return List.copyOf(out);
    }

    private static String stringOf(JsonValue.JsonObject o, String key) {
        JsonValue v = o.properties().get(key);
        return v instanceof JsonValue.JsonString s ? s.value() : null;
    }

    private static JsonValue.JsonArray array(String raw, String field) {
        JsonValue parsed;
        try {
            parsed = JsonReader.parse(raw);
        } catch (RuntimeException e) {
            throw new AisException(400, field + " 는 JSON 이어야 한다: " + e.getMessage());
        }
        if (!(parsed instanceof JsonValue.JsonArray a)) {
            throw new AisException(400, field + " 는 JSON 배열이어야 한다");
        }
        return a;
    }

    /** 경로를 vault 기준 슬래시 경로 글로 만든다. */
    public static String rel(Path vault, Path file) {
        return vault.toAbsolutePath().normalize().relativize(file.toAbsolutePath().normalize()).toString().replace('\\', '/');
    }
}
