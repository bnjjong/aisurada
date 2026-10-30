package withlog;

import withlog.args.JsonReader;
import withlog.args.JsonValue;
import withlog.cmd.Reindex;
import withlog.engine.EngineExecutor;
import withlog.engine.EngineResult;
import withlog.out.JsonWriter;
import withlog.vault.TemplateReader;

import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import java.util.concurrent.TimeUnit;
import static withlog.ChapterTestSupport.*;

/** 24장의 실제 디스크 바이트, 부분 실패와 두 명령 표면을 검증한다. */
public final class Chapter24Test {
    private static void template(Path v, String category, String file, String yaml, String body) throws Exception {
        put(v, ".withlog/templates/" + category + "/" + file, file.endsWith(".html") ? "<!--withlog\n" + yaml + "-->\n" + body : "---\n" + yaml + "---\n" + body);
    }
    private static void doc(Path v, String file, String id, boolean builtin, String body) throws Exception {
        template(v, "doc", file, "id: " + id + "\nname: " + file + "\ntype: build\ndesc: seed\nbuiltin: " + builtin + "\n", body);
    }
    private static void ritual(Path v, String file, String id, String rev, String body) throws Exception {
        template(v, "ritual", file, "id: " + id + "\nname: " + id + "\nrev: " + rev + "\nformat: markdown\n", body);
    }
    private static String json(Map<String, ?> fields) { return JsonWriter.stringify(fields); }
    private static Map<String, Object> create(Path v, String name, String body) {
        return ok(v, "save_template", json(Map.of("name", name, "body", body)));
    }
    private static void message(Path v, int code, String expected, String... args) {
        eq(stderr(error(v, code, args)), "WL-" + code + " " + expected + "\n", "exact failure text");
    }
    public static void main(String[] args) throws Exception {
        try (var t = new ChapterTestSupport("chapter24")) {
            validation(t); creationAndBytes(t); documentUpdates(t); ritualUpdates(t); reading(t); failureStates(t); socket(t);
            t.done();
        }
    }

    private static void validation(ChapterTestSupport t) throws Exception {
        Path v = t.vault();
        for (String name : List.of("delete_doc_template", "save_ritual_template")) eq(run(v, name, "{}").status(), 2, "no tool surface for " + name);
        message(v, 400, "인자 검증 실패(save_template): 모르는 키다: [titel]. 쓸 수 있는 것은 [id, name, type, desc, body] 뿐이다.", "save_template", "{\"titel\":\"x\",\"id\":1}");
        for (String key : List.of("id", "name", "type", "desc", "body")) {
            message(v, 400, "인자 검증 실패(save_template): " + key + " 는 문자열 여야 한다: null", "save_template", "{\"" + key + "\":null}");
        }
        message(v, 400, "인자 검증 실패(save_template): id 는 문자열 여야 한다: 1", "save_template", "{\"id\":1,\"name\":false,\"body\":null}");
        message(v, 400, "새 템플릿은 body 이 필요하다: " + withlog.cmd.TemplateCommands.SAVE_USAGE, "save_template", "{}");
        message(v, 400, "새 템플릿은 name 이 필요하다: " + withlog.cmd.TemplateCommands.SAVE_USAGE, "save-template", "body=x", "type=bad");
        message(v, 400, "새 템플릿은 body 이 필요하다: " + withlog.cmd.TemplateCommands.SAVE_USAGE, "save-template", "body=\u00a0\u3000", "name= ", "type=bad");
        message(v, 400, "템플릿 id 모양이 아니다:  ", "save-template", "id= ", "name=", "type=bad");
        message(v, 400, "템플릿 이름이 비었다", "save-template", "id=missing", "name=\u3000", "type=bad");
        message(v, 400, "모르는 템플릿 유형 이다:  PLAN . 쓸 수 있는 것은 [plan, build, report, research, meeting, free] 뿐이다.", "save-template", "id=missing", "type= PLAN ");
        message(v, 404, "Template not found: missing", "save-template", "id=missing", "type=PLAN");
        for (String id : List.of("", "\u00a0\u3000")) {
            message(v, 400, "인자가 하나 필요하다: delete-doc-template <문서 템플릿 id>", "delete-doc-template", id);
            message(v, 400, "인자가 하나 필요하다: " + withlog.cmd.TemplateCommands.RITUAL_USAGE, "save-ritual-template", id, "name=");
        }
        message(v, 400, "save-ritual-template 이 모르는 옵션이다: rev. 쓸 수 있는 것은 [body, desc, name] 뿐이다.", "save-ritual-template", "a", "b", " rev =morning");
        message(v, 400, "save-template 이 모르는 옵션이다: f1. 쓸 수 있는 것은 [body, desc, id, name, type] 뿐이다.", "save-template", "f1=");
        message(v, 400, "save-template 이 받는 자리 인자는 0 개인데 1 개를 받았다: " + withlog.cmd.TemplateCommands.SAVE_USAGE, "save-template", "=x");
        message(v, 400, "delete-doc-template 이 받는 자리 인자는 1 개인데 2 개를 받았다: delete-doc-template <문서 템플릿 id>", "delete-doc-template", "a", "b");
        message(v, 400, "list-templates 이 받는 자리 인자는 0 개인데 1 개를 받았다: list-templates [category=doc|ritual] [type=<유형. 주면 리추얼이 빠진다>] [bodies=true. 본문까지 싣는다. 기본은 메타만]", "list-templates", "x");
        for (String id : List.of("m-plan\n", "한글", "a b")) message(v, 400, "템플릿 id 모양이 아니다: " + id, "delete-doc-template", id);
        eq(ok(v, "delete-doc-template", "missing").get("removed"), List.of(), "absent deletion succeeds");
        Files.delete(v.resolve(".withlog/workspace.json"));
        for (String[] argv : List.of(new String[]{"save-template"}, new String[]{"delete-doc-template"}, new String[]{"save-ritual-template"}))
            check(stderr(error(v, 400, argv)).contains("워크스페이스"), "workspace before field values");
        message(v, 400, "인자 검증 실패(save_template): body 가 너무 길다(최대 200000자): 200001자", "save_template", json(Map.of("body", "x".repeat(200001))));
        eq(list(v, "list-templates"), List.of(), "reading needs no marker"); error(v, 404, "get-template", "missing");
    }

