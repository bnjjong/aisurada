package withlog.git;

import withlog.config.EngineConfig;
import withlog.engine.EngineExecutor;
import withlog.engine.EngineResult;

import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.attribute.FileTime;
import java.time.*;
import java.util.*;

/** 실제 git 커밋과 시계 주입으로 작성자 전환·복구·공통 쓰기 차례를 검증한다. */
public final class VaultGitTest {
    private static Path temp;
    private static int checks;
    private static final String ID="ndfixture-00000001";
    private static final byte[] EMPTY=new byte[0];

    public static void main(String[] args) throws Exception {
        if (args.length > 0 && args[0].equals("process-probe")) { processProbe(Path.of(args[1])); return; }
        temp=Files.createTempDirectory("aisurada-git-test-");
        try {
            commits(); blocking(); retries(); ids(); pipeline(); factsPipeline(); process();
            System.out.println("vault git: "+checks+" checks passed");
        } finally {
            EngineConfig.activate(null);
            try(var stream=Files.walk(temp)) {
                for(Path p:stream.sorted(Comparator.reverseOrder()).toList()) Files.deleteIfExists(p);
            }
        }
    }
    private static final class TestClock extends Clock {
        Instant now=Instant.parse("2026-10-29T01:00:00Z");
        public ZoneId getZone(){return ZoneOffset.UTC;}
        public Clock withZone(ZoneId z){return this;}
        public Instant instant(){return now;}
        void advance(long seconds){now=now.plusSeconds(seconds);}
    }
    private static Path repo() throws Exception {
        Path v=Files.createTempDirectory(temp,"vault-");
        git(v,"init","-b","main");
        write(v,".withlog/workspace.json","{}");
        write(v,".withlog/agendas/demo.md","---\nid: agfixture-00000001\nslug: demo\ntitle: 데모\n---\n");
        write(v,"notes/demo/root/note.md",note(ID,"본문"));
        git(v,"add","-A"); git(v,"commit","-qm","baseline"); return v;
    }
    private static String note(String id,String body){return "---\nid: "+id+"\nagenda: demo\ntitle: 검사\ntype: free\ncreated: '2026-06-16T10:00:00+09:00'\n---\n"+body+"\n";}
    private static void write(Path v,String relative,String text) throws Exception {
        Path p=v.resolve(relative);Files.createDirectories(p.getParent());Files.writeString(p,text);
    }
    private static String git(Path v,String... args) throws Exception {
        List<String> cmd=new ArrayList<>(List.of("git","-c","core.hooksPath=/dev/null","-c","commit.gpgsign=false"));
        cmd.addAll(List.of(args));
        ProcessBuilder b=new ProcessBuilder(cmd).directory(v.toFile()).redirectErrorStream(true);
        b.environment().remove("GIT_INDEX_FILE"); b.environment().put("GIT_DIR",v.resolve(".git").toString()); b.environment().put("GIT_WORK_TREE",v.toString());
        b.environment().put("GIT_AUTHOR_NAME","Fixture");b.environment().put("GIT_AUTHOR_EMAIL","fixture@example.invalid");
        b.environment().put("GIT_COMMITTER_NAME","Fixture");b.environment().put("GIT_COMMITTER_EMAIL","fixture@example.invalid");
        Process p=b.start();String out=new String(p.getInputStream().readAllBytes(),StandardCharsets.UTF_8);
        if(p.waitFor()!=0)throw new AssertionError(cmd+": "+out);return out;
    }
    private static int count(Path v) throws Exception {return Integer.parseInt(git(v,"rev-list","--count","HEAD").trim());}
    private static String message(Path v) throws Exception {return git(v,"show","-s","--format=%B","HEAD").stripTrailing()+"\n";}
    private static void outcome(VaultGit.Result r,String outcome,String blocked){eq(r,new VaultGit.Result(outcome,blocked),"commit result");}
    private static void commits() throws Exception {
        Path v=repo(); TestClock clock=new TestClock(); VaultGit g=new VaultGit(v,true,300,clock,null);
        write(v,".git/hooks/post-commit","#!/bin/sh\ntouch hook-ran\n");
        check(v.resolve(".git/hooks/post-commit").toFile().setExecutable(true),"hook executable");
        git(v,"config","commit.gpgsign","true");git(v,"config","gpg.program","/usr/bin/false");
        write(v,"notes/demo/root/note.md",note(ID,"외부 편집")); write(v,"a/deep/half.tmp","temporary");
        outcome(g.commit("startup"),"committed",null);
        check(!Files.exists(v.resolve("hook-ran")),"post-commit hook disabled; signing config overridden");
        eq(message(v),"wl(outside): 노트 1개, 파일 1개\n\nWithlog-Writer: outside\nWithlog-Reason: startup\nWithlog-Notes: "+ID+"\n","startup bytes");
        eq(git(v,"show","-s","--format=%an <%ae>|%cn <%ce>","HEAD").trim(),"withlog-auto <noreply@withlog.local>|withlog-auto <noreply@withlog.local>","author and committer");
        check(git(v,"status","--porcelain").contains("a/"),"tmp remains unstaged");
        write(v,"notes/demo/root/note.md",note(ID,"도구 편집")); g.afterWrite(true,"session-A");
        int before=count(v);g.beforeWrite(true,"session-A");eq(count(v),before,"same pair no commit");
        g.beforeWrite(true,"session-B");eq(count(v),before+1,"different tool session commits");
        check(message(v).contains("Withlog-Session: session-A\n"),"old session in writer commit");
        write(v,"notes/demo/root/note.md",note(ID,"다음 편집"));g.afterWrite(true,"session-B");
        g.beforeWrite(false,"ignored-app-session");
        check(message(v).contains("Withlog-Reason: writer\n"),"app transition reason");
        write(v,"notes/demo/root/note.md",note(ID,"앱 편집"));g.afterWrite(false,"ignored-app-session");
        before=count(v);clock.advance(299);g.tick();eq(count(v),before,"idle before threshold");
        clock.advance(1);g.tick();eq(count(v),before+1,"idle threshold");
        check(!message(v).contains("Withlog-Session"),"app session omitted");
        write(v,"other","continuous");g.afterWrite(false,null);before=count(v);
        for(int i=0;i<3;i++){clock.advance(299);g.afterWrite(false,null);g.tick();}
        eq(count(v),before,"continuous edits not idle");clock.advance(3);g.tick();eq(count(v),before+1,"15 minute overdue");
        check(message(v).contains("Withlog-Reason: overdue"),"overdue message");
        outcome(g.commit("shutdown"),"nothing",null);
        write(v,"last","shutdown");outcome(g.commit("shutdown"),"committed",null);
        check(message(v).contains("wl(outside)"),"pending cleared after success");
    }
    private static void blocking() throws Exception {
        Path v=repo();TestClock clock=new TestClock();VaultGit g=new VaultGit(v,true,0,clock,null);
        write(v,"changed","x");g.afterWrite(false,null);
        Path lock=v.resolve(".git/index.lock");Files.writeString(lock,"");Files.setLastModifiedTime(lock,FileTime.from(clock.instant()));
        outcome(g.commit("startup"),"skipped","index-lock");
        Files.setLastModifiedTime(lock,FileTime.from(clock.instant().plusSeconds(1)));outcome(g.commit("startup"),"skipped","index-lock");
        Files.setLastModifiedTime(lock,FileTime.from(clock.instant().minusSeconds(600)));outcome(g.commit("startup"),"skipped","index-lock");
        Files.setLastModifiedTime(lock,FileTime.from(clock.instant().minusSeconds(601)));outcome(g.commit("shutdown"),"committed",null);
        check(!Files.exists(lock),"stale lock removed");
        for(String marker:List.of("rebase-merge","rebase-apply","MERGE_HEAD","CHERRY_PICK_HEAD","REVERT_HEAD")){
            Path p=v.resolve(".git/"+marker);Files.writeString(p,"");outcome(g.commit("startup"),"skipped","in-progress");Files.delete(p);
        }
        git(v,"checkout","-qb","other");outcome(g.commit("shutdown"),"skipped","not-main");
        git(v,"checkout","-q","--detach");outcome(g.commit("shutdown"),"skipped","not-main");
        VaultGit off=new VaultGit(v,false,0,clock,(a,b)->{throw new AssertionError("disabled ran git");});
        off.afterWrite(true,"x");off.tick();outcome(off.commit("startup"),"disabled","off");
        Path none=Files.createTempDirectory(temp,"not-repo-");
        outcome(new VaultGit(none,true,0,clock,null).commit("startup"),"disabled","not-repo");
        Files.writeString(none.resolve(".git"),"gitdir: elsewhere");outcome(new VaultGit(none,true,0,clock,null).commit("startup"),"disabled","not-repo");
    }
    private static void retries() throws Exception {
        Path v=repo();TestClock clock=new TestClock();List<String> calls=new ArrayList<>();List<String> messages=new ArrayList<>();
        boolean[] fail={true};
        VaultGit.Runner runner=(a,b)->{
            calls.add(a.get(0));
            if(a.get(0).equals("symbolic-ref"))return result(77," refs/heads/main\n"); // exit ignored
            if(a.get(0).equals("add"))return result(fail[0]?1:0,"");
            if(a.get(0).equals("diff"))return result(0,"A\u0000file\u0000");
            if(a.get(0).equals("commit"))messages.add(new String(b,StandardCharsets.UTF_8));
            return result(0,"");
        };
        VaultGit g=new VaultGit(v,true,0,clock,runner);g.afterWrite(false,null);
        for(long wait:new long[]{30,60,120,240,480,480}){
            int n=calls.size();g.tick();eq(calls.size(),n+2,"failed add attempted");
            clock.advance(wait-1);g.beforeWrite(true,"new");g.tick();eq(calls.size(),n+2,"retry backoff");clock.advance(1);
        }
        fail[0]=false;g.afterWrite(true,"new");g.tick();
        check(messages.get(0).startsWith("wl(app,tool): 노트 0개, 파일 1개\n"),"mixed writers retained in order");
        check(messages.get(0).contains("Withlog-Session: new\n"),"session retained after failures");
        g.afterWrite(false,null);fail[0]=true;int n=calls.size();g.tick();eq(calls.size(),n+2,"failure counter reset");clock.advance(30);g.tick();eq(calls.size(),n+4,"reset backoff is 30 seconds");
        // 강제 시작/종료는 미룬 시각보다 앞서도 시도한다.
        n=calls.size();g.commit("shutdown");eq(calls.size(),n+2,"shutdown bypasses retry gate");
        Path lock=v.resolve(".git/index.lock");Files.writeString(lock,"");Files.setLastModifiedTime(lock,FileTime.from(clock.instant()));
        g.commit("startup");Files.delete(lock);n=calls.size();clock.advance(29);g.tick();eq(calls.size(),n,"skipped retry 30 sec");clock.advance(1);g.tick();eq(calls.size(),n+2,"retry after skip");
    }
    private static void ids() throws Exception {
        Path v=repo();TestClock clock=new TestClock();VaultGit g=new VaultGit(v,true,0,clock,null);
        Files.delete(v.resolve("notes/demo/root/note.md"));
        write(v,"notes/demo/renamed/note.md",note(ID,"같은 id 이동"));
        write(v,"notes/demo/bom/note.md","\ufeff"+note("bom","x"));
        write(v,"notes/demo/json/note.json",note("json","x"));
        g.afterWrite(true,"one");outcome(g.commit("shutdown"),"committed",null);
        check(message(v).startsWith("wl(tool): 노트 1개, 파일 4개\n"),"deleted HEAD id deduplicated with new path; BOM/json excluded");
        for(String text:List.of("---\nid: good\n---\n", "---\nid: '\"한글_1-\"'\n---\n"))check(VaultGit.headerId(text.getBytes(StandardCharsets.UTF_8))!=null,"header id accepted");
        for(String text:List.of("\ufeff---\nid: good\n---\n", "---\n id: good\n---\n", "---\nid: x y\n---\n", "---\nid: good\n", "---\nid: bad!\nid: good\n---\n"))eq(VaultGit.headerId(text.getBytes(StandardCharsets.UTF_8)),null,"header id rejected");
        for(int i=0;i<201;i++)write(v,"bulk/"+i+"/note.md","---\nid: note_"+i+"\n---\n");
        outcome(g.commit("startup"),"committed",null);String msg=message(v);
        check(msg.startsWith("wl(outside): 노트 201개, 파일 201개\n"),"full note count");
        eq(msg.substring(msg.indexOf("Withlog-Notes: ")+15).trim().split(" ").length,200,"notes trailer capped");
    }
    private static void pipeline() throws Exception {
        Path v=repo();EngineConfig.activate(new EngineConfig(v.toString(),v.toString(),null,false,10,true,300,true));
        write(v,"changed","syntax");run(v,400,"create-comment","bad-option=x");
        VaultGit.serving(v).commit("shutdown");check(message(v).contains("Withlog-Writer: outside"),"syntax failure not tracked");
        write(v,"changed","workspace");Files.delete(v.resolve(".withlog/workspace.json"));run(v,400,"create-comment","body=x");
        write(v,".withlog/workspace.json","{}");VaultGit.serving(v).commit("shutdown");check(message(v).contains("Withlog-Writer: outside"),"workspace failure not tracked");
        write(v,"notes/demo/poison/note.md","---\nmins: !!int abc\n---\n");
        EngineResult failed=EngineExecutor.execute(v,new String[]{"create-comment","bad-id","body=x"},true,System.currentTimeMillis());
        eq(failed.status(),3,"reconcile failure returned");Files.delete(v.resolve("notes/demo/poison/note.md"));
        write(v,"changed","reconcile");VaultGit.serving(v).commit("shutdown");check(message(v).contains("Withlog-Writer: outside"),"reconcile failure not tracked");
        write(v,"changed","semantic");run(v,400,"create-comment","bad-id","body=x");
        int n=count(v);run(v,400,"create_doc","{}");eq(count(v),n,"tool schema validation does not commit");
        run(v,400,"create_doc","{\"agenda\":\"demo\",\"type\":\"bad\",\"title\":\"x\",\"body\":\"x\"}","\u0000wl-session-id","raw");
        eq(count(v),n+1,"semantic failure marked before next writer");check(message(v).contains("Withlog-Writer: app"),"previous app writer");
        write(v,"changed","part4");run(v,409,"append-chat-turn");
        check(message(v).contains("Withlog-Session: raw"),"raw tool session committed before part4");
        write(v,"changed","after-part4");
        VaultGit.serving(v).commit("shutdown");check(message(v).contains("Withlog-Writer: app"),"unavailable part4 write tracked");
        Path one=repo();write(one,"changed","one-shot");
        EngineResult r=EngineExecutor.execute(one,new String[]{"create-comment","bad-id","body=x"},false,System.currentTimeMillis());eq(r.errorCode(),400,"oneshot semantic failure");
        VaultGit.serving(one).commit("startup");check(message(one).contains("Withlog-Writer: outside"),"oneshot pending not shared with serve");
        EngineConfig.activate(null);
    }
    private static void process() throws Exception {
        Path v=repo();GitProcess p=new GitProcess(v);outcome(new VaultGit(v,true,0,new TestClock(),p).commit("startup"),"nothing",null);
        // 출력이 파이프 용량보다 커도 동시에 비워서 끝나야 한다.
        String data="x".repeat(150_000);write(v,"large",data);git(v,"add","large");git(v,"commit","-qm","large");
        VaultGit.CommandResult result=p.run(List.of("show","HEAD:large"),EMPTY);eq(result.code(),0,"large output exit");eq(result.output().length,150_000,"large output drained");
        Path bin=Files.createTempDirectory(temp,"fake-git-");
        Path executable=bin.resolve("git");
        Files.writeString(executable,"#!/bin/sh\nfor arg do\n  if [ \"$arg\" = wait ]; then trap '' TERM; exec /bin/sleep 30; fi\ndone\nprintf '%s\\n' \"$GIT_DIR\" \"$GIT_WORK_TREE\" \"${GIT_INDEX_FILE-removed}\" \"$*\"\n");
        check(executable.toFile().setExecutable(true),"fake git executable");
        ProcessBuilder child=new ProcessBuilder(Path.of(System.getProperty("java.home"),"bin/java").toString(),"-cp",System.getProperty("java.class.path"),VaultGitTest.class.getName(),"process-probe",v.toString()).redirectErrorStream(true);
        child.environment().put("PATH",bin+":"+System.getenv("PATH"));
        child.environment().put("GIT_DIR","/elsewhere/.git");child.environment().put("GIT_WORK_TREE","/elsewhere");child.environment().put("GIT_INDEX_FILE","/elsewhere/index");
        Process probe=child.start();
        boolean done=probe.waitFor(10,java.util.concurrent.TimeUnit.SECONDS);
        if(!done)probe.destroyForcibly();check(done,"timeout probe terminates");
        String output=new String(probe.getInputStream().readAllBytes(),StandardCharsets.UTF_8);
        eq(probe.exitValue(),0,"isolated git process checks: "+output);
    }
    private static void factsPipeline() throws Exception {
        Path v=repo();EngineConfig.activate(new EngineConfig(v.toString(),v.toString(),null,false,10,true,300,true));
        int before=count(v);
        run(v,0,"facts-brief","from=2026-06-16","to=2026-06-25");
        eq(git(v,"status","--porcelain"),"","brief writes no vault files");eq(count(v),before,"brief creates no immediate commit");
        outcome(VaultGit.serving(v).commit("shutdown"),"nothing",null);
        run(v,0,"facts-brief","from=2026-06-16","to=2026-06-25");
        write(v,"outside","external change after brief");
        run(v,400,"create_doc","{\"agenda\":\"demo\",\"type\":\"bad\",\"title\":\"x\",\"body\":\"x\"}","\u0000wl-session-id","facts-session");
        eq(count(v),before+1,"brief pending writer triggers next tool transition");
        check(message(v).contains("Withlog-Writer: app\n"),"brief records app writer");
        VaultGit.serving(v).commit("shutdown"); // 도구의 의미 검사 실패 기록을 비운다.
        write(v,"outside","external change before get");run(v,0,"get-agenda-facts","agfixture-00000001");
        VaultGit.serving(v).commit("shutdown");check(message(v).contains("Withlog-Writer: outside\n"),"get facts is not a write");
        run(v,0,"update-agenda-facts","agfixture-00000001","facts=approved facts");
        outcome(VaultGit.serving(v).commit("shutdown"),"committed",null);
        check(message(v).contains("Withlog-Writer: app\n"),"update facts records app writer");
        eq(git(v,"show","HEAD:.withlog/facts/demo.md"),"approved facts\n","facts committed as file bytes");
        run(v,400,"facts-brief","min-new-file=0");write(v,"outside","semantic brief failure");
        VaultGit.serving(v).commit("shutdown");check(message(v).contains("Withlog-Writer: app\n"),"semantic brief failure still records app writer");
        EngineConfig.activate(null);
    }
    private static void processProbe(Path v) {
        GitProcess process=new GitProcess(v);
        VaultGit.CommandResult env=process.run(List.of("probe"),EMPTY);
        eq(env.code(),0,"environment probe exit");
        eq(env.text(),v.resolve(".git")+"\n"+v+"\nremoved\n-c core.hooksPath=/dev/null -c commit.gpgsign=false -c core.quotePath=false probe\n","git env isolation and literal argv");
        long before=System.nanoTime();
        eq(new GitProcess(v,Duration.ofMillis(150),Duration.ofMillis(50)).run(List.of("wait"),EMPTY).code(),-1,"timeout is failure");
        check(Duration.ofNanos(System.nanoTime()-before).compareTo(Duration.ofSeconds(5))<0,"TERM then KILL bounds ignored TERM");
    }
    private static EngineResult run(Path v,int code,String... argv){EngineResult r=EngineExecutor.execute(v,argv,true,System.currentTimeMillis());eq(r.errorCode(),code,"pipeline result "+Arrays.toString(argv)+": "+new String(r.stderrBytes(),StandardCharsets.UTF_8));return r;}
    private static VaultGit.CommandResult result(int code,String text){return new VaultGit.CommandResult(code,text.getBytes(StandardCharsets.UTF_8));}
    private static void check(boolean condition,String label){checks++;if(!condition)throw new AssertionError(label);}
    private static void eq(Object actual,Object expected,String label){check(Objects.equals(actual,expected),label+": "+actual+" != "+expected);}
}
