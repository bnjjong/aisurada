package withlog.cmd;

import withlog.args.JsonCompact;
import withlog.args.JsonReader;
import withlog.args.JsonValue;
import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.text.Uri;
import withlog.text.Utf16;
import withlog.config.EngineConfig;
import withlog.vault.AgendaTable;
import withlog.vault.NoteFile;
import withlog.vault.NoteIndex;
import withlog.vault.NoteMeta;
import withlog.vault.NoteScanner;
import withlog.write.AgendaFile;
import withlog.write.AtomicWriter;
import withlog.write.DocToken;
import withlog.write.FrontmatterWriter;
import withlog.write.IdGenerator;
import withlog.write.PathConfinement;
import withlog.write.ShardLayout;
import withlog.write.SlugBuilder;
import withlog.write.TestApparatus;
import withlog.write.TimeScheduler;
import withlog.write.WarningsDetector;
import withlog.engine.WritePipeline;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.LocalDate;
import java.time.LocalTime;
import java.time.ZoneId;
import java.time.format.DateTimeFormatter;
import java.util.*;
import java.util.regex.Pattern;

/**
 * 설명서 13장 create_doc 및 create-doc 명령 구현체.
 * 쓰기 파이프라인(7장), 워크스페이스 확인(1.1), 색인 맞추기(8.3),
 * 아젠다 조회/생성(13.5), 시각 배정, 중복 경고 및 원자적 쓰기를 수행합니다.
 */
public final class CreateDoc {
    static final DateTimeFormatter CREATED_DATE = DateTimeFormatter.ofPattern("yyyy-MM-dd");
    static final DateTimeFormatter CREATED_MONTH = DateTimeFormatter.ofPattern("yyyy-MM");

    public static final String USAGE = "create-doc agenda=<slug 또는 이름> type=<유형> title=<제목> body=<본문> " +
            "[date=<YYYY-MM-DD>] [time=<HH:MM>] [mins=<분>] [format=markdown] [links=<JSON 배열>] " +
            "[tags=<JSON 배열>] [template=<템플릿 id>] [tier=reading|record]";

    private static final Set<String> ALLOWED_TYPES = Set.of(
            "plan", "build", "report", "research", "meeting", "free"
    );

    private static final Pattern DATE_SYNTAX_PATTERN = Pattern.compile("^(?:[0-9]{4}|\\+[0-9]{5,10}|-[0-9]{4,10})-(0[1-9]|1[0-2])-(0[1-9]|[12][0-9]|3[01])$");
    private static final Pattern TIME_SYNTAX_PATTERN = Pattern.compile("^([01][0-9]|2[0-3]):[0-5][0-9]$");
    static final Pattern TEMPLATE_ID_PATTERN = Pattern.compile("^[A-Za-z0-9._-]{1,128}$");

    private CreateDoc() {}