    private static void creationAndBytes(ChapterTestSupport t) throws Exception {
        Path v = t.vault();
        System.setProperty("withlog.test.now", Long.toString(Long.parseLong("msciohji", 36))); random("6b623625");
        String body = "# [제목]\n\n배경을 적는다.\n";
        var result = ok(v, "save_template", json(Map.of("name", "리서치 노트, 정리용", "type", "research", "desc", "조사 기록용", "body", body)));
        String id = "tpl-msciohji-6b623625", relative = ".withlog/templates/doc/" + id + ".md";
        eq(result, Map.of("id", id, "action", "created", "builtin", false, "path", relative), "manual creation example response");
        eq(result.keySet().stream().toList(), List.of("id", "action", "builtin", "path"), "save response key order");
        eq(Files.readString(v.resolve(relative)), "---\nid: " + id + "\nname: 리서치 노트, 정리용\ntype: research\nformat: markdown\ndesc: 조사 기록용\nbuiltin: false\n---\n" + body, "manual exact file bytes");
        eq(ok(v, "get-template", id).get("body"), body, "body final LF round trip");
        random("00000001"); result = ok(v, "save-template", "id=", " name =\u3000 x \u00a0", "body=without final LF", "type=PLAN");
        id = result.get("id").toString(); relative = result.get("path").toString();
        String file = Files.readString(v.resolve(relative));
        check(file.contains("name: x\n") && file.contains("type: plan\n") && file.contains("desc: ''\n"), "create trims name, accepts uppercase type, emits empty desc");
        check(file.endsWith("without final LF"), "writer adds no body LF");
        var read = ok(v, "get_template", json(Map.of("id", id)));
        eq(read.keySet().stream().toList(), List.of("id", "category", "name", "type", "format", "desc", "body", "builtin"), "doc detail order");
        random("00000002"); result = create(v, "boundary", "😀".repeat(100000));
        eq(ok(v, "get-template", result.get("id").toString()).get("body").toString().length(), 200000, "tool accepts 200000 UTF16 units");
        random("00000003"); result = create(v, "default type", "body"); eq(ok(v, "get-template", result.get("id").toString()).get("type"), "plan", "creation defaults plan");
        random("00000004"); result = ok(v, "save-template", "name=large", "body=" + "한".repeat(340000));
        check(Files.size(v.resolve(result.get("path").toString())) > 1000000, "command can create larger than read limit");
        error(v, 404, "get-template", result.get("id").toString());
        random("00000005"); result = create(v, "surrogate \ud83d", "body \ud83d");
        file = Files.readString(v.resolve(result.get("path").toString())); check(file.contains("\\uD83D"), "metadata uses uppercase surrogate escape"); check(file.endsWith("body ?"), "body surrogate becomes question mark");
        String surrogateId = result.get("id").toString();
        var raw = TemplateReader.scan(v).templates().stream().filter(x -> x.id().equals(surrogateId)).findFirst().orElseThrow();
        eq(raw.name(), "surrogate \ud83d", "YAML metadata preserves unpaired unit");
        eq(ok(v, "get-template", raw.id()).get("name"), "surrogate ?", "stdout replaces metadata surrogate");
    }

