package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.out.NoteRow;
import withlog.text.Trim;
import withlog.vault.AttachText;
import withlog.vault.NoteFilter;
import withlog.vault.NoteIndex;
import withlog.vault.NoteScanner;

import java.nio.file.Path;
import java.util.*;

/**
 * list_docs 및 list-docs 명령 실행기.
 * 설명서 9장에 따라 조건에 맞는 메인 노트의 메타 목록을 생성 시각 차례로 반환합니다.
 */
public final class ListDocs {

    private ListDocs() {}

    public static List<Map<String, Object>> execute(
            Path vaultRoot,
            ParsedArgs parsed,
            NoteIndex noteIndex
    ) {
        boolean isTool = parsed.isToolSurface();

        // 1. 필터 값 추출
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

        // 2. 필터 검증 및 객체 빌드 (9.3 인자 차례)
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

        // 3. 후보 노트 수집 및 필터링
        // noteIndex.allRootNotes()는 이미 9.5 정렬(created 오름차순, 경로 U+0001)되어 있음
        List<NoteScanner.NoteRecord> candidates = new ArrayList<>();
        for (NoteScanner.NoteRecord note : noteIndex.allRootNotes()) {
            // 도구 표면은 reading 층만
            if (isTool && "record".equals(note.meta().tier())) {
                continue;
            }
            if (NoteFilter.matches(note, criteria)) {
                candidates.add(note);
            }
        }

        // 4. limit 파싱 (명령 표면만)
        int limit = -1; // -1: 무제한
        if (!isTool) {
            if (parsed.namedOptions().containsKey("limit")) {
                String rawLimit = parsed.namedOptions().get("limit");
                String trimmedLimit = Trim.trim(rawLimit);
                if (trimmedLimit.isEmpty()) {
                    limit = 100;
                } else {
                    try {
                        long parsedLimit = Long.parseLong(trimmedLimit);
                        if (parsedLimit > Integer.MAX_VALUE || parsedLimit < Integer.MIN_VALUE) {
                            throw new WlException(400, "limit 은 숫자여야 한다: " + trimmedLimit);
                        }
                        int lim = (int) parsedLimit;
                        limit = (lim <= 0) ? -1 : lim;
                    } catch (NumberFormatException e) {
                        throw new WlException(400, "limit 은 숫자여야 한다: " + trimmedLimit);
                    }
                }
            } else {
                limit = 100; // 명령 표면 기본값 100
            }
        }

        // 5. 쪽 규칙(9.5)에 따라 읽을 줄 수(rowsToRead) 계산
        int total = candidates.size();
        int rowsToRead;
        if (isTool || limit <= 0) {
            rowsToRead = total;
        } else if (limit < 100) {
            // 1~99: 쪽 크기가 limit, 1쪽만 읽으면 limit에 닿음
            rowsToRead = Math.min(total, limit);
        } else {
            // limit >= 100: 쪽 크기가 100, limit에 닿거나 전체 건수에 닿을 때까지 쪽을 통째로 읽음
            int pages = (limit + 99) / 100;
            rowsToRead = Math.min(total, pages * 100);
        }

        // 6. 읽는 범위 내의 모든 메인 노트에 대해 HTML 첨부를 읽음 (9.5, 9.9 11번)
        // 입출력 오류 발생 시 AttachText.extract에서 예상 밖 예외(RuntimeException) 발생
        String[] attachTexts = new String[rowsToRead];
        for (int i = 0; i < rowsToRead; i++) {
            NoteScanner.NoteRecord note = candidates.get(i);
            attachTexts[i] = AttachText.extract(vaultRoot, note);
        }

        // 7. bodies 파싱 (명령 표면만, 9.3 8번: 목록을 다 읽은 뒤에 검사)
        boolean includeBodies = false;
        if (!isTool && parsed.namedOptions().containsKey("bodies")) {
            String rawBodies = parsed.namedOptions().get("bodies");
            String trimmedBodies = Trim.trim(rawBodies);
            if ("true".equals(trimmedBodies)) {
                includeBodies = true;
            } else if ("false".equals(trimmedBodies) || trimmedBodies.isEmpty()) {
                includeBodies = false;
            } else {
                throw new WlException(400, "bodies 은 true 나 false 여야 한다: " + trimmedBodies);
            }
        }

        // 8. 응답에 나갈 줄 수(countToTake) 자르기
        int countToTake = (limit > 0) ? Math.min(total, limit) : total;

        List<Map<String, Object>> resultRows = new ArrayList<>(countToTake);
        for (int i = 0; i < countToTake; i++) {
            NoteScanner.NoteRecord note = candidates.get(i);
            String agendaTitle = noteIndex.agendaTable().slugToTitle().getOrDefault(note.meta().agendaSlug(), "");
            String attachText = attachTexts[i];

            resultRows.add(NoteRow.forListDocs(note, agendaTitle, isTool, includeBodies, attachText));
        }

        return resultRows;
    }
}
