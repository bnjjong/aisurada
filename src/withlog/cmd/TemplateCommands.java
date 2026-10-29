package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.engine.WritePipeline;
import withlog.text.Trim;
import withlog.vault.TemplateReader;
import withlog.write.AtomicWriter;
import withlog.write.IdGenerator;
import withlog.write.TemplateFile;
import withlog.write.WarningsDetector;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;
import java.util.regex.Pattern;

/** 24장 템플릿 쓰기. 찾기는 언제나 doc 파일 이름 순서, 다음 ritual 파일 이름 순서다. */
public final class TemplateCommands {
    public static final String SAVE_USAGE = "save-template [id=<고칠 템플릿 id. 없으면 새로 만든다>] [name=<이름. 새로 만들 때 필수>] [type=<plan|build|report|research|meeting|free. 새로 만들 때 기본 plan>] [desc=<언제 쓰는 템플릿인지>] [body=<본문. 새로 만들 때 필수>]";
    public static final String DELETE_USAGE = "delete-doc-template <문서 템플릿 id>";
    public static final String RITUAL_USAGE = "save-ritual-template <리추얼 템플릿 id> [name=<이름>] [desc=<언제 쓰는 양식인지>] [body=<본문>]";
    private static final Pattern ID = Pattern.compile("[A-Za-z0-9._-]{1,128}");
    private static final Set<String> TYPES = Set.of("plan", "build", "report", "research", "meeting", "free");
    private TemplateCommands() {}

    public static Map<String, Object> execute(Path vault, ParsedArgs args) {
        WritePipeline.index(vault);
        return switch (args.commandName()) {
            case "save-template" -> saveDocument(vault, args.namedOptions());
            case "delete-doc-template" -> delete(vault, positionalId(args, DELETE_USAGE));
            case "save-ritual-template" -> saveRitual(vault, positionalId(args, RITUAL_USAGE), args.namedOptions());
            default -> throw new IllegalArgumentException("Unknown template write: " + args.commandName());
        };
    }

    private static Map<String, Object> saveDocument(Path vault, Map<String, String> options) {
        String id = options.get("id");
        boolean create = id == null || id.isEmpty();
        String name, type, desc = options.getOrDefault("desc", ""), body = options.get("body");
        boolean builtin;
        if (create) {
            // 만들기는 훑지 않는다. 템플릿 파일 1개의 치명적 YAML 오류도 새 만들기를 막지 않는다.
            if (Trim.isBlank(body)) throw new WlException(400, "새 템플릿은 body 이 필요하다: " + SAVE_USAGE);
            name = Trim.trim(options.get("name"));
            if (name.isEmpty()) throw new WlException(400, "새 템플릿은 name 이 필요하다: " + SAVE_USAGE);
            type = type(options.get("type")); if (type == null) type = "plan";
            id = IdGenerator.nextId("tpl-"); builtin = false;
        } else {
            validateId(id); validateName(options); type = type(options.get("type"));
            var existing = find(vault, id);
            if (existing == null || !existing.category().equals("doc")) throw notFound(id);
            name = options.getOrDefault("name", existing.name());
            if (type == null) type = existing.type();
            desc = options.getOrDefault("desc", existing.desc()); body = options.getOrDefault("body", existing.body()); builtin = existing.builtin();
            String path = TemplateFile.docPath(id);
            AtomicWriter.writeAtomic(vault.resolve(path), TemplateFile.document(id, name, type, desc, builtin, body));
            if (!existing.format().equals("markdown")) remove(vault, ".withlog/templates/doc/" + id + "." + existing.format());
            return saved(id, "updated", builtin, path, options.get("body"));
        }
        String path = TemplateFile.docPath(id);
        AtomicWriter.writeAtomic(vault.resolve(path), TemplateFile.document(id, name, type, desc, builtin, body));
        return saved(id, "created", builtin, path, options.get("body"));
    }

