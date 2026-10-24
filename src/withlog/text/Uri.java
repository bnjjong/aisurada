package withlog.text;

import java.nio.charset.StandardCharsets;

/**
 * URI 인코딩 유틸리티.
 * JavaScript encodeURIComponent 표준 규격에 따라
 * 영문자, 숫자, -_.!~*'() 이외의 모든 문자를 대문자 %XX로 변환합니다.
 */
public final class Uri {
    private static final String UNRESERVED = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_.!~*'()";

    private Uri() {}

    /**
     * 문자열을 encodeURIComponent 규칙으로 변환합니다.
     * @param s 대상 문자열
     * @return 인코딩된 문자열
     */
    public static String encodeURIComponent(String s) {
        if (s == null) return "";
        byte[] bytes = s.getBytes(StandardCharsets.UTF_8);
        StringBuilder sb = new StringBuilder(bytes.length * 3);
        for (byte b : bytes) {
            int ch = b & 0xFF;
            if (UNRESERVED.indexOf(ch) != -1) {
                sb.append((char) ch);
            } else {
                sb.append('%');
                char h1 = Character.toUpperCase(Character.forDigit((ch >> 4) & 0xF, 16));
                char h2 = Character.toUpperCase(Character.forDigit(ch & 0xF, 16));
                sb.append(h1).append(h2);
            }
        }
        return sb.toString();
    }
}