    private static void documentUpdates(ChapterTestSupport t) throws Exception {
        Path v = t.vault(); doc(v, "half.html", "half", false, "<h1>옛 본문</h1>\n"); doc(v, "half.md", "half", false, "# 새 본문\n");
        eq(list(v, "list-templates").size(), 2, "duplicate id retained in list");
        eq(ok(v, "get-template", "half").get("format"), "html", "html wins first scan");
        var saved = ok(v, "save-template", "id=half", "name=고친 이름"); eq(saved.get("action"), "updated", "update action");
        check(!Files.exists(v.resolve(".withlog/templates/doc/half.html")), "old format removed after write");
        check(Files.readString(v.resolve(".withlog/templates/doc/half.md")).endsWith("<h1>옛 본문</h1>\n"), "old HTML body replaces newer markdown");
        eq(ok(v, "delete-doc-template", "half").get("removed"), List.of(".withlog/templates/doc/half.md"), "actual removed paths");
        for (String extension : List.of("md", "html", "json")) doc(v, "all." + extension, "all", false, "all");
        eq(ok(v, "delete-doc-template", "all").get("removed"), List.of(".withlog/templates/doc/all.md", ".withlog/templates/doc/all.html", ".withlog/templates/doc/all.json"), "deletes in md/html/json order");
        doc(v, "a-source.md", "bar", false, "alias original");
        ok(v, "save-template", "id=bar", "body=new body");
        check(Files.exists(v.resolve(".withlog/templates/doc/a-source.md")), "filename alias left in place");
        eq(ok(v, "get-template", "bar").get("body"), "alias original", "earlier alias still wins");
        eq(ok(v, "delete-doc-template", "bar").get("removed"), List.of(".withlog/templates/doc/bar.md"), "deletion follows id, not found filename");
        eq(ok(v, "delete-doc-template", "bar").get("removed"), List.of(), "alias-only deletion does nothing");
        doc(v, "built.md", "built", true, "body"); error(v, 400, "delete-doc-template", "built");
        ok(v, "save-template", "id=built", "body="); eq(ok(v, "get-template", "built").get("builtin"), true, "update keeps builtin");
        eq(ok(v, "get-template", "built").get("body"), "", "empty update body clears");
        template(v, "doc", "defaults.md", "id: defaults\nname: ' '\ntype: Plan\nbuiltin: 'false'\n", "body");
        error(v, 400, "delete-doc-template", "defaults");
        ok(v, "save-template", "id=defaults", "name= a ", "desc=  description  ");
        String file = Files.readString(v.resolve(".withlog/templates/doc/defaults.md"));
        check(file.contains("name: ' a '\n") && file.contains("type: build\n") && file.contains("builtin: true\n"), "update raw name and reader fallback values persisted");
        var read = ok(v, "get-template", "defaults"); eq(read.get("name"), "a", "reader trims updated name"); eq(read.get("desc"), "  description  ", "description untrimmed");
        ok(v, "save-template", "id=defaults"); check(Files.readString(v.resolve(".withlog/templates/doc/defaults.md")).contains("name: a\n"), "omitted name rewrites trimmed read value");
        String multiline = "line one\n  indented\n\n" + "long word ".repeat(12) + "\n";
        ok(v, "save-template", "id=defaults", "desc=" + multiline);
        eq(ok(v, "get-template", "defaults").get("desc"), multiline, "shared YAML blocks preserve multiline description");
        template(v, "doc", "no.md", "id: no\nbuiltin: no\n", "body"); ok(v, "delete-doc-template", "no");
        template(v, "doc", "json.json", "id: json\nbuiltin: false\n", "{\"root\":1}");
        ok(v, "save-template", "id=json", "desc="); check(!Files.exists(v.resolve(".withlog/templates/doc/json.json")), "json old format removed");
        eq(ok(v, "get-template", "json").get("body"), "{\"root\":1}", "raw json body preserved as markdown");
        doc(v, "warn.md", "warn", false, "  # indented\n");
        check(!ok(v, "save-template", "id=warn", "desc=changed").containsKey("warning"), "old body does not produce new warning");
        saved = ok(v, "save-template", "id=warn", "body=  | a | b |\n  | - | - |\n");
        check(saved.get("warning").toString().startsWith("본문에 들여쓴 블록이 1곳 있어요(1번째 줄 표)"), "shared indented-block warning only");
        eq(saved.keySet().stream().toList(), List.of("id", "action", "builtin", "path", "warning"), "warning last");
    }

