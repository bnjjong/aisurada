import aisurada.cli.AisException;
import aisurada.vault.Frontmatter;
import aisurada.write.FrontmatterWriter;
import aisurada.write.MetaWriter;

import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.PrintStream;
import java.math.BigInteger;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.stream.Stream;

/**
 * Rust 쪽 프론트매터 읽기·쓰기를 맞대 볼 기준값을 뽑는 도구다. 엔진 코드는 고치지 않고 지금 엔진의 공개 메서드를 그대로 부른다.
 *
 * <p>입력 YAML 조각마다 한 줄의 JSON을 stdout에 낸다: 읽은 결과({@code Frontmatter.parse})와 다시 쓴 결과({@code FrontmatterWriter.dumpGeneralMap}).
 * 값은 형을 잃지 않게 {@code {"$":"int","v":"10"}}처럼 꼬리표를 붙인다.
 *
 * <pre>
 * java -cp build/classes:lib/snakeyaml-2.4.jar engine-rs/tools/yaml-spike/YamlGroundTruth.java --cases 파일 --vault 폴더
 * </pre>
 */
public final class YamlGroundTruth {

    public static void main(String[] args) throws IOException {
        PrintStream out = new PrintStream(new FileOutputStream(FileDescriptor.out), false, StandardCharsets.UTF_8);
        List<String[]> cases = new ArrayList<>();
        for (int i = 0; i < args.length; i++) {
            switch (args[i]) {
                case "--vault" -> collectVault(Path.of(args[++i]), cases);
                case "--cases" -> collectCases(Path.of(args[++i]), cases);
                default -> throw new IllegalArgumentException("모르는 인자: " + args[i]);
            }
        }
        for (String[] c : cases) {
            emit(out, c[0], c[1]);
        }
        out.flush();
    }

    /** vault 폴더 아래 모든 .md, .html 파일에서 울타리 안 YAML 조각을 모은다. 엔진과 같은 {@code MetaWriter.split}을 쓴다. */
    private static void collectVault(Path root, List<String[]> cases) throws IOException {
        try (Stream<Path> walk = Files.walk(root)) {
            List<Path> files = walk.filter(Files::isRegularFile)
                    .filter(p -> p.toString().endsWith(".md") || p.toString().endsWith(".html"))
                    .sorted()
                    .toList();
            for (Path file : files) {
                String text;
                try {
                    text = Files.readString(file, StandardCharsets.UTF_8);
                } catch (IOException e) {
                    continue; // UTF-8이 아니면 노트가 아니다
                }
                MetaWriter.FileParts parts = MetaWriter.split(text);
                if (parts.fenceFound() && parts.yamlPart() != null) {
                    cases.add(new String[]{"vault:" + root.getFileName() + "/" + root.relativize(file), parts.yamlPart()});
                }
            }
        }
    }

    /** {@code #### 이름} 줄로 시작하는 구획마다 한 사례다. 구획의 글은 다음 {@code #### } 줄 앞까지다. */
    private static void collectCases(Path file, List<String[]> cases) throws IOException {
        String name = null;
        StringBuilder text = new StringBuilder();
        for (String line : Files.readAllLines(file, StandardCharsets.UTF_8)) {
            if (line.startsWith("#### ")) {
                if (name != null) {
                    cases.add(new String[]{"case:" + name, text.toString()});
                }
                name = line.substring(5).trim();
                text.setLength(0);
            } else if (name != null) {
                text.append(line).append('\n');
            }
        }
        if (name != null) {
            cases.add(new String[]{"case:" + name, text.toString()});
        }
    }

