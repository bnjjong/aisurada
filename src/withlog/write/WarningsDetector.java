package withlog.write;

import withlog.text.Trim;
import withlog.text.Utf16;
import withlog.vault.NoteScanner;

import java.util.*;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * 설명서 13.6에 명시된 5가지 경고 판정 및 문구 생성기.
 * 1. 같은 제목 경고
 * 2. 들여쓴 블록 경고
 * 3. 제목과 첫 헤딩 불일치 경고
 * 4. 깨진 글자(U+FFFD) 경고
 * 5. "## 원본 요청" 절 부재 경고 (도구 표면 전용)
 */
public final class WarningsDetector {

    private static final Pattern ORIGINAL_REQUEST_PATTERN = Pattern.compile("(?m)^#{2,6}[ \\t\\n\\x0B\\f\\r]*원본[ \\t\\n\\x0B\\f\\r]*요청");
    private static final Pattern ASCII_COLLAPSE_PATTERN = Pattern.compile("[ \\t\\n\\x0B\\f\\r]+");
    private static final Pattern CALLOUT_PATTERN = Pattern.compile("^\\[![a-zA-Z0-9_]+\\]");

    private WarningsDetector() {}

    /**
     * 5가지 경고를 순서대로 판정하여 하나로 결합된 경고 문자열을 반환합니다.
     * 경고가 하나도 없으면 null을 반환합니다.
     *
     * @param isToolSurface 도구 표면 호출 여부
     * @param body 전달받은 본문
     * @param rawTitle 전달받은 제목 (다듬지 않은 원문)
     * @param sameAgendaNotes 동일 아젠다의 기존 메인 노트 목록
     * @return 결합된 경고 문자열 또는 null
     */
    public static String detectWarnings(
            boolean isToolSurface,
            String body,
            String rawTitle,
            List<NoteScanner.NoteRecord> sameAgendaNotes
    ) {
        List<String> warnings = new ArrayList<>();

        // 1. 같은 제목 경고
        String warn1 = detectDuplicateTitleWarning(sameAgendaNotes, rawTitle);
        if (warn1 != null) {
            warnings.add(warn1);
        }

        // 2. 들여쓴 블록 경고
        String warn2 = detectIndentedBlocksWarning(body);
        if (warn2 != null) {
            warnings.add(warn2);
        }

        // 3. 제목과 첫 헤딩 불일치 경고
        String warn3 = detectTitleMismatchWarning(body, rawTitle);
        if (warn3 != null) {
            warnings.add(warn3);
        }

        // 4. U+FFFD 경고
        String warn4 = detectBrokenCharWarning(body);
        if (warn4 != null) {
            warnings.add(warn4);
        }

        // 5. 원본 요청 헤딩 부재 경고 (도구 표면만)
        if (isToolSurface) {
            String warn5 = detectMissingOriginalRequestWarning(body);
            if (warn5 != null) {
                warnings.add(warn5);
            }
        }

        if (warnings.isEmpty()) {
            return null;
        }
        return String.join("\n", warnings);
    }

    /**
     * 1. 같은 제목 경고 판정
     */
    public static String detectDuplicateTitleWarning(List<NoteScanner.NoteRecord> sameAgendaNotes, String rawTitle) {
        if (sameAgendaNotes == null || sameAgendaNotes.isEmpty()) {
            return null;
        }

        String searchKey = makeTitleKey(rawTitle);
        List<NoteScanner.NoteRecord> matches = new ArrayList<>();

        for (NoteScanner.NoteRecord record : sameAgendaNotes) {
            String t = record.meta() != null && record.meta().title() != null ? record.meta().title() : "";
            if (makeTitleKey(t).equals(searchKey)) {
                matches.add(record);
            }
        }

        if (matches.isEmpty()) {
            return null;
        }

        // path UTF-16 오름차순 정렬
        matches.sort((a, b) -> Utf16.compare(a.vaultRelPath(), b.vaultRelPath()));

        List<String> items = new ArrayList<>();
        for (NoteScanner.NoteRecord rec : matches) {
            String id = rec.meta().id();
            String created = rec.meta().created();
            String datePart = "";
            if (created != null && created.length() >= 10) {
                datePart = created.substring(0, 10);
            }
            boolean isFav = rec.meta().favorite();
            if (isFav) {
                items.add(id + "(" + datePart + ", 즐겨찾기)");
            } else {
                items.add(id + "(" + datePart + ")");
            }
        }

        String listStr = String.join(", ", items);
        return "같은 아젠다에 제목이 같은 노트가 이미 있어요: " + listStr +
                ". 기존 노트를 고쳐 쓰려던 거면 지금 만든 노트를 delete_doc으로 지우고 update_doc({ id, body })으로 그 노트를 키우세요 — 즐겨찾기·평가·연결은 노트 id에 붙어서 새 노트로 따라오지 않습니다.";
    }

