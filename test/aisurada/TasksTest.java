package aisurada;

import aisurada.cli.AisCliMain;
import aisurada.cmd.CommandRegistry;
import aisurada.engine.EngineResult;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Map;

import static aisurada.ChapterTestSupport.*;

/**
 * 태스크(티켓)의 시험입니다. 외부 설명서에 없는 확장이라, 새 영역({@code .aisurada/tasks/})이 기존 명령과 공존하는지,
 * 완료 기준이 필수인지, 상태 전이의 규칙과 조건(누가, 무엇이 차야, 막혀 있지 않아야)이 지켜지는지, 연결의 방향 계산, 고치기 보호,
 * 활동 기록, 손으로 망가뜨린 파일, MCP에는 읽기 도구만 열리는지를 잰다.
 */
public final class TasksTest {

    private static final String SID_MARK = "\u0000ais-session-id";
    private static final String AGENT_MARK = "\u0000ais-session-agent";
    private static final String CRIT = "criteria=[\"지운 뒤 자원 폴더가 남지 않는다\",{\"text\":\"되살리면 폴더도 돌아온다\"}]";

    public static void main(String[] args) throws Exception {
        // 0. 엔진이 받는 명령 이름은 모두 ais-cli가 넘겨 주어야 한다(앱은 ais-cli를 거쳐 엔진을 부른다)
        var forwarded = new java.util.HashSet<String>(AisCliMain.ENGINE_COMMANDS_61);
        forwarded.addAll(AisCliMain.ENGINE_COMMANDS_TASKS);
        forwarded.addAll(AisCliMain.ENGINE_COMMANDS_APPROVALS);
        for (String name : CommandRegistry.COMMAND_NAMES) {
            check(forwarded.contains(name), "ais-cli가 엔진 명령 " + name + " 을(를) 넘겨 주지 않는다");
        }

        try (var t = new ChapterTestSupport("tasks")) {
            Path v = t.vault();
            ok(v, "create-project", "title=앱", "repos=[{\"name\":\"app\",\"github\":\"bnjjong/aisurada\"}]");

            // 1. 만들기: Draft로 시작하고, 키와 ID, 파일 모양, 기본값
            var a = ok(v, "create-task", "title=프로젝트를 지울 때 자원 폴더도 정리하기", "repo=bnjjong/aisurada", CRIT, "type=bug", "priority=high",
                    "project=앱", "labels=[\"엔진\",\"휴지통\"]", "body=지우면 폴더가 남아요.",
                    "refs=[{\"kind\":\"note\",\"target\":\"ndmw37ezqe-6234ac9a\",\"title\":\"작업 계획\"},{\"kind\":\"github\",\"target\":\"bnjjong/aisurada#53\"}]");
            eq(a.get("key"), "TSK-1", "첫 키");
            String aid = (String) a.get("id");
            check(aid.matches("tk[0-9a-z]+-[0-9a-f]{8}"), "id 는 tk 머리: " + aid);
            eq(a.get("status"), "draft", "새 태스크는 Draft");
            eq(a.get("path"), ".aisurada/tasks/TSK-1.md", "파일 위치");
            eq(a.get("criteriaTotal"), 2L, "완료 기준 개수");
            eq(a.get("criteriaDone"), 0L, "확인된 기준");
            String file = Files.readString(v.resolve(".aisurada/tasks/TSK-1.md"));
            check(file.startsWith("---\nkey: TSK-1\nid: " + aid + "\ntitle: 프로젝트를 지울 때 자원 폴더도 정리하기\ntype: bug\npriority: high\nstatus: draft\nrepo: bnjjong/aisurada\nproject: 앱\n"), "머리말 앞부분: " + file);
            check(file.contains("criteria:\n  - text: 지운 뒤 자원 폴더가 남지 않는다\n    done: false\n  - text: 되살리면 폴더도 돌아온다\n    done: false\n"), "완료 기준은 들여쓴 목록: " + file);
            check(file.endsWith("---\n지우면 폴더가 남아요.\n"), "본문");
            var got = ok(v, "get-task", "TSK-1");
            eq(got.get("body"), "지우면 폴더가 남아요.", "본문은 그대로 돌아온다");
            eq(got.get("ready"), true, "요약, 완료 기준, 저장소가 있어 Ready 조건이 찬다");
            check(((String) got.get("precondition")).startsWith("f1:"), "f1 토큰");
            eq(ok(v, "get-task", aid).get("key"), "TSK-1", "id 로도 읽는다");
            check(String.valueOf(got.get("createdAt")).startsWith("2026-"), "만든 시각");

            // 2. 틀린 값은 쓰기 전에 AIS-400/404로 거절한다. 완료 기준 없이는 만들 수 없다(필수)
            String[][] bad = {
                    {"title=x|repo=a/b", "완료 기준"},
                    {"title=x|repo=a/b|criteria=[]", "완료 기준"},
                    {"title=|repo=a/b|" + CRIT, "title 이 필요"},
                    {"repo=a/b|" + CRIT, "title 이 필요"},
                    {"title=x|" + CRIT, "repo 가 필요"},
                    {"title=x|repo=abc|" + CRIT, "owner/name"},
                    {"title=x|repo=a/b|" + CRIT + "|type=epic", "type 은"},
                    {"title=x|repo=a/b|" + CRIT + "|priority=urgent", "priority 는"},
                    {"title=x|repo=a/b|" + CRIT + "|labels=abc", "JSON"},
                    {"title=x|repo=a/b|" + CRIT + "|labels={}", "JSON 배열"},
                    {"title=x|repo=a/b|criteria=[1]", "글이나 객체"},
                    {"title=x|repo=a/b|criteria=[{\"text\":\"\"}]", "1~"},
                    {"title=x|repo=a/b|criteria=[{\"text\":\"a\",\"done\":\"yes\"}]", "true 나 false"},
                    {"title=x|repo=a/b|criteria=[{\"txt\":\"a\"}]", "모르는 키"},
                    {"title=x|repo=a/b|" + CRIT + "|refs=[{\"kind\":\"url\",\"target\":\"http://x\"}]", "refs 항목이 틀렸다"},
                    {"title=x|repo=a/b|" + CRIT + "|refs=[{\"kind\":\"note\",\"target\":\"abc\"}]", "refs 항목이 틀렸다"},
                    {"title=x|repo=a/b|" + CRIT + "|base=-bad", "브랜치"},
                    {"title=x|repo=a/b|" + CRIT + "|actor=boss", "actor"},
            };
            for (String[] c : bad) {
                var r = error(v, 400, concat("create-task", c[0].split("\\|")));
                check(stderr(r).contains(c[1]), "거절 문구에 '" + c[1] + "': " + c[0] + " → " + stderr(r));
            }
            error(v, 404, "create-task", "title=x", "repo=a/b", CRIT, "project=없는프로젝트");
            check(stderr(error(v, 400, "create-task", "title=x", "repo=a/b", CRIT, "모르는옵션=1")).contains("모르는"), "모르는 옵션");
            eq(ok(v, "list-tasks").get("tasks") instanceof List<?> l ? l.size() : -1, 1, "거절된 만들기는 파일을 남기지 않는다");

            // 3. 고치기 보호: f1 이 없거나 어긋나면 AIS-412(종료 코드 5), 같은 값이면 바뀐 게 없다, 모르는 머리말 칸은 보존한다
            String tok = (String) got.get("precondition");
            EngineResult noToken = run(v, "update-task", "TSK-1", "title=바꾼 제목");
            eq(noToken.status(), 5, "f1 이 없으면 종료 코드 5");
            eq(noToken.errorCode(), 412, "f1 이 없으면 AIS-412");
            eq(run(v, "update-task", "TSK-1", "f1=f1:" + "0".repeat(64), "title=바꾼 제목").errorCode(), 412, "어긋난 f1 은 AIS-412");
            byte[] before = Files.readAllBytes(v.resolve(".aisurada/tasks/TSK-1.md"));
            var same = ok(v, "update-task", "TSK-1", "f1=" + tok, "title=프로젝트를 지울 때 자원 폴더도 정리하기", "priority=high");
            eq(same.get("changed"), false, "같은 값이면 바뀐 게 없다");
            check(java.util.Arrays.equals(before, Files.readAllBytes(v.resolve(".aisurada/tasks/TSK-1.md"))), "파일 바이트가 그대로");
            Path p1 = v.resolve(".aisurada/tasks/TSK-1.md");
            Files.writeString(p1, Files.readString(p1).replace("status: draft\n", "status: draft\nmyNote: 손으로 적은 칸\n"));
            tok = (String) ok(v, "get-task", "TSK-1").get("precondition");
            var edited = ok(v, "update-task", "TSK-1", "f1=" + tok, "title=자원 폴더도 같이 정리하기", "priority=medium", "base=develop", "labels=[\"엔진\"]");
            eq(edited.get("changed"), true, "고쳐졌다");
            eq(edited.get("title"), "자원 폴더도 같이 정리하기", "새 제목");
            eq(edited.get("baseBranch"), "develop", "기준 브랜치");
            check(Files.readString(p1).contains("myNote: 손으로 적은 칸\n"), "모르는 머리말 칸은 다시 써도 남는다");
            check(stderr(error(v, 400, "update-task", "TSK-1", "f1=" + edited.get("precondition"), "title=")).contains("title 이 필요"), "빈 제목 거절");
            error(v, 404, "update-task", "TSK-99", "f1=" + tok, "title=x");
            check(stderr(error(v, 400, "get-task", "abc")).contains("모양이 아니다"), "키 모양 검사");
            error(v, 404, "get-task", "TSK-99");

            // 4. 완료 기준을 확인하면 시각과 쪽이 남고, 확인이 풀리면 기록이 지워진다. 이미 확인된 항목은 기록을 그대로 둔다
            tok = (String) ok(v, "get-task", "TSK-1").get("precondition");
            var ticked = ok(v, "update-task", "TSK-1", "f1=" + tok, "criteria=[{\"text\":\"지운 뒤 자원 폴더가 남지 않는다\",\"done\":true},\"되살리면 폴더도 돌아온다\"]");
            eq(ticked.get("criteriaDone"), 1L, "확인된 기준 1");
            var c0 = list0(ticked, "criteria").get(0);
            eq(c0.get("done"), true, "첫 기준 확인");
            check(String.valueOf(c0.get("checkedAt")).startsWith("2026-"), "확인한 시각");
            eq(c0.get("checkedBy"), "human", "확인한 쪽");
            var again = ok(v, "update-task", "TSK-1", "f1=" + ticked.get("precondition"), "criteria=[{\"text\":\"지운 뒤 자원 폴더가 남지 않는다\",\"done\":true},{\"text\":\"되살리면 폴더도 돌아온다\",\"done\":true}]", "actor=runner");
            eq(list0(again, "criteria").get(0).get("checkedBy"), "human", "이미 확인된 항목의 기록은 그대로");
            eq(list0(again, "criteria").get(1).get("checkedBy"), "runner", "새로 확인한 항목은 확인한 쪽이 남는다");
            var unticked = ok(v, "update-task", "TSK-1", "f1=" + again.get("precondition"), "criteria=[\"지운 뒤 자원 폴더가 남지 않는다\",\"되살리면 폴더도 돌아온다\"]");
            check(!list0(unticked, "criteria").get(0).containsKey("checkedAt"), "확인이 풀리면 기록이 지워진다");

            // 5. 번호: 지운 번호를 다시 쓰지 않는다. 지우기는 휴지통이고 되살릴 수 있다
            var b = ok(v, "create-task", "title=휴지통 비우기 전에 개수 보여 주기", "repo=bnjjong/aisurada", "criteria=[\"개수가 보인다\"]");
            eq(b.get("key"), "TSK-2", "두 번째 키");
            var del = ok(v, "delete-task", "TSK-2");
            eq(del.get("trashed"), true, "휴지통으로");
            check(!Files.exists(v.resolve(".aisurada/tasks/TSK-2.md")), "원래 자리에는 없다");
            var c = ok(v, "create-task", "title=검색어 따옴표", "repo=bnjjong/aisurada", "criteria=[\"결과가 나온다\"]", "type=bug");
            eq(c.get("key"), "TSK-3", "지운 번호를 다시 쓰지 않는다");
            eq(list0(ok(v, "list-tasks", "trash=true"), "trash").get(0).get("key"), "TSK-2", "휴지통 목록");
            eq(ok(v, "restore-task", "TSK-2").get("restored"), true, "되살린다");
            check(Files.exists(v.resolve(".aisurada/tasks/TSK-2.md")), "되살린 파일");
            error(v, 404, "restore-task", "TSK-2");

            // 6. 상태 전이: 누가, 무엇이 차야. Draft → Ready 는 사람, 조건은 요약·완료 기준·저장소
            eq(ok(v, "transition-task", "TSK-1", "to=ready-for-develop").get("status"), "ready-for-develop", "Draft → Ready 는 사람이 한다");
            eq(ok(v, "transition-task", "TSK-1", "to=draft").get("status"), "draft", "Ready → Draft 는 사람이 한다");
            tok = (String) ok(v, "get-task", "TSK-1").get("precondition");
            ok(v, "update-task", "TSK-1", "f1=" + tok, "criteria=[]");
            var noCrit = ok(v, "get-task", "TSK-1");
            eq(noCrit.get("ready"), false, "완료 기준을 지우면 Ready 조건이 안 찬다");
            var opt = list0(noCrit, "transitions").stream().filter(o -> "ready-for-develop".equals(o.get("to"))).findFirst().get();
            eq(opt.get("ok"), false, "메뉴에서 막힌 전이");
            check(String.valueOf(opt.get("reason")).contains("criteria"), "까닭에 빠진 조건: " + opt.get("reason"));
            check(stderr(error(v, 409, "transition-task", "TSK-1", "to=ready-for-develop")).contains("criteria"), "완료 기준 없이 Ready 로 못 간다");
            tok = (String) ok(v, "get-task", "TSK-1").get("precondition");
            ok(v, "update-task", "TSK-1", "f1=" + tok, "criteria=[\"되살리면 폴더도 돌아온다\"]");
            ok(v, "transition-task", "TSK-1", "to=ready-for-develop", "from=draft");
            check(stderr(error(v, 409, "transition-task", "TSK-1", "to=ready-for-develop", "from=draft")).contains("이미 바뀌었다"), "from 이 지금 상태와 다르면 거절");
            // 사람이 못 하는 전이, 러너만 하는 전이
            check(stderr(error(v, 409, "transition-task", "TSK-1", "to=in-review")).contains("바꿀 수 없다"), "Ready → In Review 는 규칙에 없다");
            eq(ok(v, "transition-task", "TSK-1", "to=in-progress", "actor=runner").get("status"), "in-progress", "Ready → In Progress 는 러너가 한다");
            check(stderr(error(v, 409, "transition-task", "TSK-1", "to=waiting-answer")).contains("runner"), "In Progress → Waiting 은 사람이 못 한다");
            eq(ok(v, "transition-task", "TSK-1", "to=waiting-answer", "actor=runner").get("status"), "waiting-answer", "질문이 올라오면 Waiting Answer");
            eq(ok(v, "transition-task", "TSK-1", "to=in-progress").get("status"), "in-progress", "사람이 답하면 In Progress");
            check(stderr(error(v, 409, "transition-task", "TSK-1", "to=in-review")).contains("runner"), "In Progress → In Review 는 사람이 못 한다");
            eq(ok(v, "transition-task", "TSK-1", "to=in-review", "actor=runner").get("status"), "in-review", "초안 PR이 열리면 In Review");
            eq(ok(v, "transition-task", "TSK-1", "to=done").get("status"), "done", "In Review → Done 은 지금 사람이 누른다");
            check(stderr(error(v, 409, "transition-task", "TSK-1", "to=draft")).contains("바꿀 수 없다"), "Done 에서는 어디로도 못 간다");
            check(stderr(error(v, 409, "transition-task", "TSK-1", "to=cancelled")).contains("바꿀 수 없다"), "Done 은 취소할 수 없다");

            // Failed, Cancelled 와 되돌리기
            eq(ok(v, "transition-task", "TSK-2", "to=cancelled").get("status"), "cancelled", "Draft 도 사람이 취소한다");
            eq(ok(v, "transition-task", "TSK-2", "to=draft").get("status"), "draft", "Cancelled → Draft 는 사람이 되돌린다");
            ok(v, "transition-task", "TSK-3", "to=ready-for-develop");
            check(stderr(error(v, 409, "transition-task", "TSK-3", "to=failed")).contains("runner"), "Failed 는 러너가 한다");
            eq(ok(v, "transition-task", "TSK-3", "to=failed", "actor=runner").get("status"), "failed", "한도를 넘으면 Failed");
            eq(ok(v, "transition-task", "TSK-3", "to=draft").get("status"), "draft", "Failed → Draft 는 사람이 되돌린다");
            // 인자 오류
            check(stderr(error(v, 400, "transition-task", "TSK-3", "to=잠자는")).contains("상태 이름"), "모르는 상태");
            check(stderr(error(v, 400, "transition-task", "TSK-3")).contains("to 가 필요"), "to 필수");
            check(stderr(error(v, 400, "transition-task", "TSK-3", "to=draft", "actor=boss")).contains("actor"), "모르는 actor");
            check(stderr(error(v, 400, "transition-task", "TSK-3", "to=draft", "from=잠자는")).contains("상태 이름"), "모르는 from");
            eq(ok(v, "transition-task", "TSK-3", "to=Ready For Develop").get("status"), "ready-for-develop", "화면 이름으로도 읽는다");

            // 7. 연결: 건 쪽에만 적고 반대쪽은 읽을 때 계산한다. 막혀 있으면 러너가 못 집는다
            var l1 = ok(v, "link-tasks", "TSK-2", "TSK-3", "type=blocks");
            eq(l1.get("changed"), true, "연결을 걸었다");
            eq(types(ok(v, "get-task", "TSK-2")), List.of("blocks:TSK-3"), "건 쪽에는 blocks");
            eq(types(ok(v, "get-task", "TSK-3")), List.of("blocked-by:TSK-2"), "받은 쪽에는 blocked-by 가 계산된다");
            check(!Files.readString(v.resolve(".aisurada/tasks/TSK-3.md")).contains("links:"), "받은 쪽 파일에는 연결이 안 적힌다");
            eq(list0(ok(v, "list-tasks", "blocked=true"), "tasks").get(0).get("key"), "TSK-3", "막힌 것만 거르기");
            eq(ok(v, "link-tasks", "TSK-2", "TSK-3", "type=blocks").get("changed"), false, "다시 걸어도 그대로");
            check(stderr(error(v, 409, "transition-task", "TSK-3", "to=in-progress", "actor=runner")).contains("TSK-2"), "막혀 있어 시작하지 못한다");
            var blockedOpt = list0(ok(v, "get-task", "TSK-3"), "transitions").stream().filter(o -> "in-progress".equals(o.get("to"))).findFirst().get();
            eq(blockedOpt.get("ok"), false, "메뉴에서도 막힌 전이로 보인다");
            check(stderr(error(v, 400, "link-tasks", "TSK-3", "TSK-2", "type=blocks")).contains("서로 막게"), "서로 막는 순환은 거절");
            check(stderr(error(v, 400, "link-tasks", "TSK-2", "TSK-2", "type=blocks")).contains("자기 자신"), "자기 자신에게는 못 건다");
            check(stderr(error(v, 400, "link-tasks", "TSK-2", "TSK-3", "type=likes")).contains("type 은"), "모르는 종류");
            error(v, 404, "link-tasks", "TSK-2", "TSK-99", "type=blocks");
            check(stderr(error(v, 400, "link-tasks", "TSK-2", "TSK-3")).contains("type 이 필요"), "type 필수");
            // 막은 쪽이 끝나면 풀린다(Done 만 푼다. Failed·Cancelled 는 풀지 않는다)
            eq(ok(v, "transition-task", "TSK-2", "to=ready-for-develop").get("status"), "ready-for-develop", "TSK-2 Ready");
            check(stderr(error(v, 409, "transition-task", "TSK-3", "to=in-progress", "actor=runner")).contains("TSK-2"), "막은 쪽이 Done 이 아니면 계속 막힌다");
            ok(v, "transition-task", "TSK-2", "to=in-progress", "actor=runner");
            ok(v, "transition-task", "TSK-2", "to=in-review", "actor=runner");
            ok(v, "transition-task", "TSK-2", "to=done");
            eq(list0(ok(v, "list-tasks"), "tasks").stream().filter(r -> "TSK-3".equals(r.get("key"))).findFirst().get().get("blockedBy"), List.of(), "막은 쪽이 Done 이면 풀린다");
            eq(ok(v, "transition-task", "TSK-3", "to=in-progress", "actor=runner").get("status"), "in-progress", "이제 시작할 수 있다");
            // 관련·중복, 푸는 법(어느 쪽에서든)
            ok(v, "link-tasks", "TSK-1", "TSK-3", "type=relates");
            eq(ok(v, "link-tasks", "TSK-3", "TSK-1", "type=relates").get("changed"), false, "관련은 서로 한 번이면 된다");
            check(types(ok(v, "get-task", "TSK-3")).contains("relates:TSK-1"), "관련은 양쪽에 보인다");
            ok(v, "link-tasks", "TSK-3", "TSK-1", "type=duplicates");
            check(types(ok(v, "get-task", "TSK-1")).contains("duplicated-by:TSK-3"), "중복의 반대쪽 이름");
            eq(ok(v, "link-tasks", "TSK-3", "TSK-1", "type=relates", "remove=true").get("changed"), true, "받은 쪽에서 풀어도 지워진다");
            check(!types(ok(v, "get-task", "TSK-3")).contains("relates:TSK-1"), "관련이 풀렸다");
            eq(ok(v, "link-tasks", "TSK-3", "TSK-1", "type=relates", "remove=true").get("changed"), false, "없는 연결을 풀면 그대로");
            // 지운 태스크가 막던 것은 풀린다. 되살리면 다시 막는다
            ok(v, "create-task", "title=막힐 일", "repo=bnjjong/aisurada", "criteria=[\"끝낸다\"]");
            ok(v, "create-task", "title=막는 일", "repo=bnjjong/aisurada", "criteria=[\"끝낸다\"]");
            ok(v, "link-tasks", "TSK-5", "TSK-4", "type=blocks");
            eq(strs(ok(v, "get-task", "TSK-4"), "blockedBy"), List.of("TSK-5"), "TSK-5 가 TSK-4 를 막는다");
            var d5 = ok(v, "delete-task", "TSK-5");
            eq(strs(d5, "wasBlocking"), List.of("TSK-4"), "지울 때 막던 태스크를 알려 준다");
            eq(strs(ok(v, "get-task", "TSK-4"), "blockedBy"), List.of(), "지우면 막지 않는다");
            ok(v, "restore-task", "TSK-5");
            eq(strs(ok(v, "get-task", "TSK-4"), "blockedBy"), List.of("TSK-5"), "되살리면 다시 막는다");

            // 8. 활동: 만들기, 상태 바뀜, 고침, 연결, 댓글이 시간순으로 남는다
            ok(v, "add-task-comment", "TSK-1", "body=러너가 읽는 댓글");
            var act = list0(ok(v, "get-task", "TSK-1"), "activity");
            eq(act.get(0).get("kind"), "created", "첫 활동은 만들기");
            check(act.stream().anyMatch(e -> "status".equals(e.get("kind")) && "ready-for-develop".equals(e.get("to")) && "draft".equals(e.get("from"))), "상태 바뀜에는 from 과 to");
            check(act.stream().anyMatch(e -> "edited".equals(e.get("kind")) && ((List<?>) e.get("fields")).contains("criteria")), "고침에는 바뀐 칸 이름");
            eq(act.get(act.size() - 1).get("kind"), "comment", "마지막은 댓글");
            eq(act.get(act.size() - 1).get("body"), "러너가 읽는 댓글", "댓글 본문");
            eq(list0(ok(v, "get-task", "TSK-1", "activity=1"), "activity").size(), 1, "activity=1 은 가장 최근 하나");
            check(list0(ok(v, "get-task", "TSK-1", "activity=0"), "activity").size() >= act.size(), "activity=0 은 전부");
            check(stderr(error(v, 400, "add-task-comment", "TSK-1", "body=")).contains("body 가 필요"), "빈 댓글 거절");
            check(stderr(error(v, 400, "get-task", "TSK-1", "activity=-1")).contains("0 이상"), "음수 거절");
            check(act.stream().anyMatch(e -> "linked".equals(e.get("kind")) && "TSK-3".equals(e.get("with"))), "연결도 활동에 남는다");

            // 9. 기록: 사람이 고치면 시각만, AI 세션이 고치면 세션 ID(와 Claude 가 아닌 CLI 이름)가 남는다
            tok = (String) ok(v, "get-task", "TSK-4").get("precondition");
            var byAi = ok(v, "update-task", "TSK-4", "f1=" + tok, "title=AI 가 고친 제목", SID_MARK, "codex-thread-9", AGENT_MARK, "codex");
            eq(byAi.get("editedSession"), "codex-thread-9", "세션 ID 기록");
            eq(byAi.get("editedAgent"), "codex", "CLI 이름 기록");
            var byHuman = ok(v, "update-task", "TSK-4", "f1=" + byAi.get("precondition"), "title=사람이 고친 제목");
            check(!byHuman.containsKey("editedSession") && !byHuman.containsKey("editedAgent"), "사람이 고치면 세션 기록이 없다");

            // 10. 목록 거르기와 정렬: 키가 큰(새) 것이 앞이다
            var all = list0(ok(v, "list-tasks"), "tasks");
            eq(all.stream().map(r -> (String) r.get("key")).toList(), List.of("TSK-5", "TSK-4", "TSK-3", "TSK-2", "TSK-1"), "새 것이 앞");
            eq(list0(ok(v, "list-tasks", "status=done"), "tasks").size(), 2, "status 거르기");
            eq(list0(ok(v, "list-tasks", "status=In Progress"), "tasks").size(), 1, "화면 이름으로도 거른다");
            eq(list0(ok(v, "list-tasks", "type=bug"), "tasks").size(), 2, "type 거르기(TSK-1, TSK-3)");
            eq(list0(ok(v, "list-tasks", "label=엔진"), "tasks").size(), 1, "label 거르기");
            eq(list0(ok(v, "list-tasks", "q=tsk-3"), "tasks").size(), 1, "키로 찾기");
            eq(list0(ok(v, "list-tasks", "q=고친 제목"), "tasks").size(), 1, "요약에서 찾기");
            eq(list0(ok(v, "list-tasks", "repo=other/repo"), "tasks").size(), 0, "저장소 거르기");
            check(!all.get(0).containsKey("body"), "목록은 설명을 안 싣는다");
            check(list0(ok(v, "list-tasks", "bodies=true"), "tasks").get(0).containsKey("body"), "bodies=true 면 설명도");
            check(stderr(error(v, 400, "list-tasks", "status=잠자는")).contains("status"), "모르는 상태로 거르면 거절");

            // 11. 손으로 망가뜨린 파일은 목록에서 건너뛰고 이유를 알린다. 나머지는 그대로 읽는다
            put(v, ".aisurada/tasks/TSK-77.md", "머리말이 없다\n");
            put(v, ".aisurada/tasks/TSK-78.md", "---\nkey: TSK-79\nid: tkmwzzzzzz-00000000\ntitle: 이름이 다르다\nstatus: draft\n---\n");
            put(v, ".aisurada/tasks/TSK-80.md", "---\nkey: TSK-80\nid: tkmwyyyyyy-00000000\ntitle: 상태를 모른다\nstatus: sleeping\n---\n");
            var withSkipped = ok(v, "list-tasks");
            eq(list0(withSkipped, "tasks").size(), 5, "멀쩡한 태스크는 그대로");
            check(((List<?>) withSkipped.get("skipped")).size() == 3, "건너뛴 파일 3개: " + withSkipped.get("skipped"));
            check(String.valueOf(withSkipped.get("skipped")).contains("머리말(---)로 시작하지 않는다"), "이유가 적힌다");
            Files.delete(v.resolve(".aisurada/tasks/TSK-77.md"));
            Files.delete(v.resolve(".aisurada/tasks/TSK-78.md"));
            Files.delete(v.resolve(".aisurada/tasks/TSK-80.md"));

            // 12. MCP 표면: 읽기 도구 2개만 있다. 쓰는 도구는 없다
            var viaTool = ok(v, "list_tasks", "{\"status\":\"done\"}");
            eq(list0(viaTool, "tasks").size(), 2, "list_tasks");
            eq(ok(v, "get_task", "{\"id\":\"TSK-1\",\"activity\":2}").get("key"), "TSK-1", "get_task");
            eq(list0(ok(v, "get_task", "{\"id\":\"TSK-1\",\"activity\":2}"), "activity").size(), 2, "get_task 의 activity");
            check(stderr(error(v, 400, "list_tasks", "{\"status\":1}")).contains("문자열"), "도구 인자 검사");
            check(stderr(error(v, 400, "get_task", "{\"id\":\"TSK-1\",\"x\":1}")).contains("모르는 키"), "모르는 키");
            check(stderr(error(v, 400, "get_task", "{}")).contains("id 가 필요"), "id 필수");
            eq(run(v, "create_task", "{}").status(), 2, "create_task 도구는 없다");
            eq(run(v, "transition_task", "{}").status(), 2, "transition_task 도구는 없다");

            // 13. 기존 명령과 공존한다: 새 영역이 있어도 노트·프로젝트·아젠다·휴지통 명령이 그대로 돌고, empty-trash 는 태스크 휴지통을 안 비운다
            ok(v, "create-doc", "agenda=demo", "type=free", "title=태스크가 있는 vault의 노트", "body=본문");
            check(success(v, "list-docs") != null, "list-docs");
            check(!list(v, "list-projects").isEmpty(), "list-projects");
            success(v, "list-trash");
            check(success(v, "check-notes") != null, "check-notes");
            ok(v, "delete-task", "TSK-4");
            success(v, "empty-trash");
            eq(ok(v, "restore-task", "TSK-4").get("restored"), true, "empty-trash 는 태스크 휴지통을 안 비운다");
            System.out.println("TasksTest OK (" + checks + " checks)");
        }
    }

    // ------------------------------------------------------------------ 도우미

    private static String[] concat(String first, String[] rest) {
        String[] out = new String[rest.length + 1];
        out[0] = first;
        System.arraycopy(rest, 0, out, 1, rest.length);
        return out;
    }

    @SuppressWarnings("unchecked")
    private static List<Map<String, Object>> list0(Map<String, Object> m, String key) {
        return (List<Map<String, Object>>) m.get(key);
    }

    @SuppressWarnings("unchecked")
    private static List<String> strs(Map<String, Object> m, String key) {
        return (List<String>) m.get(key);
    }

    /** 연결을 「종류:키」 글로 줄인다. */
    private static List<String> types(Map<String, Object> task) {
        return list0(task, "links").stream().map(l -> l.get("type") + ":" + l.get("key")).toList();
    }
}
