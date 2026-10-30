package withlog;

import withlog.args.JsonReader;
import withlog.args.JsonValue;
import withlog.engine.EngineExecutor;
import withlog.engine.EngineResult;
import withlog.out.JsonWriter;
import withlog.vault.CommentsReader;
import withlog.write.CommentAnchors;
import withlog.write.TestApparatus;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.time.Instant;
import java.util.*;

/** 외부 fixture나 실사용 vault 없이 실제 Java 엔진과 파일 바이트를 재는 19장 회귀 시험. */
public final class CommentCommandsTest {
    private static final String ROOT = "ndfixture-00000001", SUB = "snfixture-00000001", GRAND = "snfixture-00000006";
    private static final String ID = "cmfixture-0000000a", ID2 = "cmfixture-0000000b";
    private static final String AT = "2026-06-16T10:30:00.000Z", NOW = "2026-10-21T14:05:07+09:00";
    private static Path temp;
    private static int checks;

    public static void main(String[] args) throws Exception {
        TimeZone.setDefault(TimeZone.getTimeZone("Asia/Seoul"));
        System.setProperty("withlog.test.now", Long.toString(Instant.parse("2026-10-21T05:05:07Z").toEpochMilli()));
        temp = Files.createTempDirectory("aisurada-comments-test-");
        try {
            parsing(); envelopesAndTargets(); mutations(); deletion(); shapesAndFailures(); serveAndCli();
            System.out.println("chapter19: " + checks + " checks passed (isolated Java engine/files)");
        } finally {
            try (var paths = Files.walk(temp)) {
                for (Path p : paths.sorted(Comparator.reverseOrder()).toList()) Files.deleteIfExists(p);
            }
        }
    }

    private static Path vault() throws Exception {
        Path v = Files.createTempDirectory(temp, "vault-");
        write(v, ".withlog/workspace.json", "{\"schemaVersion\":1}\n");
        write(v, ".withlog/agendas/demo.md", "---\nid: agfixture-00000001\nslug: demo\ntitle: 데모\n---\n");
        write(v, "notes/demo/root/note.md", note(ROOT, "루트", "본문\n"));
        write(v, "notes/demo/root/sub/child/note.md", note(SUB, "자식", "자식 본문\n"));
        write(v, "notes/demo/root/sub/child/sub/grand/note.md", note(GRAND, "손자", "손자 본문\n"));
        return v;
    }

    private static String note(String id, String title, String body) {
        return "---\nid: " + id + "\nagenda: demo\ntitle: " + title + "\ntype: free\nmins: 30\ncreated: '2026-06-16T10:00:00+09:00'\n"
                + "status: done\nsessionId: old-session\neditedBy: app\neditedAt: 2026-06-16T11:00:00+09:00\n---\n" + body;
    }

