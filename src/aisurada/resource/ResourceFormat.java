package aisurada.resource;

import aisurada.args.JsonReader;
import aisurada.args.JsonValue;
import aisurada.cli.AisException;
import aisurada.text.Trim;
import aisurada.vault.Frontmatter;
import aisurada.vault.StrictUtf8;
import aisurada.write.PathConfinement;
import aisurada.write.SlugBuilder;
import org.yaml.snakeyaml.DumperOptions;
import org.yaml.snakeyaml.Yaml;

import java.nio.charset.StandardCharsets;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.regex.Pattern;

/**
 * 자원 파일의 모양(머리말 + 지침)을 읽고 쓰고, 인자로 들어온 값을 검사합니다. <b>외부 설명서에 없는 확장</b>입니다.
 *
 * <p>스킬은 {@code SKILL.md}, 에이전트 정의는 {@code <이름>.md}이고 둘 다 {@code ---} 머리말(YAML)과 지침 본문으로 이루어집니다.
 * 머리말의 {@code id}, {@code name}, {@code description}은 두 종류가 같고, 에이전트는 {@code permission}, {@code skills},
 * {@code repo}가 더 있으며, {@code tools}는 도구별 선택 칸입니다. 본문은 파일에서 늘 끝에 줄바꿈 하나가 붙은 채 적히고, 읽을 때 그 하나를 뗍니다.
 */
public final class ResourceFormat {

    public static final int MAX_FILE_BYTES = 300_000;
    public static final int MAX_NAME = 60;
    public static final int MAX_DESCRIPTION = 1000;
    public static final int MAX_BODY = 100_000;
    public static final int MAX_SKILLS = 50;
    public static final int MAX_TOOL_KEYS = 20;
    public static final int MAX_TOOLS_CHARS = 20_000;
    /** 권한 3단계. 앞에서 뒤로 갈수록 더 많이 할 수 있다. */
    public static final List<String> PERMISSIONS = List.of("read", "edit", "publish");

    private static final Pattern TOOL_KEY = Pattern.compile("^[a-z][a-z0-9_-]{0,31}$");

    private ResourceFormat() {}

    // ------------------------------------------------------------------ 읽기

    /**
     * 파일 바이트를 자원으로 읽는다. 머리말이 없거나 못 읽거나 ID가 종류와 안 맞으면 {@link IllegalArgumentException}이다.
     * 목록을 만들 때는 이런 파일을 건너뛰고 이유를 알린다.
     */
    public static Resource parse(byte[] bytes, ResourceKind kind, String project, String relPath) {
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
        String id = str(m.get("id"));
        if (id == null || ResourceKind.ofId(id) != kind) {
            throw new IllegalArgumentException("id 가 없거나 " + kind.label() + " 모양(" + kind.idPrefix() + "…)이 아니다");
        }
        String name = str(m.get("name"));
        if (name == null || name.isBlank()) {
            throw new IllegalArgumentException("name 이 없다");
        }
        String description = str(m.get("description"));
        String permission = null;
        List<String> skills = List.of();
        String repo = null;
        if (kind == ResourceKind.AGENT) {
            String p = str(m.get("permission"));
            // 모르는 값은 가장 약한 권한으로 읽는다(손으로 고친 파일이 권한을 올리지 못하게)
            permission = p != null && PERMISSIONS.contains(p.trim().toLowerCase(java.util.Locale.ROOT)) ? p.trim().toLowerCase(java.util.Locale.ROOT) : "read";
            skills = skillList(m.get("skills"));
            String r = str(m.get("repo"));
            repo = r == null || r.isBlank() ? null : r.trim();
        }
        Map<String, Object> tools = new LinkedHashMap<>();
        if (m.get("tools") instanceof Map<?, ?> tm) {
            for (Map.Entry<?, ?> e : tm.entrySet()) {
                tools.put(String.valueOf(e.getKey()), e.getValue());
            }
        }
        return new Resource(id, kind, project, name.trim(), description == null ? "" : description.trim(), permission, skills, repo,
                tools, body, str(m.get("editedAt")), str(m.get("editedSession")), str(m.get("editedAgent")), relPath,
                aisurada.write.DocToken.compute(bytes));
    }

