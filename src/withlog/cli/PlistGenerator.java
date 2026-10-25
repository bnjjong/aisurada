package withlog.cli;

import withlog.text.Trim;
import withlog.vault.VaultKey;
import withlog.vault.VaultPath;

import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;

/**
 * LaunchAgent plist 생성 및 해석 유틸리티. (설명서 3.4)
 */
public final class PlistGenerator {

    private PlistGenerator() {}

    public static String escapeXml(String s) {
        if (s == null) return "";
        return s.replace("&", "&amp;")
                .replace("<", "&lt;")
                .replace(">", "&gt;");
    }

    public static String unescapeXml(String s) {
        if (s == null) return "";
        return s.replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&amp;", "&");
    }

    /**
     * 실행할 사이드카 jar 경로를 결정합니다.
     * 환경변수 WITHLOG_SIDECAR_JAR -> 설치본 bin/wl-sidecar.jar
     */
    public static String resolveSidecarJar(String userHome) {
        String env = System.getenv("WITHLOG_SIDECAR_JAR");
        if (env != null) {
            String trimmed = Trim.trim(env);
            if (!trimmed.isEmpty()) {
                return trimmed;
            }
        }
        Path installed = Paths.get(userHome, "Library", "Application Support", "withlog", "bin", "wl-sidecar.jar");
        if (Files.isRegularFile(installed)) {
            return installed.toAbsolutePath().normalize().toString();
        }
        return null;
    }

    public static String resolveSidecarJava() {
        String env = System.getenv("WITHLOG_SIDECAR_JAVA");
        if (env != null) {
            String trimmed = Trim.trim(env);
            if (!trimmed.isEmpty()) {
                return trimmed;
            }
        }
        String javaHome = System.getProperty("java.home");
        return Paths.get(javaHome, "bin", "java").toAbsolutePath().normalize().toString();
    }

    public static String generatePlist(String label, String javaBin, String jarPath, String vaultPath, String socketPath, String logPath) {
        StringBuilder sb = new StringBuilder();
        sb.append("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        sb.append("<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n");
        sb.append("<plist version=\"1.0\">\n");
        sb.append("<dict>\n");
        sb.append("  <key>Label</key>\n");
        sb.append("  <string>").append(escapeXml(label)).append("</string>\n");
        sb.append("  <key>ProgramArguments</key>\n");
        sb.append("  <array>\n");
        sb.append("    <string>").append(escapeXml(javaBin)).append("</string>\n");
        sb.append("    <string>-jar</string>\n");
        sb.append("    <string>").append(escapeXml(jarPath)).append("</string>\n");
        sb.append("    <string>serve</string>\n");
        sb.append("  </array>\n");
        sb.append("  <key>EnvironmentVariables</key>\n");
        sb.append("  <dict>\n");
        sb.append("    <key>WITHLOG_VAULT</key>\n");
        sb.append("    <string>").append(escapeXml(vaultPath)).append("</string>\n");
        sb.append("    <key>WITHLOG_SIDECAR_SOCKET</key>\n");
        sb.append("    <string>").append(escapeXml(socketPath)).append("</string>\n");
        sb.append("  </dict>\n");
        sb.append("  <key>RunAtLoad</key>\n");
        sb.append("  <false/>\n");
        sb.append("  <key>ExitTimeOut</key>\n");
        sb.append("  <integer>25</integer>\n");
        sb.append("  <key>StandardOutPath</key>\n");
        sb.append("  <string>").append(escapeXml(logPath)).append("</string>\n");
        sb.append("  <key>StandardErrorPath</key>\n");
        sb.append("  <string>").append(escapeXml(logPath)).append("</string>\n");
        sb.append("</dict>\n");
        sb.append("</plist>\n");
        return sb.toString();
    }

    /**
     * 디스크의 plist 파일에서 -jar 다음의 jar 경로를 파싱합니다.
     */
    public static String parseJarFromPlist(Path plistFile) {
        if (!Files.isRegularFile(plistFile)) {
            return null;
        }
        try {
            java.util.List<String> lines = Files.readAllLines(plistFile, java.nio.charset.StandardCharsets.UTF_8);
            boolean nextIsJar = false;
            for (String raw : lines) {
                String line = raw.strip();
                if (line.startsWith("<string>") && line.endsWith("</string>") && line.length() >= 17) {
                    String val = line.substring(8, line.length() - 9);
                    val = unescapeXml(val);
                    if (nextIsJar) {
                        return val;
                    }
                    if ("-jar".equals(val)) {
                        nextIsJar = true;
                    }
                }
            }
        } catch (Exception ignored) {}
        return null;
    }

    /**
     * 디스크의 plist 파일에서 WITHLOG_VAULT 값을 파싱합니다.
     */
    public static String parseVaultFromPlist(Path plistFile) {
        if (!Files.isRegularFile(plistFile)) {
            return null;
        }
        try {
            java.util.List<String> lines = Files.readAllLines(plistFile, java.nio.charset.StandardCharsets.UTF_8);
            boolean keyFound = false;
            for (String raw : lines) {
                String line = raw.strip();
                if ("<key>WITHLOG_VAULT</key>".equals(line)) {
                    keyFound = true;
                    continue;
                }
                if (keyFound && !line.isBlank()) {
                    if (line.startsWith("<string>") && line.endsWith("</string>") && line.length() >= 17) {
                        String val = line.substring(8, line.length() - 9);
                        return unescapeXml(val);
                    } else {
                        return null;
                    }
                }
            }
        } catch (Exception ignored) {}
        return null;
    }
}
