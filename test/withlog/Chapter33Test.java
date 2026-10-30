package withlog;

import withlog.config.EngineConfig;
import withlog.vault.VaultKey;
import withlog.vault.VaultPath;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.TimeUnit;
import static withlog.ChapterTestSupport.*;

/** 격리 프로세스로 설정 출처와 값, vault 키와 실제 엔진 문턱을 검증한다. */
public final class Chapter33Test {
    public static void main(String[] args) throws Exception {
        if (args.length > 0 && args[0].equals("probe")) {
            if (Boolean.getBoolean("chapter33.cli")) {
                var method = withlog.cli.WlCliMain.class.getDeclaredMethod("getReadyTimeoutSeconds"); method.setAccessible(true);
                System.out.println(method.invoke(null) + "|" + withlog.cli.WlCliMain.getConcurrencyLimit() + "|" + withlog.cli.SweepCommand.getQuietHours());
                return;
            }
            var c = EngineConfig.loadAndValidate(EngineConfig.parseCliOptions(Arrays.copyOfRange(args, 1, args.length)));
            System.out.println(c.normalizedVault() + "|" + c.dbHome() + "|" + c.shardByMonth() + "|" + c.idleTimeoutMinutes() + "|" + c.autocommitEnabled() + "|" + c.autocommitIdleSeconds() + "|" + c.snapshotEnabled());
            return;
        }
        try (var t = new ChapterTestSupport("chapter33")) {
            Path v = t.vault(), home = Files.createDirectory(t.temp.resolve("home"));
            keys(); values(v, home); sources(v, home); cliValues(home); gate(v, home); t.done();
        }
    }
    private static void keys() {
        Map<String,String> examples = new LinkedHashMap<>();
        examples.put("/Users/me/vault", "17dc9196931802ce-vault"); examples.put("/Users/me/내 vault", "1d02e04897d42d35-vault");
        examples.put("/Users/me/노트", "19d21c43bfe07f90-vault"); examples.put("/Users/me/My.Vault (copy)", "a96d9f6b00188e4b-my-vault-copy");
        examples.put("/Users/me/İstanbul Notes", "db74ba7b0e906010-i-stanbul-notes"); examples.put("/Users/me/Kelvin", "8752c77aef4c9417-kelvin");
        examples.put("/", "8a5edab282632443-vault");
        for (var e : examples.entrySet()) eq(VaultKey.compute(e.getKey()), e.getValue(), "manual vault key " + e.getKey());
        check(!VaultKey.compute("/tmp/é").equals(VaultKey.compute("/tmp/e\u0301")), "no Unicode normalization");
        check(!VaultKey.compute("/tmp/vault").equals(VaultKey.compute("/private/tmp/vault")), "no symlink resolution");
        eq(VaultPath.normalize("/tmp/a/../vault/"), "/tmp/vault", "path normalization");
    }
    private record Reply(int status, String output) {}
    private static Reply child(Path home, Map<String,String> environment, List<String> properties, String... args) throws Exception {
        List<String> cmd = new ArrayList<>(List.of(Path.of(System.getProperty("java.home"), "bin/java").toString(), "-Duser.home=" + home)); cmd.addAll(properties);
        cmd.addAll(List.of("-cp", System.getProperty("java.class.path"), Chapter33Test.class.getName(), "probe")); cmd.addAll(List.of(args));
        ProcessBuilder b = new ProcessBuilder(cmd).redirectErrorStream(true); b.environment().keySet().removeIf(k -> k.startsWith("WITHLOG_") || k.startsWith("SPRING_")); b.environment().putAll(environment);
        Process p = b.start(); check(p.waitFor(10, TimeUnit.SECONDS), "config probe exits"); return new Reply(p.exitValue(), new String(p.getInputStream().readAllBytes(), StandardCharsets.UTF_8).trim());
    }
    private static String[] config(Path v, Path home, Map<String,String> env, List<String> props, String... options) throws Exception {
        Map<String,String> all = new HashMap<>(env); all.putIfAbsent("WITHLOG_VAULT", v.toString()); Reply r = child(home, all, props, options);
        eq(r.status(), 0, "config loads: " + r.output()); return r.output().split("\\|", -1);
    }
    private static void invalid(Path v, Path home, String key, String value) throws Exception {
        eq(child(home, Map.of("WITHLOG_VAULT", v.toString(), key, value), List.of()).status(), 1, "invalid " + key + "=" + value);
    }
    private static void values(Path v, Path home) throws Exception {
        String[] defaults = config(v, home, Map.of(), List.of());
        eq(defaults[1], home + "/Library/Application Support/withlog/db", "default db home uses user.home");
        eq(config(v, home, Map.of("HOME", v.toString()), List.of())[1], defaults[1], "HOME does not override JVM user.home");
        eq(Arrays.asList(defaults).subList(2, 7), List.of("null", "10", "true", "300", "false"), "defaults");
        for (String blank : List.of("", " ", "\u3000", "\u00a0")) eq(config(v, home, Map.of("WITHLOG_DB_HOME", blank), List.of())[1], defaults[1], "blank db path");
        eq(config(v, home, Map.of("WITHLOG_DB_HOME", " relative "), List.of())[1], " relative ", "nonblank db path kept exactly");
        for (String yes : List.of("true", "ON", "yes", "1", "\tTrUe\n")) eq(config(v, home, Map.of("WITHLOG_VAULT_SNAPSHOT_ENABLED", yes), List.of())[6], "true", "true spelling");
        for (String no : List.of("false", "OFF", "no", "0")) eq(config(v, home, Map.of("WITHLOG_VAULT_AUTOCOMMIT_ENABLED", no), List.of())[4], "false", "false spelling");
        for (String key : List.of("WITHLOG_VAULT_SNAPSHOT_ENABLED", "WITHLOG_VAULT_AUTOCOMMIT_ENABLED", "WITHLOG_IDLE_TIMEOUT_MINUTES", "WITHLOG_VAULT_AUTOCOMMIT_IDLE_SECONDS")) for (String value : List.of("", " ", "wrong")) invalid(v, home, key, value);
        eq(config(v, home, Map.of("WITHLOG_NOTES_SHARD_BY_MONTH", ""), List.of())[2], "null", "blank shard unset");
        invalid(v, home, "WITHLOG_NOTES_SHARD_BY_MONTH", "maybe");
        for (String[] pair : List.of(new String[]{"1 0", "10"}, new String[]{"#1F", "31"}, new String[]{"-0x10", "-16"}, new String[]{"-9223372036854775808", "-9223372036854775808"}, new String[]{"-0x8000000000000000", "-9223372036854775808"}, new String[]{"９", "9"}))
            eq(config(v, home, Map.of("WITHLOG_IDLE_TIMEOUT_MINUTES", pair[0]), List.of())[3], pair[1], "integer " + pair[0]);
        for (String bad : List.of("9223372036854775808", "-9223372036854775809", "+0x1", "1_0", "1.0", "1\u00a00")) invalid(v, home, "WITHLOG_IDLE_TIMEOUT_MINUTES", bad);
    }
    private static void sources(Path v, Path home) throws Exception {
        Map<String,String> env = Map.of("WITHLOG_IDLE_TIMEOUT_MINUTES", "17", "SPRING_PROFILES_ACTIVE", "dev");
        eq(config(v, home, env, List.of())[3], "17", "env overrides dev");
        eq(config(v, home, Map.of("SPRING_PROFILES_ACTIVE", "dev"), List.of())[3], "0", "dev profile default");
        eq(config(v, home, env, List.of("-Dwithlog.idle.timeout.minutes=18"))[3], "18", "system property overrides env");
        eq(config(v, home, env, List.of("-Dwithlog.idle.timeout.minutes=18"), "--withlog.idle.timeout.minutes=19")[3], "19", "CLI overrides system property");
        String json = "{\"withlog\":{\"idle\":{\"timeout\":{\"minutes\":21}}}}";
        env = Map.of("WITHLOG_IDLE_TIMEOUT_MINUTES", "17", "SPRING_APPLICATION_JSON", json);
        eq(config(v, home, env, List.of("-Dwithlog.idle.timeout.minutes=18"))[3], "21", "JSON overrides system property");
        eq(config(v, home, env, List.of(), "--withlog.idle.timeout.minutes=22")[3], "22", "CLI overrides JSON");
        eq(config(v, home, Map.of(), List.of(), "--spring.profiles.active=dev")[3], "0", "CLI dev profile");
        eq(config(v, home, Map.of(), List.of(), "--WITHLOG.IDLE.TIMEOUT.MINUTES=99")[3], "10", "only exact config names accepted");
        eq(child(home, Map.of("WITHLOG_VAULT", v.toString()), List.of(), "--withlog.vault.autocommit.enabled=true", "--withlog.vault.autocommit.enabled=false").status(), 1, "repeated values join then fail boolean");
        eq(EngineConfig.parseCliOptions(new String[]{"--withlog.db.home=one", "--withlog.db.home=two"}).get("withlog.db.home"), "one,two", "repeated explicit values join");
        eq(EngineConfig.parseCliOptions(new String[]{"--withlog.db.home", "--withlog.db.home=two", "--withlog.db.home"}).get("withlog.db.home"), "two", "flags without = add no value");
        eq(EngineConfig.parseCliOptions(new String[]{"--withlog.db.home", "--withlog.db.home"}).get("withlog.db.home"), "", "flags without = alone are empty");
        eq(EngineConfig.parseCliOptions(new String[]{"--withlog.db.home=", "--withlog.db.home=two"}).get("withlog.db.home"), ",two", "explicit empty value joins");
        eq(EngineConfig.parseCliOptions(new String[]{"--withlog.db.home=x", "--withlog.db.home="}).get("withlog.db.home"), "x,", "explicit empty value joins at the end");
        eq(config(v, home, Map.of(), List.of(), "--withlog.notes.shard.by.month", "--withlog.notes.shard.by.month=true")[2], "true", "flag then value keeps value");
        eq(config(v, home, env, List.of(), "--withlog.notes.shard.by.month=true", "--withlog.notes.shard.by.month")[2], "true", "value then flag keeps value");
        eq(config(v, home, Map.of(), List.of(), "--withlog.notes.shard.by.month", "--withlog.notes.shard.by.month")[2], "null", "flags without = alone leave shard unset");
    }
    private static void gate(Path v, Path home) throws Exception {
        List<String> cmd = List.of(Path.of(System.getProperty("java.home"), "bin/java").toString(), "-Duser.home=" + home, "-Dwithlog.vault=" + v, "-jar", Path.of("dist/wl-engine.jar").toAbsolutePath().toString(), "history", "missing");
        ProcessBuilder b = new ProcessBuilder(cmd).redirectErrorStream(true); b.environment().remove("WITHLOG_VAULT"); Process p = b.start();
        check(p.waitFor(10, TimeUnit.SECONDS), "engine gate exits"); eq(p.exitValue(), 2, "system property alone does not pass vault gate");
        List<String> withVault = new ArrayList<>(cmd); withVault.add("--withlog.vault=" + v); b = new ProcessBuilder(withVault).redirectErrorStream(true); b.environment().remove("WITHLOG_VAULT"); p = b.start();
        check(p.waitFor(10, TimeUnit.SECONDS), "engine explicit vault exits"); eq(p.exitValue(), 0, "exact CLI vault passes gate");
    }
    private static void cliValues(Path home) throws Exception {
        Reply r = child(home, Map.of("WITHLOG_READY_TIMEOUT_SECONDS", " +３０ ", "WITHLOG_SIDECAR_LIMIT", "５", "WITHLOG_SWEEP_QUIET_HOURS", "７"), List.of("-Dchapter33.cli=true"));
        eq(r.status(), 0, "CLI numeric values read"); eq(r.output(), "30|5|7", "CLI accepts signed Unicode digits");
        r = child(home, Map.of("WITHLOG_READY_TIMEOUT_SECONDS", "0", "WITHLOG_SIDECAR_LIMIT", "2147483648", "WITHLOG_SWEEP_QUIET_HOURS", "bad"), List.of("-Dchapter33.cli=true"));
        eq(r.status(), 0, "CLI invalid values fall back"); eq(r.output(), "30|3|24", "CLI numeric defaults");
    }
}
