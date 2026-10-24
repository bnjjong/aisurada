package withlog.vault;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.Map;

/**
 * 노트 본문 파일 파서 및 바이트 처리기.
 * 설명서 1.6 및 1.14에 따라 울타리 분리, 프론트매터 추출, 본문 슬라이싱,
 * 그리고 선결 조건 토큰(f1:sha256) 생성을 수행합니다.
 */
public final class NoteFile {

    /**
     * 노트 파일 파싱 결과 DTO.
     * @param frontmatter 파싱된 프론트매터 Map (없으면 null)
     * @param body 본문 문자열
     * @param token f1:sha256 선결 조건 토큰 (json 노트이거나 본문 파일 미파싱 시 null)
     * @param hasFrontmatter 유효한 프론트매터가 존재하는지 여부
     */
    public record ParsedNoteFile(
            Map<String, Object> frontmatter,
            String body,
            String token,
            boolean hasFrontmatter
    ) {}

    private NoteFile() {}

    /**
     * 파일 전체 바이트의 SHA-256 해시를 구하여 선결 조건 토큰(f1:...)을 생성합니다.
     * @param bytes 파일 전체 바이트
     * @return f1:소문자16진수64자 토큰
     */
    public static String computeToken(byte[] bytes) {
        try {
            MessageDigest md = MessageDigest.getInstance("SHA-256");
            byte[] digest = md.digest(bytes);
            StringBuilder sb = new StringBuilder("f1:");
            for (byte b : digest) {
                sb.append(String.format("%02x", b));
            }
            return sb.toString();
        } catch (NoSuchAlgorithmException e) {
            throw new RuntimeException("SHA-256 algorithm not available", e);
        }
    }

    /**
     * 노트 본문 파일을 읽어 프론트매터와 본문, 토큰을 파싱합니다.
     * @param filePath 본문 파일 경로
     * @return 파싱 결과 ParsedNoteFile
     * @throws IOException 파일이 없거나, 1MB 초과하거나, 엄격한 UTF-8이 아닌 경우
     */
    public static ParsedNoteFile parse(Path filePath) throws IOException {
        long size = Files.size(filePath);
        if (size > 1_000_000) {
            throw new IOException("File size exceeds 1,000,000 bytes: " + size);
        }

        byte[] rawBytes = Files.readAllBytes(filePath);
        String token = computeToken(rawBytes);
        String content = StrictUtf8.decodeStrict(rawBytes);

        if (content.startsWith("\uFEFF")) {
            content = content.substring(1);
        }

        String yamlPart = null;
        String rawBody = "";
        boolean fenceFound = false;

        if (content.startsWith("<!--withlog")) {
            int closeIdx = content.indexOf("\n-->", 11);
            if (closeIdx != -1) {
                yamlPart = content.substring(11, closeIdx);
                rawBody = content.substring(closeIdx + 4);
                fenceFound = true;
            }
        } else if (content.startsWith("---")) {
            int firstLf = content.indexOf('\n', 3);
            if (firstLf != -1) {
                int closeIdx = content.indexOf("\n---", firstLf);
                if (closeIdx != -1) {
                    yamlPart = content.substring(firstLf + 1, closeIdx + 1);
                    rawBody = content.substring(closeIdx + 4);
                    fenceFound = true;
                }
            }
        }

        // 본문 앞 줄바꿈 정리 (1.6 6번): \r\n 1개 떼고, 이어서 \n 1개 뗌
        String body = rawBody;
        if (body.startsWith("\r\n")) {
            body = body.substring(2);
        }
        if (body.startsWith("\n")) {
            body = body.substring(1);
        }

        if (!fenceFound || yamlPart == null) {
            // 울타리가 없으면 프론트매터 없음
            return new ParsedNoteFile(null, body, token, false);
        }

        if (yamlPart.length() > 16_000) {
            return new ParsedNoteFile(null, body, token, false);
        }

        Map<String, Object> frontmatter = Frontmatter.parse(yamlPart);
        if (frontmatter == null) {
            return new ParsedNoteFile(null, body, token, false);
        }

        return new ParsedNoteFile(frontmatter, body, token, true);
    }
}