    private static void ritualUpdates(ChapterTestSupport t) throws Exception {
        Path v = t.vault();
        ritual(v, "morning.md", "t-morning", "morning", "# 모닝 리포트 - [날짜]\n");
        put(v, ".withlog/templates/ritual/morning.md", "---\nid: t-morning\nrev: morning\nname: 기본 모닝 데일리 템플릿\nicon: sunrise\nformat: markdown\ndesc: 아침에 씁니다\n---\n# 모닝 리포트 - [날짜]\n");
        var saved = ok(v, "save-ritual-template", "t-morning", "name=내가 고친 모닝");
        eq(saved, Map.of("id", "t-morning", "action", "updated", "builtin", false, "path", ".withlog/templates/ritual/morning.md"), "ritual response example");
        eq(Files.readString(v.resolve(".withlog/templates/ritual/morning.md")), "---\nid: t-morning\nrev: morning\nname: 내가 고친 모닝\nicon: sunrise\nformat: markdown\ndesc: 아침에 씁니다\n---\n# 모닝 리포트 - [날짜]\n", "ritual example exact file bytes");
        message(v, 400, "리추얼 템플릿은 지울 수 없다. 지우는 길이 앱에도 없다: t-morning", "delete-doc-template", "t-morning");
        error(v, 404, "save-template", "id=t-morning");
        ok(v, "save-ritual-template", "t-morning", "body=", "desc="); eq(ok(v, "get-template", "t-morning").get("body"), "", "ritual empty body allowed");
        template(v, "ritual", "old.md", "id: old\nname: ' '\nrev: bogus\nicon: |\n  invalid\nformat: json\n", "<p>legacy</p>");
        message(v, 400, "같은 rev 를 든 리추얼 템플릿이 2건이라 저장하지 않는다. 파일 이름이 rev 라서 서로를 덮는다: rev=morning", "save-ritual-template", "old");
        Files.delete(v.resolve(".withlog/templates/ritual/morning.md"));
        saved = ok(v, "save-ritual-template", "old", "body=  # warning");
        check(saved.containsKey("warning"), "ritual supplied body warning");
        String file = Files.readString(v.resolve(".withlog/templates/ritual/morning.md"));
        check(file.startsWith("---\nid: old\nrev: morning\nname: old\nicon: sunrise\nformat: html\n"), "ritual fallback metadata and markdown fence despite HTML");
        check(Files.exists(v.resolve(".withlog/templates/ritual/old.md")), "custom ritual original left in place");
        error(v, 400, "save-ritual-template", "old");
        Path aliases = t.vault(); ritual(aliases, "custom.md", "weekly-custom", "weekly", "body");
        put(aliases, ".withlog/templates/ritual/weekly.md", "broken frontmatter");
        ok(aliases, "save-ritual-template", "weekly-custom", "desc=overwrite unreadable canonical");
        eq(ok(aliases, "get-template", "weekly-custom").get("desc"), "", "earlier custom alias still wins get");
        check(Files.readString(aliases.resolve(".withlog/templates/ritual/weekly.md")).contains("desc: overwrite unreadable canonical\n"), "unreadable rev target overwritten");
        error(aliases, 400, "save-ritual-template", "weekly-custom");
        Path shield = t.vault(); ritual(shield, "first.md", "target", "evening", "body"); ritual(shield, "second.md", "shielded", "evening", "body");
        doc(shield, "shielded.md", "shielded", false, "doc");
        error(shield, 404, "save-ritual-template", "shielded");
        message(shield, 400, "같은 rev 를 든 리추얼 템플릿이 2건이라 저장하지 않는다. 파일 이름이 rev 라서 서로를 덮는다: rev=evening", "save-ritual-template", "target");
        error(shield, 400, "save-ritual-template", "target", "name=\u00a0", "body=whatever");
    }

