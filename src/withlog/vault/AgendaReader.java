package withlog.vault;

import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.text.Utf16;

import java.io.IOException;
import java.nio.file.DirectoryStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * 아젠다 디렉터리(.withlog/agendas/) 훑기 및 줄 단위 파서.
 * 설명서 2.1에 따라 아젠다 파일의 프론트매터 및 메타데이터를 추출합니다.
 */
public final class AgendaReader {
    private static final Pattern ID_PATTERN = Pattern.compile("^ag[0-9a-z]+-[0-9a-f]{8}$");

    /**
     * 설명서 20.3의 색 12개. 표의 차례가 자동 배정의 차례라 목록으로 둔다.
     * 파일에서 읽은 모르는 색은 blue로 물러서고(2.1), 명령이 받은 모르는 색은 거절한다(20.3).
     */
    public static final List<String> HUES = List.of(
            "blue", "violet", "teal", "orange", "pink", "green",
            "red", "amber", "lime", "cyan", "fuchsia", "brown"
    );

    /**
     * 아젠다 훑기 결과 DTO.
     * @param table 파싱된 아젠다 매핑 테이블
     * @param skipped 아젠다 훑기 중 건너뛴 파일 정보 목록 (kind, path, reason)
     */
    public record ScanResult(
            AgendaTable table,
            List<Map<String, String>> skipped
    ) {}

    private AgendaReader() {}

    /**
     * vault 디렉터리 내의 .withlog/agendas/를 훑어 아젠다 테이블과 건너뜀 목록을 반환합니다.
     * @param vaultRoot vault 루트 절대 경로
     * @return 아젠다 훑기 결과
     */
    public static ScanResult scanAgendas(Path vaultRoot) {
        return scanAgendas(vaultRoot, true);
    }

