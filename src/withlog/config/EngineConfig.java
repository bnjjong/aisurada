package withlog.config;

import withlog.text.Trim;
import withlog.vault.VaultPath;
import withlog.args.JsonReader;
import withlog.args.JsonValue;

import java.util.*;

/**
 * 엔진 설정 키 7개 및 우선순위, 파싱 규칙을 처리하는 클래스.
 * 설명서 3.1 및 33.1 규격을 준수합니다.
 */
public final class EngineConfig {

    public static final String WITHLOG_VAULT = "withlog.vault";
    public static final String WITHLOG_DB_HOME = "withlog.db.home";
    public static final String WITHLOG_NOTES_SHARD_BY_MONTH = "withlog.notes.shard.by.month";
    public static final String WITHLOG_IDLE_TIMEOUT_MINUTES = "withlog.idle.timeout.minutes";
    public static final String WITHLOG_VAULT_AUTOCOMMIT_ENABLED = "withlog.vault.autocommit.enabled";
    public static final String WITHLOG_VAULT_AUTOCOMMIT_IDLE_SECONDS = "withlog.vault.autocommit.idle.seconds";
    public static final String WITHLOG_VAULT_SNAPSHOT_ENABLED = "withlog.vault.snapshot.enabled";

    private static final Set<String> VALID_CONFIG_KEYS = Set.of(
            WITHLOG_VAULT,
            WITHLOG_DB_HOME,
            WITHLOG_NOTES_SHARD_BY_MONTH,
            WITHLOG_IDLE_TIMEOUT_MINUTES,
            WITHLOG_VAULT_AUTOCOMMIT_ENABLED,
            WITHLOG_VAULT_AUTOCOMMIT_IDLE_SECONDS,
            WITHLOG_VAULT_SNAPSHOT_ENABLED
    );

    private static final Set<String> TRUTHY_BOOLEANS = Set.of("true", "on", "yes", "1");
    private static final Set<String> FALSY_BOOLEANS = Set.of("false", "off", "no", "0");

    private static volatile EngineConfig active;

    private final String rawVault;
    private final String normalizedVault;
    private final String dbHome;
    private final Boolean shardByMonth;
    private final long idleTimeoutMinutes;
    private final boolean autocommitEnabled;
    private final long autocommitIdleSeconds;
    private final boolean snapshotEnabled;

    public EngineConfig(
            String rawVault,
            String normalizedVault,
            String dbHome,
            Boolean shardByMonth,
            long idleTimeoutMinutes,
            boolean autocommitEnabled,
            long autocommitIdleSeconds,
            boolean snapshotEnabled
    ) {
        this.rawVault = rawVault;
        this.normalizedVault = normalizedVault;
        this.dbHome = dbHome;
        this.shardByMonth = shardByMonth;
        this.idleTimeoutMinutes = idleTimeoutMinutes;
        this.autocommitEnabled = autocommitEnabled;
        this.autocommitIdleSeconds = autocommitIdleSeconds;
        this.snapshotEnabled = snapshotEnabled;
    }

    public String rawVault() { return rawVault; }
    public String normalizedVault() { return normalizedVault; }
    public String dbHome() { return dbHome; }
    public Boolean shardByMonth() { return shardByMonth; }
    public long idleTimeoutMinutes() { return idleTimeoutMinutes; }
    public boolean autocommitEnabled() { return autocommitEnabled; }
    public long autocommitIdleSeconds() { return autocommitIdleSeconds; }
    public boolean snapshotEnabled() { return snapshotEnabled; }

    /** 엔진이 뜰 때 검증한 설정을 프로세스에 둡니다. 명령이 설정을 다시 읽으면 명령줄 칸을 놓치기 때문입니다(1.3, 33.1). */
    public static void activate(EngineConfig config) { active = config; }

    /** 뜰 때 검증한 설정입니다. 엔진 진입점을 안 거친 호출이면 null입니다. */
    public static EngineConfig active() { return active; }

    /**
     * CLI argv에서 설정 칸을 읽어옵니다.
     * 설명서 3.1: 이름이 빈 칸(--=x)이 있으면 엔진 시작 실패 (exit 1).
     * 같은 이름을 여러 번: 모든 값을 argv 차례대로 ,로 이은 글자 1개가 값.
     * = 없는 칸은 값을 보태지 않고 --이름= 은 빈 값 1개를 보탠다. = 없는 칸뿐이면 빈 글이다.
     */
    public static Map<String, String> parseCliOptions(String[] args) {
        Map<String, CliOptionEntry> map = new LinkedHashMap<>();
        boolean insideSettings = true;

        for (String arg : args) {
            if (insideSettings && "--".equals(arg)) {
                insideSettings = false;
                continue;
            }

            if (insideSettings && arg.startsWith("--")) {
                int eqIdx = arg.indexOf('=');
                String name;
                String val;
                boolean hasEquals;
                if (eqIdx != -1) {
                    name = arg.substring(2, eqIdx);
                    val = arg.substring(eqIdx + 1);
                    hasEquals = true;
                } else {
                    name = arg.substring(2);
                    val = "";
                    hasEquals = false;
                }

                if (name.isEmpty()) {
                    throw new IllegalArgumentException("Invalid argument syntax: name is empty in " + arg);
                }

                // 33.1: 다시 짓는 구현은 정확한 이름만 받는다. 모르는 이름은 조용히 버린다.
                if (!VALID_CONFIG_KEYS.contains(name) && !name.equals("spring.profiles.active") && !name.equals("spring.application.json")) {
                    continue;
                }

                CliOptionEntry entry = map.get(name);
                if (entry == null) {
                    entry = new CliOptionEntry();
                    map.put(name, entry);
                }
                entry.add(hasEquals, val);
            }
        }

        Map<String, String> result = new LinkedHashMap<>();
        for (Map.Entry<String, CliOptionEntry> e : map.entrySet()) {
            result.put(e.getKey(), e.getValue().getValue());
        }
        return result;
    }