    private static String str(Object o) {
        return o == null ? null : String.valueOf(o);
    }

    private static List<String> skillList(Object o) {
        List<String> out = new ArrayList<>();
        if (o instanceof List<?> list) {
            for (Object e : list) {
                String s = str(e);
                if (s != null && ResourceKind.ofId(s) == ResourceKind.SKILL && !out.contains(s)) out.add(s);
            }
        }
        return List.copyOf(out);
    }

    // ------------------------------------------------------------------ 쓰기

    /** 자원을 파일 바이트로 만든다. 같은 값이면 늘 같은 바이트다. */
    public static byte[] render(Resource r) {
        Map<String, Object> m = new LinkedHashMap<>();
        m.put("id", r.id());
        m.put("name", r.name());
        m.put("description", r.description());
        if (r.kind() == ResourceKind.AGENT) {
            m.put("permission", r.permission() == null ? "read" : r.permission());
            if (r.skills() != null && !r.skills().isEmpty()) m.put("skills", new ArrayList<>(r.skills()));
            if (r.repo() != null && !r.repo().isEmpty()) m.put("repo", r.repo());
        }
        if (r.tools() != null && !r.tools().isEmpty()) m.put("tools", r.tools());
        if (r.editedAt() != null) m.put("editedAt", r.editedAt());
        if (r.editedSession() != null) m.put("editedSession", r.editedSession());
        if (r.editedAgent() != null) m.put("editedAgent", r.editedAgent());

        DumperOptions options = new DumperOptions();
        options.setDefaultFlowStyle(DumperOptions.FlowStyle.BLOCK);
        options.setAllowUnicode(true);
        options.setSplitLines(false);
        options.setIndent(2);
        options.setIndicatorIndent(2);
        options.setIndentWithIndicator(true);
        String yaml = new Yaml(options).dump(m);
        String text = "---\n" + yaml + "---\n" + (r.body() == null ? "" : r.body()) + "\n";
        return text.getBytes(StandardCharsets.UTF_8);
    }

    // ------------------------------------------------------------------ 인자 검사

    /** 이름을 검사하고 다듬어 돌려준다. 폴더·파일 이름으로 쓸 수 있어야 한다. */
    public static String name(String raw) {
        String name = raw == null ? "" : Trim.trim(raw);
        if (name.isEmpty()) {
            throw new AisException(400, "name 이 필요하다");
        }
        if (name.codePointCount(0, name.length()) > MAX_NAME) {
            throw new AisException(400, "name 은 " + MAX_NAME + "자까지다: " + name);
        }
        for (int i = 0; i < name.length(); i++) {
            char c = name.charAt(i);
            if (Character.isISOControl(c)) {
                throw new AisException(400, "name 에 제어 문자를 쓸 수 없다");
            }
            if (c == '/' || c == '\\') {
                throw new AisException(400, "name 에는 / 와 \\ 를 쓸 수 없다: " + name);
            }
        }
        slugOf(name);
        return name;
    }

    /** 이름에서 폴더·파일 이름(slug)을 만든다. 만들 수 없으면 AIS-400이다. */
    public static String slugOf(String name) {
        String slug = SlugBuilder.build(name, 50);
        if (slug == null || slug.isEmpty() || slug.startsWith(".") || PathConfinement.hasUnusablePathCharacters(slug)) {
            throw new AisException(400, "name 에서 파일 이름을 만들 수 없다: " + name);
        }
        return slug;
    }

