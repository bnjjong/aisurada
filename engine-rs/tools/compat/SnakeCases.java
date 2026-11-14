import org.yaml.snakeyaml.DumperOptions;
import org.yaml.snakeyaml.Yaml;
import java.io.*;
import java.nio.charset.StandardCharsets;
import java.util.*;

/**
 * SnakeYAML 방출기 맞대기용 사례 파일을 쓴다. 정답지는 진짜 SnakeYAML 2.4의 {@code Yaml.dump}(태스크·자원 파일을 쓰는 옵션 그대로)다.
 * 줄마다 {@code 입력 맵 JSON <탭> 기대 YAML(JSON 글)}이고, 입력의 값은 글·수·참거짓·null·배열·객체다.
 *
 * <pre>
 * javac -cp lib/snakeyaml-2.4.jar -d /tmp/snake-cases engine-rs/tools/compat/SnakeCases.java
 * java -cp /tmp/snake-cases:lib/snakeyaml-2.4.jar SnakeCases /tmp/snake-cases.tsv 1 40000
 * (cd engine-rs && cargo run -p ais-core --example snake_check -- /tmp/snake-cases.tsv)
 * </pre>
 */
public class SnakeCases {
    static String snake(Map<String, Object> m) {
        DumperOptions options = new DumperOptions();
        options.setDefaultFlowStyle(DumperOptions.FlowStyle.BLOCK);
        options.setAllowUnicode(true);
        options.setSplitLines(false);
        options.setIndent(2);
        options.setIndicatorIndent(2);
        options.setIndentWithIndicator(true);
        return new Yaml(options).dump(m);
    }
    static String q(String s) {
        StringBuilder b = new StringBuilder("\"");
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"' -> b.append("\\\"");
                case '\\' -> b.append("\\\\");
                case '\n' -> b.append("\\n");
                case '\r' -> b.append("\\r");
                case '\t' -> b.append("\\t");
                default -> { if (c < 0x20 || (c >= 0xD800 && c <= 0xDFFF) && !(Character.isHighSurrogate(c) && i + 1 < s.length() && Character.isLowSurrogate(s.charAt(i + 1))) && !(Character.isLowSurrogate(c) && i > 0 && Character.isHighSurrogate(s.charAt(i - 1)))) b.append(String.format("\\u%04x", (int) c)); else b.append(c); }
            }
        }
        return b.append('"').toString();
    }
    static String json(Object o) {
        if (o == null) return "null";
        if (o instanceof String s) return q(s);
        if (o instanceof List<?> l) { StringJoiner j = new StringJoiner(",", "[", "]"); for (Object e : l) j.add(json(e)); return j.toString(); }
        if (o instanceof Map<?, ?> m) { StringJoiner j = new StringJoiner(",", "{", "}"); for (var e : m.entrySet()) j.add(q((String) e.getKey()) + ":" + json(e.getValue())); return j.toString(); }
        return String.valueOf(o);
    }
    public static void main(String[] args) throws Exception {
        long seed = Long.parseLong(args[1]);
        int count = Integer.parseInt(args[2]);
        Random rnd = new Random(seed);
        String alphabet = " -?:,[]{}#&*!|>'\"%@`\\\n\r\t\u0085   \u0000\u0007\u007f\u0080﻿￿é한😀.+=~<_0123456789abeEnNyYtTfFoOuUlLxX/;()$^";
        List<String> pool = new ArrayList<>(List.of("", " ", "null", "Null", "~", "true", "False", "yes", "No", "y", "n", "on", "OFF", "0", "007", "0o7", "0b1", "0x1f", "1_000", "1e3", "1.5", "-.5", ".inf", ".NaN", "12:30", "1:2:3", "2026-11-05", "2026-11-05T10:00:00Z", "2026-11-05 10:00:00 +9", "<<", "=", "!", "&", "*", "---", "...", "--- a", "a: b", "a :b", ":a", "? a", "- a", "-a", "a#b", "a #b", "key:", "value:", "'quoted'", "\"dq\"", "tab\t", "x".repeat(200), "word ".repeat(60).trim(), "a\nb", "a\nb\n", "\n", "\n\n", "a\n\nb", " a\nb", "a \nb", "a\n b", "한글", "😀", "🧑‍💻 a", "#", "# a", "a\u0085b", "a b", "﻿text"));
        try (PrintWriter out = new PrintWriter(new OutputStreamWriter(new FileOutputStream(args[0]), StandardCharsets.UTF_8))) {
            List<Object> values = new ArrayList<>();
            for (String s : pool) values.add(s);
            for (int i = 0; i < count; i++) {
                int len = rnd.nextInt(9);
                StringBuilder b = new StringBuilder();
                for (int k = 0; k < len; k++) {
                    int cp = alphabet.codePointAt(rnd.nextInt(alphabet.length() - 1));
                    b.appendCodePoint(cp);
                }
                values.add(b.toString());
            }
            values.addAll(List.of(5L, -3L, 0L, 12345678901234L, true, false, 1.5, 0.1, 1.0E10, 1.0E-5, 100.0, 3.14159, Double.NaN, Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY));
            for (Object v : values) {
                for (int shape = 0; shape < 6; shape++) {
                    Map<String, Object> m = new LinkedHashMap<>();
                    m.put("key", "TSK-1");
                    Object keyForm = v;
                    switch (shape) {
                        case 0 -> m.put("title", v);
                        case 1 -> m.put("labels", new ArrayList<>(Arrays.asList(v, "x")));
                        case 2 -> { Map<String, Object> r = new LinkedHashMap<>(); r.put("text", v); r.put("done", true); r.put("checkedAt", v); m.put("criteria", new ArrayList<>(List.of(r))); }
                        case 3 -> { Map<String, Object> inner = new LinkedHashMap<>(); inner.put("a", v); inner.put("b", new ArrayList<>(Arrays.asList(v))); inner.put("c", new ArrayList<>()); inner.put("d", new LinkedHashMap<>()); m.put("extra", inner); m.put("n", 5L); m.put("t", true); m.put("z", null); }
                        case 4 -> { if (v instanceof String s) m.put(s, "v"); else m.put(String.valueOf(v), "v"); }
                        default -> { List<Object> outer = new ArrayList<>(); outer.add(new ArrayList<>(Arrays.asList(v, "y"))); outer.add(v); m.put("nest", outer); }
                    }
                    String y;
                    try { y = snake(m); } catch (Exception e) { continue; }
                    out.println(json(m) + "\t" + q(y));
                }
            }
        }
    }
}
