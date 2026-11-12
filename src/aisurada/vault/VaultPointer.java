package aisurada.vault;

import aisurada.args.JsonCompact;
import aisurada.args.JsonReader;
import aisurada.args.JsonValue;
import aisurada.text.Trim;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.AtomicMoveNotSupportedException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.nio.file.StandardOpenOption;

/**
 * 앱이 지금 열어 둔 vault를 적어 둔 파일({@code ~/Library/Application Support/aisurada/current-vault.json})입니다.
 *
 * <p>MCP 서버와 {@code ais-cli}가 어느 vault를 쓸지 정하는 방법은 원래 {@code AISURADA_VAULT} 환경변수뿐이라, 에이전트에
 * MCP 서버를 등록할 때 경로를 박아 넣어야 했고 그 값이 낡아도 아무도 몰랐습니다. 이 파일은 앱이 vault를 열 때마다
 * 호스트가 고쳐 쓰고({@code workspace.rs}), 환경변수가 없을 때 이쪽이 호출마다 읽어서 앱을 따라갑니다.
 *
 * <p>환경변수가 있으면 언제나 그것이 이깁니다. 시험이나 격리된 손 확인은 가짜 HOME을 쓰므로 이 파일도 따로 갖고,
 * 진짜 앱이 적은 값을 건드리지 않습니다.
 */
public final class VaultPointer {

    public static final String FILE_NAME = "current-vault.json";

    /** 환경변수도 파일도 없을 때 쓰는 안내. 오류 코드 줄 앞에 AIS-400을 붙여서 낸다. */
    public static final String MISSING_MESSAGE =
            "vault를 정하지 못했다. AISURADA_VAULT 환경변수에 vault 폴더의 절대 경로를 넣거나, 앱을 열어 vault를 연다"
                    + "(앱이 연 vault를 ~/Library/Application Support/aisurada/" + FILE_NAME + " 에 적어 둔다).";

    private VaultPointer() {}

    /** 앱이 vault를 적는 파일의 자리. */
    public static Path file(String userHome) {
        return Path.of(userHome, "Library", "Application Support", "aisurada", FILE_NAME);
    }

    /**
     * 파일에 적힌 vault의 절대 경로를 돌려줍니다. 파일이 없거나, JSON이 아니거나, 절대 경로가 아니면 null입니다.
     * 폴더가 지금 있는지는 보지 않습니다. 그 검사는 vault를 쓰는 쪽(경로 정리, 엔진 시작)이 이미 합니다.
     */
    public static String read(String userHome) {
        if (userHome == null || userHome.isBlank()) return null;
        Path file = file(userHome);
        try {
            if (!Files.isRegularFile(file)) return null;
            JsonValue parsed = JsonReader.parse(Files.readString(file, StandardCharsets.UTF_8));
            if (parsed instanceof JsonValue.JsonObject object && object.properties().get("vault") instanceof JsonValue.JsonString vault) {
                String path = Trim.trim(vault.value());
                return path.startsWith("/") ? path : null;
            }
        } catch (IOException | RuntimeException ignored) {
            // 못 읽는 파일은 없는 것으로 본다. 호출한 쪽이 "vault를 못 정했다"고 알린다
        }
        return null;
    }

    /**
     * 이 호출이 쓸 vault. 환경변수 값이 비지 않았으면 그것을, 아니면 앱이 적어 둔 값을 돌려줍니다.
     * 둘 다 없으면 null입니다.
     */
    public static String resolve(String envVault, String userHome) {
        if (envVault != null && !Trim.isBlank(envVault)) return envVault;
        return read(userHome);
    }

    /**
     * 파일에 vault를 적습니다. 앱(호스트)이 같은 모양으로 쓰므로 이 메서드는 주로 시험과 복구용입니다.
     * 임시 파일에 쓰고 이름을 바꿔서, 읽는 쪽이 반쯤 쓴 파일을 보지 않게 합니다.
     */
    public static void write(String userHome, String vault) throws IOException {
        Path file = file(userHome);
        Files.createDirectories(file.getParent());
        StringBuilder json = new StringBuilder("{\"vault\":");
        JsonCompact.appendString(json, vault);
        json.append(",\"updatedAt\":").append(System.currentTimeMillis()).append("}\n");
        Path temp = file.resolveSibling(FILE_NAME + ".tmp");
        Files.writeString(temp, json, StandardCharsets.UTF_8, StandardOpenOption.CREATE, StandardOpenOption.TRUNCATE_EXISTING);
        try {
            Files.move(temp, file, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
        } catch (AtomicMoveNotSupportedException e) {
            Files.move(temp, file, StandardCopyOption.REPLACE_EXISTING);
        }
    }
}
