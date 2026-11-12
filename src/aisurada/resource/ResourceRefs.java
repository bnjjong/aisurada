package aisurada.resource;

import aisurada.write.AtomicWriter;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * 프로젝트의 참조 목록(켜 둔 글로벌 자원). <b>외부 설명서에 없는 확장</b>입니다.
 *
 * <p>{@code .aisurada/resources/projects/<slug>/refs.md}에 줄마다 {@code <ID>  # <이름 힌트>}로 적습니다. 엔진은 ID만 읽고 힌트는
 * 읽는 사람을 위한 것입니다(이름이 바뀌어도 ID가 같으면 같은 자원이다). 프로젝트 파일에 적지 않는 까닭은, 설명서가 프로젝트 파일의 줄을 정해
 * 두었고 그 파일을 모르는 옛 빌드가 다시 쓸 때 모르는 줄을 버리기 때문입니다.
 */
public final class ResourceRefs {

    /** 목록 한 줄. {@code hint}는 쓴 때의 이름이다. */
    public record Entry(String id, String hint) {}

    private static final Pattern LINE = Pattern.compile("^(\\S+)(?:\\s+#\\s*(.*))?$");
    private static final String HEADER = "# 이 프로젝트가 쓰려고 참조한 글로벌 자원. 줄마다 `ID  # 이름 힌트`. 엔진은 ID만 읽는다.\n";

    private ResourceRefs() {}

    public static Path file(Path vault, String project) {
        return ResourceStore.scopeDir(vault, project).resolve("refs.md");
    }

    /** 참조 목록을 너그럽게 읽는다. ID 모양이 아닌 줄은 건너뛰고, 겹친 ID는 처음 것만 둔다. */
    public static List<Entry> read(Path vault, String project) {
        List<Entry> out = new ArrayList<>();
        Path f = file(vault, project);
        if (!Files.isRegularFile(f)) return out;
        String text;
        try {
            text = Files.readString(f, StandardCharsets.UTF_8);
        } catch (IOException | RuntimeException e) {
            return out;
        }
        for (String raw : text.split("\n")) {
            String line = raw.trim();
            if (line.isEmpty() || line.startsWith("#")) continue;
            Matcher m = LINE.matcher(line);
            if (!m.matches() || !ResourceKind.isId(m.group(1))) continue;
            String id = m.group(1);
            if (out.stream().anyMatch(e -> e.id().equals(id))) continue;
            out.add(new Entry(id, m.group(2) == null ? "" : m.group(2).trim()));
        }
        return out;
    }

    /** 참조 목록을 쓴다. 비면 파일을 지운다. */
    public static void write(Path vault, String project, List<Entry> entries) {
        Path f = file(vault, project);
        if (entries.isEmpty()) {
            try {
                Files.deleteIfExists(f);
            } catch (IOException e) {
                throw new aisurada.cli.AisException(500, "참조 목록을 지우지 못했다: " + ResourceFormat.rel(vault, f));
            }
            return;
        }
        StringBuilder sb = new StringBuilder(HEADER);
        for (Entry e : entries) {
            sb.append(e.id());
            String hint = e.hint() == null ? "" : e.hint().replace('\n', ' ').replace('\r', ' ').trim();
            if (!hint.isEmpty()) sb.append("  # ").append(hint);
            sb.append('\n');
        }
        AtomicWriter.writeAtomic(f, sb.toString().getBytes(StandardCharsets.UTF_8));
    }

    /** 이 ID를 참조한 프로젝트들의 slug. */
    public static List<String> projectsReferencing(Path vault, String id) {
        List<String> out = new ArrayList<>();
        Path projects = ResourceStore.root(vault).resolve("projects");
        if (!Files.isDirectory(projects)) return out;
        List<String> names = new ArrayList<>();
        try (var ds = Files.newDirectoryStream(projects)) {
            for (Path p : ds) {
                String n = p.getFileName().toString();
                if (!n.startsWith(".") && Files.isDirectory(p)) names.add(n);
            }
        } catch (IOException e) {
            return out;
        }
        java.util.Collections.sort(names);
        for (String n : names) {
            if (read(vault, n).stream().anyMatch(e -> e.id().equals(id))) out.add(n);
        }
        return out;
    }
}