    public static String makeTitleKey(String title) {
        String trimmed = Trim.trim(title);
        String collapsed = ASCII_COLLAPSE_PATTERN.matcher(trimmed).replaceAll(" ");
        return collapsed.toLowerCase(Locale.ROOT);
    }

    /**
     * 2. 들여쓴 블록 경고 판정
     */
    public static String detectIndentedBlocksWarning(String body) {
        if (body == null || body.isEmpty()) {
            return null;
        }

        // 1. CRLF 및 단독 CR을 LF로 변환
        String normalized = body.replace("\r\n", "\n").replace("\r", "\n");
        String[] lines = normalized.split("\n", -1);

        List<String> locationItems = new ArrayList<>();
        int totalPlaces = 0;
        boolean inFence = false;

        for (int i = 0; i < lines.length; i++) {
            String line = lines[i];
            int lineNum = i + 1;

            // 2. 울타리 줄 판정: 공백 0개 이상 뒤 백틱 3개 이상
            int firstNonSpace = 0;
            while (firstNonSpace < line.length() && isAsciiSpace(line.charAt(firstNonSpace))) {
                firstNonSpace++;
            }
            if (firstNonSpace < line.length() && line.startsWith("```", firstNonSpace)) {
                inFence = !inFence;
                continue;
            }

            if (inFence) {
                continue;
            }

            // 3. 맨 앞이 공백인 줄만 검사
            if (line.isEmpty() || !isAsciiSpace(line.charAt(0))) {
                continue;
            }

            String r = line.substring(firstNonSpace);
            String kind = classifyIndentedBlock(r, lines, i);

            if (kind != null) {
                totalPlaces++;
                if (locationItems.size() < 5) {
                    locationItems.add(lineNum + "번째 줄 " + kind);
                }
            }
        }

        if (totalPlaces == 0) {
            return null;
        }

        StringBuilder posSb = new StringBuilder();
        posSb.append(String.join(", ", locationItems));
        if (totalPlaces > 5) {
            posSb.append(" 외 ").append(totalPlaces - 5).append("곳");
        }

        return "본문에 들여쓴 블록이 " + totalPlaces + "곳 있어요(" + posSb +
                ") — 표·콜아웃·제목·하위 페이지 카드는 줄 맨 앞에서 시작해야 편집기가 변환합니다. 지금 상태로는 저장은 됐지만 화면에 그냥 문단 글자로 보입니다. 들여쓰기를 떼고 update_doc({ id, body })으로 다시 저장하세요. 리스트 항목 안에는 표를 넣을 수 없으니, 항목 문장에서 \"아래 표\"라고 가리키고 표는 들여쓰기 없이 다음 줄에 두세요.";
    }

    private static String classifyIndentedBlock(String r, String[] lines, int lineIdx) {
        // 4. R이 # 1~6개로 시작하고 바로 뒤가 공백이면 제목
        int hashCount = 0;
        while (hashCount < r.length() && r.charAt(hashCount) == '#') {
            hashCount++;
        }
        if (hashCount >= 1 && hashCount <= 6) {
            if (hashCount < r.length() && isAsciiSpace(r.charAt(hashCount))) {
                return "제목";
            }
        }

        // 5. R이 '>'로 시작
        if (r.startsWith(">")) {
            String afterGt = r.substring(1);
            if (!afterGt.isEmpty() && isAsciiSpace(afterGt.charAt(0))) {
                afterGt = afterGt.substring(1);
            }
            if (isCallout(afterGt)) {
                return "콜아웃";
            } else {
                return "인용";
            }
        }

        // 6. R이 '['로 시작하고 이어 '](#sub:'이 오는 하위 페이지 카드
        if (r.startsWith("[")) {
            int closeBracket = r.indexOf(']');
            if (closeBracket > 0 && r.startsWith("](#sub:", closeBracket)) {
                return "하위 페이지 카드";
            }
        }

        // 7. R에 '|'가 있고 다음 줄이 유효한 표 구분선이면 표
        if (r.contains("|") && lineIdx + 1 < lines.length) {
            String nextLine = lines[lineIdx + 1];
            String trimmedNext = Trim.trim(nextLine);
            if (isTableSeparatorLine(trimmedNext)) {
                return "표";
            }
        }

        return null;
    }

