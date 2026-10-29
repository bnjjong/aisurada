package withlog;

import java.nio.file.*;
import java.util.*;
import static withlog.ChapterTestSupport.*;

public final class Chapter25Test {
    public static void main(String[] args) throws Exception {
        try (var t = new ChapterTestSupport("chapter25")) {
            Path v = t.vault(); String id = "rv-evening-2026-02-30", path = ".withlog/reports/" + id + ".json";
            var r = ok(v, "save_report", "{\"kind\":\" EVENING \",\"date\":\"2026-02-30\",\"body\":\" 원문\\n\",\"memo\":\"메모\"}");
            eq(r.keySet().stream().toList(), List.of("saved", "id", "kind", "date", "hasBody", "hasMemo", "path"), "saved key order");
            eq(Files.readString(v.resolve(path)), "{\n  \"body\": \" 원문\\n\",\n  \"memo\": \"메모\"\n}\n", "exact JSON file bytes");
            eq(ok(v, "get_report", "{\"id\":\"" + id + "\"}").get("body"), " 원문\n", "body untrimmed, calendar shape only");
            var rows = list(v, "list-reports", "bodies=true"); eq(rows.size(), 1, "one saved report");
            check(rows.getFirst().containsKey("memo"), "bodies includes nullable keys");
            ok(v, "save-report", "kind=evening", "date=2026-02-30", "memo=only memo");
            eq(ok(v, "get-report", id).get("body"), null, "save replaces whole document");
            eq(ok(v, "save-report", "kind=evening", "date=2026-02-30").get("deleted"), true, "absent body+memo delete");
            eq(ok(v, "save-report", "kind=evening", "date=2026-02-30", "body=\u00a0\u3000").keySet().stream().toList(), List.of("saved", "deleted", "id"), "repeated delete shape");
            error(v, 404, "get-report", id);
            for (String bad : List.of("", "[]", "body: yes", "body: 2026-01-01", "body: 3", "{\"body\":\"\\/\"}", "{\t\"body\":\"x\"}")) {
                put(v, path, bad); error(v, 500, "get-report", id);
            }
            for (String shell : List.of("{}", "body: '  '\nmemo: null")) { put(v, path, shell); error(v, 404, "get-report", id); }
            put(v, path, "\ufeffbody: 정상"); eq(ok(v, "get-report", id).get("body"), "정상", "BOM and YAML accepted");
            ok(v, "save-report", "kind=evening", "date=2026-02-30", "body=a\u0085b");
            eq(ok(v, "get-report", id).get("body"), "a b", "YAML folds raw NEL");
            ok(v, "save-report", "kind=evening", "date=2026-02-30", "body=\u007f"); error(v, 500, "get-report", id);
            put(v, ".withlog/reports/rv-morning-2026-01-01.json", "{\"memo\":\"fine\"}");
            var incomplete = ok(v, "list-reports", "kind=morning"); check(incomplete.containsKey("incomplete"), "bad other kind still wraps list");
            check(stderr(run(v, "list-reports")).contains("회고 1건을 못 읽어 목록에서 뺐다."), "diagnostics in result stderr");
            put(v, path, "body: !!int abc"); eq(run(v, "get-report", id).status(), 1, "explicit bad YAML tag is unexpected failure");
            ok(v, "save-report", "kind=evening", "date=2026-02-30", "body=repaired");
            eq(ok(v, "get-report", id).get("body"), "repaired", "save does not read existing broken content");
            error(v, 400, "save_report", "{\"kind\":1}"); error(v, 400, "save_report", "{\"kind\":\"evening\",\"date\":\"2026-01-01\",\"memo\":\"" + "x".repeat(200001) + "\"}");
            ok(v, "save-report", "kind=weekly", "date=2026-13-45", "body=" + "한".repeat(340000));
            error(v, 500, "get-report", "rv-weekly-2026-13-45");
            error(v, 400, "list-reports", "from=2026-06-10", "to=2026-06-01");
            check(stderr(error(v, 400, "save-report", "kind= BOGUS ", "date=2026-06-16")).contains("종류 이다: BOGUS."), "kind diagnostic retains trimmed original case");
            error(v, 400, "save-report", "kind=evening", "date=２０２６-０１-０１");
            Files.delete(v.resolve(".withlog/workspace.json"));
            check(stderr(error(v, 400, "save-report", "kind=bad")).contains("워크스페이스"), "workspace before values");
            ok(v, "get-report", id); t.done();
        }
    }
}