    /**
     * create_doc / create-doc 명령을 실행합니다.
     *
     * @param vaultRoot vault 루트 경로
     * @param parsed 파싱된 인자
     * @param sessionId 세션 식별자 (도구 표면 전용)
     * @return 13.6 응답 JSON 맵
     */
    public static Map<String, Object> execute(Path vaultRoot, ParsedArgs parsed, String sessionId) {
        boolean isTool = parsed.isToolSurface();

        // 5. 색인 맞추기 (7.1 step 3, 13.4 row 9) - 아젠다 치명 오류 시 즉시 중단
        NoteIndex noteIndex = WritePipeline.index(vaultRoot);

        // 7. 값 읽기 및 의미론 검증 (13.3, 13.4 rows 10-23)
        Map<String, String> opts = parsed.namedOptions();
        Map<String, Object> toolOpts = parsed.toolOptions();

        // Row 10: format 검증
        String rawFormat = isTool ? (String) toolOpts.get("format") : opts.get("format");
        if (rawFormat != null && !"markdown".equals(rawFormat)) {
            throw new WlException(400, "모르는 본문 포맷 이다: " + rawFormat + ". 쓸 수 있는 것은 [markdown] 뿐이다.");
        }

        // Row 11: agenda 존재 검증 (명령 표면만)
        if (!isTool && !opts.containsKey("agenda")) {
            throw new WlException(400, "agenda 이 필요하다: " + USAGE);
        }
        String rawAgenda = isTool ? (String) toolOpts.get("agenda") : opts.get("agenda");

        // Row 12: type 존재 검증 (명령 표면만)
        if (!isTool && !opts.containsKey("type")) {
            throw new WlException(400, "type 이 필요하다: " + USAGE);
        }
        String rawType = isTool ? (String) toolOpts.get("type") : opts.get("type");

        // Row 13: type 값 검증 (공통, 대소문자 무시, 미다듬기)
        String canonicalType = null;
        if (rawType != null) {
            for (String allowed : ALLOWED_TYPES) {
                if (allowed.equalsIgnoreCase(rawType)) {
                    canonicalType = allowed;
                    break;
                }
            }
        }
        if (canonicalType == null) {
            throw new WlException(400, "모르는 유형 이다: " + (rawType != null ? rawType : "") +
                    ". 쓸 수 있는 것은 [plan, build, report, research, meeting, free] 뿐이다.");
        }

        // Row 14: title 존재 검증 (명령 표면만)
        if (!isTool && !opts.containsKey("title")) {
            throw new WlException(400, "title 이 필요하다: " + USAGE);
        }
        String rawTitle = isTool ? (String) toolOpts.get("title") : opts.get("title");

        // Row 15: body 존재 검증 (명령 표면만)
        if (!isTool && !opts.containsKey("body")) {
            throw new WlException(400, "body 이 필요하다: " + USAGE);
        }
        String rawBody = isTool ? (String) toolOpts.get("body") : opts.get("body");

        // Row 16: date 형식 검증
        String rawDate = isTool ? (String) toolOpts.get("date") : opts.get("date");
        LocalDate parsedDate = null;
        if (rawDate != null) {
            parsedDate = validateAndParseDate(rawDate);
        }

        // Row 17: time 형식 검증
        String rawTime = isTool ? (String) toolOpts.get("time") : opts.get("time");
        LocalTime parsedTime = null;
        if (rawTime != null) {
            parsedTime = validateAndParseTime(rawTime);
        }

        // Row 18: links 검증 및 중복 정리
        List<Map<String, String>> validatedLinks = parseAndValidateLinks(isTool, toolOpts, opts);

        // Row 19: tags 검증 및 중복 정리
        List<String> validatedTags = parseAndValidateTags(isTool, toolOpts, opts);

        // Row 20 & 23: mins 검증 (명령 표면만 받음)
        int mins = 0;
        if (!isTool && opts.containsKey("mins")) {
            String rawMins = opts.get("mins");
            String trimmedMins = Trim.trim(rawMins);
            if (!trimmedMins.isEmpty()) {
                mins = parseMins(trimmedMins);
            }
        }

        // Row 21: tier 검증 (명령 표면만 받음)
        String tier = "reading";
        if (!isTool && opts.containsKey("tier")) {
            String rawTier = opts.get("tier");
            if ("reading".equalsIgnoreCase(rawTier)) {
                tier = "reading";
            } else if ("record".equalsIgnoreCase(rawTier)) {
                tier = "record";
            } else {
                throw new WlException(400, "모르는 노트 층 이다: " + (rawTier != null ? rawTier : "") +
                        ". 쓸 수 있는 것은 [reading, record] 뿐이다.");
            }
        }

        // Row 22: title 공백 여부 검증
        if (Trim.isBlank(rawTitle)) {
            throw new WlException(400, "노트 제목이 비었다");
        }

        // Row 23: mins 음수 검증
        if (mins < 0) {
            throw new WlException(400, "노트 소요 시간이 음수다: " + mins);
        }

        // template 처리: 다듬어서 regex 일치 시 사용, 불일치 시 버림 (13.3)
        String rawTemplate = isTool ? (String) toolOpts.get("template") : opts.get("template");
        String validatedTemplate = null;
        if (rawTemplate != null) {
            String trimmedTpl = Trim.trim(rawTemplate);
            if (TEMPLATE_ID_PATTERN.matcher(trimmedTpl).matches()) {
                validatedTemplate = trimmedTpl;
            }
        }

        // 8. 아젠다 찾기 또는 생성 (13.5, Row 24)
        String agendaKey = Trim.trim(rawAgenda);
        if (agendaKey.isEmpty()) {
            throw new WlException(400, "아젠다 이름이 비었다");
        }

        AgendaLookupResult agendaResult = lookupOrCreateAgenda(vaultRoot, noteIndex.agendaTable(), agendaKey);
        String agendaSlug = agendaResult.slug();
        boolean agendaCreated = agendaResult.created();

        // 9. 시각 배정, 같은 제목 찾기, 노트 쓰기 및 색인 (13.5)
        ZoneId zone = ZoneId.systemDefault();
        LocalDate noteDate = parsedDate != null ? parsedDate : TestApparatus.today(zone);
        LocalTime noteTime;
        if (parsedTime != null) {
            noteTime = parsedTime;
        } else {
            noteTime = TimeScheduler.scheduleTime(noteDate, noteIndex.allRootNotes(), zone);
        }

        String timeStr = TimeScheduler.formatTime(noteTime);
        // 1.9: created의 연도는 기원 연도 꼴(yyyy)이라 0년은 0001, 기원전 1년은 0002로 찍힌다(13.9의 13번).
        // 폴더 이름과 응답 date는 ISO 날짜 그대로다.
        String createdStr = CREATED_DATE.format(noteDate) + "T" + timeStr + ":00+09:00";

        // 같은 아젠다 메인 노트 수집 및 경고 판정
        List<NoteScanner.NoteRecord> sameAgendaNotes = new ArrayList<>();
        for (NoteScanner.NoteRecord nr : noteIndex.allRootNotes()) {
            if (nr.meta() != null && agendaSlug.equals(nr.meta().agendaSlug())) {
                sameAgendaNotes.add(nr);
            }
        }

        String warning = WarningsDetector.detectWarnings(isTool, rawBody, rawTitle, sameAgendaNotes);

        // 노트 폴더 및 파일 경로 계산
        boolean isSharded = ShardLayout.isSharded(vaultRoot);
        String relParent;
        if (isSharded) {
            // 1.3: 샤드 연월은 created의 연월이다
            String ym = CREATED_MONTH.format(noteDate);
            relParent = "notes/" + agendaSlug + "/" + ym;
        } else {
            relParent = "notes/" + agendaSlug;
        }

        String titleSlug = SlugBuilder.build(rawTitle, 50);
        String noteId = IdGenerator.nextNoteId();
        String tail = extractIdTail(noteId);
        String timeDigits = String.format("%02d%02d", noteTime.getHour(), noteTime.getMinute());
        String folderName = noteDate + "-" + timeDigits + "-" + titleSlug + "-" + tail;
        String noteRelPath = relParent + "/" + folderName;

        // Row 29: 폴더 자리를 경로로 못 지음 (서로게이트 쌍 분리 등)
        if (PathConfinement.hasUnusablePathCharacters(noteRelPath)) {
            throw new WlException(400, "경로로 쓸 수 없는 값이다: " + noteRelPath);
        }

        Path noteDir = vaultRoot.resolve(noteRelPath);
        PathConfinement.checkConfinement(vaultRoot, noteDir);

        // 프론트매터 조립 및 상한 검사
        String frontmatter = FrontmatterWriter.buildNoteFrontmatter(
                noteId,
                agendaSlug,
                canonicalType,
                rawTitle,
                createdStr,
                isTool ? sessionId : null,
                mins,
                tier,
                validatedTags,
                validatedTemplate,
                validatedLinks
        );

        String fullFileContent = "---\n" + frontmatter + "---\n" + rawBody;

        // Row 28: 노트 파일이 저장 상한을 넘음 (UTF-16 코드 단위 1,000,000 초과)
        if (fullFileContent.length() > 1_000_000) {
            throw new WlException(500, "노트가 저장 상한을 넘어 저장하지 않았다: " + noteRelPath);
        }

        byte[] noteBytes = Utf16.toUtf8BytesWithQuestionForUnpaired(fullFileContent);
        Path noteFile = noteDir.resolve("note.md");

        // Row 30: 원자적 쓰기
        AtomicWriter.writeAtomic(noteFile, noteBytes);

        String precondition = DocToken.compute(noteBytes);

        // 쓴 뒤의 색인 맞추기 (13.5)
        reindexWrittenNote(vaultRoot, noteRelPath, noteId, noteIndex);

        // 13.6 응답 조립
        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("id", noteId);
        resp.put("agenda", agendaSlug);
        resp.put("agendaCreated", agendaCreated);
        resp.put("date", noteDate.toString());
        resp.put("path", noteRelPath);
        resp.put("precondition", precondition);
        resp.put("deepLink", "withlog://doc/" + Uri.encodeURIComponent(noteId) + "?ws=" + Uri.encodeURIComponent(vaultRoot.toString()));
        if (warning != null) {
            resp.put("warning", warning);
        }

        return resp;
    }