    @SuppressWarnings("unchecked")
    private static void reading(ChapterTestSupport t) throws Exception {
        Path v = t.vault();
        doc(v, "user.md", "user", false, "user"); ritual(v, "morning.md", "ritual", "morning", "ritual");
        eq(list(v, "list-templates", "category=ritual", "type=plan"), List.of(), "type excludes all rituals");
        var detail = ok(v, "get-template", "ritual");
        eq(detail.keySet().stream().toList(), List.of("id", "category", "name", "rev", "icon", "format", "desc", "body"), "ritual detail key order");
        var bodies = list(v, "list-templates", "bodies=true"); eq(bodies.getFirst(), ok(v, "get-template", "user"), "bodies has doc detail shape");
        eq(bodies.get(1), detail, "bodies has ritual detail shape");
        put(v, ".withlog/templates/doc/README.md", "plain readme"); put(v, ".withlog/templates/ritual/bad.md", "bad");
        var result = ok(v, "list_templates", "{\"type\":\"meeting\"}"); eq(((Map<String, Object>) result.get("incomplete")).get("skipped"), 1, "type filters ritual skipped, not doc skipped");
        result = ok(v, "list-templates", "category=ritual"); eq(((Map<String, Object>) result.get("incomplete")).get("skipped"), 1, "category filters skipped");
        String failure = stderr(error(v, 400, "list-templates", "bodies=bad", "category=doc"));
        check(failure.startsWith("템플릿 1건을 못 읽어 목록에서 뺐다.\n  .withlog/templates/doc/README.md — 프론트매터가 없거나 문법이 어긋난다\nWL-400"), "diagnostic before bodies error");
        check(!stderr(error(v, 400, "list-templates", "category=bogus", "bodies=bad")).contains("README.md"), "category error before scan diagnostics");
        eq(stderr(run(v, "list_templates", "{}")), "템플릿 2건을 못 읽어 목록에서 뺐다.\n  .withlog/templates/doc/README.md — 프론트매터가 없거나 문법이 어긋난다\n  .withlog/templates/ritual/bad.md — 프론트매터가 없거나 문법이 어긋난다\n", "tool list writes the same diagnostics as the command list");
        eq(run(v, "get-template", "user").stderrBytes().length, 0, "get suppresses unrelated skipped-file diagnostics");
        put(v, ".withlog/templates/doc/README.md", "---\n{}\n---\n"); Files.delete(v.resolve(".withlog/templates/ritual/bad.md"));
        var fallback = ok(v, "get-template", "README"); eq(fallback.get("name"), "README", "empty YAML map is a template"); eq(fallback.get("builtin"), true, "empty map builtin default true");
        for (String yaml : List.of("", "# comment\n", "[]\n")) {
            template(v, "doc", "empty.md", yaml, "body"); error(v, 404, "get-template", "empty");
        }
        template(v, "doc", "fallback.md", "id: 123\n", "body"); eq(ok(v, "get-template", "fallback").get("id"), "fallback", "non-string id falls back to filename");
        for (String[] pair : new String[][]{{"space", "' '", "모양이 어긋난다"}, {"unicode", "한글", "모양이 어긋난다"}, {"unsafe", "a/b", "안전한 경로 세그먼트가 아니다"}, {"long", "a".repeat(101), "안전한 경로 세그먼트가 아니다"}}) {
            template(v, "doc", pair[0] + ".md", "id: " + pair[1] + "\n", "body");
            check(TemplateReader.scan(v).skipped().stream().anyMatch(s -> s.relPath().endsWith(pair[0] + ".md") && s.reason().contains(pair[2])), "reader id validation order " + pair[0]);
        }
        template(v, "doc", "conflicted copy.md", "id: user\nbuiltin: false\n", "conflict body");
        eq(TemplateReader.scan(v).templates().stream().filter(x -> x.id().equals("user")).count(), 2L, "conflict copy not filtered");
        Path links = t.vault(); Files.createDirectories(links.resolve(".withlog/templates/doc"));
        Files.createSymbolicLink(links.resolve(".withlog/templates/doc/broken.md"), t.temp.resolve("absent"));
        put(links, ".withlog/templates/doc/ignored.MD", "bad"); put(links, ".withlog/templates/doc/nested/deep.md", "bad");
        eq(list(links, "list-templates"), List.of(), "broken links, uppercase extensions and nested files ignored");
        put(links, ".withlog/templates/doc/cross.md", "\ufeff<!--withlog\nid: cross\nbuiltin: false\n-->\r\n\nx");
        eq(ok(links, "get-template", "cross").get("body"), "x", "CRLF then LF removed in the declared sequence");
        put(links, ".withlog/templates/doc/cross.md", "---\nid: cross\n---\n\nx");
        eq(ok(links, "get-template", "cross").get("body"), "\nx", "one LF removed from ordinary closing fence");
        put(links, ".withlog/templates/doc/raw.json", "{\"id\":\"raw\"}"); error(links, 404, "get-template", "raw");
        template(links, "ritual", "icon.md", "id: icon\nrev: evening\nicon: |\n  custom\n", "body");
        eq(ok(links, "get-template", "icon").get("icon"), "sunset", "whole-string regex rejects trailing LF in icon");
        check(Reindex.execute(links, System.currentTimeMillis()).get("skipped").toString().contains("docTemplate"), "reindex uses same template diagnostics");
    }

