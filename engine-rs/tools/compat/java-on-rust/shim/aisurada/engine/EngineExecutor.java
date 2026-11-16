package aisurada.engine;

import aisurada.config.EngineConfig;

import java.io.ByteArrayOutputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.TimeZone;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/** 이슈 #81 대조용 바꿔치기(tools/compat/java-on-rust): Java 시험이 부르는 엔진 실행을 진짜 Rust ais-cli 프로세스 호출로 돌린다. */
public final class EngineExecutor {
    private static final Pattern CODE = Pattern.compile("^AIS-(\\d+)");

    private static String lastRandom;
    private static int cursor;

    public static boolean isPart4Enabled() { return true; }

    public static EngineResult execute(Path vaultRoot, String[] rawArgv, boolean serve, long start) {
        try {
            List<String> args = new ArrayList<>(List.of(rawArgv));
            String session = null, agent = null;
            int n = args.size();
            if (n >= 2 && "\u0000ais-session-agent".equals(args.get(n - 2))) {
                agent = args.get(n - 1);
                args = new ArrayList<>(args.subList(0, n - 2));
            }
            n = args.size();
            if (n >= 2 && "\u0000ais-session-id".equals(args.get(n - 2))) {
                session = args.get(n - 1);
                args = new ArrayList<>(args.subList(0, n - 2));
            }
            if (session == null) agent = null;
            List<String> cmd = new ArrayList<>();
            cmd.add(System.getProperty("shim.cli"));
            cmd.addAll(args);
            ProcessBuilder pb = new ProcessBuilder(cmd);
            Map<String, String> env = pb.environment();
            String path = env.get("PATH");
            env.clear();
            env.put("PATH", path == null ? "/usr/bin:/bin" : path);
            String approvalsProperty = System.getProperty("aisurada.approvals.dir");
            Path temp = approvalsProperty != null ? Path.of(approvalsProperty).getParent() : vaultRoot.toAbsolutePath().getParent().resolve(vaultRoot.getFileName() + "-shim");
            Path approvals = approvalsProperty != null ? Path.of(approvalsProperty) : temp.resolve("approvals");
            Files.createDirectories(temp);
            Path home = temp.resolve("home");
            Files.createDirectories(home);
            env.put("HOME", home.toString());
            env.put("AISURADA_VAULT", vaultRoot.toString());
            env.put("AISURADA_LOCK_DIR", temp.resolve("locks").toString());
            env.put("AISURADA_APPROVALS_DIR", approvals.toString());
            env.put("TZ", TimeZone.getDefault().getID());
            String now = System.getProperty("aisurada.test.now");
            if (now != null) env.put("AISURADA_TEST_NOW", now);
            String random = System.getProperty("aisurada.test.random");
            if (random != null) {
                if (!random.equals(lastRandom)) { lastRandom = random; cursor = 0; }
                String[] all = random.split(",");
                if (cursor < all.length) env.put("AISURADA_TEST_RANDOM", String.join(",", java.util.Arrays.copyOfRange(all, cursor, all.length)));
            } else { lastRandom = null; cursor = 0; }
            EngineConfig config = EngineConfig.active();
            if (config != null && config.shardByMonth() != null) env.put("AISURADA_NOTES_SHARD_BY_MONTH", config.shardByMonth().toString());
            if (config != null) {
                env.put("AISURADA_VAULT_AUTOCOMMIT_ENABLED", Boolean.toString(config.autocommitEnabled()));
                env.put("AISURADA_VAULT_AUTOCOMMIT_IDLE_SECONDS", Long.toString(config.autocommitIdleSeconds()));
            }
            for (String[] pair : new String[][] {{"aisurada.codex.home", "AISURADA_CODEX_HOME"}, {"aisurada.gemini.home", "AISURADA_GEMINI_HOME"}, {"aisurada.agy.home", "AISURADA_AGY_HOME"}, {"aisurada.claude.home", "AISURADA_CLAUDE_HOME"}}) {
                String value = System.getProperty(pair[0]);
                if (value != null) env.put(pair[1], value);
            }
            if (session != null) {
                env.put("AISURADA_SESSION_ID", session);
                env.put("AISURADA_SESSION_AGENT", agent == null ? "claude" : agent);
            }
            Process p = pb.start();
            p.getOutputStream().close();
            ByteArrayOutputStream err = new ByteArrayOutputStream();
            Thread t = new Thread(() -> { try { p.getErrorStream().transferTo(err); } catch (Exception ignore) { } });
            t.start();
            byte[] out = p.getInputStream().readAllBytes();
            int code = p.waitFor();
            t.join();
            byte[] errBytes = err.toByteArray();
            Matcher m = CODE.matcher(new String(errBytes, java.nio.charset.StandardCharsets.UTF_8));
            int errorCode = m.find() ? Integer.parseInt(m.group(1)) : 0;
            if (code == 0 && !args.isEmpty() && (args.get(0).equals("create-doc") || args.get(0).equals("create_doc"))) cursor++;
            return new EngineResult(code, errorCode, out, errBytes);
        } catch (Exception e) {
            throw new RuntimeException(e);
        }
    }
}
