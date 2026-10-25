package withlog.cli;

import java.io.IOException;
import java.io.InputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.concurrent.TimeUnit;

/**
 * launchctl 및 id 명령어 호출을 위한 래퍼 클래스. (설명서 3.3)
 * 10초 타임아웃, 프로세스 속성 기반 uid 탐색, 시스템 속성을 통한 테스트 오버라이드를 지원합니다.
 */
public final class Launchctl {

    private Launchctl() {}

    public static final class Result {
        private final int exitCode;
        private final String output;
        private final boolean timedOut;
        private final String error;

        public Result(int exitCode, String output, boolean timedOut, String error) {
            this.exitCode = exitCode;
            this.output = output != null ? output : "";
            this.timedOut = timedOut;
            this.error = error;
        }

        public int exitCode() { return exitCode; }
        public String output() { return output; }
        public boolean timedOut() { return timedOut; }
        public String error() { return error; }
        public boolean isSuccess() { return error == null && exitCode == 0; }
    }

    /**
     * user.home 폴더 소유자의 uid를 구합니다.
     */
    public static String getUid() throws LaunchctlException {
        String userHome = System.getProperty("user.home");
        // 1. 파일 속성에서 unix:uid 읽기 시도
        try {
            Path homePath = Paths.get(userHome).toRealPath();
            Object uidAttr = Files.getAttribute(homePath, "unix:uid");
            if (uidAttr != null) {
                String uid = uidAttr.toString().trim();
                if (!uid.isEmpty()) {
                    return uid;
                }
            }
        } catch (Exception ignored) {}

        // 2. id -u 실행
        String idCmd = System.getProperty("withlog.id.cmd", "id");
        Result r = runProcess(idCmd, List.of(idCmd, "-u"));
        if (r.error == null && r.exitCode == 0 && !r.output.trim().isEmpty()) {
            return r.output.trim();
        }

        throw new LaunchctlException("uid 를 못 읽었다. 홈 폴더 속성도 `id -u` 도 답을 안 줬다.");
    }

    /**
     * launchctl 명령어를 실행합니다.
     */
    public static Result runLaunchctl(List<String> subArgs) {
        String launchctlCmd = System.getProperty("withlog.launchctl.cmd", "/bin/launchctl");
        List<String> fullCmd = new ArrayList<>();
        fullCmd.add(launchctlCmd);
        fullCmd.addAll(subArgs);
        return runProcess(launchctlCmd, fullCmd);
    }

    private static Result runProcess(String programName, List<String> cmdList) {
        ProcessBuilder pb = new ProcessBuilder(cmdList);
        pb.redirectErrorStream(true);

        Process p;
        try {
            p = pb.start();
        } catch (IOException e) {
            return new Result(-1, "", false, programName + " 를 못 돌렸다: " + e.getMessage());
        }

        boolean finished;
        try {
            finished = p.waitFor(10, TimeUnit.SECONDS);
        } catch (InterruptedException e) {
            p.destroyForcibly();
            return new Result(-1, "", false, programName + " 를 기다리다 끊겼다");
        }

        if (!finished) {
            p.destroyForcibly();
            String output = readAll(p.getInputStream());
            String joined = String.join(", ", cmdList);
            return new Result(-1, output, true, programName + " 가 10 초 안에 안 끝났다: [" + joined + "]");
        }

        String output = readAll(p.getInputStream());
        return new Result(p.exitValue(), output, false, null);
    }

    private static String readAll(InputStream in) {
        try {
            return new String(in.readAllBytes(), StandardCharsets.UTF_8);
        } catch (Exception e) {
            return "";
        }
    }

    public static class LaunchctlException extends Exception {
        public LaunchctlException(String message) {
            super(message);
        }
    }
}
