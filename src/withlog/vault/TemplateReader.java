package withlog.vault;

import withlog.text.Trim;
import withlog.text.Utf16;

import java.io.IOException;
import java.nio.file.DirectoryStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;
import java.util.regex.Pattern;

/**
 * 템플릿 파일 읽기 및 검증 엔진.
 * 설명서 2.4 및 24.5에 따라 .withlog/templates/doc/ 및 ritual/ 아래의 템플릿 파일을 읽고 건너뛴 파일 사유를 수집합니다.
 */
public final class TemplateReader {

    private static final Pattern ID_PATTERN = Pattern.compile("^[A-Za-z0-9._-]{1,128}$");
    private static final Pattern ICON_PATTERN = Pattern.compile("^[A-Za-z][A-Za-z0-9]{0,63}$");
    private static final Set<String> ALLOWED_DOC_TYPES = Set.of("plan", "build", "report", "research", "meeting", "free");

    public record TemplateRecord(
            String id,
            String category, // "doc" or "ritual"
            String name,
            String type,     // doc only
            String rev,      // ritual only
            String icon,     // ritual only
            String format,   // markdown, html, json
            String desc,
            String body,
            Boolean builtin, // doc only
            String relPath
    ) {}

    public record SkippedTemplate(
            String category,
            String relPath,
            String reason
    ) {}

    public record ScanResult(
            List<TemplateRecord> templates,
            List<SkippedTemplate> skipped
    ) {}

    private TemplateReader() {}

    public static ScanResult scan(Path vaultRoot) {
        List<TemplateRecord> templates = new ArrayList<>();
        List<SkippedTemplate> skipped = new ArrayList<>();

        scanCategory(vaultRoot, "doc", templates, skipped);
        scanCategory(vaultRoot, "ritual", templates, skipped);

        return new ScanResult(templates, skipped);
    }

