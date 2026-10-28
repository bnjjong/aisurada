package withlog.write;

import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.HexFormat;

/**
 * 선결 조건 토큰 계산기.
 * 설명서 1.14에 따라 본문 파일 전체 바이트의 SHA-256을 'f1:<소문자 64자>' 형식으로 계산합니다.
 */
public final class DocToken {

    private DocToken() {}

    /**
     * 바이트 배열에 대한 f1 토큰을 계산합니다.
     *
     * @param bytes 파일 전체 바이트 배열
     * @return f1:<hex> 형식의 토큰 문자열
     */
    public static String compute(byte[] bytes) {
        if (bytes == null) {
            bytes = new byte[0];
        }
        try {
            MessageDigest md = MessageDigest.getInstance("SHA-256");
            byte[] digest = md.digest(bytes);
            return "f1:" + HexFormat.of().formatHex(digest);
        } catch (NoSuchAlgorithmException e) {
            throw new RuntimeException("SHA-256 algorithm not available", e);
        }
    }
}