    private static void failureStates(ChapterTestSupport t) throws Exception {
        Path v = t.vault();
        for (String invalid : List.of("id: !!int abc\n", "name: !!float x\n", "id: !!binary x\n")) {
            template(v, "doc", "poison.md", invalid, "body");
            for (String[] argv : List.of(new String[]{"list-templates"}, new String[]{"get-template", "anything"}, new String[]{"save-template", "id=anything"}, new String[]{"delete-doc-template", "anything"}, new String[]{"save-ritual-template", "anything"}, new String[]{"reindex"})) {
                var failed = run(v, argv); eq(failed.status(), 1, "explicit invalid tag halts template scan"); eq(failed.stdoutBytes().length, 0, "unexpected failure has no stdout");
            }
            random("00000011"); create(v, "creation bypasses template scan", "body");
        }
        Files.delete(v.resolve(".withlog/templates/doc/poison.md"));
        root(v, "bad", 1, "2026-06-16", "mins: !!int abc\n", "body");
        eq(run(v, "save-template", "name=x", "body=y").status(), 1, "note reconcile can block create before semantic checks");
        success(v, "list-templates"); Files.delete(v.resolve("notes/demo/bad/note.md"));
        doc(v, "source.html", "migrate", false, "old");
        put(v, ".withlog/templates/doc/migrate.html/child", "blocking directory");
        error(v, 500, "save-template", "id=migrate", "name=new");
        check(Files.readString(v.resolve(".withlog/templates/doc/migrate.md")).contains("name: new\n"), "new markdown remains after old format delete failure");
        check(Files.exists(v.resolve(".withlog/templates/doc/source.html")), "source alias survives failed cleanup");
        Path partial = t.vault(); doc(partial, "partial.md", "partial", false, "md"); doc(partial, "partial.json", "partial", false, "json");
        put(partial, ".withlog/templates/doc/partial.html/child", "blocking directory");
        error(partial, 500, "delete-doc-template", "partial");
        check(!Files.exists(partial.resolve(".withlog/templates/doc/partial.md")) && Files.exists(partial.resolve(".withlog/templates/doc/partial.json")), "partial deletion preserves later files");
        Path target = t.vault(); doc(target, "blocked.html", "blocked", false, "old html"); Files.createDirectory(target.resolve(".withlog/templates/doc/blocked.md"));
        error(target, 500, "save-template", "id=blocked", "body=new");
        check(Files.exists(target.resolve(".withlog/templates/doc/blocked.html")), "old format not deleted when new write fails");
        Path link = t.vault(), outside = t.temp.resolve("outside-template.md");
        Files.writeString(outside, "---\nid: link\nbuiltin: false\n---\noutside"); Files.createDirectories(link.resolve(".withlog/templates/doc"));
        Files.createSymbolicLink(link.resolve(".withlog/templates/doc/link.md"), outside);
        ok(link, "save-template", "id=link", "body=new"); check(!Files.isSymbolicLink(link.resolve(".withlog/templates/doc/link.md")), "atomic write replaces symlink");
        check(Files.readString(outside).endsWith("outside"), "write does not change symlink target");
        Files.delete(link.resolve(".withlog/templates/doc/link.md")); Files.createSymbolicLink(link.resolve(".withlog/templates/doc/link.md"), outside);
        ok(link, "delete-doc-template", "link"); check(Files.exists(outside), "deletion removes symlink itself");
        Path blocked = t.vault(); put(blocked, ".withlog/templates/doc", "file instead of directory");
        message(blocked, 500, "폴더를 만들지 못했다: " + blocked.resolve(".withlog/templates/doc"), "save-template", "name=x", "body=y");
        eq(list(blocked, "list-templates"), List.of(), "regular file as category folder reads empty");
    }

