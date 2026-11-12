package aisurada.cli;

import java.net.URISyntaxException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * 에이전트에 MCP 서버로 등록할 값 하나치입니다. 이 명령을 돌린 Java와 {@code ais-cli.jar}, 엔진 jar를 모은 것입니다.
 *
 * <p>vault는 일부러 담지 않습니다. 등록에 경로를 박으면 손 확인이나 옛 설정의 vault가 그대로 남아 노트가 엉뚱한 곳에
 * 쓰이기 때문입니다. 서버가 호출마다 앱이 열어 둔 vault를 읽습니다({@link aisurada.vault.VaultPointer}).
 */
record McpRegistration(String java, String cliJar, String sidecarJar) {

    /** 등록에 쓸 값을 모읍니다. jar로 돌지 않은 ais-cli에서는 jar 자리를 몰라 실패합니다. */
    static McpRegistration current() {
        String cliJar = runningJar();
        if (cliJar == null) {
            throw new IllegalStateException("ais-cli.jar의 자리를 알 수 없다. jar로 돌린 ais-cli에서만 연결할 수 있다.");
        }
        return new McpRegistration(PlistGenerator.resolveSidecarJava(), cliJar, sidecarJarFor(cliJar));
    }

    /**
     * 엔진 jar의 자리. 환경변수 AISURADA_SIDECAR_JAR, 지금 ais-cli.jar 옆의 ais-sidecar.jar(앱 번들과 dist), 설치된 jar 차례입니다.
     * 옆의 jar를 먼저 보는 까닭은, 앱 번들에서 등록하면 번들 안의 엔진을 쓰게 하려는 것입니다.
     */
    static String sidecarJarFor(String cliJar) {
        String env = System.getenv("AISURADA_SIDECAR_JAR");
        if (env != null && !env.trim().isEmpty()) {
            return env.trim();
        }
        Path sibling = Paths.get(cliJar).resolveSibling("ais-sidecar.jar");
        if (Files.isRegularFile(sibling)) {
            return sibling.toAbsolutePath().normalize().toString();
        }
        return PlistGenerator.resolveSidecarJar(System.getProperty("user.home"));
    }

    /** MCP 서버를 띄우는 java 인자. */
    List<String> serverArgs(boolean write) {
        return write ? List.of("-jar", cliJar, "mcp", "--write") : List.of("-jar", cliJar, "mcp");
    }

    /** 서버 프로세스에 줄 환경변수. 엔진 jar를 알 때만 싣는다. */
    Map<String, String> env() {
        Map<String, String> env = new LinkedHashMap<>();
        if (sidecarJar != null) {
            env.put("AISURADA_SIDECAR_JAR", sidecarJar);
        }
        return env;
    }

    /** 지금 돌고 있는 ais-cli.jar의 절대 경로. jar로 돌지 않았으면 null이다. */
    static String runningJar() {
        try {
            Path location = Paths.get(McpRegistration.class.getProtectionDomain().getCodeSource().getLocation().toURI());
            return Files.isRegularFile(location) && location.toString().endsWith(".jar") ? location.toAbsolutePath().normalize().toString() : null;
        } catch (URISyntaxException | RuntimeException e) {
            return null;
        }
    }
}
