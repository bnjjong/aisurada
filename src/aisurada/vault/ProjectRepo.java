package aisurada.vault;

import aisurada.args.JsonCompact;
import aisurada.args.JsonReader;
import aisurada.args.JsonValue;
import aisurada.cli.AisException;
import aisurada.text.Trim;

import java.util.ArrayList;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.regex.Pattern;

/**
 * 프로젝트가 가진 저장소 하나. 프로젝트 파일의 {@code repos:} 줄에 JSON 배열로 한 줄에 적히고, 아젠다는 {@code projectRepo}에
 * 이 {@code name}을 적어 그중 하나를 가리킵니다. <b>외부 설명서에 없는 확장</b>입니다.
 *
 * <p>한 줄에 적는 까닭: 프로젝트와 아젠다 파일은 설명서 2.1의 줄 읽개로 읽는데, 이 읽개는 줄 맨 앞의 {@code 키: 값}만 보고 들여쓴 줄은
 * 건너뜁니다. 들여쓴 YAML 목록을 쓰면 읽개에는 보이지 않으므로, 읽개가 값으로 받는 한 줄 흐름식 목록으로 적습니다. JSON은 YAML이기도 해서
 * 일반 YAML 도구도 읽습니다.
 *
 * @param name     프로젝트 안에서 저장소를 가리키는 이름. 아젠다의 {@code projectRepo}가 쓴다
 * @param path     이 컴퓨터의 로컬 폴더 절대 경로. 없을 수 있다
 * @param github   GitHub 저장소 {@code owner/name}. 없을 수 있다
 * @param githubId GitHub 저장소의 숫자 ID. 이름이 바뀌어도 같은 저장소를 알아보려고 둔다. {@code github}가 있을 때만 의미가 있다
 */
public record ProjectRepo(String name, String path, String github, Long githubId) {

    /** 프로젝트 하나가 가질 수 있는 저장소 수. 파일 크기 상한(100,000바이트)에 한참 못 미치게 둔다. */
    public static final int MAX_ENTRIES = 20;
    private static final int MAX_PATH_CHARS = 1024;
    private static final Pattern GITHUB = Pattern.compile("^[A-Za-z0-9](?:[A-Za-z0-9-]{0,38})/[A-Za-z0-9._-]{1,100}$");
    private static final Set<String> KEYS = Set.of("name", "path", "github", "githubId");

    /**
     * 명령 인자(JSON 배열)를 엄격하게 읽습니다. 틀린 곳이 있으면 AIS-400으로 멈춥니다. 빈 글은 빈 목록입니다.
     * 이름은 겹치면 안 되고, 항목마다 path나 github 가운데 하나는 있어야 하며, path는 절대 경로, github는 {@code owner/name}이어야 합니다.
     */
    public static List<ProjectRepo> parseStrict(String text) {
        if (text == null || Trim.isBlank(text)) {
            return List.of();
        }
        JsonValue parsed;
        try {
            parsed = JsonReader.parse(text);
        } catch (RuntimeException e) {
            throw new AisException(400, "repos 는 JSON 배열이어야 한다: " + e.getMessage());
        }
        if (!(parsed instanceof JsonValue.JsonArray array)) {
            throw new AisException(400, "repos 는 JSON 배열이어야 한다");
        }
        if (array.elements().size() > MAX_ENTRIES) {
            throw new AisException(400, "repos 는 " + MAX_ENTRIES + "개까지다: " + array.elements().size());
        }
        List<ProjectRepo> result = new ArrayList<>();
        Set<String> names = new HashSet<>();
        for (int i = 0; i < array.elements().size(); i++) {
            ProjectRepo repo = entry(array.elements().get(i), i + 1, true);
            if (!names.add(repo.name())) {
                throw new AisException(400, "repos 의 이름이 겹친다: " + repo.name());
            }
            result.add(repo);
        }
        return List.copyOf(result);
    }

    /**
     * 파일의 {@code repos:} 값을 너그럽게 읽습니다. 못 읽는 항목과 겹친 이름은 건너뛰고, 값 전체가 JSON 배열이 아니면 빈 목록입니다.
     * 손으로 고친 파일 때문에 다른 명령이 서면 안 되기 때문입니다.
     */
    public static List<ProjectRepo> parseLenient(String value) {
        if (value == null || Trim.isBlank(value)) {
            return List.of();
        }
        JsonValue parsed;
        try {
            parsed = JsonReader.parse(Trim.trim(value));
        } catch (RuntimeException e) {
            return List.of();
        }
        if (!(parsed instanceof JsonValue.JsonArray array)) {
            return List.of();
        }
        List<ProjectRepo> result = new ArrayList<>();
        Set<String> names = new HashSet<>();
        for (JsonValue element : array.elements()) {
            if (result.size() >= MAX_ENTRIES) {
                break;
            }
            try {
                ProjectRepo repo = entry(element, 0, false);
                if (names.add(repo.name())) {
                    result.add(repo);
                }
            } catch (AisException ignored) {
                // 못 읽는 항목은 없는 것으로 본다
            }
        }
        return List.copyOf(result);
    }