    /** 3.1: 그래서 --a --a=x 는 x 이고 --a=x --a= 는 x, 이다. */
    private static class CliOptionEntry {
        boolean hasExplicitValue = false;
        String currentValue = "";

        void add(boolean hasEquals, String val) {
            if (!hasEquals) return;
            currentValue = hasExplicitValue ? currentValue + "," + val : val;
            hasExplicitValue = true;
        }

        String getValue() {
            return currentValue;
        }
    }

    /**
     * 33.1 우선순위에 따라 설정을 로드하고 엄격히 검증합니다.
     * 1. CLI --key=val
     * 2. JVM sysprop -Dkey=val
     * 3. OS env (WITHLOG_*)
     * 4. Default
     */
    public static EngineConfig loadAndValidate(Map<String, String> cliOpts) {
        // 1. withlog.vault
        String rawVault = resolveRawValue(WITHLOG_VAULT, "WITHLOG_VAULT", cliOpts, null);
        if (rawVault == null || Trim.trim(rawVault).isEmpty()) {
            throw new IllegalArgumentException("withlog.vault is required");
        }
        String normalizedVault = VaultPath.resolveAndValidate(rawVault);

        // 2. withlog.db.home
        String rawDbHome = resolveRawValue(WITHLOG_DB_HOME, "WITHLOG_DB_HOME", cliOpts, "");
        String dbHome;
        if (Trim.isBlank(rawDbHome)) {
            String userHome = System.getProperty("user.home", "");
            dbHome = userHome + "/Library/Application Support/withlog/db";
        } else {
            dbHome = rawDbHome;
        }

        // 3. withlog.notes.shard.by.month
        String rawShard = resolveRawValue(WITHLOG_NOTES_SHARD_BY_MONTH, "WITHLOG_NOTES_SHARD_BY_MONTH", cliOpts, null);
        Boolean shardByMonth = parseShardByMonth(rawShard);

        // 4. withlog.idle.timeout.minutes
        String profile = cliOpts.getOrDefault("spring.profiles.active", System.getProperty("spring.profiles.active", System.getenv().getOrDefault("SPRING_PROFILES_ACTIVE", "")));
        boolean dev = Arrays.stream(profile.split(",")).anyMatch(p -> Trim.trim(p).equals("dev"));
        String rawIdle = resolveRawValue(WITHLOG_IDLE_TIMEOUT_MINUTES, "WITHLOG_IDLE_TIMEOUT_MINUTES", cliOpts, dev ? "0" : "10");
        long idleTimeoutMinutes = parseIntegerConfig(WITHLOG_IDLE_TIMEOUT_MINUTES, rawIdle);

        // 5. withlog.vault.autocommit.enabled
        String rawAutocommit = resolveRawValue(WITHLOG_VAULT_AUTOCOMMIT_ENABLED, "WITHLOG_VAULT_AUTOCOMMIT_ENABLED", cliOpts, "true");
        boolean autocommitEnabled = parseStrictBoolean(WITHLOG_VAULT_AUTOCOMMIT_ENABLED, rawAutocommit);

        // 6. withlog.vault.autocommit.idle.seconds
        String rawAutocommitIdle = resolveRawValue(WITHLOG_VAULT_AUTOCOMMIT_IDLE_SECONDS, "WITHLOG_VAULT_AUTOCOMMIT_IDLE_SECONDS", cliOpts, "300");
        long autocommitIdleSeconds = parseIntegerConfig(WITHLOG_VAULT_AUTOCOMMIT_IDLE_SECONDS, rawAutocommitIdle);

        // 7. withlog.vault.snapshot.enabled
        String rawSnapshot = resolveRawValue(WITHLOG_VAULT_SNAPSHOT_ENABLED, "WITHLOG_VAULT_SNAPSHOT_ENABLED", cliOpts, "false");
        boolean snapshotEnabled = parseStrictBoolean(WITHLOG_VAULT_SNAPSHOT_ENABLED, rawSnapshot);

        return new EngineConfig(
                rawVault,
                normalizedVault,
                dbHome,
                shardByMonth,
                idleTimeoutMinutes,
                autocommitEnabled,
                autocommitIdleSeconds,
                snapshotEnabled
        );
    }

