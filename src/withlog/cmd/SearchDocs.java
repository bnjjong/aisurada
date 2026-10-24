package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.out.NoteRow;
import withlog.text.Trim;
import withlog.text.Utf16;
import withlog.vault.*;

import java.nio.file.Path;
import java.time.Instant;
import java.time.OffsetDateTime;
import java.time.format.DateTimeFormatter;
import java.time.format.DateTimeParseException;
import java.util.*;

/**
 * search_docs 및 search-docs 명령 실행기.
 * 설명서 10장에 따라 조건에 맞는 메인 노트를 검색하고 matchedIn 우선순위 및 시각 내림차순으로 정렬하여 반환합니다.
 */
public final class SearchDocs {

    private static final String HINT_TEXT = "agenda·type·tag·dateFrom으로 좁히거나 limit을 올리세요";

    private SearchDocs() {}

    public static Map<String, Object> execute(
            Path vaultRoot,
            ParsedArgs parsed,
            NoteIndex noteIndex
    ) {
        boolean isTool = parsed.isToolSurface();

        // 1. 검색어 결정 (10.3)
        String rawQuery;
        String determinedQuery;
        String usage = "search-docs <검색어> [agenda=<slug 또는 이름>] [type=<유형>] [tag=<태그,태그>] [favorite=true] [favoriteCategory=<라벨>] [date=<YYYY-MM-DD>] [from=<YYYY-MM-DD>] [to=<YYYY-MM-DD>] [limit=<건수. 기본 20, 최대 200>]";

        if (isTool) {
            Map<String, Object> opts = parsed.toolOptions();
            rawQuery = opts != null ? (String) opts.get("query") : null;
            if (rawQuery == null) {
                throw new WlException(400, "인자 검증 실패(search_docs): query 가 필요하다.");
            }
            determinedQuery = Trim.trim(rawQuery);
            if (determinedQuery.isEmpty()) {
                throw new WlException(400, "검색어가 필요하다: " + usage);
            }
        } else {
            // 명령 표면: 10.3 3단계 결정
            if (!parsed.positionalArgs().isEmpty() && !Trim.isBlank(parsed.positionalArgs().get(0))) {
                determinedQuery = parsed.positionalArgs().get(0);
            } else if (parsed.namedOptions().containsKey("query")) {
                determinedQuery = Trim.trim(parsed.namedOptions().get("query"));
            } else {
                determinedQuery = "";
            }
            if (determinedQuery.isEmpty()) {
                throw new WlException(400, "검색어가 필요하다: " + usage);
            }
        }

        // 2. 필터 인자 추출 및 검증 (9.3 필터 1~6번 차례)
        String rawAgenda = null;
        String rawType = null;
        String rawTag = null;
        Boolean toolFavorite = null;
        String cmdFavorite = null;
        String rawFavoriteCategory = null;
        String rawDate = null;
        String rawFrom = null;
        String rawTo = null;

        if (isTool) {
            Map<String, Object> opts = parsed.toolOptions();
            if (opts != null) {
                rawAgenda = (String) opts.get("agenda");
                rawType = (String) opts.get("type");
                rawTag = (String) opts.get("tag");
                toolFavorite = (Boolean) opts.get("favorite");
                rawFavoriteCategory = (String) opts.get("favoriteCategory");
                rawDate = (String) opts.get("date");
                rawFrom = (String) opts.get("dateFrom");
                rawTo = (String) opts.get("dateTo");
            }
        } else {
            Map<String, String> opts = parsed.namedOptions();
            rawAgenda = opts.get("agenda");
            rawType = opts.get("type");
            rawTag = opts.get("tag");
            cmdFavorite = opts.get("favorite");
            rawFavoriteCategory = opts.get("favoriteCategory");
            rawDate = opts.get("date");
            rawFrom = opts.get("from");
            rawTo = opts.get("to");
        }

        NoteFilter.FilterCriteria criteria = NoteFilter.build(
                noteIndex.agendaTable(),
                rawAgenda,
                rawType,
                rawTag,
                toolFavorite,
                cmdFavorite,
                rawFavoriteCategory,
                rawDate,
                rawFrom,
                rawTo
        );

        // 3. limit 검증 (10.3: 필터 검사 뒤에 limit 범위 검사)
        int limit = 20;
        if (isTool) {
            Map<String, Object> opts = parsed.toolOptions();
            if (opts != null && opts.containsKey("limit")) {
                Long toolLimitLong = (Long) opts.get("limit");
                if (toolLimitLong > Integer.MAX_VALUE) {
                    throw new WlException(400, "limit 은 숫자여야 한다: " + toolLimitLong);
                }
                int l = toolLimitLong.intValue();
                if (l < 1 || l > 200) {
                    throw new WlException(400, "받을 건수가 범위를 벗어났다: " + l);
                }
                limit = l;
            }
        } else {
            if (parsed.namedOptions().containsKey("limit")) {
                String rawLimit = parsed.namedOptions().get("limit");
                String trimmedLimit = Trim.trim(rawLimit);
                if (trimmedLimit.isEmpty()) {
                    limit = 20;
                } else {
                    try {
                        long parsedLimit = Long.parseLong(trimmedLimit);
                        if (parsedLimit > Integer.MAX_VALUE || parsedLimit < Integer.MIN_VALUE) {
                            throw new WlException(400, "limit 은 숫자여야 한다: " + trimmedLimit);
                        }
                        int l = (int) parsedLimit;
                        if (l < 1 || l > 200) {
                            throw new WlException(400, "받을 건수가 범위를 벗어났다: " + l);
                        }
                        limit = l;
                    } catch (NumberFormatException e) {
                        throw new WlException(400, "limit 은 숫자여야 한다: " + trimmedLimit);
                    }
                }
            }
        }

        // 4. 바늘 생성 (10.6)
        String needle = Trim.trimSearch(determinedQuery);

        // 5. 후보 노트 검색 및 평가
        record SearchHit(
                NoteScanner.NoteRecord note,
                String agendaTitle,
                int rankOrder, // 0: id, 1: title, 2: meta, 3: body
                String matchedIn,
                String snippet
        ) {}

        List<SearchHit> hits = new ArrayList<>();

        if (!needle.isEmpty()) {
            String needleLower = needle.toLowerCase(Locale.ROOT);
            String needleIdTarget = needleLower;
            if (needleIdTarget.startsWith("#doc:")) {
                needleIdTarget = needleIdTarget.substring(5);
            }
            int slashIdx = needleIdTarget.indexOf('/');
            if (slashIdx >= 0) {
                needleIdTarget = needleIdTarget.substring(0, slashIdx);
            }

            for (NoteScanner.NoteRecord note : noteIndex.allRootNotes()) {
                if (isTool && "record".equals(note.meta().tier())) {
                    continue;
                }
                if (!NoteFilter.matches(note, criteria)) {
                    continue;
                }

                // 제목을 검색 다듬기로 다듬은 글이 빈 노트는 검색 불가 (10.6)
                String trimmedTitle = Trim.trimSearch(note.meta().title());
                if (trimmedTitle.isEmpty()) {
                    continue;
                }

                String agendaTitle = noteIndex.agendaTable().slugToTitle().getOrDefault(note.meta().agendaSlug(), "");

                // 1순위: id
                if (needleIdTarget.equals(note.id().toLowerCase(Locale.ROOT))) {
                    hits.add(new SearchHit(note, agendaTitle, 0, "id", null));
                    continue;
                }

                // 2순위: title
                if (findRegionMatch(note.meta().title(), needle) >= 0) {
                    hits.add(new SearchHit(note, agendaTitle, 1, "title", null));
                    continue;
                }

                // 3순위: meta (agendaTitle or tags)
                boolean metaMatched = false;
                if (findRegionMatch(agendaTitle, needle) >= 0) {
                    metaMatched = true;
                } else if (note.meta().tags() != null) {
                    for (String tag : note.meta().tags()) {
                        if (findRegionMatch(tag, needle) >= 0) {
                            metaMatched = true;
                            break;
                        }
                    }
                }
                if (metaMatched) {
                    hits.add(new SearchHit(note, agendaTitle, 2, "meta", null));
                    continue;
                }

                // 4순위: body (본문 평문 + 공백 1개 + 첨부 글)
                String bodyPlain = PlainText.extract(note.format(), note.parsedFile().body());
                String attachText = AttachText.extract(vaultRoot, note);

                String searchBody;
                if (attachText != null) {
                    searchBody = bodyPlain + " " + attachText;
                } else {
                    searchBody = bodyPlain;
                }

                int at = findRegionMatch(searchBody, needle);
                if (at >= 0) {
                    // snippet 계산 (10.6)
                    int start = Math.max(at - 24, 0);
                    int end = Math.min(at + needle.length() + 76, searchBody.length());
                    String sub = searchBody.substring(start, end);
                    String trimmedSub = Trim.trimSearch(sub);
                    String snippet = (start > 0 ? "…" : "") + trimmedSub + (end < searchBody.length() ? "…" : "");
                    hits.add(new SearchHit(note, agendaTitle, 3, "body", snippet));
                }
            }
        }

        // 6. 정렬 (10.6)
        // 1) matchedIn 순위
        // 2) created 순간 늦은 쪽 -> 시계 문자열 늦은 쪽
        // 3) note id 내림차순
        hits.sort((h1, h2) -> {
            int cmpRank = Integer.compare(h1.rankOrder(), h2.rankOrder());
            if (cmpRank != 0) return cmpRank;

            int cmpTime = compareCreatedInstantDesc(h1.note().meta().created(), h2.note().meta().created());
            if (cmpTime != 0) return cmpTime;

            return Utf16.compare(h2.note().id(), h1.note().id());
        });

        // 7. 슬라이싱 및 응답 봉투 구성
        int total = hits.size();
        int countToTake = Math.min(total, limit);
        List<Map<String, Object>> docs = new ArrayList<>(countToTake);

        for (int i = 0; i < countToTake; i++) {
            SearchHit hit = hits.get(i);
            docs.add(NoteRow.forSearchDocs(hit.note(), hit.agendaTitle(), isTool, hit.matchedIn(), hit.snippet()));
        }

        Map<String, Object> resp = new LinkedHashMap<>();
        resp.put("query", determinedQuery);
        resp.put("total", total);
        resp.put("returned", docs.size());
        if (total > docs.size()) {
            resp.put("truncated", true);
            resp.put("hint", HINT_TEXT);
        }
        resp.put("docs", docs);

        return resp;
    }

