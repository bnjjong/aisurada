package withlog.vault;

import org.yaml.snakeyaml.LoaderOptions;
import org.yaml.snakeyaml.Yaml;
import org.yaml.snakeyaml.constructor.SafeConstructor;
import org.yaml.snakeyaml.error.YAMLException;
import withlog.text.Trim;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.*;
import java.time.format.DateTimeFormatter;
import java.time.format.DateTimeParseException;
import java.util.*;
import java.util.regex.Pattern;

/**
 * 휴지통 매니페스트 읽기 및 파싱 엔진.
 * 설명서 2.6 및 26.5에 따라 .withlog/trash/trash.json 을 읽고 유효한 항목과 건너뛴 항목을 수집합니다.
 */
public final class TrashReader {

    private static final Pattern TRASH_ID_PATTERN = Pattern.compile("^tr[0-9a-z]+-[0-9a-f]{8}$");
    private static final Pattern DOC_ID_PATTERN = withlog.text.IdShape.NOTE;
    private static final Pattern AGENDA_ID_PATTERN = Pattern.compile("^ag[0-9a-z]+-[0-9a-f]{8}$");
    private static final Set<String> ALLOWED_TYPES = Set.of("plan", "build", "report", "research", "meeting", "free");

    private static final DateTimeFormatter TRASHED_AT_FORMATTER =
            DateTimeFormatter.ofPattern("uuuu-MM-dd'T'HH:mm:ss.SSS'Z'").withZone(ZoneOffset.UTC);

    public record TrashRecord(
            String trashId,
            String docId,
            String title,
            String agendaSlug,
            String agendaId,
            String type,
            String format,
            String originalPath,
            String file,
            String created,
            String trashedAt,
            Instant trashedInstant,
            int fileOrder
    ) {}

    public record SkippedTrash(
            String location,
            String reason
    ) {}

    public record ScanResult(
            List<TrashRecord> entries,
            List<SkippedTrash> skipped
    ) {}

    private static final LoaderOptions LOADER_OPTIONS;
    static {
        LOADER_OPTIONS = new LoaderOptions();
        LOADER_OPTIONS.setCodePointLimit(1_000_000);
        LOADER_OPTIONS.setMaxAliasesForCollections(50);
        LOADER_OPTIONS.setNestingDepthLimit(50); // 1.8과 같다(Frontmatter 참고)
    }

    private TrashReader() {}

