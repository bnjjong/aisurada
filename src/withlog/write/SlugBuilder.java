package withlog.write;

import withlog.text.Trim;
import withlog.text.Utf16;

import java.util.Locale;

/**
 * 설명서 1.5 규격에 따른 슬러그(slug) 생성기.
 * 제목 또는 아젠다 이름을 받아 URL 및 폴더 이름에 안전한 슬러그로 변환합니다.
 */
public final class SlugBuilder {

    private SlugBuilder() {}

    /**
     * 원본 문자열을 설명서 1.5 규칙에 따라 최대 50자의 슬러그로 변환합니다.
     *
     * @param raw 원본 문자열 (제목 등)
     * @return 슬러그 문자열
     */
    public static String build(String raw) {
        return build(raw, 50);
    }

    /**
     * 원본 문자열을 설명서 1.5 규칙에 따라 지정한 최대 길이의 슬러그로 변환합니다.
     *
     * 1. Trim (28글자 공백 제거)
     * 2. toLowerCase(Locale.ROOT)
     * 3. '와 " 제거
     * 4. \p{L}도 \p{N}도 아닌 코드 포인트가 이어진 구간을 '-' 1개로 치환
     * 5. 앞뒤 '-' 제거
     * 6. 비어 있으면 "untitled"
     * 7. UTF-16 코드 단위 maxChars개로 자름 (자른 뒤 안 다듬음)
     *
     * @param raw 원본 문자열
     * @param maxChars 최대 UTF-16 코드 단위 수
     * @return 슬러그 문자열
     */
    public static String build(String raw, int maxChars) {
        // 1. 다듬는다
        String s = Trim.trim(raw);

        // 2. 로캘 없이 소문자로 바꾼다
        s = s.toLowerCase(Locale.ROOT);

        // 3. ' 와 " 를 지운다
        StringBuilder sbNoQuotes = new StringBuilder(s.length());
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            if (c != '\'' && c != '"') {
                sbNoQuotes.append(c);
            }
        }
        s = sbNoQuotes.toString();

        // 4. 유니코드 글자(\p{L})도 숫자(\p{N})도 아닌 코드 포인트가 이어진 구간을 '-' 1개로 바꾼다
        StringBuilder sbSlug = new StringBuilder();
        boolean inDash = false;
        int len = s.length();
        for (int i = 0; i < len; ) {
            int cp = s.codePointAt(i);
            if (isLetterOrDigit(cp)) {
                sbSlug.appendCodePoint(cp);
                inDash = false;
            } else {
                if (!inDash) {
                    sbSlug.append('-');
                    inDash = true;
                }
            }
            i += Character.charCount(cp);
        }
        s = sbSlug.toString();

        // 5. 앞뒤 '-'를 뗀다
        int start = 0;
        int end = s.length();
        while (start < end && s.charAt(start) == '-') {
            start++;
        }
        while (end > start && s.charAt(end - 1) == '-') {
            end--;
        }
        s = s.substring(start, end);

        // 6. 비면 "untitled"
        if (s.isEmpty()) {
            s = "untitled";
        }

        // 7. UTF-16 코드 단위 maxChars개로 자르고, 자른 뒤 안 다듬는다
        return Utf16.cut(s, maxChars);
    }

    private static boolean isLetterOrDigit(int cp) {
        int type = Character.getType(cp);
        return type == Character.UPPERCASE_LETTER ||
                type == Character.LOWERCASE_LETTER ||
                type == Character.TITLECASE_LETTER ||
                type == Character.MODIFIER_LETTER ||
                type == Character.OTHER_LETTER ||
                type == Character.DECIMAL_DIGIT_NUMBER ||
                type == Character.LETTER_NUMBER ||
                type == Character.OTHER_NUMBER;
    }
}
