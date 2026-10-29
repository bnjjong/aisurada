package withlog.cmd;

import withlog.args.ParsedArgs;
import withlog.cli.WlException;
import withlog.git.VaultHistory;
import withlog.text.Trim;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/** 31.4: 현재 노트/색인/표식에 의존하지 않는 명령 전용 판 조회. */
public final class History {
    public static final String USAGE = "history <노트 id> [번호] [commit=<커밋>] [query=<글자>]";
    private History() {}
    public static Map<String, Object> execute(Path vault, ParsedArgs args) {
        Integer number = null;
        if (args.positionalArgs().size() > 1) {
            String raw = args.positionalArgs().get(1);
            try { number = Integer.parseInt(raw); }
            catch (NumberFormatException ignored) {}
            if (number == null || number < 1) throw new WlException(400, "판 번호는 1 이상의 정수여야 한다: " + raw);
        }
        String commit = Trim.trim(args.namedOptions().get("commit"));
        if (!commit.isEmpty() && !VaultHistory.commitShape(commit)) throw new WlException(400, "커밋은 40 자리 16 진수여야 한다: " + commit);
        if (number != null && !commit.isEmpty()) throw new WlException(400, "판 번호와 커밋을 함께 줄 수 없다. 하나만 준다");
        String id = args.positionalArgs().isEmpty() ? "" : args.positionalArgs().getFirst();
        if (Trim.isBlank(id)) throw new WlException(400, "인자가 하나 필요하다: " + USAGE);
        String query = Trim.trim(args.namedOptions().get("query"));
        var history = new VaultHistory(vault);
        var versions = history.versions(id);
        VaultHistory.Version selected = null;
        if (!commit.isEmpty()) {
            for (var version : versions) if (version.commit().equals(commit)) { selected = version; break; }
            if (selected == null) throw new WlException(409, "그 커밋은 이 노트의 판 목록에 없다: " + commit);
        } else if (number != null) {
            if (versions.isEmpty()) throw new WlException(409, "그 노트에는 판이 아직 없다: " + id);
            if (number > versions.size()) throw new WlException(409, "판 번호가 범위 밖이다(1 부터 " + versions.size() + " 까지 있다): " + number);
            selected = versions.get(number - 1);
        }
        List<VaultHistory.Content> contents = history.read(versions);
        Map<String, Object> result = new LinkedHashMap<>();
        result.put("id", id); result.put("count", versions.size());
        if (selected != null) {
            var content = history.read(List.of(selected)).getFirst();
            if (content != null) { result.put("body", content.full()); result.put("noteBody", content.body()); }
        }
        if (!query.isEmpty()) {
            List<String> matches = new ArrayList<>();
            List<VaultHistory.Content> search = history.read(versions);
            for (int i = 0; i < versions.size(); i++) {
                var content = search.get(i);
                if (content != null && contains(content.body(), query)) matches.add(versions.get(i).commit());
            }
            result.put("matches", matches);
        }
        List<Map<String, Object>> rows = new ArrayList<>();
        for (int i = 0; i < versions.size(); i++) rows.add(VaultHistory.row(versions.get(i), i + 1, contents.get(i)));
        result.put("versions", rows);
        return result;
    }
    private static boolean contains(String body, String query) {
        for (int start = 0; start <= body.length() - query.length(); start++)
            if (body.regionMatches(true, start, query, 0, query.length())) return true;
        return false;
    }
}