    static LocalDate validateAndParseDate(String dateStr) {
        if (!DATE_SYNTAX_PATTERN.matcher(dateStr).matches() || dateStr.startsWith("-0000-")) {
            throw new WlException(400, "date 은 YYYY-MM-DD 모양이어야 한다: " + dateStr);
        }

        // 그레고리력 달력 유효성 검사
        try {
            int firstDash = dateStr.indexOf('-');
            if (dateStr.startsWith("+") || dateStr.startsWith("-")) {
                firstDash = dateStr.indexOf('-', 1);
            }
            int secondDash = dateStr.indexOf('-', firstDash + 1);

            int year = Integer.parseInt(dateStr.substring(0, firstDash));
            int month = Integer.parseInt(dateStr.substring(firstDash + 1, secondDash));
            int day = Integer.parseInt(dateStr.substring(secondDash + 1));

            if (month < 1 || month > 12) {
                throw new WlException(400, "date 은 YYYY-MM-DD 모양이어야 한다: " + dateStr);
            }
            int maxDays = getDaysInMonth(year, month);
            if (day < 1 || day > maxDays) {
                throw new WlException(400, "date 은 YYYY-MM-DD 모양이어야 한다: " + dateStr);
            }
            return LocalDate.of(year, month, day);
        } catch (Exception e) {
            throw new WlException(400, "date 은 YYYY-MM-DD 모양이어야 한다: " + dateStr);
        }
    }

