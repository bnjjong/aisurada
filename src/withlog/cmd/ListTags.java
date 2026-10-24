package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.vault.NoteIndex;
import withlog.vault.NoteScanner;

import java.nio.charset.StandardCharsets;
import java.util.*;

/**
 * list_tags 및 list-tags 명령 실행기.
 * 설명서 12장에 따라 reading 층 메인 노트의 태그 빈도를 집계하여 반환합니다.
 */
public final class ListTags {

    private ListTags() {}

    public static List<Map<String, Object>> execute(
            ParsedArgs parsed,
            NoteIndex noteIndex
    ) {
        boolean isTool = parsed.isToolSurface();
        String query = null;
        Integer limit = null;

        if (isTool) {
            Map<String, Object> opts = parsed.toolOptions();
            if (opts != null) {
                query = (String) opts.get("query");
                Long toolLimitLong = (Long) opts.get("limit");
                if (toolLimitLong != null) {
                    if (toolLimitLong > Integer.MAX_VALUE) {
                        throw new WlException(400, "limit 은 숫자여야 한다: " + toolLimitLong);
                    }
                    limit = toolLimitLong.intValue();
                }
            }
        } else {
            Map<String, String> opts = parsed.namedOptions();
            if (opts.containsKey("query")) {
                query = opts.get("query");
            }
            if (opts.containsKey("limit")) {
                String rawLimit = opts.get("limit");
                String trimmedLimit = Trim.trim(rawLimit);
                if (!trimmedLimit.isEmpty()) {
                    try {
                        long parsedLimit = Long.parseLong(trimmedLimit);
                        if (parsedLimit > Integer.MAX_VALUE || parsedLimit < Integer.MIN_VALUE) {
                            throw new WlException(400, "limit 은 숫자여야 한다: " + trimmedLimit);
                        }
                        int lim = (int) parsedLimit;
                        if (lim < 1) {
                            throw new WlException(400, "태그 개수 상한은 1 이상이다: " + lim);
                        }
                        limit = lim;
                    } catch (NumberFormatException e) {
                        throw new WlException(400, "limit 은 숫자여야 한다: " + trimmedLimit);
                    }
                }
            }
        }

        // query 다듬기
        String trimmedQuery = null;
        if (query != null) {
            String t = Trim.trim(query);
            if (!t.isEmpty()) {
                trimmedQuery = t;
            }
        }

        // 태그 수집: reading 층 루트 노트만!
        Map<String, Integer> tagCounts = new HashMap<>();
        for (NoteScanner.NoteRecord note : noteIndex.allRootNotes()) {
            if ("record".equals(note.meta().tier())) {
                continue;
            }
            List<String> tags = note.meta().tags();
            if (tags != null) {
                for (String tag : tags) {
                    tagCounts.put(tag, tagCounts.getOrDefault(tag, 0) + 1);
                }
            }
        }

        // 색인에 태그가 1개도 없으면 SQLite가 패턴을 쓸 일이 없어서 [] 반환 (12.9 6번)
        if (tagCounts.isEmpty()) {
            return Collections.emptyList();
        }

        // 50,000 바이트 검사 (12.3, 12.9 6번): 태그가 1개라도 있을 때만 패턴 길이 검사
        if (trimmedQuery != null) {
            int patternBytes = 2; // 앞뒤 %
            for (int i = 0; i < trimmedQuery.length(); i++) {
                char c = trimmedQuery.charAt(i);
                if (c == '%' || c == '_' || c == '\\') {
                    patternBytes++; // escape char \
                }
            }
            patternBytes += trimmedQuery.getBytes(StandardCharsets.UTF_8).length;
            if (patternBytes > 50000) {
                throw new RuntimeException("LIKE or GLOB pattern too complex");
            }
        }

        // query 필터링
        List<Map.Entry<String, Integer>> filtered = new ArrayList<>();
        for (Map.Entry<String, Integer> entry : tagCounts.entrySet()) {
            String tag = entry.getKey();
            if (trimmedQuery == null || asciiLikeContains(tag, trimmedQuery)) {
                filtered.add(entry);
            }
        }

        // 정렬: count 내림차순, 같으면 tag UTF-8 바이트 오름차순
        filtered.sort((e1, e2) -> {
            int cmp = Integer.compare(e2.getValue(), e1.getValue());
            if (cmp != 0) return cmp;
            return NoteIndex.compareUtf8Bytes(e1.getKey(), e2.getKey());
        });

        // limit 적용
        if (limit != null && limit < filtered.size()) {
            filtered = filtered.subList(0, limit);
        }

        List<Map<String, Object>> result = new ArrayList<>(filtered.size());
        for (Map.Entry<String, Integer> entry : filtered) {
            Map<String, Object> row = new LinkedHashMap<>();
            row.put("tag", entry.getKey());
            row.put("count", entry.getValue());
            result.add(row);
        }

        return result;
    }

    private static boolean asciiLikeContains(String tag, String query) {
        return toAsciiLower(tag).contains(toAsciiLower(query));
    }

    private static String toAsciiLower(String s) {
        StringBuilder sb = new StringBuilder(s.length());
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c >= 'A' && c <= 'Z') {
                sb.append((char) (c + ('a' - 'A')));
            } else {
                sb.append(c);
            }
        }
        return sb.toString();
    }
}
