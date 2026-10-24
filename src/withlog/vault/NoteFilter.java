package withlog.vault;

import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.text.Utf16;

import java.time.LocalDate;
import java.time.format.DateTimeParseException;
import java.util.*;

/**
 * list_docs 및 search_docs의 8가지 공통 필터 검증 및 필터링 엔진.
 * 설명서 9.3, 9.4에 따른 엄격한 인자 검증 차례 및 조건을 구현합니다.
 */
public final class NoteFilter {

    private static final Set<String> VALID_TYPES = Set.of(
            "plan", "build", "report", "research", "meeting", "free"
    );

    public record FilterCriteria(
            String targetAgendaSlug,
            String targetType,
            List<String> requiredTags,
            Boolean favoriteFilter,
            String favoriteCategory,
            String lowerDateBound,
            String upperDateBound
    ) {}

    private NoteFilter() {}

    /**
     * 필터 인자를 순서대로 검증하고 FilterCriteria 객체를 생성합니다.
     */
    public static FilterCriteria build(
            AgendaTable agendaTable,
            String rawAgenda,
            String rawType,
            String rawTag,
            Boolean toolFavorite,
            String cmdFavorite,
            String rawFavoriteCategory,
            String rawDate,
            String rawFrom,
            String rawTo
    ) {
        // 1. agenda 검증 (9.3 1번)
        String targetAgendaSlug = null;
        if (rawAgenda != null) {
            String trimmedAgenda = Trim.trim(rawAgenda);
            if (!trimmedAgenda.isEmpty()) {
                targetAgendaSlug = resolveAgenda(agendaTable, trimmedAgenda);
            }
        }

        // 2. type 검증 (9.3 2번)
        String targetType = null;
        if (rawType != null) {
            String trimmedType = Trim.trim(rawType);
            if (!trimmedType.isEmpty()) {
                String matchedType = null;
                for (String vt : VALID_TYPES) {
                    if (vt.equalsIgnoreCase(trimmedType)) {
                        matchedType = vt;
                        break;
                    }
                }
                if (matchedType == null) {
                    throw new WlException(400, "모르는 유형 이다: " + trimmedType +
                            ". 쓸 수 있는 것은 [plan, build, report, research, meeting, free] 뿐이다.");
                }
                targetType = matchedType;
            }
        }

        // 3. tag 검증 (9.3 3번)
        List<String> requiredTags = null;
        if (rawTag != null) {
            String trimmedTag = Trim.trim(rawTag);
            if (!trimmedTag.isEmpty()) {
                String[] split = trimmedTag.split(",");
                Set<String> tagSet = new LinkedHashSet<>();
                for (String part : split) {
                    String piece = Trim.trim(part);
                    if (!piece.isEmpty()) {
                        if (piece.contains("\r") || piece.contains("\n")) {
                            throw new WlException(400, "태그에 줄바꿈이 있다: '" + piece + "'");
                        }
                        tagSet.add(piece);
                    }
                }
                if (!tagSet.isEmpty()) {
                    requiredTags = new ArrayList<>(tagSet);
                }
            }
        }

        // 4. favorite 검증 (9.3 4번)
        Boolean favoriteFilter = null;
        if (toolFavorite != null) {
            // 도구 표면
            if (toolFavorite) {
                favoriteFilter = true;
            }
        } else if (cmdFavorite != null) {
            // 명령 표면
            String trimmedFav = Trim.trim(cmdFavorite);
            if (!trimmedFav.isEmpty()) {
                String lowerFav = trimmedFav.toLowerCase(Locale.ROOT);
                if ("true".equals(lowerFav)) {
                    favoriteFilter = true;
                } else if ("false".equals(lowerFav)) {
                    favoriteFilter = null; // false는 필터 안 건 것과 같음
                } else {
                    throw new WlException(400, "모르는 즐겨찾기 값 이다: " + lowerFav +
                            ". 쓸 수 있는 것은 [true, false] 뿐이다.");
                }
            }
        }

        // 5. favoriteCategory 검증 (9.3 5번)
        String favoriteCategory = null;
        if (rawFavoriteCategory != null) {
            String trimmedCat = Trim.trim(rawFavoriteCategory);
            if (!trimmedCat.isEmpty()) {
                favoriteCategory = trimmedCat;
            }
        }

        // 6. date, from, to 검증 (9.3 6번)
        LocalDate dDate = parseIsoDate(rawDate, "date");
        LocalDate dFrom = parseIsoDate(rawFrom, "from");
        LocalDate dTo = parseIsoDate(rawTo, "to");

        LocalDate lower = null;
        LocalDate upper = null;

        if (dDate != null) {
            lower = dDate;
            upper = dDate;
        }

        if (dFrom != null) {
            if (lower == null || dFrom.isAfter(lower)) {
                lower = dFrom;
            }
        }

        if (dTo != null) {
            if (upper == null || dTo.isBefore(upper)) {
                upper = dTo;
            }
        }

        if (lower != null && upper != null && lower.isAfter(upper)) {
            throw new WlException(400, "날짜 구간이 뒤집혔다: " + lower + " ~ " + upper);
        }

        String lowerBoundStr = lower != null ? lower.toString() : null;
        String upperBoundStr = upper != null ? upper.toString() : null;

        return new FilterCriteria(
                targetAgendaSlug,
                targetType,
                requiredTags,
                favoriteFilter,
                favoriteCategory,
                lowerBoundStr,
                upperBoundStr
        );
    }

