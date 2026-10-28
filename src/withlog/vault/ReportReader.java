package withlog.vault;

import org.yaml.snakeyaml.LoaderOptions;
import org.yaml.snakeyaml.Yaml;
import org.yaml.snakeyaml.constructor.SafeConstructor;
import org.yaml.snakeyaml.error.YAMLException;
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
 * 회고 파일 읽기 및 검증 엔진.
 * 설명서 2.5 및 25장에 따라 .withlog/reports/ 아래의 rv-&lt;종류&gt;-&lt;YYYY-MM-DD&gt;.json 회고 파일을 읽습니다.
 */
public final class ReportReader {

    private static final Pattern CONFLICT_PATTERN = Pattern.compile(
            "conflicted copy|\\(conflicted\\b|충돌(된)?\\s*사본|conflicted version",
            Pattern.CASE_INSENSITIVE | Pattern.UNICODE_CASE
    );

    private static final Pattern FILENAME_PATTERN = Pattern.compile("^rv-([a-z]+)-([0-9]{4}-[0-9]{2}-[0-9]{2})$");
    public static final Set<String> ALLOWED_KINDS = Set.of("morning", "evening", "weekly");

    public record ReportRecord(
            String id,
            String kind,
            String date,
            boolean hasBody,
            boolean hasMemo,
            String body,
            String memo,
            String relPath
    ) {}

    public record SkippedReport(
            String relPath,
            String reason,
            boolean isConflict
    ) {}

    public record ScanResult(
            List<ReportRecord> reports,
            List<SkippedReport> skipped
    ) {}

    public sealed interface ParseResult permits ParseResult.Success, ParseResult.Failed {
        record Success(ReportRecord record) implements ParseResult {}
        record Failed(String reason, int step) implements ParseResult {}
    }

    private static final LoaderOptions LOADER_OPTIONS;
    static {
        LOADER_OPTIONS = new LoaderOptions();
        LOADER_OPTIONS.setCodePointLimit(1_000_000);
        LOADER_OPTIONS.setMaxAliasesForCollections(50);
        LOADER_OPTIONS.setNestingDepthLimit(50); // 1.8과 같다(Frontmatter 참고)
    }

    private ReportReader() {}

    public static ScanResult scan(Path vaultRoot) {
        Path dir = vaultRoot.resolve(".withlog").resolve("reports");
        if (!Files.exists(dir) || !Files.isDirectory(dir)) {
            return new ScanResult(Collections.emptyList(), Collections.emptyList());
        }

        List<Path> candidateFiles = new ArrayList<>();
        try (DirectoryStream<Path> ds = Files.newDirectoryStream(dir)) {
            for (Path entry : ds) {
                if (!Files.isRegularFile(entry)) {
                    continue;
                }
                String name = entry.getFileName().toString();
                if (name.endsWith(".json")) {
                    candidateFiles.add(entry);
                }
            }
        } catch (IOException e) {
            // 폴더 목록을 못 읽으면 25.4 차례 8번: WL-500
            throw new WlException(500, "회고 폴더를 읽지 못했다: " + dir.toAbsolutePath());
        }

        // 파일 이름의 UTF-16 순서로 정렬
        candidateFiles.sort((a, b) -> Utf16.compare(a.getFileName().toString(), b.getFileName().toString()));

        List<ReportRecord> reports = new ArrayList<>();
        List<SkippedReport> skipped = new ArrayList<>();

        for (Path file : candidateFiles) {
            String fn = file.getFileName().toString();
            String relPath = ".withlog/reports/" + fn;

            // 1. 충돌 사본 모양 검사 (25.5 1번)
            if (CONFLICT_PATTERN.matcher(fn).find()) {
                skipped.add(new SkippedReport(relPath, "동기화 충돌 사본이다", true));
                continue;
            }

            // 2. 파일 이름 형식 검사 (25.5 2번)
            String idPart = fn.substring(0, fn.length() - 5);
            Matcher m = FILENAME_PATTERN.matcher(idPart);
            if (!m.matches() || !ALLOWED_KINDS.contains(m.group(1))) {
                skipped.add(new SkippedReport(relPath, "파일 이름이 rv-<종류>-<YYYY-MM-DD> 모양이 아니다", false));
                continue;
            }

            String kind = m.group(1);
            String date = m.group(2);

            ParseResult parseResult = parseReportFile(file, idPart, kind, date, relPath);
            if (parseResult instanceof ParseResult.Success succ) {
                reports.add(succ.record());
            } else if (parseResult instanceof ParseResult.Failed fail) {
                skipped.add(new SkippedReport(relPath, fail.reason(), false));
            }
        }

        return new ScanResult(reports, skipped);
    }

    public static ParseResult parseReportFile(
            Path file,
            String id,
            String kind,
            String date,
            String relPath
    ) {
        // 3. 파일 크기 및 UTF-8 검증 (1,000,000 바이트)
        byte[] rawBytes;
        try {
            long size = Files.size(file);
            if (size > 1_000_000L) {
                return new ParseResult.Failed("파일을 못 읽었거나 크기 상한을 넘었다", 3);
            }
            rawBytes = Files.readAllBytes(file);
        } catch (IOException e) {
            return new ParseResult.Failed("파일을 못 읽었거나 크기 상한을 넘었다", 3);
        }

        String content;
        try {
            content = StrictUtf8.decodeStrict(rawBytes);
        } catch (IOException e) {
            return new ParseResult.Failed("파일을 못 읽었거나 크기 상한을 넘었다", 3);
        }

        if (content.startsWith("\uFEFF")) {
            content = content.substring(1);
        }

        // 4. YAML 1.1 파싱 (시각 해석 켬)
        Yaml yaml = new Yaml(new SafeConstructor(LOADER_OPTIONS));
        Object loaded;
        try {
            loaded = yaml.load(content);
        } catch (YAMLException e) {
            return new ParseResult.Failed("JSON 이 깨졌거나 객체가 아니다", 4);
        }

        if (!(loaded instanceof Map<?, ?> map)) {
            return new ParseResult.Failed("JSON 이 깨졌거나 객체가 아니다", 4);
        }

        // 5. body 및 memo 타입 검증
        Object rawBody = map.get("body");
        Object rawMemo = map.get("memo");

        if (rawBody != null && !(rawBody instanceof String)) {
            return new ParseResult.Failed("본문이나 메모가 문자열이 아닌 값이다", 5);
        }
        if (rawMemo != null && !(rawMemo instanceof String)) {
            return new ParseResult.Failed("본문이나 메모가 문자열이 아닌 값이다", 5);
        }

        String strBody = (String) rawBody;
        String strMemo = (String) rawMemo;

        boolean hasBody = strBody != null && !Trim.isBlank(strBody);
        boolean hasMemo = strMemo != null && !Trim.isBlank(strMemo);

        // 6. 껍데기 검증 (둘 다 없거나 공백뿐이면 껍데기)
        if (!hasBody && !hasMemo) {
            return new ParseResult.Failed("본문도 메모도 들어 있지 않은 껍데기다", 6);
        }

        ReportRecord record = new ReportRecord(
                id,
                kind,
                date,
                hasBody,
                hasMemo,
                hasBody ? strBody : null,
                hasMemo ? strMemo : null,
                relPath
        );

        return new ParseResult.Success(record);
    }
}
