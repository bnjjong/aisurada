package withlog.cmd;

import withlog.args.JsonCompact;
import withlog.args.JsonReader;
import withlog.args.JsonValue;
import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.out.JsonWriter;
import withlog.text.Trim;
import withlog.text.Utf16;
import withlog.vault.AgendaReader;
import withlog.write.AtomicWriter;
import withlog.write.TemplateFileBytes;
import withlog.write.TestApparatus;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Instant;
import java.time.ZoneOffset;
import java.time.format.DateTimeFormatter;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.regex.Pattern;

/**
 * 설명서 23장 init-workspace 명령. vault 폴더 1개를 워크스페이스로 갖춥니다.
 * 골격 폴더를 만들고, 자리가 빈 템플릿만 심고, 맨 마지막에 표식 파일을 없을 때만 쓴다.
 * 표식을 맨 끝에 쓰는 것이 핵심이다. 중간에 멈춘 폴더가 vault가 아닌 채로 남아야 다시 불렀을 때 마저 끝나기 때문이다.
 */
public final class InitWorkspace {

    public static final String USAGE = "init-workspace [name=<워크스페이스 이름>] [templates=<빌트인 템플릿 JSON 배열>]";

    /** 명령 표면에는 도구 표면의 항목 수 검사가 없어서 여기 따로 둔다(23.3). */
    private static final int MAX_TEMPLATES = 64;

    /** 23.5의 골격 폴더. 이 차례로 만들고 1개를 못 만들면 그 앞의 폴더만 남는다. */
    private static final List<String> SKELETON = List.of(
            ".withlog/agendas", ".withlog/templates/doc", ".withlog/templates/ritual", "notes");

    /** 갈래마다 칸을 보는 차례(23.4의 8번과 11번). category는 따로 먼저 본다. */
    private static final List<String> DOC_FIELDS = List.of("id", "name", "format", "desc", "body", "type", "builtin");
    private static final List<String> RITUAL_FIELDS = List.of("id", "name", "format", "desc", "body", "rev", "icon");

    private static final List<String> FORMATS = List.of("markdown", "html", "json");
    private static final List<String> TYPES = List.of("plan", "build", "report", "research", "meeting", "free");
    private static final List<String> REVS = List.of("morning", "evening", "weekly");
    private static final Pattern ID_SHAPE = Pattern.compile("^[A-Za-z0-9._-]{1,128}$");
    private static final Pattern ICON_SHAPE = Pattern.compile("^[A-Za-z][A-Za-z0-9]{0,63}$");

    private static final DateTimeFormatter CREATED_AT =
            DateTimeFormatter.ofPattern("uuuu-MM-dd'T'HH:mm:ss.SSS'Z'").withZone(ZoneOffset.UTC);

    /** 모양 검사를 지난 템플릿 항목 1개. 문서 항목이면 rev와 icon이 null이고, 리추얼 항목이면 type이 null이다. */
    private record Seed(String category, String id, String name, String format, String desc, String body,
                        String type, boolean builtin, String rev, String icon) {}

    private InitWorkspace() {}

