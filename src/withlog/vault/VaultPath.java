package withlog.vault;

import withlog.text.Trim;

import java.io.File;
import java.nio.file.Files;
import java.nio.file.InvalidPathException;
import java.nio.file.Path;
import java.nio.file.Paths;

/**
 * vault 경로 정규화 및 검증 클래스.
 * 설명서 1.1 및 3.1에 명시된 경로 정리 규칙(NUL 검사, ., .. 정리, 절대 경로화, 폴더 여부 확인)을 수행합니다.
 */
public final class VaultPath {
    private VaultPath() {}

    /**
     * 입력받은 vault 경로 문자열을 정규화하고 유효한 폴더인지 검증합니다.
     * @param rawVault 명령줄 또는 환경변수에서 받은 vault 경로 문자열
     * @return 정규화된 절대 경로 문자열
     * @throws IllegalArgumentException 경로가 비어있거나, NUL이 있거나, 디렉터리가 아닌 경우
     */
    public static String resolveAndValidate(String rawVault) {
        if (rawVault == null) {
            throw new IllegalArgumentException("vault 경로가 없다. withlog.vault 설정이나 WITHLOG_VAULT 환경변수로 준다.");
        }
        String trimmed = Trim.trim(rawVault);
        if (trimmed.isEmpty()) {
            throw new IllegalArgumentException("vault 경로가 없다. withlog.vault 설정이나 WITHLOG_VAULT 환경변수로 준다.");
        }
        if (trimmed.indexOf('\0') != -1) {
            throw new IllegalArgumentException("vault 경로에 NUL 문자가 포함되어 있다.");
        }

        try {
            Path path = Paths.get(trimmed);
            if (!path.isAbsolute()) {
                String userDir = System.getProperty("user.dir", ".");
                path = Paths.get(userDir).resolve(path);
            }
            Path normalized = path.normalize();
            if (!Files.isDirectory(normalized)) {
                throw new IllegalArgumentException("vault 뿌리가 폴더가 아니다: " + normalized);
            }
            return normalized.toString();
        } catch (InvalidPathException e) {
            throw new IllegalArgumentException("vault 경로가 올바르지 않다: " + trimmed, e);
        }
    }
}
