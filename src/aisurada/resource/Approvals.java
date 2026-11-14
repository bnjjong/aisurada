package aisurada.resource;

import aisurada.args.JsonReader;
import aisurada.args.JsonValue;
import aisurada.cli.AisException;
import aisurada.out.JsonWriter;
import aisurada.write.AtomicWriter;
import aisurada.write.DocToken;
import aisurada.write.TestApparatus;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.time.Instant;
import java.util.ArrayList;
import java.util.HexFormat;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;

/**
 * 승인 기록장(#53). <b>외부 설명서에 없는 확장</b>입니다.
 *
 * <p>AI가 고친 스킬·에이전트 정의가 곧바로 쓰이지 않고, 사람이 승인한 판만 쓰이게 하는 장치의 바탕입니다. 승인한 판의 <b>지문</b>(파일 바이트의
 * SHA-256, {@code f1:…})을 vault 밖의 앱 폴더에 적어 두고, 읽는 쪽은 지금 파일의 지문이 기록과 맞을 때만 그 파일을 씁니다.
 * 맞지 않으면(손으로 파일을 고쳤거나 편집기가 바꿨다) 승인해 둔 사본을 대신 쓰고, 기록에 없는 새 파일은 아예 쓰지 않습니다.
 *
 * <pre>
 * &lt;앱 폴더&gt;/approvals/&lt;vault 해시&gt;/
 *   ledger.json   정책과 승인한 판의 지문 {"policy":{…}, "items":{"res:&lt;id&gt;":{"token","at","by"}, "refs:&lt;프로젝트&gt;":{…}}}
 *   copies/       승인한 판의 바이트 사본(지문이 어긋났을 때 이쪽을 쓴다)
 * </pre>
 *
 * <p>기록장이 없는 vault는 처음 읽을 때 지금 있는 파일을 모두 승인한 것으로 받아들인다(기준선). 이 기능이 생기기 전에 만든 자원이 갑자기
 * 못 쓰이게 되지 않게 하려는 것이고, 그 뒤로는 기록에 없거나 어긋난 파일이 「승인 안 된 변경」으로 보인다.
 *
 * <p><b>한계</b>: 앱 폴더를 쓸 수 있는 프로세스(셸을 마음대로 쓰는 AI)는 지문까지 고칠 수 있다. 그것은 이 기록장이 아니라 실행 격리의 몫이다.
 */
public final class Approvals {

    /** 시험이 기록장 위치를 바꿀 때 쓰는 시스템 속성. 없으면 {@code ~/Library/Application Support/aisurada/approvals}. */
    public static final String DIR_PROPERTY = "aisurada.approvals.dir";

    public static final String AUTO = "auto";
    public static final String APPROVE = "approve";

    private static final Object LOCK = new Object();

    private Approvals() {}

    /** 승인이 필요한 변경의 종류. {@code fixed}는 정책을 꺼도 승인이 필요한 것(자물쇠를 AI가 정책으로 풀지 못하게)이다. */
    public enum Category {
        PERMISSION_RAISE("permission-raise", "권한 올림", true),
        TOOLS_CHANGE("tools-change", "도구 칸 변경", true),
        REF_ADD("ref-add", "참조 켜기", true),
        PERMISSION_LOWER("permission-lower", "권한 내림", false),
        REF_REMOVE("ref-remove", "참조 끄기", false),
        GLOBAL_CHANGE("global-change", "글로벌 자원 변경", false),
        PROJECT_CONTENT("project-content", "프로젝트 자원 변경", false);

        private final String code;
        private final String label;
        private final boolean fixed;

        Category(String code, String label, boolean fixed) {
            this.code = code;
            this.label = label;
            this.fixed = fixed;
        }

        public String code() {
            return code;
        }

        public String label() {
            return label;
        }

        public boolean fixed() {
            return fixed;
        }

        public static Category ofCode(String code) {
            for (Category c : values()) {
                if (c.code.equals(code)) return c;
            }
            return null;
        }
    }