    /**
     * init-workspace 명령을 실행합니다. 23.1의 1번(칸 가르기)은 인자 가르기가, 2번(색인 맞추기)과
     * 3번(vault git 쓰기 앞 처리)은 쓰기 파이프라인이 이미 마친 뒤에 불린다. 표식 확인은 이 명령만 건너뛴다.
     *
     * @param vaultRoot 1.1에서 정리한 vault 뿌리
     * @param parsed    파싱된 인자
     * @return 23.6의 응답(created, seeded, skipped)
     */
    public static Map<String, Object> execute(Path vaultRoot, ParsedArgs parsed) {
        Map<String, String> opts = parsed.namedOptions();

        // 4. templates의 모양, 다음에 name. 여기까지 서면 vault 파일은 그대로다
        List<Seed> seeds = readTemplates(opts.get("templates"));
        String name = opts.get("name");
        if (name != null && Trim.isBlank(name)) {
            // 안 준 것은 「알아서 정하라」이고 빈 글은 「이름 없는 워크스페이스」라서 뜻이 다르다
            throw new WlException(400, "워크스페이스 이름이 비었다. 이름을 주거나 아예 빼서 폴더 이름을 쓰게 한다.");
        }

        // 5. 골격 폴더. 있는 폴더는 그대로 두고, 그 자리가 폴더가 아니면 선다
        for (String dir : SKELETON) {
            Path path = vaultRoot.resolve(dir);
            try {
                Files.createDirectories(path);
            } catch (IOException e) {
                throw new WlException(500, "폴더를 만들지 못했다: " + path);
            }
        }

        // 6. 값 검사를 모두 마친 뒤 쓴다. 문서 항목을 먼저, 리추얼 항목을 나중에 본다
        List<Seed> docs = new ArrayList<>();
        List<Seed> rituals = new ArrayList<>();
        for (Seed seed : seeds) {
            ("doc".equals(seed.category()) ? docs : rituals).add(seed);
        }
        for (Seed seed : docs) {
            checkValues(seed);
        }
        for (Seed seed : rituals) {
            checkValues(seed);
        }

        List<String> seeded = new ArrayList<>();
        List<String> skipped = new ArrayList<>();
        for (Seed seed : docs) {
            // 경로 안전 검사는 그 항목을 쓰는 차례에, 자리가 차 있는지 보기 전에 한다(23.9의 1번)
            if (!AgendaReader.isValidSlugSegment(seed.id())) {
                throw new WlException(400, "템플릿 id 가 안전한 경로 세그먼트가 아니다: " + seed.id());
            }
            plant(vaultRoot, TemplateFileBytes.docRelPath(seed.id(), seed.format()),
                    TemplateFileBytes.renderDoc(seed.id(), seed.name(), seed.type(), seed.format(), seed.desc(),
                            seed.builtin(), seed.body()),
                    seeded, skipped);
        }
        // 리추얼 항목의 id는 경로 안전 검사를 안 받는다. 파일 이름이 id가 아니라 rev이기 때문이다(23.9의 8번)
        for (Seed seed : rituals) {
            plant(vaultRoot, TemplateFileBytes.ritualRelPath(seed.rev()),
                    TemplateFileBytes.renderRitual(seed.id(), seed.rev(), seed.name(), seed.icon(), seed.format(),
                            seed.desc(), seed.body()),
                    seeded, skipped);
        }

        // 7. 표식은 없을 때만 쓴다. 내용은 안 보고 자리가 차 있는지만 본다
        Path marker = vaultRoot.resolve(".withlog/workspace.json");
        boolean created = !Files.exists(marker);
        if (created) {
            AtomicWriter.writeAtomic(marker, markerBytes(name != null ? name : folderName(vaultRoot)));
        }

        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("created", created);
        resp.put("seeded", seeded);
        resp.put("skipped", skipped);
        return resp;
    }

    /**
     * 23.3의 templates 모양 검사. 안 줬으면 항목 0개다. 칸 이름과 값의 타입만 보고 값의 뜻은 안 본다.
     * 읽개는 kotlinx.serialization 1.6.3의 기본 설정과 같은 {@link JsonReader}다. 따옴표 없는 값은 문자열이 아닌 리터럴이다.
     */
    private static List<Seed> readTemplates(String raw) {
        if (raw == null) {
            return List.of();
        }
        JsonValue value;
        try {
            value = JsonReader.parseFile(raw);
        } catch (IllegalArgumentException e) {
            value = null;
        }
        if (!(value instanceof JsonValue.JsonArray array)) {
            throw new WlException(400, "templates 은 JSON 배열이어야 한다: " + raw);
        }
        if (array.elements().size() > MAX_TEMPLATES) {
            throw new WlException(400, "templates 항목이 너무 많다(최대 " + MAX_TEMPLATES + "건): "
                    + array.elements().size() + "건");
        }
        List<Seed> seeds = new ArrayList<>(array.elements().size());
        for (JsonValue element : array.elements()) {
            seeds.add(readSeed(element));
        }
        return seeds;
    }

    /** 항목 1개를 객체인지, category, 모르는 키, 칸 차례로 본다(23.4의 8번). */
    private static Seed readSeed(JsonValue element) {
        if (!(element instanceof JsonValue.JsonObject object)) {
            throw new WlException(400, "templates 항목이 객체가 아니다: " + JsonCompact.compact(element));
        }
        Map<String, JsonValue> props = object.properties();
        JsonValue categoryValue = props.get("category");
        if (!(categoryValue instanceof JsonValue.JsonString category)) {
            throw new WlException(400, "templates 항목의 category 가 문자열이 아니다: " + compactOrNull(categoryValue));
        }
        boolean doc;
        if ("doc".equals(category.value())) {
            doc = true;
        } else if ("ritual".equals(category.value())) {
            doc = false;
        } else {
            throw new WlException(400, "templates 항목의 category 를 모른다: " + category.value()
                    + ". 쓸 수 있는 것은 [doc, ritual] 뿐이다.");
        }

        // 버려진 키가 곧 파일에 안 담긴 값이라 모르는 키는 거절한다
        List<String> fields = doc ? DOC_FIELDS : RITUAL_FIELDS;
        List<String> allowed = new ArrayList<>(fields);
        allowed.add("category");
        allowed.sort(Utf16::compare);
        List<String> unknown = new ArrayList<>();
        for (String key : props.keySet()) {
            if (!allowed.contains(key)) {
                unknown.add(key);
            }
        }
        if (!unknown.isEmpty()) {
            unknown.sort(Utf16::compare);
            throw new WlException(400, "templates 항목에 모르는 키가 있다: [" + String.join(", ", unknown)
                    + "]. 쓸 수 있는 것은 [" + String.join(", ", allowed) + "] 뿐이다.");
        }

        Map<String, String> strings = new LinkedHashMap<>();
        boolean builtin = false;
        for (String field : fields) {
            JsonValue v = props.get(field);
            if ("builtin".equals(field)) {
                // 따옴표 없는 true와 false 글자만 불리언이다. True와 "true"는 거절한다
                if (!(v instanceof JsonValue.JsonLiteral literal)
                        || !("true".equals(literal.raw()) || "false".equals(literal.raw()))) {
                    throw new WlException(400, "templates 항목의 builtin 이 boolean 이 아니다: " + compactOrNull(v));
                }
                builtin = "true".equals(literal.raw());
            } else {
                if (!(v instanceof JsonValue.JsonString s)) {
                    throw new WlException(400, "templates 항목의 " + field + " 가 문자열이 아니다: " + compactOrNull(v));
                }
                strings.put(field, s.value());
            }
        }
        return new Seed(category.value(), strings.get("id"), strings.get("name"), strings.get("format"),
                strings.get("desc"), strings.get("body"), strings.get("type"), builtin,
                strings.get("rev"), strings.get("icon"));
    }

