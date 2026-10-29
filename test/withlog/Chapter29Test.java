package withlog;

import java.nio.file.*;
import java.util.*;
import static withlog.ChapterTestSupport.*;

public final class Chapter29Test {
    public static void main(String[] args) throws Exception {
        try (var t = new ChapterTestSupport("chapter29")) {
            Path v = t.vault();
            var draft = ok(v, "plan_day", "{\"date\":\"2026-06-22\"}");
            eq(draft.keySet().stream().toList(), List.of("saved", "date", "body"), "draft shape");
            eq(draft.get("body"), "# 2026-06-22 데일리 플랜\n\n## 어제 한 일\n- (기록 없음)\n\n## 오늘\n- (아직 예정 없음)\n", "empty draft exact text");
            agenda(v, "alpha", "agfixture-00000002", "알파");
            root(v, "friday", 1, "2026-06-19T09:00:00+09:00", "title: 데모 어제\n", "");
            note(v, "notes/alpha/friday/note.md", "ndfixture-00000002", "alpha", "2026-06-19T16:00:00Z", "title: 알파 어제\n", "");
            root(v, "today", 3, "2026-06-22T09:00:00+09:00", "title: 데모 오늘\n", "");
            note(v, "notes/alpha/today/note.html", "ndfixture-00000004", "alpha", "2026-06-22T10:00:00Z", "title: 알파 오늘\n", "<p>html</p>");
            root(v, "record", 5, "2026-06-21", "tier: record\ntitle: 기록 제외\n", "");
            note(v, "notes/demo/friday/sub/child/note.md", "snfixture-00000001", "demo", "2026-06-20", "title: 하위 제외\n", "");
            String expected = "# 2026-06-22 데일리 플랜\n\n## 어제 한 일 (2026-06-19)\n- 알파 어제 · 알파 (build)\n- 데모 어제 · 데모 (build)\n\n## 오늘\n- 데모 오늘 · 데모 (build)\n- 알파 오늘 · 알파 (build)\n";
            draft = ok(v, "plan-day", "date=2026-06-22", "agenda=존재 안 함", "dryRun=true");
            eq(draft.get("body"), expected, "weekend previous day and differing section sort"); eq(draft.get("saved"), false, "dry run with agenda does not save");
            check(!Files.exists(v.resolve(".withlog/agendas/존재-안-함.md")), "dry run does not create agenda");
            random("0c1d2e3f,1c2d3e4f");
            var saved = ok(v, "plan_day", "{\"date\":\"2026-06-22\",\"agenda\":\"데모\"}", "\u0000wl-session-id", "plan-session");
            eq(saved.keySet().stream().toList(), List.of("saved", "id", "agenda", "agendaCreated", "path", "body"), "saved shape omits date");
            eq(saved.get("body"), expected, "saved body drafted before inserting itself"); eq(saved.get("agenda"), "demo", "agenda title lookup");
            String file = Files.readString(v.resolve(saved.get("path") + "/note.md"));
            check(file.contains("type: plan\n") && file.contains("created: '2026-06-22T09:00:00+09:00'\n") && file.contains("mins: 0\n"), "fixed plan metadata");
            check(!file.contains("sessionId:"), "plan tool session deliberately omitted"); check(file.endsWith(expected), "file body identical to response");
            var second = run(v, "plan-day", "date=2026-06-22", "agenda=demo", "dryRun=");
            eq(second.status(), 0, "empty dryRun is false"); check(stdout(second).contains("데일리 플랜 · 데모 (plan)"), "prior saved plan in next draft");
            check(stderr(second).contains("같은 아젠다에 제목이 같은 노트가 이미 있어요:"), "duplicate warning in result stderr"); check(!stdout(second).contains("\"warning\""), "warning not in response shape");
            var newAgenda = ok(v, "plan-day", "date=2026-06-22", "agenda=새 작업"); eq(newAgenda.get("agendaCreated"), true, "missing agenda created by shared create path");
            for (String date : List.of("0000-02-29", "-0001-12-31", "+10000-01-01")) ok(v, "plan-day", "date=" + date);
            for (String date : List.of("2026-02-30", "2026-6-22", "２０２６-０６-２２", "+2026-06-22")) error(v, 400, "plan-day", "date=" + date, "dryRun=bad");
            error(v, 400, "plan_day", "{\"dryRun\":\"true\"}"); error(v, 400, "plan-day", "date=2026-06-22", "dryRun=TRUE"); error(v, 400, "plan-day", "2026-06-22");
            Path many = t.vault(); for (int i = 1; i <= 101; i++) root(many, "n" + i, i, "2026-06-22T10:00:00+09:00", "title: item" + i + "\n", "");
            String body = (String) ok(many, "plan-day", "date=2026-06-22").get("body"); eq(body.lines().filter(line -> line.startsWith("- item")).count(), 101L, "no 100-note page cutoff");
            Files.delete(v.resolve(".withlog/workspace.json")); check(stderr(error(v, 400, "plan-day", "date=bad")).contains("워크스페이스"), "workspace before date");
            t.done();
        }
    }
}
