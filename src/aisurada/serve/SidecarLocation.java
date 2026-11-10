package aisurada.serve;

import aisurada.text.Trim;
import aisurada.vault.VaultKey;
import aisurada.vault.VaultPath;

import java.nio.charset.StandardCharsets;
import java.nio.file.Path;
import java.nio.file.Paths;

/**
 * 설명서 3.3 및 3.7 규격: 사이드카 소켓, 실행 상태, 잠금 파일 위치를 일원화하여 관리하는 레코드.
 */
public record SidecarLocation(
        String vaultRoot,
        String vaultKey,
        String hash16,
        Path socketPath,
        Path runDir,
        Path stateFile,
        Path lockFile
) {

    public static final int MAX_SOCKET_PATH_BYTES = 102;

    public static SidecarLocation resolve(String vaultRoot) {
        String userHome = System.getProperty("user.home", "");
        String vKey = VaultKey.compute(vaultRoot);
        String h16 = VaultKey.computeHash16(vaultRoot);

        String rawSocket = System.getenv("AISURADA_SIDECAR_SOCKET");
        if (rawSocket == null || Trim.isBlank(rawSocket)) {
            rawSocket = userHome + "/Library/Caches/aisurada/ais-" + h16 + ".sock";
        } else {
            rawSocket = Trim.trim(rawSocket);
        }
        Path sPath = Paths.get(VaultPath.normalize(rawSocket));

        Path rDir = Paths.get(userHome, "Library", "Application Support", "aisurada", "run");
        Path sFile = rDir.resolve(vKey + ".json");
        Path lFile = rDir.resolve(vKey + ".lock");

        return new SidecarLocation(vaultRoot, vKey, h16, sPath, rDir, sFile, lFile);
    }

    public boolean isSocketPathTooLong() {
        return socketPathBytes().length > MAX_SOCKET_PATH_BYTES;
    }

    public byte[] socketPathBytes() {
        return socketPath.toString().getBytes(StandardCharsets.UTF_8);
    }
}