    /**
     * 승인 정책. 각 칸은 {@code auto}(AI가 써도 바로 적용)나 {@code approve}(사람이 승인해야 적용)다.
     * 기본값은 프로젝트 자원의 내용만 바로 적용하고, 글로벌 자원·권한 내림·참조 끄기는 승인을 거친다.
     * 권한 올림, 참조 켜기, 도구 칸 변경은 이 값과 상관없이 늘 승인을 거친다.
     */
    public record Policy(String project, String global, String refs, String permission) {

        public static final List<String> KEYS = List.of("project", "global", "refs", "permission");

        public static Policy defaults() {
            return new Policy(AUTO, APPROVE, APPROVE, APPROVE);
        }

        public Policy with(String key, String value) {
            if (!AUTO.equals(value) && !APPROVE.equals(value)) {
                throw new AisException(400, key + " 는 auto 나 approve 여야 한다: " + value);
            }
            return switch (key) {
                case "project" -> new Policy(value, global, refs, permission);
                case "global" -> new Policy(project, value, refs, permission);
                case "refs" -> new Policy(project, global, value, permission);
                case "permission" -> new Policy(project, global, refs, value);
                default -> throw new AisException(400, "정책 칸은 " + KEYS + " 가운데 하나여야 한다: " + key);
            };
        }

        /** 이 종류의 변경에 승인이 필요하면 true. */
        public boolean requires(Category c) {
            if (c.fixed()) return true;
            String v = switch (c) {
                case GLOBAL_CHANGE -> global;
                case PROJECT_CONTENT -> project;
                case REF_REMOVE -> refs;
                case PERMISSION_LOWER -> permission;
                default -> APPROVE;
            };
            return APPROVE.equals(v);
        }

        public Map<String, Object> toMap() {
            Map<String, Object> m = new LinkedHashMap<>();
            m.put("project", project);
            m.put("global", global);
            m.put("refs", refs);
            m.put("permission", permission);
            return m;
        }
    }

    /** 승인한 판의 지문과 기록. {@code by}는 {@code human}, {@code policy}, {@code approved}, {@code adopted}, {@code baseline} 가운데 하나다. */
    public record Entry(String token, String at, String by) {}

    public record Ledger(Policy policy, Map<String, Entry> items) {}

    // ------------------------------------------------------------------ 분류와 판단

    /** 판단 결과. {@code queue}가 true면 사람의 승인을 기다려야 한다. {@code reasons}는 승인을 요구한 종류다. */
    public record Decision(boolean queue, List<Category> reasons) {}

    /**
     * 한 변경의 종류를 가른다. 예전 판({@code before}, 없으면 새로 만들기)과 새 판({@code after}, 없으면 지우기)을 견준다.
     * 되살리기는 예전 판이 없는 만들기로 본다(되살린 자원의 권한이 read보다 높으면 권한 올림이다).
     */
    public static List<Category> classify(Resource before, Resource after) {
        Resource any = after != null ? after : before;
        List<Category> out = new ArrayList<>();
        out.add(any.isGlobal() ? Category.GLOBAL_CHANGE : Category.PROJECT_CONTENT);
        if (after != null && any.kind() == ResourceKind.AGENT) {
            int b = before == null ? 0 : rank(before.permission());
            int a = rank(after.permission());
            if (a > b) out.add(Category.PERMISSION_RAISE);
            else if (a < b) out.add(Category.PERMISSION_LOWER);
        }
        if (after != null) {
            Map<String, Object> bt = before == null || before.tools() == null ? Map.of() : before.tools();
            Map<String, Object> at = after.tools() == null ? Map.of() : after.tools();
            if (!Objects.equals(bt, at)) out.add(Category.TOOLS_CHANGE);
        }
        return out;
    }

    /** 참조 목록의 변경 종류. */
    public static List<Category> classifyRefs(List<String> added, List<String> removed) {
        List<Category> out = new ArrayList<>();
        if (!added.isEmpty()) out.add(Category.REF_ADD);
        if (!removed.isEmpty()) out.add(Category.REF_REMOVE);
        return out;
    }