    private static int getDaysInMonth(int year, int month) {
        return switch (month) {
            case 1, 3, 5, 7, 8, 10, 12 -> 31;
            case 4, 6, 9, 11 -> 30;
            case 2 -> isLeapYear(year) ? 29 : 28;
            default -> 0;
        };
    }

    private static boolean isLeapYear(int year) {
        return (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    }

    static LocalTime validateAndParseTime(String timeStr) {
        if (!TIME_SYNTAX_PATTERN.matcher(timeStr).matches()) {
            throw new WlException(400, "time 은 HH:MM 모양이어야 한다: " + timeStr);
        }
        int colon = timeStr.indexOf(':');
        int hour = Integer.parseInt(timeStr.substring(0, colon));
        int minute = Integer.parseInt(timeStr.substring(colon + 1));
        return LocalTime.of(hour, minute);
    }

    static int parseMins(String trimmed) {
        // 부호 0~1개 뒤 Unicode Nd 숫자 1개 이상
        int start = 0;
        boolean negative = false;
        if (trimmed.startsWith("+")) {
            start = 1;
        } else if (trimmed.startsWith("-")) {
            start = 1;
            negative = true;
        }

        if (start >= trimmed.length()) {
            throw new WlException(400, "mins 은 숫자여야 한다: " + trimmed);
        }

        long val = 0;
        for (int i = start; i < trimmed.length(); i++) {
            char c = trimmed.charAt(i);
            int digit = Character.digit(c, 10);
            if (digit < 0) {
                throw new WlException(400, "mins 은 숫자여야 한다: " + trimmed);
            }
            val = val * 10 + digit;
            if (val > (long) Integer.MAX_VALUE + 1L) {
                throw new WlException(400, "mins 은 숫자여야 한다: " + trimmed);
            }
        }

        long signed = negative ? -val : val;
        if (signed < Integer.MIN_VALUE || signed > Integer.MAX_VALUE) {
            throw new WlException(400, "mins 은 숫자여야 한다: " + trimmed);
        }
        return (int) signed;
    }

    @SuppressWarnings("unchecked")
    private static List<Map<String, String>> parseAndValidateLinks(
            boolean isTool,
            Map<String, Object> toolOpts,
            Map<String, String> opts
    ) {
        if (isTool) {
            if (!toolOpts.containsKey("links")) {
                return Collections.emptyList();
            }
            List<Object> rawList = (List<Object>) toolOpts.get("links");
            return validateLinksList(rawList);
        } else {
            if (!opts.containsKey("links")) {
                return Collections.emptyList();
            }
            String rawJson = opts.get("links");
            JsonValue val;
            try {
                val = JsonReader.parse(rawJson);
            } catch (Exception e) {
                throw new WlException(400, "links 은 JSON 배열이어야 한다: " + rawJson);
            }
            if (!(val instanceof JsonValue.JsonArray arr)) {
                throw new WlException(400, "links 은 JSON 배열이어야 한다: " + rawJson);
            }
            return validateJsonArrayLinks(arr);
        }
    }

    private static List<Map<String, String>> validateLinksList(List<Object> rawList) {
        List<Map<String, String>> result = new ArrayList<>();
        Set<String> seen = new HashSet<>();

        for (Object item : rawList) {
            if (!(item instanceof Map<?, ?> map)) {
                throw new WlException(400, "인자 검증 실패(create_doc): links 는 객체 배열 여야 한다: " + item);
            }
            Map<String, String> validLink = validateLinkEntry(map);
            String key = validLink.get("to") + "\u0000" + validLink.get("kind");
            if (!seen.contains(key)) {
                seen.add(key);
                result.add(validLink);
            }
        }
        return result;
    }

    private static List<Map<String, String>> validateJsonArrayLinks(JsonValue.JsonArray arr) {
        List<Map<String, String>> result = new ArrayList<>();
        Set<String> seen = new HashSet<>();

        for (JsonValue el : arr.elements()) {
            if (!(el instanceof JsonValue.JsonObject obj)) {
                throw new WlException(400, "links 항목이 객체가 아니다: " + JsonCompact.compact(el));
            }
            Map<String, String> validLink = validateLinkJsonObject(obj);
            String key = validLink.get("to") + "\u0000" + validLink.get("kind");
            if (!seen.contains(key)) {
                seen.add(key);
                result.add(validLink);
            }
        }
        return result;
    }

    private static Map<String, String> validateLinkJsonObject(JsonValue.JsonObject obj) {
        // 1. 키 검증: to, kind만 허용
        List<String> unknownKeys = new ArrayList<>();
        for (String k : obj.properties().keySet()) {
            if (!"to".equals(k) && !"kind".equals(k)) {
                unknownKeys.add(k);
            }
        }
        if (!unknownKeys.isEmpty()) {
            unknownKeys.sort(String::compareTo);
            throw new WlException(400, "links 항목에 모르는 키가 있다: " + unknownKeys + ". 쓸 수 있는 것은 [to, kind] 뿐이다.");
        }

        // 2. to 문자열 및 200자 검증
        JsonValue toVal = obj.properties().get("to");
        if (!(toVal instanceof JsonValue.JsonString toStr)) {
            throw new WlException(400, "links 항목의 to 가 문자열이 아니다: " + (toVal != null ? JsonCompact.compact(toVal) : "null"));
        }
        String to = toStr.value();
        if (to.length() > 200) {
            throw new WlException(400, "links 항목의 to 가 너무 길다(최대 200자): " + to.length() + "자");
        }

        // 3. kind 문자열 및 200자 검증
        JsonValue kindVal = obj.properties().get("kind");
        if (!(kindVal instanceof JsonValue.JsonString kindStr)) {
            throw new WlException(400, "links 항목의 kind 가 문자열이 아니다: " + (kindVal != null ? JsonCompact.compact(kindVal) : "null"));
        }
        String kind = kindStr.value();
        if (kind.length() > 200) {
            throw new WlException(400, "links 항목의 kind 가 너무 길다(최대 200자): " + kind.length() + "자");
        }

        // 4. kind 값 검증 (direct, indirect, attach)
        String canonicalKind = validateKind(kind);

        // 5. to id 모양 검증
        if (!IdGenerator.isValidNoteId(to)) {
            throw new WlException(400, "노트 id 모양이 아니다: " + to);
        }

        Map<String, String> link = new LinkedHashMap<>();
        link.put("to", to);
        link.put("kind", canonicalKind);
        return link;
    }

    private static Map<String, String> validateLinkEntry(Map<?, ?> map) {
        List<String> unknownKeys = new ArrayList<>();
        for (Object k : map.keySet()) {
            String keyStr = String.valueOf(k);
            if (!"to".equals(keyStr) && !"kind".equals(keyStr)) {
                unknownKeys.add(keyStr);
            }
        }
        if (!unknownKeys.isEmpty()) {
            unknownKeys.sort(String::compareTo);
            throw new WlException(400, "links 항목에 모르는 키가 있다: " + unknownKeys + ". 쓸 수 있는 것은 [to, kind] 뿐이다.");
        }

        Object toObj = map.get("to");
        if (!(toObj instanceof String to)) {
            throw new WlException(400, "links 항목의 to 가 문자열이 아니다: " + (toObj != null ? "\"" + toObj + "\"" : "null"));
        }
        if (to.length() > 200) {
            throw new WlException(400, "links 항목의 to 가 너무 길다(최대 200자): " + to.length() + "자");
        }

        Object kindObj = map.get("kind");
        if (!(kindObj instanceof String kind)) {
            throw new WlException(400, "links 항목의 kind 가 문자열이 아니다: " + (kindObj != null ? "\"" + kindObj + "\"" : "null"));
        }
        if (kind.length() > 200) {
            throw new WlException(400, "links 항목의 kind 가 너무 길다(최대 200자): " + kind.length() + "자");
        }

        String canonicalKind = validateKind(kind);
        if (!IdGenerator.isValidNoteId(to)) {
            throw new WlException(400, "노트 id 모양이 아니다: " + to);
        }

        Map<String, String> link = new LinkedHashMap<>();
        link.put("to", to);
        link.put("kind", canonicalKind);
        return link;
    }

    private static String validateKind(String kind) {
        if ("direct".equalsIgnoreCase(kind)) {
            return "direct";
        } else if ("indirect".equalsIgnoreCase(kind)) {
            return "indirect";
        } else if ("attach".equalsIgnoreCase(kind)) {
            return "attach";
        } else {
            throw new WlException(400, "모르는 관계 종류 이다: " + kind + ". 쓸 수 있는 것은 [direct, indirect, attach] 뿐이다.");
        }
    }

    @SuppressWarnings("unchecked")
    static List<String> parseAndValidateTags(
            boolean isTool,
            Map<String, Object> toolOpts,
            Map<String, String> opts
    ) {
        List<String> rawItems = new ArrayList<>();
        if (isTool) {
            if (!toolOpts.containsKey("tags")) {
                return Collections.emptyList();
            }
            List<Object> list = (List<Object>) toolOpts.get("tags");
            for (Object o : list) {
                if (!(o instanceof String s)) {
                    throw new WlException(400, "인자 검증 실패(create_doc): tags 는 문자열 배열 여야 한다: " + o);
                }
                rawItems.add(s);
            }
        } else {
            if (!opts.containsKey("tags")) {
                return Collections.emptyList();
            }
            String rawJson = opts.get("tags");
            JsonValue val;
            try {
                val = JsonReader.parse(rawJson);
            } catch (Exception e) {
                throw new WlException(400, "tags 은 JSON 배열이어야 한다: " + rawJson);
            }
            if (!(val instanceof JsonValue.JsonArray arr)) {
                throw new WlException(400, "tags 은 JSON 배열이어야 한다: " + rawJson);
            }
            for (JsonValue el : arr.elements()) {
                if (!(el instanceof JsonValue.JsonString str)) {
                    throw new WlException(400, "tags 항목이 문자열이 아니다: " + JsonCompact.compact(el));
                }
                rawItems.add(str.value());
            }
        }

        // 13.3 태그 검사 순서:
        // 2. 공백뿐인 항목 조용히 버림
        // 3. 다듬고 줄바꿈(LF/CR) 검사
        // 4. 대소문자 구분 중복 제거
        List<String> result = new ArrayList<>();
        Set<String> seen = new HashSet<>();

        for (String raw : rawItems) {
            if (Trim.isBlank(raw)) {
                continue;
            }
            String trimmed = Trim.trim(raw);
            if (trimmed.indexOf('\n') >= 0 || trimmed.indexOf('\r') >= 0) {
                throw new WlException(400, "태그에 줄바꿈이 있다: '" + trimmed + "'");
            }
            if (!seen.contains(trimmed)) {
                seen.add(trimmed);
                result.add(trimmed);
            }
        }
        return result;
    }

    private record AgendaLookupResult(String slug, boolean created) {}

    private static AgendaLookupResult lookupOrCreateAgenda(Path vaultRoot, AgendaTable table, String key) {
        // 13.5 아젠다 찾기
        // 2. slug가 key와 일치하는 아젠다
        AgendaTable.AgendaInfo match = table.slugToAgenda().get(key);
        if (match != null) {
            return new AgendaLookupResult(match.slug(), false);
        }

        // 3. 소문자로 바꾼 제목이 key의 소문자와 일치하는 아젠다
        String lowerKey = key.toLowerCase(Locale.ROOT);
        for (AgendaTable.AgendaInfo info : table.slugToAgenda().values()) {
            if (info.title() != null && info.title().toLowerCase(Locale.ROOT).equals(lowerKey)) {
                return new AgendaLookupResult(info.slug(), false);
            }
        }

        // 4. key를 slug화한 값과 slug가 일치하는 아젠다
        String slugKey = SlugBuilder.build(key, 50);
        // Row 26: 경로로 쓸 수 없는 slug 검사
        if (PathConfinement.hasUnusablePathCharacters(slugKey + ".md")) {
            throw new WlException(400, "경로로 쓸 수 없는 값이다: " + slugKey + ".md");
        }

        match = table.slugToAgenda().get(slugKey);
        if (match != null) {
            return new AgendaLookupResult(match.slug(), false);
        }

        // 5. 새 아젠다 생성. create-agenda와 같은 색 규칙과 바이트다(20.8). 저장소와 프로젝트는 없고 본문은 빈 글이다
        String newId = IdGenerator.nextAgendaId();
        String chosenHue = AgendaFile.leastUsedHue(table.idToAgenda().values());

        Path agendaFile = vaultRoot.resolve(AgendaFile.relPath(slugKey));
        // Row 27: 새 아젠다 쓰기
        AtomicWriter.writeAtomic(agendaFile, AgendaFile.render(newId, slugKey, key, chosenHue, null, null, ""));

        // 색인에 추가
        AgendaTable.AgendaInfo createdInfo = new AgendaTable.AgendaInfo(newId, slugKey, key, chosenHue, null, null, null);
        table.slugToId().put(slugKey, newId);
        table.slugToTitle().put(slugKey, key);
        table.idToAgenda().put(newId, createdInfo);
        table.slugToAgenda().put(slugKey, createdInfo);

        return new AgendaLookupResult(slugKey, true);
    }

    static String extractIdTail(String id) {
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < id.length(); i++) {
            char c = id.charAt(i);
            if ((c >= 'a' && c <= 'z') || (c >= '0' && c <= '9')) {
                sb.append(c);
            }
        }
        if (sb.length() >= 4) {
            return sb.substring(sb.length() - 4);
        } else if (sb.length() > 0) {
            return sb.toString();
        } else {
            return "0";
        }
    }

