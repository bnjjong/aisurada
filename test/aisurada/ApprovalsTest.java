package aisurada;

import aisurada.engine.EngineResult;
import aisurada.resource.Approvals;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Map;

import static aisurada.ChapterTestSupport.*;

/**
 * 스킬·에이전트 변경의 승인 워크플로(#53) 시험입니다. AI의 통로(세션 표식이 달린 요청, MCP 도구 이름으로 부른 요청)로 온 변경이
 * 정책에 따라 대기하는지, 대기 중에는 목록·조회·쓸 수 있는 자원에 이전 판이 그대로 보이는지, 승인·거절·기준이 바뀐 변경이 기록되는지,
 * 권한 올림·참조 켜기·도구 칸 변경은 정책과 무관하게 승인이 필요한지, 파일을 직접 고친 것이 「승인 안 된 변경」으로 걸러지는지를 잰다.
 */
public final class ApprovalsTest {

    private static final String SID_MARK = "\u0000ais-session-id";

    public static void main(String[] args) throws Exception {
        try (var t = new ChapterTestSupport("approvals")) {
            Path v = t.vault();
            ok(v, "create-project", "title=앱", "repos=[{\"name\":\"web\",\"path\":\"/Users/me/web\"}]");
            ok(v, "create-project", "title=문서");

            // 1. 사람이 한 변경은 바로 적용되고, 기록장은 앱 폴더에 생긴다(vault 안이 아니다)
            var skill = ok(v, "save-resource", "kind=skill", "scope=global", "name=번역 검수", "description=번역을 검수한다", "body=원래 지침");
            String sid = (String) skill.get("id");
            check(Files.exists(Approvals.dir(v).resolve("ledger.json")), "기록장은 앱 폴더에 생긴다");
            check(!Approvals.dir(v).startsWith(v), "기록장은 vault 밖이다");
            var c0 = ok(v, "list-resource-changes");
            eq(count(c0, "pending"), 0L, "처음에는 대기가 없다");
            eq(count(c0, "unapproved"), 0L, "승인 안 된 직접 수정도 없다");
            eq(policy(v).get("project"), "auto", "기본 정책: 프로젝트 자원은 바로");
            eq(policy(v).get("global"), "approve", "기본 정책: 글로벌 자원은 승인");
            eq(policy(v).get("refs"), "approve", "기본 정책: 참조는 승인");
            eq(policy(v).get("permission"), "approve", "기본 정책: 권한 내림도 승인");

            // 2. AI(세션 표식)가 글로벌 자원을 고치면 대기에 오르고, 승인 전에는 아무 데도 반영되지 않는다
            ok(v, "set-resource-ref", "앱", sid);
            String tok = (String) ok(v, "get-resource", sid).get("precondition");
            byte[] fileBefore = Files.readAllBytes(v.resolve((String) skill.get("path")));
            var q1 = ok(v, "save-resource", "id=" + sid, "f1=" + tok, "body=AI 가 고친 지침", "description=AI 설명", SID_MARK, "ai-1");
            eq(q1.get("queued"), true, "AI 의 글로벌 변경은 대기에 오른다");
            String cid1 = (String) q1.get("changeId");
            check(cid1.matches("pc[0-9a-z]+-[0-9a-f]{8}"), "변경 id 는 pc 머리: " + cid1);
            check(String.valueOf(q1.get("message")).contains("승인"), "AI 가 읽을 안내: " + q1.get("message"));
            eq(strs(list0(q1, "reasons"), "category"), List.of("global-change"), "승인을 요구한 까닭");
            eq(ok(v, "get-resource", sid).get("body"), "원래 지침", "조회는 이전 판이다");
            eq(list0(ok(v, "list-resources", "scope=global", "bodies=true"), "resources").get(0).get("body"), "원래 지침", "목록도 이전 판이다");
            eq(list0(ok(v, "usable-resources", "앱", "bodies=true"), "skills").get(0).get("body"), "원래 지침", "쓸 수 있는 자원도 이전 판이다");
            check(java.util.Arrays.equals(fileBefore, Files.readAllBytes(v.resolve((String) skill.get("path")))), "자원 파일은 그대로다");
            check(Files.isRegularFile(v.resolve(".aisurada/pending/" + cid1 + ".json")), "대기 변경은 vault 의 .aisurada/pending/ 에 있다");
            var c1 = ok(v, "list-resource-changes");
            eq(count(c1, "pending"), 1L, "대기 1건");
            var row1 = list0(c1, "pending").get(0);
            eq(row1.get("id"), cid1, "목록의 변경 id");
            eq(((Map<?, ?>) row1.get("proposer")).get("session"), "ai-1", "쓴 세션이 남는다");
            check(String.valueOf(row1.get("summary")).contains("번역 검수") && String.valueOf(row1.get("summary")).contains("고치기"), "요약: " + row1.get("summary"));
            check(!row1.containsKey("before") && !row1.containsKey("after"), "목록에는 비교용 칸이 없다");

            // 3. 같은 변경을 되풀이해도 쌓이지 않는다
            var again = ok(v, "save-resource", "id=" + sid, "f1=" + tok, "body=AI 가 고친 지침", "description=AI 설명", SID_MARK, "ai-1");
            eq(again.get("changeId"), cid1, "같은 변경은 같은 id");
            eq(again.get("duplicate"), true, "되풀이는 표시된다");
            eq(count(ok(v, "list-resource-changes"), "pending"), 1L, "대기는 그대로 1건");

            // 4. MCP 도구 이름(밑줄)으로 부르면 세션 표식이 없어도 AI 의 통로다
            var q2 = ok(v, "save_resource", "{\"id\":\"" + sid + "\",\"f1\":\"" + tok + "\",\"description\":\"MCP 가 고친 설명\"}");
            eq(q2.get("queued"), true, "도구 이름으로 부른 변경도 대기한다");
            var d2 = ok(v, "get-resource-change", (String) q2.get("changeId"));
            eq(((Map<?, ?>) d2.get("proposer")).get("channel"), "mcp", "통로가 기록된다");
            eq(count(ok(v, "list-resource-changes"), "pending"), 2L, "대기 2건");
            // 승인 전의 대기 변경은 AI 도 읽을 수 있다(도구 이름)
            check(ok(v, "list_resource_changes", "{}").containsKey("pending"), "list_resource_changes 도구");
            eq(ok(v, "get_resource_change", "{\"id\":\"" + cid1 + "\"}").get("status"), "pending", "get_resource_change 도구");

            // 5. 승인·거절·정책 변경·직접 수정 처리는 사람만 한다
            for (String[] cmd : new String[][] {
                    {"approve-resource-change", cid1}, {"reject-resource-change", cid1}, {"set-resource-policy", "global=auto"},
                    {"resolve-resource-edit", sid, "action=approve"}}) {
                String[] withMark = concat(cmd, SID_MARK, "ai-1");
                var r = error(v, 403, withMark);
                check(stderr(r).contains("사람만"), cmd[0] + " 은 AI 가 못 한다: " + stderr(r));
            }
            eq(policy(v).get("global"), "approve", "AI 가 정책을 못 바꿨다");
            eq(count(ok(v, "list-resource-changes"), "pending"), 2L, "AI 가 승인하지 못했다");

            // 6. 사람이 승인하면 적용되고, 쓴 이는 올린 AI 로 남고, 결정이 기록된다
            var approved = ok(v, "approve-resource-change", cid1);
            eq(approved.get("approved"), true, "승인");
            eq(ok(v, "get-resource", sid).get("body"), "AI 가 고친 지침", "승인하면 조회에 반영된다");
            eq(ok(v, "get-resource", sid).get("editedSession"), "ai-1", "쓴 세션은 올린 AI 의 것이다");
            eq(list0(ok(v, "usable-resources", "앱", "bodies=true"), "skills").get(0).get("body"), "AI 가 고친 지침", "쓸 수 있는 자원에도 반영");
            var done = ok(v, "get-resource-change", cid1);
            eq(done.get("status"), "approved", "상태가 승인");
            eq(done.get("decidedBy"), "human", "승인한 쪽");
            check(done.get("decidedAt") != null, "승인 시각");
            check(((List<?>) done.get("changedFields")).containsAll(List.of("body", "description")), "비교 칸: " + done.get("changedFields"));
            eq(((Map<?, ?>) done.get("before")).get("body"), "원래 지침", "이전 판");
            eq(((Map<?, ?>) done.get("after")).get("body"), "AI 가 고친 지침", "새 판");
            error(v, 409, "approve-resource-change", cid1);
            error(v, 404, "approve-resource-change", "pcmwzzzzzz-00000000");
            error(v, 400, "approve-resource-change", "엉뚱한 글");

            // 7. 기준이 바뀐 변경: 승인 전에 대상이 바뀌면 적용하지 않고 stale 로 남긴다
            String staleId = (String) q2.get("changeId");
            var stale = run(v, "approve-resource-change", staleId);
            eq(stale.errorCode(), 412, "승인 전에 파일이 바뀌었으면 AIS-412");
            eq(ok(v, "get-resource-change", staleId).get("status"), "stale", "stale 로 남는다");
            check(String.valueOf(ok(v, "get-resource-change", staleId).get("reason")).contains("읽은 뒤에"), "까닭이 남는다");
            check(!"MCP 가 고친 설명".equals(ok(v, "get-resource", sid).get("description")), "stale 변경은 적용되지 않았다");
            eq(count(ok(v, "list-resource-changes"), "pending"), 0L, "대기가 비었다");

            // 8. 거절: 이유가 남고 적용되지 않으며, 다시 승인할 수 없다
            tok = (String) ok(v, "get-resource", sid).get("precondition");
            var q3 = ok(v, "save-resource", "id=" + sid, "f1=" + tok, "body=거절될 지침", SID_MARK, "ai-2");
            var rej = ok(v, "reject-resource-change", (String) q3.get("changeId"), "reason=지침을 이렇게 바꾸면 안 된다");
            eq(rej.get("rejected"), true, "거절");
            var rejected = ok(v, "get-resource-change", (String) q3.get("changeId"));
            eq(rejected.get("status"), "rejected", "상태가 거절");
            eq(rejected.get("reason"), "지침을 이렇게 바꾸면 안 된다", "거절 이유");
            eq(ok(v, "get-resource", sid).get("body"), "AI 가 고친 지침", "거절한 변경은 적용되지 않는다");
            error(v, 409, "approve-resource-change", (String) q3.get("changeId"));
            error(v, 409, "reject-resource-change", (String) q3.get("changeId"));
            var decided = list0(ok(v, "list-resource-changes"), "decided");
            eq(decided.size(), 3, "결정이 난 것 3건(승인, stale, 거절)");
            eq(list0(ok(v, "list-resource-changes", "decided=1"), "decided").size(), 1, "decided 로 줄인다");
            eq(list0(ok(v, "list-resource-changes", "decided=0"), "decided").size(), 0, "decided=0 은 안 싣는다");
            error(v, 400, "list-resource-changes", "decided=많이");

            // 9. 만들기·지우기·되살리기도 대기하고, 승인하면 대기에 올릴 때의 id 로 만들어진다
            var q4 = ok(v, "save-resource", "kind=skill", "scope=global", "name=새 스킬", "description=AI 가 만든다", SID_MARK, "ai-1");
            eq(q4.get("queued"), true, "AI 의 글로벌 만들기는 대기한다");
            eq(list0(ok(v, "list-resources", "scope=global"), "resources").stream().filter(r -> "새 스킬".equals(r.get("name"))).count(), 0L, "승인 전에는 목록에 없다");
            String plannedId = (String) ((Map<?, ?>) ok(v, "get-resource-change", (String) q4.get("changeId")).get("after")).get("id");
            ok(v, "approve-resource-change", (String) q4.get("changeId"));
            eq(ok(v, "get-resource", plannedId).get("name"), "새 스킬", "대기 때 보인 id 로 만들어진다");
            var q5 = ok(v, "delete-resource", plannedId, SID_MARK, "ai-1");
            eq(q5.get("queued"), true, "AI 의 지우기는 대기한다");
            eq(ok(v, "get-resource", plannedId).get("name"), "새 스킬", "승인 전에는 그대로 있다");
            ok(v, "approve-resource-change", (String) q5.get("changeId"));
            error(v, 404, "get-resource", plannedId);
            var q6 = ok(v, "restore-resource", plannedId, SID_MARK, "ai-1");
            eq(q6.get("queued"), true, "AI 의 되살리기는 대기한다");
            error(v, 404, "get-resource", plannedId);
            ok(v, "approve-resource-change", (String) q6.get("changeId"));
            eq(ok(v, "get-resource", plannedId).get("name"), "새 스킬", "승인하면 돌아온다");

            // 10. 정책을 모두 auto 로 풀어도 권한 올림, 도구 칸 변경, 참조 켜기는 늘 승인이 필요하다(자물쇠를 AI 가 못 푼다)
            ok(v, "set-resource-policy", "project=auto", "global=auto", "refs=auto", "permission=auto");
            var open = policy(v);
            check(open.values().stream().allMatch("auto"::equals), "정책을 모두 풀었다: " + open);
            var auto1 = ok(v, "save-resource", "id=" + sid, "f1=" + ok(v, "get-resource", sid).get("precondition"), "description=정책이 열려 있어 바로", SID_MARK, "ai-1");
            check(!auto1.containsKey("queued") && "정책이 열려 있어 바로".equals(auto1.get("description")), "글로벌 정책이 auto 면 AI 도 바로 적용한다");
            var agent = ok(v, "save-resource", "kind=agent", "project=앱", "name=웹 담당", "description=웹을 맡는다");
            String aid = (String) agent.get("id");
            var raise = ok(v, "save-resource", "id=" + aid, "f1=" + agent.get("precondition"), "permission=publish", SID_MARK, "ai-1");
            eq(raise.get("queued"), true, "권한 올림은 정책과 무관하게 대기한다");
            eq(strs(list0(raise, "reasons"), "category"), List.of("permission-raise"), "까닭: 권한 올림");
            eq(list0(raise, "reasons").get(0).get("fixed"), true, "고정 종류");
            eq(ok(v, "get-resource", aid).get("permission"), "read", "승인 전에는 옛 권한이다(러너는 read 로 돌린다)");
            var tools = ok(v, "save-resource", "id=" + aid, "f1=" + agent.get("precondition"), "tools={\"claude\":{\"allowed-tools\":[\"Bash\"]}}", SID_MARK, "ai-1");
            eq(tools.get("queued"), true, "도구 칸 변경은 대기한다");
            eq(strs(list0(tools, "reasons"), "category"), List.of("tools-change"), "까닭: 도구 칸");
            var refAdd = ok(v, "set-resource-ref", "문서", sid, SID_MARK, "ai-1");
            eq(refAdd.get("queued"), true, "참조 켜기는 정책과 무관하게 대기한다");
            eq(strs(list0(refAdd, "reasons"), "category"), List.of("ref-add"), "까닭: 참조 켜기");
            eq(list0(ok(v, "usable-resources", "문서"), "skills").size(), 0, "승인 전에는 쓸 수 없다");
            var newAgent = ok(v, "save-resource", "kind=agent", "project=앱", "name=권한 높은 새 에이전트", "description=d", "permission=edit", SID_MARK, "ai-1");
            eq(newAgent.get("queued"), true, "read 보다 높은 권한으로 만들면 권한 올림이다");
            var plainAgent = ok(v, "save-resource", "kind=agent", "project=앱", "name=평범한 새 에이전트", "description=d", SID_MARK, "ai-1");
            check(!plainAgent.containsKey("queued"), "read 로 만드는 프로젝트 에이전트는 바로(정책)");
            ok(v, "approve-resource-change", (String) refAdd.get("changeId"));
            eq(list0(ok(v, "usable-resources", "문서"), "skills").size(), 1, "승인하면 참조가 켜진다");
            // 참조 끄기와 권한 내림은 정책을 따른다
            var refOff = ok(v, "set-resource-ref", "문서", sid, "remove=true", SID_MARK, "ai-1");
            check(!refOff.containsKey("queued") && strs(refOff, "removed").equals(List.of(sid)), "refs=auto 면 끄기는 바로: " + refOff);
            ok(v, "set-resource-ref", "문서", sid);
            ok(v, "approve-resource-change", (String) raise.get("changeId"));
            eq(ok(v, "get-resource", aid).get("permission"), "publish", "승인하면 권한이 올라간다");
            var lower = ok(v, "save-resource", "id=" + aid, "f1=" + ok(v, "get-resource", aid).get("precondition"), "permission=read", SID_MARK, "ai-1");
            check(!lower.containsKey("queued") && "read".equals(lower.get("permission")), "permission=auto 면 권한 내림은 바로");
            ok(v, "set-resource-policy", "refs=approve", "permission=approve", "project=approve");
            var lowerQ = ok(v, "save-resource", "id=" + aid, "f1=" + ok(v, "get-resource", aid).get("precondition"), "description=정책이 승인이라 대기", SID_MARK, "ai-1");
            eq(lowerQ.get("queued"), true, "project=approve 면 프로젝트 자원 변경도 대기한다");
            eq(strs(list0(lowerQ, "reasons"), "category"), List.of("project-content"), "까닭: 프로젝트 자원");
            ok(v, "reject-resource-change", (String) lowerQ.get("changeId"));
            ok(v, "set-resource-policy", "project=auto");
            error(v, 400, "set-resource-policy", "global=가끔");
            error(v, 400, "set-resource-policy");
            for (String old : List.of((String) tools.get("changeId"), (String) newAgent.get("changeId"))) ok(v, "reject-resource-change", old);

            // 11. 사람이 낸 변경은 정책과 상관없이 바로 적용된다(권한 올림도)
            var human = ok(v, "save-resource", "id=" + aid, "f1=" + ok(v, "get-resource", aid).get("precondition"), "permission=edit");
            check(!human.containsKey("queued") && "edit".equals(human.get("permission")), "사람은 바로 올린다");

            // 12. 파일을 직접 고치면 「승인 안 된 변경」이 되고, 승인해 둔 판이 계속 쓰인다
            var guard = ok(v, "save-resource", "kind=agent", "project=앱", "name=감시 대상", "description=원래 설명", "permission=read", "body=원래 지침");
            String gid = (String) guard.get("id");
            ok(v, "set-resource-ref", "앱", sid);
            Path gfile = v.resolve((String) guard.get("path"));
            Files.writeString(gfile, Files.readString(gfile).replace("permission: read", "permission: publish").replace("원래 지침", "몰래 바꾼 지침"));
            var served = ok(v, "get-resource", gid);
            eq(served.get("permission"), "read", "직접 고친 권한은 안 쓰인다");
            eq(served.get("body"), "원래 지침", "직접 고친 지침도 안 쓰인다");
            eq(served.get("approval"), "dirty", "직접 고쳐졌다고 표시된다");
            eq(list0(ok(v, "usable-resources", "앱"), "agents").stream().filter(r -> gid.equals(r.get("id"))).findFirst().get().get("permission"), "read", "쓸 수 있는 자원도 승인한 판");
            var un = ok(v, "list-resource-changes");
            eq(count(un, "unapproved"), 1L, "승인 안 된 변경 1건");
            var urow = list0(un, "unapproved").get(0);
            eq(urow.get("state"), "dirty", "상태: 직접 고쳐짐");
            eq(((Map<?, ?>) urow.get("approved")).get("permission"), "read", "비교: 승인한 판");
            eq(((Map<?, ?>) urow.get("live")).get("permission"), "publish", "비교: 지금 파일");
            eq(((Map<?, ?>) ok(v, "list-resources", "project=앱").get("approvals")).get("unapproved"), 1L, "목록의 배지용 수");
            // 사람이 고치면 승인한 판 위에 얹어 쓰고, 직접 고친 내용은 이어받지 않는다
            var humanOnDirty = ok(v, "save-resource", "id=" + gid, "f1=" + served.get("precondition"), "description=사람이 고친 설명");
            eq(humanOnDirty.get("permission"), "read", "직접 고친 권한은 이어받지 않는다");
            eq(humanOnDirty.get("body"), "원래 지침", "직접 고친 지침도 이어받지 않는다");
            eq(count(ok(v, "list-resource-changes"), "unapproved"), 0L, "사람이 저장하면 승인 안 된 변경이 풀린다");
            // 직접 고친 것을 받아들이거나 되돌린다
            Files.writeString(gfile, Files.readString(gfile).replace("permission: read", "permission: edit"));
            error(v, 409, "resolve-resource-edit", (String) ok(v, "get-resource", aid).get("id"), "action=approve");
            ok(v, "resolve-resource-edit", gid, "action=approve");
            eq(ok(v, "get-resource", gid).get("permission"), "edit", "받아들이면 그 내용이 쓰인다");
            eq(ok(v, "get-resource", gid).containsKey("approval"), false, "받아들이면 표시가 없다");
            Files.writeString(gfile, Files.readString(gfile).replace("permission: edit", "permission: publish"));
            ok(v, "resolve-resource-edit", gid, "action=revert");
            check(Files.readString(gfile).contains("permission: edit\n"), "되돌리면 승인한 판의 파일이 된다");
            check(ok(v, "list-resource-versions", gid).get("versions") != null && !list0(ok(v, "list-resource-versions", gid), "versions").isEmpty(), "고쳐진 내용은 이력에 남는다");
            error(v, 400, "resolve-resource-edit", gid, "action=잘 모르겠다");

            // 13. 승인 기록 없이 생긴 새 파일은 쓰이지 않는다. 승인하면 쓰이고, 되돌리면 휴지통으로 간다
            Path handFile = v.resolve(".aisurada/resources/projects/앱/skills/손으로/SKILL.md");
            Files.createDirectories(handFile.getParent());
            Files.writeString(handFile, "---\nid: skmwhand000-0000abcd\nname: 손으로\ndescription: 손으로 만든 스킬\n---\n본문\n");
            error(v, 404, "get-resource", "skmwhand000-0000abcd");
            eq(list0(ok(v, "list-resources", "project=앱"), "resources").stream().filter(r -> "손으로".equals(r.get("name"))).count(), 0L, "목록에 안 나온다");
            eq(list0(ok(v, "usable-resources", "앱"), "skills").stream().filter(r -> "손으로".equals(r.get("name"))).count(), 0L, "쓸 수 있는 자원에도 없다");
            var newRow = list0(ok(v, "list-resource-changes"), "unapproved").get(0);
            eq(newRow.get("state"), "new", "상태: 승인 기록 없는 새 파일");
            check(stderr(error(v, 409, "save-resource", "id=skmwhand000-0000abcd", "f1=f1:" + "0".repeat(64), "description=x")).contains("승인되지 않은"), "새 파일은 고칠 수 없다");
            ok(v, "resolve-resource-edit", "skmwhand000-0000abcd", "action=revert");
            check(!Files.exists(handFile), "되돌리면 제자리에서 사라진다");
            eq(list0(ok(v, "list-resources", "trash=true"), "trash").stream().filter(r -> "skmwhand000-0000abcd".equals(r.get("id"))).count(), 1L, "휴지통으로 갔다");
            Files.createDirectories(handFile.getParent());
            Files.writeString(handFile, "---\nid: skmwhand111-0000abcd\nname: 손으로2\ndescription: 또 손으로\n---\n본문\n");
            ok(v, "resolve-resource-edit", "skmwhand111-0000abcd", "action=approve");
            eq(ok(v, "get-resource", "skmwhand111-0000abcd").get("name"), "손으로2", "받아들이면 쓰인다");

            // 14. 참조 목록을 직접 고친 것도 걸러진다
            Path refsFile = v.resolve(".aisurada/resources/projects/문서/refs.md");
            byte[] refsBytes = Files.readAllBytes(refsFile);
            Files.writeString(refsFile, Files.readString(refsFile) + (String) skill.get("id") + "\n" + plannedId + "  # 몰래\n");
            eq(list0(ok(v, "usable-resources", "문서"), "skills").size(), 1, "직접 늘린 참조는 안 쓰인다(승인한 목록 그대로)");
            var refsRow = list0(ok(v, "list-resource-changes"), "unapproved").stream().filter(r -> "refs".equals(r.get("type"))).findFirst().get();
            eq(refsRow.get("key"), "refs:문서", "참조 목록의 키");
            eq(refsRow.get("state"), "dirty", "참조 목록이 직접 고쳐졌다");
            eq(((List<?>) refsRow.get("approved")).size(), 1, "승인한 참조 1개");
            ok(v, "resolve-resource-edit", "refs:문서", "action=revert");
            check(java.util.Arrays.equals(refsBytes, Files.readAllBytes(refsFile)), "되돌리면 승인한 참조 목록의 바이트로 돌아온다");
            error(v, 409, "resolve-resource-edit", "refs:문서", "action=approve");

            // 15. 기록장이 없던 vault 는 처음 읽을 때 있는 자원을 모두 승인한 것으로 받아들인다(기준선)
            Path v2 = t.vault();
            ok(v2, "create-project", "title=옛 프로젝트");
            Path legacy = v2.resolve(".aisurada/resources/global/skills/옛-스킬/SKILL.md");
            Files.createDirectories(legacy.getParent());
            Files.writeString(legacy, "---\nid: skmwlegacy0-0000abcd\nname: 옛 스킬\ndescription: 기능이 생기기 전의 자원\n---\n본문\n");
            check(!Files.exists(Approvals.dir(v2).resolve("ledger.json")), "기록장이 아직 없다");
            eq(list0(ok(v2, "list-resources"), "resources").size(), 1, "처음 읽으면 기준선으로 받아들여 목록에 나온다");
            eq(count(ok(v2, "list-resource-changes"), "unapproved"), 0L, "기준선이라 승인 안 된 변경이 아니다");
            Files.writeString(legacy, Files.readString(legacy).replace("본문", "나중에 직접 고침"));
            eq(ok(v2, "get-resource", "skmwlegacy0-0000abcd").get("body"), "본문", "기준선 뒤의 직접 수정은 걸러진다");

            // 16. 기록장이 깨지면 멈춘다(모든 파일이 다시 승인된 것이 되지 않게)
            Files.writeString(Approvals.dir(v2).resolve("ledger.json"), "깨진 기록장");
            var broken = run(v2, "list-resources");
            eq(broken.errorCode(), 500, "깨진 기록장은 AIS-500");
            check(stderr(broken).contains("승인 기록을 읽지 못했다"), "문구: " + stderr(broken));
            error(v2, 500, "save-resource", "kind=skill", "scope=global", "name=멈춤", "description=d");
            check(!Files.exists(v2.resolve(".aisurada/resources/global/skills/멈춤")), "멈춘 쓰기는 파일을 만들지 않았다");

            // 17. 대기 변경 파일이 있어도 기존 명령(노트 만들기, 목록, 점검)은 그대로 돈다
            ok(v, "set-resource-policy", "global=approve");
            var pendingNow = ok(v, "save-resource", "id=" + sid, "f1=" + ok(v, "get-resource", sid).get("precondition"), "body=공존 시험", SID_MARK, "ai-9");
            eq(pendingNow.get("queued"), true, "공존 시험용 대기 변경");
            ok(v, "create-doc", "agenda=demo", "type=free", "title=대기 변경이 있는 vault의 노트", "body=본문");
            check(success(v, "list-docs") != null, "list-docs");
            check(success(v, "check-notes") != null, "check-notes");
            check(success(v, "list-trash") != null, "list-trash");
            eq(count(ok(v, "list-resource-changes"), "pending"), 1L, "노트 명령을 써도 대기는 그대로");

            // 18. 승인 명령의 모양 점검
            check(stderr(error(v, 400, "approve-resource-change")).contains("인자가 하나 필요하다"), "인자 없는 승인");
            check(stderr(error(v, 400, "reject-resource-change", "pcmwzzzzzz-00000000", "foo=1")).contains("모르는 옵션"), "모르는 옵션");
            eq(ok(v, "get-resource-policy").get("defaults"), Map.of("project", "auto", "global", "approve", "refs", "approve", "permission", "approve"), "기본 정책을 같이 알려 준다");
            check(list0(ok(v, "get-resource-policy"), "categories").stream().filter(c -> Boolean.TRUE.equals(c.get("fixed"))).count() == 3, "고정 종류 3개");
            System.out.println("ApprovalsTest OK (" + checks + " checks)");
        }
    }

    // ------------------------------------------------------------------ 도우미

    private static Map<String, Object> policy(Path v) {
        @SuppressWarnings("unchecked")
        Map<String, Object> p = (Map<String, Object>) ok(v, "get-resource-policy").get("policy");
        return p;
    }

    private static Object count(Map<String, Object> m, String key) {
        return ((Map<?, ?>) m.get("counts")).get(key);
    }

    private static String[] concat(String[] first, String... rest) {
        String[] out = new String[first.length + rest.length];
        System.arraycopy(first, 0, out, 0, first.length);
        System.arraycopy(rest, 0, out, first.length, rest.length);
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

    private static List<String> strs(List<Map<String, Object>> rows, String key) {
        return rows.stream().map(r -> String.valueOf(r.get(key))).toList();
    }
}