    private static void write(Path v, String relative, String text) throws Exception {
        Path file = v.resolve(relative); Files.createDirectories(file.getParent()); Files.writeString(file, text);
    }
    private static Path comments(Path v) { return v.resolve("notes/demo/root/comments.json"); }
    private static Map<String, Object> item(String id, String at, String body) {
        Map<String, Object> row = new LinkedHashMap<>(); row.put("id", id); row.put("createdAt", at); row.put("body", body); return row;
    }
    private static void save(Path file, List<?> rows) throws Exception {
        Files.createDirectories(file.getParent());
        Map<String, Object> document = new LinkedHashMap<>(); document.put("v", 1); document.put("comments", rows);
        Files.write(file, JsonWriter.toFileBytes(document));
    }
    private static EngineResult run(Path v, String... args) { return EngineExecutor.execute(v, args, false, System.currentTimeMillis()); }
    private static Map<String, Object> ok(Path v, String... args) {
        EngineResult r = run(v, args); eq(r.status(), 0, "success " + Arrays.toString(args) + ": " + new String(r.stderrBytes(), StandardCharsets.UTF_8));
        eq(r.stdoutBytes()[r.stdoutBytes().length - 1], (byte) 10, "stdout final LF");
        return object(r.stdoutBytes());
    }
    private static void error(Path v, int status, int code, String message, String... args) {
        EngineResult r = run(v, args); eq(r.status(), status, "error status " + Arrays.toString(args));
        eq(r.errorCode(), code, "WL code"); eq(r.stdoutBytes().length, 0, "failure stdout empty");
        if (message != null) eq(new String(r.stderrBytes(), StandardCharsets.UTF_8), "WL-" + code + " " + message + "\n", "error text");
    }
    @SuppressWarnings("unchecked")
    private static Map<String, Object> object(byte[] raw) { return (Map<String, Object>) value(JsonReader.parseFile(new String(raw, StandardCharsets.UTF_8))); }
    private static Object value(JsonValue v) {
        if (v instanceof JsonValue.JsonString s) return s.value();
        if (v instanceof JsonValue.JsonArray a) return a.elements().stream().map(CommentCommandsTest::value).toList();
        if (v instanceof JsonValue.JsonObject o) { Map<String, Object> m = new LinkedHashMap<>(); o.properties().forEach((k,x) -> m.put(k,value(x))); return m; }
        String raw = ((JsonValue.JsonLiteral) v).raw();
        return switch (raw) { case "true" -> true; case "false" -> false; case "null" -> null; default -> Integer.valueOf(raw); };
    }
    @SuppressWarnings("unchecked")
    private static List<Map<String, Object>> rows(Map<String, Object> response) { return (List<Map<String, Object>>) response.get("comments"); }
    private static String sha(byte[] bytes) throws Exception { return "f1:" + HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(bytes)); }
    private static void eq(Object got, Object expected, String label) {
        checks++; if (!Objects.equals(got, expected)) throw new AssertionError(label + "\nexpected=" + expected + "\ngot=" + got);
    }

    private static void parsing() throws Exception {
        Path v = vault(), f = comments(v);
        for (String text : List.of("", " \t\r\n", "\ufeff{}", "\u00a0{}", "{id:\"x\"}", "'a b'", "[{},]", "{\"a\":1,}", "{} x", "{}0", "{\"a\":\"\\x41\"}")) {
            Files.writeString(f, text);
            eq(CommentsReader.readComments(v,f), Map.of("state","unreadable","reason","내용이 JSON 이 아니다: notes/demo/root/comments.json"), "invalid raw file " + text);
        }
        for (String text : List.of("hello", "NaN", "01", "true", "'x'", "42", "null", "\"글\"", "{\"comments\":false}", "[1,null,\"글\"]")) {
            Files.writeString(f,text); eq(CommentsReader.readComments(v,f), Map.of("state","loaded","comments",List.of()), "loaded scalar/empty " + text);
        }
        String raw = "[{\"id\":\"" + ID + "\",\"createdAt\":\" \",\"body\":\"실제\n탭\t끝\",\"editedAt\":\" \",\"anchor\":{\"quote\":\" \"}}]";
        Files.writeString(f,raw); var row=rows(CommentsReader.readComments(v,f)).getFirst();
        eq(row.get("body"),"실제\n탭\t끝","raw controls"); eq(row.get("editedAt")," ","blank editedAt"); eq(row.get("anchor"),Map.of("quote"," "),"blank quote");
        var malformed = item(ID, "", "discard"); var later = item(ID, AT, "also discarded");
        var uppercase = item(ID.toUpperCase(Locale.ROOT), " ", "");
        var withExtras = item(ID2,"z","한".repeat(19999) + "😀");
        withExtras.put("anchor",Map.of("quote","나".repeat(1999)+"😀","extra",1)); withExtras.put("extra",true);
        save(f,List.of(malformed,later,uppercase,withExtras));
        var parsed=rows(CommentsReader.readComments(v,f)); eq(parsed.size(),2,"claim id before rejecting candidate");
        eq(parsed.get(0).get("id"),ID.toUpperCase(Locale.ROOT),"case-sensitive identity and UTF16 order");
        eq(((String)parsed.get(1).get("body")).length(),20000,"UTF16 body cap");
        eq(((Map<?,?>)parsed.get(1).get("anchor")).get("quote"),"나".repeat(1999)+"\ud83d","UTF16 quote cap");
        eq(parsed.get(1).containsKey("extra"),false,"unknown file fields discarded");
        Files.write(f,new byte[]{(byte)0xff}); eq(CommentsReader.readComments(v,f).get("state"),"loaded","replacement decoded literal");
        Files.writeString(f,"{}"+" ".repeat(199998)); eq(CommentsReader.readComments(v,f).get("state"),"loaded","200000 bytes allowed");
        Files.writeString(f," ".repeat(200001)); eq(CommentsReader.readComments(v,f).get("reason"),"파일이 너무 크다(200001바이트). 상한은 200000바이트다.","file limit");
        Files.writeString(f,"{}\n");
        if (Files.getFileStore(f).supportsFileAttributeView("posix")) {
            var permissions=Files.getPosixFilePermissions(f);
            try {
                Files.setPosixFilePermissions(f,Set.of());
                if (!Files.isReadable(f)) {
                    eq(CommentsReader.readComments(v,f),Map.of("state","unreadable","reason","파일을 못 읽었다: notes/demo/root/comments.json"),"I/O unreadable reason");
                    error(v,3,409,null,"create-comment",ROOT,"body=쓰지 않음");
                }
            } finally { Files.setPosixFilePermissions(f,permissions); }
        }
        eq(JsonReader.parse(" ") instanceof JsonValue.JsonObject,true,"argument parser empty fallback preserved");
    }

    private static void envelopesAndTargets() throws Exception {
        Path v=vault();
        eq(ok(v,"list-comments",ROOT),Map.of("state","absent"),"absent");
        save(comments(v),List.of(item(ID,AT,"루트")));
        eq(rows(ok(v,"list_comments","{\"parent\":\""+ROOT+"// \"}")).getFirst().get("body"),"루트","tool blank segments");
        Path child=v.resolve("notes/demo/root/sub-comments/"+SUB+".json");
        Path grand=v.resolve("notes/demo/root/sub/child/sub-comments/"+GRAND+".json");
        save(child,List.of(item(ID2,AT,"자식"))); save(grand,List.of(item("cmfixture-0000000e",AT,"손자")));
        eq(rows(ok(v,"list-comments",ROOT,SUB)).getFirst().get("body"),"자식","direct container path");
        eq(rows(ok(v,"list_comments","{\"parent\":\""+ROOT+"/"+GRAND+"\"}")).getFirst().get("body"),"손자","grandchild with two pieces");
        var grandCreated=ok(v,"create-comment",ROOT,GRAND,"body=손자 쓰기","comment_id=cmfixture-0000000f");
        eq(grandCreated.get("path"),"notes/demo/root/sub/child/sub-comments/"+GRAND+".json","grandchild write in its direct container");
        ok(v,"update-comment",ROOT,GRAND,"comment_id=cmfixture-0000000f","body=손자 고침");
        eq(ok(v,"delete-comment",ROOT,GRAND,"comment_id=cmfixture-0000000f").get("markerStripped"),false,"grandchild delete without marker");
        eq(rows(ok(v,"list-comments",ROOT," \t")).size(),1,"blank second positional omitted");
        error(v,3,400,"인자 검증 실패(list_comments): parent 는 `노트id` 이거나 `노트id/하위노트id` 여야 한다: "+ROOT+"/"+SUB+"/"+GRAND,
                "list_comments","{\"parent\":\""+ROOT+"/"+SUB+"/"+GRAND+"\"}");
        for (String parent : List.of("", "/ /")) {
            error(v,3,400,"인자 검증 실패(list_comments): parent 는 `노트id` 이거나 `노트id/하위노트id` 여야 한다: "+parent,
                    "list_comments","{\"parent\":\""+parent+"\"}");
        }
        error(v,3,400,null,"list_comments","{\"parent\":null}");
        error(v,3,400,null,"list_comments","{\"parent\":\""+ROOT+"\",\"body\":\"x\"}");
        error(v,3,400,null,"list-comments",ROOT,SUB,GRAND);
        error(v,3,404,"SubNote not found: snfixture-00000099","list-comments",ROOT,"snfixture-00000099");
        error(v,3,400,"노트 id 모양이 아니다: "+ROOT+"/"+SUB,"list-comments",ROOT+"/"+SUB);
        String root=Files.readString(v.resolve("notes/demo/root/note.md"));
        Files.writeString(v.resolve("notes/demo/root/note.md"),root.replace("status: done","tier: record"));
        eq(ok(v,"list_comments","{\"parent\":\""+ROOT+"\"}").get("state"),"loaded","no tier gate");
    }

    private static void mutations() throws Exception {
        Path v=vault(), f=comments(v); byte[] before=Files.readAllBytes(v.resolve("notes/demo/root/note.md"));
        var made=ok(v,"create-comment",ROOT,"body= 원문\n줄 ","anchor_quote= 인용문 ","comment_id= "+ID+" ");
        eq(made,new LinkedHashMap<>(Map.of("id",ID,"path","notes/demo/root/comments.json","total",1)),"create response");
        String expected="{\n  \"v\": 1,\n  \"comments\": [\n    {\n      \"id\": \""+ID+"\",\n      \"createdAt\": \""+NOW+"\",\n      \"anchor\": {\n        \"quote\": \" 인용문 \"\n      },\n      \"body\": \" 원문\\n줄 \"\n    }\n  ]\n}\n";
        eq(Files.readString(f),expected,"create exact file bytes/key order/LF");
        ok(v,"update-comment",ROOT,"comment_id="+ID,"body=고친 글");
        var updated=rows(object(Files.readAllBytes(f))).getFirst();
        eq(new ArrayList<>(updated.keySet()),List.of("id","createdAt","editedAt","anchor","body"),"update key order");
        eq(updated.get("createdAt"),NOW,"createdAt unchanged"); eq(updated.get("editedAt"),NOW,"editedAt current");
        eq(updated.get("anchor"),Map.of("quote"," 인용문 "),"anchor unchanged");
        eq(Arrays.equals(before,Files.readAllBytes(v.resolve("notes/demo/root/note.md"))),true,"create/update body untouched");
        error(v,3,409,"이미 그 식별자를 쓰는 코멘트가 있다: "+ID,"create-comment",ROOT,"comment_id="+ID,"body=중복");
        ok(v,"create-comment",ROOT,"comment_id="+ID.toUpperCase(Locale.ROOT),"body=대문자");
        eq(rows(ok(v,"list-comments",ROOT)).size(),2,"case-only ids coexist");
        ok(v,"create-comment",ROOT,"comment_id=hello","body=보이지 않는 id");
        eq(rows(ok(v,"list-comments",ROOT)).size(),2,"malformed create id absent next read");
        ok(v,"update-comment",ROOT,"comment_id="+ID,"body=새 글"); eq(Files.readString(f).contains("hello"),false,"next rewrite removes invalid id");
        save(f,List.of(item(ID,"2099-01-01T00:00:00+09:00","미래")));
        ok(v,"update-comment",ROOT,"comment_id="+ID,"body=시계 역행"); eq(rows(object(Files.readAllBytes(f))).getFirst().get("editedAt"),"2099-01-01T00:00:00+09:00","25-char clock clamp");
        save(f,List.of(item(ID,"2099-01-01T00:00:00.000Z","앱 미래")));
        ok(v,"update-comment",ROOT,"comment_id="+ID,"body=시계 역행"); eq(rows(object(Files.readAllBytes(f))).getFirst().get("editedAt"),NOW,"Z milliseconds not clamped");
        Files.write(f,JsonWriter.toFileBytes(List.of(item(ID,AT,"옛 배열"))));
        ok(v,"create-comment",ROOT,"body=추가","comment_id="+ID2); eq(Files.readString(f).startsWith("{\n  \"v\": 1,"),true,"legacy array normalized");
        save(f,List.of(item(ID,AT,"x".repeat(19999)+"😀")));
        ok(v,"create-comment",ROOT,"body=추가","comment_id="+ID2);
        eq(Files.readString(f).contains("\\ud83d"),true,"file escapes cut surrogate");
        eq(((String)rows(ok(v,"list-comments",ROOT)).getFirst().get("body")).endsWith("?"),true,"stdout surrogate becomes question mark");
        Files.delete(f); TestApparatus.resetRandomQueue(); System.setProperty("withlog.test.random","5e6f7a8b"); TestApparatus.resetRandomQueue();
        String generated=(String)ok(v,"create-comment",ROOT,"body=발급").get("id");
        eq(generated,"cm"+Long.toString(TestApparatus.nowMillis(),36)+"-5e6f7a8b","engine id generation");
        System.clearProperty("withlog.test.random"); TestApparatus.resetRandomQueue();
        Path target=v.resolve("outside.json"); Files.move(f,target); Files.createSymbolicLink(f,target);
        ok(v,"update-comment",ROOT,"comment_id="+generated,"body=링크 갱신");
        eq(Files.isSymbolicLink(f),true,"comment symlink retained"); eq(Files.readString(target).contains("링크 갱신"),true,"symlink target rewritten");
        TimeZone.setDefault(TimeZone.getTimeZone("UTC"));
        try {
            ok(v,"update-comment",ROOT,"comment_id="+generated,"body=UTC 시계");
            eq(rows(object(Files.readAllBytes(target))).getFirst().get("editedAt"),NOW,"25-char clamp also applies across wall clocks");
            ok(v,"create-comment",ROOT,"comment_id="+ID2,"body=UTC 생성");
            eq(rows(object(Files.readAllBytes(target))).getFirst().get("createdAt"),"2026-10-21T05:05:07+09:00","process wall clock with fixed offset label preserved");
        } finally { TimeZone.setDefault(TimeZone.getTimeZone("Asia/Seoul")); }
    }

    private static void deletion() throws Exception {
        Path v=vault(), f=comments(v), body=v.resolve("notes/demo/root/note.md");
        String marked="<!--WLC:"+ID+"-->앞<!--/WLC:"+ID+"-->뒤\n```md\n<!--wlc:"+ID+"-->펜스\n```\n"
                +"<!--wlc:"+ID.toUpperCase(Locale.ROOT)+"-->다른 id<!--/wlc:"+ID.toUpperCase(Locale.ROOT)+"-->\n";
        Files.writeString(body,note(ROOT,"루트",marked)); save(f,List.of(item(ID,AT,"삭제")));
        byte[] before=Files.readAllBytes(body);
        var deleted=ok(v,"delete-comment",ROOT,"comment_id="+ID);
        eq(new ArrayList<>(deleted.keySet()),List.of("id","path","remaining","markerStripped","bodyBefore","bodyAfter"),"delete response order");
        eq(deleted.get("bodyBefore"),sha(before),"before hash"); eq(deleted.get("bodyAfter"),sha(Files.readAllBytes(body)),"after hash");
        String after=Files.readString(body); eq(after.contains("앞뒤"),true,"marker removal inserts no spaces");
        eq(after.contains("```md\n펜스\n```"),true,"half marker in fence removed");
        eq(after.contains("<!--wlc:"+ID.toUpperCase(Locale.ROOT)+"-->"),true,"literal id comparison");
        eq(after.contains("status:"),false,"unknown frontmatter removed"); eq(after.contains("sessionId: old-session"),true,"old session retained");
        eq(after.contains("editedAt: '2026-06-16T11:00:00+09:00'"),true,"old stamp retained"); eq(Files.exists(f),false,"last comment file deleted");
        eq(CommentAnchors.strip("<!-- wlc:"+ID+" -->"+"<!--wlc:"+ID+"0-->",ID),"<!-- wlc:"+ID+" -->"+"<!--wlc:"+ID+"0-->","strict marker spelling and id length");
        save(f,List.of(item(ID,AT,"앵커 없음"))); before=Files.readAllBytes(body);
        eq(ok(v,"delete-comment",ROOT,"comment_id="+ID),Map.of("id",ID,"path","notes/demo/root/comments.json","remaining",0,"markerStripped",false),"no marker response");
        eq(Arrays.equals(before,Files.readAllBytes(body)),true,"no marker no body write");
        Files.writeString(body,note(ROOT,"루트","<!--wlc:"+ID+"-->없는 코멘트")); before=Files.readAllBytes(body);
        error(v,3,409,"그 코멘트가 없다: "+ID,"delete-comment",ROOT,"comment_id="+ID);
        eq(Arrays.equals(before,Files.readAllBytes(body)),true,"missing comment leaves marker");
        Path childFile=v.resolve("notes/demo/root/sub-comments/"+SUB+".json");
        save(childFile,List.of(item(ID2,AT,"하위")));
        Path childBody=v.resolve("notes/demo/root/sub/child/note.md"); Files.writeString(childBody,note(SUB,"자식","<!--wlc:"+ID2+"-->자식<!--/wlc:"+ID2+"-->"));
        var subDeleted=ok(v,"delete-comment",ROOT,SUB,"comment_id="+ID2);
        eq(subDeleted.get("path"),"notes/demo/root/sub-comments/"+SUB+".json","sub comment stays in direct container");
        eq(Files.exists(v.resolve("notes/demo/root/sub/2026-06-16-1000-자식-0001/note.md")),true,"derived sub body write renames folder");
        eq(Files.isDirectory(childFile.getParent()),true,"empty sub-comments directory retained");
        write(v,"notes/demo/root/sub/legacy.md",note("snfixture-00000009","옛 하위","<!--wlc:"+ID2+"-->원문"));
        Path legacyComments=v.resolve("notes/demo/root/sub-comments/snfixture-00000009.json");
        save(legacyComments,List.of(item(ID2,AT,"옛 하위 코멘트")));
        byte[] legacyBefore=Files.readAllBytes(legacyComments);
        error(v,3,500,null,"delete-comment",ROOT,"snfixture-00000009","comment_id="+ID2);
        eq(Arrays.equals(legacyBefore,Files.readAllBytes(legacyComments)),true,"legacy sub write failure keeps comment");
    }

    private static void shapesAndFailures() throws Exception {
        Path v=vault(), f=comments(v); byte[] body=Files.readAllBytes(v.resolve("notes/demo/root/note.md"));
        for (String raw : List.of("{bad",""," ".repeat(200001))) {
            Files.writeString(f,raw);
            for (String[] args : List.of(new String[]{"create-comment",ROOT,"body=새 글"},new String[]{"update-comment",ROOT,"comment_id="+ID,"body=고침"},new String[]{"delete-comment",ROOT,"comment_id="+ID})) {
                error(v,3,409,null,args); eq(Files.readString(f),raw,"unreadable file untouched");
                eq(Arrays.equals(body,Files.readAllBytes(v.resolve("notes/demo/root/note.md"))),true,"unreadable body untouched");
            }
        }
        // 옵션 검사는 코멘트 파일 읽기보다 앞이다.
        byte[] unreadableBefore=Files.readAllBytes(f);
        for (String blank : List.of("", " ", " \t", "\r\n", "\u00a0\u2003")) {
            error(v,3,400,"코멘트 본문이 비었다.","create-comment",ROOT,"body="+blank);
            error(v,3,400,"코멘트 본문이 비었다.","update-comment",ROOT,"comment_id="+ID,"body="+blank);
            eq(Arrays.equals(unreadableBefore,Files.readAllBytes(f)),true,"blank body preserves comments");
            eq(Arrays.equals(body,Files.readAllBytes(v.resolve("notes/demo/root/note.md"))),true,"blank body preserves note");
        }
        error(v,3,400,"코멘트 본문이 너무 길다(20001자). 상한은 20000자다.","create-comment",ROOT,"body="+"x".repeat(20001));
        error(v,3,400,"앵커 인용문이 너무 길다. 상한은 2000자다.","create-comment",ROOT,"body=x","anchor_quote="+" ".repeat(2001));
        error(v,3,400,null,"update-comment",ROOT,"body=x"); error(v,3,400,null,"create-comment",ROOT);
        error(v,3,400,null,"delete-comment",ROOT,"comment_id="+ID,"f1=anything");
        error(v,3,400,null,"update-comment",ROOT,"comment_id="+ID,"body=x","anchor_quote=x");
        Files.delete(f);
        for (String format:List.of("json","html")) {
            Files.move(v.resolve("notes/demo/root/note.md"),v.resolve("notes/demo/root/note."+format));
            ok(v,"create-comment",ROOT,"body=비 markdown","comment_id="+ID);
            byte[] previous=Files.readAllBytes(f);
            error(v,3,409,"본문이 markdown 이 아니라 코멘트 앵커를 걷어낼 수 없다(포맷="+format.toUpperCase(Locale.ROOT)+"): "+ROOT,"delete-comment",ROOT,"comment_id="+ID);
            eq(Arrays.equals(previous,Files.readAllBytes(f)),true,"format rejection preserves comments");
            Files.delete(f); Files.move(v.resolve("notes/demo/root/note."+format),v.resolve("notes/demo/root/note.md"));
        }
        write(v,"notes/demo/legacy.md",note("ndfixture-00000007","단일 파일","본문"));
        error(v,3,500,"폴더를 만들지 못했다: "+v.resolve("notes/demo/legacy.md"),"create-comment","ndfixture-00000007","body=글");
        error(v,3,409,"그 코멘트가 없다: "+ID,"delete-comment","ndfixture-00000007","comment_id="+ID);
        // 쓰기 상한을 새로 만들지 않는다. 한글 20,000자 코멘트 4개 뒤에는 읽기 상한이 막는다.
        for(int n=0;n<4;n++) ok(v,"create-comment",ROOT,"body="+"한".repeat(20000),"comment_id=cmfixture-0000000"+n);
        eq(Files.size(f)>200000,true,"writer has no file size cap");
        eq(ok(v,"list-comments",ROOT).get("state"),"unreadable","oversize after successful writes");
        error(v,3,409,null,"update-comment",ROOT,"comment_id=cmfixture-00000000","body=줄임");
        Files.delete(f);
        Files.writeString(f,"[".repeat(10000)+"]".repeat(10000));
        error(v,1,0,null,"list-comments",ROOT);
        Files.delete(f);
        Files.delete(v.resolve(".withlog/workspace.json"));
        error(v,3,400,null,"create-comment",ROOT,"body=글");
        error(v,2,0,null,"create_comment","{}");
    }

    /** 진짜 serve와 wl-cli 사이의 요청/응답 프레임으로 5개 이름을 모두 실행한다. launchd는 호출하지 않는다. */
    private static void serveAndCli() throws Exception {
        Path v=vault();
        Path home=Files.createTempDirectory(temp,"home-");
        // macOS 유닉스 소켓 경로 길이 상한을 피한다.
        Path socket=Path.of("/tmp/asc-"+UUID.randomUUID().toString().substring(0,8)+".sock");
        Path engine=Path.of("dist/wl-engine.jar").toAbsolutePath(), cli=Path.of("dist/wl-cli.jar").toAbsolutePath();
        String javaExecutable=Path.of(System.getProperty("java.home"),"bin/java").toString();
        List<String> prefix=List.of(javaExecutable,"-Duser.home="+home,"-Dwithlog.test.now="+TestApparatus.nowMillis(),"-jar");
        var args=new ArrayList<>(prefix); args.add(engine.toString()); args.add("serve");
        ProcessBuilder builder=new ProcessBuilder(args);
        builder.environment().clear(); builder.environment().putAll(Map.of("HOME",home.toString(),"TZ","Asia/Seoul","WITHLOG_VAULT",v.toString(),"WITHLOG_SIDECAR_SOCKET",socket.toString()));
        builder.redirectOutput(home.resolve("stdout.log").toFile()); builder.redirectError(home.resolve("stderr.log").toFile());
        Process server=builder.start();
        try {
            long until=System.nanoTime()+java.util.concurrent.TimeUnit.SECONDS.toNanos(10);
            while(!Files.exists(socket) && server.isAlive() && System.nanoTime()<until) Thread.sleep(25);
            eq(Files.exists(socket),true,"serve socket ready: "+Files.readString(home.resolve("stderr.log")));
            var requests=List.of(
                    new String[]{"create-comment",ROOT,"comment_id="+ID,"body=소켓 원문\n줄","anchor_quote=본문"},
                    new String[]{"list_comments","{\"parent\":\""+ROOT+"\"}"},
                    new String[]{"list-comments",ROOT},
                    new String[]{"update-comment",ROOT,"comment_id="+ID,"body=소켓 고침"},
                    new String[]{"delete-comment",ROOT,"comment_id="+ID});
            for(String[] request:requests) {
                var command=new ArrayList<>(prefix); command.add(cli.toString()); command.addAll(Arrays.asList(request));
                var process=new ProcessBuilder(command); process.environment().clear(); process.environment().putAll(builder.environment());
                Process child=process.start();
                try {
                    eq(child.waitFor(10,java.util.concurrent.TimeUnit.SECONDS),true,"wl-cli completed");
                    byte[] stdout=child.getInputStream().readAllBytes(), stderr=child.getErrorStream().readAllBytes();
                    eq(child.exitValue(),0,"wl-cli "+request[0]+": "+new String(stderr,StandardCharsets.UTF_8));
                    var response=object(stdout);
                    if(request[0].startsWith("list")) eq(rows(response).getFirst().get("body"),"소켓 원문\n줄","socket UTF8 body");
                    else eq(response.get("id"),ID,"socket id");
                    eq(stdout[stdout.length-1],(byte)10,"socket stdout final LF");
                } finally { if(child.isAlive()) child.destroyForcibly().waitFor(); }
            }
            eq(Files.exists(comments(v)),false,"socket delete removed file");
        } finally {
            server.destroy();
            if(!server.waitFor(5,java.util.concurrent.TimeUnit.SECONDS)) server.destroyForcibly().waitFor();
            Files.deleteIfExists(socket);
        }
    }
}