    private record Reply(String out, String err) {
        @SuppressWarnings("unchecked") Map<String, Object> object() { return (Map<String, Object>) value(JsonReader.parseFile(out)); }
    }
    private static void socket(ChapterTestSupport t) throws Exception {
        Path v = t.vault(), home = Files.createDirectory(t.temp.resolve("socket-home"));
        ritual(v, "morning.md", "ritual", "morning", "ritual body");
        Path socket = Path.of("/tmp", "wl-templates-" + UUID.randomUUID().toString().substring(0, 8) + ".sock");
        List<String> prefix = List.of(Path.of(System.getProperty("java.home"), "bin/java").toString(), "-Duser.home=" + home, "-jar");
        Map<String, String> env = Map.of("HOME", home.toString(), "WITHLOG_VAULT", v.toString(), "WITHLOG_SIDECAR_SOCKET", socket.toString(), "WITHLOG_VAULT_AUTOCOMMIT_ENABLED", "false");
        List<String> command = new ArrayList<>(prefix); command.add(Path.of("dist/wl-engine.jar").toAbsolutePath().toString()); command.add("serve");
        ProcessBuilder builder = new ProcessBuilder(command); builder.environment().clear(); builder.environment().putAll(env);
        builder.redirectOutput(home.resolve("out.log").toFile()); builder.redirectError(home.resolve("err.log").toFile()); Process server = builder.start();
        try {
            long until = System.nanoTime() + TimeUnit.SECONDS.toNanos(10);
            while (!Files.exists(socket) && server.isAlive() && System.nanoTime() < until) Thread.sleep(25);
            check(Files.exists(socket), "template server ready: " + Files.readString(home.resolve("err.log")));
            var created = request(prefix, env, 0, "save_template", json(Map.of("name", "소켓 템플릿", "body", "원문\n둘째 줄"))).object();
            String id = created.get("id").toString();
            eq(request(prefix, env, 0, "get-template", id).object().get("body"), "원문\n둘째 줄", "socket UTF8 and multiline body");
            request(prefix, env, 0, "save-template", "id=" + id, "body=changed");
            request(prefix, env, 0, "save-ritual-template", "ritual", "name=소켓 모닝");
            eq(request(prefix, env, 0, "get_template", "{\"id\":\"ritual\"}").object().get("name"), "소켓 모닝", "socket ritual update");
            put(v, ".withlog/templates/doc/README.md", "broken");
            Reply response = request(prefix, env, 3, "list-templates", "bodies=yes");
            check(response.err.startsWith("템플릿 1건") && response.err.contains("\nWL-400 bodies"), "socket diagnostic precedes WL error");
            request(prefix, env, 0, "delete-doc-template", id); request(prefix, env, 3, "get-template", id);
        } finally {
            server.destroy(); if (!server.waitFor(5, TimeUnit.SECONDS)) server.destroyForcibly().waitFor(); Files.deleteIfExists(socket);
        }
    }
    private static Reply request(List<String> prefix, Map<String, String> env, int status, String... args) throws Exception {
        List<String> command = new ArrayList<>(prefix); command.add(Path.of("dist/wl-cli.jar").toAbsolutePath().toString()); command.addAll(List.of(args));
        ProcessBuilder builder = new ProcessBuilder(command); builder.environment().clear(); builder.environment().putAll(env); Process child = builder.start();
        try {
            check(child.waitFor(10, TimeUnit.SECONDS), "template CLI completes");
            Reply result = new Reply(new String(child.getInputStream().readAllBytes(), StandardCharsets.UTF_8), new String(child.getErrorStream().readAllBytes(), StandardCharsets.UTF_8));
            eq(child.exitValue(), status, "template CLI status: " + result.err); if (status != 0) eq(result.out, "", "failed CLI stdout empty"); return result;
        } finally { if (child.isAlive()) child.destroyForcibly().waitFor(); }
    }
}