    /**
     * 승인이 필요한지 정한다. 사람이 한 변경은 늘 바로 적용한다. AI가 한 변경은 고정 종류가 하나라도 있거나 정책이 승인을 요구하는 종류가 있으면 대기한다.
     */
    public static Decision decide(Policy policy, List<Category> categories, boolean aiChannel) {
        if (!aiChannel) return new Decision(false, List.of());
        List<Category> reasons = new ArrayList<>();
        for (Category c : categories) {
            if (policy.requires(c)) reasons.add(c);
        }
        return new Decision(!reasons.isEmpty(), reasons);
    }

    private static int rank(String permission) {
        int i = ResourceFormat.PERMISSIONS.indexOf(permission == null ? "read" : permission);
        return Math.max(i, 0);
    }

    // ------------------------------------------------------------------ 기록장

    /** 이 vault의 기록장을 읽는다. 처음이면 지금 있는 파일을 승인한 것으로 받아들여(기준선) 기록장을 만든다. 못 읽으면 AIS-500이다(다시 받아들이지 않는다). */
    public static Ledger ledger(Path vault) {
        synchronized (LOCK) {
            return load(vault);
        }
    }

    public static Policy policy(Path vault) {
        return ledger(vault).policy();
    }

    /** 정책을 바꾼다. 사람만 부른다. */
    public static Policy setPolicy(Path vault, Policy policy) {
        synchronized (LOCK) {
            Ledger cur = load(vault);
            store(vault, new Ledger(policy, cur.items()));
            return policy;
        }
    }

    /** 승인한 판을 기록한다. {@code bytes}는 지금 파일의 바이트다. */
    public static void record(Path vault, String key, byte[] bytes, String by) {
        synchronized (LOCK) {
            Ledger cur = load(vault);
            Map<String, Entry> items = new LinkedHashMap<>(cur.items());
            items.put(key, new Entry(DocToken.compute(bytes), now(), by));
            writeCopy(vault, key, bytes);
            store(vault, new Ledger(cur.policy(), items));
        }
    }

    /** 기록을 지운다(자원을 휴지통으로 보낼 때). */
    public static void forget(Path vault, String key) {
        synchronized (LOCK) {
            Ledger cur = load(vault);
            if (!cur.items().containsKey(key)) return;
            Map<String, Entry> items = new LinkedHashMap<>(cur.items());
            items.remove(key);
            try {
                Files.deleteIfExists(copyPath(vault, key));
            } catch (IOException ignored) {
                // 못 지운 사본은 쓰이지 않는다(기록이 없으면 사본을 안 본다)
            }
            store(vault, new Ledger(cur.policy(), items));
        }
    }

    /** 승인해 둔 판의 사본. 없으면 null이다. */
    public static byte[] copy(Path vault, String key) {
        try {
            Path p = copyPath(vault, key);
            return Files.isRegularFile(p) ? Files.readAllBytes(p) : null;
        } catch (IOException e) {
            return null;
        }
    }

    public static String resKey(String id) {
        return "res:" + id;
    }

    public static String refsKey(String project) {
        return "refs:" + project;
    }

    // ------------------------------------------------------------------ 파일

    /** 이 vault의 기록 폴더. */
    public static Path dir(Path vault) {
        String override = System.getProperty(DIR_PROPERTY);
        Path base;
        if (override != null && !override.isBlank()) {
            base = Path.of(override);
        } else {
            String home = System.getProperty("user.home", "");
            if (home.isBlank()) {
                throw new AisException(500, "승인 기록 폴더를 정하지 못했다(HOME 이 없다)");
            }
            base = Path.of(home, "Library", "Application Support", "aisurada", "approvals");
        }
        return base.resolve(sha256Hex(vault.toAbsolutePath().normalize().toString()).substring(0, 16));
    }

    private static Path ledgerFile(Path vault) {
        return dir(vault).resolve("ledger.json");
    }

    private static Path copyPath(Path vault, String key) {
        return dir(vault).resolve("copies").resolve(sha256Hex(key).substring(0, 32) + ".bin");
    }

    private static void writeCopy(Path vault, String key, byte[] bytes) {
        AtomicWriter.writeAtomic(copyPath(vault, key), bytes);
    }