    private static void scanCategory(
            Path vaultRoot,
            String category,
            List<TemplateRecord> templates,
            List<SkippedTemplate> skipped
    ) {
        Path dir = vaultRoot.resolve(".withlog").resolve("templates").resolve(category);
        if (!Files.exists(dir) || !Files.isDirectory(dir)) {
            return;
        }

        List<Path> candidateFiles = new ArrayList<>();
        try (DirectoryStream<Path> ds = Files.newDirectoryStream(dir)) {
            for (Path entry : ds) {
                // 심볼릭 링크는 따라가서 일반 파일인지 확인
                if (!Files.isRegularFile(entry)) {
                    continue;
                }
                String name = entry.getFileName().toString();
                if ("doc".equals(category)) {
                    if (name.endsWith(".md") || name.endsWith(".html") || name.endsWith(".json")) {
                        candidateFiles.add(entry);
                    }
                } else if ("ritual".equals(category)) {
                    if (name.endsWith(".md")) {
                        candidateFiles.add(entry);
                    }
                }
            }
        } catch (IOException e) {
            // 폴더 목록을 못 읽으면 0개로 보고 조용히 복귀 (2.4)
            return;
        }

        // 파일 이름의 UTF-16 순서로 정렬
        candidateFiles.sort((a, b) -> Utf16.compare(a.getFileName().toString(), b.getFileName().toString()));

        for (Path file : candidateFiles) {
            String relPath = ".withlog/templates/" + category + "/" + file.getFileName().toString();

            // 1. 크기 및 UTF-8 검증 (1,000,000 바이트)
            byte[] rawBytes;
            try {
                long size = Files.size(file);
                if (size > 1_000_000L) {
                    skipped.add(new SkippedTemplate(category, relPath, "파일을 못 읽었거나 크기 상한을 넘었다"));
                    continue;
                }
                rawBytes = Files.readAllBytes(file);
            } catch (IOException e) {
                skipped.add(new SkippedTemplate(category, relPath, "파일을 못 읽었거나 크기 상한을 넘었다"));
                continue;
            }

            String content;
            try {
                content = StrictUtf8.decodeStrict(rawBytes);
            } catch (IOException e) {
                skipped.add(new SkippedTemplate(category, relPath, "파일을 못 읽었거나 크기 상한을 넘었다"));
                continue;
            }

            // 2. 맨 앞 U+FEFF 1개 제거 및 울타리 파싱
            if (content.startsWith("\uFEFF")) {
                content = content.substring(1);
            }

            String yamlPart;
            String rawBody;
            if (content.startsWith("<!--withlog")) {
                int closeIdx = content.indexOf("\n-->", 11);
                if (closeIdx == -1) {
                    skipped.add(new SkippedTemplate(category, relPath, "프론트매터가 없거나 문법이 어긋난다"));
                    continue;
                }
                yamlPart = content.substring(11, closeIdx);
                rawBody = content.substring(closeIdx + 4);
            } else if (content.startsWith("---")) {
                int firstLf = content.indexOf('\n', 3);
                if (firstLf == -1) {
                    skipped.add(new SkippedTemplate(category, relPath, "프론트매터가 없거나 문법이 어긋난다"));
                    continue;
                }
                int closeIdx = content.indexOf("\n---", firstLf);
                if (closeIdx == -1) {
                    skipped.add(new SkippedTemplate(category, relPath, "프론트매터가 없거나 문법이 어긋난다"));
                    continue;
                }
                yamlPart = content.substring(firstLf + 1, closeIdx + 1);
                rawBody = content.substring(closeIdx + 4);
            } else {
                skipped.add(new SkippedTemplate(category, relPath, "프론트매터가 없거나 문법이 어긋난다"));
                continue;
            }

            // 3. YAML 길이 16,000자 및 YAML 파싱
            if (yamlPart.length() > 16_000) {
                skipped.add(new SkippedTemplate(category, relPath, "프론트매터가 없거나 문법이 어긋난다"));
                continue;
            }

            Map<String, Object> fm = Frontmatter.parse(yamlPart);
            if (fm == null) {
                skipped.add(new SkippedTemplate(category, relPath, "프론트매터가 없거나 문법이 어긋난다"));
                continue;
            }

            // 4. 본문 앞 줄바꿈 정리 (2.4 5번)
            String body = rawBody;
            if (body.startsWith("\r\n")) {
                body = body.substring(2);
            }
            if (body.startsWith("\n")) {
                body = body.substring(1);
            }

            // 5. id 결정 (2.4 6번)
            String id;
            Object idVal = fm.get("id");
            if (idVal instanceof String strId && !strId.isEmpty()) {
                id = strId;
            } else {
                String fn = file.getFileName().toString();
                if (fn.endsWith(".html")) {
                    id = fn.substring(0, fn.length() - 5);
                } else if (fn.endsWith(".json")) {
                    id = fn.substring(0, fn.length() - 5);
                } else if (fn.endsWith(".md")) {
                    id = fn.substring(0, fn.length() - 3);
                } else {
                    id = fn;
                }
            }

            // 6. id 경로 안전성 검증 (2.4 7번)
            boolean safe = !id.isEmpty() && id.length() <= 100 && !id.contains("/") && !id.contains("\\") &&
                    !id.contains("..") && !id.startsWith(".") && hasNoControlChars(id);
            if (!safe) {
                skipped.add(new SkippedTemplate(category, relPath, "템플릿 id 가 안전한 경로 세그먼트가 아니다: " + id));
                continue;
            }

            // 7. id 정규식 및 비공백 검증 (2.4 8번)
            boolean validShape = !Trim.isBlank(id) && ID_PATTERN.matcher(id).matches();
            if (!validShape) {
                skipped.add(new SkippedTemplate(category, relPath, "템플릿 id 모양이 어긋난다: " + id));
                continue;
            }

            // 8. 나머지 필드 파싱 (2.4 9번)
            String name;
            Object nameVal = fm.get("name");
            if (nameVal instanceof String ns) {
                String trimmed = Trim.trim(ns);
                name = trimmed.isEmpty() ? id : trimmed;
            } else {
                name = id;
            }

            String desc = (fm.get("desc") instanceof String ds) ? ds : "";

            if ("doc".equals(category)) {
                String format;
                String fn = file.getFileName().toString();
                if (fn.endsWith(".html")) {
                    format = "html";
                } else if (fn.endsWith(".json")) {
                    format = "json";
                } else {
                    format = "markdown";
                }

                String type = "build";
                Object typeVal = fm.get("type");
                if (typeVal instanceof String ts && ALLOWED_DOC_TYPES.contains(ts)) {
                    type = ts;
                }

                Boolean builtin = true;
                Object bVal = fm.get("builtin");
                if (bVal instanceof Boolean b) {
                    builtin = b;
                }

                templates.add(new TemplateRecord(id, "doc", name, type, null, null, format, desc, body, builtin, relPath));
            } else {
                String format = "html";
                Object fVal = fm.get("format");
                if ("markdown".equals(fVal)) {
                    format = "markdown";
                }

                String rev = "morning";
                Object rVal = fm.get("rev");
                if (rVal instanceof String rs && ("morning".equals(rs) || "evening".equals(rs) || "weekly".equals(rs))) {
                    rev = rs;
                }

                String icon;
                Object iVal = fm.get("icon");
                if (iVal instanceof String is && ICON_PATTERN.matcher(is).matches()) {
                    icon = is;
                } else {
                    icon = switch (rev) {
                        case "evening" -> "sunset";
                        case "weekly" -> "calendar";
                        default -> "sunrise";
                    };
                }

                templates.add(new TemplateRecord(id, "ritual", name, null, rev, icon, format, desc, body, null, relPath));
            }
        }
    }

    private static boolean hasNoControlChars(String s) {
        for (int i = 0; i < s.length(); i++) {
            if (s.charAt(i) < ' ') {
                return false;
            }
        }
        return true;
    }
}
