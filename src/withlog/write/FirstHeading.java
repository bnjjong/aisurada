package withlog.write;

import withlog.text.Trim;

import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * 설명서 1.11 규격에 따른 첫 헤딩(First Heading) 추출기.
 * 마크다운 헤딩 우선 검색, 실패 시 HTML 헤딩 대체 검색 및 엔티티 해제를 수행합니다.
 */
public final class FirstHeading {

    private static final Pattern MD_HEADING_PATTERN = Pattern.compile("^[ \\t\\n\\x0B\\f\\r]{0,3}#{1,6}[ \\t\\n\\x0B\\f\\r]+(.+?)[ \\t\\n\\x0B\\f\\r]*#*[ \\t\\n\\x0B\\f\\r]*$");
    private static final Pattern HTML_HEADING_PATTERN = Pattern.compile("(?i)<h[1-6][^>]*>([\\s\\S]*?)</h[1-6]>");
    private static final Pattern HTML_TAG_PATTERN = Pattern.compile("<[^>]+>");
    private static final Pattern NUMERIC_APOS_PATTERN = Pattern.compile("&#0*39;");
    private static final Pattern ASCII_SPACES_PATTERN = Pattern.compile("[ \\t\\n\\x0B\\f\\r]+");

    private FirstHeading() {}

    /**
     * 본문에서 첫 번째 헤딩을 추출합니다.
     * 결과가 비어 있거나 정확히 "[제목]"인 경우 헤딩이 없는 것으로 판정(null 반환)합니다.
     *
     * @param body 원본 본문 문자열
     * @return 추출된 첫 헤딩 문자열 (헤딩이 없으면 null)
     */
    public static String extract(String body) {
        if (body == null || body.isEmpty()) {
            return null;
        }

        String heading = extractMarkdownHeading(body);
        if (heading == null) {
            heading = extractHtmlHeading(body);
        }

        if (heading == null || heading.isEmpty() || "[제목]".equals(heading)) {
            return null;
        }
        return heading;
    }

    private static String extractMarkdownHeading(String body) {
        String[] lines = body.split("\n", -1);
        for (String line : lines) {
            Matcher m = MD_HEADING_PATTERN.matcher(line);
            if (m.matches()) {
                String rawGroup = m.group(1);
                String trimmed = Trim.trim(rawGroup);
                if (trimmed.isEmpty()) {
                    // 마크다운 헤딩이 빈 글이면 다음 마크다운 줄로 가지 않고 HTML 헤딩으로 넘어감 (1.11)
                    return null;
                }
                return trimmed;
            }
        }
        return null;
    }

    private static String extractHtmlHeading(String body) {
        Matcher m = HTML_HEADING_PATTERN.matcher(body);
        if (!m.find()) {
            return null;
        }
        String inner = m.group(1);
        // <[^>]+> 태그 제거
        String noTags = HTML_TAG_PATTERN.matcher(inner).replaceAll("");

        // 엔티티 6가지 순차 해제 (대소문자 구분)
        String unescaped = noTags.replace("&amp;", "&")
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&quot;", "\"");

        unescaped = NUMERIC_APOS_PATTERN.matcher(unescaped).replaceAll("'");
        unescaped = unescaped.replace("&apos;", "'")
                .replace("&nbsp;", " ");

        // ASCII 공백 이어진 구간을 공백 1개로 줄이고 다듬음
        String collapsed = ASCII_SPACES_PATTERN.matcher(unescaped).replaceAll(" ");
        return Trim.trim(collapsed);
    }
}
