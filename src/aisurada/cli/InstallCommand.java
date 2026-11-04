package aisurada.cli;

import aisurada.text.Trim;

import java.io.File;
import java.io.IOException;
import java.net.URI;
import java.nio.file.*;

/**
 * ais-cli install [--dry-run] [--cli <경로>] [--sidecar <경로>] 처리. (설명서 3.5)
 */
public final class InstallCommand {

    private InstallCommand() {}

    public static void run(String[] args) {
        boolean dryRun = false;
        String cliArg = null;
        String sidecarArg = null;

        for (int i = 0; i < args.length; i++) {
            String a = args[i];
            if ("--dry-run".equals(a)) {
                if (!dryRun) dryRun = true;
            } else if ("--cli".equals(a)) {
                if (cliArg == null) {
                    if (i + 1 >= args.length || Trim.trim(args[i + 1]).isEmpty() || Trim.trim(args[i + 1]).startsWith("--")) {
                        System.err.println("--cli 뒤에 경로가 없다.");
                        System.exit(2);
                        return;
                    }
                    cliArg = Trim.trim(args[i + 1]);
                    i++;
                }
            } else if ("--sidecar".equals(a)) {
                if (sidecarArg == null) {
                    if (i + 1 >= args.length || Trim.trim(args[i + 1]).isEmpty() || Trim.trim(args[i + 1]).startsWith("--")) {
                        System.err.println("--sidecar 뒤에 경로가 없다.");
                        System.exit(2);
                        return;
                    }
                    sidecarArg = Trim.trim(args[i + 1]);
                    i++;
                }
            }
        }

        String userDir = System.getProperty("user.dir", ".");
        String runningJarPath = getRunningJarPath();

        // 1. ais-cli.jar 원본 후보 결정
        String cliSource = null;
        if (cliArg != null) {
            cliSource = toAbsolutePath(cliArg, userDir);
        } else {
            String envCli = System.getenv("AISURADA_CLI_JAR");
            if (envCli != null && !Trim.isBlank(envCli)) {
                cliSource = toAbsolutePath(Trim.trim(envCli), userDir);
            } else if (runningJarPath != null && runningJarPath.matches(".*/ais-cli/build/libs/[^/]+\\.jar")) {
                cliSource = runningJarPath;
            } else {
                Path cand1 = Paths.get(userDir, "aisurada-backend", "ais-cli", "build", "libs", "ais-cli.jar");
                Path cand2 = Paths.get(userDir).getParent() != null ? Paths.get(userDir).getParent().resolve("aisurada-backend/ais-cli/build/libs/ais-cli.jar") : null;
                Path cand3 = Paths.get(userDir, "dist", "ais-cli.jar");

                if (Files.isRegularFile(cand1)) {
                    cliSource = cand1.toAbsolutePath().normalize().toString();
                } else if (cand2 != null && Files.isRegularFile(cand2)) {
                    cliSource = cand2.toAbsolutePath().normalize().toString();
                } else if (Files.isRegularFile(cand3)) {
                    cliSource = cand3.toAbsolutePath().normalize().toString();
                }
            }
        }

        if (cliSource == null) {
            System.err.println("원본 ais-cli.jar 을 못 찾았다. 저장소에서 `pnpm backend:jars` 를 돌린 뒤 그 자리에서 부르거나, --cli 로 절대 경로를 준다.");
            System.err.println("  지금 도는 jar: " + (runningJarPath != null ? runningJarPath : "(모름)"));
            System.err.println("  작업 폴더: " + Paths.get(userDir).toAbsolutePath().normalize());
            System.exit(2);
            return;
        }

        // 2. ais-sidecar.jar 원본 후보 결정
        String sidecarSource = null;
        if (sidecarArg != null) {
            sidecarSource = toAbsolutePath(sidecarArg, userDir);
        } else {
            String envSidecar = System.getenv("AISURADA_SIDECAR_JAR");
            if (envSidecar != null && !Trim.isBlank(envSidecar)) {
                sidecarSource = toAbsolutePath(Trim.trim(envSidecar), userDir);
            } else if (cliSource.matches(".*/ais-cli/build/libs/[^/]+\\.jar")) {
                int idx = cliSource.indexOf("/ais-cli/build/libs/");
                String root = cliSource.substring(0, idx);
                sidecarSource = root + "/ais-sidecar/build/libs/ais-sidecar.jar";
            } else if (cliSource.endsWith("/dist/ais-cli.jar")) {
                Path distSidecar = Paths.get(cliSource).getParent().resolve("ais-sidecar.jar");
                Path distEngine = Paths.get(cliSource).getParent().resolve("ais-engine.jar");
                if (Files.isRegularFile(distSidecar)) {
                    sidecarSource = distSidecar.toAbsolutePath().normalize().toString();
                } else {
                    sidecarSource = distEngine.toAbsolutePath().normalize().toString();
                }
            }
        }

        if (sidecarSource == null) {
            System.err.println("--sidecar 를 줘야 한다. " + cliSource + " 가 빌드 산출물 모양이 아니라 짝을 못 찾는다. 빌드 산출물 모양은 `<뿌리>/ais-cli/build/libs/*.jar` 다.");
            System.exit(2);
            return;
        }

        // 3. 두 원본이 일반 파일인지 확인 (ais-cli.jar 먼저)
        Path cliPath = Paths.get(cliSource);
        if (!Files.isRegularFile(cliPath)) {
            System.err.println("원본 jar 가 없다: " + cliPath.toAbsolutePath().normalize() + ". 먼저 빌드한다(`pnpm backend:jars`).");
            System.exit(2);
            return;
        }

        Path sidecarPath = Paths.get(sidecarSource);
        if (!Files.isRegularFile(sidecarPath)) {
            System.err.println("원본 jar 가 없다: " + sidecarPath.toAbsolutePath().normalize() + ". 먼저 빌드한다(`pnpm backend:jars`).");
            System.exit(2);
            return;
        }

        String userHome = System.getProperty("user.home");
        Path destDir = Paths.get(userHome, "Library", "Application Support", "aisurada", "bin");

        long cliSize;
        long sidecarSize;
        try {
            cliSize = Files.size(cliPath);
            sidecarSize = Files.size(sidecarPath);
        } catch (IOException e) {
            System.err.println("파일 크기를 읽지 못했다: " + e.getMessage());
            System.exit(1);
            return;
        }

        if (dryRun) {
            System.out.println("# (dry-run) 아무것도 안 바꿈 — 복사될 것 미리보기");
            System.out.println("# 자리: " + destDir);
            System.out.println("  ais-cli.jar  " + cliSize + " 바이트  ← " + cliPath);
            System.out.println("  ais-sidecar.jar  " + sidecarSize + " 바이트  ← " + sidecarPath);
            System.exit(0);
            return;
        }

        // 4. 설치 진행 (원자적 교체)
        try {
            Files.createDirectories(destDir);

            // ais-cli.jar 설치
            Path cliInstalling = destDir.resolve("ais-cli.jar.installing");
            Path cliTarget = destDir.resolve("ais-cli.jar");
            Files.copy(cliPath, cliInstalling, StandardCopyOption.REPLACE_EXISTING);
            try {
                Files.move(cliInstalling, cliTarget, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
            } catch (AtomicMoveNotSupportedException e) {
                Files.move(cliInstalling, cliTarget, StandardCopyOption.REPLACE_EXISTING);
            }
            try { Files.deleteIfExists(cliInstalling); } catch (Exception ignored) {}

            // ais-sidecar.jar 설치
            Path sidecarInstalling = destDir.resolve("ais-sidecar.jar.installing");
            Path sidecarTarget = destDir.resolve("ais-sidecar.jar");
            Files.copy(sidecarPath, sidecarInstalling, StandardCopyOption.REPLACE_EXISTING);
            try {
                Files.move(sidecarInstalling, sidecarTarget, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
            } catch (AtomicMoveNotSupportedException e) {
                Files.move(sidecarInstalling, sidecarTarget, StandardCopyOption.REPLACE_EXISTING);
            }
            try { Files.deleteIfExists(sidecarInstalling); } catch (Exception ignored) {}

            long installedCliSize = Files.size(cliTarget);
            long installedSidecarSize = Files.size(sidecarTarget);

            System.out.println("✓ jar 둘을 설치함");
            System.out.println("  자리: " + destDir);
            System.out.println("  ais-cli.jar  " + installedCliSize + " 바이트  ← " + cliPath);
            System.out.println("  ais-sidecar.jar  " + installedSidecarSize + " 바이트  ← " + sidecarPath);
            System.out.println("  래퍼와 plist 가 이 자리를 박게 하려면 `schedule on` 을 한 번 돌린다.");
            System.exit(0);
        } catch (Exception e) {
            e.printStackTrace(System.err);
            System.exit(1);
        }
    }

    private static String toAbsolutePath(String pathStr, String userDir) {
        Path p = Paths.get(pathStr);
        if (!p.isAbsolute()) {
            p = Paths.get(userDir).resolve(p);
        }
        return p.normalize().toString();
    }

    private static String getRunningJarPath() {
        try {
            URI uri = InstallCommand.class.getProtectionDomain().getCodeSource().getLocation().toURI();
            File file = new File(uri);
            if (file.isFile() && file.getName().endsWith(".jar")) {
                return file.getAbsolutePath();
            }
        } catch (Exception ignored) {}
        return null;
    }
}