    private static void reindexWrittenNote(Path vaultRoot, String noteRelPath, String noteId, NoteIndex noteIndex) {
        Path noteFile = vaultRoot.resolve(noteRelPath).resolve("note.md");
        if (!Files.isRegularFile(noteFile)) {
            return;
        }

        // 파일에서 다시 읽어 색인
        try {
            NoteFile.ParsedNoteFile parsed = NoteFile.parse(noteFile);
            if (parsed.hasFrontmatter() && parsed.frontmatter() != null) {
                String folderAgendaSlug = noteRelPath.startsWith("notes/") ?
                        noteRelPath.substring("notes/".length(), noteRelPath.indexOf('/', "notes/".length())) : "";
                NoteMeta.MetaData meta = NoteMeta.parseMeta(parsed.frontmatter(), folderAgendaSlug, noteIndex.agendaTable(), false);
                if (meta != null) {
                    // Row 31 / 13.9 16번: 이미 다른 노트로 색인되어 있던 자리인 경우 제약 위반
                    for (NoteScanner.NoteRecord existing : noteIndex.notesById().values()) {
                        if (existing.vaultRelPath().equals(noteRelPath) && !existing.meta().id().equals(noteId)) {
                            throw new IllegalStateException("Storage path conflict in note index: " + noteRelPath);
                        }
                    }
                    NoteScanner.NoteRecord rec = new NoteScanner.NoteRecord(
                            meta.id(), false, null, null, null, noteRelPath, "markdown",
                            noteFile, meta, parsed, new LinkedHashMap<>(), false, false
                    );
                    noteIndex.notesById().put(noteId, rec);
                }
            }
        } catch (IllegalStateException e) {
            throw e;
        } catch (Exception e) {
            // 다시 읽기가 실패하면 경고 로그만 남기고 성공 응답 유지 (13.5, 13.9 12/13/17번)
            System.err.println("Warning: failed to re-read note for indexing: " + e.getMessage());
        }
    }
}