    /**
     * vault 디렉터리 내의 .withlog/agendas/를 훑어 아젠다 테이블과 건너뜀 목록을 반환합니다.
     * @param vaultRoot vault 루트 절대 경로
     * @param haltOnFatal 치명적 오류(중복 slug 다른 id 등) 발생 시 예외를 던질지 여부 (reindex=true, 읽기 명령=false)
     * @return 아젠다 훑기 결과
     */
    public static ScanResult scanAgendas(Path vaultRoot, boolean haltOnFatal) {
        Path agendasDir = vaultRoot.resolve(".withlog/agendas");
        List<Map<String, String>> skipped = new ArrayList<>();
        Map<String, String> slugToId = new LinkedHashMap<>();
        Map<String, String> slugToTitle = new LinkedHashMap<>();
        Map<String, AgendaTable.AgendaInfo> idToAgenda = new LinkedHashMap<>();
        Map<String, AgendaTable.AgendaInfo> slugToAgenda = new LinkedHashMap<>();

        if (!Files.exists(agendasDir)) {
            return new ScanResult(new AgendaTable(slugToId, slugToTitle, idToAgenda, slugToAgenda), skipped);
        }
        if (!Files.isDirectory(agendasDir)) {
            return new ScanResult(new AgendaTable(slugToId, slugToTitle, idToAgenda, slugToAgenda), skipped);
        }

        List<Path> files = new ArrayList<>();
        try (DirectoryStream<Path> ds = Files.newDirectoryStream(agendasDir)) {
            for (Path entry : ds) {
                if (Files.isRegularFile(entry) && entry.getFileName().toString().endsWith(".md")) {
                    files.add(entry);
                }
            }
        } catch (IOException e) {
            throw new WlException(500, "아젠다 폴더를 읽지 못했다: " + agendasDir.toAbsolutePath().normalize());
        }

        files.sort((a, b) -> Utf16.compare(a.getFileName().toString(), b.getFileName().toString()));

        for (Path file : files) {
            String relPath = vaultRoot.relativize(file).toString().replace('\\', '/');
            long size;
            try {
                size = Files.size(file);
            } catch (IOException e) {
                addSkipped(skipped, relPath, "파일을 못 읽었다");
                continue;
            }

            if (size > 1_000_000) {
                addSkipped(skipped, relPath, "파일이 크기 상한을 넘었다");
                continue;
            }

            String content;
            try {
                content = StrictUtf8.decodeStrict(Files.readAllBytes(file));
            } catch (Exception e) {
                addSkipped(skipped, relPath, "파일을 못 읽었다");
                continue;
            }

            if (content.startsWith("\uFEFF")) {
                content = content.substring(1);
            }

            Fenced fenced = splitFence(content);
            if (fenced == null) {
                addSkipped(skipped, relPath, "프론트매터가 없다");
                continue;
            }
            String bodyPart = fenced.body();

            Map<String, String> kv = parseLineReader(fenced.frontmatter());

            String fileName = file.getFileName().toString();
            String fileBase = fileName.substring(0, fileName.length() - 3);

            String rawSlugVal = kv.get("slug");
            String slug;
            if (rawSlugVal != null && !Trim.isBlank(rawSlugVal)) {
                slug = Trim.trim(rawSlugVal);
            } else {
                slug = fileBase;
            }

            if (!isValidSlugSegment(slug)) {
                addSkipped(skipped, relPath, "slug 가 안전한 경로 세그먼트가 아니다");
                continue;
            }

            String rawId = kv.get("id");
            String id = rawId != null ? Trim.trim(rawId) : "";
            if (!isValidAgendaId(id)) {
                addSkipped(skipped, relPath, "아젠다 id 가 없거나 모양이 어긋난다");
                continue;
            }

            // 제목 결정 (2.1 3번)
            String rawTitle = kv.get("title");
            String title;
            if (rawTitle != null && !Trim.isBlank(rawTitle)) {
                title = Trim.trim(rawTitle);
            } else {
                String extracted = extractTitleFromBody(bodyPart);
                if (!extracted.isEmpty()) {
                    title = extracted;
                } else {
                    title = slug;
                }
            }

            // 20.9 13번: 아젠다 제목이 공백뿐인 경우 색인 맞추기 전체를 세움
            if (Trim.isBlank(title)) {
                if (haltOnFatal) {
                    throw new WlException(400, "아젠다 제목이 비었다: " + slug);
                } else {
                    addSkipped(skipped, relPath, "아젠다 제목이 비었다");
                    continue;
                }
            }

            // 색상 결정 (2.1 4번)
            String rawHue = kv.get("hue");
            String hue = "blue";
            if (rawHue != null) {
                String th = Trim.trim(rawHue);
                if (HUES.contains(th)) {
                    hue = th;
                }
            }

            String repo = kv.get("repo");
            if (repo != null) {
                repo = Trim.trim(repo);
                if (repo.isEmpty()) repo = null;
            }

            String project = kv.get("project");
            if (project != null) {
                project = Trim.trim(project);
                if (!isValidSlugSegment(project)) {
                    project = null;
                }
            }

            String summary = Trim.trim(bodyPart);

            // 동일 slug 중복 검사: 2.9 (2번)에 따라 같은 slug에 다른 id가 있으면 예상 밖 실패(종료 코드 1)
            if (slugToId.containsKey(slug)) {
                String existingId = slugToId.get(slug);
                if (!existingId.equals(id)) {
                    if (haltOnFatal) {
                        throw new IllegalStateException("Duplicate agenda slug with different id: " + slug);
                    } else {
                        // 읽기 명령에서는 아젠다 훑기 중단 및 노트 0개 처리 (8.4, 8.7 14번)
                        return new ScanResult(new AgendaTable(Collections.emptyMap(), Collections.emptyMap(), Collections.emptyMap(), Collections.emptyMap()), skipped);
                    }
                }
            }

            AgendaTable.AgendaInfo info = new AgendaTable.AgendaInfo(id, slug, title, hue, repo, project, summary);
            slugToId.put(slug, id);
            slugToTitle.put(slug, title);
            idToAgenda.put(id, info);
            slugToAgenda.put(slug, info);
        }

        return new ScanResult(new AgendaTable(slugToId, slugToTitle, idToAgenda, slugToAgenda), skipped);
    }

    /**
     * 2.1의 4번과 5번으로 가른 프론트매터와 본문.
     *
     * @param frontmatter 여는 줄 LF 다음 글자부터 닫는 {@code \n---}의 LF까지. 그 LF를 품는다
     * @param body        닫는 {@code ---} 3글자 바로 뒤부터 끝까지
     */
    record Fenced(String frontmatter, String body) {}

    /**
     * 2.1의 4번과 5번 울타리 찾기. 맨 앞 U+FEFF를 뗀 글을 받는다. 프로젝트 파일(2.2)도 같은 규칙으로 가른다.
     * {@code ---}로 시작하지 않거나, 여는 줄 뒤 LF가 없거나, 닫는 {@code \n---}가 없거나,
     * 프론트매터가 16,000자를 넘으면 null이다. 이유 문구는 4가지 모두 「프론트매터가 없다」다.
     */
    static Fenced splitFence(String content) {
        if (!content.startsWith("---")) {
            return null;
        }
        int firstLf = content.indexOf('\n', 3);
        if (firstLf == -1) {
            return null;
        }
        int closeIndex = content.indexOf("\n---", firstLf);
        if (closeIndex == -1) {
            return null;
        }
        String frontmatter = content.substring(firstLf + 1, closeIndex + 1);
        if (frontmatter.length() > 16_000) {
            return null;
        }
        return new Fenced(frontmatter, content.substring(closeIndex + 4));
    }