    private static Map<String, Object> saveRitual(Path vault, String id, Map<String, String> options) {
        validateName(options);
        var existing = find(vault, id);
        if (existing == null || !existing.category().equals("ritual")) throw notFound(id);
        long sameRev = TemplateReader.scan(vault).templates().stream().filter(t -> t.category().equals("ritual") && t.rev().equals(existing.rev())).count();
        if (sameRev != 1) throw new WlException(400, "같은 rev 를 든 리추얼 템플릿이 " + sameRev + "건이라 저장하지 않는다. 파일 이름이 rev 라서 서로를 덮는다: rev=" + existing.rev());
        String path = TemplateFile.ritualPath(existing.rev());
        AtomicWriter.writeAtomic(vault.resolve(path), TemplateFile.ritual(id, existing.rev(), options.getOrDefault("name", existing.name()), existing.icon(), existing.format(), options.getOrDefault("desc", existing.desc()), options.getOrDefault("body", existing.body())));
        return saved(id, "updated", false, path, options.get("body"));
    }

    private static Map<String, Object> delete(Path vault, String id) {
        var existing = find(vault, id);
        List<String> removed = new ArrayList<>();
        if (existing != null) {
            if (existing.category().equals("ritual")) throw new WlException(400, "리추얼 템플릿은 지울 수 없다. 지우는 길이 앱에도 없다: " + id);
            if (existing.builtin()) throw new WlException(400, "빌트인 템플릿은 지울 수 없다. 고친 것을 되돌리려면 기본값으로 복원한다: " + id);
            for (String extension : List.of("md", "html", "json")) {
                String path = ".withlog/templates/doc/" + id + "." + extension;
                if (remove(vault, path)) removed.add(path);
            }
        }
        Map<String, Object> response = new LinkedHashMap<>(); response.put("id", id); response.put("removed", removed); return response;
    }

    private static boolean remove(Path vault, String relative) {
        Path file = vault.resolve(relative);
        try { return Files.deleteIfExists(file); }
        catch (IOException e) { throw new WlException(500, "파일을 지우지 못했다: " + file.toAbsolutePath().normalize()); }
    }
    private static String positionalId(ParsedArgs args, String usage) {
        if (args.positionalArgs().isEmpty() || Trim.isBlank(args.positionalArgs().getFirst())) throw new WlException(400, "인자가 하나 필요하다: " + usage);
        String id = args.positionalArgs().getFirst(); validateId(id); return id;
    }
    private static void validateId(String id) {
        if (Trim.isBlank(id) || !ID.matcher(id).matches()) throw new WlException(400, "템플릿 id 모양이 아니다: " + id);
    }
    private static void validateName(Map<String, String> options) {
        if (options.containsKey("name") && Trim.isBlank(options.get("name"))) throw new WlException(400, "템플릿 이름이 비었다");
    }
    private static String type(String raw) {
        if (raw == null) return null;
        String type = raw.toLowerCase(Locale.ROOT);
        if (!TYPES.contains(type)) throw new WlException(400, "모르는 템플릿 유형 이다: " + raw + ". 쓸 수 있는 것은 [plan, build, report, research, meeting, free] 뿐이다.");
        return type;
    }
    private static TemplateReader.TemplateRecord find(Path vault, String id) {
        return TemplateReader.scan(vault).templates().stream().filter(t -> t.id().equals(id)).findFirst().orElse(null);
    }
    private static WlException notFound(String id) { return new WlException(404, "Template not found: " + id); }
    private static Map<String, Object> saved(String id, String action, boolean builtin, String path, String receivedBody) {
        Map<String, Object> result = new LinkedHashMap<>(); result.put("id", id); result.put("action", action); result.put("builtin", builtin); result.put("path", path);
        String warning = WarningsDetector.detectIndentedBlocksWarning(receivedBody);
        if (warning != null) result.put("warning", warning);
        return result;
    }
}