    private static boolean isCallout(String s) {
        Matcher m = CALLOUT_PATTERN.matcher(s);
        if (!m.find()) {
            return false;
        }
        // 그 뒤 글의 맨 끝이 아닌 자리에 U+0085, U+2028, U+2029가 있으면 정규식 .이 못 넘어 인용이 됨 (13.6)
        String rest = s.substring(m.end());
        for (int i = 0; i < rest.length() - 1; i++) {
            char c = rest.charAt(i);
            if (c == 0x85 || c == 0x2028 || c == 0x2029) {
                return false;
            }
        }
        return true;
    }

    private static boolean isTableSeparatorLine(String trimmed) {
        if (trimmed.isEmpty()) return false;
        boolean hasHyphen = false;
        for (int i = 0; i < trimmed.length(); i++) {
            char c = trimmed.charAt(i);
            if (Trim.isWhitespace(c) || c == ':' || c == '|') {
                continue;
            } else if (c == '-') {
                hasHyphen = true;
            } else {
                return false;
            }
        }
        return hasHyphen;
    }

    private static boolean isAsciiSpace(char c) {
        return c == ' ' || c == '\t' || c == 0x0B || c == '\f';
    }

    /**
     * 3. 제목과 첫 헤딩 불일치 경고 판정
     */
    public static String detectTitleMismatchWarning(String body, String rawTitle) {
        String firstHeading = FirstHeading.extract(body);
        if (firstHeading == null) {
            return null;
        }
        if (!firstHeading.equals(rawTitle)) {
            return "본문 첫 헤딩(\"" + firstHeading + "\")이 제목(\"" + rawTitle +
                    "\")과 달라요 — 앱에서 이 노트를 한 글자라도 고치면 그 순간 제목이 이 헤딩으로 바뀌고 폴더도 따라 움직입니다. 본문 첫 줄을 \"# " +
                    rawTitle + "\"로 맞추거나, update_doc으로 제목을 헤딩값에 맞추세요.";
        }
        return null;
    }

    /**
     * 4. U+FFFD 경고 판정
     */
    public static String detectBrokenCharWarning(String body) {
        if (body == null || body.indexOf('\uFFFD') < 0) {
            return null;
        }

        int count = 0;
        for (int i = 0; i < body.length(); i++) {
            if (body.charAt(i) == '\uFFFD') {
                count++;
            }
        }

        String normalized = body.replace("\r\n", "\n").replace("\r", "\n");
        String[] lines = normalized.split("\n", -1);

        List<String> posItems = new ArrayList<>();
        int lineCount = 0;
        for (int i = 0; i < lines.length; i++) {
            if (lines[i].indexOf('\uFFFD') >= 0) {
                lineCount++;
                if (posItems.size() < 5) {
                    posItems.add((i + 1) + "번째 줄");
                }
            }
        }

        StringBuilder posSb = new StringBuilder();
        posSb.append(String.join(", ", posItems));
        if (lineCount > 5) {
            posSb.append(" 외 ").append(lineCount - 5).append("줄");
        }

        return "본문에 깨진 글자(U+FFFD)가 " + count + "개 늘었어요(지금 깨진 자리: " + posSb +
                ") — 본문을 청크로 이어 읽으면 다바이트 글자가 경계에서 쪼개져 이렇게 죽습니다. 한글은 글자마다 3바이트라 큰 노트에서 반드시 걸립니다. 저장은 됐지만 그 자리의 원래 글자는 사라졌습니다. 본문을 통째로 한 번에 디코드해서 다시 읽고, 그 값으로 다시 저장하세요. vault 가 git 저장소이므로 직전 판에서 원래 글자를 찾을 수 있습니다.";
    }

    /**
     * 5. 원본 요청 헤딩 부재 경고 판정
     */
    public static String detectMissingOriginalRequestWarning(String body) {
        if (body == null || !ORIGINAL_REQUEST_PATTERN.matcher(body).find()) {
            return "본문에 \"## 원본 요청\" 절이 없어요 — 반년 뒤에도 \"무엇을 요청했나\"에 답하려면 `## 요약` 바로 다음에 `## 원본 요청`(사용자의 말을 인용부호로 원문 그대로)을 두세요. 대화 중에 갈래를 골랐으면 `## 대화에서 좁힌 범위`도 그 바로 다음에 둡니다.";
        }
        return null;
    }
}
