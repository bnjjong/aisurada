package withlog.text;

/**
 * withlog 규격의 28글자 공백 판정 및 다듬기 유틸리티.
 * U+0009~U+000D, U+001C~U+001F, U+0020, U+00A0, U+1680,
 * U+2000~U+200A, U+2028, U+2029, U+202F, U+205F, U+3000 문자를 다듬습니다.
 */
public final class Trim {
    private Trim() {}

    /**
     * 지정한 문자가 withlog 28글자 공백 문자에 해당하는지 판정합니다.
     * @param c 검사할 문자
     * @return 28글자 공백에 해당하면 true
     */
    public static boolean isWhitespace(char c) {
        if (c >= '\t' && c <= '\r') return true;
        if (c >= '\u001C' && c <= '\u001F') return true;
        if (c == ' ') return true;
        if (c == '\u00A0') return true;
        if (c == '\u1680') return true;
        if (c >= '\u2000' && c <= '\u200A') return true;
        if (c == '\u2028' || c == '\u2029' || c == '\u202F' || c == '\u205F') return true;
        if (c == '\u3000') return true;
        return false;
    }

    /**
     * 문자열의 앞뒤에서 28글자 공백을 제거합니다.
     * @param s 대상 문자열
     * @return 다듬어진 문자열
     */
    public static String trim(String s) {
        if (s == null || s.isEmpty()) return "";
        int start = 0;
        int end = s.length();
        while (start < end && isWhitespace(s.charAt(start))) {
            start++;
        }
        while (end > start && isWhitespace(s.charAt(end - 1))) {
            end--;
        }
        return s.substring(start, end);
    }

    /**
     * 문자열이 null이거나 28글자 공백으로만 이루어져 있는지 판정합니다.
     * @param s 대상 문자열
     * @return 공백뿐이면 true
     */
    public static boolean isBlank(String s) {
        if (s == null || s.isEmpty()) return true;
        for (int i = 0; i < s.length(); i++) {
            if (!isWhitespace(s.charAt(i))) {
                return false;
            }
        }
        return true;
    }

    /**
     * 지정한 문자가 10.6 '검색 다듬기' 집합(28글자 공백 + U+FEFF)에 해당하는지 판정합니다.
     */
    public static boolean isSearchWhitespace(char c) {
        return isWhitespace(c) || c == '\uFEFF';
    }

    /**
     * 문자열 앞뒤에서 10.6 '검색 다듬기' 문자들을 제거합니다.
     */
    public static String trimSearch(String s) {
        if (s == null || s.isEmpty()) return "";
        int start = 0;
        int end = s.length();
        while (start < end && isSearchWhitespace(s.charAt(start))) {
            start++;
        }
        while (end > start && isSearchWhitespace(s.charAt(end - 1))) {
            end--;
        }
        return s.substring(start, end);
    }
}
