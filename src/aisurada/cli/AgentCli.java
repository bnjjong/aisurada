package aisurada.cli;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;

/**
 * 에이전트 CLI(codex, agy, gemini)의 {@code mcp} 하위 명령을 돌리는 도구입니다. 설정 파일을 직접 고치지 않고 각 CLI가 정한
 * 등록 명령만 부르려는 것이라, 찾기와 실행을 한곳에 둡니다.
 */
final class AgentCli {

    static final long TIMEOUT_SECONDS = 40;

    private AgentCli() {}

    record Result(int status, String text) {}

    /**
     * 실행 파일을 찾습니다. 차례는 환경변수로 준 자리([envNames] 차례), 홈의 .local/bin, Homebrew, /usr/local/bin, PATH이고
     * 각 자리에서 [binNames] 차례로 봅니다. 없으면 null입니다.
     */
    static Path resolve(Map<String, String> env, List<String> envNames, List<String> binNames) {
        List<Path> candidates = new ArrayList<>();
        for (String envName : envNames) {
            String bin = env.get(envName);
            if (bin != null && !bin.isEmpty()) {
                candidates.add(Paths.get(bin));
            }
        }
        List<Path> dirs = new ArrayList<>();
        String home = env.get("HOME");
        if (home != null && !home.isEmpty()) {
            dirs.add(Paths.get(home, ".local/bin"));
        }
        dirs.add(Paths.get("/opt/homebrew/bin"));
        dirs.add(Paths.get("/usr/local/bin"));
        String path = env.get("PATH");
        if (path != null) {
            for (String dir : path.split(":")) {
                if (!dir.isEmpty()) {
                    dirs.add(Paths.get(dir));
                }
            }
        }
        for (Path dir : dirs) {
            for (String name : binNames) {
                candidates.add(dir.resolve(name));
            }
        }
        for (Path candidate : candidates) {
            if (Files.isRegularFile(candidate)) {
                return candidate;
            }
        }
        return null;
    }

    /**
     * CLI를 홈 폴더에서 돌려 종료 코드와 출력을 모읍니다. 프로젝트 설정(.mcp.json 따위)이 끼어들지 않게 하려는 것입니다.
     * [label]은 시간이 넘었을 때 알림 글에 쓰는 이름입니다.
     */
    static Result run(String label, Path bin, Map<String, String> env, List<String> args)
            throws IOException, InterruptedException, ExecutionException, TimeoutException {
        List<String> command = new ArrayList<>();
        command.add(bin.toString());
        command.addAll(args);
        ProcessBuilder builder = new ProcessBuilder(command).redirectErrorStream(true);
        String home = env.get("HOME");
        if (home != null && Files.isDirectory(Paths.get(home))) {
            builder.directory(Paths.get(home).toFile());
        }
        builder.environment().clear();
        builder.environment().putAll(env);
        Process process = builder.start();
        process.getOutputStream().close();
        CompletableFuture<String> output = CompletableFuture.supplyAsync(() -> {
            try {
                return new String(process.getInputStream().readAllBytes(), StandardCharsets.UTF_8);
            } catch (IOException e) {
                return "";
            }
        });
        if (!process.waitFor(TIMEOUT_SECONDS, TimeUnit.SECONDS)) {
            process.destroyForcibly();
            throw new TimeoutException(label + " " + String.join(" ", args.subList(0, Math.min(2, args.size()))) + " 가 " + TIMEOUT_SECONDS + "초 안에 안 끝났다");
        }
        return new Result(process.exitValue(), output.get(5, TimeUnit.SECONDS));
    }
}