    private static ProjectRepo entry(JsonValue element, int index, boolean strict) {
        String where = "repos[" + index + "]";
        if (!(element instanceof JsonValue.JsonObject object)) {
            throw new AisException(400, where + " 는 객체여야 한다");
        }
        for (String key : object.properties().keySet()) {
            if (!KEYS.contains(key)) {
                throw new AisException(400, where + " 의 모르는 키다: " + key + ". 쓸 수 있는 것은 [name, path, github, githubId] 뿐이다.");
            }
        }
        String name = text(object, "name", where);
        if (name == null || name.isEmpty()) {
            throw new AisException(400, where + " 에 name 이 필요하다");
        }
        if (name.length() > 50 || !AgendaReader.isValidSlugSegment(name)) {
            throw new AisException(400, where + " 의 name 이 안전한 이름이 아니다(50자까지, 빗금 없음): " + name);
        }
        String path = text(object, "path", where);
        if (path != null && path.isEmpty()) {
            path = null;
        }
        if (path != null && (!path.startsWith("/") || path.length() > MAX_PATH_CHARS || path.indexOf('\n') >= 0 || path.indexOf('\r') >= 0)) {
            throw new AisException(400, where + " 의 path 는 이 컴퓨터의 절대 경로여야 한다: " + path);
        }
        String github = text(object, "github", where);
        if (github != null && github.isEmpty()) {
            github = null;
        }
        if (github != null && !GITHUB.matcher(github).matches()) {
            throw new AisException(400, where + " 의 github 는 owner/name 모양이어야 한다: " + github);
        }
        Long githubId = null;
        JsonValue rawId = object.properties().get("githubId");
        if (rawId != null && !(rawId instanceof JsonValue.JsonLiteral lit && lit.isNull())) {
            if (!(rawId instanceof JsonValue.JsonLiteral literal) || !literal.raw().matches("^[0-9]{1,18}$") || Long.parseLong(literal.raw()) <= 0) {
                throw new AisException(400, where + " 의 githubId 는 양의 정수여야 한다");
            }
            githubId = Long.parseLong(literal.raw());
        }
        if (githubId != null && github == null) {
            throw new AisException(400, where + " 의 githubId 는 github 와 함께 줘야 한다");
        }
        if (path == null && github == null) {
            throw new AisException(400, where + " 에 path 나 github 가운데 하나는 있어야 한다");
        }
        return new ProjectRepo(name, path, github, githubId);
    }

    private static String text(JsonValue.JsonObject object, String key, String where) {
        JsonValue value = object.properties().get(key);
        if (value == null || (value instanceof JsonValue.JsonLiteral lit && lit.isNull())) {
            return null;
        }
        if (!(value instanceof JsonValue.JsonString s)) {
            throw new AisException(400, where + " 의 " + key + " 는 글이어야 한다");
        }
        return Trim.trim(s.value());
    }

    /** 파일에 적는 한 줄 JSON. 키 차례는 name, path, github, githubId이고 없는 값은 키째 뺀다. 빈 목록은 빈 글이다. */
    public static String toJson(List<ProjectRepo> repos) {
        if (repos == null || repos.isEmpty()) {
            return "";
        }
        StringBuilder sb = new StringBuilder("[");
        for (int i = 0; i < repos.size(); i++) {
            if (i > 0) {
                sb.append(',');
            }
            ProjectRepo repo = repos.get(i);
            sb.append("{\"name\":");
            JsonCompact.appendString(sb, repo.name());
            if (repo.path() != null) {
                sb.append(",\"path\":");
                JsonCompact.appendString(sb, repo.path());
            }
            if (repo.github() != null) {
                sb.append(",\"github\":");
                JsonCompact.appendString(sb, repo.github());
            }
            if (repo.githubId() != null) {
                sb.append(",\"githubId\":").append(repo.githubId());
            }
            sb.append('}');
        }
        return sb.append(']').toString();
    }

    /** 응답에 싣는 모양. 없는 값은 키째 뺀다. */
    public Map<String, Object> toMap() {
        Map<String, Object> map = new LinkedHashMap<>();
        map.put("name", name);
        if (path != null) {
            map.put("path", path);
        }
        if (github != null) {
            map.put("github", github);
        }
        if (githubId != null) {
            map.put("githubId", githubId);
        }
        return map;
    }

    /** 이름으로 찾는다. 없으면 null이다. */
    public static ProjectRepo find(List<ProjectRepo> repos, String name) {
        if (repos == null || name == null) {
            return null;
        }
        for (ProjectRepo repo : repos) {
            if (repo.name().equals(name)) {
                return repo;
            }
        }
        return null;
    }
}
