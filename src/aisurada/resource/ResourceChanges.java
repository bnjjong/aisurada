package aisurada.resource;

import aisurada.args.JsonReader;
import aisurada.args.JsonValue;
import aisurada.cli.AisException;
import aisurada.out.JsonWriter;
import aisurada.write.AtomicWriter;
import aisurada.write.IdGenerator;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.DirectoryStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.regex.Pattern;

/**
 * 승인을 기다리는 변경(대기 변경)을 vault 안에 두는 곳(#53). <b>외부 설명서에 없는 확장</b>입니다.
 *
 * <p>변경 하나가 파일 하나입니다: {@code .aisurada/pending/<변경 id>.json}. AI가 쓴 변경이 승인이 필요하면 바로 적용하지 않고 여기에 올립니다.
 * 대기 중인 AI 초안도 지식이라 vault 안에 두고, 어떤 AI가 무엇을 제안했는지가 이력에 남고 vault를 옮겨도 따라옵니다. 노트 목록과 검색은
 * {@code .aisurada/} 아래를 보지 않아서 이 파일들은 거기에 나타나지 않습니다. 결정이 난 변경(승인·거절·기준이 바뀌어 무효)도 상태만 바꿔 남겨서
 * 「누가 썼고 누가 승인했는지」를 볼 수 있게 하고, 오래된 것부터 {@value #DECIDED_KEEP}개를 넘으면 지웁니다.
 *
 * <p>변경은 JSON 객체(맵)입니다. 칸은 다음과 같습니다.
 * <pre>
 * id, status(pending|approved|rejected|stale), command(save-resource|delete-resource|restore-resource|set-resource-ref),
 * createdAt, proposer{session,agent,channel}, categories[코드], fixed(정책과 무관하게 승인이 필요한 종류가 있는가),
 * target{kind,scope,project,id,name}, args{명령에 준 옵션}, positional[명령에 준 자리 인자], forcedId(만들기),
 * before{…}, after{…}(보여 주기용 칸), summary, decidedAt, decidedBy, decision(approved|rejected|stale), reason
 * </pre>
 * 적용할 때는 {@code command}, {@code args}, {@code positional}로 같은 명령 경로를 한 번 더 지나서, 승인 때의 검사와 평소 검사가 갈라지지 않습니다.
 */
public final class ResourceChanges {

    public static final String DIR = ".aisurada/pending";
    public static final int DECIDED_KEEP = 200;

    public static final String PENDING = "pending";
    public static final String APPROVED = "approved";
    public static final String REJECTED = "rejected";
    public static final String STALE = "stale";

    private static final Pattern ID_SHAPE = Pattern.compile("^pc[0-9a-z]+-[0-9a-f]{8}$");

    private ResourceChanges() {}

    public static Path dir(Path vault) {
        return vault.resolve(DIR);
    }

    public static boolean isId(String id) {
        return id != null && ID_SHAPE.matcher(id).matches();
    }

    /** 새 변경 ID({@code pc…}). */
    public static String newId() {
        return IdGenerator.nextId("pc");
    }

    /** 대기에 올린다. 변경에는 {@code id}가 이미 들어 있어야 한다. */
    public static void write(Path vault, Map<String, Object> change) {
        String id = (String) change.get("id");
        if (!isId(id)) throw new AisException(500, "변경 id 모양이 아니다: " + id);
        AtomicWriter.writeAtomic(dir(vault).resolve(id + ".json"), JsonWriter.toFileBytes(change));
    }

    /** 변경 하나를 읽는다. 없으면 AIS-404, 모양이 틀리면 AIS-400이다. */
    public static Map<String, Object> read(Path vault, String id) {
        if (!isId(id)) throw new AisException(400, "변경 id 모양이 아니다(pc…): " + id);
        Path f = dir(vault).resolve(id + ".json");
        if (!Files.isRegularFile(f)) throw new AisException(404, "대기 변경을 못 찾았다: " + id);
        Map<String, Object> m = readFile(f);
        if (m == null) throw new AisException(500, "대기 변경 파일을 못 읽었다: " + id);
        return m;
    }

    /** 모든 변경. 올린 시각이 늦은 것이 앞이다. 못 읽는 파일은 건너뛴다. */
    public static List<Map<String, Object>> list(Path vault) {
        List<Map<String, Object>> out = new ArrayList<>();
        Path d = dir(vault);
        if (!Files.isDirectory(d)) return out;
        try (DirectoryStream<Path> ds = Files.newDirectoryStream(d, "*.json")) {
            for (Path p : ds) {
                Map<String, Object> m = readFile(p);
                if (m != null && isId((String) m.get("id"))) out.add(m);
            }
        } catch (IOException e) {
            return out;
        }
        out.sort(Comparator.comparing((Map<String, Object> m) -> String.valueOf(m.get("createdAt"))).reversed()
                .thenComparing(m -> String.valueOf(m.get("id")), Comparator.reverseOrder()));
        return out;
    }

    /** 대기 중인 것({@code status=pending})만. */
    public static List<Map<String, Object>> pending(Path vault) {
        return list(vault).stream().filter(m -> PENDING.equals(m.get("status"))).toList();
    }

    /** 결정이 난 변경이 너무 많으면 오래된 것부터 지운다. */
    public static void prune(Path vault) {
        List<Map<String, Object>> decided = list(vault).stream().filter(m -> !PENDING.equals(m.get("status"))).toList();
        for (int i = DECIDED_KEEP; i < decided.size(); i++) {
            try {
                Files.deleteIfExists(dir(vault).resolve(decided.get(i).get("id") + ".json"));
            } catch (IOException ignored) {
                // 못 지운 기록은 다음에 다시 지운다
            }
        }
    }

    // ------------------------------------------------------------------ JSON

    private static Map<String, Object> readFile(Path f) {
        try {
            JsonValue v = JsonReader.parse(Files.readString(f, StandardCharsets.UTF_8));
            Object o = toJava(v);
            @SuppressWarnings("unchecked")
            Map<String, Object> m = o instanceof Map<?, ?> ? (Map<String, Object>) o : null;
            return m;
        } catch (IOException | RuntimeException e) {
            return null;
        }
    }

    /** JSON 값을 맵·목록·문자열·숫자·불리언·null로 바꾼다. */
    public static Object toJava(JsonValue v) {
        if (v instanceof JsonValue.JsonString s) return s.value();
        if (v instanceof JsonValue.JsonArray a) {
            List<Object> out = new ArrayList<>();
            for (JsonValue e : a.elements()) out.add(toJava(e));
            return out;
        }
        if (v instanceof JsonValue.JsonObject o) {
            Map<String, Object> out = new LinkedHashMap<>();
            for (Map.Entry<String, JsonValue> e : o.properties().entrySet()) out.put(e.getKey(), toJava(e.getValue()));
            return out;
        }
        String raw = ((JsonValue.JsonLiteral) v).raw();
        return switch (raw) {
            case "true" -> Boolean.TRUE;
            case "false" -> Boolean.FALSE;
            case "null" -> null;
            default -> {
                try {
                    yield Long.valueOf(raw);
                } catch (NumberFormatException e) {
                    yield Double.valueOf(raw);
                }
            }
        };
    }

    /** 자원을 보여 주기용 칸 맵으로 줄인다(비교 화면이 쓴다). 지침은 길어서 그대로 싣는다. */
    public static Map<String, Object> view(Resource r) {
        if (r == null) return null;
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
        m.put("body", r.body());
        return m;
    }
}
