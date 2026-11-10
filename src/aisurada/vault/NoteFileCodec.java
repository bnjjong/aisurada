package aisurada.vault;

import aisurada.cli.AisException;
import aisurada.write.AtomicWriter;
import aisurada.write.FrontmatterWriter;

import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.charset.CharacterCodingException;
import java.nio.charset.CharsetDecoder;
import java.nio.charset.CodingErrorAction;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;

/**
 * 설명서 1.6, 1.9, 1.14 및 부록 D 규격:
 * 노트 파일의 읽기, 엄격 UTF-8 디코딩, 머리말 가르기/잇기, 프론트매터 파싱/직렬화,
 * 선결 조건 토큰 계산, 원자적 쓰기를 일원화하여 전담하는 코덱 컴포넌트.
 */
public final class NoteFileCodec {

    public static final long MAX_FILE_BYTES = 1_000_000L; // 1MB 상한

    private NoteFileCodec() {}

    /**
     * 파일 전체 바이트의 SHA-256 해시를 구하여 선결 조건 토큰(f1:...)을 생성합니다.
     */
    public static String computeToken(byte[] bytes) {
        if (bytes == null) return null;
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
     * 파일을 1MB 상한 및 엄격 UTF-8 검증 하에 읽습니다.
     * 파일이 없거나, 일반 파일이 아니거나, 1MB를 초과하거나, 잘못된 바이트가 있으면 AisException 예외가 발생합니다.
     */
    public static byte[] readStrictBytes(Path path) {
        if (!Files.isRegularFile(path)) {
            throw new AisException(404, "파일이 없거나 일반 파일이 아니다: " + path);
        }
        long size;
        try {
            size = Files.size(path);
        } catch (IOException e) {
            throw new AisException(500, "파일 크기를 읽지 못했다: " + path);
        }
        if (size > MAX_FILE_BYTES) {
            throw new AisException(400, "파일 크기가 1MB(1,000,000 바이트)를 넘는다: " + size + " bytes");
        }
        byte[] bytes;
        try {
            bytes = Files.readAllBytes(path);
        } catch (IOException e) {
            throw new AisException(500, "파일을 읽지 못했다: " + path);
        }
        CharsetDecoder decoder = StandardCharsets.UTF_8.newDecoder()
                .onMalformedInput(CodingErrorAction.REPORT)
                .onUnmappableCharacter(CodingErrorAction.REPORT);
        try {
            decoder.decode(ByteBuffer.wrap(bytes));
        } catch (CharacterCodingException e) {
            throw new AisException(400, "파일이 유효한 UTF-8 인코딩이 아니다: " + path);
        }
        return bytes;
    }

    /**
     * 머리말(frontmatter YAML)과 본문(body)을 표준 양식(--- ... --- 본문)으로 조립합니다.
     */
    public static String join(String frontmatterYaml, String body) {
        StringBuilder sb = new StringBuilder();
        if (frontmatterYaml != null && !frontmatterYaml.trim().isEmpty()) {
            sb.append("---\n");
            sb.append(frontmatterYaml.endsWith("\n") ? frontmatterYaml : frontmatterYaml + "\n");
            sb.append("---\n");
        }
        if (body != null) {
            sb.append(body);
        }
        return sb.toString();
    }

    /**
     * 프론트매터 데이터와 본문을 표준 규격으로 조립하여 대상 파일에 원자적으로 기록합니다.
     */
    public static void writeNote(Path path, FrontmatterWriter.FrontmatterData frontmatter, String body) {
        String yaml = frontmatter != null ? frontmatter.build() : "";
        String fullContent = join(yaml, body);
        byte[] bytes = fullContent.getBytes(StandardCharsets.UTF_8);
        AtomicWriter.writeAtomic(path, bytes);
    }

    /**
     * 노트 파일 파싱을 수행합니다.
     */
    public static NoteFile.ParsedNoteFile parse(byte[] bytes) {
        try {
            return NoteFile.parse(bytes);
        } catch (IOException e) {
            throw new AisException(400, "노트 파싱 실패: " + e.getMessage());
        }
    }

    /**
     * 파일 경로로부터 직접 노트 파일 파싱을 수행합니다.
     */
    public static NoteFile.ParsedNoteFile parse(Path filePath) {
        try {
            return NoteFile.parse(filePath);
        } catch (IOException e) {
            throw new AisException(400, "노트 파싱 실패: " + e.getMessage());
        }
    }
}