    public static ScanResult scan(Path vaultRoot) {
        Path trashJson = vaultRoot.resolve(".withlog").resolve("trash").resolve("trash.json");
        if (!Files.isRegularFile(trashJson)) {
            return new ScanResult(Collections.emptyList(), Collections.emptyList());
        }

        List<TrashRecord> entries = new ArrayList<>();
        List<SkippedTrash> skipped = new ArrayList<>();

        // 1. 크기 및 UTF-8 검증 (1,000,000 바이트)
        byte[] rawBytes;
        try {
            long size = Files.size(trashJson);
            if (size > 1_000_000L) {
                skipped.add(new SkippedTrash(".withlog/trash/trash.json", "매니페스트를 못 읽었다"));
                return new ScanResult(entries, skipped);
            }
            rawBytes = Files.readAllBytes(trashJson);
        } catch (IOException e) {
            skipped.add(new SkippedTrash(".withlog/trash/trash.json", "매니페스트를 못 읽었다"));
            return new ScanResult(entries, skipped);
        }

        String content;
        try {
            content = StrictUtf8.decodeStrict(rawBytes);
        } catch (IOException e) {
            skipped.add(new SkippedTrash(".withlog/trash/trash.json", "매니페스트를 못 읽었다"));
            return new ScanResult(entries, skipped);
        }

        if (content.startsWith("\uFEFF")) {
            content = content.substring(1);
        }

        // 2. YAML 1.1 파싱 (시각 해석 끔)
        Yaml yaml = new Yaml(new SafeConstructor(LOADER_OPTIONS));
        Object loaded;
        try {
            loaded = yaml.load(content);
        } catch (YAMLException e) {
            skipped.add(new SkippedTrash(".withlog/trash/trash.json", "매니페스트가 배열이 아니다"));
            return new ScanResult(entries, skipped);
        }

        if (!(loaded instanceof List<?> list)) {
            skipped.add(new SkippedTrash(".withlog/trash/trash.json", "매니페스트가 배열이 아니다"));
            return new ScanResult(entries, skipped);
        }

        // 3. 배열 원소 순회
        for (int i = 0; i < list.size(); i++) {
            Object item = list.get(i);
            String itemLocation = ".withlog/trash/trash.json[" + i + "]";

            if (!(item instanceof Map<?, ?> map)) {
                skipped.add(new SkippedTrash(itemLocation, "항목이 객체가 아니다"));
                continue;
            }

            // 필드 추출 (문자열이 아니면 없는 것으로 취급)
            String rawTrashId = getString(map, "trashId");
            String rawFile = getString(map, "file");
            String rawOriginalPath = getString(map, "originalPath");
            String rawDocId = getString(map, "docId");
            String rawAgendaId = getString(map, "agendaId");
            String rawAgendaSlug = getString(map, "agendaSlug");
            String rawTitle = getString(map, "title");
            String rawType = getString(map, "type");
            String rawFormat = getString(map, "format");
            String rawCreated = getString(map, "created");
            String rawTrashedAt = getString(map, "trashedAt");

            String tTrashId = Trim.trim(rawTrashId);
            String tFile = Trim.trim(rawFile);
            String tOriginalPath = Trim.trim(rawOriginalPath);
            String tDocId = Trim.trim(rawDocId);
            String tAgendaId = Trim.trim(rawAgendaId);
            String tAgendaSlug = Trim.trim(rawAgendaSlug);
            String tTitle = Trim.trim(rawTitle);
            String tType = Trim.trim(rawType);
            String tFormat = Trim.trim(rawFormat);
            String tCreated = Trim.trim(rawCreated);
            String tTrashedAt = Trim.trim(rawTrashedAt);

            // 유효성 판정
            boolean valid = true;

            if (tTrashId.isEmpty() || !TRASH_ID_PATTERN.matcher(tTrashId).matches()) {
                valid = false;
            }
            if (tFile.isEmpty()) {
                valid = false;
            }
            if (tOriginalPath.isEmpty()) {
                valid = false;
            }
            if (tDocId.isEmpty() || !DOC_ID_PATTERN.matcher(tDocId).matches()) {
                valid = false;
            }
            if (tAgendaId.isEmpty() || (!AGENDA_ID_PATTERN.matcher(tAgendaId).matches() && !"ag-recovered".equals(tAgendaId))) {
                valid = false;
            }
            if (tAgendaSlug.isEmpty()) {
                valid = false;
            }

            // created 파싱
            String formattedCreated = null;
            if (valid) {
                formattedCreated = parseAndFormatCreated(tCreated);
                if (formattedCreated == null) {
                    valid = false;
                }
            }

            // trashedAt 파싱
            Instant trashedInstant = null;
            String formattedTrashedAt = null;
            if (valid) {
                ParsedInstant pi = parseTrashedAt(tTrashedAt);
                if (pi == null) {
                    valid = false;
                } else {
                    trashedInstant = pi.instant();
                    formattedTrashedAt = pi.formatted();
                }
            }

            if (!valid) {
                String hint = "식별 불가";
                if (!tTrashId.isEmpty()) {
                    hint = tTrashId;
                } else if (!tFile.isEmpty()) {
                    hint = tFile;
                }
                skipped.add(new SkippedTrash(itemLocation, "휴지통 항목으로 세울 수 없다: " + hint));
                continue;
            }

            // title 결정
            String title = tTitle;
            if (title.isEmpty()) {
                title = tDocId;
            }

            // type 결정 (소문자, 6가지 아니면 build)
            String type = ALLOWED_TYPES.contains(tType) ? tType : "build";

            // format 결정 (html/json 아니면 markdown)
            String format = ("html".equals(tFormat) || "json".equals(tFormat)) ? tFormat : "markdown";

            entries.add(new TrashRecord(
                    tTrashId,
                    tDocId,
                    title,
                    tAgendaSlug,
                    tAgendaId,
                    type,
                    format,
                    tOriginalPath,
                    tFile,
                    formattedCreated,
                    formattedTrashedAt,
                    trashedInstant,
                    i
            ));
        }

        // 정렬: trashedInstant 내림차순, 같으면 파일 순서 (fileOrder 오름차순)
        entries.sort((e1, e2) -> {
            int cmp = e2.trashedInstant().compareTo(e1.trashedInstant());
            if (cmp != 0) return cmp;
            return Integer.compare(e1.fileOrder(), e2.fileOrder());
        });

        return new ScanResult(entries, skipped);
    }

