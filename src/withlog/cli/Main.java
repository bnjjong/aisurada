package withlog.cli;

import withlog.cmd.CommandRegistry;
import withlog.config.EngineConfig;
import withlog.engine.EngineExecutor;
import withlog.engine.EngineResult;
import withlog.serve.ServeMode;
import withlog.text.Trim;

import java.io.IOException;
import java.nio.file.Path;
import java.util.Map;

/**
 * withlog 1부 엔진 단발 모드 및 serve 모드 실행 진입점.
 * 설명서 3.1의 엄격한 7단계 실행 차례를 준수합니다.
 */
public class Main {

    public static void main(String[] args) {
        long startTime = System.currentTimeMillis();

        // 1. vault 문턱 (3.1 1번)
        String envVault = System.getenv("WITHLOG_VAULT");
        boolean hasCliVaultPrefix = false;
        for (String arg : args) {
            if (arg.startsWith("--withlog.vault=")) {
                hasCliVaultPrefix = true;
                break;
            }
        }

        if ((envVault == null || Trim.isBlank(envVault)) && !hasCliVaultPrefix) {
            System.err.println("WITHLOG_VAULT 환경변수가 없다. vault 폴더의 절대 경로를 넣고 다시 실행한다.");
            System.err.println("설정으로 주려면 --withlog.vault=<경로> 를 붙인다.");
            System.exit(2);
        }

        // 2. 설정 칸 읽기 (3.1 2번)
        Map<String, String> cliOpts;
        try {
            cliOpts = EngineConfig.parseCliOptions(args);
        } catch (Exception e) {
            // 이름이 빈 칸(--=x) 등 설정 칸 구문 오류 -> stdout 0바이트, 종료 코드 1
            System.err.println("설정 칸 해석 실패: " + e.getMessage());
            System.exit(1);
            return;
        }

        // 3. 엔진 조립 및 설정 검증 (3.1 3번)
        EngineConfig config;
        try {
            config = EngineConfig.loadAndValidate(cliOpts);
        } catch (Exception e) {
            // 설정 값 타입 불일치 또는 vault 폴더 유효성 검증 실패 -> stdout 0바이트, 종료 코드 1
            System.err.println("엔진 조립 실패: " + e.getMessage());
            System.exit(1);
            return;
        }

        // 4. 첫 칸이 정확히 serve면 serve 모드로 진입 (3.1 4번, 3.2)
        if (args.length > 0 && "serve".equals(args[0])) {
            ServeMode.run(config.normalizedVault(), config);
            return;
        }

        // 5. 세션 표식 분리 (단발 모드에서는 해당 없음, EngineExecutor에서 처리)
        // 6. 명령 찾기 및 7. 실행 (3.1 6번, 7번)
        String firstArg = args.length > 0 ? args[0] : "";
        if (firstArg.isEmpty()) {
            System.err.print(CommandRegistry.buildUnknownCommandMessage("(없음)"));
            System.exit(2);
            return;
        }

        boolean isTool = CommandRegistry.TOOL_NAMES.contains(firstArg);
        boolean isCmd = !isTool && CommandRegistry.COMMAND_NAMES.contains(firstArg);

        if (!isTool && !isCmd) {
            System.err.print(CommandRegistry.buildUnknownCommandMessage(firstArg));
            System.exit(2);
            return;
        }

        // 명령 실행 (EngineExecutor)
        Path vaultPath = Path.of(config.normalizedVault());
        EngineResult result = EngineExecutor.execute(vaultPath, args, false, startTime);

        try {
            if (result.stdoutBytes() != null && result.stdoutBytes().length > 0) {
                System.out.write(result.stdoutBytes());
                System.out.flush();
            }
            if (result.stderrBytes() != null && result.stderrBytes().length > 0) {
                System.err.write(result.stderrBytes());
                System.err.flush();
            }
        } catch (IOException e) {
            System.exit(1);
        }

        System.exit(result.status());
    }
}
