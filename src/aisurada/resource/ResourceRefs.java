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

    /** 참조 목록을 너그럽게 읽는다. ID 모양이 아닌 줄은 건너뛰고, 겹친 ID는 처음 것만 둔다. 파일의 지금 모습 그대로이고 승인 여부는 보지 않는다. */
    public static List<Entry> read(Path vault, String project) {
        byte[] bytes = liveBytes(vault, project);
        return bytes.length == 0 ? new ArrayList<>() : parse(new String(bytes, StandardCharsets.UTF_8));
    }

    /** 참조 목록 글을 읽는다. */
    public static List<Entry> parse(String text) {
        List<Entry> out = new ArrayList<>();
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

    /** 파일의 지금 바이트. 파일이 없거나 못 읽으면 빈 배열이다. */
    public static byte[] liveBytes(Path vault, String project) {
        Path f = file(vault, project);
        try {
            return Files.isRegularFile(f) ? Files.readAllBytes(f) : new byte[0];
        } catch (IOException | RuntimeException e) {
            return new byte[0];
        }
    }

    /** 참조 목록 파일이 있는 프로젝트들의 slug(승인 기준선을 만들 때 쓴다). */
    public static List<String> projectsWithRefsFile(Path vault) {
        List<String> out = new ArrayList<>();
        Path projects = ResourceStore.root(vault).resolve("projects");
        if (!Files.isDirectory(projects)) return out;
        try (var ds = Files.newDirectoryStream(projects)) {
            for (Path p : ds) {
                String n = p.getFileName().toString();
                if (!n.startsWith(".") && Files.isDirectory(p) && Files.isRegularFile(p.resolve("refs.md"))) out.add(n);
            }
        } catch (IOException e) {
            return out;
        }
        java.util.Collections.sort(out);
        return out;
    }

    /** 참조 목록의 승인 상태. */
    public enum State {
        /** 승인한 판과 같다(또는 승인 기록도 파일도 없다). */
        APPROVED,
        /** 승인한 뒤에 파일이 바뀌었다. 승인해 둔 사본을 쓴다. */
        DIRTY,
        /** 승인 기록이 없는데 파일이 있다. 쓰지 않는다(빈 목록으로 본다). */
        UNTRUSTED
    }

    public static State state(Path vault, String project) {
        byte[] live = liveBytes(vault, project);
        Approvals.Entry e = Approvals.ledger(vault).items().get(Approvals.refsKey(project));
        if (e == null) return live.length == 0 ? State.APPROVED : State.UNTRUSTED;
        return aisurada.write.DocToken.compute(live).equals(e.token()) ? State.APPROVED : State.DIRTY;
    }

    /** 승인한 판의 참조 목록. 실행과 목록이 쓰는 것은 이쪽이다. 직접 고친 파일은 승인해 둔 사본으로 읽고, 기록 없는 새 파일은 비어 있는 것으로 본다. */
    public static List<Entry> readApproved(Path vault, String project) {
        return switch (state(vault, project)) {
            case APPROVED -> read(vault, project);
            case DIRTY -> {
                byte[] copy = Approvals.copy(vault, Approvals.refsKey(project));
                yield copy == null ? new ArrayList<>() : parse(new String(copy, StandardCharsets.UTF_8));
            }
            case UNTRUSTED -> new ArrayList<>();
        };
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
