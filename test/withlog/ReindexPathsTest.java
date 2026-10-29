package withlog;

import withlog.cmd.Reindex;
import withlog.config.EngineConfig;
import withlog.engine.EngineExecutor;
import withlog.engine.EngineResult;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.*;

/** 8.5/8.6: 실제 임시 vault의 선택 범위와 진단, CLI 문법을 검증한다. */
public final class ReindexPathsTest {
    private static Path temp;
    private static int checks;
    public static void main(String[] args) throws Exception {
        temp = Files.createTempDirectory("aisurada-paths-test-");
        try {
            paths(); diagnostics(); boundaries();
            System.out.println("reindex-paths: " + checks + " checks passed");
        } finally {
            EngineConfig.activate(null);
            try (var stream = Files.walk(temp)) {
                for (Path p : stream.sorted(Comparator.reverseOrder()).toList()) Files.deleteIfExists(p);
            }
        }
    }
    private static Path vault() throws Exception {
        Path v = Files.createTempDirectory(temp, "vault-");
        write(v,".withlog/agendas/demo.md","---\nid: agfixture-00000001\nslug: demo\ntitle: 데모\n---\n");
        note(v,"notes/demo/a/note.md","ndfixture-00000001");
        note(v,"notes/demo/folder/b/note.md","ndfixture-00000002");
        note(v,"notes/demo/folder/c/note.md","ndfixture-00000003");
        return v;
    }
    private static void note(Path v, String path, String id) throws Exception {
        write(v,path,"---\nid: " + id + "\nagenda: demo\ntitle: 검사\ntype: free\ncreated: '2026-06-16T10:00:00+09:00'\n---\n본문\n");
    }
    private static void write(Path v, String path, String text) throws Exception {
        Path p = v.resolve(path); Files.createDirectories(p.getParent()); Files.writeString(p,text);
    }
    private static Map<String,Object> index(Path v, String... paths) {
        Map<String,Object> result = Reindex.paths(v,List.of(paths),System.currentTimeMillis());
        eq(result.keySet().stream().toList(),List.of("mode","elapsedMs","agendas","scanned","indexed","unchanged","removed","skipped","conflicts","unsharded"),"ordered envelope");
        eq(result.get("scanned"),result.get("indexed"),"stateless scanned count");
        eq(result.get("unchanged"),0,"stateless unchanged"); eq(result.get("removed"),0,"stateless removed");
        check(((Number)result.get("elapsedMs")).longValue() >= 0,"elapsed nonnegative");
        return result;
    }
    private static void paths() throws Exception {
        Path v = vault();
        eq(index(v).get("indexed"),0,"no paths");
        eq(index(v,"notes", "notes/", "NOTES/demo", "elsewhere", " ").get("indexed"),0,"ignored paths");
        eq(index(v,"\u00a0//notes/demo/a/note.md///\u00a0").get("indexed"),1,"trim and outer slashes");
        eq(index(v,"notes/demo/a/sub/missing/note.md", "notes/demo/a/attach/absent.png", "notes/demo/a").get("indexed"),1,"fold and deduplicate root");
        eq(index(v,"notes/demo/folder").get("indexed"),2,"ordinary folder descendants");
        eq(index(v,"notes/demo/folder/missing/note.md").get("indexed"),0,"missing middle does not ascend");
        eq(index(v,"notes/demo/a/missing/..").get("indexed"),1,"stop at root before remaining components");
        eq(index(v,"notes/demo/ /folder//b/note.md").get("indexed"),1,"discard blank components");
        eq(index(v,"notes/demo").get("indexed"),3,"agenda folder");
        eq(index(v,"notes/..").get("mode"),"paths","second-component parent accepted");
        Files.createSymbolicLink(v.resolve("notes/alias"),v.resolve("notes/demo"));
        eq(index(v,"notes/alias/a").get("indexed"),1,"agenda symlink followed");
        Files.createSymbolicLink(v.resolve("notes/demo/link"),v.resolve("notes/demo/a"));
        Map<String,Object> link=index(v,"notes/demo/link/note.md");
        eq(link.get("indexed"),0,"nested symlink skipped");
        check(link.get("skipped").toString().contains("심링크"),"symlink diagnostic");
        Path empty=Files.createTempDirectory(temp,"empty-");
        eq(index(empty,"notes/demo").get("agendas"),0,"empty vault requires no workspace");
    }
    private static void diagnostics() throws Exception {
        Path v=vault();
        write(v,"notes/demo/bad.md","no header");
        Files.createSymbolicLink(v.resolve("notes/demo/z-link"),v.resolve("notes/demo/a"));
        note(v,"notes/demo/conflicted copy/note.md","ndfixture-00000004");
        Map<String,Object> result=index(v,"notes/demo");
        String skipped=result.get("skipped").toString();
        check(skipped.indexOf("z-link") < skipped.indexOf("bad.md"),"collection diagnostics before opening diagnostics");
        check(result.get("conflicts").toString().contains("conflicted copy"),"conflict copied once");
        write(v,"notes/demo/poison/note.md","---\nmins: !!int abc\n---\n");
        eq(index(v,"notes/demo/a").get("indexed"),1,"unselected fatal note not opened");
        EngineResult fatal=run(v,"reindex-paths","notes/demo/poison");
        eq(fatal.status(),1,"selected fatal YAML fails the command");
        EngineResult r=run(v,"reindex-paths","notes/demo/poison","notes/demo/../escape");
        eq(r.errorCode(),400,"path validation precedes opening fatal YAML");
        check(new String(r.stderrBytes(),StandardCharsets.UTF_8).contains("기준 폴더 밖"),"path error reason");
        r=run(v,"reindex-paths","notes/demo/\u0000"); eq(r.errorCode(),400,"NUL rejected");
        r=run(v,"reindex-paths","notes/demo/\ud800"); eq(r.errorCode(),400,"unpaired surrogate rejected");
        Files.delete(v.resolve("notes/demo/poison/note.md"));
        EngineConfig.activate(new EngineConfig(v.toString(),v.toString(),null,true,10,true,300,true));
        result=index(v,"notes/demo/a","notes/demo/bad.md");
        eq(result.get("unsharded"),List.of("notes/demo/a","notes/demo/bad.md"),"unsharded includes invalid candidate");
        note(v,"notes/demo/2026-10/month/note.md","ndfixture-00000005");
        eq(index(v,"notes/demo/2026-10/month").get("unsharded"),List.of(),"monthly path accepted");
        EngineConfig.activate(null);
    }
    private static void boundaries() throws Exception {
        Path v=vault();
        String[] args=new String[200]; Arrays.fill(args,"");
        eq(index(v,args).get("mode"),"paths","200 raw positions");
        args=new String[201]; Arrays.fill(args,""); args[0]="notes/demo/../bad";
        Map<String,Object> result=index(v,args);
        eq(result.get("mode"),"full","201 positions bypass path validation");
        eq(result.get("indexed"),3,"fallback collects full notes");
        write(v,".withlog/templates/doc/broken.md","no header");
        result=index(v,args);eq(result.get("skipped"),List.of(),"fallback excludes template diagnostics");
        check(!Reindex.execute(v,System.currentTimeMillis()).get("skipped").equals(List.of()),"full reindex still includes template diagnostics");
        EngineResult r=run(v,"reindex-paths","abc=def"); eq(r.errorCode(),400,"unknown option");
        eq(new String(r.stderrBytes(),StandardCharsets.UTF_8),"WL-400 reindex-paths 이 모르는 옵션이다: abc. 쓸 수 있는 것은 [] 뿐이다.\n","option bytes");
        r=run(v,"reindex-paths","--unknown=x","notes/demo/a"); eq(r.status(),0,"double dash setting discarded");
        r=run(v,"reindex_paths"); eq(r.status(),2,"no tool surface");
    }
    private static EngineResult run(Path v,String... args) { return EngineExecutor.execute(v,args,false,System.currentTimeMillis()); }
    private static void check(boolean condition,String label) { checks++; if(!condition)throw new AssertionError(label); }
    private static void eq(Object a,Object b,String label) { check(Objects.equals(a,b),label+": "+a+" != "+b); }
}
