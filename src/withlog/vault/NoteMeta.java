package withlog.vault;

import withlog.text.Trim;
import withlog.text.Utf16;

import java.time.Instant;
import java.time.LocalDate;
import java.time.LocalDateTime;
import java.time.OffsetDateTime;
import java.time.ZoneOffset;
import java.time.format.DateTimeFormatter;
import java.time.format.DateTimeParseException;
import java.util.*;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * 프론트매터 맵에서 노트 메타데이터를 추출하고 정규화하는 클래스.
 * 설명서 1.7 및 1.8의 규칙에 따라 각 키의 유효성을 검사하고 값을 정제합니다.
 */
public final class NoteMeta {
    private static final Pattern NOTE_ID_PATTERN = withlog.text.IdShape.NOTE;
    private static final Pattern SUB_NOTE_ID_PATTERN = withlog.text.IdShape.SUB_NOTE;
    private static final Pattern TEMPLATE_ID_PATTERN = Pattern.compile("^[A-Za-z0-9._-]{1,128}$");
    private static final Set<String> ALLOWED_TYPES = Set.of("plan", "build", "report", "research", "meeting", "free");
    private static final Set<String> ALLOWED_KINDS = Set.of("direct", "indirect", "attach");

    /**
     * 노트 메타데이터 레코드.
     */
    public record MetaData(
            String id,
            String agendaSlug,
            String type,
            String title,
            String created,
            String date,
            String time,
            String sessionId,
            int mins,
            String tier,
            String editedBy,
            String editedAt,
            String editedSessionId,
            List<String> tags,
            List<Map<String, String>> links,
            boolean favorite,
            String favoriteCategory,
            String template,
            Map<String, Object> evaluation
    ) {}

    private NoteMeta() {}

    /**
     * 프론트매터 Map에서 노트 메타데이터를 추출합니다.
     * @param map 프론트매터 Map
     * @param folderAgendaSlug 노트가 속한 폴더의 아젠다 slug
     * @param agendaTable 아젠다 조회 테이블
     * @param isSubNote 하위 노트 여부
     * @return MetaData 객체, 또는 검증 실패 시 null
     * @throws IllegalArgumentException created 날짜 오류 또는 slug 불일치 시
     */
    @SuppressWarnings("unchecked")
    public static MetaData parseMeta(
            Map<String, Object> map,
            String folderAgendaSlug,
            AgendaTable agendaTable,
            boolean isSubNote
    ) {
        return parseMeta(map, folderAgendaSlug, agendaTable, isSubNote, null);
    }

