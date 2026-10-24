package withlog.text;

import java.nio.ByteBuffer;
import java.nio.CharBuffer;
import java.nio.charset.CharacterCodingException;
import java.nio.charset.CharsetEncoder;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;

/**
 * UTF-16 코드 단위 처리 및 바이트 인코딩 유틸리티.
 * 이름 차례 비교, UTF-16 단위 자르기, 짝 없는 서로게이트를 '?'로 대체하는 인코딩을 제공합니다.
 */
public final class Utf16 {
    private Utf16() {}

    /**
     * 문자열을 UTF-16 코드 단위 최대 길이로 자릅니다.
     * @param s 대상 문자열
     * @param maxChars 최대 코드 단위 수
     * @return 잘린 문자열
     */
    public static String cut(String s, int maxChars) {
        if (s == null) return null;
        if (s.length() <= maxChars) return s;
        return s.substring(0, maxChars);
    }

    /**
     * 두 문자열을 UTF-16 코드 단위 순서(이름 차례)로 비교합니다.
     * @param a 첫 번째 문자열
     * @param b 두 번째 문자열
     * @return 비교 결과 정수
     */
    public static int compare(String a, String b) {
        if (a == null && b == null) return 0;
        if (a == null) return -1;
        if (b == null) return 1;
        return a.compareTo(b);
    }

    /**
     * 문자열을 UTF-8 바이트 배열로 변환합니다.
     * 짝 없는 서로게이트는 '?' (0x3F) 1바이트로 치환됩니다.
     * @param text 변환할 문자열
     * @return UTF-8 바이트 배열
     */
    public static byte[] toUtf8BytesWithQuestionForUnpaired(String text) {
        if (text == null) return new byte[0];
        CharsetEncoder encoder = StandardCharsets.UTF_8.newEncoder()
                .onMalformedInput(CodingErrorAction.REPLACE)
                .onUnmappableCharacter(CodingErrorAction.REPLACE)
                .replaceWith(new byte[]{(byte) '?'});
        try {
            ByteBuffer buffer = encoder.encode(CharBuffer.wrap(text));
            byte[] bytes = new byte[buffer.remaining()];
            buffer.get(bytes);
            return bytes;
        } catch (CharacterCodingException e) {
            // REPLACE 정책이므로 도달하지 않음
            return text.getBytes(StandardCharsets.UTF_8);
        }
    }
}