    private static int findRegionMatch(String text, String needle) {
        if (text == null || needle == null) return -1;
        int textLen = text.length();
        int needleLen = needle.length();
        if (needleLen == 0) return 0;
        if (needleLen > textLen) return -1;
        int max = textLen - needleLen;
        for (int i = 0; i <= max; i++) {
            if (text.regionMatches(true, i, needle, 0, needleLen)) {
                return i;
            }
        }
        return -1;
    }

    private static int compareCreatedInstantDesc(String c1, String c2) {
        if (c1 == null && c2 == null) return 0;
        if (c1 == null) return 1;
        if (c2 == null) return -1;
        try {
            OffsetDateTime odt1 = OffsetDateTime.parse(c1, DateTimeFormatter.ISO_OFFSET_DATE_TIME);
            OffsetDateTime odt2 = OffsetDateTime.parse(c2, DateTimeFormatter.ISO_OFFSET_DATE_TIME);
            Instant inst1 = odt1.toInstant();
            Instant inst2 = odt2.toInstant();
            int cmp = inst2.compareTo(inst1); // instant 늦은 쪽 먼저 (내림차순)
            if (cmp != 0) return cmp;
        } catch (DateTimeParseException ignored) {
            int cmp = Utf16.compare(c2, c1);
            if (cmp != 0) return cmp;
        }
        // 순간이 같으면 적힌 벽시계 시각이 늦은 쪽이 먼저 (내림차순)
        return Utf16.compare(c2, c1);
    }
}