    private static LocalDate parseIsoDate(String raw, String fieldName) {
        if (raw == null) return null;
        String trimmed = Trim.trim(raw);
        if (trimmed.isEmpty()) return null;
        try {
            return LocalDate.parse(trimmed);
        } catch (DateTimeParseException e) {
            throw new WlException(400, fieldName + " 은 YYYY-MM-DD 모양이어야 한다: " + trimmed);
        }
    }

    private static String resolveAgenda(AgendaTable agendaTable, String input) {
        // 1단계: slug와 글자까지 똑같은 아젠다
        boolean canBeSlug = input.length() <= 100 && !input.contains("/") && !input.contains("\\") &&
                !input.contains("..") && !input.startsWith(".") && hasNoControlChars(input);

        if (canBeSlug && agendaTable.slugToId().containsKey(input)) {
            return input;
        }

        // 2단계: agendaTitle과 equalsIgnoreCase 비교
        List<String> matchingSlugs = new ArrayList<>();
        for (Map.Entry<String, String> entry : agendaTable.slugToTitle().entrySet()) {
            String slug = entry.getKey();
            String title = entry.getValue();
            if (title.equalsIgnoreCase(input)) {
                matchingSlugs.add(slug);
            }
        }

        if (matchingSlugs.isEmpty()) {
            throw new WlException(404, "아젠다 not found: " + input);
        }

        // 복수 매칭 시 slug의 UTF-16 차례로 맨 앞
        matchingSlugs.sort(Utf16::compare);
        return matchingSlugs.get(0);
    }

    private static boolean hasNoControlChars(String s) {
        for (int i = 0; i < s.length(); i++) {
            if (s.charAt(i) < ' ') {
                return false;
            }
        }
        return true;
    }

    /**
     * 단일 노드가 FilterCriteria 조건을 모두 만족하는지 판정합니다.
     */
    public static boolean matches(NoteScanner.NoteRecord note, FilterCriteria criteria) {
        NoteMeta.MetaData meta = note.meta();
        if (meta == null) return false;

        // 1. agenda
        if (criteria.targetAgendaSlug() != null) {
            if (!criteria.targetAgendaSlug().equals(meta.agendaSlug())) {
                return false;
            }
        }

        // 2. type
        if (criteria.targetType() != null) {
            if (!criteria.targetType().equalsIgnoreCase(meta.type())) {
                return false;
            }
        }

        // 3. tag
        if (criteria.requiredTags() != null) {
            List<String> noteTags = meta.tags() != null ? meta.tags() : Collections.emptyList();
            for (String req : criteria.requiredTags()) {
                if (!noteTags.contains(req)) {
                    return false;
                }
            }
        }

        // 4. favorite
        if (criteria.favoriteFilter() != null && criteria.favoriteFilter()) {
            if (!meta.favorite()) {
                return false;
            }
        }

        // 5. favoriteCategory
        if (criteria.favoriteCategory() != null) {
            if (!criteria.favoriteCategory().equals(meta.favoriteCategory())) {
                return false;
            }
        }

        // 6. date range
        if (criteria.lowerDateBound() != null || criteria.upperDateBound() != null) {
            String noteDate = meta.date();
            if (noteDate == null) return false;
            if (criteria.lowerDateBound() != null) {
                if (NoteIndex.compareUtf8Bytes(noteDate, criteria.lowerDateBound()) < 0) {
                    return false;
                }
            }
            if (criteria.upperDateBound() != null) {
                if (NoteIndex.compareUtf8Bytes(noteDate, criteria.upperDateBound()) > 0) {
                    return false;
                }
            }
        }

        return true;
    }
}