    public static MetaData parseMeta(
            Map<String, Object> map,
            String folderAgendaSlug,
            AgendaTable agendaTable,
            boolean isSubNote,
            String rootNoteId
    ) {
        if (map == null) return null;

        // 1. id 검증
        Object rawId = map.get("id");
        if (!(rawId instanceof String strId)) {
            return null;
        }
        if (isSubNote) {
            if (!SUB_NOTE_ID_PATTERN.matcher(strId).matches()) {
                return null;
            }
        } else {
            if (!NOTE_ID_PATTERN.matcher(strId).matches()) {
                return null;
            }
        }
        String id = strId;

        // 2. agenda 결정
        String agendaSlug;
        if (isSubNote) {
            agendaSlug = folderAgendaSlug;
        } else {
            Object rawAgenda = map.get("agenda");
            if (rawAgenda instanceof String strAgenda && !strAgenda.isEmpty()) {
                agendaSlug = strAgenda;
            } else {
                agendaSlug = folderAgendaSlug;
            }
            if (agendaTable != null && !agendaTable.slugToId().containsKey(agendaSlug)) {
                throw new IllegalArgumentException("아젠다 slug 를 식별자로 못 바꾼다: " + agendaSlug);
            }
        }

        // 3. type
        Object rawType = map.get("type");
        String type = "build";
        if (rawType instanceof String strType) {
            if (ALLOWED_TYPES.contains(strType)) {
                type = strType;
            }
        }

        // 4. title
        Object rawTitle = map.get("title");
        String title = id;
        if (rawTitle instanceof String strTitle && !Trim.isBlank(strTitle)) {
            title = strTitle;
        }

        // 5. created
        Object rawCreated = map.get("created");
        if (!(rawCreated instanceof String strCreated)) {
            throw new IllegalArgumentException("created 가 달력에 없는 날짜다");
        }
        CreatedResult createdResult = parseCreated(strCreated);
        if (createdResult == null) {
            throw new IllegalArgumentException("created 가 달력에 없는 날짜다");
        }

        // 6. sessionId
        String sessionId = null;
        if (!isSubNote) {
            Object rawSid = map.get("sessionId");
            if (rawSid instanceof String strSid) {
                String trimmed = Trim.trim(strSid);
                if (!trimmed.isEmpty()) {
                    sessionId = Utf16.cut(trimmed, 128);
                }
            }
        }

        // 7. mins
        Object rawMins = map.get("mins");
        int mins = 0;
        if (rawMins instanceof Number num) {
            double d = num.doubleValue();
            if (Double.isNaN(d)) {
                throw new IllegalStateException("Cannot round NaN value.");
            }
            if (d < 0) {
                mins = 0;
            } else {
                long rounded = Math.round(d);
                if (rounded < 0) mins = 0;
                else if (rounded > Integer.MAX_VALUE) mins = Integer.MAX_VALUE;
                else mins = (int) rounded;
            }
        }

        // 8. tier
        String tier = "reading";
        if (!isSubNote) {
            Object rawTier = map.get("tier");
            if (rawTier instanceof String strTier) {
                String t = Trim.trim(strTier);
                if ("reading".equals(t)) {
                    tier = "reading";
                } else if (!t.isEmpty()) {
                    tier = "record";
                }
            }
        }

        // 9. 도장 (editedBy, editedAt, editedSessionId)
        String editedBy = null;
        String editedAt = null;
        String editedSessionId = null;

        Object rawEb = map.get("editedBy");
        Object rawEa = map.get("editedAt");
        if (rawEb instanceof String strEb && rawEa instanceof String strEa) {
            String teb = Trim.trim(strEb);
            String tea = Trim.trim(strEa);
            if (!teb.isEmpty() && !tea.isEmpty()) {
                editedBy = teb;
                editedAt = tea;
                Object rawEsid = map.get("editedSessionId");
                if (rawEsid instanceof String strEsid) {
                    String tesid = Trim.trim(strEsid);
                    if (!tesid.isEmpty()) {
                        editedSessionId = tesid;
                    }
                }
            }
        }

        // 10. tags
        List<String> tags = new ArrayList<>();
        Object rawTags = map.get("tags");
        if (rawTags instanceof List<?> tagList) {
            Set<String> seen = new HashSet<>();
            for (Object item : tagList) {
                if (item instanceof String strTag) {
                    String trimmed = Trim.trim(strTag);
                    if (!trimmed.isEmpty() && !seen.contains(trimmed)) {
                        seen.add(trimmed);
                        tags.add(trimmed);
                        if (tags.size() == 50) {
                            break;
                        }
                    }
                }
            }
            // CR/LF가 든 항목 제거
            tags.removeIf(t -> t.contains("\n") || t.contains("\r"));
        }

        // 11. links
        List<Map<String, String>> links = new ArrayList<>();
        Object rawLinks = map.get("links");
        if (rawLinks instanceof List<?> linkList) {
            Set<String> seenPairs = new HashSet<>();
            for (Object item : linkList) {
                if (item instanceof Map<?, ?> linkMap) {
                    Object toObj = linkMap.get("to");
                    Object kindObj = linkMap.get("kind");
                    if (toObj instanceof String to && kindObj instanceof String kind) {
                        if (NOTE_ID_PATTERN.matcher(to).matches() && ALLOWED_KINDS.contains(kind)) {
                            boolean isSelfRef = to.equals(id) || (isSubNote && rootNoteId != null && to.equals(rootNoteId));
                            if (!isSelfRef) {
                                String pairKey = to + ":" + kind;
                                if (!seenPairs.contains(pairKey)) {
                                    seenPairs.add(pairKey);
                                    Map<String, String> m = new LinkedHashMap<>();
                                    m.put("to", to);
                                    m.put("kind", kind);
                                    links.add(m);
                                }
                            }
                        }
                    }
                }
            }
        }

        // 12. favorite
        boolean favorite = false;
        String favoriteCategory = null;
        if (!isSubNote) {
            Object rawFav = map.get("favorite");
            if (rawFav instanceof Boolean b) {
                favorite = b;
            }
            Object rawCat = map.get("favoriteCategory");
            if (rawCat instanceof String strCat) {
                String trimmed = Trim.trim(strCat);
                if (!trimmed.isEmpty()) {
                    favoriteCategory = Utf16.cut(trimmed, 100);
                }
            }
        }

        // 13. template
        String template = null;
        if (!isSubNote) {
            Object rawTpl = map.get("template");
            if (rawTpl instanceof String strTpl) {
                String trimmed = Trim.trim(strTpl);
                String cut = Utf16.cut(trimmed, 128);
                String retrimmed = Trim.trim(cut);
                if (TEMPLATE_ID_PATTERN.matcher(retrimmed).matches()) {
                    template = retrimmed;
                }
            }
        }

        // 14. evaluation
        Map<String, Object> evaluation = null;
        if (!isSubNote) {
            Object rawEval = map.get("evaluation");
            if (rawEval instanceof Map<?, ?> evalMap) {
                Object rawScore = evalMap.get("score");
                if (rawScore instanceof Number num) {
                    double d = num.doubleValue();
                    if (Double.isNaN(d)) {
                        throw new IllegalStateException("Cannot round NaN value.");
                    }
                    long rounded = Math.round(d);
                    int score = (int) Math.max(0, Math.min(100, rounded));

                    String note = "";
                    Object rawNote = evalMap.get("note");
                    if (rawNote instanceof String strNote) {
                        note = Utf16.cut(Trim.trim(strNote), 1000);
                    }

                    String at = null;
                    Object rawAt = evalMap.get("at");
                    if (rawAt instanceof String strAt && !Trim.isBlank(strAt)) {
                        at = parseEvaluationAt(strAt, createdResult.utcInstant);
                    } else {
                        at = createdResult.utcInstant;
                    }

                    Map<String, Object> em = new LinkedHashMap<>();
                    em.put("score", score);
                    em.put("note", note);
                    em.put("at", at);
                    evaluation = em;
                }
            }
        }

        return new MetaData(
                id, agendaSlug, type, title,
                createdResult.formattedCreated, createdResult.date, createdResult.time,
                sessionId, mins, tier,
                editedBy, editedAt, editedSessionId,
                tags, links, favorite, favoriteCategory, template, evaluation
        );
    }

