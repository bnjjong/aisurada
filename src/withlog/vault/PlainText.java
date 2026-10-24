package withlog.vault;

import withlog.text.Trim;

import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * 노트 본문의 포맷별 평문 텍스트 추출기.
 * 설명서 10.5에 따라 markdown 13단계, html 4단계, json(빈 글) 규칙을 적용합니다.
 */
public final class PlainText {

    private static final Pattern MD_STEP1 = Pattern.compile("<!--[\\s\\S]*?-->");
    private static final Pattern MD_STEP2 = Pattern.compile("```[\\s\\S]*?```");
    private static final Pattern MD_STEP3 = Pattern.compile("`([^`]*)`");
    private static final Pattern MD_STEP4 = Pattern.compile("!\\[[^\\]]*\\]\\([^)]*\\)");
    private static final Pattern MD_STEP5 = Pattern.compile("\\[([^\\]]*)\\]\\([^)]*\\)");
    private static final Pattern MD_STEP6 = Pattern.compile("^\\s{0,3}#{1,6}\\s+", Pattern.MULTILINE);
    private static final Pattern MD_STEP7 = Pattern.compile("^\\s*>\\s?(\\[![a-z]+\\])?\\s*", Pattern.MULTILINE | Pattern.CASE_INSENSITIVE | Pattern.UNICODE_CASE);
    private static final Pattern MD_STEP8 = Pattern.compile("^\\s*(?:[-*+]|\\d+\\.)\\s+(?:\\[[ x/-]\\]\\s*)?", Pattern.MULTILINE);
    private static final Pattern MD_STEP9 = Pattern.compile("^\\s*\\|[\\s:|-]*\\|\\s*$", Pattern.MULTILINE);
    private static final Pattern MD_STEP10 = Pattern.compile("^\\s*\\|.*\\|\\s*$", Pattern.MULTILINE);
    private static final Pattern MD_STEP11 = Pattern.compile("^\\s*[-*_]{3,}\\s*$", Pattern.MULTILINE);
    private static final Pattern MD_STEP12 = Pattern.compile("\\*\\*([^*]+)\\*\\*|\\*([^*]+)\\*|__([^_]+)__|~~([^~]+)~~");
    private static final Pattern MD_STEP13 = Pattern.compile("\\s+");

    private PlainText() {}

    /**
     * 포맷과 원본 본문으로부터 평문 텍스트를 추출합니다.
     * @param format markdown, html, json 중 하나
     * @param rawBody 원본 본문 문자열
     * @return 5.3으로 다듬어진 평문 텍스트
     */
    public static String extract(String format, String rawBody) {
        if (rawBody == null || rawBody.isEmpty()) {
            return "";
        }
        if ("json".equals(format)) {
            return "";
        }
        if ("html".equals(format)) {
            return AttachText.htmlToPlainText(rawBody);
        }

        // markdown
        String s = rawBody;
        s = MD_STEP1.matcher(s).replaceAll(" ");
        s = MD_STEP2.matcher(s).replaceAll(" ");
        s = MD_STEP3.matcher(s).replaceAll("$1");
        s = MD_STEP4.matcher(s).replaceAll(" ");
        s = MD_STEP5.matcher(s).replaceAll("$1");
        s = MD_STEP6.matcher(s).replaceAll("");
        s = MD_STEP7.matcher(s).replaceAll("");
        s = MD_STEP8.matcher(s).replaceAll("");
        s = MD_STEP9.matcher(s).replaceAll(" ");
        s = MD_STEP10.matcher(s).replaceAll(mr -> Matcher.quoteReplacement(mr.group().replace('|', ' ')));
        s = MD_STEP11.matcher(s).replaceAll(" ");
        s = MD_STEP12.matcher(s).replaceAll(mr -> {
            for (int g = 1; g <= 4; g++) {
                String val = mr.group(g);
                if (val != null) {
                    return Matcher.quoteReplacement(val);
                }
            }
            return "";
        });
        s = MD_STEP13.matcher(s).replaceAll(" ");
        return Trim.trim(s);
    }
}
