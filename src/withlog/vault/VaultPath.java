package withlog.vault;

import withlog.text.Trim;

import java.nio.file.Files;
import java.nio.file.InvalidPathException;
import java.nio.file.Path;
import java.nio.file.Paths;

/**
 * vault 및 소켓 경로 정규화 및 검증 클래스.
 * 설명서 1.1 및 3.1에 명시된 경로 정리 규칙(NUL 검사, ., .. 정리, 절대 경로화, 폴더 여부 확인)을 수행합니다.
 */
public final class VaultPath {
    private VaultPath() {}

    /**
     * 경로 문자열을 3.1 규칙에 따라 정규화합니다.
     * 1. NUL 글자가 있으면 예외.
     * 2. 이어진 /를 1개로 줄이고 끝의 /를 뗀다 (뿌리 /는 그대로 유지).
     * 3. 상대 경로면 프로세스 작업 폴더(user.dir)를 앞에 붙여 절대 경로로 바꾼다.
     * 4. . 조각을 지운다.
     * 5. <이름>/.. 짝을 지운다 (맨 앞 ..은 뿌리 위로 갈 수 없어 지움).
     * 6. 심볼릭 링크는 안 풀고, 유니코드 정규화도 안 한다.
     *
     * @param rawPath 원본 경로 문자열
     * @return 정규화된 절대 경로 문자열
     */
    public static String normalize(String rawPath) {
        if (rawPath == null) {
            throw new IllegalArgumentException("경로가 null입니다.");
        }
        String trimmed = Trim.trim(rawPath);
        if (trimmed.isEmpty()) {
            throw new IllegalArgumentException("경로가 비어 있습니다.");
        }
        if (trimmed.indexOf('\0') != -1) {
            throw new IllegalArgumentException("경로에 NUL 문자가 포함되어 있다.");
        }

        try {
            Path path = Paths.get(trimmed);
            if (!path.isAbsolute()) {
                String userDir = System.getProperty("user.dir", ".");
                path = Paths.get(userDir).resolve(path);
            }
            Path normalized = path.normalize();
            return normalized.toString();
        } catch (InvalidPathException e) {
            throw new IllegalArgumentException("경로가 올바르지 않다: " + trimmed, e);
        }
    }

    /**
     * 입력받은 vault 경로 문자열을 정규화하고 유효한 폴더인지 검증합니다.
     *
     * @param rawVault 명령줄 또는 환경변수에서 받은 vault 경로 문자열
     * @return 정규화된 절대 경로 문자열
     * @throws IllegalArgumentException 경로가 비어있거나, NUL이 있거나, 디렉터리가 아닌 경우
     */
    public static String resolveAndValidate(String rawVault) {
        String normalized = normalize(rawVault);
        Path path = Paths.get(normalized);
        if (!Files.isDirectory(path)) {
            throw new IllegalArgumentException("vault 뿌리가 폴더가 아니다: " + normalized);
        }
        return normalized;
    }
}
