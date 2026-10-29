package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.text.Trim;
import withlog.vault.CommentsReader;
import withlog.vault.NoteIndex;
import withlog.write.CommentAnchors;
import withlog.write.CommentsWriter;
import withlog.write.DerivedBodyWriter;
import withlog.write.DocToken;
import withlog.write.IdGenerator;
import withlog.write.TestApparatus;
import withlog.engine.WritePipeline;

import java.nio.file.Path;
import java.time.ZoneId;
import java.time.format.DateTimeFormatter;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;

/** 19.8: 명령 표면 전용 코멘트 달기/고치기/지우기. 본문 앵커는 지우기에서 먼저 걷는다. */
public final class CommentCommands {
    public static final String CREATE_USAGE = "create-comment <노트 id> [하위 노트 id] body=<본문> [anchor_quote=<가리킨 글>] [comment_id=<식별자. 안 주면 발급한다>]";
    public static final String UPDATE_USAGE = "update-comment <노트 id> [하위 노트 id] comment_id=<코멘트 id> body=<본문>";
    public static final String DELETE_USAGE = "delete-comment <노트 id> [하위 노트 id] comment_id=<코멘트 id>";
    private static final DateTimeFormatter AT = DateTimeFormatter.ofPattern("uuuu-MM-dd'T'HH:mm:ss'+09:00'", Locale.ROOT);

    private CommentCommands() {}

    @SuppressWarnings("unchecked")
    public static Map<String, Object> execute(Path vault, ParsedArgs parsed) {

        NoteIndex index = WritePipeline.index(vault);
        String command = parsed.commandName();
        boolean create = command.equals("create-comment");
        boolean delete = command.equals("delete-comment");
        String usage = create ? CREATE_USAGE : delete ? DELETE_USAGE : UPDATE_USAGE;
        var ids = CommentTarget.parse(parsed, usage);
        Map<String, String> opts = parsed.namedOptions();
        String id = opts.containsKey("comment_id") ? Trim.trim(opts.get("comment_id")) : "";
        if (!create && id.isEmpty()) throw new WlException(400, "comment_id 이 필요하다: " + usage);
        String body = opts.get("body");
        if (!delete) checkBody(body, usage);
        String quote = opts.get("anchor_quote");
        if (create && quote != null && quote.length() > 2_000) {
            throw new WlException(400, "앵커 인용문이 너무 길다. 상한은 2000자다.");
        }

        // 옵션 검사가 끝난 뒤에 대상을 찾고 코멘트 파일을 연다(19.8 공통 6~7번).
        var target = CommentTarget.resolve(vault, index, ids);
        var envelope = CommentsReader.readComments(vault, target.file());
        if ("unreadable".equals(envelope.get("state"))) {
            throw new WlException(409, "코멘트 파일을 못 읽어서 아무것도 안 썼다: " + envelope.get("reason")
                    + ". 여기서 쓰면 그 파일에 있던 코멘트가 통째로 사라진다. 파일을 먼저 고치거나 치운다.");
        }
        List<Map<String, Object>> comments = "loaded".equals(envelope.get("state"))
                ? new ArrayList<>((List<Map<String, Object>>) envelope.get("comments")) : new ArrayList<>();
        if (create && id.isEmpty()) id = IdGenerator.nextId("cm");
        Map<String, Object> found = null;
        for (var item : comments) {
            if (id.equals(item.get("id"))) { found = item; break; }
        }
        if (create && found != null) throw new WlException(409, "이미 그 식별자를 쓰는 코멘트가 있다: " + id);
        if (!create && found == null) throw new WlException(409, "그 코멘트가 없다: " + id);

        Map<String, Object> response = new LinkedHashMap<>();
        response.put("id", id);
        response.put("path", vault.relativize(target.file()).toString().replace('\\', '/'));
        if (create) {
            Map<String, Object> item = new LinkedHashMap<>();
            item.put("id", id);
            item.put("createdAt", now());
            if (quote != null && !Trim.isBlank(quote)) item.put("anchor", Map.of("quote", quote));
            item.put("body", body);
            comments.add(item);
        } else if (!delete) {
            String editedAt = now();
            String createdAt = (String) found.get("createdAt");
            if (createdAt.length() == 25 && editedAt.length() == 25 && editedAt.compareTo(createdAt) < 0) editedAt = createdAt;
            found.put("editedAt", editedAt);
            found.put("body", body);
        } else {
            String format = target.owner().format();
            if (!"markdown".equals(format)) {
                throw new WlException(409, "본문이 markdown 이 아니라 코멘트 앵커를 걷어낼 수 없다(포맷="
                        + format.toUpperCase(Locale.ROOT) + "): " + target.owner().id());
            }
            // 1.13/19.7: 확장자로 끝나는 폴더의 읽기 버그는 본문을 빈 글로 본다.
            Path bodyFile = target.owner().isExtensionFolder() ? vault.resolve(target.owner().vaultRelPath()) : target.owner().contentFilePath();
            var read = DerivedBodyWriter.read(bodyFile);
            String stripped = CommentAnchors.strip(read.body(), id);
            boolean changed = !stripped.equals(read.body());
            response.put("remaining", comments.size() - 1);
            response.put("markerStripped", changed);
            if (changed) {
                byte[] written = target.owner().isSubNote()
                        ? DerivedBodyWriter.rewriteSub(vault, target.root(), target.owner(), stripped).bytes()
                        : DerivedBodyWriter.rewriteRoot(vault, target.owner(), stripped);
                response.put("bodyBefore", DocToken.compute(read.bytes()));
                response.put("bodyAfter", DocToken.compute(written));
            }
            comments.remove(found);
        }
        // 본문이 바뀐 뒤에만 코멘트 항목을 지우고 파일 전체를 쓴다. 뒤 쓰기가 실패하면 본문은 이미 바뀌었다.
        CommentsWriter.write(target.file(), comments);
        if (!delete) response.put("total", comments.size());
        // 원본의 쓴 뒤 색인 재삽입은 하지 않는다. 이 구현은 다음 호출마다 새로 훑는다(8.6).
        return response;
    }

    private static void checkBody(String body, String usage) {
        if (body == null) throw new WlException(400, "body 이 필요하다: " + usage);
        if (Trim.isBlank(body)) throw new WlException(400, "코멘트 본문이 비었다");
        if (body.length() > 20_000) throw new WlException(400, "코멘트 본문이 너무 길다(" + body.length() + "자). 상한은 20000자다.");
    }

    /** 19.9.4의 현행 호환 동작: 프로세스 벽시계에 +09:00 문자열을 붙인다. */
    private static String now() {
        return AT.format(TestApparatus.nowLocalDateTime(ZoneId.systemDefault()));
    }
}