    private static void addSkipped(List<Map<String, String>> skipped, String path, String reason) {
        Map<String, String> map = new LinkedHashMap<>();
        map.put("kind", "agenda");
        map.put("path", path);
        map.put("reason", reason);
        skipped.add(map);
    }

    /**
     * 설명서 20.3의 slug 모양 검사. 아젠다 slug와 프로젝트 slug에 같다.
     * 1자 이상 100자 이하이고, /와 \와 ..이 없고, .으로 시작하지 않고, U+0020 미만 글자가 없어야 한다.
     * 이 값이 경로 1조각이 되어 vault 밖이나 .withlog 아래의 다른 파일을 가리키면 안 되기 때문이다.
     *
     * @param s 다듬은 slug
     * @return 경로 1조각으로 안전하면 true
     */
    public static boolean isValidSlugSegment(String s) {
        if (s == null || s.isEmpty() || s.length() > 100) return false;
        if (s.startsWith(".")) return false;
        if (s.contains("/") || s.contains("\\") || s.contains("..")) return false;
        for (int i = 0; i < s.length(); i++) {
            if (s.charAt(i) < 0x0020) return false;
        }
        return true;
    }

    private static boolean isValidAgendaId(String id) {
        if (id == null) return false;
        if ("ag-recovered".equals(id)) return true;
        return ID_PATTERN.matcher(id).matches();
    }

    private static String extractTitleFromBody(String bodyPart) {
        if (bodyPart == null) return "";
        String trimmed = Trim.trim(bodyPart);
        if (trimmed.isEmpty()) return "";

        String firstLine;
        int cr = trimmed.indexOf('\r');
        int lf = trimmed.indexOf('\n');
        int end = -1;
        if (cr != -1 && lf != -1) end = Math.min(cr, lf);
        else if (cr != -1) end = cr;
        else if (lf != -1) end = lf;

        if (end == -1) {
            firstLine = trimmed;
        } else {
            firstLine = trimmed.substring(0, end);
        }

        // ^#+\s* 머리 떼기
        int i = 0;
        while (i < firstLine.length() && firstLine.charAt(i) == '#') {
            i++;
        }
        if (i > 0) {
            while (i < firstLine.length()) {
                char c = firstLine.charAt(i);
                if (c == ' ' || c == '\t' || c == '\u000B' || c == '\u000C') {
                    i++;
                } else {
                    break;
                }
            }
            firstLine = firstLine.substring(i);
        }
        return Trim.trim(firstLine);
    }

    /**
     * 설명서 2.1 줄 읽개(Line Reader) 구현. 프로젝트 파일(2.2)도 이 읽개를 그대로 쓴다.
     */
    static Map<String, String> parseLineReader(String text) {
        Map<String, String> result = new LinkedHashMap<>();
        // LF, CRLF, CR 단위 줄 분리
        String[] lines = text.split("\r\n|\n|\r");
        for (String line : lines) {
            if (line.isEmpty() || Trim.isBlank(line)) {
                continue;
            }
            char firstChar = line.charAt(0);
            if (Trim.isWhitespace(firstChar)) {
                continue;
            }
            if (firstChar == '#') {
                continue;
            }

            int colonIdx = line.indexOf(':');
            if (colonIdx <= 0) {
                continue;
            }

            if (colonIdx + 1 < line.length()) {
                char after = line.charAt(colonIdx + 1);
                if (after != ' ') {
                    continue;
                }
            }

            String key = Trim.trim(line.substring(0, colonIdx));
            if (key.isEmpty()) {
                continue;
            }

            String rawVal = line.substring(colonIdx + 1);
            String val = Trim.trim(rawVal);

            // 따옴표 벗기기 (2.1 6번)
            if (val.length() >= 2) {
                if (val.startsWith("'") && val.endsWith("'")) {
                    val = val.substring(1, val.length() - 1).replace("''", "'");
                } else if (val.startsWith("\"") && val.endsWith("\"")) {
                    StringBuilder sb = new StringBuilder();
                    int p = 1;
                    int end = val.length() - 1;
                    while (p < end) {
                        char c = val.charAt(p++);
                        if (c == '\\' && p < end) {
                            char next = val.charAt(p++);
                            if (next == 'n') {
                                sb.append('\n');
                            } else {
                                sb.append(next);
                            }
                        } else {
                            sb.append(c);
                        }
                    }
                    val = sb.toString();
                }
            }

            val = Trim.trim(val);
            result.put(key, val);
        }
        return result;
    }
}