    @SuppressWarnings("unchecked")
    private static void emit(PrintStream out, String name, String yaml) {
        StringBuilder sb = new StringBuilder();
        sb.append("{\"name\":").append(js(name)).append(",\"input\":").append(js(yaml)).append(",\"parse\":");

        Map<String, Object> parsed = null;
        String parseError = null;
        try {
            parsed = Frontmatter.parse(yaml);
        } catch (RuntimeException e) {
            parseError = e.getClass().getSimpleName();
        }

        if (parseError != null) {
            sb.append("{\"kind\":\"error\",\"error\":").append(js(parseError)).append('}');
        } else if (parsed == null) {
            sb.append("{\"kind\":\"none\"}");
        } else {
            String encoded;
            try {
                encoded = enc(parsed, new java.util.IdentityHashMap<>());
            } catch (StackOverflowError e) {
                encoded = null;
            }
            if (encoded == null) {
                // 자기 자신을 가리키는 구조(재귀 앵커)라 값으로 적지 못한다
                sb.append("{\"kind\":\"cycle\"}");
            } else {
                sb.append("{\"kind\":\"map\",\"value\":").append(encoded).append('}');
                String dump;
                try {
                    dump = "{\"kind\":\"text\",\"text\":" + js(FrontmatterWriter.dumpGeneralMap(parsed)) + "}";
                } catch (AisException e) {
                    dump = "{\"kind\":\"error\"}";
                } catch (RuntimeException e) {
                    dump = "{\"kind\":\"error\",\"error\":" + js(e.getClass().getSimpleName()) + "}";
                }
                sb.append(",\"dump\":").append(dump);
            }
        }
        sb.append('}');
        out.println(sb);
    }

    /** 읽은 값을 꼬리표 JSON으로 적는다. 맵은 순서와 키의 형을 지키려고 [키, 값] 쌍의 목록으로 적는다. */
    private static String enc(Object v, java.util.IdentityHashMap<Object, Boolean> path) {
        if (v instanceof Map<?, ?> || v instanceof List<?>) {
            if (path.put(v, Boolean.TRUE) != null) {
                throw new StackOverflowError(); // 순환
            }
            try {
                return encContainer(v, path);
            } finally {
                path.remove(v);
            }
        }
        return encLeaf(v);
    }

    private static String encContainer(Object v, java.util.IdentityHashMap<Object, Boolean> path) {
        if (v instanceof Map<?, ?> m) {
            StringBuilder sb = new StringBuilder("{\"$\":\"map\",\"e\":[");
            boolean first = true;
            for (Map.Entry<?, ?> e : m.entrySet()) {
                if (!first) {
                    sb.append(',');
                }
                first = false;
                sb.append('[').append(enc(e.getKey(), path)).append(',').append(enc(e.getValue(), path)).append(']');
            }
            return sb.append("]}").toString();
        }
        List<?> l = (List<?>) v;
        StringBuilder sb = new StringBuilder("[");
        for (int i = 0; i < l.size(); i++) {
            if (i > 0) {
                sb.append(',');
            }
            sb.append(enc(l.get(i), path));
        }
        return sb.append(']').toString();
    }

    private static String encLeaf(Object v) {
        if (v == null) {
            return "null";
        }
        if (v instanceof Boolean b) {
            return b.toString();
        }
        if (v instanceof String s) {
            return js(s);
        }
        if (v instanceof Integer || v instanceof Long || v instanceof Short || v instanceof Byte || v instanceof BigInteger) {
            return "{\"$\":\"int\",\"v\":\"" + v + "\"}";
        }
        if (v instanceof Double || v instanceof Float) {
            long bits = Double.doubleToLongBits(((Number) v).doubleValue());
            return "{\"$\":\"float\",\"v\":\"" + Long.toHexString(bits) + "\"}";
        }
        if (v instanceof Set<?>) {
            return "{\"$\":\"other\",\"v\":\"set\"}";
        }
        return "{\"$\":\"other\",\"v\":" + js(v.getClass().getSimpleName()) + "}";
    }

    private static String js(String s) {
        StringBuilder sb = new StringBuilder(s.length() + 2).append('"');
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"' -> sb.append("\\\"");
                case '\\' -> sb.append("\\\\");
                case '\n' -> sb.append("\\n");
                case '\r' -> sb.append("\\r");
                case '\t' -> sb.append("\\t");
                default -> {
                    if (c < 0x20 || c == 0x7F || c == 0x2028 || c == 0x2029) {
                        sb.append(String.format("\\u%04x", (int) c));
                    } else {
                        sb.append(c);
                    }
                }
            }
        }
        return sb.append('"').toString();
    }
}