    /** 키가 없으면 null 글자를 적는다. */
    private static String compactOrNull(JsonValue value) {
        return value == null ? "null" : JsonCompact.compact(value);
    }

    /**
     * 23.3의 값 검사. 모양 검사와 같은 칸 차례로 본다. 디스크를 읽을 때(2.4)는 모르는 type과 rev를 기본값으로 접지만
     * 여기서는 거절한다. 방금 값을 보낸 쪽이 있고, 리추얼 rev의 오타가 morning으로 접히면 엉뚱한 본문이 morning.md로 심기기 때문이다.
     */
    private static void checkValues(Seed seed) {
        if (Trim.isBlank(seed.id()) || !ID_SHAPE.matcher(seed.id()).matches()) {
            throw new WlException(400, "템플릿 id 모양이 아니다: " + seed.id());
        }
        if (Trim.isBlank(seed.name())) {
            throw new WlException(400, "템플릿 이름이 비었다");
        }
        if (!FORMATS.contains(seed.format())) {
            throw new WlException(400, "모르는 템플릿 포맷이다: " + seed.format()
                    + ". 쓸 수 있는 것은 [markdown, html, json] 뿐이다.");
        }
        if ("doc".equals(seed.category())) {
            if (!TYPES.contains(seed.type())) {
                throw new WlException(400, "모르는 노트 유형이다: " + seed.type()
                        + ". 쓸 수 있는 것은 [plan, build, report, research, meeting, free] 뿐이다.");
            }
            return;
        }
        if (!REVS.contains(seed.rev())) {
            throw new WlException(400, "모르는 리추얼 종류다: " + seed.rev()
                    + ". 쓸 수 있는 것은 [morning, evening, weekly] 뿐이다.");
        }
        if (!ICON_SHAPE.matcher(seed.icon()).matches()) {
            throw new WlException(400, "아이콘 이름 모양이 아니다: " + seed.icon());
        }
    }

    /**
     * 자리가 비었으면 쓰고 {@code seeded}에, 차 있으면 {@code skipped}에 적는다. 「차 있다」는 파일 시스템에 묻는다(0.3).
     * 같은 id의 템플릿이 다른 이름으로 있는지는 안 본다. 손편집으로 id를 바꾼 파일 때문에 차 있는 자리를 비었다고 보고 덮게 되기 때문이다.
     */
    private static void plant(Path vaultRoot, String relPath, byte[] bytes, List<String> seeded, List<String> skipped) {
        Path target = vaultRoot.resolve(relPath);
        if (Files.exists(target)) {
            skipped.add(relPath);
            return;
        }
        AtomicWriter.writeAtomic(target, bytes);
        seeded.add(relPath);
    }

    /** 1.1의 표식 바이트. createdAt은 밀리초 3자리의 UTC 시각이고, 옛 구현의 toISOString() 바이트를 따른다. */
    private static byte[] markerBytes(String name) {
        Map<String, Object> marker = new LinkedHashMap<>();
        marker.put("schemaVersion", 1);
        marker.put("name", name);
        marker.put("createdAt", CREATED_AT.format(Instant.ofEpochMilli(TestApparatus.nowMillis())));
        return JsonWriter.toFileBytes(marker);
    }

    /** 정리한 vault 경로의 마지막 조각. 뿌리가 /면 빈 글이고, 빈 글이어도 거절하지 않는다. */
    private static String folderName(Path vaultRoot) {
        Path fileName = vaultRoot.getFileName();
        return fileName == null ? "" : fileName.toString();
    }
}