    public record CreatedResult(
            String formattedCreated,
            String date,
            String time,
            String utcInstant
    ) {}

    private static final DateTimeFormatter UTC_MILLIS_FORMATTER =
            DateTimeFormatter.ofPattern("uuuu-MM-dd'T'HH:mm:ss.SSS'Z'").withZone(ZoneOffset.UTC);

    private static String formatUtcMillis(Instant inst) {
        return UTC_MILLIS_FORMATTER.format(inst);
    }

    public static CreatedResult parseCreated(String s) {
        if (s == null || s.isEmpty()) return null;
        if (s.startsWith(" ") || s.endsWith(" ")) return null;
        if (s.length() < 10) return null;

        String datePart = s.substring(0, 10);
        if (!datePart.matches("^[0-9]{4}-[0-9]{2}-[0-9]{2}$")) {
            return null;
        }

        LocalDate ld;
        try {
            ld = LocalDate.parse(datePart);
        } catch (DateTimeParseException e) {
            return null;
        }

        boolean isYear0000 = datePart.startsWith("0000-");
        String outDate = (isYear0000 ? "0001" : datePart.substring(0, 4)) + datePart.substring(4);

        if (s.length() == 10) {
            String formatted = outDate + "T00:00:00+09:00";
            ZoneOffset kst = ZoneOffset.ofHours(9);
            OffsetDateTime odt = OffsetDateTime.of(ld.getYear() == 0 ? 1 : ld.getYear(), ld.getMonthValue(), ld.getDayOfMonth(), 0, 0, 0, 0, kst);
            return new CreatedResult(formatted, outDate, "00:00", formatUtcMillis(odt.toInstant()));
        }

        char sep = s.charAt(10);
        if (sep != 'T' && sep != 't') {
            return null;
        }

        OffsetDateTime odt;
        try {
            odt = OffsetDateTime.parse(s, DateTimeFormatter.ISO_OFFSET_DATE_TIME);
        } catch (DateTimeParseException e1) {
            try {
                LocalDateTime ldt = LocalDateTime.parse(s, DateTimeFormatter.ISO_LOCAL_DATE_TIME);
                odt = ldt.atOffset(ZoneOffset.ofHours(9));
            } catch (DateTimeParseException e2) {
                try {
                    LocalDate lDate = LocalDate.parse(s, DateTimeFormatter.ISO_LOCAL_DATE);
                    odt = lDate.atTime(0, 0, 0).atOffset(ZoneOffset.ofHours(9));
                } catch (DateTimeParseException e3) {
                    return null;
                }
            }
        }

        int hour = odt.getHour();
        int min = odt.getMinute();
        int sec = odt.getSecond();
        String time = String.format("%02d:%02d", hour, min);

        int totalOffsetSeconds = odt.getOffset().getTotalSeconds();
        int absSec = Math.abs(totalOffsetSeconds);
        int offH = absSec / 3600;
        int offM = (absSec % 3600) / 60;

        String normalizedOffset;
        if (offH == 0 && offM == 0) {
            normalizedOffset = "Z";
        } else {
            char sign = totalOffsetSeconds >= 0 ? '+' : '-';
            normalizedOffset = String.format("%c%02d:%02d", sign, offH, offM);
        }

        String formattedCreated = String.format("%sT%02d:%02d:%02d%s", outDate, hour, min, sec, normalizedOffset);

        OffsetDateTime odtForInstant = (odt.getYear() == 0) ? odt.withYear(1) : odt;
        String utcInstant = formatUtcMillis(odtForInstant.toInstant());

        return new CreatedResult(formattedCreated, outDate, time, utcInstant);
    }

    /** 1.8의 at 규칙 1번(순간)이나 2번(created 꼴)으로 읽히는 글인지 봅니다. 코멘트 색인 사본(1.12)이 씁니다. */
    static boolean readsAsAt(String s) {
        if (s == null || Trim.isBlank(s)) return false;
        try {
            Instant.parse(s.replace('t', 'T').replace('z', 'Z'));
            return true;
        } catch (DateTimeParseException e) {
            try {
                return parseCreated(s) != null;
            } catch (RuntimeException e2) {
                return false;
            }
        }
    }

    private static String parseEvaluationAt(String s, String fallbackUtcInstant) {
        if (s == null || s.isEmpty()) return fallbackUtcInstant;
        if (Trim.isBlank(s)) return fallbackUtcInstant;

        try {
            Instant inst = Instant.parse(s.replace('t', 'T').replace('z', 'Z'));
            return formatUtcMillis(inst);
        } catch (DateTimeParseException e1) {
            CreatedResult cr = parseCreated(s);
            if (cr != null) {
                return cr.utcInstant();
            }
            return fallbackUtcInstant;
        }
    }
}
