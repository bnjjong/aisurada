package withlog.vault;

import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.CharBuffer;
import java.nio.charset.CharacterCodingException;
import java.nio.charset.CharsetDecoder;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * RFC 3629 엄격한 UTF-8 읽기 유틸리티.
 * 설명서 0.3에 명시된 엄격한 UTF-8 해독 규칙을 수행하며,
 * 비정상 바이트 열이 있을 경우 CharacterCodingException을 발생시킵니다.
 */
public final class StrictUtf8 {
    private StrictUtf8() {}

    /**
     * 바이트 배열을 엄격한 UTF-8 규칙으로 문자열로 해독합니다.
     * @param bytes 입력 바이트 배열
     * @return 해독된 문자열
     * @throws CharacterCodingException 엄격한 UTF-8에 어긋나는 바이트가 있을 때
     */
    public static String decodeStrict(byte[] bytes) throws CharacterCodingException {
        CharsetDecoder decoder = StandardCharsets.UTF_8.newDecoder()
                .onMalformedInput(CodingErrorAction.REPORT)
                .onUnmappableCharacter(CodingErrorAction.REPORT);
        CharBuffer buffer = decoder.decode(ByteBuffer.wrap(bytes));
        return buffer.toString();
    }

    /**
     * 지정한 파일이 일반 파일이고 크기 상한 이하일 때 엄격한 UTF-8로 전체 문자열을 읽습니다.
     * @param path 파일 경로
     * @param maxBytes 바이트 단위 최대 허용 크기
     * @return 해독된 문자열
     * @throws IOException 입출력 실패, 크기 초과, 또는 UTF-8 디코딩 실패 시
     */
    public static String readStrict(Path path, long maxBytes) throws IOException {
        long size = Files.size(path);
        if (size > maxBytes) {
            throw new IOException("File size exceeds limit: " + size + " > " + maxBytes);
        }
        byte[] bytes = Files.readAllBytes(path);
        return decodeStrict(bytes);
    }
}