    private static Ledger load(Path vault) {
        Path f = ledgerFile(vault);
        if (!Files.exists(f)) return adoptBaseline(vault);
        try {
            JsonValue v = JsonReader.parse(Files.readString(f, StandardCharsets.UTF_8));
            if (!(v instanceof JsonValue.JsonObject root)) throw new IllegalArgumentException("객체가 아니다");
            Policy policy = Policy.defaults();
            if (root.properties().get("policy") instanceof JsonValue.JsonObject po) {
                for (String key : Policy.KEYS) {
                    if (po.properties().get(key) instanceof JsonValue.JsonString s) {
                        policy = policy.with(key, s.value());
                    }
                }
            }
            Map<String, Entry> items = new LinkedHashMap<>();
            if (root.properties().get("items") instanceof JsonValue.JsonObject io) {
                for (Map.Entry<String, JsonValue> e : io.properties().entrySet()) {
                    if (e.getValue() instanceof JsonValue.JsonObject eo && eo.properties().get("token") instanceof JsonValue.JsonString t) {
                        items.put(e.getKey(), new Entry(t.value(), str(eo, "at"), str(eo, "by")));
                    }
                }
            }
            return new Ledger(policy, items);
        } catch (IOException | RuntimeException e) {
            // 읽지 못한 기록장을 비어 있는 것으로 보면 모든 파일이 다시 승인된 것이 된다. 멈추고 사람이 보게 한다
            throw new AisException(500, "승인 기록을 읽지 못했다: " + f + " (" + e.getMessage() + "). 파일을 확인한다");
        }
    }

    private static String str(JsonValue.JsonObject o, String key) {
        return o.properties().get(key) instanceof JsonValue.JsonString s ? s.value() : null;
    }

    private static void store(Path vault, Ledger ledger) {
        Map<String, Object> root = new LinkedHashMap<>();
        root.put("version", 1L);
        root.put("policy", ledger.policy().toMap());
        Map<String, Object> items = new LinkedHashMap<>();
        for (Map.Entry<String, Entry> e : ledger.items().entrySet()) {
            Map<String, Object> m = new LinkedHashMap<>();
            m.put("token", e.getValue().token());
            if (e.getValue().at() != null) m.put("at", e.getValue().at());
            if (e.getValue().by() != null) m.put("by", e.getValue().by());
            items.put(e.getKey(), m);
        }
        root.put("items", items);
        AtomicWriter.writeAtomic(ledgerFile(vault), JsonWriter.toFileBytes(root));
    }

    /** 기록장이 없을 때: 지금 있는 자원 파일과 참조 목록을 모두 승인한 것으로 받아들인다. */
    private static Ledger adoptBaseline(Path vault) {
        Map<String, Entry> items = new LinkedHashMap<>();
        String at = now();
        for (Resource r : ResourceStore.scan(vault).items()) {
            byte[] bytes = readQuiet(vault.resolve(r.relPath()));
            if (bytes == null) continue;
            String key = resKey(r.id());
            writeCopy(vault, key, bytes);
            items.put(key, new Entry(DocToken.compute(bytes), at, "baseline"));
        }
        for (String project : ResourceRefs.projectsWithRefsFile(vault)) {
            byte[] bytes = readQuiet(ResourceRefs.file(vault, project));
            if (bytes == null) continue;
            String key = refsKey(project);
            writeCopy(vault, key, bytes);
            items.put(key, new Entry(DocToken.compute(bytes), at, "baseline"));
        }
        Ledger ledger = new Ledger(Policy.defaults(), items);
        store(vault, ledger);
        return ledger;
    }

    private static byte[] readQuiet(Path p) {
        try {
            return Files.isRegularFile(p) ? Files.readAllBytes(p) : null;
        } catch (IOException e) {
            return null;
        }
    }

    private static String now() {
        return Instant.ofEpochMilli(TestApparatus.nowMillis()).toString();
    }

    private static String sha256Hex(String s) {
        try {
            return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(s.getBytes(StandardCharsets.UTF_8)));
        } catch (NoSuchAlgorithmException e) {
            throw new IllegalStateException(e);
        }
    }
}