    private static String resolveRawValue(String key, String envKey, Map<String, String> cliOpts, String defaultVal) {
        if (cliOpts.containsKey(key)) {
            return cliOpts.get(key);
        }
        String json = cliOpts.getOrDefault("spring.application.json", System.getenv("SPRING_APPLICATION_JSON"));
        if (json != null && !json.isEmpty()) {
            JsonValue root = JsonReader.parse(json);
            if (!(root instanceof JsonValue.JsonObject object)) throw new IllegalArgumentException("spring.application.json must be an object");
            JsonValue value = object.properties().get(key);
            if (value == null) {
                value = object;
                for (String part : key.split("\\.")) {
                    if (!(value instanceof JsonValue.JsonObject nested)) { value = null; break; }
                    value = nested.properties().get(part);
                }
            }
            if (value instanceof JsonValue.JsonString s) return s.value();
            if (value instanceof JsonValue.JsonLiteral literal && !literal.isNull()) return literal.raw();
        }
        String sysProp = System.getProperty(key);
        if (sysProp != null) {
            return sysProp;
        }
        String envVal = System.getenv(envKey);
        if (envVal != null) {
            return envVal;
        }
        return defaultVal;
    }

    private static String trimAscii(String s) {
        if (s == null) return "";
        int start = 0;
        while (start < s.length() && s.charAt(start) <= ' ') start++;
        int end = s.length();
        while (end > start && s.charAt(end - 1) <= ' ') end--;
        return s.substring(start, end);
    }

    private static Boolean parseShardByMonth(String rawVal) {
        if (rawVal == null) return null;
        String trimmed = trimAscii(rawVal);
        if (trimmed.isEmpty()) return null;
        String lower = trimmed.toLowerCase(Locale.ROOT);
        if (TRUTHY_BOOLEANS.contains(lower)) return true;
        if (FALSY_BOOLEANS.contains(lower)) return false;
        throw new IllegalArgumentException("Invalid boolean value for " + WITHLOG_NOTES_SHARD_BY_MONTH + ": " + rawVal);
    }

    private static boolean parseStrictBoolean(String key, String rawVal) {
        if (rawVal == null) {
            throw new IllegalArgumentException("Empty boolean value for " + key);
        }
        String trimmed = trimAscii(rawVal);
        if (trimmed.isEmpty()) {
            throw new IllegalArgumentException("Empty boolean value for " + key);
        }
        String lower = trimmed.toLowerCase(Locale.ROOT);
        if (TRUTHY_BOOLEANS.contains(lower)) return true;
        if (FALSY_BOOLEANS.contains(lower)) return false;
        throw new IllegalArgumentException("Invalid boolean value for " + key + ": " + rawVal);
    }

    private static boolean isTrimWhitespace(char c) {
        if ((c >= 0x0009 && c <= 0x000D) || (c >= 0x001C && c <= 0x001F)) {
            return true;
        }
        int type = Character.getType(c);
        return type == Character.SPACE_SEPARATOR ||
                type == Character.LINE_SEPARATOR ||
                type == Character.PARAGRAPH_SEPARATOR;
    }

    private static long parseCustomLong(String s) {
        boolean negative = false;
        String rem = s;
        if (rem.startsWith("-")) {
            negative = true;
            rem = rem.substring(1);
        } else if (rem.startsWith("+")) {
            rem = rem.substring(1);
            if (rem.startsWith("0x") || rem.startsWith("0X") || rem.startsWith("#")) {
                throw new NumberFormatException("+0x not allowed");
            }
        }
        int radix = 10;
        if (rem.startsWith("0x") || rem.startsWith("0X")) {
            radix = 16;
            rem = rem.substring(2);
        } else if (rem.startsWith("#")) {
            radix = 16;
            rem = rem.substring(1);
        }
        if (rem.isEmpty()) throw new NumberFormatException("empty digits");

        StringBuilder normalized = new StringBuilder();
        for (int i = 0; i < rem.length(); i++) {
            char ch = rem.charAt(i);
            int d = Character.digit(ch, radix);
            if (d < 0) throw new NumberFormatException("invalid digit: " + ch);
            normalized.append(Integer.toString(d, radix));
        }
        return Long.parseLong((negative ? "-" : "") + normalized, radix);
    }

    private static long parseIntegerConfig(String key, String rawVal) {
        if (rawVal == null || rawVal.isEmpty()) {
            throw new IllegalArgumentException("Empty integer value for " + key);
        }
        StringBuilder sb = new StringBuilder();
        for (int i = 0; i < rawVal.length(); i++) {
            char ch = rawVal.charAt(i);
            if (ch == 0x00A0 || ch == 0x2007 || ch == 0x202F) {
                sb.append(ch);
            } else if (isTrimWhitespace(ch)) {
                // remove whitespace
            } else {
                sb.append(ch);
            }
        }
        String s = sb.toString();
        if (s.isEmpty()) {
            throw new IllegalArgumentException("Empty integer value for " + key);
        }
        try {
            return parseCustomLong(s);
        } catch (Exception e) {
            throw new IllegalArgumentException("Invalid integer value for " + key + ": " + rawVal, e);
        }
    }
}