    private static String getString(Map<?, ?> map, String key) {
        Object val = map.get(key);
        return (val instanceof String s) ? s : null;
    }

    private static String parseAndFormatCreated(String s) {
        if (s == null || s.isEmpty()) return null;
        try {
            OffsetDateTime odt = OffsetDateTime.parse(s, DateTimeFormatter.ISO_OFFSET_DATE_TIME);
            return formatCreatedOffsetDateTime(odt);
        } catch (DateTimeParseException e1) {
            try {
                LocalDateTime ldt = LocalDateTime.parse(s, DateTimeFormatter.ISO_LOCAL_DATE_TIME);
                return formatCreatedOffsetDateTime(ldt.atOffset(ZoneOffset.ofHours(9)));
            } catch (DateTimeParseException e2) {
                try {
                    LocalDate ld = LocalDate.parse(s, DateTimeFormatter.ISO_LOCAL_DATE);
                    return formatCreatedOffsetDateTime(ld.atTime(0, 0, 0).atOffset(ZoneOffset.ofHours(9)));
                } catch (DateTimeParseException e3) {
                    return null;
                }
            }
        }
    }

    private static String formatCreatedOffsetDateTime(OffsetDateTime odt) {
        int year = odt.getYear();
        String yearStr = String.format("%04d", year);
        String dtPart = String.format("%s-%02d-%02dT%02d:%02d:%02d",
                yearStr, odt.getMonthValue(), odt.getDayOfMonth(),
                odt.getHour(), odt.getMinute(), odt.getSecond());

        ZoneOffset offset = odt.getOffset();
        int totalSec = offset.getTotalSeconds();
        // 초 단위 오프셋은 분 단위로 절사 (1.9, 26.5 line 10848)
        int hours = totalSec / 3600;
        int mins = Math.abs((totalSec % 3600) / 60);

        if (hours == 0 && mins == 0) {
            return dtPart + "Z";
        }
        return String.format("%s%s%02d:%02d", dtPart, hours >= 0 ? "+" : "-", Math.abs(hours), mins);
    }

    public record ParsedInstant(Instant instant, String formatted) {}

    private static ParsedInstant parseTrashedAt(String s) {
        if (s == null || s.isEmpty()) return null;
        Instant inst = null;
        try {
            inst = Instant.parse(s);
        } catch (DateTimeParseException e1) {
            try {
                OffsetDateTime odt = OffsetDateTime.parse(s, DateTimeFormatter.ISO_OFFSET_DATE_TIME);
                inst = odt.toInstant();
            } catch (DateTimeParseException e2) {
                try {
                    LocalDateTime ldt = LocalDateTime.parse(s, DateTimeFormatter.ISO_LOCAL_DATE_TIME);
                    inst = ldt.atOffset(ZoneOffset.ofHours(9)).toInstant();
                } catch (DateTimeParseException e3) {
                    try {
                        LocalDate ld = LocalDate.parse(s, DateTimeFormatter.ISO_LOCAL_DATE);
                        inst = ld.atTime(0, 0, 0).atOffset(ZoneOffset.ofHours(9)).toInstant();
                    } catch (DateTimeParseException e4) {
                        return null;
                    }
                }
            }
        }

        String formatted = TRASHED_AT_FORMATTER.format(inst);
        return new ParsedInstant(inst, formatted);
    }
}