    public static String description(String raw) {
        String d = raw == null ? "" : Trim.trim(raw);
        if (d.isEmpty()) {
            throw new AisException(400, "description 이 필요하다");
        }
        if (d.codePointCount(0, d.length()) > MAX_DESCRIPTION) {
            throw new AisException(400, "description 은 " + MAX_DESCRIPTION + "자까지다");
        }
        return d;
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

    public static String permission(String raw) {
        String p = raw == null ? "" : raw.trim().toLowerCase(java.util.Locale.ROOT);
        if (!PERMISSIONS.contains(p)) {
            throw new AisException(400, "permission 은 " + PERMISSIONS + " 가운데 하나여야 한다: " + raw);
        }
        return p;
    }

    /** {@code skills=} 값(JSON 배열)을 스킬 ID 목록으로 읽는다. 빈 값은 빈 목록(비우기)이다. */
    public static List<String> skillIds(String raw) {
        if (raw == null || Trim.isBlank(raw)) return List.of();
        JsonValue parsed = parseJson(raw, "skills");
        if (!(parsed instanceof JsonValue.JsonArray array)) {
            throw new AisException(400, "skills 는 JSON 배열이어야 한다");
        }
        if (array.elements().size() > MAX_SKILLS) {
            throw new AisException(400, "skills 는 " + MAX_SKILLS + "개까지다");
        }
        Set<String> seen = new LinkedHashSet<>();
        for (JsonValue v : array.elements()) {
            if (!(v instanceof JsonValue.JsonString s) || ResourceKind.ofId(s.value()) != ResourceKind.SKILL) {
                throw new AisException(400, "skills 의 항목은 스킬 ID(sk…)여야 한다: " + v);
            }
            if (!seen.add(s.value())) {
                throw new AisException(400, "skills 에 같은 ID 가 겹친다: " + s.value());
            }
        }
        return List.copyOf(seen);
    }

    /**
     * {@code tools=} 값(JSON 객체)을 도구별 선택 칸으로 읽는다. 칸 이름은 소문자 도구 이름이다. 빈 값은 빈 맵(비우기)이다.
     * 안쪽 값은 검사하지 않고 그대로 보존한다.
     */
    public static Map<String, Object> tools(String raw) {
        if (raw == null || Trim.isBlank(raw)) return Map.of();
        JsonValue parsed = parseJson(raw, "tools");
        if (!(parsed instanceof JsonValue.JsonObject obj)) {
            throw new AisException(400, "tools 는 JSON 객체여야 한다");
        }
        if (obj.properties().size() > MAX_TOOL_KEYS) {
            throw new AisException(400, "tools 는 " + MAX_TOOL_KEYS + "칸까지다");
        }
        Map<String, Object> out = new LinkedHashMap<>();
        for (Map.Entry<String, JsonValue> e : obj.properties().entrySet()) {
            if (!TOOL_KEY.matcher(e.getKey()).matches()) {
                throw new AisException(400, "tools 의 칸 이름은 소문자 도구 이름이어야 한다(예: claude, codex): " + e.getKey());
            }
            out.put(e.getKey(), toJava(e.getValue()));
        }
        if (raw.length() > MAX_TOOLS_CHARS) {
            throw new AisException(400, "tools 는 " + MAX_TOOLS_CHARS + "자까지다");
        }
        return out;
    }

    private static JsonValue parseJson(String raw, String field) {
        try {
            return JsonReader.parse(raw);
        } catch (RuntimeException e) {
            throw new AisException(400, field + " 는 JSON 이어야 한다: " + e.getMessage());
        }
    }

    private static Object toJava(JsonValue v) {
        if (v instanceof JsonValue.JsonString s) return s.value();
        if (v instanceof JsonValue.JsonArray a) {
            List<Object> list = new ArrayList<>();
            for (JsonValue e : a.elements()) list.add(toJava(e));
            return list;
        }
        if (v instanceof JsonValue.JsonObject o) {
            Map<String, Object> map = new LinkedHashMap<>();
            for (Map.Entry<String, JsonValue> e : o.properties().entrySet()) map.put(e.getKey(), toJava(e.getValue()));
            return map;
        }
        String raw = ((JsonValue.JsonLiteral) v).raw();
        if ("true".equals(raw)) return Boolean.TRUE;
        if ("false".equals(raw)) return Boolean.FALSE;
        if ("null".equals(raw)) return null;
        if (raw.matches("-?\\d{1,18}")) return Long.parseLong(raw);
        if (raw.matches("-?\\d+(\\.\\d+)?([eE][-+]?\\d+)?")) {
            try {
                return Double.parseDouble(raw);
            } catch (NumberFormatException ignored) {
                // 아래에서 글로 둔다
            }
        }
        return raw;
    }

    /** 경로를 vault 기준 슬래시 경로 글로 만든다. */
    public static String rel(Path vault, Path file) {
        return vault.toAbsolutePath().normalize().relativize(file.toAbsolutePath().normalize()).toString().replace('\\', '/');
    }
}
